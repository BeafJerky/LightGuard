//! 全部 `docker_*` 命令。命令层只做参数校验、取会话与调用 read/write/system。

use std::sync::Arc;

use tauri::State;

use crate::alerts::now_ms;
use crate::db::{DbError, DbState};
use crate::docker::dto::{
    DockerCapabilities, DockerContainer, DockerContainerDetail, DockerImage, DockerOverview,
    DockerPreview, DockerProgress, DockerStatsSample, DockerSystemInput, DockerTerminalHandle,
    DockerTerminalInput, DockerWriteInput, DockerWriteResult, RegistryConfig, RegistrySearchItem,
    RegistryTags, SudoStatus,
};
use crate::docker::exec::{self, ExecOptions};
use crate::docker::read;
use crate::docker::state::DockerState;
use crate::docker::system::{self, SystemPayload};
use crate::docker::validate;
use crate::docker::write::{self, WritePayload};
use crate::guard::preview;
use crate::guard::GuardService;
use crate::ssh::commands::{validate_server_id, validate_terminal_size};
use crate::ssh::dto::TerminalEvent;
use crate::ssh::error::SshError;
use crate::ssh::session::ClientHandler;
use crate::ssh::sftp::client;
use crate::ssh::sftp::path::shell_quote;
use crate::ssh::state::AppState;
use crate::ssh::terminal::{self, MAX_TERMINALS_PER_SERVER};

pub(crate) async fn handle_for(
    state: &AppState,
    server_id: &str,
) -> Result<Arc<russh::client::Handle<crate::ssh::session::ClientHandler>>, SshError> {
    validate_server_id(server_id)?;
    client::session_handle(state, server_id).await
}

#[tauri::command]
pub async fn docker_capabilities(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<DockerCapabilities, SshError> {
    let handle = handle_for(state.inner(), &server_id).await?;
    read::capabilities(&handle).await
}

#[tauri::command]
pub async fn docker_overview(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    include_df: Option<bool>,
) -> Result<DockerOverview, SshError> {
    let handle = handle_for(state.inner(), &server_id).await?;
    read::overview(&handle, include_df.unwrap_or(true)).await
}

#[tauri::command]
pub async fn docker_containers(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<Vec<DockerContainer>, SshError> {
    let handle = handle_for(state.inner(), &server_id).await?;
    read::containers(&handle).await
}

#[tauri::command]
pub async fn docker_container_detail(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    container_id: String,
) -> Result<DockerContainerDetail, SshError> {
    let container_id = validate::container_id(&container_id)?;
    let handle = handle_for(state.inner(), &server_id).await?;
    read::container_detail(&handle, &container_id).await
}

#[tauri::command]
pub async fn docker_stats(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<Vec<DockerStatsSample>, SshError> {
    let handle = handle_for(state.inner(), &server_id).await?;
    read::stats(&handle).await
}

#[tauri::command]
pub async fn docker_images(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<Vec<DockerImage>, SshError> {
    let handle = handle_for(state.inner(), &server_id).await?;
    read::images(&handle).await
}

#[tauri::command]
pub async fn docker_registry_search(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    term: String,
    limit: Option<u32>,
) -> Result<Vec<RegistrySearchItem>, SshError> {
    let term = validate::search_term(&term)?;
    let handle = handle_for(state.inner(), &server_id).await?;
    read::registry_search(&handle, &term, limit.unwrap_or(25)).await
}

#[tauri::command]
pub async fn docker_registry_tags(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    repo: String,
) -> Result<RegistryTags, SshError> {
    let handle = handle_for(state.inner(), &server_id).await?;
    read::registry_tags(&handle, &repo).await
}

fn database(state: &State<'_, DbState>) -> Result<Arc<crate::db::Database>, SshError> {
    let error = DbError::unavailable("本地数据库未就绪");
    state.handle().ok_or_else(|| SshError {
        code: error.code,
        message: error.message,
        hint: error.hint,
        stage: None,
        retryable: false,
    })
}

/// 预览是写操作的第一道闸：没有许可连远端状态都不读，读到的状态再冻结成载荷存进
/// 一次性预览里，执行时只认这份载荷。
#[tauri::command]
pub async fn docker_write_preview(
    state: State<'_, Arc<AppState>>,
    guard: State<'_, Arc<GuardService>>,
    server_id: String,
    input: DockerWriteInput,
) -> Result<DockerPreview, SshError> {
    validate_server_id(&server_id)?;
    guard.require_permit(&server_id)?;
    let handle = handle_for(state.inner(), &server_id).await?;
    let (payload, view) = write::preview(&handle, &server_id, &input).await?;
    let payload =
        serde_json::to_value(&payload).map_err(|error| SshError::internal(error.to_string()))?;
    let preview_id = guard.store_preview(&server_id, &write::audit_op(&input.kind), payload);
    Ok(write::build_preview(
        preview_id,
        &input.kind,
        view,
        now_ms() + write::preview_ttl_ms(),
    ))
}

/// 执行顺序：取预览（一次性）→ 载荷与预览同源校验 → 许可 → 写锁 → 执行 → 审计。
#[tauri::command]
pub async fn docker_write_execute(
    state: State<'_, Arc<AppState>>,
    guard: State<'_, Arc<GuardService>>,
    db_state: State<'_, DbState>,
    preview_id: String,
    on_event: tauri::ipc::Channel<DockerProgress>,
) -> Result<DockerWriteResult, SshError> {
    let entry = guard.take_preview(&preview_id)?;
    let payload: WritePayload = serde_json::from_value(entry.payload)
        .map_err(|error| SshError::preview_failed(error.to_string()))?;
    // 预览缓存不按服务器分区：服务器与操作必须同时与预览记录一致，
    // 否则一个预览就能被换到另一台服务器上执行。
    if entry.server_id != payload.server_id || entry.op != payload.audit_op {
        return Err(SshError::preview_failed("预览与执行目标不一致"));
    }
    if write::action_script(&payload).is_empty() {
        return Err(SshError::preview_failed("预览操作不受支持"));
    }
    let minutes = guard.require_permit(&payload.server_id)?;
    let permit_source = format!("unlock:{minutes}m");
    let db = database(&db_state)?;
    // 先取写锁再登记取消标志：拿不到锁时不会留下没人读取的标志，
    // 也不会覆盖正在运行的拉取的标志。
    let _write = guard.begin_write(&payload.server_id)?;
    let pulling = payload.kind == "image.pull";
    let cancel = if pulling {
        Some(state.docker.begin_pull(&payload.server_id))
    } else {
        None
    };
    let mut emit = |line: &str| {
        let _ = on_event.send(DockerProgress {
            line: line.to_string(),
        });
    };
    let result = write::execute(
        state.inner(),
        &db,
        &payload,
        &permit_source,
        None,
        &mut emit,
        cancel,
    )
    .await;
    if pulling {
        state.docker.finish_pull(&payload.server_id);
    }
    result
}

/// 只对进行中的拉取生效：没有拉取时返回 `false`，容器与镜像写操作不受影响。
#[tauri::command]
pub fn docker_write_cancel(state: State<'_, Arc<AppState>>, server_id: String) -> bool {
    state.docker.cancel_pull(&server_id)
}

/// 命令级密码暂存：任何分支返回都会清零；成功时把字节所有权交给 `DockerState`。
struct PendingSecret(Option<Vec<u8>>);

impl PendingSecret {
    fn as_deref(&self) -> Option<&[u8]> {
        self.0.as_deref()
    }

    fn take(&mut self) -> Option<Vec<u8>> {
        self.0.take()
    }
}

impl Drop for PendingSecret {
    fn drop(&mut self) {
        if let Some(bytes) = self.0.as_mut() {
            DockerState::wipe(bytes);
        }
    }
}

async fn auth_for(
    handle: &russh::client::Handle<ClientHandler>,
    state: &AppState,
    server_id: &str,
    password: Option<String>,
) -> Result<PendingSecret, SshError> {
    if let Some(password) = password {
        let mut bytes = password.into_bytes();
        if bytes.iter().all(u8::is_ascii_whitespace) {
            DockerState::wipe(&mut bytes);
            return Err(SshError::sudo_failed("密码不能为空"));
        }
        return Ok(PendingSecret(Some(bytes)));
    }
    if let Some(bytes) = state.docker.password_copy(server_id) {
        return Ok(PendingSecret(Some(bytes)));
    }
    match state.docker.passwordless(server_id) {
        Some(true) => return Ok(PendingSecret(None)),
        Some(false) => return Err(SshError::sudo_required()),
        None => {}
    }
    let status = system::probe(handle).await?;
    state
        .docker
        .set_passwordless(server_id, status.passwordless);
    if status.passwordless {
        Ok(PendingSecret(None))
    } else if status.available {
        Err(SshError::sudo_required())
    } else {
        Err(SshError::sudo_failed("当前账号没有 sudo 权限"))
    }
}

#[tauri::command]
pub async fn docker_sudo_status(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<SudoStatus, SshError> {
    validate_server_id(&server_id)?;
    let handle = handle_for(state.inner(), &server_id).await?;
    let status = system::probe(&handle).await?;
    state
        .docker
        .set_passwordless(&server_id, status.passwordless);
    Ok(status)
}

#[tauri::command]
pub async fn docker_registry_config(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<RegistryConfig, SshError> {
    validate_server_id(&server_id)?;
    let handle = handle_for(state.inner(), &server_id).await?;
    let secret = PendingSecret(state.docker.password_copy(&server_id));
    system::registry_config(&handle, secret.as_deref()).await
}

#[tauri::command]
pub async fn docker_system_preview(
    state: State<'_, Arc<AppState>>,
    guard: State<'_, Arc<GuardService>>,
    server_id: String,
    input: DockerSystemInput,
    password: Option<String>,
) -> Result<DockerPreview, SshError> {
    validate_server_id(&server_id)?;
    let handle = handle_for(state.inner(), &server_id).await?;
    let mut secret = auth_for(&handle, state.inner(), &server_id, password).await?;
    if let Some(bytes) = secret.as_deref() {
        if let Err(error) = system::verify_sudo(&handle, Some(bytes)).await {
            state.docker.clear_password(&server_id);
            return Err(error);
        }
    }
    let (payload, view) = system::preview(&handle, &server_id, &input, secret.as_deref()).await?;
    let value =
        serde_json::to_value(&payload).map_err(|error| SshError::internal(error.to_string()))?;
    let preview_id = guard.store_preview_with_ttl(
        &server_id,
        &system::audit_op(&input.kind),
        value,
        preview::SYSTEM_PREVIEW_TTL,
    );
    if let Some(bytes) = secret.take() {
        state.docker.cache_password(&server_id, bytes);
    }
    Ok(write::build_preview(
        preview_id,
        &input.kind,
        view,
        now_ms() + preview::SYSTEM_PREVIEW_TTL.as_millis() as i64,
    ))
}

#[tauri::command]
pub async fn docker_system_execute(
    state: State<'_, Arc<AppState>>,
    guard: State<'_, Arc<GuardService>>,
    db_state: State<'_, DbState>,
    preview_id: String,
    password: Option<String>,
    on_event: tauri::ipc::Channel<DockerProgress>,
) -> Result<DockerWriteResult, SshError> {
    let entry = guard.peek_preview(&preview_id)?;
    let payload: SystemPayload = serde_json::from_value(entry.payload.clone())
        .map_err(|error| SshError::preview_failed(error.to_string()))?;
    if entry.server_id != payload.server_id || entry.op != payload.audit_op {
        return Err(SshError::preview_failed("预览与执行目标不一致"));
    }
    let handle = handle_for(state.inner(), &payload.server_id).await?;
    let mut secret = auth_for(&handle, state.inner(), &payload.server_id, password).await?;
    if let Some(bytes) = secret.as_deref() {
        if let Err(error) = system::verify_sudo(&handle, Some(bytes)).await {
            state.docker.clear_password(&payload.server_id);
            return Err(error);
        }
    }
    let passwordless = secret.as_deref().is_none();
    let entry = guard.take_preview(&preview_id)?;
    if entry.server_id != payload.server_id || entry.op != payload.audit_op {
        return Err(SshError::preview_failed("预览与执行目标不一致"));
    }
    let db = database(&db_state)?;
    let _write = guard.begin_write(&payload.server_id)?;
    let mut emit = |line: &str| {
        let _ = on_event.send(DockerProgress {
            line: line.to_string(),
        });
    };
    let result = system::execute(
        state.inner(),
        &db,
        &payload,
        secret.as_deref(),
        "system",
        &mut emit,
    )
    .await;
    match result {
        Ok(result) => {
            if let Some(bytes) = secret.take() {
                state.docker.cache_password(&payload.server_id, bytes);
            }
            state
                .docker
                .set_passwordless(&payload.server_id, passwordless);
            Ok(result)
        }
        Err(error) => {
            if matches!(error.code, "SUDO_FAILED" | "SUDO_REQUIRED") {
                state.docker.clear_password(&payload.server_id);
            }
            Err(error)
        }
    }
}

pub fn container_title(container_name: &str, server_name: &str) -> String {
    if container_name.is_empty() {
        format!("容器终端 · {server_name}")
    } else {
        format!("{container_name} · {server_name}")
    }
}

async fn probe_shell(
    handle: &russh::client::Handle<crate::ssh::session::ClientHandler>,
    container_id: &str,
) -> Result<&'static str, SshError> {
    for shell in ["/bin/sh", "/bin/bash"] {
        let script = format!(
            "docker exec {} {} -c true",
            shell_quote(container_id),
            shell_quote(shell)
        );
        let output = exec::run(
            handle,
            &script,
            ExecOptions {
                timeout: std::time::Duration::from_secs(15),
                max_bytes: 64 * 1024,
                stdin: None,
                cancel: None,
            },
            |_| {},
        )
        .await?;
        if output.code == Some(0) {
            return Ok(shell);
        }
    }
    Err(SshError::terminal("镜像不包含可用的 Shell（/bin/sh 或 /bin/bash）"))
}

/// 容器终端不要求许可：它是只读交互，命令固定为后端构造的 `docker exec -it`。
#[tauri::command]
pub async fn docker_container_terminal_open(
    state: State<'_, Arc<AppState>>,
    db_state: State<'_, DbState>,
    input: DockerTerminalInput,
    on_event: tauri::ipc::Channel<TerminalEvent>,
) -> Result<DockerTerminalHandle, SshError> {
    validate_server_id(&input.server_id)?;
    validate_terminal_size(input.cols, input.rows)?;
    let container_id = validate::container_id(&input.container_id)?;

    let handle = handle_for(state.inner(), &input.server_id).await?;
    {
        let terminals = state.terminals.lock().await;
        let count = terminals
            .values()
            .filter(|entry| entry.server_id == input.server_id)
            .count();
        if count >= MAX_TERMINALS_PER_SERVER {
            return Err(SshError::terminal(format!(
                "单个服务器最多 {MAX_TERMINALS_PER_SERVER} 个终端标签"
            )));
        }
    }

    let inspect = exec::run(
        &handle,
        &format!(
            "docker inspect --format '{{{{.Name}}}}' {}",
            shell_quote(&container_id)
        ),
        ExecOptions {
            timeout: std::time::Duration::from_secs(15),
            max_bytes: 64 * 1024,
            stdin: None,
            cancel: None,
        },
        |_| {},
    )
    .await?;
    if inspect.code != Some(0) {
        return Err(SshError::docker_failed("容器不存在或已删除"));
    }
    let name = inspect
        .stdout_text()
        .trim()
        .trim_start_matches('/')
        .to_string();
    let name = if name.is_empty() {
        container_id.chars().take(12).collect::<String>()
    } else {
        name
    };
    let server_name = database(&db_state)
        .ok()
        .and_then(|db| db.server_name(&input.server_id).ok())
        .unwrap_or_else(|| input.server_id.clone());
    let title = container_title(&name, &server_name);

    let shell = probe_shell(&handle, &container_id).await?;
    let command = terminal::exec_command(&container_id, shell);
    let (term, reader) = terminal::open_exec(
        &handle,
        &input.server_id,
        &title,
        input.cols,
        input.rows,
        &command,
    )
    .await?;
    let handle_out = DockerTerminalHandle {
        terminal_id: reader.id.clone(),
        title: term.title.clone(),
    };

    {
        let mut terminals = state.terminals.lock().await;
        let count = terminals
            .values()
            .filter(|entry| entry.server_id == input.server_id)
            .count();
        if count >= MAX_TERMINALS_PER_SERVER {
            drop(terminals);
            terminal::close(term).await;
            return Err(SshError::terminal(format!(
                "单个服务器最多 {MAX_TERMINALS_PER_SERVER} 个终端标签"
            )));
        }
        terminals.insert(reader.id.clone(), term);
    }
    terminal::spawn_reader(reader, on_event, state.terminals.clone());
    Ok(handle_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn container_title_combines_name_and_server() {
        assert_eq!(container_title("shop-web-1", "nas"), "shop-web-1 · nas");
        assert_eq!(container_title("", "nas"), "容器终端 · nas");
    }
}

//! 全部 `docker_*` 命令。命令层只做参数校验、取会话与调用 read/write/system。

use std::sync::Arc;
use std::time::{Duration, Instant};

use tauri::State;
use tauri_plugin_dialog::DialogExt;

use crate::alerts::now_ms;
use crate::db::{DbError, DbState};
use crate::docker::dto::{
    DockerCapabilities, DockerContainer, DockerContainerDetail, DockerImage, DockerOverview,
    DockerPreview, DockerProgress, DockerStatsSample, DockerSystemInput, DockerTerminalHandle,
    DockerTerminalInput, DockerWriteInput, DockerWriteResult, ImageUpdateStatus, RegistryConfig,
    RegistrySearchItem, RegistryTags, SudoStatus,
};
use crate::docker::exec::{self, ExecOptions};
use crate::docker::{export, image_ops, read};
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
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    read::capabilities(&handle, elevated).await
}

#[tauri::command]
pub async fn docker_overview(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    include_df: Option<bool>,
) -> Result<DockerOverview, SshError> {
    let handle = handle_for(state.inner(), &server_id).await?;
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    read::overview(&handle, include_df.unwrap_or(true), elevated).await
}

#[tauri::command]
pub async fn docker_containers(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<Vec<DockerContainer>, SshError> {
    let handle = handle_for(state.inner(), &server_id).await?;
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    read::containers(&handle, elevated).await
}

#[tauri::command]
pub async fn docker_container_detail(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    container_id: String,
) -> Result<DockerContainerDetail, SshError> {
    let container_id = validate::container_id(&container_id)?;
    let handle = handle_for(state.inner(), &server_id).await?;
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    read::container_detail(&handle, &container_id, elevated).await
}

#[tauri::command]
pub async fn docker_stats(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<Vec<DockerStatsSample>, SshError> {
    let handle = handle_for(state.inner(), &server_id).await?;
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    read::stats(&handle, elevated).await
}

#[tauri::command]
pub async fn docker_images(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<Vec<DockerImage>, SshError> {
    let handle = handle_for(state.inner(), &server_id).await?;
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    read::images(&handle, elevated).await
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
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    read::registry_search(&handle, &term, limit.unwrap_or(25), elevated).await
}

#[tauri::command]
pub async fn docker_registry_tags(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    repo: String,
) -> Result<RegistryTags, SshError> {
    let handle = handle_for(state.inner(), &server_id).await?;
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    read::registry_tags(&handle, &repo, elevated).await
}


#[tauri::command]
pub async fn docker_compose_projects(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<Vec<crate::docker::dto::ComposeProject>, SshError> {
    let handle = handle_for(state.inner(), &server_id).await?;
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    read::compose_projects(&handle, elevated).await
}

#[tauri::command]
pub async fn docker_compose_logs(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    project: String,
    tail: Option<u32>,
) -> Result<String, SshError> {
    let handle = handle_for(state.inner(), &server_id).await?;
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    read::compose_logs(&handle, &project, tail.unwrap_or(200), elevated).await
}

#[tauri::command]
pub async fn docker_networks(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<Vec<crate::docker::dto::DockerNetworkSummary>, SshError> {
    let handle = handle_for(state.inner(), &server_id).await?;
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    read::networks(&handle, elevated).await
}

#[tauri::command]
pub async fn docker_volumes(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<Vec<crate::docker::dto::DockerVolumeSummary>, SshError> {
    let handle = handle_for(state.inner(), &server_id).await?;
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    read::volumes(&handle, elevated).await
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
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    let (payload, view) = write::preview(&handle, &server_id, &input, elevated).await?;
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
    if !matches!(payload.kind.as_str(), "image.import" | "image.build")
        && write::action_script(&payload).is_empty()
    {
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

#[tauri::command]
pub async fn docker_container_logs(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    container_id: String,
    tail: Option<u32>,
    on_event: tauri::ipc::Channel<String>,
) -> Result<(), SshError> {
    validate_server_id(&server_id)?;
    let container_id = validate::container_id(&container_id)?;
    let handle = handle_for(state.inner(), &server_id).await?;
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    let cancel = state.docker.begin_logs(&server_id);
    let script = format!(
        "docker logs --tail {} -f {}",
        tail.unwrap_or(300).clamp(1, 2000),
        shell_quote(&container_id)
    );
    let result = exec::run(
        &handle,
        &script,
        ExecOptions {
            timeout: std::time::Duration::from_secs(3600),
            max_bytes: 4 * 1024 * 1024,
            stdin: None,
            cancel: Some(cancel),
            elevated,
        },
        |line| {
            let _ = on_event.send(line.to_string());
        },
    )
    .await;
    state.docker.finish_logs(&server_id);
    result.map(|_| ())
}

#[tauri::command]
pub fn docker_container_logs_cancel(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> bool {
    state.docker.cancel_logs(&server_id)
}

#[tauri::command]
pub async fn docker_container_files(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    container_id: String,
    path: String,
) -> Result<Vec<crate::docker::dto::ContainerFileEntry>, SshError> {
    validate_server_id(&server_id)?;
    let container_id = validate::container_id(&container_id)?;
    crate::docker::files::validate_path(&path)?;
    let handle = handle_for(state.inner(), &server_id).await?;
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    let shell = probe_shell(&handle, &container_id, elevated).await?;
    crate::docker::files::list(&handle, &container_id, shell, &path, elevated).await
}

#[tauri::command]
pub async fn docker_container_file(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    container_id: String,
    path: String,
) -> Result<crate::docker::dto::ContainerFilePreview, SshError> {
    validate_server_id(&server_id)?;
    let container_id = validate::container_id(&container_id)?;
    crate::docker::files::validate_path(&path)?;
    let handle = handle_for(state.inner(), &server_id).await?;
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    let shell = probe_shell(&handle, &container_id, elevated).await?;
    crate::docker::files::read(&handle, &container_id, shell, &path, elevated).await
}

fn container_title(container_name: &str, server_name: &str) -> String {
    if container_name.is_empty() {
        format!("容器终端 · {server_name}")
    } else {
        format!("{container_name} · {server_name}")
    }
}


#[tauri::command]
pub async fn docker_container_export(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    server_id: String,
    container_id: String,
    transfer_id: String,
    on_event: tauri::ipc::Channel<crate::ssh::sftp::dto::TransferProgress>,
) -> Result<String, SshError> {
    validate_server_id(&server_id)?;
    let container_id = validate::container_id(&container_id)?;
    crate::docker::export::export(
        &app,
        state.inner(),
        &server_id,
        &container_id,
        &transfer_id,
        &on_event,
    )
    .await
}

pub(crate) async fn probe_shell(
    handle: &russh::client::Handle<crate::ssh::session::ClientHandler>,
    container_id: &str,
    elevated: bool,
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
                elevated,
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
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &input.server_id).await;
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
            elevated,
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

    let shell = probe_shell(&handle, &container_id, elevated).await?;
    let command = terminal::exec_command(&container_id, shell);
    let command = if elevated {
        format!("sudo -n {command}")
    } else {
        command
    };
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

#[tauri::command]
pub async fn docker_image_export(
    app: tauri::AppHandle,
    state: State<'_, Arc<AppState>>,
    server_id: String,
    image_id: String,
    transfer_id: String,
    on_event: tauri::ipc::Channel<crate::ssh::sftp::dto::TransferProgress>,
) -> Result<String, SshError> {
    validate_server_id(&server_id)?;
    let handle = handle_for(state.inner(), &server_id).await?;
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    let rows = read::images(&handle, elevated).await?;
    let image = write::resolve_image(&image_id, &rows)
        .ok_or_else(|| SshError::docker_failed("镜像不存在或已变化"))?;
    let display = image_display(image);
    let _ = transfer_id;
    export::export_image(
        &app,
        state.inner(),
        &server_id,
        &image.id,
        &display,
        &on_event,
    )
    .await
}

#[tauri::command]
pub async fn docker_image_update(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    image_id: String,
) -> Result<ImageUpdateStatus, SshError> {
    validate_server_id(&server_id)?;
    let handle = handle_for(state.inner(), &server_id).await?;
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    let rows = read::images(&handle, elevated).await?;
    let image = write::resolve_image(&image_id, &rows)
        .ok_or_else(|| SshError::docker_failed("镜像不存在或已变化"))?;
    let reference = image
        .tags
        .first()
        .filter(|_| !image.repo.is_empty())
        .map(|tag| format!("{}:{}", image.repo, tag))
        .ok_or_else(|| SshError::docker_failed("悬空镜像没有仓库标签，无法检测更新"))?;
    let proxy = cli_proxy(state.inner(), &handle, &server_id).await;
    image_ops::check_update(&handle, &reference, elevated, proxy.as_deref()).await
}

fn image_display(image: &DockerImage) -> String {
    image
        .tags
        .first()
        .filter(|_| !image.repo.is_empty())
        .map(|tag| format!("{}:{}", image.repo, tag))
        .unwrap_or_else(|| image.short_id.clone())
}

#[tauri::command]
pub async fn docker_pick_archive(app: tauri::AppHandle) -> Option<String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let app_for_thread = app.clone();
    let dispatched = app.run_on_main_thread(move || {
        app_for_thread
            .dialog()
            .file()
            .set_title("选择镜像归档")
            .add_filter("镜像归档", &["tar", "gz", "tgz"])
            .pick_file(move |path| {
                let _ = sender.send(path);
            });
    });
    if dispatched.is_err() {
        return None;
    }
    match tokio::time::timeout(Duration::from_secs(300), receiver).await {
        Ok(Ok(Some(path))) => path.into_path().ok().map(|path| path.display().to_string()),
        _ => None,
    }
}

#[tauri::command]
pub async fn docker_pick_directory(app: tauri::AppHandle) -> Option<String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let app_for_thread = app.clone();
    let dispatched = app.run_on_main_thread(move || {
        app_for_thread
            .dialog()
            .file()
            .set_title("选择构建上下文目录")
            .pick_folder(move |path| {
                let _ = sender.send(path);
            });
    });
    if dispatched.is_err() {
        return None;
    }
    match tokio::time::timeout(Duration::from_secs(300), receiver).await {
        Ok(Ok(Some(path))) => path.into_path().ok().map(|path| path.display().to_string()),
        _ => None,
    }
}

fn registry_credentials(
    registry: &str,
    username: &str,
    password: &str,
) -> Result<(String, String), SshError> {
    let registry = validate::registry_host(registry)?;
    let username = username.trim();
    if username.is_empty()
        || username.len() > 128
        || username.chars().any(char::is_control)
    {
        return Err(SshError::invalid_input("用户名无效"));
    }
    if password.is_empty() || password.len() > 512 {
        return Err(SshError::invalid_input("密码不能为空且不超过 512 字节"));
    }
    Ok((registry, username.to_string()))
}

/// CLI 侧的 registry 请求（login / manifest inspect）不会读取 daemon.json 的代理，
/// 需要显式继承当前 daemon 代理；读取失败时退回直连，不阻塞操作。
async fn cli_proxy(
    state: &AppState,
    handle: &russh::client::Handle<ClientHandler>,
    server_id: &str,
) -> Option<String> {
    let secret = PendingSecret(state.docker.password_copy(server_id));
    let config = system::registry_config(handle, secret.as_deref()).await.ok()?;
    let url = if !config.https_proxy.is_empty() {
        config.https_proxy
    } else {
        config.http_proxy
    };
    if url.is_empty() {
        return None;
    }
    validate::proxy_url(&url).ok()
}

fn proxy_env(proxy: Option<&str>) -> String {
    proxy
        .map(|url| {
            let quoted = shell_quote(url);
            format!("HTTP_PROXY={quoted} HTTPS_PROXY={quoted} ")
        })
        .unwrap_or_default()
}

fn record_registry(
    db: &crate::db::Database,
    server_id: &str,
    registry: &str,
    op: &str,
    started: Instant,
    permit_source: &str,
    result: &str,
) -> Result<i64, SshError> {
    let server_name = db
        .server_name(server_id)
        .unwrap_or_else(|_| server_id.to_string());
    let params = serde_json::json!({ "registry": registry }).to_string();
    crate::guard::write::record(
        db,
        server_id,
        &server_name,
        op,
        &params,
        result,
        started,
        permit_source,
        None,
    )
}

/// 登录不进入预览：预览载荷会落进护栏缓存，密码不应出现在那里。
/// 仍要求显式解锁、写锁与审计，密码只经 stdin 交给 `docker login`。
#[tauri::command]
pub async fn docker_registry_login(
    state: State<'_, Arc<AppState>>,
    guard: State<'_, Arc<GuardService>>,
    db_state: State<'_, DbState>,
    server_id: String,
    registry: String,
    username: String,
    password: String,
    on_event: tauri::ipc::Channel<DockerProgress>,
) -> Result<DockerWriteResult, SshError> {
    validate_server_id(&server_id)?;
    let (registry, username) = registry_credentials(&registry, &username, &password)?;
    let minutes = guard.require_permit(&server_id)?;
    let permit_source = format!("unlock:{minutes}m");
    let db = database(&db_state)?;
    let _write = guard.begin_write(&server_id)?;
    let handle = handle_for(state.inner(), &server_id).await?;
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    let started = Instant::now();
    let proxy = cli_proxy(state.inner(), &handle, &server_id).await;
    let script = format!(
        "{}docker login {} --username {} --password-stdin",
        proxy_env(proxy.as_deref()),
        shell_quote(&registry),
        shell_quote(&username)
    );
    let mut secret = password.into_bytes();
    let mut emit = |line: &str| {
        let _ = on_event.send(DockerProgress {
            line: line.to_string(),
        });
    };
    let result = exec::run(
        &handle,
        &script,
        ExecOptions {
            timeout: Duration::from_secs(90),
            max_bytes: 1024 * 1024,
            stdin: Some(&secret),
            cancel: None,
            elevated,
        },
        &mut emit,
    )
    .await;
    DockerState::wipe(&mut secret);
    let (outcome, message) = match result {
        Ok(output) if output.code == Some(0) => ("success", "登录成功".to_string()),
        Ok(output) => {
            let text = validate::sanitize_output(output.stderr_text().trim(), None);
            (
                "failed",
                if text.is_empty() {
                    "登录失败：请检查用户名、密码与仓库地址".to_string()
                } else {
                    text
                },
            )
        }
        Err(error) => (crate::guard::write::outcome_for(&error), error.message),
    };
    let audit_id = record_registry(
        &db,
        &server_id,
        &registry,
        "docker.registry.login",
        started,
        &permit_source,
        outcome,
    )
    .map_err(|audit| {
        crate::guard::write::audit_write_failed(
            SshError::docker_failed(if outcome == "success" {
                "登录已执行"
            } else {
                "登录未成功"
            }),
            &audit,
            outcome == "success",
        )
    })?;
    Ok(DockerWriteResult {
        audit_id,
        target: registry,
        result: outcome.to_string(),
        message,
    })
}

#[tauri::command]
pub async fn docker_registry_logout(
    state: State<'_, Arc<AppState>>,
    guard: State<'_, Arc<GuardService>>,
    db_state: State<'_, DbState>,
    server_id: String,
    registry: String,
    on_event: tauri::ipc::Channel<DockerProgress>,
) -> Result<DockerWriteResult, SshError> {
    validate_server_id(&server_id)?;
    let registry = validate::registry_host(&registry)?;
    let minutes = guard.require_permit(&server_id)?;
    let permit_source = format!("unlock:{minutes}m");
    let db = database(&db_state)?;
    let _write = guard.begin_write(&server_id)?;
    let handle = handle_for(state.inner(), &server_id).await?;
    let elevated = crate::ssh::session::docker_elevated(state.inner(), &server_id).await;
    let started = Instant::now();
    let script = format!("docker logout {}", shell_quote(&registry));
    let mut emit = |line: &str| {
        let _ = on_event.send(DockerProgress {
            line: line.to_string(),
        });
    };
    let result = exec::run(
        &handle,
        &script,
        ExecOptions {
            timeout: Duration::from_secs(60),
            max_bytes: 512 * 1024,
            stdin: None,
            cancel: None,
            elevated,
        },
        &mut emit,
    )
    .await;
    let (outcome, message) = match result {
        Ok(output) if output.code == Some(0) => ("success", "已退出登录".to_string()),
        Ok(output) => ("failed", output.stderr_text().trim().to_string()),
        Err(error) => (crate::guard::write::outcome_for(&error), error.message),
    };
    let audit_id = record_registry(
        &db,
        &server_id,
        &registry,
        "docker.registry.logout",
        started,
        &permit_source,
        outcome,
    )
    .map_err(|audit| {
        crate::guard::write::audit_write_failed(
            SshError::docker_failed(if outcome == "success" {
                "退出登录已执行"
            } else {
                "退出登录未成功"
            }),
            &audit,
            outcome == "success",
        )
    })?;
    Ok(DockerWriteResult {
        audit_id,
        target: registry,
        result: outcome.to_string(),
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_env_is_shell_quoted_and_optional() {
        assert_eq!(
            proxy_env(Some("http://192.168.5.4:7890")),
            "HTTP_PROXY='http://192.168.5.4:7890' HTTPS_PROXY='http://192.168.5.4:7890' "
        );
        assert_eq!(proxy_env(None), "");
    }

    #[test]
    fn container_title_combines_name_and_server() {
        assert_eq!(container_title("shop-web-1", "nas"), "shop-web-1 · nas");
        assert_eq!(container_title("", "nas"), "容器终端 · nas");
    }
}

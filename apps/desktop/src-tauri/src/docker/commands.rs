//! 全部 `docker_*` 命令。命令层只做参数校验、取会话与调用 read/write/system。

use std::sync::Arc;

use tauri::State;

use crate::alerts::now_ms;
use crate::db::{DbError, DbState};
use crate::docker::dto::{
    DockerCapabilities, DockerContainer, DockerContainerDetail, DockerImage, DockerOverview,
    DockerPreview, DockerProgress, DockerStatsSample, DockerWriteInput, DockerWriteResult,
    RegistrySearchItem,
};
use crate::docker::read;
use crate::docker::validate;
use crate::docker::write::{self, WritePayload};
use crate::guard::GuardService;
use crate::ssh::commands::validate_server_id;
use crate::ssh::error::SshError;
use crate::ssh::sftp::client;
use crate::ssh::state::AppState;

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

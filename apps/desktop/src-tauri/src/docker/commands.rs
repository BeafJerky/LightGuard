//! 全部 `docker_*` 命令。命令层只做参数校验、取会话与调用 read/write/system。

use std::sync::Arc;

use tauri::State;

use crate::docker::dto::{
    DockerCapabilities, DockerContainer, DockerContainerDetail, DockerImage, DockerOverview,
    DockerStatsSample, RegistrySearchItem,
};
use crate::docker::read;
use crate::docker::validate;
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

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use super::super::error::SshError;
use super::super::state::AppState;
use super::client::{self, TransferHandle};
use super::dto::{
    DownloadResult, DownloadStatus, FileEntry, FileKind, FileListing, ImagePreview, TextPreview,
    TransferProgress,
};
use super::list;
use super::path::{basename, sanitize_local_name, validate_remote_path};
use super::preview;
use super::transfer::{self, TransferOutcome};

fn validate_id(value: &str, label: &str) -> Result<(), SshError> {
    let valid = !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    if valid {
        Ok(())
    } else {
        Err(SshError::invalid_input(format!("{label}无效")))
    }
}

fn test_download_dir() -> Option<PathBuf> {
    std::env::var_os("LG_TEST_DOWNLOAD_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
}

#[tauri::command]
pub async fn sftp_home(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<String, SshError> {
    validate_id(&server_id, "服务器标识")?;
    list::home(state.inner(), &server_id).await
}

#[tauri::command]
pub async fn sftp_list(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    path: String,
) -> Result<FileListing, SshError> {
    validate_id(&server_id, "服务器标识")?;
    validate_remote_path(&path)?;
    list::list(state.inner(), &server_id, &path).await
}

#[tauri::command]
pub async fn sftp_stat(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    path: String,
) -> Result<FileEntry, SshError> {
    validate_id(&server_id, "服务器标识")?;
    validate_remote_path(&path)?;
    list::stat(state.inner(), &server_id, &path).await
}

#[tauri::command]
pub async fn sftp_preview_text(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    path: String,
) -> Result<TextPreview, SshError> {
    validate_id(&server_id, "服务器标识")?;
    validate_remote_path(&path)?;
    preview::text(state.inner(), &server_id, &path).await
}

#[tauri::command]
pub async fn sftp_preview_image(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    path: String,
) -> Result<ImagePreview, SshError> {
    validate_id(&server_id, "服务器标识")?;
    validate_remote_path(&path)?;
    preview::image(state.inner(), &server_id, &path).await
}

#[tauri::command]
pub async fn sftp_download(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    server_id: String,
    path: String,
    kind: FileKind,
    transfer_id: String,
    on_event: Channel<TransferProgress>,
) -> Result<DownloadResult, SshError> {
    validate_id(&server_id, "服务器标识")?;
    validate_id(&transfer_id, "传输标识")?;
    validate_remote_path(&path)?;

    let cancel = Arc::new(AtomicBool::new(false));
    state.transfers.lock().await.insert(
        transfer_id.clone(),
        TransferHandle {
            server_id: server_id.clone(),
            cancel: cancel.clone(),
        },
    );

    let result = run_download(
        &app,
        state.inner(),
        &server_id,
        &path,
        kind,
        &transfer_id,
        &cancel,
        &on_event,
    )
    .await;

    state.transfers.lock().await.remove(&transfer_id);
    result
}

fn canceled_result() -> DownloadResult {
    DownloadResult {
        status: DownloadStatus::Canceled,
        target_path: None,
        bytes: 0,
    }
}

async fn run_download(
    app: &AppHandle,
    state: &AppState,
    server_id: &str,
    path: &str,
    kind: FileKind,
    transfer_id: &str,
    cancel: &Arc<AtomicBool>,
    on_event: &Channel<TransferProgress>,
) -> Result<DownloadResult, SshError> {
    if cancel.load(Ordering::SeqCst) {
        return Ok(canceled_result());
    }

    // The stat goes through the retry wrapper too: a dead channel detected here
    // is recovered before the save dialog ever opens.
    let entry = client::with_retry(state, server_id, |client| async move {
        list::stat_with(&client, path).await
    })
    .await?;
    let resolved = entry.target_kind.unwrap_or(entry.kind);
    if kind != resolved {
        return Err(SshError::path_invalid("文件类型已变化，请刷新后重试"));
    }

    let is_dir = matches!(resolved, FileKind::Dir);
    let default_name = if is_dir {
        format!("{}.tar.gz", sanitize_local_name(&basename(path), "download"))
    } else {
        sanitize_local_name(&basename(path), "download")
    };

    // Slots are taken before the dialog so a cancellation while queued is seen
    // before the user is asked where to save.
    let _permit = state
        .transfer_slots
        .acquire()
        .await
        .map_err(|_| SshError::internal("传输队列已关闭"))?;
    if cancel.load(Ordering::SeqCst) {
        return Ok(canceled_result());
    }

    let target = match test_download_dir() {
        Some(directory) => directory.join(default_name),
        None => match transfer::pick_save_path(app, default_name).await {
            Some(target) => target,
            None => return Ok(canceled_result()),
        },
    };
    if cancel.load(Ordering::SeqCst) {
        return Ok(canceled_result());
    }

    // The save path and permit are fixed, so a retry only rebuilds the network
    // side (fresh SFTP client, and for directories a fresh SSH channel).
    let outcome = client::with_retry(state, server_id, |client| {
        let target = &target;
        async move {
            let mut progress = |transferred: u64, total: Option<u64>| {
                let _ = on_event.send(TransferProgress::Progress {
                    transferred,
                    total,
                });
            };
            if is_dir {
                let handle = client::session_handle(state, server_id).await?;
                transfer::download_dir(
                    &handle,
                    &client,
                    path,
                    &target,
                    transfer_id,
                    cancel,
                    &mut progress,
                )
                .await
            } else {
                transfer::download_file(
                    &client,
                    path,
                    &target,
                    transfer_id,
                    cancel,
                    &mut progress,
                )
                .await
            }
        }
    })
    .await?;

    Ok(match outcome {
        TransferOutcome::Saved { bytes } => DownloadResult {
            status: DownloadStatus::Saved,
            target_path: Some(target.display().to_string()),
            bytes,
        },
        TransferOutcome::Canceled { bytes } => DownloadResult {
            status: DownloadStatus::Canceled,
            target_path: None,
            bytes,
        },
    })
}

#[tauri::command]
pub async fn sftp_transfer_cancel(
    state: State<'_, Arc<AppState>>,
    transfer_id: String,
) -> Result<(), SshError> {
    validate_id(&transfer_id, "传输标识")?;
    if let Some(handle) = state.transfers.lock().await.get(&transfer_id) {
        handle.cancel.store(true, Ordering::SeqCst);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_transfer_ids() {
        assert!(validate_id("t-01_a", "传输标识").is_ok());
        assert!(validate_id("../x", "传输标识").is_err());
        assert!(validate_id("", "传输标识").is_err());
    }
}

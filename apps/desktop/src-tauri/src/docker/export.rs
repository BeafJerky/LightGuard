//! 流式导出：把远端命令的 stdout 写入本地文件。
//! 容器导出只读镜像内容，镜像导出只读本地镜像，都不改变远端状态。

use std::sync::atomic::AtomicBool;
use std::time::Duration;

use russh::ChannelMsg;
use tauri::ipc::Channel;
use tokio::io::AsyncWriteExt;

use crate::ssh::error::SshError;
use crate::ssh::sftp::dto::TransferProgress;
use crate::ssh::sftp::path::shell_quote;
use crate::ssh::state::AppState;
use crate::ssh::sftp::client;

pub async fn export(
    app: &tauri::AppHandle,
    state: &AppState,
    server_id: &str,
    container_id: &str,
    transfer_id: &str,
    on_event: &Channel<TransferProgress>,
) -> Result<String, SshError> {
    let name = format!(
        "container-{}.tar.gz",
        container_id.chars().take(12).collect::<String>()
    );
    let script = stream_script(&format!("docker export {}", shell_quote(container_id)));
    let _ = transfer_id;
    stream_to_file(app, state, server_id, &name, &script, "导出容器", on_event).await
}

pub async fn export_image(
    app: &tauri::AppHandle,
    state: &AppState,
    server_id: &str,
    image_id: &str,
    display: &str,
    on_event: &Channel<TransferProgress>,
) -> Result<String, SshError> {
    let safe = display
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .take(48)
        .collect::<String>();
    let reference = if image_id.starts_with("sha256:") {
        image_id.to_string()
    } else {
        format!("sha256:{image_id}")
    };
    let name = format!("image-{safe}.tar.gz");
    let script = stream_script(&format!(
        "docker image save {}",
        shell_quote(&reference)
    ));
    stream_to_file(app, state, server_id, &name, &script, "导出镜像", on_event).await
}

/// POSIX sh 没有 pipefail：先把 save/export 落到远端临时文件，成功后再 gzip 流出，
/// 这样 docker 的失败码不会被管道末端的 gzip 吞掉；任何结局都清理临时文件。
fn stream_script(source: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = format!("/tmp/lightguard-export-{stamp}-{seq}.tar");
    let quoted = shell_quote(&temp);
    format!("{source} > {quoted} && gzip -c {quoted}; rc=$?; rm -f {quoted}; exit $rc")
}

async fn stream_to_file(
    app: &tauri::AppHandle,
    state: &AppState,
    server_id: &str,
    default_name: &str,
    script: &str,
    label: &str,
    on_event: &Channel<TransferProgress>,
) -> Result<String, SshError> {
    let Some(target) = crate::ssh::sftp::transfer::pick_save_path(app, default_name.to_string()).await
    else {
        return Err(SshError::transfer_failed("已取消保存"));
    };
    let handle = client::session_handle(state, server_id).await?;
    let elevated = crate::ssh::session::docker_elevated(state, server_id).await;
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let _ = cancel;
    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|error| SshError::docker_failed(error.to_string()))?;
    let command = if elevated {
        format!("sudo -n sh -c {}", shell_quote(script))
    } else {
        script.to_string()
    };
    channel
        .exec(true, command)
        .await
        .map_err(|error| SshError::docker_failed(error.to_string()))?;

    let mut file = tokio::fs::File::create(&target)
        .await
        .map_err(|error| SshError::transfer_failed(format!("无法创建本地文件：{error}")))?;
    let mut transferred = 0u64;
    let mut code = None;
    let mut stderr = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1800);
    let mut eof_at: Option<std::time::Instant> = None;
    loop {
        // Eof 后只等 2 秒的退出状态；服务端可能不再发 Close，不能一直拖到总超时。
        if eof_at.is_some_and(|at| at.elapsed() > Duration::from_secs(2)) {
            break;
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            let _ = tokio::fs::remove_file(&target).await;
            return Err(SshError::timeout(format!("{label}超时")));
        }
        match tokio::time::timeout(remaining.min(Duration::from_millis(500)), channel.wait()).await {
            Ok(Some(ChannelMsg::Data { data })) => {
                if let Err(error) = file.write_all(&data).await {
                    drop(file);
                    let _ = tokio::fs::remove_file(&target).await;
                    return Err(SshError::transfer_failed(format!(
                        "写入本地文件失败：{error}"
                    )));
                }
                transferred += data.len() as u64;
                let _ = on_event.send(TransferProgress::Progress {
                    transferred,
                    total: None,
                });
            }
            Ok(Some(ChannelMsg::ExtendedData { data, .. })) => {
                if stderr.len() < 4096 {
                    let room = 4096 - stderr.len();
                    stderr.extend_from_slice(&data[..data.len().min(room)]);
                }
            }
            Ok(Some(ChannelMsg::ExitStatus { exit_status })) => {
                code = Some(exit_status);
                break;
            }
            // Eof 只表示对端不再写数据，退出状态可能随后才到。
            Ok(Some(ChannelMsg::Eof)) => {
                eof_at = Some(std::time::Instant::now());
                continue;
            }
            Ok(Some(ChannelMsg::Close)) | Ok(None) => break,
            Ok(Some(_)) => {}
            Err(_) => continue,
        }
    }
    // 退出状态偶尔在 Close 前丢失：有数据且 stderr 为空说明命令已正常收尾。
    let status_missing_ok = code.is_none() && transferred > 0 && stderr.is_empty();
    if code != Some(0) && !status_missing_ok {
        drop(file);
        let _ = tokio::fs::remove_file(&target).await;
        return Err(SshError::docker_failed(format!(
            "{label}失败：{}",
            String::from_utf8_lossy(&stderr).trim()
        )));
    }
    let _ = on_event.send(TransferProgress::Progress {
        transferred,
        total: Some(transferred),
    });
    Ok(target.display().to_string())
}
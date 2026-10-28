use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use russh_sftp::client::error::Error;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::StatusCode;

use super::super::error::SshError;
use super::super::session::ClientHandler;
use super::super::state::AppState;

pub struct SftpHandle {
    pub session_id: String,
    pub client: Arc<SftpSession>,
}

pub struct TransferHandle {
    pub server_id: String,
    pub cancel: Arc<AtomicBool>,
}

pub async fn with_timeout<F, T>(
    duration: Duration,
    label: &'static str,
    future: F,
) -> Result<T, SshError>
where
    F: Future<Output = Result<T, SshError>>,
{
    match tokio::time::timeout(duration, future).await {
        Ok(result) => result,
        Err(_) => Err(SshError::timeout(label)),
    }
}

pub fn map_error(error: Error, path: &str) -> SshError {
    match error {
        Error::Status(status) => match status.status_code {
            StatusCode::NoSuchFile => SshError::file_not_found(format!("路径不存在：{path}")),
            StatusCode::PermissionDenied => {
                SshError::permission_denied(format!("没有权限读取：{path}"))
            }
            _ => SshError::sftp_failed(format!("{path}：{}", status.error_message)),
        },
        Error::Timeout => SshError::timeout("SFTP 请求超时"),
        other => SshError::sftp_failed(format!("{path}：{other}")),
    }
}

pub async fn acquire(state: &AppState, server_id: &str) -> Result<Arc<SftpSession>, SshError> {
    let (session_id, handle) = session_snapshot(state, server_id).await?;
    if let Some(existing) = cached_client(state, server_id, &session_id).await {
        return Ok(existing);
    }

    let channel = handle
        .channel_open_session()
        .await
        .map_err(|error| SshError::sftp_failed(format!("打开 SFTP 通道失败：{error}")))?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|error| SshError::sftp_failed(format!("服务器未接受 SFTP 子系统：{error}")))?;
    let client = SftpSession::new(channel.into_stream())
        .await
        .map_err(|error| SshError::sftp_failed(format!("SFTP 握手失败：{error}")))?;

    Ok(store_client(state, server_id, session_id, client).await)
}

pub async fn session_handle(
    state: &AppState,
    server_id: &str,
) -> Result<Arc<russh::client::Handle<ClientHandler>>, SshError> {
    let (_, handle) = session_snapshot(state, server_id).await?;
    Ok(handle)
}

pub async fn drop_for_server(state: &AppState, server_id: &str) {
    let removed = state.sftp.lock().await.remove(server_id);
    if let Some(handle) = removed {
        let _ = handle.client.close().await;
    }
    cancel_for_server(state, Some(server_id)).await;
}

pub async fn clear_all(state: &AppState) {
    let handles = {
        let mut registry = state.sftp.lock().await;
        registry.drain().map(|(_, handle)| handle).collect::<Vec<_>>()
    };
    for handle in handles {
        let _ = handle.client.close().await;
    }
    cancel_for_server(state, None).await;
}

/// Session id plus shared handle, read under both locks without holding the
/// session map guard across the entry lock or any channel work.
async fn session_snapshot(
    state: &AppState,
    server_id: &str,
) -> Result<(String, Arc<russh::client::Handle<ClientHandler>>), SshError> {
    let session = {
        let sessions = state.sessions.lock().await;
        sessions
            .get(server_id)
            .cloned()
            .ok_or_else(SshError::session_not_found)?
    };
    let entry = session.lock().await;
    if entry.handle.is_closed() {
        return Err(SshError::session_closed());
    }
    Ok((entry.session_id.clone(), entry.handle.clone()))
}

async fn cached_client(
    state: &AppState,
    server_id: &str,
    session_id: &str,
) -> Option<Arc<SftpSession>> {
    let registry = state.sftp.lock().await;
    registry
        .get(server_id)
        .filter(|existing| existing.session_id == session_id)
        .map(|existing| existing.client.clone())
}

async fn store_client(
    state: &AppState,
    server_id: &str,
    session_id: String,
    client: SftpSession,
) -> Arc<SftpSession> {
    let mut registry = state.sftp.lock().await;
    if let Some(existing) = registry.get(server_id) {
        if existing.session_id == session_id {
            return existing.client.clone();
        }
    }
    let client = Arc::new(client);
    registry.insert(
        server_id.to_string(),
        SftpHandle {
            session_id,
            client: client.clone(),
        },
    );
    client
}

async fn cancel_for_server(state: &AppState, server_id: Option<&str>) {
    let transfers = state.transfers.lock().await;
    for handle in transfers.values() {
        if server_id.map_or(true, |id| handle.server_id == id) {
            handle.cancel.store(true, Ordering::SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use russh_sftp::protocol::{Status, StatusCode};

    fn status(code: StatusCode) -> Error {
        Error::Status(Status {
            id: 1,
            status_code: code,
            error_message: String::new(),
            language_tag: String::new(),
        })
    }

    #[test]
    fn maps_missing_and_forbidden_paths() {
        let missing = map_error(status(StatusCode::NoSuchFile), "/nope");
        assert_eq!(missing.code, "FILE_NOT_FOUND");
        let denied = map_error(status(StatusCode::PermissionDenied), "/root");
        assert_eq!(denied.code, "PERMISSION_DENIED");
    }

    #[test]
    fn maps_timeout_and_protocol_errors() {
        assert_eq!(map_error(Error::Timeout, "/tmp").code, "TIMEOUT");
        assert_eq!(
            map_error(Error::UnexpectedPacket, "/tmp").code,
            "SFTP_FAILED"
        );
    }

    #[tokio::test]
    async fn with_timeout_guards_slow_and_passes_fast_futures() {
        let slow = with_timeout(Duration::from_millis(10), "SFTP 请求超时", async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            Ok::<u8, SshError>(1)
        })
        .await;
        assert_eq!(slow.err().expect("slow future times out").code, "TIMEOUT");

        let fast = with_timeout(Duration::from_secs(5), "SFTP 请求超时", async {
            Ok::<u8, SshError>(7)
        })
        .await;
        assert_eq!(fast.expect("fast future resolves"), 7);
    }

    #[tokio::test]
    async fn acquire_reports_unknown_session() {
        let state = AppState::new(PathBuf::from("."));
        let error = acquire(&state, "missing").await.err().expect("no session");
        assert_eq!(error.code, "SESSION_NOT_FOUND");
        let error = session_handle(&state, "missing")
            .await
            .err()
            .expect("no session");
        assert_eq!(error.code, "SESSION_NOT_FOUND");
        assert!(state.sftp.lock().await.is_empty());
    }

    #[tokio::test]
    async fn registry_caches_by_session_and_replaces_on_reconnect() {
        let state = AppState::new(PathBuf::from("."));
        assert!(cached_client(&state, "srv", "s1").await.is_none());

        let first = store_client(&state, "srv", "s1".to_string(), fake_sftp().await).await;
        let hit = cached_client(&state, "srv", "s1")
            .await
            .expect("cache hit for same session");
        assert!(Arc::ptr_eq(&hit, &first));

        // Still an s1 session: the double-check inside the write path keeps the
        // first client instead of overwriting it with the late one.
        let raced = store_client(&state, "srv", "s1".to_string(), fake_sftp().await).await;
        assert!(Arc::ptr_eq(&raced, &first));

        // Reconnect produced a new session id: the stale entry must not be reused.
        assert!(cached_client(&state, "srv", "s2").await.is_none());
        let second = store_client(&state, "srv", "s2".to_string(), fake_sftp().await).await;
        assert!(!Arc::ptr_eq(&second, &first));
        {
            let registry = state.sftp.lock().await;
            let handle = registry.get("srv").expect("registry entry");
            assert_eq!(handle.session_id, "s2");
            assert!(Arc::ptr_eq(&handle.client, &second));
        }

        drop_for_server(&state, "srv").await;
        assert!(state.sftp.lock().await.is_empty());
        assert!(cached_client(&state, "srv", "s2").await.is_none());
    }

    #[tokio::test]
    async fn cancellation_targets_one_server_then_everything() {
        let state = AppState::new(PathBuf::from("."));
        let (alpha, beta) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
        {
            let mut transfers = state.transfers.lock().await;
            transfers.insert(
                "t1".to_string(),
                TransferHandle {
                    server_id: "alpha".to_string(),
                    cancel: alpha.clone(),
                },
            );
            transfers.insert(
                "t2".to_string(),
                TransferHandle {
                    server_id: "beta".to_string(),
                    cancel: beta.clone(),
                },
            );
        }

        drop_for_server(&state, "alpha").await;
        assert!(alpha.load(Ordering::SeqCst));
        assert!(!beta.load(Ordering::SeqCst));

        clear_all(&state).await;
        assert!(beta.load(Ordering::SeqCst));
    }

    /// A real [`SftpSession`] whose transport is an in-memory duplex: the peer
    /// answers the SFTP INIT with a VERSION packet and then idles. Enough for
    /// the registry tests, which never issue another request.
    async fn fake_sftp() -> SftpSession {
        let (client_side, mut server_side) = tokio::io::duplex(1024);
        tokio::spawn(async move {
            let mut header = [0u8; 4];
            if server_side.read_exact(&mut header).await.is_err() {
                return;
            }
            let mut body = vec![0u8; u32::from_be_bytes(header) as usize];
            if server_side.read_exact(&mut body).await.is_err() {
                return;
            }
            // SSH_FXP_VERSION(3), no extensions.
            let _ = server_side.write_all(&[0, 0, 0, 5, 2, 0, 0, 0, 3]).await;
            let _ = server_side.flush().await;
            let mut sink = [0u8; 256];
            while matches!(server_side.read(&mut sink).await, Ok(read) if read > 0) {}
        });
        SftpSession::new(client_side).await.expect("fake sftp handshake")
    }
}

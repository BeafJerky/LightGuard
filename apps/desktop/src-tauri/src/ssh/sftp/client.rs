use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use russh_sftp::client::error::Error;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::StatusCode;

use super::super::error::SshError;
use super::super::session::ClientHandler;
use super::super::state::AppState;

/// Upper bound for opening the channel, requesting the subsystem and the SFTP
/// `INIT`/`VERSION` exchange, so a peer that never answers cannot hang the
/// caller (the handshake runs outside any caller-side timeout).
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(15);

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
        // The server answered, so this is a business error even though the
        // request failed: retrying the same call cannot help.
        other @ Error::Limited(_) => SshError::sftp_failed(format!("{path}：{other}")),
        // These mean the channel itself is gone while the SSH session may still
        // be alive. Mark them retryable so callers can rebuild the channel.
        Error::Timeout => SshError::sftp_channel("SFTP 请求超时"),
        other @ (Error::IO(_) | Error::UnexpectedPacket | Error::UnexpectedBehavior(_)) => {
            SshError::sftp_channel(format!("{path}：{other}"))
        }
    }
}

/// Removes the cached client for `server_id` only when it is still the exact
/// `Arc` the caller failed on, so a concurrent fresh client is never dropped.
/// The channel is closed outside the registry lock.
pub async fn invalidate(state: &AppState, server_id: &str, client: &Arc<SftpSession>) {
    let removed = {
        let mut registry = state.sftp.lock().await;
        let matches = registry
            .get(server_id)
            .map(|handle| Arc::ptr_eq(&handle.client, client))
            .unwrap_or(false);
        if matches {
            registry.remove(server_id)
        } else {
            None
        }
    };
    if let Some(handle) = removed {
        let _ = handle.client.close().await;
    }
}

type SftpFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Runs `operation` against the cached client and, when it fails with a
/// retryable channel error, discards that exact client and runs the operation
/// once more on a freshly acquired session. Business errors never retry.
pub async fn retry_once<'a, T, A, I, F, Fut>(
    mut acquire: A,
    mut invalidate: I,
    operation: F,
) -> Result<T, SshError>
where
    A: FnMut() -> SftpFuture<'a, Result<Arc<SftpSession>, SshError>>,
    I: FnMut(Arc<SftpSession>) -> SftpFuture<'a, ()>,
    F: Fn(Arc<SftpSession>) -> Fut,
    Fut: Future<Output = Result<T, SshError>>,
{
    let client = acquire().await?;
    match operation(client.clone()).await {
        Err(error) if error.retryable => {
            invalidate(client).await;
            let fresh = acquire().await?;
            operation(fresh).await
        }
        result => result,
    }
}

/// [`retry_once`] bound to the app's session registry.
pub async fn with_retry<'a, T, F, Fut>(
    state: &'a AppState,
    server_id: &'a str,
    operation: F,
) -> Result<T, SshError>
where
    F: Fn(Arc<SftpSession>) -> Fut,
    Fut: Future<Output = Result<T, SshError>>,
{
    let acquire_slot = move || -> SftpFuture<'a, Result<Arc<SftpSession>, SshError>> {
        Box::pin(async move { acquire(state, server_id).await })
    };
    let invalidate_slot = move |client: Arc<SftpSession>| -> SftpFuture<'a, ()> {
        Box::pin(async move {
            invalidate(state, server_id, &client).await;
        })
    };
    retry_once(acquire_slot, invalidate_slot, operation).await
}

pub async fn acquire(state: &AppState, server_id: &str) -> Result<Arc<SftpSession>, SshError> {
    let (session_id, handle) = session_snapshot(state, server_id).await?;
    if let Some(existing) = cached_client(state, server_id, &session_id).await {
        return Ok(existing);
    }

    let client = with_timeout(HANDSHAKE_TIMEOUT, "SFTP 握手超时", async {
        let channel = handle
            .channel_open_session()
            .await
            .map_err(|error| SshError::sftp_failed(format!("打开 SFTP 通道失败：{error}")))?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|error| SshError::sftp_failed(format!("服务器未接受 SFTP 子系统：{error}")))?;
        SftpSession::new(channel.into_stream())
            .await
            .map_err(|error| SshError::sftp_failed(format!("SFTP 握手失败：{error}")))
    })
    .await?;

    // The handshake above awaits, so the server may have reconnected meanwhile:
    // the captured id can be stale and must not key the fresh SFTP session.
    let current = match current_session_id(state, server_id).await {
        Ok(current) => current,
        Err(error) => {
            let _ = client.close().await;
            return Err(error);
        }
    };
    adopt(state, server_id, &session_id, &current, client).await
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

/// The session id registered for `server_id` right now, re-read after the SFTP
/// handshake so a reconnect during that await is detected.
async fn current_session_id(state: &AppState, server_id: &str) -> Result<String, SshError> {
    let session = {
        let sessions = state.sessions.lock().await;
        sessions
            .get(server_id)
            .cloned()
            .ok_or_else(SshError::session_not_found)?
    };
    let entry = session.lock().await;
    Ok(entry.session_id.clone())
}

/// Post-handshake commit: a client whose handshake straddled a reconnect is
/// closed instead of cached, because its channel belongs to a dead session.
async fn adopt(
    state: &AppState,
    server_id: &str,
    captured: &str,
    current: &str,
    client: SftpSession,
) -> Result<Arc<SftpSession>, SshError> {
    if captured != current {
        let _ = client.close().await;
        return Err(SshError::sftp_failed("会话已更新，请重新加载"));
    }
    Ok(store_client(state, server_id, captured.to_string(), client).await)
}

async fn store_client(
    state: &AppState,
    server_id: &str,
    session_id: String,
    client: SftpSession,
) -> Arc<SftpSession> {
    let mut registry = state.sftp.lock().await;
    let existing = registry
        .get(server_id)
        .filter(|existing| existing.session_id == session_id)
        .map(|existing| existing.client.clone());
    if let Some(existing) = existing {
        // A concurrent acquire cached this session first: keep its client and
        // close the late one rather than leaking the channel.
        drop(registry);
        let _ = client.close().await;
        return existing;
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
    use std::sync::atomic::AtomicUsize;
    use std::sync::Mutex;
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
    fn classifies_business_errors_as_final() {
        let failure = map_error(status(StatusCode::Failure), "/tmp");
        assert_eq!(failure.code, "SFTP_FAILED");
        assert!(!failure.retryable);

        let limited = map_error(Error::Limited("open files".to_string()), "/tmp");
        assert_eq!(limited.code, "SFTP_FAILED");
        assert!(!limited.retryable);
        assert!(limited.message.contains("open files"));

        assert!(!map_error(status(StatusCode::NoSuchFile), "/tmp").retryable);
        assert!(!map_error(status(StatusCode::PermissionDenied), "/tmp").retryable);
    }

    #[test]
    fn classifies_channel_failures_as_retryable() {
        for error in [
            Error::Timeout,
            Error::UnexpectedPacket,
            Error::UnexpectedBehavior("bad packet".to_string()),
            Error::IO("broken pipe".to_string()),
        ] {
            let mapped = map_error(error, "/tmp");
            assert_eq!(mapped.code, "SFTP_FAILED");
            assert_eq!(mapped.stage, Some("sftp"));
            assert!(mapped.retryable, "channel failure must retry");
        }
        // The human message is unchanged from the previous flattening.
        assert_eq!(map_error(Error::Timeout, "/tmp").message, "SFTP 请求超时");
        assert_eq!(
            map_error(Error::UnexpectedPacket, "/tmp").message,
            "/tmp：Unexpected packet"
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
    async fn invalidate_removes_only_the_matching_client() {
        let state = AppState::new(PathBuf::from("."));
        let cached = store_client(&state, "srv", "s1".to_string(), fake_sftp().await).await;

        // A different client (e.g. a concurrent fresh acquire) must not evict
        // the cached one.
        let other = Arc::new(fake_sftp().await);
        invalidate(&state, "srv", &other).await;
        assert!(cached_client(&state, "srv", "s1").await.is_some());

        invalidate(&state, "srv", &cached).await;
        assert!(state.sftp.lock().await.is_empty());
        assert!(cached_client(&state, "srv", "s1").await.is_none());
    }

    #[tokio::test]
    async fn retries_once_with_a_fresh_session_after_invalidation() {
        let first = Arc::new(fake_sftp().await);
        let second = Arc::new(fake_sftp().await);
        let attempts = Arc::new(AtomicUsize::new(0));
        let invalidated: Arc<Mutex<Vec<Arc<SftpSession>>>> = Arc::new(Mutex::new(Vec::new()));

        let result = retry_once(
            {
                let first = first.clone();
                let second = second.clone();
                let attempts = attempts.clone();
                move || {
                    let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                    let session = if attempt == 0 {
                        first.clone()
                    } else {
                        second.clone()
                    };
                    Box::pin(async move { Ok(session) })
                }
            },
            {
                let invalidated = invalidated.clone();
                move |client: Arc<SftpSession>| {
                    invalidated.lock().expect("invalidate log").push(client);
                    Box::pin(async {})
                }
            },
            {
                let first = first.clone();
                move |client: Arc<SftpSession>| {
                    let stale = Arc::ptr_eq(&client, &first);
                    async move {
                        if stale {
                            Err(SshError::sftp_channel("通道已断开"))
                        } else {
                            Ok(7u8)
                        }
                    }
                }
            },
        )
        .await;

        assert_eq!(result.expect("retry succeeds"), 7);
        assert_eq!(attempts.load(Ordering::SeqCst), 2, "one retry, not a loop");
        let invalidated = invalidated.lock().expect("invalidate log");
        assert_eq!(invalidated.len(), 1);
        assert!(Arc::ptr_eq(&invalidated[0], &first));
        assert!(!Arc::ptr_eq(&invalidated[0], &second));
    }

    #[tokio::test]
    async fn business_errors_are_not_retried() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let invalidations = Arc::new(AtomicUsize::new(0));
        let result = retry_once(
            {
                let attempts = attempts.clone();
                move || {
                    attempts.fetch_add(1, Ordering::SeqCst);
                    Box::pin(async { Ok(Arc::new(fake_sftp().await)) })
                }
            },
            {
                let invalidations = invalidations.clone();
                move |_client: Arc<SftpSession>| {
                    invalidations.fetch_add(1, Ordering::SeqCst);
                    Box::pin(async {})
                }
            },
            |_client| async { Err::<u8, SshError>(SshError::file_not_found("gone")) },
        )
        .await;

        assert_eq!(result.expect_err("business error").code, "FILE_NOT_FOUND");
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
        assert_eq!(invalidations.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn rejects_sftp_session_adopted_after_reconnect() {
        let state = AppState::new(PathBuf::from("."));

        // Handshake started on s1, but the reconnect made s2 the live session:
        // the freshly built client must be rejected and nothing may be cached.
        let stale = adopt(&state, "srv", "s1", "s2", fake_sftp().await)
            .await
            .err()
            .expect("stale session is rejected");
        assert_eq!(stale.code, "SFTP_FAILED");
        assert!(stale.message.contains("会话已更新"));
        assert!(state.sftp.lock().await.is_empty());

        // Control: the same adoption succeeds while the session is unchanged.
        let stored = adopt(&state, "srv", "s1", "s1", fake_sftp().await)
            .await
            .expect("current session is cached");
        {
            let registry = state.sftp.lock().await;
            let handle = registry.get("srv").expect("registry entry");
            assert_eq!(handle.session_id, "s1");
            assert!(Arc::ptr_eq(&handle.client, &stored));
        }
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

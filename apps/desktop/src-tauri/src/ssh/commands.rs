use std::sync::Arc;
use std::time::Duration;

use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

use super::credentials::{CredentialStore, SecretBundle};
use super::dto::{
    AuthKind, ConnectInput, ConnectResult, CredentialStatus, ReconnectInput, ServerSnapshot,
    SessionStatus, StatusEvent, TerminalEvent, TerminalHandle, TerminalOpenInput,
};
use super::error::SshError;
use super::hostkey::HostKeyStore;
use super::session::{self, SessionEntry};
use super::state::AppState;
use super::terminal::{self, MAX_TERMINALS_PER_SERVER};
use crate::alerts::AlertService;
use crate::guard::GuardService;

const MAX_SECRET_LEN: usize = 64 * 1024;
const MAX_INPUT_LEN: usize = 128 * 1024;

#[tauri::command]
pub async fn ssh_connect(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    input: ConnectInput,
) -> Result<ConnectResult, SshError> {
    validate_server_id(&input.server_id)?;
    validate_connect(&input)?;

    let store = CredentialStore::from_config(&state.config_dir);
    let outcome = match session::connect(input.clone(), &state.config_dir, &store).await {
        Ok(outcome) => outcome,
        Err(error) => {
            // 指纹变化不是普通连接失败：它要进告警中心并弹出通知，所以先记账再返回。
            if error.code == "HOST_KEY_MISMATCH" {
                state
                    .alerts
                    .on_host_key_mismatch(&input.server_id, &error.message);
            }
            return Err(error);
        }
    };

    replace_session(&app, &state, &input.server_id, outcome.entry).await;
    Ok(outcome.result)
}

#[tauri::command]
pub async fn ssh_reconnect(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    input: ReconnectInput,
) -> Result<ConnectResult, SshError> {
    validate_server_id(&input.server_id)?;
    validate_host(&input.host)?;
    validate_user(&input.user)?;

    let store = CredentialStore::from_config(&state.config_dir);
    let outcome = match session::reconnect(
        &input.server_id,
        input.host.trim().to_string(),
        input.port,
        input.user.trim().to_string(),
        input.verify_host_key,
        &state.config_dir,
        &store,
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            if error.code == "HOST_KEY_MISMATCH" {
                state
                    .alerts
                    .on_host_key_mismatch(&input.server_id, &error.message);
            }
            return Err(error);
        }
    };

    let session_id = outcome.result.session_id.clone();
    replace_session(&app, &state, &input.server_id, outcome.entry).await;
    let _ = app.emit(
        "ssh://status",
        StatusEvent {
            server_id: input.server_id,
            state: "reconnected".to_string(),
            message: Some(session_id),
        },
    );
    Ok(outcome.result)
}

#[tauri::command]
pub async fn ssh_snapshot(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<ServerSnapshot, SshError> {
    validate_server_id(&server_id)?;
    let session = get_session(&state, &server_id).await?;
    let mut entry = session.lock().await;
    session::snapshot(&mut entry).await
}

#[tauri::command]
pub async fn ssh_disconnect(
    state: State<'_, Arc<AppState>>,
    guard: State<'_, Arc<GuardService>>,
    server_id: String,
) -> Result<(), SshError> {
    validate_server_id(&server_id)?;
    // 写操作进行中不允许断开：中途断线会让远端结果不可知，也拿不到审计回退入口。
    // 这里只拒绝断开请求，绝不反过来中止已经发出的写操作。
    if guard.is_write_active(&server_id) {
        return Err(SshError::write_busy());
    }
    let removed = state.sessions.lock().await.remove(&server_id);
    super::sftp::client::drop_for_server(state.inner(), &server_id).await;
    close_server_terminals(state.inner(), &server_id).await;
    // 显式断开：立刻停掉采样与登录监控（不进入离线宽限），并让许可与预览失效。
    state.alerts.on_explicit_disconnect(&server_id);
    guard.clear_server(&server_id);
    if let Some(session) = removed {
        let entry = session.lock().await;
        let _ = entry
            .handle
            .disconnect(russh::Disconnect::ByApplication, "user disconnect", "en")
            .await;
    }
    Ok(())
}

#[tauri::command]
pub async fn ssh_session_status(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<SessionStatus, SshError> {
    validate_server_id(&server_id)?;
    let session = state.sessions.lock().await.get(&server_id).cloned();
    let Some(session) = session else {
        return Ok(SessionStatus {
            connected: false,
            session_id: None,
        });
    };
    let entry = session.lock().await;
    if entry.handle.is_closed() {
        return Ok(SessionStatus {
            connected: false,
            session_id: Some(entry.session_id.clone()),
        });
    }
    Ok(SessionStatus {
        connected: true,
        session_id: Some(entry.session_id.clone()),
    })
}

#[tauri::command]
pub async fn ssh_save_credentials(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    auth: AuthKind,
    secret: String,
    passphrase: Option<String>,
) -> Result<(), SshError> {
    validate_server_id(&server_id)?;
    if secret.is_empty() {
        return Err(SshError::invalid_input("凭据内容不能为空"));
    }
    if secret.len() > MAX_SECRET_LEN {
        return Err(SshError::invalid_input("凭据内容过长"));
    }
    if passphrase.as_ref().is_some_and(|value| value.len() > 1024) {
        return Err(SshError::invalid_input("私钥口令过长"));
    }
    let store = CredentialStore::from_config(&state.config_dir);
    store.save(
        &server_id,
        &SecretBundle {
            auth,
            secret,
            passphrase,
        },
    )
}

#[tauri::command]
pub async fn ssh_credential_status(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<CredentialStatus, SshError> {
    validate_server_id(&server_id)?;
    let store = CredentialStore::from_config(&state.config_dir);
    let bundle = store.load(&server_id)?;
    Ok(match bundle {
        Some(bundle) => CredentialStatus {
            saved: true,
            auth: Some(bundle.auth),
        },
        None => CredentialStatus {
            saved: false,
            auth: None,
        },
    })
}

#[tauri::command]
pub async fn ssh_forget_credentials(
    state: State<'_, Arc<AppState>>,
    server_id: String,
) -> Result<(), SshError> {
    validate_server_id(&server_id)?;
    let store = CredentialStore::from_config(&state.config_dir);
    store.delete(&server_id)
}

#[tauri::command]
pub async fn ssh_forget_host_key(
    state: State<'_, Arc<AppState>>,
    host: String,
    port: u16,
) -> Result<(), SshError> {
    validate_host(&host)?;
    let store = HostKeyStore::new(state.config_dir.join("known_hosts.json"));
    store.forget(host.trim(), port)
}

#[tauri::command]
pub async fn ssh_terminal_open(
    state: State<'_, Arc<AppState>>,
    input: TerminalOpenInput,
    on_event: Channel<TerminalEvent>,
) -> Result<TerminalHandle, SshError> {
    validate_server_id(&input.server_id)?;
    validate_terminal_size(input.cols, input.rows)?;

    let session = get_session(&state, &input.server_id).await?;
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

    let title = {
        let entry = session.lock().await;
        format!("{}@{}", entry.user, entry.host)
    };

    let (term, reader) = {
        let entry = session.lock().await;
        terminal::open(
            &entry.handle,
            &input.server_id,
            &title,
            input.cols,
            input.rows,
        )
        .await?
    };

    let handle = TerminalHandle {
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
    Ok(handle)
}

#[tauri::command]
pub async fn ssh_terminal_write(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    terminal_id: String,
    data: String,
) -> Result<(), SshError> {
    validate_server_id(&server_id)?;
    validate_terminal_id(&terminal_id)?;
    if data.len() > MAX_INPUT_LEN {
        return Err(SshError::invalid_input("终端输入过长"));
    }
    let terminals = state.terminals.lock().await;
    let entry = terminals
        .get(&terminal_id)
        .filter(|entry| entry.server_id == server_id)
        .ok_or_else(|| SshError::terminal("终端标签不存在"))?;
    terminal::write(entry, &data).await
}

#[tauri::command]
pub async fn ssh_terminal_resize(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    terminal_id: String,
    cols: u32,
    rows: u32,
) -> Result<(), SshError> {
    validate_server_id(&server_id)?;
    validate_terminal_id(&terminal_id)?;
    validate_terminal_size(cols, rows)?;
    let terminals = state.terminals.lock().await;
    let entry = terminals
        .get(&terminal_id)
        .filter(|entry| entry.server_id == server_id)
        .ok_or_else(|| SshError::terminal("终端标签不存在"))?;
    terminal::resize(entry, cols, rows).await
}

#[tauri::command]
pub async fn ssh_terminal_close(
    state: State<'_, Arc<AppState>>,
    server_id: String,
    terminal_id: String,
) -> Result<(), SshError> {
    validate_server_id(&server_id)?;
    validate_terminal_id(&terminal_id)?;
    let removed = {
        let mut terminals = state.terminals.lock().await;
        match terminals.get(&terminal_id) {
            Some(entry) if entry.server_id == server_id => terminals.remove(&terminal_id),
            _ => None,
        }
    };
    if let Some(entry) = removed {
        terminal::close(entry).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn ssh_reset_local_data(
    state: State<'_, Arc<AppState>>,
    alerts: State<'_, Arc<AlertService>>,
    guard: State<'_, Arc<GuardService>>,
) -> Result<(), SshError> {
    let sessions = {
        let mut locked = state.sessions.lock().await;
        std::mem::take(&mut *locked)
    };
    for (_, session) in sessions {
        let entry = session.lock().await;
        if !entry.handle.is_closed() {
            let _ = entry
                .handle
                .disconnect(russh::Disconnect::ByApplication, "reset local data", "en")
                .await;
        }
    }

    let terminals = {
        let mut locked = state.terminals.lock().await;
        std::mem::take(&mut *locked)
    };
    for (_, entry) in terminals {
        terminal::close(entry).await;
    }

    super::sftp::client::clear_all(state.inner()).await;

    // 清空本地数据等于取消所有授权：告警与写许可都必须归零，清理后不能还留着
    // 有效许可或一次性预览。放在删文件之前，确保即使删文件失败也不会留下授权。
    alerts.reset();
    guard.clear_all();

    CredentialStore::from_config(&state.config_dir).clear()?;
    HostKeyStore::new(state.config_dir.join("known_hosts.json")).clear()
}

async fn get_session(
    state: &State<'_, Arc<AppState>>,
    server_id: &str,
) -> Result<Arc<Mutex<SessionEntry>>, SshError> {
    state
        .sessions
        .lock()
        .await
        .get(server_id)
        .cloned()
        .ok_or_else(SshError::session_not_found)
}

async fn replace_session(
    app: &AppHandle,
    state: &State<'_, Arc<AppState>>,
    server_id: &str,
    entry: SessionEntry,
) {
    super::sftp::client::drop_for_server(state.inner(), server_id).await;
    close_server_terminals(state.inner(), server_id).await;
    let handle = entry.handle.clone();
    let session = Arc::new(Mutex::new(entry));
    let previous = {
        let mut sessions = state.sessions.lock().await;
        let previous = sessions.insert(server_id.to_string(), session.clone());
        previous
    };
    if let Some(previous) = previous {
        let old = previous.lock().await;
        if !old.handle.is_closed() {
            let _ = old
                .handle
                .disconnect(russh::Disconnect::ByApplication, "replaced", "en")
                .await;
        }
    }
    watch_session(app.clone(), state.inner().clone(), server_id.to_string(), session);
    // 新连接：上一轮遗留的许可与预览一律作废，然后开始采样与登录监控。
    state.guard.clear_server(server_id);
    state.alerts.start(server_id.to_string(), handle);
}

fn watch_session(
    app: AppHandle,
    state: Arc<AppState>,
    server_id: String,
    session: Arc<Mutex<SessionEntry>>,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let closed = {
                let entry = session.lock().await;
                entry.handle.is_closed()
            };
            if !closed {
                continue;
            }
            let current = {
                let mut sessions = state.sessions.lock().await;
                let same = sessions
                    .get(&server_id)
                    .map(|stored| Arc::ptr_eq(stored, &session))
                    .unwrap_or(false);
                if same {
                    sessions.remove(&server_id);
                }
                same
            };
            if current {
                super::sftp::client::drop_for_server(&state, &server_id).await;
                close_server_terminals(&state, &server_id).await;
                // 被动断线：停止采样与登录监控，按设置进入离线宽限；许可立刻失效。
                // 正在跑的写操作不在这里中止——它拿着自己的写锁，结束后自行释放。
                state.alerts.on_session_closed(&server_id);
                state.guard.clear_server(&server_id);
                let _ = app.emit(
                    "ssh://status",
                    StatusEvent {
                        server_id,
                        state: "closed".to_string(),
                        message: None,
                    },
                );
            }
            break;
        }
    });
}

async fn close_server_terminals(state: &AppState, server_id: &str) {
    let entries = {
        let mut terminals = state.terminals.lock().await;
        let ids: Vec<String> = terminals
            .iter()
            .filter(|(_, entry)| entry.server_id == server_id)
            .map(|(id, _)| id.clone())
            .collect();
        ids.into_iter()
            .filter_map(|id| terminals.remove(&id))
            .collect::<Vec<_>>()
    };
    for entry in entries {
        terminal::close(entry).await;
    }
}

fn validate_server_id(value: &str) -> Result<(), SshError> {
    let valid = !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    if valid {
        Ok(())
    } else {
        Err(SshError::invalid_input("服务器标识无效"))
    }
}

fn validate_terminal_id(value: &str) -> Result<(), SshError> {
    let valid = !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'));
    if valid {
        Ok(())
    } else {
        Err(SshError::invalid_input("终端标识无效"))
    }
}

fn validate_host(value: &str) -> Result<(), SshError> {
    let host = value.trim();
    if host.is_empty() {
        return Err(SshError::invalid_input("服务器地址不能为空"));
    }
    if host.len() > 253 || host.chars().any(char::is_whitespace) {
        return Err(SshError::invalid_input("服务器地址无效"));
    }
    Ok(())
}

fn validate_user(value: &str) -> Result<(), SshError> {
    let user = value.trim();
    if user.is_empty() {
        return Err(SshError::invalid_input("登录用户不能为空"));
    }
    if user.len() > 64 || user.chars().any(char::is_whitespace) {
        return Err(SshError::invalid_input("登录用户无效"));
    }
    Ok(())
}

fn validate_connect(input: &ConnectInput) -> Result<(), SshError> {
    validate_host(&input.host)?;
    validate_user(&input.user)?;
    if input.port == 0 {
        return Err(SshError::invalid_input("端口必须在 1–65535 之间"));
    }
    if input.secret.len() > MAX_SECRET_LEN {
        return Err(SshError::invalid_input("凭据内容过长"));
    }
    if input
        .passphrase
        .as_ref()
        .is_some_and(|value| value.len() > 1024)
    {
        return Err(SshError::invalid_input("私钥口令过长"));
    }
    Ok(())
}

fn validate_terminal_size(cols: u32, rows: u32) -> Result<(), SshError> {
    if !(1..=500).contains(&cols) || !(1..=500).contains(&rows) {
        return Err(SshError::invalid_input("终端尺寸超出范围"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_like_server_ids() {
        assert!(validate_server_id("srv-01_a").is_ok());
        assert!(validate_server_id("../secrets").is_err());
        assert!(validate_server_id("a/b").is_err());
        assert!(validate_server_id("").is_err());
    }

    #[test]
    fn rejects_unsafe_hosts_and_users() {
        assert!(validate_host("10.0.0.1").is_ok());
        assert!(validate_host("host name").is_err());
        assert!(validate_user("root").is_ok());
        assert!(validate_user("bad user").is_err());
        assert!(validate_user("").is_err());
    }

    #[test]
    fn terminal_size_bounds() {
        assert!(validate_terminal_size(80, 24).is_ok());
        assert!(validate_terminal_size(0, 24).is_err());
        assert!(validate_terminal_size(80, 501).is_err());
    }
}

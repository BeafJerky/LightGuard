use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use russh::client::AuthResult;
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use tokio::sync::Mutex;

use super::collector::{self, CollectorState};
use super::credentials::{CredentialStore, SecretBundle};
use super::dto::{
    AuthKind, ConnectInput, ConnectResult, EncryptionInfo, HostKeyState, ServerSnapshot,
};
use super::error::SshError;
use super::hostkey::HostKeyStore;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(12);
const AUTH_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Default)]
pub struct Observed {
    pub fingerprint: String,
    pub algorithm: String,
    pub encryption: Option<EncryptionInfo>,
}

#[derive(Clone)]
pub struct ClientHandler {
    observed: Arc<Mutex<Observed>>,
}

impl ClientHandler {
    pub fn new(observed: Arc<Mutex<Observed>>) -> Self {
        Self { observed }
    }
}

impl russh::client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let public = match server_key {
            PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
            PublicKeyOrCertificate::Certificate(certificate) => {
                russh::keys::PublicKey::from(certificate.public_key().clone())
            }
        };
        let mut observed = self.observed.lock().await;
        observed.fingerprint = public.fingerprint(HashAlg::Sha256).to_string();
        observed.algorithm = public.algorithm().to_string();
        Ok(true)
    }

    async fn kex_done(
        &mut self,
        _shared_secret: Option<&[u8]>,
        names: &russh::Names,
        _session: &mut russh::client::Session,
    ) -> Result<(), Self::Error> {
        let mut observed = self.observed.lock().await;
        observed.encryption = Some(EncryptionInfo {
            cipher: names.cipher.as_ref().to_string(),
            kex: names.kex.as_ref().to_string(),
            mac: names.server_mac.as_ref().to_string(),
        });
        Ok(())
    }
}

pub struct SessionEntry {
    pub handle: Arc<russh::client::Handle<ClientHandler>>,
    pub session_id: String,
    pub started_at: String,
    pub fingerprint: String,
    pub host_key_state: HostKeyState,
    pub encryption: Option<EncryptionInfo>,
    pub collector: CollectorState,
    pub latency_ms: u64,
    pub auth_label: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub elevation: crate::ssh::dto::ElevateStatus,
}

pub struct ConnectOutcome {
    pub entry: SessionEntry,
    pub result: ConnectResult,
}

pub fn client_config() -> russh::client::Config {
    russh::client::Config {
        inactivity_timeout: Some(Duration::from_secs(120)),
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        nodelay: true,
        ..Default::default()
    }
}

/// 固定探测命令：只按退出码/输出判定，不拼接任何外部输入。
const PROBE_SUDO: &str = "sudo -n true";
const PROBE_SFTP: &str = "sh -c 'for p in /usr/libexec/openssh/sftp-server /usr/lib/openssh/sftp-server; do [ -x \"$p\" ] && { printf %s \"$p\"; exit 0; }; done; command -v sftp-server'";
const PROBE_DOCKER: &str = "sh -c 'command -v docker'";

async fn exec_probe(
    handle: &russh::client::Handle<ClientHandler>,
    command: &str,
) -> (u32, String) {
    use russh::ChannelMsg;

    let mut channel = match handle.channel_open_session().await {
        Ok(channel) => channel,
        Err(error) => return (255, error.to_string()),
    };
    if let Err(error) = channel.exec(true, command).await {
        return (255, error.to_string());
    }
    let mut output = Vec::new();
    let mut code = None;
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        match tokio::time::timeout(remaining, channel.wait()).await {
            Ok(Some(ChannelMsg::Data { data })) => output.extend_from_slice(&data),
            Ok(Some(ChannelMsg::ExtendedData { data, .. })) => output.extend_from_slice(&data),
            Ok(Some(ChannelMsg::ExitStatus { exit_status })) => {
                code = Some(exit_status);
                break;
            }
            Ok(Some(ChannelMsg::Eof)) | Ok(Some(ChannelMsg::Close)) | Ok(None) => break,
            Ok(Some(_)) => {}
            Err(_) => break,
        }
    }
    let code = settle_exit_status(&mut channel, code).await;
    (
        code.unwrap_or(255),
        String::from_utf8_lossy(&output).trim().to_string(),
    )
}

/// 部分服务器先发 EOF/Close，再补 ExitStatus；给一个短暂的收尾窗口，
/// 否则 `sudo -n true` 这类无输出命令会被误判为失败。
async fn settle_exit_status(
    channel: &mut russh::Channel<russh::client::Msg>,
    mut code: Option<u32>,
) -> Option<u32> {
    use russh::ChannelMsg;

    if code.is_some() {
        return code;
    }
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(1);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return code;
        }
        match tokio::time::timeout(remaining, channel.wait()).await {
            Ok(Some(ChannelMsg::ExitStatus { exit_status })) => {
                code = Some(exit_status);
                return code;
            }
            Ok(Some(ChannelMsg::Data { .. }))
            | Ok(Some(ChannelMsg::ExtendedData { .. }))
            | Ok(Some(_)) => {}
            Ok(None) => return code,
            Err(_) => return code,
        }
    }
}

/// 认证后探测提权能力；任一失败只降级，不终止连接。总预算 8 秒。
async fn probe_elevation(
    handle: &russh::client::Handle<ClientHandler>,
    config: &crate::ssh::dto::ElevateConfig,
) -> crate::ssh::dto::ElevateStatus {
    use crate::ssh::dto::{ElevateConfig, ElevateStatus};

    if !config.enabled {
        return ElevateStatus::default();
    }
    let mut status = ElevateStatus {
        requested: true,
        ..Default::default()
    };
    let probe = async {
        let (base_code, _) = exec_probe(handle, PROBE_SUDO).await;
        if base_code != 0 {
            return (false, None, None, vec!["登录账户缺少免密 sudo".to_string()]);
        }
        let (sftp_code, sftp_path) = exec_probe(handle, PROBE_SFTP).await;
        let (docker_code, docker_path) = exec_probe(handle, PROBE_DOCKER).await;
        let mut reasons = Vec::new();
        let files = if sftp_code == 0 && !sftp_path.is_empty() {
            Some(sftp_path)
        } else {
            reasons.push("未找到 sftp-server".to_string());
            None
        };
        let docker = if docker_code == 0 && !docker_path.is_empty() {
            Some(docker_path)
        } else {
            reasons.push("未找到 docker 命令".to_string());
            None
        };
        (true, files, docker, reasons)
    };
    let (base, files, docker, reasons) =
        match tokio::time::timeout(std::time::Duration::from_secs(8), probe).await {
            Ok(value) => value,
            Err(_) => (false, None, None, vec!["提权探测超时".to_string()]),
        };
    status.base = base;
    if base {
        status.collect = ElevateConfig::child(config.collect);
        status.terminal = ElevateConfig::child(config.terminal);
        status.files = if ElevateConfig::child(config.files) {
            files
        } else {
            None
        };
        status.docker = if ElevateConfig::child(config.docker) {
            docker
        } else {
            None
        };
    }
    status.reasons = reasons;
    status
}

pub async fn connect(
    input: ConnectInput,
    config_dir: &Path,
    store: &CredentialStore,
) -> Result<ConnectOutcome, SshError> {
    if input.host.trim().is_empty() {
        return Err(SshError::invalid_input("服务器地址不能为空"));
    }
    if input.user.trim().is_empty() {
        return Err(SshError::invalid_input("登录用户不能为空"));
    }
    if input.secret.is_empty() {
        return Err(SshError::invalid_input(match input.auth {
            AuthKind::Password => "密码不能为空",
            AuthKind::Key => "私钥内容不能为空",
        }));
    }

    let started = Instant::now();
    let observed = Arc::new(Mutex::new(Observed::default()));
    let handler = ClientHandler::new(observed.clone());
    let config = Arc::new(client_config());

    let connect = russh::client::connect(
        config,
        (input.host.as_str(), input.port),
        handler,
    );
    let mut handle = match tokio::time::timeout(CONNECT_TIMEOUT, connect).await {
        Ok(Ok(handle)) => handle,
        Ok(Err(error)) => return Err(map_transport_error(error)),
        Err(_) => return Err(SshError::timeout("SSH 连接超时")),
    };

    let observed_now = observed.lock().await.clone();
    if observed_now.fingerprint.is_empty() {
        let _ = handle
            .disconnect(russh::Disconnect::ByApplication, "no host key", "en")
            .await;
        return Err(SshError::handshake("服务器未提供主机密钥"));
    }

    let host_keys = HostKeyStore::new(config_dir.join("known_hosts.json"));
    let host_key_state = match host_keys.verify(
        &input.host,
        input.port,
        &observed_now.fingerprint,
        &observed_now.algorithm,
        input.verify_host_key,
    ) {
        Ok(state) => state,
        Err(error) => {
            let _ = handle
                .disconnect(russh::Disconnect::ByApplication, "host key mismatch", "en")
                .await;
            return Err(error);
        }
    };

    authenticate(&mut handle, &input).await?;
    let elevation = probe_elevation(&handle, &input.elevate).await;

    let latency_ms = started.elapsed().as_millis() as u64;
    let auth_label = match input.auth {
        AuthKind::Password => "密码认证",
        AuthKind::Key => "私钥认证",
    }
    .to_string();

    let session_id = uid("s");
    let started_at = now_iso();

    let mut entry = SessionEntry {
        handle: Arc::new(handle),
        session_id,
        started_at,
        fingerprint: observed_now.fingerprint,
        host_key_state,
        encryption: observed_now.encryption,
        collector: CollectorState::default(),
        latency_ms,
        auth_label,
        host: input.host.clone(),
        port: input.port,
        user: input.user.clone(),
        elevation: elevation.clone(),
    };

    let snapshot = match collect_and_map(&mut entry).await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let _ = entry
                .handle
                .disconnect(russh::Disconnect::ByApplication, "collect failed", "en")
                .await;
            return Err(error);
        }
    };

    if input.remember {
        // A missing OS keyring must not tear down a working session; the UI
        // reports the credential as unsaved and asks again next launch.
        let _ = store.save(
            &input.server_id,
            &SecretBundle {
                auth: input.auth,
                secret: input.secret,
                passphrase: input.passphrase,
            },
        );
    }

    let result = ConnectResult {
        session_id: entry.session_id.clone(),
        started_at: entry.started_at.clone(),
        fingerprint: entry.fingerprint.clone(),
        host_key_state: entry.host_key_state,
        auth_label: entry.auth_label.clone(),
        latency_ms: entry.latency_ms,
        snapshot,
        encryption: entry.encryption.clone(),
        elevation: entry.elevation.clone(),
    };
    Ok(ConnectOutcome { entry, result })
}

pub async fn reconnect(
    server_id: &str,
    host: String,
    port: u16,
    user: String,
    verify_host_key: bool,
    elevate: crate::ssh::dto::ElevateConfig,
    config_dir: &Path,
    store: &CredentialStore,
) -> Result<ConnectOutcome, SshError> {
    let bundle = store.load(server_id)?.ok_or_else(|| {
        let mut error = SshError::auth_failed();
        error.hint = "没有可用的已保存凭据，请更新凭据后重试。".to_string();
        error
    })?;
    connect(
        ConnectInput {
            server_id: server_id.to_string(),
            host,
            port,
            user,
            auth: bundle.auth,
            secret: bundle.secret,
            passphrase: bundle.passphrase,
            remember: false,
            verify_host_key,
            elevate,
        },
        config_dir,
        store,
    )
    .await
}

pub async fn snapshot(entry: &mut SessionEntry) -> Result<ServerSnapshot, SshError> {
    collect_and_map(entry).await
}

/// 当前会话请求并实际生效的提权状态；会话不存在时视为全关。
pub async fn elevation_of(
    state: &crate::ssh::state::AppState,
    server_id: &str,
) -> crate::ssh::dto::ElevateStatus {
    let session = {
        let sessions = state.sessions.lock().await;
        sessions.get(server_id).cloned()
    };
    match session {
        Some(session) => session.lock().await.elevation.clone(),
        None => crate::ssh::dto::ElevateStatus::default(),
    }
}

pub async fn docker_elevated(state: &crate::ssh::state::AppState, server_id: &str) -> bool {
    elevation_of(state, server_id).await.docker_active()
}

async fn collect_and_map(entry: &mut SessionEntry) -> Result<ServerSnapshot, SshError> {
    match collector::collect(&entry.handle, &mut entry.collector, entry.elevation.collect_active()).await {
        Ok(snapshot) => Ok(snapshot),
        Err(error) => {
            if entry.handle.is_closed() {
                Err(SshError::session_closed())
            } else {
                Err(error)
            }
        }
    }
}

async fn authenticate(
    handle: &mut russh::client::Handle<ClientHandler>,
    input: &ConnectInput,
) -> Result<(), SshError> {
    let attempt = async {
        match input.auth {
            AuthKind::Password => handle
                .authenticate_password(input.user.as_str(), input.secret.as_str())
                .await
                .map_err(map_auth_error),
            AuthKind::Key => {
                let key = russh::keys::decode_secret_key(
                    &input.secret,
                    input.passphrase.as_deref(),
                )
                .map_err(|_| {
                    SshError::invalid_input("无法解析私钥，或私钥口令不正确")
                })?;
                let hash = handle
                    .best_supported_rsa_hash()
                    .await
                    .map_err(map_auth_error)?
                    .flatten();
                handle
                    .authenticate_publickey(
                        input.user.as_str(),
                        PrivateKeyWithHashAlg::new(Arc::new(key), hash),
                    )
                    .await
                    .map_err(map_auth_error)
            }
        }
    };

    let result = match tokio::time::timeout(AUTH_TIMEOUT, attempt).await {
        Ok(result) => result?,
        Err(_) => return Err(SshError::timeout("身份认证超时")),
    };

    match result {
        AuthResult::Success => Ok(()),
        AuthResult::Failure { .. } => Err(SshError::auth_failed()),
    }
}

fn map_auth_error(error: russh::Error) -> SshError {
    match error {
        russh::Error::IO(io) => SshError::host_unreachable(io.to_string()),
        russh::Error::Disconnect | russh::Error::HUP => SshError::auth_failed(),
        other => {
            let mapped = SshError::from(other);
            if mapped.code == "HANDSHAKE_FAILED" {
                SshError::auth_failed()
            } else {
                mapped
            }
        }
    }
}

fn map_transport_error(error: russh::Error) -> SshError {
    match error {
        russh::Error::IO(io) => SshError::host_unreachable(io.to_string()),
        russh::Error::ConnectionTimeout | russh::Error::InactivityTimeout => {
            SshError::timeout("SSH 连接超时")
        }
        other => SshError::from(other),
    }
}

fn uid(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}{millis:x}{counter:04x}")
}

pub fn now_iso() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0);
    iso_from_seconds(seconds)
}

fn iso_from_seconds(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let remainder = seconds % 86_400;
    let hour = remainder / 3_600;
    let minute = (remainder % 3_600) / 60;
    let second = remainder % 60;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = (z - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096)
            / 365;
    let mut year = year_of_era as i64 + era * 400;
    let day_of_year =
        day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    if month <= 2 {
        year += 1;
    }

    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_failure_maps_to_auth_failed() {
        assert_eq!(map_auth_error(russh::Error::HUP).code, "AUTH_FAILED");
    }

    #[test]
    fn config_prefers_modern_algorithms() {
        let config = client_config();
        assert!(config
            .preferred
            .key
            .iter()
            .any(|algorithm| algorithm.to_string().contains("ed25519")));
        assert!(config.keepalive_interval.is_some());
    }

    #[test]
    fn iso_formats_utc() {
        assert_eq!(iso_from_seconds(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_from_seconds(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(iso_from_seconds(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn session_ids_are_unique() {
        assert_ne!(uid("s"), uid("s"));
    }

    #[tokio::test]
    async fn real_server_readonly_probe() {
        let (Ok(host), Ok(user), Ok(password)) = (
            std::env::var("LG_SSH_HOST"),
            std::env::var("LG_SSH_USER"),
            std::env::var("LG_SSH_PASSWORD"),
        ) else {
            eprintln!("skipping: LG_SSH_HOST / LG_SSH_USER / LG_SSH_PASSWORD not set");
            return;
        };
        if host.is_empty() || user.is_empty() || password.is_empty() {
            eprintln!("skipping: LG_SSH_* variables are empty");
            return;
        }
        let port = std::env::var("LG_SSH_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(22);
        let dir = tempfile::tempdir().unwrap();
        let store = CredentialStore::with_master_key(dir.path().to_path_buf(), [9u8; 32]);

        let outcome = connect(
            ConnectInput {
                server_id: "it".to_string(),
                host,
                port,
                user,
                auth: AuthKind::Password,
                secret: password,
                passphrase: None,
                remember: false,
                verify_host_key: false,
                elevate: Default::default(),
            },
            dir.path(),
            &store,
        )
        .await
        .expect("read-only probe connect");

        let snapshot = &outcome.result.snapshot;
        assert!(!snapshot.hostname.is_empty());
        assert!(snapshot.cpu_cores > 0);
        assert!(snapshot.mem_total_gb > 0.0);
        assert!(!outcome.result.fingerprint.is_empty());

        let mut channel = outcome
            .entry
            .handle
            .channel_open_session()
            .await
            .expect("channel");
        channel.exec(true, "whoami").await.expect("exec");
        let mut output = Vec::new();
        while let Some(message) = channel.wait().await {
            match message {
                russh::ChannelMsg::Data { data } => output.extend_from_slice(&data),
                russh::ChannelMsg::Close | russh::ChannelMsg::Eof => break,
                _ => {}
            }
        }
        assert!(!String::from_utf8_lossy(&output).trim().is_empty());

        let _ = outcome
            .entry
            .handle
            .disconnect(russh::Disconnect::ByApplication, "probe done", "en")
            .await;
    }
}

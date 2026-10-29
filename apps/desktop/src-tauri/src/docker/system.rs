//! 系统级 Docker 操作：sudo 授权、服务启停与 daemon.json 合并。
//!
//! 密码只经 stdin 交给 `sudo -S`，不进命令行、不进审计、不进错误信息；
//! 预览阶段完成全部校验，执行前重新校验服务状态与 daemon.json，失败自动恢复。

use std::time::{Duration, Instant};

use russh::client::Handle;
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::docker::capability;
use crate::docker::daemon::{self, DaemonPayload, DAEMON_PATH};
use crate::docker::dto::{DockerSystemInput, DockerWriteResult, RegistryConfig, SudoStatus};
use crate::docker::exec::{self, ExecOptions, ExecOutput};
use crate::docker::state::DockerState;
use crate::docker::validate::{redact_url, sanitize_output};
use crate::docker::write::PreviewView;
use crate::guard::write::{audit_lost_after_success, audit_write_failed, outcome_for, record};
use crate::ssh::error::SshError;
use crate::ssh::session::ClientHandler;
use crate::ssh::sftp::client;
use crate::ssh::sftp::path::shell_quote;
use crate::ssh::state::AppState;

/// 预览时确认过的系统操作。执行前据此重新校验，不信任前端回传的脚本。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPayload {
    pub server_id: String,
    pub kind: String,
    pub audit_op: String,
    pub display: String,
    /// 已确认的变更项，例如 `registry-mirrors`；审计只记键名，不记值。
    pub changed: Vec<String>,
    /// 预览时的输入参数；服务操作存 `{ "active": bool }`，daemon 更新存原始合并输入。
    pub request: serde_json::Value,
}

pub const PROBE_SUDO_SCRIPT: &str = r#"if [ "$(id -u)" = "0" ]; then echo 'LG_SUDO=passwordless'; else
  if command -v sudo >/dev/null 2>&1; then
    if sudo -n true >/dev/null 2>&1; then echo 'LG_SUDO=passwordless'; else echo 'LG_SUDO=password'; fi
  else
    echo 'LG_SUDO=none'
  fi
fi"#;

pub const SERVICE_STATE_SCRIPT: &str = "if command -v systemctl >/dev/null 2>&1; then systemctl is-active docker 2>/dev/null || true; elif service docker status >/dev/null 2>&1; then echo active; else echo inactive; fi";

pub fn audit_op(kind: &str) -> String {
    format!("docker.{kind}")
}

pub fn verb_for(kind: &str) -> &'static str {
    match kind {
        "service.start" => "启动 Docker 服务",
        "service.stop" => "停止 Docker 服务",
        "daemon.update" => "更新 Docker 配置",
        _ => "系统操作",
    }
}

/// systemd 优先；没有 systemctl 时退回 SysV `service`。
pub fn service_action(action: &str) -> String {
    format!(
        "if command -v systemctl >/dev/null 2>&1; then systemctl {action} docker; else service docker {action}; fi"
    )
}

pub fn parse_service_state(text: &str) -> bool {
    text.lines().any(|line| line.trim() == "active")
}

pub fn parse_probe(text: &str) -> SudoStatus {
    match text
        .lines()
        .find_map(|line| line.trim().strip_prefix("LG_SUDO="))
        .unwrap_or("none")
    {
        "passwordless" => SudoStatus {
            passwordless: true,
            available: true,
        },
        "password" => SudoStatus {
            passwordless: false,
            available: true,
        },
        _ => SudoStatus {
            passwordless: false,
            available: false,
        },
    }
}

pub async fn probe(handle: &Handle<ClientHandler>) -> Result<SudoStatus, SshError> {
    let output = exec::run(
        handle,
        PROBE_SUDO_SCRIPT,
        ExecOptions {
            timeout: Duration::from_secs(15),
            max_bytes: 16 * 1024,
            stdin: None,
            cancel: None,
        },
        |_| {},
    )
    .await?;
    Ok(parse_probe(&output.stdout_text()))
}

pub async fn service_active(handle: &Handle<ClientHandler>) -> Result<bool, SshError> {
    let output = exec::run(
        handle,
        SERVICE_STATE_SCRIPT,
        ExecOptions {
            timeout: Duration::from_secs(15),
            max_bytes: 8 * 1024,
            stdin: None,
            cancel: None,
        },
        |_| {},
    )
    .await?;
    Ok(parse_service_state(&output.stdout_text()))
}

pub fn sudo_script(script: &str, with_password: bool) -> String {
    let quoted = shell_quote(script);
    if with_password {
        format!("sudo -S -p '' sh -c {quoted}")
    } else {
        format!("sudo -n sh -c {quoted}")
    }
}

fn password_str(password: Option<&[u8]>) -> Option<&str> {
    password.and_then(|bytes| std::str::from_utf8(bytes).ok())
}

/// 任何退出码都原样返回；鉴权失败与操作失败由调用方区分。
pub async fn run_sudo(
    handle: &Handle<ClientHandler>,
    script: &str,
    password: Option<&[u8]>,
    timeout: Duration,
) -> Result<ExecOutput, SshError> {
    let mut stdin = Vec::new();
    if let Some(bytes) = password {
        stdin.extend_from_slice(bytes);
        stdin.push(b'\n');
    }
    let command = sudo_script(script, password.is_some());
    let options = ExecOptions {
        timeout,
        max_bytes: 256 * 1024,
        stdin: if stdin.is_empty() {
            None
        } else {
            Some(stdin.as_slice())
        },
        cancel: None,
    };
    let result = exec::run(handle, &command, options, |_| {}).await;
    DockerState::wipe(&mut stdin);
    result
}

/// 只识别 sudo 自身的拒绝：密码错误、需要密码、没有 sudo 权限。
/// 其余失败属于脚本执行失败，交给 `failure_for`。
pub fn auth_error(output: &ExecOutput) -> Option<SshError> {
    if output.code == Some(0) {
        return None;
    }
    let lower = output.stderr_text().to_ascii_lowercase();
    if lower.contains("not in the sudoers") {
        return Some(SshError::sudo_failed("当前账号没有 sudo 权限"));
    }
    if lower.contains("incorrect password")
        || lower.contains("sorry, try again")
        || lower.contains("no password was provided")
    {
        return Some(SshError::sudo_failed("sudo 密码错误"));
    }
    if lower.contains("a password is required") || lower.contains("a terminal is required") {
        return Some(SshError::sudo_required());
    }
    None
}

/// dockerd 校验失败时会把配置里的代理/镜像地址原样回显，转发给用户前先抹掉其中的凭据。
/// 空白分隔逐段处理，段内只认 `http://` / `https://` 开头的核心，标点原样保留。
pub fn redact_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for segment in text.split_inclusive(char::is_whitespace) {
        out.push_str(&redact_segment(segment));
    }
    out
}

/// 只允许包在 URL 外面的标点被剥离，剥离后重新拼回去。
fn is_wrapping(ch: char) -> bool {
    matches!(
        ch,
        '"' | '\'' | '(' | ')' | '[' | ']' | ',' | ';' | ':' | '.'
    )
}

fn redact_segment(segment: &str) -> String {
    let head = segment.len() - segment.trim_start_matches(is_wrapping).len();
    let tail = segment.len() - segment.trim_end_matches(is_wrapping).len();
    if head + tail >= segment.len() {
        return segment.to_string();
    }
    let core = &segment[head..segment.len() - tail];
    if !(core.starts_with("http://") || core.starts_with("https://")) {
        return segment.to_string();
    }
    let redacted = redact_url(core);
    if redacted == core {
        return segment.to_string();
    }
    format!(
        "{}{}{}",
        &segment[..head],
        redacted,
        &segment[segment.len() - tail..]
    )
}

pub fn failure_for(kind: &str, output: &ExecOutput, password: Option<&[u8]>) -> SshError {
    let text = sanitize_output(&output.stderr_text(), password_str(password));
    let text = if text.is_empty() {
        sanitize_output(&output.stdout_text(), password_str(password))
    } else {
        text
    };
    if kind == "daemon.update" {
        match output.code {
            Some(3) => {
                let detail = if text.is_empty() {
                    "配置未通过 dockerd 校验".to_string()
                } else {
                    redact_text(&text)
                };
                SshError::daemon_invalid(format!("daemon.json 校验失败：{detail}"))
            }
            Some(4) => SshError::daemon_invalid("新配置导致 Docker 启动失败，已恢复原配置"),
            _ => SshError::docker_failed(if text.is_empty() {
                "写入 daemon.json 失败".to_string()
            } else {
                text
            }),
        }
    } else if text.is_empty() {
        SshError::docker_failed("Docker 服务操作失败")
    } else {
        SshError::docker_failed(text)
    }
}

pub async fn verify_sudo(
    handle: &Handle<ClientHandler>,
    password: Option<&[u8]>,
) -> Result<(), SshError> {
    let output = run_sudo(handle, "true", password, Duration::from_secs(20)).await?;
    if output.code == Some(0) {
        return Ok(());
    }
    if let Some(error) = auth_error(&output) {
        return Err(error);
    }
    let text = sanitize_output(&output.stderr_text(), password_str(password));
    Err(SshError::sudo_failed(if text.is_empty() {
        "系统授权失败".to_string()
    } else {
        text
    }))
}

/// 读取 daemon.json：先普通读取，被权限拒绝时再用 sudo；
/// 文件不存在返回 `None`，调用方按"将新建"处理。
pub async fn read_daemon(
    handle: &Handle<ClientHandler>,
    password: Option<&[u8]>,
) -> Result<Option<String>, SshError> {
    let script = format!(
        "if [ -f {path} ]; then cat {path}; else printf '__LG_MISSING__'; fi",
        path = shell_quote(DAEMON_PATH)
    );
    let output = exec::run(
        handle,
        &script,
        ExecOptions {
            timeout: Duration::from_secs(15),
            max_bytes: 256 * 1024,
            stdin: None,
            cancel: None,
        },
        |_| {},
    )
    .await?;
    if output.code == Some(0) {
        return Ok(daemon_text(&output.stdout_text()));
    }
    let output = run_sudo(handle, &script, password, Duration::from_secs(20)).await?;
    if let Some(error) = auth_error(&output) {
        return Err(error);
    }
    if output.code != Some(0) {
        return Err(SshError::sudo_failed("无法读取 daemon.json"));
    }
    Ok(daemon_text(&output.stdout_text()))
}

fn daemon_text(stdout: &str) -> Option<String> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() || trimmed == "__LG_MISSING__" {
        None
    } else {
        Some(stdout.to_string())
    }
}

pub async fn registry_config(
    handle: &Handle<ClientHandler>,
    password: Option<&[u8]>,
) -> Result<RegistryConfig, SshError> {
    let capabilities = capability::detect(handle).await?;
    match read_daemon(handle, password).await {
        Ok(text) => Ok(parse_registry(
            text.as_deref(),
            capabilities.proxy_supported,
        )),
        Err(error) if error.code == "SUDO_REQUIRED" || error.code == "SUDO_FAILED" => {
            Ok(RegistryConfig {
                mirrors: Vec::new(),
                http_proxy: String::new(),
                https_proxy: String::new(),
                no_proxy: String::new(),
                proxy_configured: false,
                proxy_supported: capabilities.proxy_supported,
                readable: false,
                invalid: false,
                note: error.message,
            })
        }
        Err(error) => Err(error),
    }
}

/// 读回来的代理值只在没有凭据时回显；含凭据时留空，保存时空白即保持原值。
pub fn parse_registry(text: Option<&str>, proxy_supported: bool) -> RegistryConfig {
    let Some(text) = text else {
        return RegistryConfig {
            mirrors: Vec::new(),
            http_proxy: String::new(),
            https_proxy: String::new(),
            no_proxy: String::new(),
            proxy_configured: false,
            proxy_supported,
            readable: true,
            invalid: false,
            note: "未找到 daemon.json，保存时会新建。".to_string(),
        };
    };
    let trimmed = text.trim();
    let value: serde_json::Value = if trimmed.is_empty() {
        serde_json::Value::Object(Default::default())
    } else {
        match serde_json::from_str(trimmed) {
            Ok(value @ serde_json::Value::Object(_)) => value,
            _ => {
                return RegistryConfig {
                    mirrors: Vec::new(),
                    http_proxy: String::new(),
                    https_proxy: String::new(),
                    no_proxy: String::new(),
                    proxy_configured: false,
                    proxy_supported,
                    readable: false,
                    invalid: true,
                    note: "daemon.json 不是合法的 JSON 对象，请先在服务器上修正。".to_string(),
                }
            }
        }
    };
    let mirrors = value
        .get("registry-mirrors")
        .and_then(|mirrors| mirrors.as_array())
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let proxies = value.get("proxies").and_then(|value| value.as_object());
    let mut hidden = false;
    let mut display = |key: &str| -> String {
        let raw = proxies
            .and_then(|proxies| proxies.get(key))
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if raw.is_empty() {
            return String::new();
        }
        let redacted = redact_url(raw);
        if redacted == raw {
            raw.to_string()
        } else {
            hidden = true;
            String::new()
        }
    };
    let http_proxy = display("http-proxy");
    let https_proxy = display("https-proxy");
    let no_proxy = display("no-proxy");
    let proxy_configured = proxies.map(|proxies| !proxies.is_empty()).unwrap_or(false);
    RegistryConfig {
        mirrors,
        http_proxy,
        https_proxy,
        no_proxy,
        proxy_configured,
        proxy_supported,
        readable: true,
        invalid: false,
        note: if hidden {
            "代理地址包含凭据，已隐藏；留空保持不变。".to_string()
        } else {
            String::new()
        },
    }
}

pub async fn preview(
    handle: &Handle<ClientHandler>,
    server_id: &str,
    input: &DockerSystemInput,
    password: Option<&[u8]>,
) -> Result<(SystemPayload, PreviewView), SshError> {
    match input.kind.as_str() {
        "service.start" | "service.stop" => {
            let capabilities = capability::detect(handle).await?;
            if !capabilities.installed {
                return Err(SshError::docker_unavailable("未安装 Docker"));
            }
            let active = service_active(handle).await?;
            let start = input.kind == "service.start";
            if start == active {
                return Err(SshError::docker_failed(if start {
                    "Docker 服务已在运行"
                } else {
                    "Docker 服务未运行"
                }));
            }
            let payload = SystemPayload {
                server_id: server_id.to_string(),
                kind: input.kind.clone(),
                audit_op: audit_op(&input.kind),
                display: "Docker 服务".to_string(),
                changed: Vec::new(),
                request: serde_json::json!({ "active": active }),
            };
            let view = PreviewView {
                operation: verb_for(&input.kind).to_string(),
                target: "Docker 服务".to_string(),
                impact: if start {
                    "将启动 Docker 守护进程，容器按重启策略恢复运行。".to_string()
                } else {
                    "将停止 Docker 守护进程，所有容器随即停止；服务恢复后按重启策略启动。"
                        .to_string()
                },
                rollback:
                    "不可回退：系统级操作不在审计回退范围，如需恢复请再次预览并执行相反操作。"
                        .to_string(),
                irreversible: false,
                details: vec![
                    format!("当前状态：{}", if active { "运行中" } else { "已停止" }),
                    "需要服务器账号的 sudo 权限。".to_string(),
                ],
            };
            Ok((payload, view))
        }
        "daemon.update" => {
            let daemon_payload: DaemonPayload = serde_json::from_value(input.payload.clone())
                .map_err(|error| SshError::daemon_invalid(format!("参数格式错误：{error}")))?;
            let current = read_daemon(handle, password).await?;
            let merged = daemon::merge(current.as_deref().unwrap_or(""), &daemon_payload)?;
            if merged.changed.is_empty() {
                return Err(SshError::docker_failed("配置没有变化"));
            }
            if merged.proxy_changed {
                let capabilities = capability::detect(handle).await?;
                if !capabilities.proxy_supported {
                    return Err(SshError::docker_failed(
                        "当前 Docker 版本不支持代理配置（需要 23 及以上）",
                    ));
                }
            }
            let mut details = daemon::preview_lines(&merged);
            if current.is_none() {
                details.insert(0, "未找到 daemon.json，将新建。".to_string());
            }
            let payload = SystemPayload {
                server_id: server_id.to_string(),
                kind: input.kind.clone(),
                audit_op: audit_op(&input.kind),
                display: DAEMON_PATH.to_string(),
                changed: merged.changed.clone(),
                request: input.payload.clone(),
            };
            let view = PreviewView {
                operation: verb_for(&input.kind).to_string(),
                target: DAEMON_PATH.to_string(),
                impact: "将原子替换 daemon.json 并重启 Docker 服务；启动失败会自动恢复原配置。"
                    .to_string(),
                rollback: "不可回退：变更前自动备份，启动失败自动恢复。".to_string(),
                irreversible: true,
                details,
            };
            Ok((payload, view))
        }
        _ => Err(SshError::invalid_input("不支持的系统操作")),
    }
}

struct Plan {
    script: String,
    backup: Option<String>,
}

/// 预览时的决定在执行前重新核对；这一步失败说明什么都没做，不落审计。
async fn prepare(
    handle: &Handle<ClientHandler>,
    payload: &SystemPayload,
    password: Option<&[u8]>,
    on_line: &mut (dyn FnMut(&str) + Send),
) -> Result<Plan, SshError> {
    match payload.kind.as_str() {
        "service.start" | "service.stop" => {
            let expected = payload
                .request
                .get("active")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            if service_active(handle).await? != expected {
                return Err(SshError::docker_failed("服务状态已变化，请重新发起预览"));
            }
            on_line("正在请求系统授权…");
            Ok(Plan {
                script: service_action(if payload.kind == "service.start" {
                    "start"
                } else {
                    "stop"
                }),
                backup: None,
            })
        }
        "daemon.update" => {
            let daemon_payload: DaemonPayload = serde_json::from_value(payload.request.clone())
                .map_err(|error| SshError::daemon_invalid(format!("参数格式错误：{error}")))?;
            let current = read_daemon(handle, password).await?;
            let merged = daemon::merge(current.as_deref().unwrap_or(""), &daemon_payload)?;
            if merged.changed.is_empty() {
                return Err(SshError::docker_failed("配置没有变化"));
            }
            let backup = format!("{DAEMON_PATH}.lg-{}.bak", crate::alerts::now_ms());
            on_line("正在校验并写入 daemon.json…");
            Ok(Plan {
                script: apply_script(&merged.rendered, &backup),
                backup: Some(backup),
            })
        }
        _ => Err(SshError::invalid_input("不支持的系统操作")),
    }
}

async fn finish(
    handle: &Handle<ClientHandler>,
    payload: &SystemPayload,
    plan: &Plan,
    password: Option<&[u8]>,
    on_line: &mut (dyn FnMut(&str) + Send),
) -> Result<(), SshError> {
    match payload.kind.as_str() {
        "service.start" => {
            on_line("正在等待 Docker 服务就绪…");
            if wait_health(handle, Duration::from_secs(30)).await {
                Ok(())
            } else {
                Err(SshError::docker_failed("Docker 服务启动后未能就绪"))
            }
        }
        "service.stop" => {
            on_line("正在等待 Docker 服务停止…");
            if wait_service(handle, false, Duration::from_secs(20)).await {
                Ok(())
            } else {
                Err(SshError::docker_failed("Docker 服务未能停止"))
            }
        }
        "daemon.update" => {
            on_line("正在等待 Docker 服务重启…");
            if wait_health(handle, Duration::from_secs(30)).await {
                return Ok(());
            }
            let confirmed = if let Some(backup) = plan.backup.as_deref() {
                on_line("新配置未能启动 Docker，正在恢复原配置…");
                restore(handle, backup, password).await
            } else {
                false
            };
            Err(SshError::daemon_invalid(restore_failure_message(confirmed)))
        }
        _ => Ok(()),
    }
}

pub async fn wait_health(handle: &Handle<ClientHandler>, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let probe = exec::run(
            handle,
            "docker info --format '{{.ServerVersion}}'",
            ExecOptions {
                timeout: Duration::from_secs(5),
                max_bytes: 8 * 1024,
                stdin: None,
                cancel: None,
            },
            |_| {},
        )
        .await;
        if matches!(probe, Ok(output) if output.code == Some(0)) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    false
}

async fn wait_service(handle: &Handle<ClientHandler>, expected: bool, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if service_active(handle).await.unwrap_or(!expected) == expected {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    false
}

/// 只有回滚脚本成功退出、且回滚后的健康检查通过，才算"已恢复"；
/// 否则调用方必须如实说明恢复未获确认。
async fn restore(handle: &Handle<ClientHandler>, backup: &str, password: Option<&[u8]>) -> bool {
    let output = run_sudo(
        handle,
        &restore_script(backup),
        password,
        Duration::from_secs(120),
    )
    .await;
    if !matches!(output, Ok(ref output) if output.code == Some(0)) {
        return false;
    }
    wait_health(handle, Duration::from_secs(30)).await
}

/// 回滚结果决定错误文案：未确认时不能说"已恢复"。
pub fn restore_failure_message(confirmed: bool) -> &'static str {
    if confirmed {
        "新配置未能启动 Docker，已恢复原配置"
    } else {
        "新配置未能启动 Docker，恢复原配置未获确认；请手动检查 /etc/docker/daemon.json 与 Docker 服务状态"
    }
}

/// 命令返回后的结果分类：`Some(0)` 成功；用户取消后没有退出码仍算失败；
/// 其余没有退出码的情况说明连接在收尾前断开，命令可能已经执行，只能算未知。
pub fn outcome_kind(code: Option<u32>, canceled: bool) -> &'static str {
    match code {
        Some(0) => "success",
        None if !canceled => "unknown",
        _ => "failed",
    }
}

pub(crate) async fn execute(
    state: &AppState,
    db: &Database,
    payload: &SystemPayload,
    password: Option<&[u8]>,
    permit_source: &str,
    on_line: &mut (dyn FnMut(&str) + Send),
) -> Result<DockerWriteResult, SshError> {
    let started = Instant::now();
    let handle = client::session_handle(state, &payload.server_id).await?;
    let plan = prepare(&handle, payload, password, on_line).await?;
    let verb = verb_for(&payload.kind);
    // 连接在命令收尾前断开时命令可能已经跑完，按"结果未知"记账。
    let mut unknown = false;
    let outcome = match run_sudo(
        &handle,
        &plan.script,
        password,
        if payload.kind == "daemon.update" {
            Duration::from_secs(180)
        } else {
            Duration::from_secs(120)
        },
    )
    .await
    {
        Ok(output) => {
            // 鉴权失败说明命令没有执行，返回错误且不落审计。
            if let Some(error) = auth_error(&output) {
                return Err(error);
            }
            match outcome_kind(output.code, output.canceled) {
                "success" => finish(&handle, payload, &plan, password, on_line).await,
                "unknown" => {
                    unknown = true;
                    Err(SshError::docker_failed("连接在命令收尾前断开，结果未知"))
                }
                _ => Err(failure_for(&payload.kind, &output, password)),
            }
        }
        // 连接中断发生在执行途中：按"结果未知"落审计。
        Err(error) => Err(error),
    };
    let (result, message) = match outcome {
        Ok(()) => ("success", format!("{verb}成功")),
        Err(error) if unknown => ("unknown", error.message),
        Err(error) => (outcome_for(&error), error.message),
    };
    let server_name = db
        .server_name(&payload.server_id)
        .unwrap_or_else(|_| payload.server_id.clone());
    let params = audit_params(payload);
    let audit_id = record(
        db,
        &payload.server_id,
        &server_name,
        &payload.audit_op,
        &params,
        result,
        started,
        permit_source,
        None,
    )
    .map_err(|audit| {
        if result == "success" {
            audit_lost_after_success(verb, &audit)
        } else {
            audit_write_failed(
                SshError::docker_failed(message.clone()),
                &audit,
                result == "unknown",
            )
        }
    })?;
    Ok(DockerWriteResult {
        audit_id,
        target: payload.display.clone(),
        result: result.to_string(),
        message,
    })
}

pub fn audit_params(payload: &SystemPayload) -> String {
    match payload.kind.as_str() {
        "service.start" => {
            serde_json::json!({ "action": "start", "service": "docker" }).to_string()
        }
        "service.stop" => serde_json::json!({ "action": "stop", "service": "docker" }).to_string(),
        _ => serde_json::json!({ "file": DAEMON_PATH, "changed": payload.changed }).to_string(),
    }
}

/// daemon.json 通过 base64 传输，避免 JSON 里的引号与换行破坏脚本。
pub fn base64(input: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

/// 写入流程：备份 → 临时文件 → dockerd 校验 → 原子替换 → 重启。
/// 重启失败时立刻恢复备份并再次重启；成功后只保留最近三份备份。
/// 校验日志与配置临时文件都用 mktemp，避免可预测的固定路径。
pub fn apply_script(rendered: &str, backup: &str) -> String {
    format!(
        r#"set -u
DAEMON={daemon}
BACKUP={backup}
mkdir -p /etc/docker || exit 2
HAD=0
if [ -f "$DAEMON" ]; then cp -p "$DAEMON" "$BACKUP" || exit 2; HAD=1; fi
TMP=$(mktemp /etc/docker/.daemon.json.XXXXXX) || exit 2
LOG=$(mktemp /tmp/lightguard-dockerd.XXXXXX) || exit 2
trap 'rm -f "$TMP" "$LOG"' EXIT
printf '%s' '{encoded}' | base64 -d > "$TMP" || exit 2
if command -v dockerd >/dev/null 2>&1; then
  if dockerd --validate --config-file "$TMP" > "$LOG" 2>&1; then
    :
  elif grep -q 'unknown flag' "$LOG" 2>/dev/null; then
    :
  else
    cat "$LOG" >&2
    exit 3
  fi
fi
rm -f "$LOG"
chmod 644 "$TMP"
mv -f "$TMP" "$DAEMON" || exit 2
trap - EXIT
if command -v systemctl >/dev/null 2>&1; then
  systemctl restart docker
else
  service docker restart
fi
if [ "$?" = 0 ]; then
  ls -1t /etc/docker/daemon.json.lg-*.bak 2>/dev/null | tail -n +4 | xargs -r rm -f
  echo 'LG_RESTARTED'
  exit 0
fi
if [ "$HAD" = 1 ]; then cp -p "$BACKUP" "$DAEMON"; else rm -f "$DAEMON"; fi
if command -v systemctl >/dev/null 2>&1; then
  systemctl restart docker >/dev/null 2>&1
else
  service docker restart >/dev/null 2>&1
fi
echo 'LG_RESTORED' >&2
exit 4
"#,
        daemon = shell_quote(DAEMON_PATH),
        backup = shell_quote(backup),
        encoded = base64(rendered),
    )
}

pub fn restore_script(backup: &str) -> String {
    format!(
        r#"set -u
DAEMON={daemon}
BACKUP={backup}
if [ -f "$BACKUP" ]; then cp -p "$BACKUP" "$DAEMON" || exit 2; else rm -f "$DAEMON"; fi
if command -v systemctl >/dev/null 2>&1; then
  systemctl restart docker
else
  service docker restart
fi
"#,
        daemon = shell_quote(DAEMON_PATH),
        backup = shell_quote(backup),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(stdout: &str, stderr: &str, code: Option<u32>) -> ExecOutput {
        ExecOutput {
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
            code,
            truncated: false,
            canceled: false,
        }
    }

    #[test]
    fn service_and_daemon_scripts_are_fixed() {
        let start = service_action("start");
        assert!(start.contains("systemctl start docker"));
        assert!(start.contains("service docker start"));
        let stop = service_action("stop");
        assert!(stop.contains("systemctl stop docker"));
        assert!(stop.contains("service docker stop"));
        let script = apply_script("{}\n", "/etc/docker/daemon.json.lg-1.bak");
        assert!(script.contains("dockerd --validate"));
        assert!(script.contains("mv -f \"$TMP\" \"$DAEMON\""));
        assert!(script.contains("systemctl restart docker"));
        assert!(script.contains("service docker restart"));
        assert!(script.contains("cp -p \"$BACKUP\" \"$DAEMON\""));
        assert!(script.contains("| base64 -d > \"$TMP\""));
        assert!(script.contains("mktemp"));
        assert!(!script.contains("/tmp/lightguard-dockerd.log"));
        assert!(
            restore_script("/etc/docker/daemon.json.lg-1.bak").contains("service docker restart")
        );
    }

    #[test]
    fn daemon_restore_is_only_claimed_when_confirmed() {
        let confirmed = restore_failure_message(true);
        assert!(confirmed.contains("已恢复"));
        let unconfirmed = restore_failure_message(false);
        assert!(!unconfirmed.contains("已恢复"));
        assert!(unconfirmed.contains("未获确认"));
        assert!(unconfirmed.contains("/etc/docker/daemon.json"));
        assert!(unconfirmed.contains("Docker 服务状态"));
    }

    #[test]
    fn missing_exit_code_is_unknown_unless_canceled() {
        assert_eq!(outcome_kind(Some(0), false), "success");
        assert_eq!(outcome_kind(None, false), "unknown");
        assert_eq!(outcome_kind(None, true), "failed");
        assert_eq!(outcome_kind(Some(1), false), "failed");
        assert_eq!(outcome_kind(Some(3), false), "failed");
    }

    #[test]
    fn daemon_validation_errors_redact_url_credentials() {
        let error = failure_for(
            "daemon.update",
            &output("", "dockerd: invalid proxy http://u:p@proxy:3128", Some(3)),
            None,
        );
        assert!(error.message.contains("http://***@proxy:3128"));
        assert!(!error.message.contains("u:p@"));

        let path = failure_for(
            "daemon.update",
            &output(
                "",
                "unable to configure the Docker daemon with file /etc/docker/daemon.json",
                Some(3),
            ),
            None,
        );
        assert!(path.message.contains("/etc/docker/daemon.json"));

        assert_eq!(
            redact_text("a (\"https://u:p@mirror.example.com/x\"), b"),
            "a (\"https://***@mirror.example.com/x\"), b"
        );
        assert_eq!(redact_text("plain output"), "plain output");
    }

    #[test]
    fn base64_matches_the_standard_alphabet() {
        assert_eq!(base64(""), "");
        assert_eq!(base64("{}"), "e30=");
        assert_eq!(base64("daemon"), "ZGFlbW9u");
        assert_eq!(base64("汉"), "5rGJ");
    }

    #[test]
    fn auth_errors_are_recognized_without_leaking_the_password() {
        let wrong = output("", "[sudo] password for admin: Sorry, try again.", Some(1));
        let error = auth_error(&wrong).unwrap();
        assert_eq!(error.code, "SUDO_FAILED");
        assert!(!error.message.contains("s3cret"));

        let missing = output("", "sudo: a password is required", Some(1));
        assert_eq!(auth_error(&missing).unwrap().code, "SUDO_REQUIRED");

        let denied = output("", "admin is not in the sudoers file.", Some(1));
        assert_eq!(auth_error(&denied).unwrap().code, "SUDO_FAILED");

        assert!(auth_error(&output("", "", Some(0))).is_none());
        assert!(auth_error(&output("", "Failed to start docker.service", Some(1))).is_none());
    }

    #[test]
    fn daemon_failures_map_to_config_errors() {
        let invalid = output(
            "",
            "unable to configure the Docker daemon with file /etc/docker/daemon.json",
            Some(3),
        );
        assert_eq!(
            failure_for("daemon.update", &invalid, None).code,
            "DAEMON_INVALID"
        );

        let restored = output("", "LG_RESTORED", Some(4));
        let error = failure_for("daemon.update", &restored, None);
        assert_eq!(error.code, "DAEMON_INVALID");
        assert!(error.message.contains("已恢复"));

        let service = output("", "Failed to start docker.service", Some(1));
        assert_eq!(
            failure_for("service.start", &service, None).code,
            "DOCKER_FAILED"
        );
    }

    #[test]
    fn registry_config_never_returns_proxy_credentials() {
        let config = parse_registry(
            Some(
                r#"{"registry-mirrors":["https://mirror.example.com"],"proxies":{"http-proxy":"http://u:p@proxy:3128","no-proxy":"localhost"}}"#,
            ),
            true,
        );
        assert_eq!(config.mirrors, vec!["https://mirror.example.com"]);
        assert_eq!(config.http_proxy, "");
        assert!(config.proxy_configured);
        assert!(config.note.contains("隐藏"));
        assert_eq!(config.no_proxy, "localhost");
        assert!(config.proxy_supported);
        assert!(config.readable);
        assert!(!config.invalid);
    }

    #[test]
    fn registry_config_marks_missing_and_broken_files() {
        let missing = parse_registry(None, false);
        assert!(missing.readable);
        assert!(!missing.invalid);
        assert!(missing.note.contains("未找到"));

        let broken = parse_registry(Some("[1]"), true);
        assert!(!broken.readable);
        assert!(broken.invalid);
        assert!(broken.note.contains("JSON"));
    }

    #[test]
    fn audit_params_never_include_proxy_values() {
        let payload = SystemPayload {
            server_id: "srv".to_string(),
            kind: "daemon.update".to_string(),
            audit_op: "docker.daemon.update".to_string(),
            display: "/etc/docker/daemon.json".to_string(),
            changed: vec!["proxies.http-proxy".to_string()],
            request: serde_json::json!({ "httpProxy": "http://u:p@proxy:3128" }),
        };
        let params = audit_params(&payload);
        assert!(!params.contains("u:p"));
        assert!(!params.contains("proxy:3128"));
        assert!(params.contains("proxies.http-proxy"));
    }

    #[test]
    fn probe_lines_map_to_sudo_status() {
        assert_eq!(
            parse_probe("LG_SUDO=passwordless\n"),
            SudoStatus {
                passwordless: true,
                available: true
            }
        );
        assert_eq!(
            parse_probe("LG_SUDO=password\n"),
            SudoStatus {
                passwordless: false,
                available: true
            }
        );
        assert_eq!(
            parse_probe("LG_SUDO=none\n"),
            SudoStatus {
                passwordless: false,
                available: false
            }
        );
        assert_eq!(
            parse_probe(""),
            SudoStatus {
                passwordless: false,
                available: false
            }
        );
    }

    #[test]
    fn service_state_is_parsed() {
        assert!(parse_service_state("active\n"));
        assert!(!parse_service_state("inactive\n"));
        assert!(!parse_service_state("failed\n"));
        assert!(!parse_service_state(""));
    }
}

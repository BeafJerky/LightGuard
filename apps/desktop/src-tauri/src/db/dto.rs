use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::DbError;

pub const MAX_SERVERS: usize = 500;
pub const MAX_LOGS: usize = 80;
const MAX_JSON_BYTES: usize = 64 * 1024;
const MAX_TAGS: usize = 24;
const AUTH_KINDS: [&str; 3] = ["password", "key", "agent"];
const LOG_STATUSES: [&str; 3] = ["success", "warning", "failed"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedServer {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub role: String,
    pub ip: String,
    pub port: u16,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub note: String,
    pub user: String,
    #[serde(default = "default_auth")]
    pub auth: String,
    #[serde(default)]
    pub auth_label: String,
    #[serde(default)]
    pub fingerprint: String,
    #[serde(default)]
    pub last_sync: String,
    #[serde(default)]
    pub last_seen: String,
    #[serde(default)]
    pub cpu_cores: u32,
    #[serde(default)]
    pub mem_total: f64,
    #[serde(default)]
    pub disk_total: f64,
    #[serde(default)]
    pub os: String,
    #[serde(default)]
    pub kernel: String,
    #[serde(default)]
    pub arch: String,
    #[serde(default)]
    pub cpu_model: String,
    #[serde(default)]
    pub cpu_freq: String,
    #[serde(default)]
    pub timezone: String,
}

fn default_auth() -> String {
    "key".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: String,
    pub ts: i64,
    pub server_id: String,
    #[serde(default)]
    pub server_name: String,
    #[serde(default)]
    pub ip: String,
    pub port: u16,
    pub action: String,
    pub status: String,
    #[serde(default)]
    pub duration: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub auth: String,
    #[serde(default)]
    pub client: String,
    #[serde(default)]
    pub session: String,
    #[serde(default)]
    pub fingerprint: String,
    #[serde(default)]
    pub hint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeChoices {
    pub console: String,
    pub desktop: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredState {
    #[serde(default)]
    pub settings: Option<Value>,
    #[serde(default)]
    pub themes: Option<ThemeChoices>,
    #[serde(default)]
    pub servers: Vec<PersistedServer>,
    #[serde(default)]
    pub logs: Vec<LogEntry>,
    #[serde(default)]
    pub backup: Option<Value>,
    #[serde(default)]
    pub render: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadResult {
    pub initialized: bool,
    pub themes: Option<ThemeChoices>,
    pub settings: Option<Value>,
    pub servers: Vec<PersistedServer>,
    pub logs: Vec<LogEntry>,
    pub backup: Option<Value>,
    pub render: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum DbOp {
    ServerUpsert { server: PersistedServer },
    ServerDelete { id: String },
    LogAppend { log: LogEntry },
    LogsClear,
    ThemesSet { themes: ThemeChoices },
    SettingsSet { settings: Value },
    BackupSet { summary: Option<Value> },
    RenderSet { settings: Value },
    Replace { snapshot: StoredState },
    Reset,
}

fn check_len(value: &str, label: &str, max: usize) -> Result<(), DbError> {
    if value.chars().count() > max {
        return Err(DbError::invalid_input(format!("{label}超出长度限制")));
    }
    Ok(())
}

fn require_text(value: &str, label: &str, max: usize) -> Result<(), DbError> {
    if value.trim().is_empty() {
        return Err(DbError::invalid_input(format!("{label}不能为空")));
    }
    check_len(value, label, max)
}

fn validate_json(value: &Value, label: &str) -> Result<(), DbError> {
    let size = serde_json::to_string(value)
        .map_err(|error| DbError::invalid_input(error.to_string()))?
        .len();
    if size > MAX_JSON_BYTES {
        return Err(DbError::invalid_input(format!("{label}数据过大")));
    }
    Ok(())
}

pub fn validate_server(server: &PersistedServer) -> Result<(), DbError> {
    if server.id.is_empty()
        || server.id.len() > 64
        || !server
            .id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(DbError::invalid_input("服务器标识无效"));
    }
    require_text(&server.name, "服务器名称", 80)?;
    require_text(&server.ip, "主机地址", 253)?;
    require_text(&server.user, "登录用户", 64)?;
    if server.port == 0 {
        return Err(DbError::invalid_input("端口必须在 1–65535 之间"));
    }
    if !AUTH_KINDS.contains(&server.auth.as_str()) {
        return Err(DbError::invalid_input("认证方式无效"));
    }
    if server.tags.len() > MAX_TAGS || server.tags.iter().any(|tag| tag.chars().count() > 40) {
        return Err(DbError::invalid_input("标签数量或长度超出限制"));
    }
    if !server.mem_total.is_finite() || server.mem_total < 0.0 {
        return Err(DbError::invalid_input("内存容量无效"));
    }
    if !server.disk_total.is_finite() || server.disk_total < 0.0 {
        return Err(DbError::invalid_input("磁盘容量无效"));
    }
    check_len(&server.hostname, "主机名", 253)?;
    check_len(&server.role, "角色", 80)?;
    check_len(&server.region, "区域", 64)?;
    check_len(&server.group, "分组", 40)?;
    check_len(&server.note, "备注", 4000)?;
    check_len(&server.auth_label, "认证标签", 32)?;
    check_len(&server.fingerprint, "主机指纹", 256)?;
    check_len(&server.last_sync, "同步时间", 64)?;
    check_len(&server.last_seen, "在线时间", 64)?;
    check_len(&server.os, "系统名称", 128)?;
    check_len(&server.kernel, "内核版本", 128)?;
    check_len(&server.arch, "架构", 64)?;
    check_len(&server.cpu_model, "处理器型号", 200)?;
    check_len(&server.cpu_freq, "处理器频率", 64)?;
    check_len(&server.timezone, "时区", 64)?;
    Ok(())
}

pub fn validate_log(log: &LogEntry) -> Result<(), DbError> {
    require_text(&log.id, "记录标识", 80)?;
    require_text(&log.action, "记录动作", 120)?;
    if log.ts < 0 {
        return Err(DbError::invalid_input("记录时间无效"));
    }
    if log.port == 0 {
        return Err(DbError::invalid_input("记录端口无效"));
    }
    if !LOG_STATUSES.contains(&log.status.as_str()) {
        return Err(DbError::invalid_input("记录状态无效"));
    }
    check_len(&log.server_id, "服务器标识", 80)?;
    check_len(&log.server_name, "服务器名称", 80)?;
    check_len(&log.ip, "主机地址", 253)?;
    check_len(&log.duration, "耗时", 32)?;
    check_len(&log.source, "来源", 32)?;
    check_len(&log.code, "结果代码", 64)?;
    check_len(&log.auth, "认证方式", 32)?;
    check_len(&log.client, "客户端", 128)?;
    check_len(&log.session, "会话标识", 80)?;
    check_len(&log.fingerprint, "主机指纹", 256)?;
    check_len(&log.hint, "提示", 2000)?;
    Ok(())
}

pub fn validate_themes(themes: &ThemeChoices) -> Result<(), DbError> {
    require_text(&themes.console, "控制台主题", 32)?;
    require_text(&themes.desktop, "桌面主题", 32)?;
    Ok(())
}

pub fn validate_settings(settings: &Value) -> Result<(), DbError> {
    if !settings.is_object() {
        return Err(DbError::invalid_input("设置数据格式无效"));
    }
    validate_json(settings, "设置")
}

pub fn validate_render(settings: &Value) -> Result<(), DbError> {
    if !settings.is_object() {
        return Err(DbError::invalid_input("渲染设置格式无效"));
    }
    validate_json(settings, "渲染设置")
}

pub fn validate_backup(summary: &Value) -> Result<(), DbError> {
    if !summary.is_object() {
        return Err(DbError::invalid_input("备份摘要格式无效"));
    }
    validate_json(summary, "备份摘要")
}

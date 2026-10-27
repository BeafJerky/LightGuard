use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthKind {
    Password,
    Key,
}

impl AuthKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Password => "密码",
            Self::Key => "SSH 私钥",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostKeyState {
    New,
    Known,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub mount: String,
    pub fs: String,
    pub kind: String,
    pub total_gb: f64,
    pub used_gb: f64,
    pub inode_percent: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InterfaceInfo {
    pub name: String,
    pub ip: String,
    pub mac: String,
    pub speed: String,
    pub rx_total_gb: f64,
    pub tx_total_gb: f64,
    pub down_mbps: f64,
    pub up_mbps: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub user: String,
    pub cpu: f64,
    pub mem: f64,
    pub state: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptionInfo {
    pub cipher: String,
    pub kex: String,
    pub mac: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSnapshot {
    pub hostname: String,
    pub os: String,
    pub kernel: String,
    pub arch: String,
    pub timezone: String,
    pub uptime_seconds: u64,
    pub boot_time: String,
    pub cpu_model: String,
    pub cpu_cores: u32,
    pub cpu_freq: String,
    pub cpu_percent: f64,
    pub cpu_cores_load: Vec<f64>,
    pub load: [f64; 3],
    pub temp_c: f64,
    pub mem_total_gb: f64,
    pub mem_available_gb: f64,
    pub mem_percent: f64,
    pub swap_total_gb: f64,
    pub swap_used_gb: f64,
    pub disk_total_gb: f64,
    pub disk_percent: f64,
    pub disks: Vec<DiskInfo>,
    pub down_mbps: f64,
    pub up_mbps: f64,
    pub interfaces: Vec<InterfaceInfo>,
    pub gateway: String,
    pub dns: String,
    pub public_ip: String,
    pub mtu: String,
    pub processes: Vec<ProcessInfo>,
    pub process_count: u32,
    pub sampled_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectInput {
    pub server_id: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth: AuthKind,
    pub secret: String,
    #[serde(default)]
    pub passphrase: Option<String>,
    #[serde(default)]
    pub remember: bool,
    #[serde(default = "default_true")]
    pub verify_host_key: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectInput {
    pub server_id: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    #[serde(default = "default_true")]
    pub verify_host_key: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectResult {
    pub session_id: String,
    pub started_at: String,
    pub fingerprint: String,
    pub host_key_state: HostKeyState,
    pub auth_label: String,
    pub latency_ms: u64,
    pub snapshot: ServerSnapshot,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encryption: Option<EncryptionInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatus {
    pub saved: bool,
    pub auth: Option<AuthKind>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    pub connected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOpenInput {
    pub server_id: String,
    pub cols: u32,
    pub rows: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalHandle {
    pub terminal_id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TerminalEvent {
    Data { data: String },
    Title { title: String },
    Exit { code: Option<i32> },
    Closed { reason: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEvent {
    pub server_id: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

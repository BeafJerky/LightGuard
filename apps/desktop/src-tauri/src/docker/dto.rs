use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerCapabilities {
    pub installed: bool,
    pub daemon_running: bool,
    pub socket_ok: bool,
    pub systemd: bool,
    /// `systemctl is-enabled docker` 的原始输出；systemctl 不可用时为空串。
    pub auto_start: String,
    pub compose: bool,
    pub version: String,
    pub api_version: String,
    pub proxy_supported: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerInfo {
    pub version: String,
    pub api_version: String,
    pub storage_driver: String,
    pub data_root: String,
    pub mirrors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DockerCounts {
    pub containers_running: i64,
    pub containers_total: i64,
    pub images: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerDf {
    pub images_size: String,
    pub containers_size: String,
    pub volumes_size: String,
    pub build_cache_size: String,
    pub reclaimable: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerOverview {
    pub capabilities: DockerCapabilities,
    pub info: DockerInfo,
    pub counts: DockerCounts,
    pub df: Option<DockerDf>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DockerContainer {
    pub id: String,
    pub short_id: String,
    pub name: String,
    pub image: String,
    pub command: String,
    pub state: String,
    pub status: String,
    pub created_at: String,
    pub project: String,
    pub service: String,
    pub ports: String,
    pub cpu_percent: Option<f64>,
    pub mem_usage: String,
    pub mem_limit: String,
    pub mem_percent: Option<f64>,
    pub net_rx: String,
    pub net_tx: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerStatsSample {
    pub id: String,
    pub cpu_percent: f64,
    pub mem_usage: String,
    pub mem_limit: String,
    pub mem_percent: f64,
    pub net_rx: String,
    pub net_tx: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerPort {
    pub host_ip: String,
    pub host_port: String,
    pub container_port: String,
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerMount {
    pub source: String,
    pub destination: String,
    pub mode: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerNetwork {
    pub name: String,
    pub ip: String,
    pub gateway: String,
    pub mac: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerEnvVar {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerContainerDetail {
    pub id: String,
    pub name: String,
    pub image: String,
    pub command: String,
    pub state: String,
    pub status: String,
    pub health: String,
    pub restart_policy: String,
    pub created_at: String,
    pub started_at: String,
    pub ports: Vec<DockerPort>,
    pub mounts: Vec<DockerMount>,
    pub networks: Vec<DockerNetwork>,
    pub env: Vec<DockerEnvVar>,
    pub env_truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerImage {
    pub id: String,
    pub short_id: String,
    pub repo: String,
    pub tags: Vec<String>,
    pub size: String,
    pub created_at: String,
    pub containers: i64,
    pub dangling: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistryConfig {
    pub mirrors: Vec<String>,
    pub http_proxy: String,
    pub https_proxy: String,
    pub no_proxy: String,
    pub proxy_configured: bool,
    pub proxy_supported: bool,
    pub readable: bool,
    /// daemon.json 存在但不是合法 JSON 对象：前端禁止编辑，避免覆盖。
    pub invalid: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySearchItem {
    pub name: String,
    pub description: String,
    pub stars: i64,
    pub official: bool,
    pub automated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerPreview {
    pub preview_id: String,
    pub kind: String,
    pub operation: String,
    pub target: String,
    pub impact: String,
    pub rollback: String,
    pub irreversible: bool,
    pub expires_at_ms: i64,
    pub details: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerWriteResult {
    pub audit_id: i64,
    pub target: String,
    pub result: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerProgress {
    pub line: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SudoStatus {
    pub passwordless: bool,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerTerminalHandle {
    pub terminal_id: String,
    pub title: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerWriteInput {
    pub kind: String,
    pub target: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerSystemInput {
    pub kind: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DockerTerminalInput {
    pub server_id: String,
    pub container_id: String,
    pub cols: u32,
    pub rows: u32,
}

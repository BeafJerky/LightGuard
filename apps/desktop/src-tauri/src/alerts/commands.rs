use std::sync::Arc;

use tauri::State;

use super::{AlertService, AlertsPage};
use crate::ssh::error::SshError;

#[tauri::command]
pub async fn alerts_list(
    alerts: State<'_, Arc<AlertService>>,
    filter: String,
    limit: i64,
    offset: i64,
) -> Result<AlertsPage, SshError> {
    let alerts = Arc::clone(alerts.inner());
    tauri::async_runtime::spawn_blocking(move || alerts.list_alerts(&filter, limit, offset))
        .await
        .map_err(|error| SshError::internal(error.to_string()))?
}

#[tauri::command]
pub fn alerts_ignore(
    alerts: State<'_, Arc<AlertService>>,
    alert_id: i64,
    reason: String,
    until_ms: i64,
) -> Result<(), SshError> {
    alerts.ignore_alert(alert_id, &reason, until_ms)
}

#[tauri::command]
pub fn alerts_mute(
    alerts: State<'_, Arc<AlertService>>,
    server_id: String,
    rule_id: Option<String>,
    until_ms: i64,
) -> Result<(), SshError> {
    alerts.mute_server(&server_id, rule_id.as_deref(), until_ms)
}

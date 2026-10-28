//! 测试专用的 `AppState` 装配。
//!
//! 生产构建里这个模块不存在（`lib.rs` 用 `#[cfg(test)]` 声明它）。这里把告警与写
//! 许可服务接到"没有 Tauri 句柄、没有 SQLite 句柄"的形态，让命令层之外的用例也能
//! 拿到完整的 `AppState`，同时不碰磁盘、网络或应用清单。

use std::path::PathBuf;
use std::sync::Arc;

use crate::alerts::AlertService;
use crate::guard::GuardService;
use crate::ssh::state::AppState;

/// 测试用 `AppState`：服务永远不会发出事件，也不会读写数据库。
pub fn headless_state(config_dir: PathBuf) -> Arc<AppState> {
    let alerts = AlertService::new(None, None);
    let guard = GuardService::new(None);
    AppState::new(config_dir, alerts, guard)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::headless_state;
    use crate::alerts::AlertService;
    use crate::guard::{GuardService, PERMIT_EVENT, WRITE_EVENT};
    use crate::ssh::state::AppState;

    #[test]
    fn headless_state_wires_locked_guard_and_empty_alerts() {
        let dir = tempfile::tempdir().unwrap();
        let state = headless_state(dir.path().to_path_buf());
        assert!(state.guard.status("srv").locked);
        assert_eq!(state.alerts.active_count(), 0);
        assert_eq!(state.config_dir, dir.path());
    }

    /// 事件名是与前端约定的契约，改名必须让测试失败。
    #[test]
    fn guard_event_names_are_pinned() {
        assert_eq!(PERMIT_EVENT, "ssh://permit");
        assert_eq!(WRITE_EVENT, "guard://write");
    }

    /// `ssh://permit` 的载体就是 `PermitStatus` 本身：前端按 camelCase 读这些字段。
    #[test]
    fn permit_event_payload_has_documented_keys() {
        let dir = tempfile::tempdir().unwrap();
        let state = headless_state(dir.path().to_path_buf());
        state.guard.unlock("srv", 15).unwrap();
        let payload = serde_json::to_value(state.guard.status("srv")).unwrap();
        assert_eq!(payload["serverId"], "srv");
        assert_eq!(payload["locked"], false);
        assert_eq!(payload["minutes"], 15);
        assert_eq!(payload["writeActive"], false);
        assert!(payload["expiresAtMs"].as_i64().unwrap_or(0) > 0);
    }

    /// `guard://write` 的载荷由 `write_payload` 生成，写遮罩只认这两个字段。
    #[test]
    fn write_event_payload_has_documented_keys() {
        assert_eq!(
            crate::guard::write_payload("srv", true),
            serde_json::json!({ "serverId": "srv", "active": true })
        );
        assert_eq!(
            crate::guard::write_payload("srv", false),
            serde_json::json!({ "serverId": "srv", "active": false })
        );
    }

    /// `AppState` 持有的两个服务必须与命令层拿到的是同一实例，否则生命周期钩子
    /// （断线失效、清空归零）会作用在另一个副本上。
    #[test]
    fn headless_state_keeps_the_service_handles_it_was_given() {
        let dir = tempfile::tempdir().unwrap();
        let alerts = AlertService::new(None, None);
        let guard = GuardService::new(None);
        let state = AppState::new(dir.path().to_path_buf(), alerts.clone(), guard.clone());
        assert!(Arc::ptr_eq(&state.alerts, &alerts));
        assert!(Arc::ptr_eq(&state.guard, &guard));

        // 通过 `AppState` 触发的失效必须落在同一个服务实例上。
        guard.unlock("srv", 5).unwrap();
        state.guard.clear_server("srv");
        assert!(guard.status("srv").locked);
        assert_eq!(alerts.active_count(), 0);
    }
}

//! 写许可、一次性预览与逐服务器写锁。
//!
//! 三者都是运行期状态：不落库、不写日志、不进入配置备份。任何一台服务器默认锁定，
//! 只有用户显式解锁（5/15/30 分钟）后才允许写；写操作之间按 `server_id` 串行，
//! 服务端之间互不阻塞。

pub mod permit;
pub mod preview;
pub mod write;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use tauri::AppHandle;
#[cfg(not(test))]
use tauri::Emitter;
use tokio::sync::{Mutex as AsyncMutex, OwnedMutexGuard};

use crate::ssh::error::SshError;

use self::permit::{PermitStatus, PermitStore};
use self::preview::{PreviewEntry, PreviewStore};

/// 内部映射锁的取用：中毒只说明某个持锁线程 panic 过，内存里的数据仍然完整，
/// 因此复用内部值而不是把 `PoisonError` 升级成用户可见错误。
fn lock_map<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn write_payload(server_id: &str, active: bool) -> serde_json::Value {
    serde_json::json!({ "serverId": server_id, "active": active })
}

pub struct GuardService {
    // 与 `AlertService` 相同：测试二进制没有应用清单，`AppHandle` 的析构会把
    // tauri 平台层（muda 的 comctl32 v6 导入）链进测试二进制，加载即失败。
    // 测试从不注入句柄，因此测试构建不持有它。
    #[cfg(not(test))]
    app: Option<AppHandle>,
    permits: PermitStore,
    previews: PreviewStore,
    /// 每台服务器一把异步互斥量；只按需创建，条目数由服务器数量决定。
    locks: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
    /// 当前有写操作在跑的服务器集合，用于 `WRITE_BUSY` 判定与事件载体。
    active: Mutex<HashSet<String>>,
}

/// 持有写锁期间一直存活；析构（含 panic 展开）即清标记并向前端广播。
pub struct WriteLock {
    server_id: String,
    service: Arc<GuardService>,
    _guard: OwnedMutexGuard<()>,
}

impl Drop for WriteLock {
    fn drop(&mut self) {
        lock_map(&self.service.active).remove(&self.server_id);
        self.service.emit_write(&self.server_id, false);
    }
}

impl GuardService {
    pub fn new(app: Option<AppHandle>) -> Arc<Self> {
        // 测试构建不保存句柄；忘记而不是丢弃它，避免实例化 `AppHandle` 的析构胶水。
        #[cfg(test)]
        std::mem::forget(app);
        Arc::new(Self {
            #[cfg(not(test))]
            app,
            permits: PermitStore::new(),
            previews: PreviewStore::new(),
            locks: Mutex::new(HashMap::new()),
            active: Mutex::new(HashSet::new()),
        })
    }

    pub fn status(&self, server_id: &str) -> PermitStatus {
        let mut status = self.permits.status(server_id);
        status.write_active = self.is_write_active(server_id);
        status
    }

    pub fn unlock(&self, server_id: &str, minutes: u8) -> Result<PermitStatus, SshError> {
        if self.is_write_active(server_id) {
            return Err(SshError::write_busy());
        }
        self.permits.unlock(server_id, minutes)?;
        let status = self.status(server_id);
        self.emit_permit(&status);
        Ok(status)
    }

    pub fn lock(&self, server_id: &str) -> Result<PermitStatus, SshError> {
        if self.is_write_active(server_id) {
            return Err(SshError::write_busy());
        }
        self.permits.lock(server_id);
        let status = self.status(server_id);
        self.emit_permit(&status);
        Ok(status)
    }

    /// 写命令的第一道闸：锁定（含刚到期）时广播锁定事件并返回 `PERMIT_REQUIRED`。
    pub fn require_permit(&self, server_id: &str) -> Result<u8, SshError> {
        let status = self.status(server_id);
        if status.locked {
            self.emit_permit(&status);
            return Err(SshError::permit_required());
        }
        Ok(status.minutes)
    }

    pub fn begin_write(self: &Arc<Self>, server_id: &str) -> Result<WriteLock, SshError> {
        let lock = {
            let mut locks = lock_map(&self.locks);
            Arc::clone(
                locks
                    .entry(server_id.to_string())
                    .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
            )
        };
        let guard = lock.try_lock_owned().map_err(|_| SshError::write_busy())?;
        lock_map(&self.active).insert(server_id.to_string());
        self.emit_write(server_id, true);
        Ok(WriteLock {
            server_id: server_id.to_string(),
            service: Arc::clone(self),
            _guard: guard,
        })
    }

    pub fn is_write_active(&self, server_id: &str) -> bool {
        lock_map(&self.active).contains(server_id)
    }

    pub fn store_preview(&self, server_id: &str, op: &str, payload: serde_json::Value) -> String {
        self.previews.insert(server_id, op, payload)
    }

    pub fn take_preview(&self, id: &str) -> Result<PreviewEntry, SshError> {
        self.previews.take(id)
    }

    pub fn clear_server(&self, server_id: &str) {
        self.permits.clear_server(server_id);
        self.previews.clear_server(server_id);
        let status = self.status(server_id);
        self.emit_permit(&status);
    }

    pub fn clear_all(&self) {
        self.permits.clear_all();
        self.previews.clear();
    }

    #[cfg(not(test))]
    fn emit_permit(&self, status: &PermitStatus) {
        if let Some(app) = &self.app {
            let _ = app.emit("ssh://permit", status);
        }
    }

    #[cfg(test)]
    fn emit_permit(&self, _status: &PermitStatus) {}

    #[cfg(not(test))]
    fn emit_write(&self, server_id: &str, active: bool) {
        if let Some(app) = &self.app {
            let _ = app.emit("guard://write", write_payload(server_id, active));
        }
    }

    #[cfg(test)]
    fn emit_write(&self, _server_id: &str, _active: bool) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn status_starts_locked() {
        let guard = GuardService::new(None);
        let status = guard.status("srv");
        assert!(status.locked);
        assert_eq!(status.expires_at_ms, 0);
        assert!(!status.write_active);
        assert_eq!(status.server_id, "srv");
    }

    #[test]
    fn unlock_and_lock_round_trip() {
        let guard = GuardService::new(None);
        assert_eq!(guard.unlock("srv", 10).unwrap_err().code, "INVALID_INPUT");
        let status = guard.unlock("srv", 15).unwrap();
        assert!(!status.locked);
        assert_eq!(status.minutes, 15);
        let locked = guard.lock("srv").unwrap();
        assert!(locked.locked);
    }

    #[test]
    fn second_write_is_busy_until_first_drops() {
        let guard = GuardService::new(None);
        let first = guard.begin_write("srv").unwrap();
        assert!(guard.is_write_active("srv"));
        match guard.begin_write("srv") {
            Err(error) => assert_eq!(error.code, "WRITE_BUSY"),
            Ok(_) => panic!("expected WRITE_BUSY"),
        }
        drop(first);
        assert!(!guard.is_write_active("srv"));
        assert!(guard.begin_write("srv").is_ok());
    }

    #[test]
    fn distinct_servers_write_concurrently() {
        let guard = GuardService::new(None);
        let first = guard.begin_write("srv_a").unwrap();
        let second = guard.begin_write("srv_b").unwrap();
        assert!(guard.is_write_active("srv_a"));
        assert!(guard.is_write_active("srv_b"));
        drop(first);
        assert!(!guard.is_write_active("srv_a"));
        assert!(guard.is_write_active("srv_b"));
        drop(second);
        assert!(!guard.is_write_active("srv_b"));
    }

    #[test]
    fn write_in_progress_blocks_lock_changes() {
        let guard = GuardService::new(None);
        guard.unlock("srv", 5).unwrap();
        let write = guard.begin_write("srv").unwrap();
        assert_eq!(guard.lock("srv").unwrap_err().code, "WRITE_BUSY");
        assert_eq!(guard.unlock("srv", 5).unwrap_err().code, "WRITE_BUSY");
        drop(write);
        assert!(guard.lock("srv").is_ok());
    }

    #[test]
    fn expired_permit_requires_unlock_again() {
        let guard = GuardService::new(None);
        guard.permits.unlock_for("srv", 5, Duration::ZERO);
        assert_eq!(
            guard.require_permit("srv").unwrap_err().code,
            "PERMIT_REQUIRED"
        );
        assert!(guard.status("srv").locked);
    }

    #[test]
    fn live_permit_reports_remaining_minutes() {
        let guard = GuardService::new(None);
        guard
            .permits
            .unlock_for("srv", 30, Duration::from_secs(30 * 60));
        assert_eq!(guard.require_permit("srv").unwrap(), 30);
        assert!(!guard.status("srv").locked);
    }

    #[test]
    fn clear_server_drops_permit_and_previews() {
        let guard = GuardService::new(None);
        guard.unlock("srv", 5).unwrap();
        let id = guard.store_preview("srv", "fs.mkdir", serde_json::json!({ "path": "/x" }));
        guard.clear_server("srv");
        assert!(guard.status("srv").locked);
        assert_eq!(guard.take_preview(&id).unwrap_err().code, "PREVIEW_EXPIRED");
    }

    #[test]
    fn clear_all_drops_every_server() {
        let guard = GuardService::new(None);
        guard
            .permits
            .unlock_for("srv_a", 5, Duration::from_secs(60));
        let id = guard.store_preview("srv_a", "fs.mkdir", serde_json::json!({}));
        guard.clear_all();
        assert!(guard.status("srv_a").locked);
        assert_eq!(guard.take_preview(&id).unwrap_err().code, "PREVIEW_EXPIRED");
    }

    #[test]
    fn preview_of_one_server_is_consumed_once() {
        let guard = GuardService::new(None);
        let id = guard.store_preview("srv", "fs.mkdir", serde_json::json!({ "path": "/x" }));
        let entry = guard.take_preview(&id).unwrap();
        assert_eq!(entry.server_id, "srv");
        assert_eq!(entry.op, "fs.mkdir");
        assert_eq!(guard.take_preview(&id).unwrap_err().code, "PREVIEW_EXPIRED");
    }

    #[test]
    fn write_lock_releases_on_panic() {
        let guard = GuardService::new(None);
        let previous = std::panic::take_hook();
        // 展开是测试的一部分，别把它当成失败信息打印到测试输出里。
        std::panic::set_hook(Box::new(|_| {}));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _hold = guard.begin_write("srv").unwrap();
            panic!("写操作内部 panic");
        }));
        std::panic::set_hook(previous);
        assert!(outcome.is_err());
        assert!(!guard.is_write_active("srv"));
        assert!(guard.begin_write("srv").is_ok());
    }

    #[test]
    fn write_event_payload_is_camel_case() {
        assert_eq!(
            write_payload("srv", true),
            serde_json::json!({ "serverId": "srv", "active": true })
        );
        assert_eq!(
            write_payload("srv", false),
            serde_json::json!({ "serverId": "srv", "active": false })
        );
    }

    #[test]
    fn permit_status_serializes_camel_case() {
        let guard = GuardService::new(None);
        guard.permits.unlock_for("srv", 5, Duration::from_secs(60));
        let json = serde_json::to_value(guard.status("srv")).unwrap();
        assert_eq!(json["serverId"], "srv");
        assert_eq!(json["locked"], false);
        assert_eq!(json["minutes"], 5);
        assert_eq!(json["writeActive"], false);
        assert!(json["expiresAtMs"].as_i64().unwrap_or(0) > 0);
    }
}

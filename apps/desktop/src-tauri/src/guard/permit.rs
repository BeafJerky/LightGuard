//! 内存许可：`server_id → 到期时刻`。
//!
//! 许可是运行期状态：不落库、不写日志、不随配置备份导出；进程退出即全部回到锁定。
//! 到期判断只依赖 `Instant`（单调时钟），`expires_at_ms` 只用于前端倒计时展示。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::alerts::now_ms;
use crate::ssh::error::SshError;

/// 可选的解锁时长（分钟）。UI 只提供这三档，后端按同一集合校验。
pub const PERMIT_OPTIONS: [u8; 3] = [5, 15, 30];

#[derive(Debug, Clone, Copy)]
pub struct PermitEntry {
    pub expires_at: Instant,
    pub minutes: u8,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermitStatus {
    pub server_id: String,
    pub locked: bool,
    pub minutes: u8,
    pub expires_at_ms: i64,
    pub write_active: bool,
}

#[derive(Debug, Default)]
pub struct PermitStore {
    entries: Mutex<HashMap<String, PermitEntry>>,
}

impl PermitStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn unlock(&self, server_id: &str, minutes: u8) -> Result<PermitStatus, SshError> {
        if !PERMIT_OPTIONS.contains(&minutes) {
            return Err(SshError::invalid_input("解锁时长只支持 5 / 15 / 30 分钟"));
        }
        let duration = match std::env::var("LG_TEST_PERMIT_SECONDS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
        {
            Some(seconds) => Duration::from_secs(seconds),
            None => Duration::from_secs(minutes as u64 * 60),
        };
        Ok(self.unlock_for(server_id, minutes, duration))
    }

    /// 测试入口：调用方直接给出存活时长，避免真实等待。
    pub fn unlock_for(&self, server_id: &str, minutes: u8, duration: Duration) -> PermitStatus {
        let entry = PermitEntry {
            expires_at: Instant::now() + duration,
            minutes,
            expires_at_ms: now_ms() + duration.as_millis() as i64,
        };
        super::lock_map(&self.entries).insert(server_id.to_string(), entry);
        self.status(server_id)
    }

    pub fn lock(&self, server_id: &str) {
        super::lock_map(&self.entries).remove(server_id);
    }

    /// 读取即清理：过期条目在这里被删除，所以调用方无需自己做过期判断。
    pub fn status(&self, server_id: &str) -> PermitStatus {
        let mut entries = super::lock_map(&self.entries);
        if let Some(entry) = entries.get(server_id) {
            if Instant::now() < entry.expires_at {
                return PermitStatus {
                    server_id: server_id.to_string(),
                    locked: false,
                    minutes: entry.minutes,
                    expires_at_ms: entry.expires_at_ms,
                    write_active: false,
                };
            }
            entries.remove(server_id);
        }
        PermitStatus {
            server_id: server_id.to_string(),
            locked: true,
            minutes: 0,
            expires_at_ms: 0,
            write_active: false,
        }
    }

    pub fn clear_all(&self) {
        super::lock_map(&self.entries).clear();
    }

    pub fn clear_server(&self, server_id: &str) {
        super::lock_map(&self.entries).remove(server_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlock_accepts_only_options() {
        let store = PermitStore::new();
        assert_eq!(store.unlock("srv", 10).unwrap_err().code, "INVALID_INPUT");
        assert_eq!(store.unlock("srv", 0).unwrap_err().code, "INVALID_INPUT");
        let status = store.unlock("srv", 15).unwrap();
        assert!(!status.locked);
        assert_eq!(status.minutes, 15);
        assert!(status.expires_at_ms > now_ms());
    }

    #[test]
    fn lock_and_clear_return_to_locked() {
        let store = PermitStore::new();
        store.unlock("srv", 5).unwrap();
        store.lock("srv");
        assert!(store.status("srv").locked);
        store.unlock("srv", 5).unwrap();
        store.clear_all();
        assert!(store.status("srv").locked);
    }

    #[test]
    fn expired_permit_reads_as_locked() {
        let store = PermitStore::new();
        // 零存活：读状态时必然已过期，无需 sleep。
        let status = store.unlock_for("srv", 5, Duration::ZERO);
        assert!(status.locked);
        assert_eq!(status.minutes, 0);
        assert_eq!(status.expires_at_ms, 0);
        assert!(store.status("srv").locked);
    }

    #[test]
    fn live_permit_survives_reads() {
        let store = PermitStore::new();
        store.unlock_for("srv", 30, Duration::from_secs(30 * 60));
        let status = store.status("srv");
        assert!(!status.locked);
        assert_eq!(status.minutes, 30);
        assert!(status.expires_at_ms > now_ms());
        // 其他服务器不受影响。
        assert!(store.status("other").locked);
    }

    #[test]
    fn clear_server_only_drops_that_server() {
        let store = PermitStore::new();
        store.unlock_for("srv_a", 5, Duration::from_secs(60));
        store.unlock_for("srv_b", 5, Duration::from_secs(60));
        store.clear_server("srv_a");
        assert!(store.status("srv_a").locked);
        assert!(!store.status("srv_b").locked);
    }

    #[test]
    fn expired_entry_is_pruned_on_read() {
        let store = PermitStore::new();
        store.unlock_for("srv", 5, Duration::ZERO);
        assert!(store.status("srv").locked);
        // 过期条目已被读路径删除，重新解锁可以直接写入。
        store.unlock_for("srv", 5, Duration::from_secs(60));
        assert_eq!(store.status("srv").minutes, 5);
    }
}

//! 一次性预览缓存：确认写操作前必须先拿到预览，执行时 `take` 消耗掉。
//!
//! 条目只存在内存里，带 TTL 与每服务器容量上限；过期或被淘汰的预览再由 `take`
//! 统一报 `PREVIEW_EXPIRED`，调用方无需区分"没用过"与"已过期"。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::OsRng;

use crate::ssh::error::SshError;

/// 预览有效期：超过即作废，必须重新发起预览。
pub const PREVIEW_TTL: Duration = Duration::from_secs(60);
/// 单台服务器同时保留的预览条数上限，超出淘汰最旧的一条。
pub const PREVIEW_CAP: usize = 8;

#[derive(Debug, Clone)]
pub struct PreviewEntry {
    pub server_id: String,
    pub op: String,
    pub payload: serde_json::Value,
    pub created_at: Instant,
    /// 插入序号：`Instant` 精度不足时用它稳定"最旧优先"的淘汰顺序。
    seq: u64,
}

#[derive(Debug, Default)]
pub struct PreviewStore {
    entries: Mutex<HashMap<String, PreviewEntry>>,
    next_seq: AtomicU64,
}

impl PreviewStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&self, server_id: &str, op: &str, payload: serde_json::Value) -> String {
        self.insert_aged(server_id, op, payload, Duration::ZERO)
    }

    fn insert_aged(
        &self,
        server_id: &str,
        op: &str,
        payload: serde_json::Value,
        age: Duration,
    ) -> String {
        let id = preview_id();
        let seq = self.next_seq.fetch_add(1, Ordering::Relaxed);
        // `Instant` 的回溯极限受开机时长约束；极端情况下退回"当前时刻"而不是 panic。
        let created_at = Instant::now().checked_sub(age).unwrap_or_else(Instant::now);
        let entry = PreviewEntry {
            server_id: server_id.to_string(),
            op: op.to_string(),
            payload,
            created_at,
            seq,
        };
        let mut entries = super::lock_map(&self.entries);
        entries.retain(|_, existing| existing.created_at.elapsed() < PREVIEW_TTL);
        entries.insert(id.clone(), entry);
        while entries
            .values()
            .filter(|existing| existing.server_id == server_id)
            .count()
            > PREVIEW_CAP
        {
            let oldest = entries
                .iter()
                .filter(|(key, existing)| {
                    existing.server_id == server_id && key.as_str() != id.as_str()
                })
                .min_by_key(|(_, existing)| (existing.created_at, existing.seq))
                .map(|(key, _)| key.clone());
            match oldest {
                Some(key) => {
                    entries.remove(&key);
                }
                None => break,
            }
        }
        id
    }

    pub fn take(&self, id: &str) -> Result<PreviewEntry, SshError> {
        match super::lock_map(&self.entries).remove(id) {
            Some(entry) if entry.created_at.elapsed() < PREVIEW_TTL => Ok(entry),
            _ => Err(SshError::preview_expired()),
        }
    }

    pub fn clear(&self) {
        super::lock_map(&self.entries).clear();
    }

    pub fn clear_server(&self, server_id: &str) {
        super::lock_map(&self.entries).retain(|_, entry| entry.server_id != server_id);
    }
}

fn preview_id() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preview_is_single_use() {
        let store = PreviewStore::new();
        let id = store.insert("srv", "fs.mkdir", json!({ "path": "/data/x" }));
        assert_eq!(id.len(), 32, "16 random bytes hex-encoded");
        let entry = store.take(&id).unwrap();
        assert_eq!(entry.op, "fs.mkdir");
        assert_eq!(entry.server_id, "srv");
        assert_eq!(store.take(&id).unwrap_err().code, "PREVIEW_EXPIRED");
    }

    #[test]
    fn ids_are_unique_per_insert() {
        let store = PreviewStore::new();
        let first = store.insert("srv", "fs.mkdir", json!({}));
        let second = store.insert("srv", "fs.mkdir", json!({}));
        assert_ne!(first, second);
    }

    #[test]
    fn expired_preview_is_rejected() {
        let store = PreviewStore::new();
        let id = store.insert_aged(
            "srv",
            "fs.mkdir",
            json!({}),
            PREVIEW_TTL + Duration::from_secs(1),
        );
        assert_eq!(store.take(&id).unwrap_err().code, "PREVIEW_EXPIRED");
    }

    #[test]
    fn unknown_preview_is_rejected() {
        let store = PreviewStore::new();
        assert_eq!(
            store
                .take("ffffffffffffffffffffffffffffffff")
                .unwrap_err()
                .code,
            "PREVIEW_EXPIRED"
        );
    }

    #[test]
    fn oldest_preview_is_evicted_beyond_cap() {
        let store = PreviewStore::new();
        let mut ids = Vec::new();
        for index in 0..(PREVIEW_CAP + 1) {
            ids.push(store.insert("srv", "fs.mkdir", json!({ "index": index })));
        }
        assert_eq!(store.take(&ids[0]).unwrap_err().code, "PREVIEW_EXPIRED");
        assert!(store.take(&ids[PREVIEW_CAP]).is_ok());
    }

    #[test]
    fn cap_is_tracked_per_server() {
        let store = PreviewStore::new();
        let mut other = Vec::new();
        for index in 0..PREVIEW_CAP {
            other.push(store.insert("srv_b", "fs.mkdir", json!({ "index": index })));
        }
        for index in 0..PREVIEW_CAP {
            store.insert("srv_a", "fs.mkdir", json!({ "index": index }));
        }
        // srv_a 的插入不会挤掉 srv_b 的预览。
        for id in other {
            assert!(store.take(&id).is_ok());
        }
    }

    #[test]
    fn clear_drops_everything() {
        let store = PreviewStore::new();
        let id = store.insert("srv", "fs.mkdir", json!({}));
        store.clear();
        assert_eq!(store.take(&id).unwrap_err().code, "PREVIEW_EXPIRED");
    }

    #[test]
    fn clear_server_keeps_other_servers() {
        let store = PreviewStore::new();
        let mine = store.insert("srv_a", "fs.mkdir", json!({}));
        let other = store.insert("srv_b", "fs.mkdir", json!({}));
        store.clear_server("srv_a");
        assert_eq!(store.take(&mine).unwrap_err().code, "PREVIEW_EXPIRED");
        assert!(store.take(&other).is_ok());
    }
}

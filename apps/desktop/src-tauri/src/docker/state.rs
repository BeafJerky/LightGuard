//! Docker 运行期状态：拉取取消标志与 sudo 密码缓存。
//! 两者都只驻留内存；SSH 断线、应用退出或清空本地数据时归零。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// sudo 密码的唯一载体：不实现 `Clone` / `Serialize`，`Debug` 只输出占位符，
/// 析构与显式清理都会把字节清零。
struct Secret(Vec<u8>);

impl Secret {
    fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        for byte in &mut self.0 {
            *byte = 0;
        }
        std::hint::black_box(&self.0);
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Secret(***)")
    }
}

#[derive(Debug, Default)]
pub struct DockerState {
    pulls: Mutex<HashMap<String, Arc<AtomicBool>>>,
    passwords: Mutex<HashMap<String, Secret>>,
    passwordless: Mutex<HashMap<String, bool>>,
}

impl DockerState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一次新的拉取并返回它的取消标志；同一服务器的旧条目被替换。
    /// 只有持有写锁的拉取才能走到这里，因此不会与正在运行的拉取互相覆盖。
    pub fn begin_pull(&self, server_id: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        self.lock_pulls().insert(server_id.to_string(), Arc::clone(&flag));
        flag
    }

    /// 拉取进行中才会命中：只置标志、不删除条目，收尾由 `finish_pull` 负责。
    pub fn cancel_pull(&self, server_id: &str) -> bool {
        match self.lock_pulls().get(server_id) {
            Some(flag) => {
                flag.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    pub fn finish_pull(&self, server_id: &str) {
        self.lock_pulls().remove(server_id);
    }

    /// 直接接管字节所有权，避免密码在多个 `String` 副本间流转。
    pub fn cache_password(&self, server_id: &str, password: Vec<u8>) {
        self.lock_passwords()
            .insert(server_id.to_string(), Secret(password));
    }

    /// 传输用的临时副本：调用方在下一次 await 之后必须调用 [`Self::wipe`]。
    pub fn password_copy(&self, server_id: &str) -> Option<Vec<u8>> {
        self.lock_passwords()
            .get(server_id)
            .map(|secret| secret.as_bytes().to_vec())
    }

    pub fn clear_password(&self, server_id: &str) {
        self.lock_passwords().remove(server_id);
    }

    pub fn set_passwordless(&self, server_id: &str, value: bool) {
        self.lock_passwordless().insert(server_id.to_string(), value);
    }

    pub fn passwordless(&self, server_id: &str) -> Option<bool> {
        self.lock_passwordless().get(server_id).copied()
    }

    pub fn wipe(bytes: &mut [u8]) {
        for byte in bytes.iter_mut() {
            *byte = 0;
        }
        std::hint::black_box(bytes);
    }

    pub fn clear_server(&self, server_id: &str) {
        self.lock_pulls().remove(server_id);
        self.lock_passwords().remove(server_id);
        self.lock_passwordless().remove(server_id);
    }

    pub fn clear_all(&self) {
        self.lock_pulls().clear();
        self.lock_passwords().clear();
        self.lock_passwordless().clear();
    }

    fn lock_pulls(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<AtomicBool>>> {
        self.pulls.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    fn lock_passwords(&self) -> std::sync::MutexGuard<'_, HashMap<String, Secret>> {
        self.passwords.lock().unwrap_or_else(|poison| poison.into_inner())
    }

    fn lock_passwordless(&self) -> std::sync::MutexGuard<'_, HashMap<String, bool>> {
        self.passwordless.lock().unwrap_or_else(|poison| poison.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_flag_is_per_server_and_one_shot_until_started_again() {
        let state = DockerState::new();
        let flag = state.begin_pull("srv");
        assert!(!flag.load(std::sync::atomic::Ordering::Relaxed));
        assert!(!state.cancel_pull("srv_b"));
        assert!(state.cancel_pull("srv"));
        assert!(flag.load(std::sync::atomic::Ordering::Relaxed));
        state.finish_pull("srv");
        assert!(!state.cancel_pull("srv"));
    }

    #[test]
    fn clear_server_drops_only_its_flags() {
        let state = DockerState::new();
        state.begin_pull("srv_a");
        state.begin_pull("srv_b");
        state.clear_server("srv_a");
        assert!(!state.cancel_pull("srv_a"));
        assert!(state.cancel_pull("srv_b"));
    }

    #[test]
    fn passwords_are_stored_cleared_and_wiped() {
        let state = DockerState::new();
        state.cache_password("srv", b"s3cret".to_vec());
        let mut bytes = state.password_copy("srv").unwrap();
        assert_eq!(bytes, b"s3cret");
        DockerState::wipe(&mut bytes);
        assert!(bytes.iter().all(|byte| *byte == 0));
        state.clear_password("srv");
        assert!(state.password_copy("srv").is_none());
    }

    #[test]
    fn passwordless_flag_is_cached_per_server() {
        let state = DockerState::new();
        state.set_passwordless("srv", true);
        assert_eq!(state.passwordless("srv"), Some(true));
        assert_eq!(state.passwordless("other"), None);
        state.clear_server("srv");
        assert_eq!(state.passwordless("srv"), None);
    }

    #[test]
    fn debug_output_never_contains_the_password() {
        let state = DockerState::new();
        state.cache_password("srv", b"s3cret".to_vec());
        let debug = format!("{state:?}");
        assert!(!debug.contains("s3cret"));
    }
}

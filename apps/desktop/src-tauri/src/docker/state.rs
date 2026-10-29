//! Docker 运行期状态：拉取取消标志（Task 10 再追加 sudo 密码缓存）。
//! 与许可、预览一样只驻留内存，SSH 断线、应用退出或清空本地数据时归零。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
pub struct DockerState {
    pulls: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl DockerState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一次新的拉取并返回它的取消标志；同一服务器的旧条目被替换。
    /// 只有持有写锁的拉取才能走到这里，因此不会与正在运行的拉取互相覆盖。
    pub fn begin_pull(&self, server_id: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        let mut pulls = self.lock_pulls();
        pulls.insert(server_id.to_string(), Arc::clone(&flag));
        flag
    }

    /// 拉取进行中才会命中：只置标志、不删除条目，收尾由 `finish_pull` 负责。
    pub fn cancel_pull(&self, server_id: &str) -> bool {
        let pulls = self.lock_pulls();
        match pulls.get(server_id) {
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

    pub fn clear_server(&self, server_id: &str) {
        self.lock_pulls().remove(server_id);
    }

    pub fn clear_all(&self) {
        self.lock_pulls().clear();
    }

    fn lock_pulls(&self) -> std::sync::MutexGuard<'_, HashMap<String, Arc<AtomicBool>>> {
        self.pulls.lock().unwrap_or_else(|poison| poison.into_inner())
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
}

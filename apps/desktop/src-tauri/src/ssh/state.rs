use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{Mutex, Semaphore};

use super::session::SessionEntry;
use super::sftp::client::{SftpHandle, TransferHandle};
use super::terminal::TerminalEntry;
use crate::alerts::AlertService;
use crate::guard::GuardService;

pub struct AppState {
    pub config_dir: PathBuf,
    pub sessions: Mutex<HashMap<String, Arc<Mutex<SessionEntry>>>>,
    pub terminals: Arc<Mutex<HashMap<String, TerminalEntry>>>,
    pub sftp: Mutex<HashMap<String, SftpHandle>>,
    pub transfers: Mutex<HashMap<String, TransferHandle>>,
    pub transfer_slots: Arc<Semaphore>,
    /// 与 `manage` 给命令层的 `Arc<AlertService>` 是同一个实例：连接、断线、指纹
    /// 变更与清空数据这些生命周期钩子都直接调用它。
    pub alerts: Arc<AlertService>,
    /// 同上。许可与预览是运行期状态，随连接生命周期归零。
    pub guard: Arc<GuardService>,
    /// Docker 拉取取消与（Task 10 起）sudo 密码缓存；随连接生命周期清理。
    pub docker: crate::docker::state::DockerState,
}

impl AppState {
    /// 只做字段装配：不开后台任务、不连网、不碰磁盘，保证启动路径保持廉价。
    pub fn new(
        config_dir: PathBuf,
        alerts: Arc<AlertService>,
        guard: Arc<GuardService>,
    ) -> Arc<Self> {
        Arc::new(Self {
            config_dir,
            sessions: Mutex::new(HashMap::new()),
            terminals: Arc::new(Mutex::new(HashMap::new())),
            sftp: Mutex::new(HashMap::new()),
            transfers: Mutex::new(HashMap::new()),
            transfer_slots: Arc::new(Semaphore::new(2)),
            alerts,
            guard,
            docker: crate::docker::state::DockerState::new(),
        })
    }
}

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{Mutex, Semaphore};

use super::session::SessionEntry;
use super::sftp::client::{SftpHandle, TransferHandle};
use super::terminal::TerminalEntry;

pub struct AppState {
    pub config_dir: PathBuf,
    pub sessions: Mutex<HashMap<String, Arc<Mutex<SessionEntry>>>>,
    pub terminals: Arc<Mutex<HashMap<String, TerminalEntry>>>,
    pub sftp: Mutex<HashMap<String, SftpHandle>>,
    pub transfers: Mutex<HashMap<String, TransferHandle>>,
    pub transfer_slots: Arc<Semaphore>,
}

impl AppState {
    pub fn new(config_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            config_dir,
            sessions: Mutex::new(HashMap::new()),
            terminals: Arc::new(Mutex::new(HashMap::new())),
            sftp: Mutex::new(HashMap::new()),
            transfers: Mutex::new(HashMap::new()),
            transfer_slots: Arc::new(Semaphore::new(2)),
        })
    }
}

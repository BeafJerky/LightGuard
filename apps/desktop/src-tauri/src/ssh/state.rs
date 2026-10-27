use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;

use super::session::SessionEntry;
use super::terminal::TerminalEntry;

pub struct AppState {
    pub config_dir: PathBuf,
    pub sessions: Mutex<HashMap<String, Arc<Mutex<SessionEntry>>>>,
    pub terminals: Arc<Mutex<HashMap<String, TerminalEntry>>>,
}

impl AppState {
    pub fn new(config_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            config_dir,
            sessions: Mutex::new(HashMap::new()),
            terminals: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

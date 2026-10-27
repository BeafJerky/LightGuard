use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::dto::HostKeyState;
use super::error::SshError;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct KnownHosts {
    #[serde(default)]
    hosts: BTreeMap<String, HostRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostRecord {
    pub algorithm: String,
    pub fingerprint: String,
    pub first_seen: String,
    pub last_seen: String,
}

pub struct HostKeyStore {
    path: PathBuf,
}

impl HostKeyStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn verify(
        &self,
        host: &str,
        port: u16,
        fingerprint: &str,
        algorithm: &str,
        verify: bool,
    ) -> Result<HostKeyState, SshError> {
        let mut known = self.read();
        let key = host_key(host, port);
        if let Some(record) = known.hosts.get_mut(&key) {
            if record.fingerprint == fingerprint {
                record.last_seen = super::session::now_iso();
                let _ = self.write(&known);
                return Ok(HostKeyState::Known);
            }
            if verify {
                return Err(SshError::host_key_mismatch());
            }
            return Ok(HostKeyState::New);
        }
        let now = super::session::now_iso();
        known.hosts.insert(
            key,
            HostRecord {
                algorithm: algorithm.to_string(),
                fingerprint: fingerprint.to_string(),
                first_seen: now.clone(),
                last_seen: now,
            },
        );
        self.write(&known)?;
        Ok(HostKeyState::New)
    }

    pub fn forget(&self, host: &str, port: u16) -> Result<(), SshError> {
        let mut known = self.read();
        known.hosts.remove(&host_key(host, port));
        self.write(&known)
    }

    pub fn clear(&self) -> Result<(), SshError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(SshError::internal("无法清除主机指纹记录")),
        }
    }

    fn read(&self) -> KnownHosts {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn write(&self, known: &KnownHosts) -> Result<(), SshError> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| SshError::internal("无法创建主机指纹目录"))?;
        }
        let text = serde_json::to_string_pretty(known)
            .map_err(|_| SshError::internal("主机指纹序列化失败"))?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|_| SshError::internal("无法写入主机指纹记录"))?;
        std::fs::rename(&tmp, &self.path)
            .map_err(|_| SshError::internal("无法保存主机指纹记录"))?;
        Ok(())
    }
}

fn host_key(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn tofu_lifecycle() {
        let dir = tempdir().unwrap();
        let store = HostKeyStore::new(dir.path().join("known_hosts.json"));
        assert_eq!(
            store
                .verify("10.0.0.1", 22, "SHA256:aaa", "ssh-ed25519", true)
                .unwrap(),
            HostKeyState::New
        );
        assert_eq!(
            store
                .verify("10.0.0.1", 22, "SHA256:aaa", "ssh-ed25519", true)
                .unwrap(),
            HostKeyState::Known
        );
        assert!(store
            .verify("10.0.0.1", 22, "SHA256:bbb", "ssh-ed25519", true)
            .is_err());
        store.forget("10.0.0.1", 22).unwrap();
        assert_eq!(
            store
                .verify("10.0.0.1", 22, "SHA256:bbb", "ssh-ed25519", true)
                .unwrap(),
            HostKeyState::New
        );
    }

    #[test]
    fn ports_are_isolated() {
        let dir = tempdir().unwrap();
        let store = HostKeyStore::new(dir.path().join("known_hosts.json"));
        store
            .verify("10.0.0.1", 22, "SHA256:aaa", "ssh-ed25519", true)
            .unwrap();
        assert_eq!(
            store
                .verify("10.0.0.1", 2222, "SHA256:bbb", "ssh-ed25519", true)
                .unwrap(),
            HostKeyState::New
        );
    }
}

use std::path::{Path, PathBuf};

use aes_gcm::aead::{rand_core::RngCore, Aead, OsRng, Payload};
use aes_gcm::{AeadCore, Aes256Gcm, KeyInit};
use serde::{Deserialize, Serialize};

use super::dto::AuthKind;
use super::error::SshError;

const SERVICE: &str = "com.lightguard.desktop";
const MASTER_USER: &str = "master-key";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretBundle {
    pub auth: AuthKind,
    pub secret: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<String>,
}

#[derive(Clone)]
pub struct CredentialStore {
    dir: PathBuf,
    master: Option<[u8; 32]>,
}

impl CredentialStore {
    /// Builds a store even when the OS keyring is unavailable. Saving or
    /// loading then fails with `CREDENTIAL_ERROR`, but a session that does not
    /// remember credentials can still be used.
    pub fn from_config(config_dir: &Path) -> Self {
        Self {
            dir: config_dir.join("secrets"),
            master: load_or_create_master_key().ok(),
        }
    }

    #[doc(hidden)]
    pub fn with_master_key(dir: PathBuf, master: [u8; 32]) -> Self {
        Self {
            dir,
            master: Some(master),
        }
    }

    fn master_key(&self) -> Result<[u8; 32], SshError> {
        self.master
            .ok_or_else(|| SshError::credential("系统凭据库不可用"))
    }

    pub fn path_for(&self, server_id: &str) -> PathBuf {
        self.dir.join(format!("{server_id}.bin"))
    }

    pub fn save(&self, server_id: &str, bundle: &SecretBundle) -> Result<(), SshError> {
        let master = self.master_key()?;
        let cipher = Aes256Gcm::new_from_slice(&master)
            .map_err(|_| SshError::credential("主密钥无效"))?;
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let plaintext = serde_json::to_vec(bundle)
            .map_err(|_| SshError::credential("凭据序列化失败"))?;
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: &plaintext,
                    aad: server_id.as_bytes(),
                },
            )
            .map_err(|_| SshError::credential("凭据加密失败"))?;
        let path = self.path_for(server_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| SshError::credential("无法创建凭据目录"))?;
        }
        let mut file = Vec::with_capacity(nonce.len() + ciphertext.len());
        file.extend_from_slice(&nonce);
        file.extend_from_slice(&ciphertext);
        std::fs::write(path, file).map_err(|_| SshError::credential("无法写入凭据文件"))?;
        Ok(())
    }

    pub fn load(&self, server_id: &str) -> Result<Option<SecretBundle>, SshError> {
        let path = self.path_for(server_id);
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(SshError::credential("无法读取凭据文件")),
        };
        if bytes.len() <= 12 {
            return Err(SshError::credential("凭据文件已损坏"));
        }
        let (nonce, ciphertext) = bytes.split_at(12);
        let master = self.master_key()?;
        let cipher = Aes256Gcm::new_from_slice(&master)
            .map_err(|_| SshError::credential("主密钥无效"))?;
        let nonce = aes_gcm::Nonce::from_slice(nonce);
        let plaintext = cipher
            .decrypt(
                nonce,
                Payload {
                    msg: ciphertext,
                    aad: server_id.as_bytes(),
                },
            )
            .map_err(|_| SshError::credential("凭据解密失败，请重新输入"))?;
        serde_json::from_slice(&plaintext).map_err(|_| SshError::credential("凭据内容无效"))
    }

    pub fn delete(&self, server_id: &str) -> Result<(), SshError> {
        match std::fs::remove_file(self.path_for(server_id)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(SshError::credential("无法删除凭据文件")),
        }
    }

    pub fn clear(&self) -> Result<(), SshError> {
        match std::fs::remove_dir_all(&self.dir) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(SshError::credential("无法清空凭据目录")),
        }
    }
}

fn load_or_create_master_key() -> Result<[u8; 32], SshError> {
    let entry = keyring::Entry::new(SERVICE, MASTER_USER)
        .map_err(|_| SshError::credential("系统凭据库不可用"))?;
    if let Ok(bytes) = entry.get_secret() {
        if bytes.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            return Ok(key);
        }
    }
    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    entry
        .set_secret(&key)
        .map_err(|_| SshError::credential("无法写入系统凭据库"))?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn bundles_roundtrip_without_plaintext() {
        let dir = tempdir().unwrap();
        let store = CredentialStore::with_master_key(dir.path().to_path_buf(), [7u8; 32]);
        store
            .save(
                "srv1",
                &SecretBundle {
                    auth: AuthKind::Password,
                    secret: "p@ss".into(),
                    passphrase: None,
                },
            )
            .unwrap();
        let raw = std::fs::read(store.path_for("srv1")).unwrap();
        assert!(!raw.windows(4).any(|w| w == b"p@ss"));
        let loaded = store.load("srv1").unwrap().unwrap();
        assert_eq!(loaded.secret, "p@ss");
    }

    #[test]
    fn wrong_master_key_fails() {
        let dir = tempdir().unwrap();
        let a = CredentialStore::with_master_key(dir.path().join("a"), [1u8; 32]);
        a.save(
            "srv1",
            &SecretBundle {
                auth: AuthKind::Key,
                secret: "-----BEGIN".into(),
                passphrase: None,
            },
        )
        .unwrap();
        let b = CredentialStore::with_master_key(dir.path().join("a"), [2u8; 32]);
        assert!(b.load("srv1").is_err());
    }

    #[test]
    fn delete_removes_ciphertext() {
        let dir = tempdir().unwrap();
        let store = CredentialStore::with_master_key(dir.path().to_path_buf(), [3u8; 32]);
        store
            .save(
                "srv1",
                &SecretBundle {
                    auth: AuthKind::Password,
                    secret: "x".into(),
                    passphrase: None,
                },
            )
            .unwrap();
        store.delete("srv1").unwrap();
        assert!(store.load("srv1").unwrap().is_none());
    }
}

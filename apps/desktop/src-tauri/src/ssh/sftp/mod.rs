pub mod client;
pub mod commands;
pub mod dto;
pub mod list;
pub mod path;
pub mod preview;
pub mod transfer;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    use super::client;
    use super::dto::FileKind;
    use super::list;
    use super::preview;
    use super::transfer;
    use crate::ssh::credentials::CredentialStore;
    use crate::ssh::dto::{AuthKind, ConnectInput};
    use crate::ssh::session;
    use crate::ssh::state::AppState;

    /// Credentials for the real-server test. All three of `LG_SSH_HOST`,
    /// `LG_SSH_USER` and `LG_SSH_PASSWORD` must be non-empty; `LG_SSH_PORT`
    /// defaults to 22.
    fn env() -> Option<(String, String, String, u16)> {
        let host = std::env::var("LG_SSH_HOST").ok()?;
        let user = std::env::var("LG_SSH_USER").ok()?;
        let password = std::env::var("LG_SSH_PASSWORD").ok()?;
        if host.is_empty() || user.is_empty() || password.is_empty() {
            return None;
        }
        let port = std::env::var("LG_SSH_PORT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(22);
        Some((host, user, password, port))
    }

    /// Opt-in round trip against a real server: home, listing, stat, text
    /// preview, single-file download and directory download all go through the
    /// production code paths. Everything is read-only; the only writes are the
    /// downloads inside a local temporary directory. Without `LG_SSH_*` the test
    /// prints a skip line and passes, so CI stays offline-friendly.
    #[tokio::test]
    async fn real_sftp_readonly_roundtrip() {
        let Some((host, user, password, port)) = env() else {
            eprintln!("skipping: LG_SSH_* not set");
            return;
        };
        let dir = tempfile::tempdir().unwrap();
        let store = CredentialStore::with_master_key(dir.path().to_path_buf(), [7u8; 32]);
        let outcome = session::connect(
            ConnectInput {
                server_id: "sftp-it".to_string(),
                host,
                port,
                user,
                auth: AuthKind::Password,
                secret: password,
                passphrase: None,
                remember: false,
                verify_host_key: false,
            },
            dir.path(),
            &store,
        )
        .await
        .expect("connect");

        let state = AppState::new(dir.path().to_path_buf());
        state.sessions.lock().await.insert(
            "sftp-it".to_string(),
            Arc::new(tokio::sync::Mutex::new(outcome.entry)),
        );

        let home = list::home(&state, "sftp-it").await.expect("home");
        assert!(home.starts_with('/'), "home must be absolute: {home}");
        let listing = list::list(&state, "sftp-it", &home).await.expect("list home");
        assert_eq!(listing.canonical_path, home);
        assert!(PathBuf::from(&listing.path).has_root());

        let stat = list::stat(&state, "sftp-it", "/etc/hostname")
            .await
            .expect("stat");
        assert_eq!(stat.kind, FileKind::File);

        let text = preview::text(&state, "sftp-it", "/etc/hostname")
            .await
            .expect("preview");
        assert!(!text.content.trim().is_empty());

        let client = client::acquire(&state, "sftp-it").await.expect("client");
        let cancel = AtomicBool::new(false);
        let file = dir.path().join("hostname.txt");
        let outcome = transfer::download_file(
            &client,
            "/etc/hostname",
            &file,
            "it-file",
            &cancel,
            |_, _| {},
        )
        .await
        .expect("download");
        assert!(matches!(outcome, transfer::TransferOutcome::Saved { .. }));
        assert!(!std::fs::read(&file).unwrap().is_empty());

        // `LG_SSH_DIR` points at a directory the login user can read in full;
        // /etc holds root-only files on a normal host, and the spec turns a
        // non-zero tar exit into TRANSFER_FAILED (asserted below when
        // `LG_SSH_DENIED_DIR` names such a directory).
        let source = std::env::var("LG_SSH_DIR").unwrap_or_else(|_| "/etc/apt".to_string());
        let archive = dir.path().join("dir.tar.gz");
        let handle = client::session_handle(&state, "sftp-it")
            .await
            .expect("handle");
        let outcome = transfer::download_dir(
            &handle,
            &client,
            &source,
            &archive,
            "it-archive",
            &cancel,
            |_, _| {},
        )
        .await
        .expect("tar");
        assert!(matches!(outcome, transfer::TransferOutcome::Saved { .. }));
        let bytes = std::fs::read(&archive).unwrap();
        assert_eq!(&bytes[..2], &[0x1f, 0x8b]);

        if let Ok(denied_source) = std::env::var("LG_SSH_DENIED_DIR") {
            let denied = dir.path().join("denied.tar.gz");
            let error = transfer::download_dir(
                &handle,
                &client,
                &denied_source,
                &denied,
                "it-denied",
                &cancel,
                |_, _| {},
            )
            .await
            .expect_err("unreadable subtree must fail");
            assert_eq!(error.code, "TRANSFER_FAILED");
            assert!(error.message.contains("退出码"));
            assert!(!std::fs::read_dir(dir.path())
                .unwrap()
                .any(|entry| entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains(".lgpart")));
        }

        let missing = list::stat(&state, "sftp-it", "/definitely-not-here-12345").await;
        assert_eq!(missing.unwrap_err().code, "FILE_NOT_FOUND");

        client::drop_for_server(&state, "sftp-it").await;
    }
}

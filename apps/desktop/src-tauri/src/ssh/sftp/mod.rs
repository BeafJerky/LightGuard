pub mod client;
pub mod commands;
pub mod dto;
pub mod list;
pub mod path;
pub mod preview;
pub mod transfer;

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use super::client;
    use super::dto::FileKind;
    use super::list;
    use super::path::shell_quote;
    use super::preview;
    use super::transfer;
    use crate::ssh::credentials::CredentialStore;
    use crate::ssh::dto::{AuthKind, ConnectInput};
    use crate::ssh::session;

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

        let state = crate::test_support::headless_state(dir.path().to_path_buf());
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

    /// Runs one remote shell command over `handle` and returns its exit status
    /// plus the merged stdout/stderr bytes as lossy text.
    async fn exec(
        handle: &russh::client::Handle<session::ClientHandler>,
        command: &str,
    ) -> (u32, String) {
        use russh::ChannelMsg;

        let mut channel = handle.channel_open_session().await.expect("channel");
        channel.exec(true, command).await.expect("exec");
        let mut code = None;
        let mut output = Vec::new();
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Data { data }) => output.extend_from_slice(&data),
                Some(ChannelMsg::ExtendedData { data, .. }) => output.extend_from_slice(&data),
                Some(ChannelMsg::ExitStatus { exit_status }) => code = Some(exit_status),
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                _ => {}
            }
        }
        (
            code.unwrap_or(255),
            String::from_utf8_lossy(&output).to_string(),
        )
    }

    /// Edge checks that only make sense against a real server: a 20 000 entry
    /// listing, a GB18030 text preview, a 10 MiB image preview, an SFTP
    /// reconnect, and a pre-canceled download that must leave no residue.
    /// Failures are collected instead of asserted so the caller always gets to
    /// run the cleanup command.
    async fn run_edge_checks(
        state: &crate::ssh::state::AppState,
        handle: &russh::client::Handle<session::ClientHandler>,
        fixture: &str,
        local: &Path,
    ) -> Vec<String> {
        let mut failures = Vec::new();

        macro_rules! record {
            ($label:expr, $result:expr) => {
                match $result {
                    Ok(value) => Some(value),
                    Err(error) => {
                        failures.push(format!("{}: {:?}", $label, error));
                        None
                    }
                }
            };
        }

        let started = Instant::now();
        if let Some(listing) = record!(
            "list 20k",
            list::list(state, "sftp-edge", &format!("{fixture}/many")).await
        ) {
            if listing.entries.len() != 20_000 {
                failures.push(format!(
                    "20k listing returned {} entries",
                    listing.entries.len()
                ));
            }
            if listing.truncated {
                failures.push("20k listing was truncated".to_string());
            }
            let elapsed = started.elapsed();
            if elapsed > Duration::from_secs(60) {
                failures.push(format!("20k listing took {elapsed:?}"));
            }
        }

        if let Some(text) = record!(
            "gb18030 preview",
            preview::text(state, "sftp-edge", &format!("{fixture}/gb18030.txt")).await
        ) {
            if text.encoding != "gb18030" {
                failures.push(format!("gb18030 preview decoded as {}", text.encoding));
            }
            if !text.content.contains("中文测试") {
                failures.push("gb18030 preview lost the text".to_string());
            }
        }

        if let Some(image) = record!(
            "10 MiB image preview",
            preview::image(state, "sftp-edge", &format!("{fixture}/big.png")).await
        ) {
            if image.bytes != 10 * 1024 * 1024 {
                failures.push(format!("image preview returned {} bytes", image.bytes));
            }
            if image.base64.is_empty() {
                failures.push("image preview returned no data".to_string());
            }
        }

        client::drop_for_server(state, "sftp-edge").await;
        if let Some(listing) = record!(
            "list after reconnect",
            list::list(state, "sftp-edge", fixture).await
        ) {
            if !listing
                .entries
                .iter()
                .any(|entry| entry.name == "gb18030.txt")
            {
                failures.push("reconnected listing missed the fixture file".to_string());
            }
        }

        let client = match record!(
            "re-acquire client",
            client::acquire(state, "sftp-edge").await
        ) {
            Some(client) => client,
            None => return failures,
        };
        let cancel = AtomicBool::new(true);
        let target = local.join("cancel.bin");
        if let Some(outcome) = record!(
            "pre-cancel download",
            transfer::download_file(
                &client,
                &format!("{fixture}/big.png"),
                &target,
                "edge-cancel",
                &cancel,
                |_, _| {},
            )
            .await
        ) {
            if !matches!(outcome, transfer::TransferOutcome::Canceled { bytes: 0 }) {
                failures.push(format!("pre-cancel download returned {outcome:?}"));
            }
            if target.exists() {
                failures.push("cancelled download left the target file".to_string());
            }
            if transfer::part_path(&target, "edge-cancel").exists() {
                failures.push("cancelled download left a .lgpart file".to_string());
            }
        }

        let (status, output) =
            exec(handle, &format!("test -f {}/big.png", shell_quote(fixture))).await;
        if status != 0 {
            failures.push(format!("fixture disappeared before cleanup: {output}"));
        }
        failures
    }

    /// Opt-in edge run against a real server. The fixture lives in a fresh
    /// `LG_TEST_REMOTE_DIR/lg-edge-<stamp>` directory, is never written outside
    /// it, and is removed with `rm -rf` on every exit path. Without `LG_SSH_*`
    /// or `LG_TEST_REMOTE_DIR` the test prints a skip line and returns, so it is
    /// never mistaken for a real-server pass.
    #[tokio::test]
    async fn real_sftp_edges() {
        let Some((host, user, password, port)) = env() else {
            eprintln!("skipping: LG_SSH_* not set");
            return;
        };
        let Some(root) = std::env::var("LG_TEST_REMOTE_DIR")
            .ok()
            .filter(|value| !value.is_empty())
        else {
            eprintln!("skipping: LG_TEST_REMOTE_DIR not set");
            return;
        };

        let dir = tempfile::tempdir().unwrap();
        let store = CredentialStore::with_master_key(dir.path().to_path_buf(), [8u8; 32]);
        let outcome = session::connect(
            ConnectInput {
                server_id: "sftp-edge".to_string(),
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

        let state = crate::test_support::headless_state(dir.path().to_path_buf());
        state.sessions.lock().await.insert(
            "sftp-edge".to_string(),
            Arc::new(tokio::sync::Mutex::new(outcome.entry)),
        );
        let handle = client::session_handle(&state, "sftp-edge")
            .await
            .expect("handle");

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let fixture = format!("{}/lg-edge-{stamp}", root.trim_end_matches('/'));
        let quoted = shell_quote(&fixture);
        let setup = format!(
            "set -e; mkdir -p {quoted}/many; printf '\\326\\320\\316\\304\\262\\342\\312\\324\\n' > {quoted}/gb18030.txt; dd if=/dev/zero of={quoted}/big.png bs=1M count=10 status=none; cd {quoted}/many && seq 1 20000 | xargs touch"
        );
        let (status, output) = exec(&handle, &setup).await;
        if status != 0 {
            let _ = exec(&handle, &format!("rm -rf {quoted}")).await;
            panic!("fixture setup failed: {output}");
        }

        let failures = run_edge_checks(&state, &handle, &fixture, dir.path()).await;
        let (status, output) = exec(&handle, &format!("rm -rf {quoted}")).await;
        client::drop_for_server(&state, "sftp-edge").await;

        assert_eq!(status, 0, "fixture cleanup failed: {output}");
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}

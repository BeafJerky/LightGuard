use std::time::Duration;

use russh_sftp::client::fs::DirEntry;
use russh_sftp::client::SftpSession;

use super::super::error::SshError;
use super::super::state::AppState;
use super::client::{acquire, map_error, with_timeout};
use super::dto::{file_entry, FileEntry, FileKind, FileListing};
use super::path::{basename, validate_remote_path};

/// Upper bound for one listing. A directory larger than this is returned
/// truncated instead of being rejected or exhausting memory.
pub const MAX_ENTRIES: usize = 20_000;

const LIST_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn home(state: &AppState, server_id: &str) -> Result<String, SshError> {
    let client = acquire(state, server_id).await?;
    home_with(&client).await
}

pub async fn list(state: &AppState, server_id: &str, path: &str) -> Result<FileListing, SshError> {
    validate_remote_path(path)?;
    let client = acquire(state, server_id).await?;
    list_with(&client, path).await
}

pub async fn stat(state: &AppState, server_id: &str, path: &str) -> Result<FileEntry, SshError> {
    validate_remote_path(path)?;
    let client = acquire(state, server_id).await?;
    stat_with(&client, path).await
}

/// The remote home directory as reported by the server (`.` canonicalized).
async fn home_with(client: &SftpSession) -> Result<String, SshError> {
    with_timeout(LIST_TIMEOUT, "读取主目录超时", async {
        client
            .canonicalize(".")
            .await
            .map_err(|error| map_error(error, "."))
    })
    .await
}

/// Lists `path` on an already acquired session. `path` must have passed
/// [`validate_remote_path`] (the public entry points do that before acquiring).
async fn list_with(client: &SftpSession, path: &str) -> Result<FileListing, SshError> {
    list_limited(client, path, MAX_ENTRIES).await
}

/// Same as [`list_with`] with an explicit cap, so the truncation path is
/// reachable without a 20 000 entry fixture.
async fn list_limited(
    client: &SftpSession,
    path: &str,
    limit: usize,
) -> Result<FileListing, SshError> {
    let canonical = with_timeout(LIST_TIMEOUT, "解析路径超时", async {
        client
            .canonicalize(path)
            .await
            .map_err(|error| map_error(error, path))
    })
    .await?;

    let directory = with_timeout(LIST_TIMEOUT, "读取目录超时", async {
        client
            .read_dir(canonical.as_str())
            .await
            .map_err(|error| map_error(error, &canonical))
    })
    .await?;

    let (entries, truncated) = collect_entries(directory, limit);
    Ok(FileListing {
        path: path.to_string(),
        canonical_path: canonical,
        entries,
        truncated,
    })
}

/// Filters the synthetic `.`/`..` entries (russh-sftp already drops them, but a
/// peer may still send them) and stops after `limit` real entries, flagging the
/// listing as truncated when the directory has more.
fn collect_entries(
    directory: impl Iterator<Item = DirEntry>,
    limit: usize,
) -> (Vec<FileEntry>, bool) {
    let mut entries = Vec::new();
    let mut truncated = false;
    for item in directory {
        let name = item.file_name();
        if name == "." || name == ".." {
            continue;
        }
        if entries.len() >= limit {
            truncated = true;
            break;
        }
        let metadata = item.metadata();
        entries.push(file_entry(name, item.path(), &metadata));
    }
    (entries, truncated)
}

/// Stats `path` on an already acquired session. Symlinks keep their own kind
/// (from `lstat`) plus the target name, and report the resolved kind and size
/// when the target is reachable.
pub async fn stat_with(client: &SftpSession, path: &str) -> Result<FileEntry, SshError> {
    validate_remote_path(path)?;
    let metadata = with_timeout(LIST_TIMEOUT, "读取文件属性超时", async {
        client
            .symlink_metadata(path)
            .await
            .map_err(|error| map_error(error, path))
    })
    .await?;

    let mut entry = file_entry(basename(path), path.to_string(), &metadata);
    if entry.kind == FileKind::Symlink {
        // A dangling link is still a valid entry: a failed or timed-out probe
        // only leaves `symlink_target`/`target_kind` empty. Both calls share one
        // budget so a peer that stops answering cannot hang the caller.
        let probe = with_timeout(LIST_TIMEOUT, "解析符号链接超时", async {
            let target = client.read_link(path).await.ok();
            let resolved = client.metadata(path).await.ok();
            Ok::<_, SshError>((target, resolved))
        })
        .await;
        if let Ok((target, resolved)) = probe {
            entry.symlink_target = target;
            if let Some(resolved) = resolved {
                entry.target_kind = Some(FileKind::from_sftp(resolved.file_type()));
                entry.size = resolved.size.unwrap_or(entry.size);
            }
        }
    }
    Ok(entry)
}

#[cfg(test)]
mod tests {
    use std::future::Future;
    use std::path::PathBuf;

    use russh_sftp::protocol::{Attrs, File, FileAttributes, Handle, Name, Status, StatusCode};
    use russh_sftp::server::Handler;

    use super::*;
    use crate::ssh::state::AppState;

    const HOME: &str = "/home/user";

    #[test]
    fn entry_limit_is_twenty_thousand() {
        assert_eq!(MAX_ENTRIES, 20_000);
    }

    #[test]
    fn symlink_stat_shape_is_explicit() {
        let entry = FileEntry {
            name: "link".into(),
            path: "/tmp/link".into(),
            kind: FileKind::Symlink,
            size: 0,
            mtime: None,
            mode: 0o777,
            mode_text: "lrwxrwxrwx".into(),
            uid: Some(0),
            gid: Some(0),
            symlink_target: Some("/etc".into()),
            target_kind: Some(FileKind::Dir),
        };
        assert_eq!(entry.target_kind, Some(FileKind::Dir));
    }

    #[tokio::test]
    async fn resolves_home_directory() {
        let client = fake_session().await;
        assert_eq!(home_with(&client).await.expect("home"), HOME);
    }

    #[tokio::test]
    async fn lists_directory_and_skips_dot_entries() {
        let client = fake_session().await;
        let listing = list_with(&client, HOME).await.expect("listing");

        assert_eq!(listing.path, HOME);
        assert_eq!(listing.canonical_path, HOME);
        assert!(!listing.truncated);

        let names: Vec<&str> = listing
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert_eq!(names, ["a.txt", "sub", "link"]);

        let file = &listing.entries[0];
        assert_eq!(file.path, "/home/user/a.txt");
        assert_eq!(file.kind, FileKind::File);
        assert_eq!(file.size, 12);
        assert_eq!(file.mode_text, "-rw-r--r--");
        assert_eq!(file.mtime, Some(1_700_000_000_000));
        assert_eq!(listing.entries[1].kind, FileKind::Dir);
        assert_eq!(listing.entries[2].kind, FileKind::Symlink);
        assert_eq!(listing.entries[2].mode_text, "lrwxrwxrwx");
    }

    #[tokio::test]
    async fn caps_entries_and_reports_truncation() {
        let client = fake_session().await;
        let listing = list_limited(&client, HOME, 2).await.expect("listing");

        let names: Vec<&str> = listing
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert_eq!(names, ["a.txt", "sub"]);
        assert!(listing.truncated);
    }

    #[tokio::test]
    async fn stat_follows_symlink_but_keeps_link_metadata() {
        let client = fake_session().await;
        let entry = stat_with(&client, "/home/user/link").await.expect("stat");

        assert_eq!(entry.name, "link");
        assert_eq!(entry.path, "/home/user/link");
        assert_eq!(entry.kind, FileKind::Symlink);
        assert_eq!(entry.mode, 0o777);
        assert_eq!(entry.mode_text, "lrwxrwxrwx");
        assert_eq!(entry.symlink_target.as_deref(), Some("/etc"));
        assert_eq!(entry.target_kind, Some(FileKind::Dir));
        assert_eq!(entry.size, 4_096);
    }

    #[tokio::test]
    async fn stat_reports_plain_file_without_resolving_links() {
        let client = fake_session().await;
        let entry = stat_with(&client, "/home/user/a.txt").await.expect("stat");

        assert_eq!(entry.kind, FileKind::File);
        assert_eq!(entry.size, 12);
        assert_eq!(entry.symlink_target, None);
        assert_eq!(entry.target_kind, None);
    }

    #[tokio::test]
    async fn stat_keeps_link_entry_when_target_probe_fails() {
        let client = fake_session().await;
        // `/home/user/broken`: LSTAT succeeds, READLINK and STAT return an error.
        // The link is still a valid entry; only the probe results are missing.
        let entry = stat_with(&client, "/home/user/broken")
            .await
            .expect("broken link is still an entry");

        assert_eq!(entry.name, "broken");
        assert_eq!(entry.path, "/home/user/broken");
        assert_eq!(entry.kind, FileKind::Symlink);
        assert_eq!(entry.mode_text, "lrwxrwxrwx");
        assert_eq!(entry.size, 0);
        assert_eq!(entry.symlink_target, None);
        assert_eq!(entry.target_kind, None);
    }

    #[tokio::test]
    async fn maps_missing_and_denied_paths() {
        let client = fake_session().await;

        let missing = stat_with(&client, "/home/user/missing")
            .await
            .expect_err("missing file");
        assert_eq!(missing.code, "FILE_NOT_FOUND");

        let denied = list_with(&client, "/denied").await.expect_err("denied dir");
        assert_eq!(denied.code, "PERMISSION_DENIED");

        let absent = list_with(&client, "/nope").await.expect_err("absent dir");
        assert_eq!(absent.code, "FILE_NOT_FOUND");
    }

    #[tokio::test]
    async fn stat_with_rejects_invalid_paths_without_a_round_trip() {
        let client = fake_session().await;
        let invalid = stat_with(&client, "a\0b").await.expect_err("nul path");
        assert_eq!(invalid.code, "PATH_INVALID");
    }

    #[tokio::test]
    async fn rejects_invalid_paths_before_opening_a_session() {
        let state = AppState::new(PathBuf::from("."));

        let empty = list(&state, "missing", "  ").await.expect_err("empty path");
        assert_eq!(empty.code, "PATH_INVALID");

        let control = stat(&state, "missing", "a\0b").await.expect_err("nul path");
        assert_eq!(control.code, "PATH_INVALID");

        // A valid path gets past validation and reaches the session registry.
        let unreachable = list(&state, "missing", "/").await.expect_err("no session");
        assert_eq!(unreachable.code, "SESSION_NOT_FOUND");
    }

    fn attrs(permissions: u32, size: u64) -> FileAttributes {
        FileAttributes {
            size: Some(size),
            uid: Some(1000),
            gid: Some(1000),
            permissions: Some(permissions),
            mtime: Some(1_700_000_000),
            ..FileAttributes::empty()
        }
    }

    fn lstat_attrs(path: &str) -> Option<FileAttributes> {
        match path {
            "/home/user/a.txt" => Some(attrs(0o100644, 12)),
            "/home/user/sub" => Some(attrs(0o040755, 4_096)),
            // A link reports its own kind and length, not the target's.
            "/home/user/link" => Some(attrs(0o120777, 0)),
            // Dangling link: it exists, its target does not.
            "/home/user/broken" => Some(attrs(0o120777, 0)),
            _ => None,
        }
    }

    fn stat_attrs(path: &str) -> Option<FileAttributes> {
        match path {
            // `link` resolves to a directory, so following it changes kind and size.
            "/home/user/link" => Some(attrs(0o040755, 4_096)),
            // `broken` cannot be resolved, so a following STAT fails.
            "/home/user/broken" => None,
            other => lstat_attrs(other),
        }
    }

    fn name(id: u32, filename: &str) -> Name {
        Name {
            id,
            files: vec![File::dummy(filename)],
        }
    }

    /// In-memory SFTP peer: answers REALPATH, OPENDIR, READDIR, CLOSE, LSTAT,
    /// STAT and READLINK only, so the client code under test talks the real
    /// wire protocol and any write attempt would fail loudly.
    struct FakeFs {
        readdir_sent: bool,
    }

    impl Handler for FakeFs {
        type Error = StatusCode;

        fn unimplemented(&self) -> StatusCode {
            StatusCode::OpUnsupported
        }

        fn realpath(
            &mut self,
            id: u32,
            path: String,
        ) -> impl Future<Output = Result<Name, Self::Error>> + Send {
            async move {
                match path.as_str() {
                    "." => Ok(name(id, HOME)),
                    "/home/user" => Ok(name(id, HOME)),
                    "/denied" => Ok(name(id, "/denied")),
                    "/home/user/sub" => Ok(name(id, "/home/user/sub")),
                    _ => Err(StatusCode::NoSuchFile),
                }
            }
        }

        fn opendir(
            &mut self,
            id: u32,
            path: String,
        ) -> impl Future<Output = Result<Handle, Self::Error>> + Send {
            async move {
                match path.as_str() {
                    "/home/user" => Ok(Handle {
                        id,
                        handle: "dir-1".to_string(),
                    }),
                    "/denied" => Err(StatusCode::PermissionDenied),
                    _ => Err(StatusCode::NoSuchFile),
                }
            }
        }

        fn readdir(
            &mut self,
            id: u32,
            handle: String,
        ) -> impl Future<Output = Result<Name, Self::Error>> + Send {
            async move {
                if handle != "dir-1" {
                    return Err(StatusCode::Failure);
                }
                if self.readdir_sent {
                    return Err(StatusCode::Eof);
                }
                self.readdir_sent = true;
                // Servers commonly include "." and ".." in listings.
                Ok(Name {
                    id,
                    files: vec![
                        File::new(".", attrs(0o040755, 4_096)),
                        File::new("..", attrs(0o040755, 4_096)),
                        File::new("a.txt", attrs(0o100644, 12)),
                        File::new("sub", attrs(0o040755, 4_096)),
                        File::new("link", attrs(0o120777, 0)),
                    ],
                })
            }
        }

        fn close(
            &mut self,
            id: u32,
            handle: String,
        ) -> impl Future<Output = Result<Status, Self::Error>> + Send {
            async move {
                if handle == "dir-1" {
                    Ok(Status {
                        id,
                        status_code: StatusCode::Ok,
                        error_message: String::new(),
                        language_tag: String::new(),
                    })
                } else {
                    Err(StatusCode::Failure)
                }
            }
        }

        fn lstat(
            &mut self,
            id: u32,
            path: String,
        ) -> impl Future<Output = Result<Attrs, Self::Error>> + Send {
            async move {
                lstat_attrs(&path)
                    .map(|attrs| Attrs { id, attrs })
                    .ok_or(StatusCode::NoSuchFile)
            }
        }

        fn stat(
            &mut self,
            id: u32,
            path: String,
        ) -> impl Future<Output = Result<Attrs, Self::Error>> + Send {
            async move {
                stat_attrs(&path)
                    .map(|attrs| Attrs { id, attrs })
                    .ok_or(StatusCode::NoSuchFile)
            }
        }

        fn readlink(
            &mut self,
            id: u32,
            path: String,
        ) -> impl Future<Output = Result<Name, Self::Error>> + Send {
            async move {
                if path == "/home/user/link" {
                    Ok(name(id, "/etc"))
                } else {
                    Err(StatusCode::NoSuchFile)
                }
            }
        }
    }

    async fn fake_session() -> SftpSession {
        let (client, server) = tokio::io::duplex(8_192);
        let peer = FakeFs {
            readdir_sent: false,
        };
        // `server::run` only spawns the peer task, so awaiting it returns at once.
        russh_sftp::server::run(server, peer).await;
        SftpSession::new(client).await.expect("fake SFTP handshake")
    }
}

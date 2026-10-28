use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use russh::ChannelMsg;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use tauri_plugin_dialog::DialogExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::super::error::SshError;
use super::super::session::ClientHandler;
use super::client::map_error;
use super::path::{shell_quote, validate_remote_path};

const CHUNK: usize = 64 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const STDERR_LIMIT: usize = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferOutcome {
    Saved { bytes: u64 },
    Canceled { bytes: u64 },
}

pub fn part_path(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_os_string();
    name.push(".lgpart");
    PathBuf::from(name)
}

pub fn tar_command(parent: &str, base: &str) -> String {
    format!(
        "tar -C {} -czf - -- {}",
        shell_quote(parent),
        shell_quote(base)
    )
}

pub fn split_for_tar(canonical: &str) -> Result<(String, String), SshError> {
    let trimmed = canonical.trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(SshError::path_invalid("根目录不能整体打包"));
    }
    match trimmed.rsplit_once('/') {
        Some(("", base)) if !base.is_empty() => Ok(("/".to_string(), base.to_string())),
        Some((parent, base)) if !base.is_empty() => Ok((parent.to_string(), base.to_string())),
        _ => Ok((".".to_string(), trimmed.to_string())),
    }
}

async fn finalize(target: &Path, part: &Path, bytes: u64) -> Result<TransferOutcome, SshError> {
    if target.exists() {
        if let Err(error) = tokio::fs::remove_file(target).await {
            discard(part).await;
            return Err(SshError::transfer_failed(format!(
                "无法覆盖目标文件：{error}"
            )));
        }
    }
    if let Err(error) = tokio::fs::rename(part, target).await {
        return Err(SshError::transfer_failed(format!(
            "临时文件已保留：{}（重命名失败：{error}）",
            part.display()
        )));
    }
    Ok(TransferOutcome::Saved { bytes })
}

async fn discard(part: &Path) {
    let _ = tokio::fs::remove_file(part).await;
}

pub async fn download_file(
    client: &SftpSession,
    remote_path: &str,
    target: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<TransferOutcome, SshError> {
    validate_remote_path(remote_path)?;
    let total = client
        .metadata(remote_path)
        .await
        .ok()
        .and_then(|meta| meta.size);
    let mut source = client
        .open_with_flags(remote_path, OpenFlags::READ)
        .await
        .map_err(|error| map_error(error, remote_path))?;
    let part = part_path(target);
    let mut sink = tokio::fs::File::create(&part)
        .await
        .map_err(|error| SshError::transfer_failed(format!("无法创建临时文件：{error}")))?;

    let mut buffer = vec![0u8; CHUNK];
    let mut transferred = 0u64;
    let mut last_emit = Instant::now();
    progress(0, total);

    loop {
        if cancel.load(Ordering::SeqCst) {
            drop(sink);
            discard(&part).await;
            return Ok(TransferOutcome::Canceled { bytes: transferred });
        }
        let read = match tokio::time::timeout(IDLE_TIMEOUT, source.read(&mut buffer)).await {
            Ok(Ok(count)) => count,
            Ok(Err(error)) => {
                drop(sink);
                discard(&part).await;
                return Err(SshError::transfer_failed(format!(
                    "读取远端文件失败：{error}"
                )));
            }
            Err(_) => {
                drop(sink);
                discard(&part).await;
                return Err(SshError::transfer_failed("传输空闲超时（60 秒）"));
            }
        };
        if read == 0 {
            break;
        }
        if let Err(error) = sink.write_all(&buffer[..read]).await {
            drop(sink);
            discard(&part).await;
            return Err(SshError::transfer_failed(format!(
                "写入本地文件失败：{error}"
            )));
        }
        transferred += read as u64;
        if last_emit.elapsed() >= PROGRESS_INTERVAL {
            progress(transferred, total);
            last_emit = Instant::now();
        }
    }

    if let Err(error) = sink.flush().await {
        drop(sink);
        discard(&part).await;
        return Err(SshError::transfer_failed(format!(
            "刷新本地文件失败：{error}"
        )));
    }
    if let Err(error) = sink.sync_all().await {
        drop(sink);
        discard(&part).await;
        return Err(SshError::transfer_failed(format!("写入磁盘失败：{error}")));
    }
    drop(sink);

    if cancel.load(Ordering::SeqCst) {
        discard(&part).await;
        return Ok(TransferOutcome::Canceled { bytes: transferred });
    }
    progress(transferred, total);
    finalize(target, &part, transferred).await
}

enum Stop {
    Cancel,
    Idle,
    Done(Option<u32>),
}

pub async fn download_dir(
    handle: &russh::client::Handle<ClientHandler>,
    client: &SftpSession,
    remote_path: &str,
    target: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<TransferOutcome, SshError> {
    validate_remote_path(remote_path)?;
    let canonical = client
        .canonicalize(remote_path)
        .await
        .map_err(|error| map_error(error, remote_path))?;
    let (parent, base) = split_for_tar(&canonical)?;
    let command = tar_command(&parent, &base);

    let mut channel = handle
        .channel_open_session()
        .await
        .map_err(|error| SshError::transfer_failed(format!("打开打包通道失败：{error}")))?;
    channel
        .exec(true, command)
        .await
        .map_err(|error| SshError::transfer_failed(format!("启动远端打包失败：{error}")))?;

    let part = part_path(target);
    let mut sink = tokio::fs::File::create(&part)
        .await
        .map_err(|error| SshError::transfer_failed(format!("无法创建临时文件：{error}")))?;

    let mut stderr = Vec::new();
    let mut transferred = 0u64;
    let mut last_emit = Instant::now();
    let mut last_data = Instant::now();
    let mut ticker = tokio::time::interval(Duration::from_millis(200));
    progress(0, None);

    let stop = loop {
        tokio::select! {
            _ = ticker.tick() => {
                if cancel.load(Ordering::SeqCst) {
                    break Stop::Cancel;
                }
                if last_data.elapsed() >= IDLE_TIMEOUT {
                    break Stop::Idle;
                }
            }
            message = channel.wait() => match message {
                Some(ChannelMsg::Data { data }) => {
                    if let Err(error) = sink.write_all(&data).await {
                        let _ = channel.close().await;
                        drop(sink);
                        discard(&part).await;
                        return Err(SshError::transfer_failed(format!("写入本地文件失败：{error}")));
                    }
                    transferred += data.len() as u64;
                    last_data = Instant::now();
                    if last_emit.elapsed() >= PROGRESS_INTERVAL {
                        progress(transferred, None);
                        last_emit = Instant::now();
                    }
                }
                Some(ChannelMsg::ExtendedData { data, .. }) => {
                    if stderr.len() < STDERR_LIMIT {
                        let room = STDERR_LIMIT - stderr.len();
                        stderr.extend_from_slice(&data[..data.len().min(room)]);
                    }
                }
                Some(ChannelMsg::ExitStatus { exit_status }) => break Stop::Done(Some(exit_status)),
                Some(ChannelMsg::Close) | Some(ChannelMsg::Eof) | None => break Stop::Done(None),
                Some(_) => {}
            }
        }
    };

    match stop {
        Stop::Cancel => {
            channel.close().await.ok();
            drop(sink);
            discard(&part).await;
            Ok(TransferOutcome::Canceled { bytes: transferred })
        }
        Stop::Idle => {
            channel.close().await.ok();
            drop(sink);
            discard(&part).await;
            Err(SshError::transfer_failed("远端打包空闲超时（60 秒）"))
        }
        Stop::Done(code) => {
            let flushed = sink.flush().await;
            let synced = match flushed {
                Ok(()) => sink.sync_all().await,
                Err(error) => Err(error),
            };
            drop(sink);
            if let Err(error) = synced {
                discard(&part).await;
                return Err(SshError::transfer_failed(format!("写入磁盘失败：{error}")));
            }
            if let Some(exit_status) = code {
                if exit_status != 0 {
                    discard(&part).await;
                    let detail = String::from_utf8_lossy(&stderr);
                    let detail = detail.trim();
                    return Err(SshError::transfer_failed(if detail.is_empty() {
                        format!("远端 tar 退出码 {exit_status}")
                    } else {
                        format!("远端 tar 退出码 {exit_status}：{detail}")
                    }));
                }
            }
            if transferred == 0 {
                discard(&part).await;
                return Err(SshError::transfer_failed("远端未返回打包内容"));
            }
            progress(transferred, None);
            finalize(target, &part, transferred).await
        }
    }
}

pub async fn pick_save_path(app: &tauri::AppHandle, file_name: String) -> Option<PathBuf> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let app_for_thread = app.clone();
    let dispatched = app.run_on_main_thread(move || {
        app_for_thread
            .dialog()
            .file()
            .set_title("保存到本地")
            .set_file_name(file_name)
            .save_file(move |path| {
                let _ = sender.send(path);
            });
    });
    if dispatched.is_err() {
        return None;
    }
    match tokio::time::timeout(Duration::from_secs(300), receiver).await {
        Ok(Ok(Some(file))) => file.into_path().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::future::Future;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    use russh_sftp::client::SftpSession;
    use russh_sftp::protocol::{
        Attrs, Data, FileAttributes, Handle, OpenFlags, Status, StatusCode,
    };
    use russh_sftp::server::Handler;

    use super::*;

    const REMOTE: &str = "/home/user/report.bin";

    fn payload() -> Vec<u8> {
        (0..CHUNK * 2 + 17)
            .map(|index| (index % 251) as u8)
            .collect()
    }

    // ---- path and command helpers -------------------------------------------

    #[test]
    fn appends_part_suffix() {
        let target = Path::new("C:/tmp/report.txt");
        assert_eq!(part_path(target), PathBuf::from("C:/tmp/report.txt.lgpart"));
    }

    #[test]
    fn part_path_keeps_hostile_local_names_intact() {
        let target = Path::new("C:/tmp/报告 $(x) 'quoted'.tar.gz");
        assert_eq!(
            part_path(target),
            PathBuf::from("C:/tmp/报告 $(x) 'quoted'.tar.gz.lgpart")
        );
        // The part file never collides with the final name.
        assert_ne!(part_path(target), target.to_path_buf());
    }

    #[test]
    fn builds_fixed_tar_command() {
        assert_eq!(
            tar_command("/var/log", "nginx"),
            "tar -C '/var/log' -czf - -- 'nginx'"
        );
        assert_eq!(
            tar_command("/tmp/a b", "it's"),
            "tar -C '/tmp/a b' -czf - -- 'it'\\''s'"
        );
    }

    #[test]
    fn quotes_hostile_tar_arguments() {
        // `..` stays a single quoted argument and cannot escape `-C`.
        assert_eq!(tar_command("/", ".."), "tar -C '/' -czf - -- '..'");
        assert_eq!(
            tar_command("/tmp/x", "$(rm -rf /); `id`"),
            "tar -C '/tmp/x' -czf - -- '$(rm -rf /); `id`'"
        );
        assert_eq!(
            tar_command("/a'b", "c'd"),
            "tar -C '/a'\\''b' -czf - -- 'c'\\''d'"
        );
    }

    #[test]
    fn splits_canonical_paths() {
        assert_eq!(
            split_for_tar("/var/log/nginx").unwrap(),
            ("/var/log".to_string(), "nginx".to_string())
        );
        assert_eq!(
            split_for_tar("/etc").unwrap(),
            ("/".to_string(), "etc".to_string())
        );
        assert!(split_for_tar("/").is_err());
    }

    #[test]
    fn splits_hostile_canonical_paths() {
        // A trailing slash is dropped before the split, so the parent is the
        // last real segment's parent (canonical paths never keep the slash).
        assert_eq!(
            split_for_tar("/var/log/").unwrap(),
            ("/var".to_string(), "log".to_string())
        );
        assert_eq!(
            split_for_tar("/var/log/../etc").unwrap(),
            ("/var/log/..".to_string(), "etc".to_string())
        );
        assert_eq!(
            split_for_tar("logs").unwrap(),
            (".".to_string(), "logs".to_string())
        );
        assert_eq!(
            split_for_tar("/tmp/a b").unwrap(),
            ("/tmp".to_string(), "a b".to_string())
        );
        assert!(split_for_tar("///").is_err());
        assert_eq!(split_for_tar("/").unwrap_err().code, "PATH_INVALID");
    }

    // ---- single file streaming over a fake SFTP peer ------------------------

    #[tokio::test]
    async fn streams_file_to_target_and_replaces_existing_file() {
        let peer = FakePeer::new();
        let client = peer.session().await;
        let directory = tempfile::tempdir().expect("temp dir");
        let target = directory.path().join("report final.bin");
        tokio::fs::write(&target, b"stale")
            .await
            .expect("pre-existing target");

        let cancel = AtomicBool::new(false);
        let mut calls = Vec::new();
        let outcome = download_file(&client, REMOTE, &target, &cancel, |done, total| {
            calls.push((done, total));
        })
        .await
        .expect("download succeeds");

        let bytes = payload().len() as u64;
        assert_eq!(outcome, TransferOutcome::Saved { bytes });
        assert_eq!(tokio::fs::read(&target).await.expect("target"), payload());
        assert!(!part_path(&target).exists(), "part file must be renamed");
        assert_eq!(calls.first(), Some(&(0, Some(bytes))));
        assert_eq!(calls.last(), Some(&(bytes, Some(bytes))));
        assert_read_only(&peer.opened());
    }

    #[tokio::test]
    async fn cancel_before_first_chunk_removes_the_part_file() {
        let peer = FakePeer::new();
        let client = peer.session().await;
        let directory = tempfile::tempdir().expect("temp dir");
        let target = directory.path().join("report.bin");

        let cancel = AtomicBool::new(true);
        let outcome = download_file(&client, REMOTE, &target, &cancel, |_, _| {})
            .await
            .expect("cancel is not an error");

        assert_eq!(outcome, TransferOutcome::Canceled { bytes: 0 });
        assert!(!target.exists(), "canceled download leaves no target");
        assert!(!part_path(&target).exists(), "part file must be deleted");
    }

    #[tokio::test]
    async fn read_failure_removes_the_part_file() {
        let peer = FakePeer::failing_read();
        let client = peer.session().await;
        let directory = tempfile::tempdir().expect("temp dir");
        let target = directory.path().join("report.bin");

        let cancel = AtomicBool::new(false);
        let error = download_file(&client, REMOTE, &target, &cancel, |_, _| {})
            .await
            .expect_err("read failure");

        assert_eq!(error.code, "TRANSFER_FAILED");
        assert!(error.message.contains("读取远端文件失败"));
        assert!(!target.exists());
        assert!(!part_path(&target).exists(), "part file must be deleted");
    }

    #[tokio::test]
    async fn missing_remote_file_leaves_no_trace() {
        let peer = FakePeer::new();
        let client = peer.session().await;
        let directory = tempfile::tempdir().expect("temp dir");
        let target = directory.path().join("missing.bin");

        let cancel = AtomicBool::new(false);
        let error = download_file(
            &client,
            "/home/user/missing.bin",
            &target,
            &cancel,
            |_, _| {},
        )
        .await
        .expect_err("missing file");

        assert_eq!(error.code, "FILE_NOT_FOUND");
        assert!(!target.exists());
        assert!(!part_path(&target).exists());
    }

    #[tokio::test]
    async fn rejects_invalid_remote_paths_before_writing() {
        let peer = FakePeer::new();
        let client = peer.session().await;
        let directory = tempfile::tempdir().expect("temp dir");
        let target = directory.path().join("report.bin");

        let cancel = AtomicBool::new(false);
        let error = download_file(&client, "/home/user/a\0b", &target, &cancel, |_, _| {})
            .await
            .expect_err("nul byte");

        assert_eq!(error.code, "PATH_INVALID");
        assert!(peer.opened().is_empty());
        assert!(!part_path(&target).exists());
    }

    fn assert_read_only(flags: &[OpenFlags]) {
        assert!(!flags.is_empty(), "download never opened the file");
        for flags in flags {
            assert!(flags.contains(OpenFlags::READ));
            assert!(!flags.contains(OpenFlags::WRITE));
            assert!(!flags.contains(OpenFlags::APPEND));
            assert!(!flags.contains(OpenFlags::CREATE));
            assert!(!flags.contains(OpenFlags::TRUNCATE));
            assert!(!flags.contains(OpenFlags::EXCLUDE));
        }
    }

    // ---- in-memory SFTP peer ------------------------------------------------

    struct FakePeer {
        opened: Arc<Mutex<Vec<OpenFlags>>>,
        files: HashMap<String, Vec<u8>>,
        fail_read: bool,
    }

    impl FakePeer {
        fn new() -> Self {
            let mut files = HashMap::new();
            files.insert(REMOTE.to_string(), payload());
            Self {
                opened: Arc::new(Mutex::new(Vec::new())),
                files,
                fail_read: false,
            }
        }

        fn failing_read() -> Self {
            Self {
                fail_read: true,
                ..Self::new()
            }
        }

        async fn session(&self) -> SftpSession {
            let (client, server) = tokio::io::duplex(64 * 1024);
            let peer = Peer {
                opened: self.opened.clone(),
                files: self.files.clone(),
                fail_read: self.fail_read,
            };
            russh_sftp::server::run(server, peer).await;
            SftpSession::new(client).await.expect("fake SFTP handshake")
        }

        fn opened(&self) -> Vec<OpenFlags> {
            self.opened.lock().expect("open log").clone()
        }
    }

    /// In-memory SFTP peer: answers STAT, OPEN, READ and CLOSE only, so the
    /// download must stream through the real wire protocol with a read handle.
    struct Peer {
        opened: Arc<Mutex<Vec<OpenFlags>>>,
        files: HashMap<String, Vec<u8>>,
        fail_read: bool,
    }

    impl Handler for Peer {
        type Error = StatusCode;

        fn unimplemented(&self) -> StatusCode {
            StatusCode::OpUnsupported
        }

        fn stat(
            &mut self,
            id: u32,
            path: String,
        ) -> impl Future<Output = Result<Attrs, Self::Error>> + Send {
            let attrs = self.files.get(&path).map(|bytes| FileAttributes {
                size: Some(bytes.len() as u64),
                permissions: Some(0o100644),
                ..FileAttributes::empty()
            });
            async move {
                attrs
                    .map(|attrs| Attrs { id, attrs })
                    .ok_or(StatusCode::NoSuchFile)
            }
        }

        fn open(
            &mut self,
            id: u32,
            filename: String,
            pflags: OpenFlags,
            _attrs: FileAttributes,
        ) -> impl Future<Output = Result<Handle, Self::Error>> + Send {
            self.opened.lock().expect("open log").push(pflags);
            let exists = self.files.contains_key(&filename);
            async move {
                if exists {
                    Ok(Handle {
                        id,
                        handle: filename,
                    })
                } else {
                    Err(StatusCode::NoSuchFile)
                }
            }
        }

        fn read(
            &mut self,
            id: u32,
            handle: String,
            offset: u64,
            len: u32,
        ) -> impl Future<Output = Result<Data, Self::Error>> + Send {
            let fail = self.fail_read;
            let data = self.files.get(&handle).map(|bytes| {
                let start = (offset as usize).min(bytes.len());
                let end = (start + len as usize).min(bytes.len());
                bytes[start..end].to_vec()
            });
            async move {
                if fail {
                    return Err(StatusCode::Failure);
                }
                match data {
                    Some(data) => Ok(Data { id, data }),
                    None => Err(StatusCode::NoSuchFile),
                }
            }
        }

        fn close(
            &mut self,
            id: u32,
            _handle: String,
        ) -> impl Future<Output = Result<Status, Self::Error>> + Send {
            async move {
                Ok(Status {
                    id,
                    status_code: StatusCode::Ok,
                    error_message: String::new(),
                    language_tag: String::new(),
                })
            }
        }
    }
}

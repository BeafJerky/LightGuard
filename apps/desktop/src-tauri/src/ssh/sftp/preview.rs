use std::time::Duration;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use tokio::io::AsyncReadExt;

use super::super::error::SshError;
use super::super::state::AppState;
use super::client::{map_error, with_retry, with_timeout};
use super::dto::{FileKind, ImagePreview, TextPreview};
use super::list::stat_with;
use super::path::{image_mime, validate_remote_path};

/// Largest text prefix returned to the UI; longer files are truncated on a
/// character boundary instead of being rejected.
pub const TEXT_LIMIT: u64 = 1_048_576;
/// Largest image that may be read and base64-encoded into the response.
pub const IMAGE_LIMIT: u64 = 10 * 1_048_576;
/// Only the first 8 KiB are sniffed for NUL bytes when classifying a file.
const SNIFF_LEN: usize = 8_192;
const PREVIEW_TIMEOUT: Duration = Duration::from_secs(20);
/// Initial buffer size; larger payloads grow as they are read instead of
/// reserving the full limit up front.
const READ_BUFFER_HINT: usize = 64 * 1024;

pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(SNIFF_LEN).any(|byte| *byte == 0)
}

pub fn decode_text(bytes: &[u8]) -> (String, &'static str) {
    match std::str::from_utf8(bytes) {
        Ok(text) => (text.to_string(), "utf-8"),
        Err(_) => {
            let (text, _, _) = encoding_rs::GB18030.decode(bytes);
            (text.into_owned(), "gb18030")
        }
    }
}

/// Decodes the first `TEXT_LIMIT` bytes, backing up at most three bytes so the
/// cut never splits a UTF-8 character (four-byte sequences can lose up to
/// three continuation bytes).
fn decode_preview(bytes: &[u8]) -> (String, &'static str) {
    let limit = TEXT_LIMIT as usize;
    if bytes.len() <= limit {
        return decode_text(bytes);
    }
    let mut end = limit;
    let floor = end.saturating_sub(3);
    while end > floor && std::str::from_utf8(&bytes[..end]).is_err() {
        end -= 1;
    }
    decode_text(&bytes[..end])
}

/// Reads at most `limit` bytes of `path` with a read-only handle. The cap is
/// enforced by the reader itself, so a peer that reports the wrong size or
/// keeps sending data cannot exhaust memory.
async fn read_prefix(client: &SftpSession, path: &str, limit: u64) -> Result<Vec<u8>, SshError> {
    let file = client
        .open_with_flags(path, OpenFlags::READ)
        .await
        .map_err(|error| map_error(error, path))?;
    let mut limited = file.take(limit);
    let mut buffer = Vec::with_capacity((limit as usize).min(READ_BUFFER_HINT));
    limited
        .read_to_end(&mut buffer)
        .await
        .map_err(|error| SshError::sftp_failed(format!("读取文件失败：{error}")))?;
    Ok(buffer)
}

pub async fn text(state: &AppState, server_id: &str, path: &str) -> Result<TextPreview, SshError> {
    validate_remote_path(path)?;
    with_retry(state, server_id, |client| async move {
        text_with(&client, path).await
    })
    .await
}

async fn text_with(client: &SftpSession, path: &str) -> Result<TextPreview, SshError> {
    validate_remote_path(path)?;
    let entry = stat_with(client, path).await?;
    if matches!(entry.target_kind.unwrap_or(entry.kind), FileKind::Dir) {
        return Err(SshError::path_invalid("目录无法预览内容"));
    }

    let size = entry.size;
    // Four extra bytes let the truncation path see whether a multi-byte
    // character straddles the limit without ever reading the whole file.
    let read_limit = size.min(TEXT_LIMIT + 4);
    let bytes = with_timeout(PREVIEW_TIMEOUT, "读取文件超时", async {
        read_prefix(client, path, read_limit).await
    })
    .await?;

    if looks_binary(&bytes) {
        return Err(SshError::file_binary());
    }
    let (content, encoding) = decode_preview(&bytes);
    Ok(TextPreview {
        content,
        encoding,
        bytes: size,
        truncated: size > TEXT_LIMIT,
    })
}

pub async fn image(
    state: &AppState,
    server_id: &str,
    path: &str,
) -> Result<ImagePreview, SshError> {
    validate_remote_path(path)?;
    // Reject unsupported formats before touching the session registry.
    if image_mime(path).is_none() {
        return Err(SshError::path_invalid("不支持预览该图片格式"));
    }
    with_retry(state, server_id, |client| async move {
        image_with(&client, path).await
    })
    .await
}

async fn image_with(client: &SftpSession, path: &str) -> Result<ImagePreview, SshError> {
    validate_remote_path(path)?;
    let mime = image_mime(path).ok_or_else(|| SshError::path_invalid("不支持预览该图片格式"))?;
    let entry = stat_with(client, path).await?;
    if entry.size > IMAGE_LIMIT {
        return Err(SshError::file_too_large(format!(
            "图片超过 10MB 上限（{} 字节）",
            entry.size
        )));
    }

    // One byte past the limit keeps the read bounded while still detecting a
    // peer whose reported size understates the real payload.
    let bytes = with_timeout(PREVIEW_TIMEOUT, "读取图片超时", async {
        read_prefix(client, path, IMAGE_LIMIT + 1).await
    })
    .await?;
    if bytes.len() as u64 > IMAGE_LIMIT {
        return Err(SshError::file_too_large("图片超过 10MB 上限"));
    }
    Ok(ImagePreview {
        mime: mime.to_string(),
        base64: BASE64.encode(&bytes),
        bytes: bytes.len() as u64,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::future::Future;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use russh_sftp::client::SftpSession;
    use russh_sftp::protocol::{
        Attrs, Data, FileAttributes, Handle, OpenFlags, Status, StatusCode,
    };
    use russh_sftp::server::Handler;

    use super::*;
    use crate::ssh::state::AppState;

    const NOTE: &str = "你好，世界";
    const GBK: [u8; 4] = [0xD6, 0xD0, 0xCE, 0xC4];
    const PNG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

    // ---- decode and limit helpers -------------------------------------------

    #[test]
    fn detects_binary_by_nul() {
        assert!(looks_binary(b"hello\0world"));
        assert!(!looks_binary("你好，世界".as_bytes()));
    }

    #[test]
    fn decodes_utf8_and_gb18030() {
        assert_eq!(
            decode_text("你好".as_bytes()),
            ("你好".to_string(), "utf-8")
        );
        let gb = [0xD6, 0xD0, 0xCE, 0xC4];
        assert_eq!(decode_text(&gb), ("中文".to_string(), "gb18030"));
    }

    #[test]
    fn trims_partial_utf8_at_limit() {
        let mut bytes = vec![b'a'; (TEXT_LIMIT - 1) as usize];
        bytes.extend_from_slice("你".as_bytes());
        let (text, encoding) = decode_preview(&bytes);
        assert_eq!(encoding, "utf-8");
        assert_eq!(text.chars().count(), TEXT_LIMIT as usize - 1);
    }

    #[test]
    fn image_allowlist_is_extension_based() {
        assert!(image_mime("a.png").is_some());
        assert!(image_mime("a.svg").is_none());
    }

    #[test]
    fn limits_match_the_spec() {
        assert_eq!(TEXT_LIMIT, 1_048_576);
        assert_eq!(IMAGE_LIMIT, 10_485_760);
    }

    // ---- text preview over a fake SFTP peer ---------------------------------

    #[tokio::test]
    async fn previews_utf8_text_over_sftp() {
        let peer = FakePeer::new();
        let client = peer.session().await;

        let preview = text_with(&client, "/home/user/note.txt")
            .await
            .expect("utf-8 preview");

        assert_eq!(preview.content, NOTE);
        assert_eq!(preview.encoding, "utf-8");
        assert_eq!(preview.bytes, NOTE.len() as u64);
        assert!(!preview.truncated);
        assert_read_only(&peer.opened());
    }

    #[tokio::test]
    async fn previews_gb18030_text_over_sftp() {
        let peer = FakePeer::new();
        let client = peer.session().await;

        let preview = text_with(&client, "/home/user/gbk.txt")
            .await
            .expect("gb18030 preview");

        assert_eq!(preview.content, "中文");
        assert_eq!(preview.encoding, "gb18030");
        assert!(!preview.truncated);
    }

    #[tokio::test]
    async fn rejects_binary_files_over_sftp() {
        let peer = FakePeer::new();
        let client = peer.session().await;

        let error = text_with(&client, "/home/user/blob.bin")
            .await
            .expect_err("NUL byte means binary");

        assert_eq!(error.code, "FILE_BINARY");
        // The sniff is limited to the first 8 KiB and never opens for writing.
        assert_read_only(&peer.opened());
    }

    #[tokio::test]
    async fn truncates_text_at_a_char_boundary_over_sftp() {
        let peer = FakePeer::new();
        let client = peer.session().await;

        let preview = text_with(&client, "/home/user/large.txt")
            .await
            .expect("truncated preview");

        assert!(preview.truncated);
        assert_eq!(preview.bytes, TEXT_LIMIT + 2);
        assert_eq!(preview.encoding, "utf-8");
        // `large.txt` is `TEXT_LIMIT - 1` ASCII bytes plus a 3-byte char; the
        // cut must drop the partial char, not four bytes of content.
        assert_eq!(preview.content.chars().count(), TEXT_LIMIT as usize - 1);
        assert!(preview.content.chars().all(|character| character == 'a'));
    }

    #[tokio::test]
    async fn read_prefix_never_reads_past_the_cap() {
        let peer = FakePeer::new();
        let client = peer.session().await;

        let bytes = read_prefix(&client, "/home/user/large.txt", 16)
            .await
            .expect("prefix");

        assert_eq!(bytes, b"aaaaaaaaaaaaaaaa".to_vec());
    }

    #[tokio::test]
    async fn refuses_to_preview_directories() {
        let peer = FakePeer::new();
        let client = peer.session().await;

        let error = text_with(&client, "/home/user/sub")
            .await
            .expect_err("directory has no text preview");

        assert_eq!(error.code, "PATH_INVALID");
        assert!(error.message.contains("目录"));
        assert!(peer.opened().is_empty());
    }

    #[tokio::test]
    async fn maps_missing_files() {
        let peer = FakePeer::new();
        let client = peer.session().await;

        let error = text_with(&client, "/home/user/missing.txt")
            .await
            .expect_err("missing file");

        assert_eq!(error.code, "FILE_NOT_FOUND");
    }

    // ---- image preview over a fake SFTP peer --------------------------------

    #[tokio::test]
    async fn previews_image_as_exact_base64() {
        let peer = FakePeer::new();
        let client = peer.session().await;

        let preview = image_with(&client, "/home/user/pic.png")
            .await
            .expect("image preview");

        assert_eq!(preview.mime, "image/png");
        assert_eq!(preview.bytes, PNG.len() as u64);
        assert_eq!(
            BASE64.decode(preview.base64).expect("valid base64"),
            PNG.to_vec()
        );
        assert_read_only(&peer.opened());
    }

    #[tokio::test]
    async fn rejects_oversize_images_before_reading() {
        let peer = FakePeer::new();
        let client = peer.session().await;

        let error = image_with(&client, "/home/user/huge.png")
            .await
            .expect_err("oversize image");

        assert_eq!(error.code, "FILE_TOO_LARGE");
        assert!(error.message.contains("10MB"));
        assert!(peer.opened().is_empty());
    }

    #[tokio::test]
    async fn rejects_unsupported_image_extensions_without_a_round_trip() {
        let peer = FakePeer::new();
        let client = peer.session().await;

        let error = image_with(&client, "/home/user/diagram.svg")
            .await
            .expect_err("svg is text, not a raster preview");

        assert_eq!(error.code, "PATH_INVALID");
        assert!(peer.opened().is_empty());
    }

    // ---- public entry points ------------------------------------------------

    #[tokio::test]
    async fn rejects_invalid_paths_before_opening_a_session() {
        let state = AppState::new(PathBuf::from("."));

        let control = text(&state, "missing", "a\0b").await.expect_err("nul path");
        assert_eq!(control.code, "PATH_INVALID");

        let empty = image(&state, "missing", "  ")
            .await
            .expect_err("empty path");
        assert_eq!(empty.code, "PATH_INVALID");

        // Valid paths get past validation and reach the session registry.
        let text_session = text(&state, "missing", "/note.txt")
            .await
            .expect_err("no session");
        assert_eq!(text_session.code, "SESSION_NOT_FOUND");
        let image_session = image(&state, "missing", "/pic.png")
            .await
            .expect_err("no session");
        assert_eq!(image_session.code, "SESSION_NOT_FOUND");
    }

    /// Any preview read must use a read-only handle.
    fn assert_read_only(flags: &[OpenFlags]) {
        assert!(!flags.is_empty(), "preview never opened the file");
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

    fn fixture_files() -> HashMap<String, Vec<u8>> {
        let mut files = HashMap::new();
        files.insert("/home/user/note.txt".to_string(), NOTE.as_bytes().to_vec());
        files.insert("/home/user/gbk.txt".to_string(), GBK.to_vec());
        files.insert("/home/user/blob.bin".to_string(), b"hello\0world".to_vec());
        let mut large = vec![b'a'; (TEXT_LIMIT - 1) as usize];
        large.extend_from_slice("你".as_bytes());
        files.insert("/home/user/large.txt".to_string(), large);
        files.insert("/home/user/pic.png".to_string(), PNG.to_vec());
        files
    }

    fn file_attrs(permissions: u32, size: u64) -> FileAttributes {
        FileAttributes {
            size: Some(size),
            uid: Some(1000),
            gid: Some(1000),
            permissions: Some(permissions),
            mtime: Some(1_700_000_000),
            ..FileAttributes::empty()
        }
    }

    fn attrs_for(files: &HashMap<String, Vec<u8>>, path: &str) -> Option<FileAttributes> {
        match path {
            "/home/user/sub" => Some(file_attrs(0o040755, 4_096)),
            // Advertises more than the image cap; its bytes are never read.
            "/home/user/huge.png" => Some(file_attrs(0o100644, IMAGE_LIMIT + 1)),
            other => files
                .get(other)
                .map(|bytes| file_attrs(0o100644, bytes.len() as u64)),
        }
    }

    struct FakePeer {
        opened: Arc<Mutex<Vec<OpenFlags>>>,
        files: HashMap<String, Vec<u8>>,
    }

    impl FakePeer {
        fn new() -> Self {
            Self {
                opened: Arc::new(Mutex::new(Vec::new())),
                files: fixture_files(),
            }
        }

        async fn session(&self) -> SftpSession {
            let (client, server) = tokio::io::duplex(64 * 1024);
            let peer = Peer {
                opened: self.opened.clone(),
                files: self.files.clone(),
            };
            // `server::run` only spawns the peer task, so awaiting it returns at once.
            russh_sftp::server::run(server, peer).await;
            SftpSession::new(client).await.expect("fake SFTP handshake")
        }

        fn opened(&self) -> Vec<OpenFlags> {
            self.opened.lock().expect("open log").clone()
        }
    }

    /// In-memory SFTP peer: answers LSTAT, STAT, OPEN, READ and CLOSE only, so
    /// previews must talk the real wire protocol and any write attempt fails.
    struct Peer {
        opened: Arc<Mutex<Vec<OpenFlags>>>,
        files: HashMap<String, Vec<u8>>,
    }

    impl Handler for Peer {
        type Error = StatusCode;

        fn unimplemented(&self) -> StatusCode {
            StatusCode::OpUnsupported
        }

        fn lstat(
            &mut self,
            id: u32,
            path: String,
        ) -> impl Future<Output = Result<Attrs, Self::Error>> + Send {
            let attrs = attrs_for(&self.files, &path);
            async move {
                attrs
                    .map(|attrs| Attrs { id, attrs })
                    .ok_or(StatusCode::NoSuchFile)
            }
        }

        fn stat(
            &mut self,
            id: u32,
            path: String,
        ) -> impl Future<Output = Result<Attrs, Self::Error>> + Send {
            let attrs = attrs_for(&self.files, &path);
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
            let exists = attrs_for(&self.files, &filename).is_some();
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
            let data = self.files.get(&handle).map(|bytes| {
                let start = (offset as usize).min(bytes.len());
                let end = (start + len as usize).min(bytes.len());
                bytes[start..end].to_vec()
            });
            async move {
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

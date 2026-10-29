use serde::{Deserialize, Serialize};

use super::path::mode_text;
use russh_sftp::protocol::{FileAttributes, FileType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    File,
    Dir,
    Symlink,
    Other,
}

impl FileKind {
    pub fn from_sftp(value: FileType) -> Self {
        match value {
            FileType::File => Self::File,
            FileType::Dir => Self::Dir,
            FileType::Symlink => Self::Symlink,
            FileType::Other => Self::Other,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub kind: FileKind,
    pub size: u64,
    pub mtime: Option<u64>,
    pub mode: u32,
    pub mode_text: String,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symlink_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_kind: Option<FileKind>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileListing {
    pub path: String,
    pub canonical_path: String,
    pub entries: Vec<FileEntry>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextPreview {
    pub content: String,
    pub encoding: &'static str,
    pub bytes: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagePreview {
    pub mime: String,
    pub base64: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TransferProgress {
    Progress {
        transferred: u64,
        total: Option<u64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Saved,
    Canceled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResult {
    pub status: DownloadStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_path: Option<String>,
    pub bytes: u64,
}

pub fn file_entry(name: String, path: String, meta: &FileAttributes) -> FileEntry {
    let kind = FileKind::from_sftp(meta.file_type());
    let mode = meta.permissions.unwrap_or(0) & 0o7777;
    FileEntry {
        name,
        path,
        kind,
        size: meta.size.unwrap_or(0),
        mtime: meta.mtime.map(|seconds| seconds as u64 * 1000),
        mode,
        mode_text: mode_text(kind, mode),
        uid: meta.uid,
        gid: meta.gid,
        symlink_target: None,
        target_kind: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_entry_from_attributes() {
        let meta = FileAttributes {
            size: Some(12),
            uid: Some(1000),
            gid: Some(1000),
            permissions: Some(0o100644),
            mtime: Some(1_700_000_000),
            ..FileAttributes::default()
        };
        let entry = file_entry("a.txt".into(), "/tmp/a.txt".into(), &meta);
        assert_eq!(entry.kind, FileKind::File);
        assert_eq!(entry.mode_text, "-rw-r--r--");
        assert_eq!(entry.mtime, Some(1_700_000_000_000));
    }
}

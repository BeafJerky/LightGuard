//! 只读递归搜索：从给定目录向下遍历，名称包含匹配，不跟随符号链接。
//! 深度、结果数与耗时都有上限，超限返回 `truncated=true`。

use std::time::{Duration, Instant};

use russh_sftp::client::SftpSession;

use super::super::error::SshError;
use super::client::map_error;
use super::dto::{file_entry, FileEntry, FileKind};
use super::path::validate_remote_path;

pub const MAX_DEPTH: usize = 8;
pub const MAX_RESULTS: usize = 500;
pub const BUDGET: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub entries: Vec<FileEntry>,
    pub truncated: bool,
}

pub fn matches(query: &str, name: &str) -> bool {
    let needle = query.trim().to_lowercase();
    !needle.is_empty() && name.to_lowercase().contains(&needle)
}

pub async fn search(
    client: &SftpSession,
    root: &str,
    query: &str,
) -> Result<SearchResult, SshError> {
    validate_remote_path(root)?;
    if query.trim().is_empty() {
        return Ok(SearchResult {
            entries: Vec::new(),
            truncated: false,
        });
    }
    let started = Instant::now();
    let mut entries = Vec::new();
    let mut truncated = false;
    let mut stack = vec![(root.to_string(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        if started.elapsed() > BUDGET {
            truncated = true;
            break;
        }
        let listing = match client.read_dir(&dir).await {
            Ok(listing) => listing,
            // 单个子目录不可读时跳过，不中断整个搜索（根目录错误交由调用方判断）。
            Err(error) => {
                if dir == root {
                    return Err(map_error(error, &dir));
                }
                continue;
            }
        };
        for item in listing {
            let name = item.file_name();
            if name == "." || name == ".." {
                continue;
            }
            let meta = item.metadata();
            if matches(query, &name) {
                entries.push(file_entry(name.clone(), item.path(), &meta));
                if entries.len() >= MAX_RESULTS {
                    truncated = true;
                    break;
                }
            }
            if FileKind::from_sftp(meta.file_type()) == FileKind::Dir && depth < MAX_DEPTH {
                stack.push((item.path(), depth + 1));
            }
        }
        if truncated {
            break;
        }
    }

    Ok(SearchResult { entries, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_is_case_insensitive_and_ignores_blank() {
        assert!(matches("LOG", "app.log"));
        assert!(matches("报告", "2026报告.txt"));
        assert!(!matches("   ", "anything"));
        assert!(!matches("xyz", "app.log"));
    }

    #[test]
    fn limits_are_explicit() {
        assert_eq!(MAX_DEPTH, 8);
        assert_eq!(MAX_RESULTS, 500);
    }
}
//! 递归统计与删除：只处理链接本身，不跟随符号链接；深度有上限。
//!
//! 删除采用迭代而不是异步递归：Rust 不允许未装箱的 `async fn` 递归，
//! 而装箱递归在深树上会放大分配开销。这里用显式栈，先收集目录，
//! 再自底向上 `rmdir`。

use std::time::{Duration, Instant};

use russh_sftp::client::SftpSession;

use super::super::error::SshError;
use super::client::map_error;
use super::dto::FileKind;
use super::path::validate_remote_path;

pub const MAX_DELETE_DEPTH: usize = 32;

#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteStats {
    pub entries: u64,
    pub bytes: u64,
    pub truncated: bool,
}

pub fn exceeds_budget(entries: u64, max_entries: u64) -> bool {
    entries >= max_entries
}

/// 预览阶段的目录统计：达到条目上限、层级上限或时间预算就停止，
/// 并标记 `truncated`，避免在超大目录上把预览拖死。
pub async fn count_tree(
    client: &SftpSession,
    path: &str,
    max_entries: u64,
    max_depth: usize,
    budget: Duration,
) -> Result<DeleteStats, SshError> {
    validate_remote_path(path)?;
    let started = Instant::now();
    let mut stats = DeleteStats::default();
    let mut stack = vec![(path.to_string(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if started.elapsed() > budget || depth > max_depth {
            stats.truncated = true;
            break;
        }
        let entries = client
            .read_dir(dir.as_str())
            .await
            .map_err(|error| map_error(error, &dir))?;
        for item in entries {
            if exceeds_budget(stats.entries, max_entries) {
                stats.truncated = true;
                break;
            }
            let meta = item.metadata();
            stats.entries += 1;
            stats.bytes += meta.size.unwrap_or(0);
            if FileKind::from_sftp(meta.file_type()) == FileKind::Dir {
                stack.push((item.path(), depth + 1));
            }
        }
        if stats.truncated {
            break;
        }
    }
    Ok(stats)
}

pub async fn remove_tree(
    client: &SftpSession,
    path: &str,
    max_depth: usize,
) -> Result<u64, SshError> {
    validate_remote_path(path)?;
    let mut pending: Vec<(String, usize)> = vec![(path.to_string(), 0)];
    let mut dirs: Vec<String> = Vec::new();
    let mut removed = 0u64;
    let mut failures: Vec<String> = Vec::new();
    while let Some((dir, depth)) = pending.pop() {
        if depth > max_depth {
            return Err(SshError::write_failed("目录层级过深，已停止删除"));
        }
        let entries = match client.read_dir(&dir).await {
            Ok(entries) => entries,
            Err(error) => {
                failures.push(format!("{dir}：{}", map_error(error, &dir).message));
                continue;
            }
        };
        for item in entries {
            let child = item.path();
            if FileKind::from_sftp(item.metadata().file_type()) == FileKind::Dir {
                pending.push((child, depth + 1));
            } else {
                match client.remove_file(&child).await {
                    Ok(()) => removed += 1,
                    Err(error) => failures.push(format!("{child}：{}", map_error(error, &child).message)),
                }
            }
        }
        dirs.push(dir);
    }
    for dir in dirs.into_iter().rev() {
        match client.remove_dir(&dir).await {
            Ok(()) => removed += 1,
            Err(error) => failures.push(format!("{dir}：{}", map_error(error, &dir).message)),
        }
    }
    if failures.is_empty() {
        Ok(removed)
    } else {
        let shown = failures.iter().take(5).cloned().collect::<Vec<_>>().join("；");
        let suffix = if failures.len() > 5 {
            format!("；其余 {} 项略", failures.len() - 5)
        } else {
            String::new()
        };
        Err(SshError::write_failed(format!(
            "已删除 {removed} 项，{} 项失败：{shown}{suffix}",
            failures.len()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_is_inclusive() {
        assert!(!exceeds_budget(999, 1000));
        assert!(exceeds_budget(1000, 1000));
        assert!(exceeds_budget(1001, 1000));
    }
}
//! 容器内文件浏览（只读）：固定 `ls -la` 列表与 `cat` 预览，路径作为参数传入，
//! 不接受任意 shell；不提供任何写操作。

use std::time::Duration;

use russh::client::Handle;

use crate::docker::dto::{ContainerFileEntry, ContainerFilePreview};
use crate::docker::exec::{self, ExecOptions};
use crate::docker::validate;
use crate::ssh::error::SshError;
use crate::ssh::session::ClientHandler;
use crate::ssh::sftp::path::shell_quote;

const LIST_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_READ_BYTES: usize = 512 * 1024;

fn quote_path(path: &str) -> Result<String, SshError> {
    let trimmed = path.trim();
    if !trimmed.starts_with('/') {
        return Err(SshError::invalid_input("容器内路径必须是绝对路径"));
    }
    if trimmed.len() > 4096 || trimmed.chars().any(char::is_control) {
        return Err(SshError::invalid_input("容器内路径包含非法字符"));
    }
    if trimmed
        .split('/')
        .any(|segment| segment == ".." || segment == ".")
    {
        return Err(SshError::invalid_input("容器内路径不能包含 . 或 .."));
    }
    Ok(shell_quote(trimmed))
}

pub async fn list(
    handle: &Handle<ClientHandler>,
    container_id: &str,
    shell: &str,
    path: &str,
    elevated: bool,
) -> Result<Vec<ContainerFileEntry>, SshError> {
    let quoted = quote_path(path)?;
    let script = format!(
        "docker exec {} {} -c 'cd -- \"$1\" && LC_ALL=C ls -la --' sh {}",
        shell_quote(container_id),
        shell_quote(shell),
        quoted
    );
    let output = exec::run(
        handle,
        &script,
        ExecOptions {
            timeout: LIST_TIMEOUT,
            max_bytes: 2 * 1024 * 1024,
            stdin: None,
            cancel: None,
            elevated,
        },
        |_| {},
    )
    .await?;
    if output.code != Some(0) {
        let message = output.stderr_text().trim().to_string();
        return Err(SshError::docker_failed(if message.is_empty() {
            "读取容器目录失败".to_string()
        } else {
            message
        }));
    }
    Ok(parse_ls(&output.stdout_text(), path.trim_end_matches('/')))
}

fn parse_ls(text: &str, parent: &str) -> Vec<ContainerFileEntry> {
    let mut entries = Vec::new();
    for line in text.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            continue;
        }
        let perms = parts[0];
        let kind = match perms.chars().next() {
            Some('d') => "dir",
            Some('l') => "symlink",
            _ => "file",
        };
        let mut name = parts[8..].join(" ");
        if let Some((link, _)) = name.split_once(" -> ") {
            name = link.to_string();
        }
        if name == "." || name == ".." {
            continue;
        }
        let size = parts[4].parse::<u64>().unwrap_or(0);
        let path = if parent.is_empty() || parent == "/" {
            format!("/{name}")
        } else {
            format!("{parent}/{name}")
        };
        entries.push(ContainerFileEntry {
            name,
            path,
            kind: kind.to_string(),
            size,
            mode: perms.to_string(),
            mtime: parts[5..8].join(" "),
        });
    }
    entries.sort_by(|a, b| match (a.kind == "dir", b.kind == "dir") {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    entries
}

pub async fn read(
    handle: &Handle<ClientHandler>,
    container_id: &str,
    shell: &str,
    path: &str,
    elevated: bool,
) -> Result<ContainerFilePreview, SshError> {
    let quoted = quote_path(path)?;
    let script = format!(
        "docker exec {} {} -c 'cat -- \"$1\"' sh {}",
        shell_quote(container_id),
        shell_quote(shell),
        quoted
    );
    let output = exec::run(
        handle,
        &script,
        ExecOptions {
            timeout: READ_TIMEOUT,
            max_bytes: MAX_READ_BYTES,
            stdin: None,
            cancel: None,
            elevated,
        },
        |_| {},
    )
    .await?;
    if output.code != Some(0) {
        return Err(SshError::docker_failed("读取容器文件失败"));
    }
    let truncated = output.truncated || output.stdout.len() >= MAX_READ_BYTES;
    let slice = &output.stdout[..output.stdout.len().min(MAX_READ_BYTES)];
    let binary = slice.iter().take(8 * 1024).any(|byte| *byte == 0);
    Ok(ContainerFilePreview {
        content: if binary {
            String::new()
        } else {
            String::from_utf8_lossy(slice).to_string()
        },
        binary,
        truncated,
    })
}

/// 容器内路径校验给命令层复用（消息与前端一致）。
pub fn validate_path(path: &str) -> Result<(), SshError> {
    quote_path(path).map(|_| ())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ls_listing_and_sorts_dirs_first() {
        let text = "total 8\ndrwxr-xr-x 2 root root 4096 Jan 01 12:00 bin\n-rw-r--r-- 1 root root 12 Jan 01 12:00 app.conf\nlrwxrwxrwx 1 root root 3 Jan 01 12:00 link -> app.conf\n";
        let entries = parse_ls(text, "/etc");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].name, "bin");
        assert_eq!(entries[0].kind, "dir");
        assert_eq!(entries[0].path, "/etc/bin");
        assert_eq!(entries[1].name, "app.conf");
        assert_eq!(entries[1].size, 12);
        assert_eq!(entries[2].kind, "symlink");
        assert_eq!(entries[2].name, "link");
    }

    #[test]
    fn rejects_relative_or_traversal_paths() {
        for path in ["relative", "/etc/../root", "/etc/./x", "/etc/\0x"] {
            assert_eq!(validate_path(path).unwrap_err().code, "INVALID_INPUT");
        }
        assert!(validate_path("/etc/os-release").is_ok());
    }
}

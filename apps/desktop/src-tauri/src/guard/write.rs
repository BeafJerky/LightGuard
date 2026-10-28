//! 写操作的输入校验：远端是 POSIX，所以只拒绝 POSIX 层面真正危险或不可表示的名称
//! （分隔符、NUL 与控制字符、`.`/`..`、超长）；Windows 保留名不需要在这里拦。

use crate::ssh::error::SshError;

/// 单个目录名的字节上限（POSIX `NAME_MAX` 的常见取值）。
pub const MAX_NAME_BYTES: usize = 255;
/// 完整远端路径的字节上限，与 `ssh::sftp::path::MAX_REMOTE_PATH` 保持一致。
pub const MAX_PATH_BYTES: usize = 4096;

pub fn validate_folder_name(name: &str) -> Result<String, SshError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(SshError::preview_failed("文件夹名称不能为空"));
    }
    if trimmed.len() > MAX_NAME_BYTES {
        return Err(SshError::preview_failed("文件夹名称不能超过 255 字节"));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains('\0') {
        return Err(SshError::preview_failed("文件夹名称不能包含 / \\ 或 NUL"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(SshError::preview_failed("文件夹名称不能包含控制字符"));
    }
    if trimmed == "." || trimmed == ".." {
        return Err(SshError::preview_failed("不能使用 . 或 .. 作为名称"));
    }
    Ok(trimmed.to_string())
}

/// 把已校验的名称拼到目录后面，并再次确认结果没有逃出目标目录。
pub fn join_target(dir: &str, name: &str) -> Result<String, SshError> {
    let name = validate_folder_name(name)?;
    let base = dir.trim_end_matches('/');
    if !base.is_empty() && !base.starts_with('/') {
        return Err(SshError::preview_failed("目标目录必须是绝对路径"));
    }
    if base.chars().any(char::is_control) {
        return Err(SshError::preview_failed("目标路径包含非法字符"));
    }
    if base.split('/').any(|segment| segment == "..") {
        return Err(SshError::preview_failed("目标路径不能包含 .."));
    }
    let path = if base.is_empty() {
        format!("/{name}")
    } else {
        format!("{base}/{name}")
    };
    if path.len() > MAX_PATH_BYTES {
        return Err(SshError::preview_failed("完整路径超过 4096 字节"));
    }
    // 兜底：名称与目录都已排除 `..` 和分隔符，结果只可能是绝对路径。
    if !path.starts_with('/') {
        return Err(SshError::preview_failed("目标路径非法"));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_names_and_trims() {
        assert_eq!(validate_folder_name("  reports  ").unwrap(), "reports");
        assert!(validate_folder_name("a b-c_d.1").is_ok());
        assert!(validate_folder_name("报告").is_ok());
    }

    #[test]
    fn rejects_illegal_names() {
        for name in ["", "   ", ".", "..", "a/b", "a\\b", "a\0b"] {
            assert_eq!(
                validate_folder_name(name).unwrap_err().code,
                "PREVIEW_FAILED",
                "name {name:?}"
            );
        }
        let long = "x".repeat(256);
        assert_eq!(
            validate_folder_name(&long).unwrap_err().code,
            "PREVIEW_FAILED"
        );
        let max = "x".repeat(255);
        assert!(validate_folder_name(&max).is_ok());
    }

    #[test]
    fn rejects_control_characters() {
        for name in ["a\tb", "a\nb", "a\u{7f}b", "a\u{1}b"] {
            assert_eq!(
                validate_folder_name(name).unwrap_err().code,
                "PREVIEW_FAILED",
                "name {name:?}"
            );
        }
    }

    #[test]
    fn joins_without_double_slash() {
        assert_eq!(join_target("/", "x").unwrap(), "/x");
        assert_eq!(join_target("/home/u/", "x").unwrap(), "/home/u/x");
        assert_eq!(join_target("/home/u", "x").unwrap(), "/home/u/x");
        let deep = format!("/{}", "d".repeat(4096));
        assert_eq!(join_target(&deep, "x").unwrap_err().code, "PREVIEW_FAILED");
    }

    #[test]
    fn trims_the_name_before_joining() {
        assert_eq!(join_target("/data", "  logs  ").unwrap(), "/data/logs");
    }

    #[test]
    fn rejects_traversal_in_dir_or_name() {
        for dir in ["/data/../etc", "/..", "..", "relative/path", "/data/\0x"] {
            assert_eq!(
                join_target(dir, "x").unwrap_err().code,
                "PREVIEW_FAILED",
                "dir {dir:?}"
            );
        }
        for name in ["..", ".", "../x", "a/b"] {
            assert_eq!(
                join_target("/data", name).unwrap_err().code,
                "PREVIEW_FAILED",
                "name {name:?}"
            );
        }
        // `..hidden` 只是普通文件名，不是上跳。
        assert_eq!(join_target("/data", "..hidden").unwrap(), "/data/..hidden");
    }

    #[test]
    fn empty_dir_falls_back_to_root() {
        assert_eq!(join_target("", "x").unwrap(), "/x");
    }
}

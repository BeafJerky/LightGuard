use super::dto::FileKind;
use crate::ssh::error::SshError;

pub const MAX_REMOTE_PATH: usize = 4096;

pub fn validate_remote_path(path: &str) -> Result<(), SshError> {
    if path.trim().is_empty() {
        return Err(SshError::path_invalid("路径不能为空"));
    }
    if path.len() > MAX_REMOTE_PATH {
        return Err(SshError::path_invalid("路径过长"));
    }
    if path.contains('\0') || path.chars().any(char::is_control) {
        return Err(SshError::path_invalid("路径包含非法字符"));
    }
    Ok(())
}

pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn basename(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    trimmed.rsplit('/').next().unwrap_or(trimmed).to_string()
}

pub fn parent_of(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    match trimmed.rsplit_once('/') {
        Some(("", _)) => Some("/".to_string()),
        Some((parent, _)) => Some(parent.to_string()),
        None => Some(".".to_string()),
    }
}

pub fn extension_of(path: &str) -> Option<String> {
    let name = basename(path);
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .filter(|ext| !ext.is_empty())
}

pub fn image_mime(path: &str) -> Option<&'static str> {
    match extension_of(path)?.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "ico" => Some("image/x-icon"),
        _ => None,
    }
}

pub fn mode_text(kind: FileKind, mode: u32) -> String {
    let type_char = match kind {
        FileKind::File => '-',
        FileKind::Dir => 'd',
        FileKind::Symlink => 'l',
        FileKind::Other => '?',
    };
    let mut text = String::with_capacity(10);
    text.push(type_char);
    for shift in [6, 3, 0] {
        let bits = (mode >> shift) & 0o7;
        text.push(if bits & 0o4 != 0 { 'r' } else { '-' });
        text.push(if bits & 0o2 != 0 { 'w' } else { '-' });
        text.push(if bits & 0o1 != 0 { 'x' } else { '-' });
    }
    text
}

pub fn sanitize_local_name(name: &str, fallback: &str) -> String {
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6",
        "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6",
        "LPT7", "LPT8", "LPT9",
    ];
    let filtered: String = name
        .chars()
        .filter(|character| {
            !character.is_control()
                && !matches!(character, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        .collect();
    let mut cleaned = filtered.trim().trim_end_matches('.').to_string();
    if cleaned.is_empty() {
        cleaned = fallback.to_string();
    }
    let stem = cleaned
        .split('.')
        .next()
        .unwrap_or(&cleaned)
        .to_ascii_uppercase();
    if RESERVED.contains(&stem.as_str()) {
        cleaned.push('_');
    }
    truncate_name(&cleaned, 120)
}

fn truncate_name(name: &str, max: usize) -> String {
    let chars: Vec<char> = name.chars().collect();
    if chars.len() <= max {
        return name.to_string();
    }
    // 复合扩展名（如 .tar.gz）最多保留两段；单段扩展名最长 16 字符。
    let mut extension: Option<usize> = None;
    let mut search = name.len();
    for _ in 0..2 {
        let Some(dot) = name[..search].rfind('.') else {
            break;
        };
        if dot == 0 || name.len() - dot > 16 {
            break;
        }
        extension = Some(dot);
        search = dot;
    }
    match extension {
        Some(index) => {
            let length = name[index..].chars().count();
            let head: String = chars[..max.saturating_sub(length)].iter().collect();
            format!("{}{}", head, &name[index..])
        }
        None => chars[..max].iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_and_oversized_paths() {
        assert!(validate_remote_path("").is_err());
        assert!(validate_remote_path("a\0b").is_err());
        assert!(validate_remote_path(&"a".repeat(4097)).is_err());
        assert!(validate_remote_path("/etc/hostname").is_ok());
    }

    #[test]
    fn quotes_single_quotes() {
        assert_eq!(shell_quote("a'b"), "'a'\\''b'");
        assert_eq!(shell_quote("/tmp/a b"), "'/tmp/a b'");
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn sanitizes_windows_names() {
        assert_eq!(sanitize_local_name("CON.txt", "download"), "CON.txt_");
        assert_eq!(sanitize_local_name("a:b*c?.log", "download"), "abc.log");
        assert_eq!(sanitize_local_name("name.  ", "download"), "name");
        assert_eq!(sanitize_local_name("...", "download"), "download");
        assert_eq!(sanitize_local_name("", "download"), "download");
    }

    #[test]
    fn truncates_long_names_keeping_extension() {
        let name = format!("{}.tar.gz", "a".repeat(200));
        let cleaned = sanitize_local_name(&name, "download");
        assert!(cleaned.chars().count() <= 120);
        assert!(cleaned.ends_with(".tar.gz"));
    }

    #[test]
    fn renders_unix_mode() {
        assert_eq!(mode_text(FileKind::File, 0o644), "-rw-r--r--");
        assert_eq!(mode_text(FileKind::Dir, 0o750), "drwxr-x---");
        assert_eq!(mode_text(FileKind::Symlink, 0o777), "lrwxrwxrwx");
    }

    #[test]
    fn splits_remote_paths() {
        assert_eq!(basename("/etc/hostname"), "hostname");
        assert_eq!(basename("/var/log/"), "log");
        assert_eq!(parent_of("/etc/hostname"), Some("/etc".to_string()));
        assert_eq!(parent_of("/etc"), Some("/".to_string()));
        assert_eq!(parent_of("/"), None);
    }

    #[test]
    fn maps_image_extensions() {
        assert_eq!(image_mime("a.PNG"), Some("image/png"));
        assert_eq!(image_mime("a.jpeg"), Some("image/jpeg"));
        assert_eq!(image_mime("a.ico"), Some("image/x-icon"));
        assert_eq!(image_mime("a.svg"), None);
        assert_eq!(image_mime("README"), None);
    }
}

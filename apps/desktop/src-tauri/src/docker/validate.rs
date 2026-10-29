//! Docker 输入的纯函数校验与输出脱敏。所有远端脚本拼接前的用户输入都必须经过这里。

use crate::ssh::error::SshError;

const MAX_IMAGE_REFERENCE: usize = 255;
const MAX_SEARCH_TERM: usize = 128;
const MAX_URL: usize = 2048;

pub fn container_id(value: &str) -> Result<String, SshError> {
    let trimmed = value.trim();
    if trimmed.len() == 64 && trimmed.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Ok(trimmed.to_ascii_lowercase())
    } else {
        Err(SshError::invalid_input("容器标识无效，需要 64 位十六进制全 ID"))
    }
}

pub fn image_reference(value: &str) -> Result<String, SshError> {
    let trimmed = value.trim();
    let mut chars = trimmed.chars();
    let valid_first = chars
        .next()
        .map(|ch| ch.is_ascii_alphanumeric())
        .unwrap_or(false);
    let valid_rest = chars.all(|ch| {
        ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '/' | ':' | '@')
    });
    if valid_first && valid_rest && trimmed.len() <= MAX_IMAGE_REFERENCE {
        Ok(trimmed.to_string())
    } else {
        Err(SshError::invalid_input("镜像引用无效"))
    }
}

pub fn search_term(value: &str) -> Result<String, SshError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(SshError::invalid_input("搜索关键词不能为空"));
    }
    if trimmed.chars().count() > MAX_SEARCH_TERM {
        return Err(SshError::invalid_input("搜索关键词不能超过 128 个字符"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(SshError::invalid_input("搜索关键词包含控制字符"));
    }
    Ok(trimmed.to_string())
}

pub fn proxy_url(value: &str) -> Result<String, SshError> {
    let trimmed = value.trim();
    let scheme_ok = trimmed.starts_with("http://") || trimmed.starts_with("https://");
    let host_ok = trimmed
        .split_once("://")
        .map(|(_, rest)| !rest.is_empty())
        .unwrap_or(false);
    if scheme_ok
        && host_ok
        && trimmed.len() <= MAX_URL
        && !trimmed.chars().any(|ch| ch.is_whitespace() || ch.is_control())
    {
        Ok(trimmed.to_string())
    } else {
        Err(SshError::daemon_invalid("代理地址必须是 http:// 或 https:// URL"))
    }
}

pub fn no_proxy(value: &str) -> Result<String, SshError> {
    let trimmed = value.trim();
    if trimmed.len() > MAX_URL {
        return Err(SshError::daemon_invalid("NO_PROXY 过长"));
    }
    let allowed = trimmed.chars().all(|ch| {
        ch.is_ascii_alphanumeric()
            || matches!(ch, '.' | '-' | '_' | ',' | ':' | '/' | '*' | '[' | ']')
    });
    if allowed {
        Ok(trimmed.to_string())
    } else {
        Err(SshError::daemon_invalid("NO_PROXY 只能包含主机名、域名与 CIDR"))
    }
}

pub fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    ["pass", "pwd", "secret", "token", "key", "credential", "auth"]
        .iter()
        .any(|needle| lower.contains(needle))
}

pub fn mask_env_value(key: &str, value: &str) -> String {
    if is_sensitive_key(key) {
        return "••••".to_string();
    }
    value.chars().take(2048).collect()
}

/// `scheme://user:pass@host` → `scheme://***@host`；无凭据时原样返回。
pub fn redact_url(value: &str) -> String {
    let Some((scheme, rest)) = value.split_once("://") else {
        return value.to_string();
    };
    let (authority, tail) = match rest.split_once('/') {
        Some((authority, path)) => (authority, Some(path)),
        None => (rest, None),
    };
    if !authority.contains('@') {
        return value.to_string();
    }
    let host = authority.rsplit_once('@').map(|(_, host)| host).unwrap_or(authority);
    match tail {
        Some(path) => format!("{scheme}://***@{host}/{path}"),
        None => format!("{scheme}://***@{host}"),
    }
}

/// sudo 输出统一清洗：抹掉密码原文，丢弃 sudo 密码提示与含 password/token 的整行。
pub fn sanitize_output(text: &str, password: Option<&str>) -> String {
    let mut cleaned = text.to_string();
    if let Some(password) = password {
        if !password.is_empty() {
            cleaned = cleaned.replace(password, "***");
        }
    }
    cleaned
        .lines()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            !(lower.contains("sudo: password")
                || lower.contains("password for")
                || lower.contains("password")
                || lower.contains("token"))
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_full_container_ids() {
        let id = "a".repeat(64);
        assert_eq!(container_id(&id).unwrap(), id);
        let upper = "A".repeat(64);
        assert_eq!(container_id(&upper).unwrap(), "a".repeat(64));
        for bad in ["", "abc", &"g".repeat(64)] {
            assert_eq!(container_id(bad).unwrap_err().code, "INVALID_INPUT");
        }
    }

    #[test]
    fn image_reference_is_whitelisted() {
        for good in ["nginx", "nginx:1.27", "ghcr.io/a/b:c", "registry:5000/x/y@sha256:abc"] {
            assert!(image_reference(good).is_ok(), "ref {good}");
        }
        for bad in ["", "nginx:latest; rm -rf /", "a b", "$(id)", "`id`", "a|b", "a>b", "-n"] {
            assert_eq!(image_reference(bad).unwrap_err().code, "INVALID_INPUT", "ref {bad}");
        }
        assert!(image_reference(&"a".repeat(256)).is_err());
    }

    #[test]
    fn search_terms_are_trimmed_and_bounded() {
        assert_eq!(search_term("  nginx  ").unwrap(), "nginx");
        assert_eq!(search_term("").unwrap_err().code, "INVALID_INPUT");
        assert_eq!(search_term(&"a".repeat(129)).unwrap_err().code, "INVALID_INPUT");
        assert_eq!(search_term("a\tb").unwrap_err().code, "INVALID_INPUT");
    }

    #[test]
    fn proxy_urls_require_http_scheme() {
        assert!(proxy_url("http://proxy:3128").is_ok());
        assert!(proxy_url("https://u:p@proxy:3128").is_ok());
        for bad in ["", "socks5://proxy:1080", "http://a b", "http://x/\u{7f}"] {
            assert_eq!(proxy_url(bad).unwrap_err().code, "DAEMON_INVALID", "url {bad}");
        }
        assert!(proxy_url(&format!("http://{}", "a".repeat(2048))).is_err());
    }

    #[test]
    fn no_proxy_accepts_hosts_and_cidrs() {
        for good in ["localhost,127.0.0.1", "*.example.com", "10.0.0.0/8", "[::1]"] {
            assert!(no_proxy(good).is_ok(), "value {good}");
        }
        assert!(no_proxy("a;b").is_err());
        assert!(no_proxy("a b").is_err());
    }

    #[test]
    fn sensitive_keys_are_masked() {
        for key in ["DB_PASSWORD", "mysql_pwd", "API_SECRET", "TOKEN", "SSH_KEY", "CREDENTIALS", "AUTH"] {
            assert!(is_sensitive_key(key), "key {key}");
            assert_eq!(mask_env_value(key, &"x".repeat(3000)).chars().count(), 4);
        }
        assert_eq!(mask_env_value("PATH", "/usr/bin"), "/usr/bin");
        assert!(!is_sensitive_key("PATH"));
    }

    #[test]
    fn urls_are_redacted_for_preview() {
        assert_eq!(redact_url("http://u:p@proxy:3128"), "http://***@proxy:3128");
        assert_eq!(redact_url("http://proxy:3128"), "http://proxy:3128");
    }

    #[test]
    fn sudo_output_never_keeps_the_password() {
        let raw = "sudo: password for admin: \nsecretpw\nCannot connect to the Docker daemon";
        let cleaned = sanitize_output(raw, Some("secretpw"));
        assert!(!cleaned.contains("secretpw"));
        assert!(!cleaned.to_lowercase().contains("password"));
        assert!(cleaned.contains("Cannot connect"));
    }
}

// 只读：仅通过 journalctl / tail 读取登录日志，不写文件、不提权。
pub const LOGIN_SCRIPT: &str = r#"LC_ALL=C; export LC_ALL
if command -v journalctl >/dev/null 2>&1; then
  echo '###LG:src=journal###'
  journalctl -q --no-pager --since '-15 min' -u ssh -u sshd 2>/dev/null | tail -n 500
elif [ -r /var/log/auth.log ]; then
  echo '###LG:src=authlog###'
  tail -n 500 /var/log/auth.log
elif [ -r /var/log/secure ]; then
  echo '###LG:src=secure###'
  tail -n 500 /var/log/secure
fi
"#;

#[derive(Debug, Clone, PartialEq)]
pub struct LoginReport {
    pub available: bool,
    pub source: String,
    pub failed: u32,
    pub accepted: Vec<String>,
}

pub fn parse_login(text: &str) -> LoginReport {
    let source = text
        .lines()
        .find_map(|line| {
            line.strip_prefix("###LG:src=")
                .and_then(|rest| rest.strip_suffix("###"))
        })
        .unwrap_or("")
        .to_string();
    if source.is_empty() {
        return LoginReport {
            available: false,
            source,
            failed: 0,
            accepted: Vec::new(),
        };
    }
    let mut failed = 0u32;
    let mut accepted: Vec<String> = Vec::new();
    for line in text.lines() {
        if line.contains("Failed password")
            || line.contains("Failed publickey")
            || line.contains("Failed keyboard-interactive")
        {
            failed += 1;
            continue;
        }
        let Some(rest) = line.split("Accepted ").nth(1) else {
            continue;
        };
        if !rest.starts_with("password")
            && !rest.starts_with("publickey")
            && !rest.starts_with("keyboard-interactive")
        {
            continue;
        }
        let Some(ip) = rest
            .split(" from ")
            .nth(1)
            .and_then(|tail| tail.split_whitespace().next())
        else {
            continue;
        };
        if !ip.is_empty() && !accepted.iter().any(|existing| existing == ip) {
            accepted.push(ip.to_string());
        }
    }
    LoginReport {
        available: true,
        source,
        failed,
        accepted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JOURNAL: &str = "###LG:src=journal###
Jul 01 host sshd[1]: Failed password for root from 203.0.113.9 port 5000 ssh2
Jul 01 host sshd[1]: Failed publickey for admin from 198.51.100.7 port 5001 ssh2
Jul 01 host sshd[1]: Failed keyboard-interactive/pam for admin from 198.51.100.7 port 5002 ssh2
Jul 01 host sshd[1]: Accepted publickey for admin from 198.51.100.7 port 5003 ssh2: RSA SHA256:abc
Jul 01 host sshd[1]: Accepted password for root from 203.0.113.9 port 5004 ssh2
Jul 01 host sshd[1]: Accepted publickey for admin from 198.51.100.7 port 5005 ssh2: RSA SHA256:def
";

    #[test]
    fn counts_failures_and_dedupes_accepted_ips() {
        let report = parse_login(JOURNAL);
        assert!(report.available);
        assert_eq!(report.source, "journal");
        assert_eq!(report.failed, 3);
        assert_eq!(report.accepted, vec!["198.51.100.7", "203.0.113.9"]);
    }

    #[test]
    fn authlog_and_secure_markers_are_recognized() {
        let auth = parse_login("###LG:src=authlog###\nFailed password for root from 1.2.3.4 port 1 ssh2\n");
        assert!(auth.available);
        assert_eq!(auth.source, "authlog");
        assert_eq!(auth.failed, 1);

        let secure = parse_login("###LG:src=secure###\n");
        assert!(secure.available);
        assert_eq!(secure.source, "secure");
    }

    #[test]
    fn missing_marker_means_unavailable() {
        let report = parse_login("no logs here\nFailed password for root\n");
        assert!(!report.available);
        assert_eq!(report.failed, 0, "unavailable reports never count");
        assert!(report.accepted.is_empty());
    }

    #[test]
    fn script_stays_read_only() {
        for forbidden in ["rm ", "mkdir", "chmod", "sudo", "tee", "touch"] {
            assert!(!LOGIN_SCRIPT.contains(forbidden), "脚本不得包含 {forbidden}");
        }
    }
}

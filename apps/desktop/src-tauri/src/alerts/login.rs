// 只读：仅通过 journalctl / tail 读取登录日志，不写文件、不提权。
// journalctl 存在不代表能读系统日志（非 systemd-journal/adm 组时会静默返回空），
// 因此先捕获输出，只有真的拿到 ssh 日志才打印标记，否则继续回落到文件日志。
pub const LOGIN_SCRIPT: &str = r#"LC_ALL=C; export LC_ALL
if command -v journalctl >/dev/null 2>&1; then
  lg_journal=$(journalctl -q --no-pager --since '-15 min' -u ssh -u sshd 2>/dev/null | tail -n 500)
  if [ -n "$lg_journal" ]; then
    echo '###LG:src=journal###'
    printf '%s\n' "$lg_journal"
    exit 0
  fi
fi
if [ -r /var/log/auth.log ]; then
  echo '###LG:src=authlog###'
  tail -n 500 /var/log/auth.log
  exit 0
fi
if [ -r /var/log/secure ]; then
  echo '###LG:src=secure###'
  tail -n 500 /var/log/secure
  exit 0
fi
exit 0
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
    fn empty_journal_falls_back_to_file_sources() {
        // journalctl 存在但读不到系统日志时只会产出空输出，脚本此时不打 journal 标记，
        // 而是继续落到 auth.log / secure，两者都必须照常可解析。
        let authlog = parse_login(
            "###LG:src=authlog###\n\
             Jul 01 host sshd[1]: Failed password for root from 203.0.113.9 port 5000 ssh2\n",
        );
        assert!(authlog.available);
        assert_eq!(authlog.source, "authlog");
        assert_eq!(authlog.failed, 1);

        let secure = parse_login(
            "###LG:src=secure###\n\
             Jul 01 host sshd[1]: Failed password for admin from 198.51.100.7 port 5001 ssh2\n\
             Jul 01 host sshd[1]: Accepted password for admin from 198.51.100.7 port 5002 ssh2\n",
        );
        assert!(secure.available);
        assert_eq!(secure.source, "secure");
        assert_eq!(secure.failed, 1);
        assert_eq!(secure.accepted, vec!["198.51.100.7"]);
    }

    #[test]
    fn script_stays_read_only() {
        for forbidden in ["rm ", "mkdir", "chmod", "sudo", "tee", "touch"] {
            assert!(!LOGIN_SCRIPT.contains(forbidden), "脚本不得包含 {forbidden}");
        }
        for source in ["journal", "authlog", "secure"] {
            assert!(
                LOGIN_SCRIPT.contains(&format!("###LG:src={source}###")),
                "缺少 {source} 标记"
            );
        }
    }

    #[test]
    fn journal_branch_requires_output_before_its_marker() {
        let capture = LOGIN_SCRIPT
            .find("lg_journal=$(journalctl")
            .expect("journalctl 输出必须先捕获");
        let guard = LOGIN_SCRIPT
            .find("[ -n \"$lg_journal\" ]")
            .expect("journal 分支必须要求非空输出");
        let marker = LOGIN_SCRIPT
            .find("###LG:src=journal###")
            .expect("journal 标记");
        assert!(
            capture < guard && guard < marker,
            "空的 journalctl 输出不得打印 journal 标记"
        );
        assert!(
            LOGIN_SCRIPT.matches("exit 0").count() >= 3,
            "每个日志来源命中后都应立即结束脚本"
        );
    }
}

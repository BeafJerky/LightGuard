//! Docker 能力探测：一次固定脚本同时确认安装、daemon、socket 权限、systemd、
//! compose 与版本；版本决定 `proxies` 是否可写。

use std::time::Duration;

use russh::client::Handle;

use crate::docker::dto::DockerCapabilities;
use crate::docker::exec::{self, ExecOptions, ExecOutput};
use crate::ssh::error::SshError;
use crate::ssh::session::ClientHandler;

pub const PROBE_SCRIPT: &str = r#"if command -v docker >/dev/null 2>&1; then echo LG_DOCKER=1; else echo LG_DOCKER=0; fi
if command -v systemctl >/dev/null 2>&1; then echo LG_SYSTEMD=1; else echo LG_SYSTEMD=0; fi
if command -v systemctl >/dev/null 2>&1; then echo "LG_ENABLED=$(systemctl is-enabled docker 2>/dev/null || true)"; else echo LG_ENABLED=; fi
if command -v docker-compose >/dev/null 2>&1; then echo LG_COMPOSE=1; else echo LG_COMPOSE=0; fi
docker info --format '{{.ServerVersion}}|{{.APIVersion}}' 2>&1
echo "LG_INFO_EXIT=$?"
"#;

pub async fn detect(handle: &Handle<ClientHandler>) -> Result<DockerCapabilities, SshError> {
    let output = exec::run(
        handle,
        PROBE_SCRIPT,
        ExecOptions {
            timeout: Duration::from_secs(20),
            max_bytes: 64 * 1024,
            stdin: None,
            cancel: None,
        },
        |_| {},
    )
    .await?;
    Ok(parse_probe(&output))
}

pub fn parse_probe(output: &ExecOutput) -> DockerCapabilities {
    let text = output.stdout_text();
    let combined = format!("{}\n{}", text, output.stderr_text());
    let flag = |name: &str| text.lines().any(|line| line.trim() == format!("LG_{name}=1"));
    let installed = flag("DOCKER");
    let systemd = flag("SYSTEMD");
    let compose = flag("COMPOSE");
    let auto_start = text
        .lines()
        .find_map(|line| line.strip_prefix("LG_ENABLED="))
        .unwrap_or("")
        .trim()
        .to_string();
    let info_line = text
        .lines()
        .find(|line| line.contains('|') && !line.starts_with("LG_"));
    let info_exit = text
        .lines()
        .find_map(|line| line.strip_prefix("LG_INFO_EXIT="))
        .and_then(|value| value.trim().parse::<i32>().ok())
        .unwrap_or(1);
    let lowered = combined.to_ascii_lowercase();
    let permission = lowered.contains("permission denied");
    let refused = lowered.contains("cannot connect to the docker daemon")
        || lowered.contains("is the docker daemon running");
    let (version, api_version) = info_line
        .map(|line| {
            let mut parts = line.splitn(2, '|');
            (
                parts.next().unwrap_or("").trim().to_string(),
                parts.next().unwrap_or("").trim().to_string(),
            )
        })
        .unwrap_or_default();
    let daemon_running = installed && info_exit == 0;
    let socket_ok = daemon_running;
    let permission_denied = installed && permission && !socket_ok;
    let proxy_supported = major_version(&version) >= 23;
    let message = if !installed {
        "未安装 Docker"
    } else if permission && !socket_ok {
        "当前账号没有 Docker socket 权限"
    } else if refused || (!socket_ok && !permission) {
        "Docker 服务未运行"
    } else {
        ""
    }
    .to_string();
    DockerCapabilities {
        installed,
        daemon_running,
        socket_ok,
        permission_denied,
        systemd,
        auto_start,
        compose,
        version,
        api_version,
        proxy_supported,
        message,
    }
}

fn major_version(version: &str) -> u32 {
    version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(stdout: &str, stderr: &str) -> ExecOutput {
        ExecOutput {
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
            code: Some(0),
            truncated: false,
            canceled: false,
        }
    }

    #[test]
    fn reads_versions_and_flags() {
        let probe = parse_probe(&output(
            "LG_DOCKER=1\nLG_SYSTEMD=1\nLG_ENABLED=enabled\nLG_COMPOSE=0\n24.0.7|1.43\nLG_INFO_EXIT=0\n",
            "",
        ));
        assert!(probe.installed);
        assert!(probe.daemon_running);
        assert!(probe.socket_ok);
        assert!(probe.systemd);
        assert_eq!(probe.auto_start, "enabled");
        assert!(!probe.compose);
        assert_eq!(probe.version, "24.0.7");
        assert_eq!(probe.api_version, "1.43");
        assert!(probe.proxy_supported);
    }

    #[test]
    fn old_versions_do_not_support_proxies() {
        let probe = parse_probe(&output(
            "LG_DOCKER=1\nLG_SYSTEMD=0\nLG_COMPOSE=0\n20.10.24|1.41\nLG_INFO_EXIT=0\n",
            "",
        ));
        assert!(!probe.proxy_supported);
    }

    #[test]
    fn missing_docker_is_reported() {
        let probe = parse_probe(&output(
            "LG_DOCKER=0\nLG_SYSTEMD=1\nLG_COMPOSE=0\nsh: docker: not found\nLG_INFO_EXIT=127\n",
            "sh: docker: not found",
        ));
        assert!(!probe.installed);
        assert!(!probe.daemon_running);
        assert_eq!(probe.message, "未安装 Docker");
    }

    #[test]
    fn socket_permission_is_distinguished_from_stopped_daemon() {
        let probe = parse_probe(&output(
            "LG_DOCKER=1\nLG_SYSTEMD=1\nLG_COMPOSE=0\npermission denied while trying to connect to the Docker daemon socket\nLG_INFO_EXIT=1\n",
            "",
        ));
        assert!(probe.installed);
        assert!(!probe.socket_ok);
        assert!(probe.permission_denied);
        assert!(probe.message.contains("权限"));

        let stopped = parse_probe(&output(
            "LG_DOCKER=1\nLG_SYSTEMD=1\nLG_COMPOSE=0\nCannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?\nLG_INFO_EXIT=1\n",
            "",
        ));
        assert!(stopped.message.contains("未运行"));
        // 服务停止不是权限问题：前端仍要给出「启动 Docker 服务」入口。
        assert!(!stopped.permission_denied);
    }
}

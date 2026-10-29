//! Docker 只读采集。所有脚本固定，参数经 `shell_quote`；失败按 Docker 语义分成
//! 未安装、daemon 未运行、socket 无权限与一般失败，前端据此展示不同空状态。

use std::time::Duration;

use russh::client::Handle;

use crate::docker::capability;
use crate::docker::dto::{
    DockerCapabilities, DockerContainer, DockerContainerDetail, DockerImage, DockerOverview,
    DockerStatsSample, RegistrySearchItem,
};
use crate::docker::exec::{self, ExecOptions, ExecOutput};
use crate::docker::parse;
use crate::ssh::error::SshError;
use crate::ssh::session::ClientHandler;
use crate::ssh::sftp::path::shell_quote;

const READ_TIMEOUT: Duration = Duration::from_secs(20);
const STATS_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_OUTPUT: usize = 8 * 1024 * 1024;

pub async fn capabilities(handle: &Handle<ClientHandler>) -> Result<DockerCapabilities, SshError> {
    capability::detect(handle).await
}

pub async fn overview(
    handle: &Handle<ClientHandler>,
    include_df: bool,
) -> Result<DockerOverview, SshError> {
    let capabilities = capability::detect(handle).await?;
    if !capabilities.installed {
        return Err(SshError::docker_unavailable(capabilities.message.clone()));
    }
    let info_output = exec::run(
        handle,
        "docker info --format '{{json .}}'",
        ExecOptions {
            timeout: READ_TIMEOUT,
            max_bytes: MAX_OUTPUT,
            stdin: None,
            cancel: None,
        },
        |_| {},
    )
    .await?;
    let (info, counts) = if info_output.code == Some(0) {
        parse::overview_parts(&info_output.stdout_text())
    } else {
        (
            crate::docker::dto::DockerInfo {
                version: capabilities.version.clone(),
                api_version: capabilities.api_version.clone(),
                storage_driver: String::new(),
                data_root: String::new(),
                mirrors: Vec::new(),
            },
            crate::docker::dto::DockerCounts::default(),
        )
    };
    let df = if include_df && info_output.code == Some(0) {
        exec::run(
            handle,
            "docker system df --format '{{json .}}'",
            ExecOptions {
                timeout: READ_TIMEOUT,
                max_bytes: MAX_OUTPUT,
                stdin: None,
                cancel: None,
            },
            |_| {},
        )
        .await
        .ok()
        .filter(|output| output.code == Some(0))
        .and_then(|output| parse::df(&output.stdout_text()))
    } else {
        None
    };
    Ok(DockerOverview {
        capabilities,
        info,
        counts,
        df,
    })
}

pub async fn containers(handle: &Handle<ClientHandler>) -> Result<Vec<DockerContainer>, SshError> {
    let listing = run_read(
        handle,
        "docker ps -a --no-trunc --format '{{json .}}'",
        READ_TIMEOUT,
        "读取容器",
    )
    .await?;
    let mut rows = parse::containers(&listing.stdout_text());
    if !rows.is_empty() {
        if let Ok(output) = exec::run(
            handle,
            "docker stats --no-stream --format '{{json .}}'",
            ExecOptions {
                timeout: STATS_TIMEOUT,
                max_bytes: MAX_OUTPUT,
                stdin: None,
                cancel: None,
            },
            |_| {},
        )
        .await
        {
            if output.code == Some(0) {
                parse::merge_stats(&mut rows, &parse::stats(&output.stdout_text()));
            }
        }
    }
    Ok(rows)
}

pub async fn container_detail(
    handle: &Handle<ClientHandler>,
    container_id: &str,
) -> Result<DockerContainerDetail, SshError> {
    let script = format!("docker inspect {}", shell_quote(container_id));
    let output = run_read(handle, &script, READ_TIMEOUT, "读取容器详情").await?;
    parse::container_detail(&output.stdout_text())
}

pub async fn stats(handle: &Handle<ClientHandler>) -> Result<Vec<DockerStatsSample>, SshError> {
    let output = run_read(
        handle,
        "docker stats --no-stream --format '{{json .}}'",
        STATS_TIMEOUT,
        "读取容器资源",
    )
    .await?;
    Ok(parse::stats(&output.stdout_text()))
}

pub async fn images(handle: &Handle<ClientHandler>) -> Result<Vec<DockerImage>, SshError> {
    let output = run_read(
        handle,
        "docker images --no-trunc --format '{{json .}}'",
        READ_TIMEOUT,
        "读取镜像",
    )
    .await?;
    Ok(parse::images(&output.stdout_text()))
}

pub async fn registry_search(
    handle: &Handle<ClientHandler>,
    term: &str,
    limit: u32,
) -> Result<Vec<RegistrySearchItem>, SshError> {
    let output = run_read(
        handle,
        &search_script(term, limit),
        Duration::from_secs(30),
        "搜索镜像仓库",
    )
    .await
    .map_err(|error| {
        if error.code == "DOCKER_FAILED" {
            SshError::registry_failed(error.message)
        } else {
            error
        }
    })?;
    Ok(parse::search(&output.stdout_text()))
}

pub fn search_script(term: &str, limit: u32) -> String {
    format!(
        "docker search --limit {} --format '{{{{json .}}}}' {}",
        limit.clamp(1, 25),
        shell_quote(term)
    )
}

pub(crate) async fn run_read(
    handle: &Handle<ClientHandler>,
    script: &str,
    timeout: Duration,
    action: &str,
) -> Result<ExecOutput, SshError> {
    let output = exec::run(
        handle,
        script,
        ExecOptions {
            timeout,
            max_bytes: MAX_OUTPUT,
            stdin: None,
            cancel: None,
        },
        |_| {},
    )
    .await?;
    if output.code == Some(0) {
        Ok(output)
    } else {
        Err(classify(&output, action))
    }
}

pub(crate) fn classify(output: &ExecOutput, action: &str) -> SshError {
    let combined = format!("{}\n{}", output.stdout_text(), output.stderr_text());
    let lowered = combined.to_ascii_lowercase();
    if lowered.contains("permission denied while trying to connect to the docker daemon socket") {
        return SshError::docker_permission(format!("{action}需要 Docker socket 权限"));
    }
    if lowered.contains("cannot connect to the docker daemon")
        || lowered.contains("is the docker daemon running")
    {
        return SshError::docker_unavailable("Docker 服务未运行");
    }
    if lowered.contains("command not found") || lowered.contains("docker: not found") {
        return SshError::docker_unavailable("未安装 Docker");
    }
    SshError::docker_failed(format!("{action}失败：{}", first_line(&combined)))
}

fn first_line(text: &str) -> String {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("未知错误")
        .trim()
        .chars()
        .take(200)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(stdout: &str, stderr: &str, code: u32) -> ExecOutput {
        ExecOutput {
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
            code: Some(code),
            truncated: false,
            canceled: false,
        }
    }

    #[test]
    fn classifies_socket_permission_and_stopped_daemon() {
        let denied = classify(
            &output("", "permission denied while trying to connect to the Docker daemon socket", 1),
            "读取容器",
        );
        assert_eq!(denied.code, "DOCKER_PERMISSION");
        let stopped = classify(
            &output("Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?", "", 1),
            "读取容器",
        );
        assert_eq!(stopped.code, "DOCKER_UNAVAILABLE");
        let missing = classify(&output("docker: not found", "", 127), "读取容器");
        assert_eq!(missing.code, "DOCKER_UNAVAILABLE");
        let failed = classify(&output("", "no such container", 1), "读取容器");
        assert_eq!(failed.code, "DOCKER_FAILED");
        assert!(failed.message.contains("no such container"));
    }

    #[test]
    fn search_script_quotes_the_term_and_clamps_the_limit() {
        let script = search_script("it's a repo", 99);
        assert!(script.contains("--limit 25"));
        assert!(script.contains(r#"'it'\''s a repo'"#));
        assert_eq!(search_script("nginx", 0), "docker search --limit 1 --format '{{json .}}' 'nginx'");
    }
}

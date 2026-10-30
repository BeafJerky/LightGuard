//! Docker 只读采集。所有脚本固定，参数经 `shell_quote`；失败按 Docker 语义分成
//! 未安装、daemon 未运行、socket 无权限与一般失败，前端据此展示不同空状态。

use std::time::Duration;

use russh::client::Handle;

use crate::docker::capability;
use crate::docker::dto::{
    DockerCapabilities, DockerContainer, DockerContainerDetail, DockerImage, DockerOverview,
    DockerStatsSample, RegistrySearchItem, RegistryTags,
};
use crate::docker::exec::{self, ExecOptions, ExecOutput};
use crate::docker::parse::{self, TagLookup};
use crate::docker::validate;
use crate::ssh::error::SshError;
use crate::ssh::session::ClientHandler;
use crate::ssh::sftp::path::shell_quote;

const READ_TIMEOUT: Duration = Duration::from_secs(20);
const STATS_TIMEOUT: Duration = Duration::from_secs(8);
const TAGS_TIMEOUT: Duration = Duration::from_secs(15);
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
    .map_err(search_failure)?;
    Ok(parse::search(&output.stdout_text()))
}

/// `docker search` 由 CLI 直连 Docker Hub，不经过 daemon 的 registry mirror。
/// 服务器网络无法访问 Docker Hub 时 exec 只会等到超时，原样抛给前端会显示
/// 内部的「Docker 命令执行超时」，这里统一换成仓库级错误，由前端补充可操作说明。
fn search_failure(error: SshError) -> SshError {
    match error.code {
        "DOCKER_FAILED" => SshError::registry_failed(error.message),
        "TIMEOUT" => SshError::registry_failed("连接 Docker Hub 超时"),
        _ => error,
    }
}

pub fn search_script(term: &str, limit: u32) -> String {
    format!(
        "docker search --limit {} --format '{{{{json .}}}}' {}",
        limit.clamp(1, 25),
        shell_quote(term)
    )
}

/// 加速源上的标签列表：只读 registry 的 `tags/list`，不经过 Docker Hub 搜索。
/// 服务器可能配置了多个加速源，按顺序试前三个，命中即返回。
pub async fn registry_tags(
    handle: &Handle<ClientHandler>,
    input: &str,
) -> Result<RegistryTags, SshError> {
    let repo = validate::repo_name(input)?;
    let mirrors = registry_mirrors(handle).await?;
    let mut not_found = false;
    let mut last: Option<SshError> = None;
    for mirror in mirrors.iter().take(3) {
        let url = tags_url(mirror, &repo);
        let output =
            match run_read(handle, &tags_script(&url), TAGS_TIMEOUT, "查询镜像版本").await {
                Ok(output) => output,
                Err(error) => {
                    last = Some(tags_transport_failure(mirror, error));
                    continue;
                }
            };
        match parse::tag_lookup(&output.stdout_text()) {
            TagLookup::Tags(tags) => return Ok(RegistryTags { repo, tags }),
            TagLookup::NotFound => not_found = true,
            TagLookup::Unauthorized => {
                last = Some(SshError::registry_failed(format!(
                    "加速源 {} 要求登录，无法列出版本",
                    mirror_host(mirror)
                )));
            }
            TagLookup::Failed(reason) => {
                last = Some(SshError::registry_failed(format!(
                    "加速源 {} 查询失败：{reason}",
                    mirror_host(mirror)
                )));
            }
        }
    }
    if not_found {
        return Err(SshError::registry_failed(format!(
            "加速源上没有 {repo}，请检查镜像名"
        )));
    }
    Err(last.unwrap_or_else(|| SshError::registry_failed("没有可用的加速源")))
}

/// 生效的加速源来自 `docker info`：与 `docker pull` 实际使用的是同一份，且不需要系统授权。
async fn registry_mirrors(handle: &Handle<ClientHandler>) -> Result<Vec<String>, SshError> {
    let output = run_read(
        handle,
        "docker info --format '{{json .RegistryConfig.Mirrors}}'",
        READ_TIMEOUT,
        "读取加速源",
    )
    .await?;
    let text = output.stdout_text();
    let value: serde_json::Value = serde_json::from_str(text.trim())
        .map_err(|_| SshError::registry_failed("无法读取加速源配置，请检查 Docker 版本"))?;
    let mirrors: Vec<String> = value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|mirror| mirror.starts_with("http://") || mirror.starts_with("https://"))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if mirrors.is_empty() {
        return Err(SshError::registry_failed(
            "还没有配置加速源，无法查询版本；请先在「加速源设置」里添加",
        ));
    }
    Ok(mirrors)
}

fn tags_transport_failure(mirror: &str, error: SshError) -> SshError {
    if error.message.contains("__LG_NO_HTTP__") {
        return SshError::registry_failed("服务器缺少 curl 与 wget，无法查询加速源");
    }
    if matches!(error.code, "DOCKER_UNAVAILABLE" | "DOCKER_PERMISSION") {
        return error;
    }
    let message = if error.code == "TIMEOUT" || error.message.contains("__LG_FETCH_FAIL__") {
        format!("连接加速源 {} 超时", mirror_host(mirror))
    } else {
        format!("加速源 {} 不可达", mirror_host(mirror))
    };
    SshError::registry_failed(message)
}

pub fn tags_url(mirror: &str, repo: &str) -> String {
    format!("{}/v2/{repo}/tags/list", mirror.trim_end_matches('/'))
}

pub fn mirror_host(mirror: &str) -> String {
    mirror
        .trim_end_matches('/')
        .split("://")
        .nth(1)
        .unwrap_or(mirror)
        .split('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

/// 优先用 curl（能拿到 HTTP 状态码区分 404 / 401），没有 curl 时退回 wget；
/// 两者都没有时留下标记，由上层换成可读提示。
pub fn tags_script(url: &str) -> String {
    format!(
        "if command -v curl >/dev/null 2>&1; then curl -sS --max-time 8 -w '\\n__LG_HTTP__%{{http_code}}' -- {url}; \
elif command -v wget >/dev/null 2>&1; then if wget -qO- --timeout=8 --tries=1 -- {url}; then printf '\\n__LG_HTTP__200'; \
else echo '__LG_FETCH_FAIL__' >&2; exit 3; fi; \
else echo '__LG_NO_HTTP__' >&2; exit 3; fi",
        url = shell_quote(url)
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
    fn search_timeout_becomes_a_registry_failure() {
        let mapped = search_failure(SshError::timeout("Docker 命令执行超时"));
        assert_eq!(mapped.code, "REGISTRY_FAILED");
        assert_eq!(mapped.message, "连接 Docker Hub 超时");
        assert_eq!(
            search_failure(SshError::docker_failed("no such registry")).code,
            "REGISTRY_FAILED"
        );
        assert_eq!(
            search_failure(SshError::docker_permission("搜索镜像仓库需要 Docker socket 权限")).code,
            "DOCKER_PERMISSION"
        );
    }

    #[test]
    fn search_script_quotes_the_term_and_clamps_the_limit() {
        let script = search_script("it's a repo", 99);
        assert!(script.contains("--limit 25"));
        assert!(script.contains(r#"'it'\''s a repo'"#));
        assert_eq!(search_script("nginx", 0), "docker search --limit 1 --format '{{json .}}' 'nginx'");
    }

    #[test]
    fn tag_urls_ignore_trailing_slashes_and_keep_the_repo_path() {
        assert_eq!(
            tags_url("https://docker.1ms.run/", "library/nginx"),
            "https://docker.1ms.run/v2/library/nginx/tags/list"
        );
        assert_eq!(
            tags_url("https://docker.1ms.run", "bitnami/nginx"),
            "https://docker.1ms.run/v2/bitnami/nginx/tags/list"
        );
        assert_eq!(mirror_host("https://docker.1ms.run/"), "docker.1ms.run");
        assert_eq!(mirror_host("docker.1ms.run"), "docker.1ms.run");
    }

    #[test]
    fn tags_script_prefers_curl_and_falls_back_to_wget() {
        let script = tags_script("https://mirror.example.com/v2/library/nginx/tags/list");
        assert!(script.contains("command -v curl"));
        assert!(script.contains("command -v wget"));
        assert!(script.contains("__LG_HTTP__%{http_code}"));
        assert!(script.contains("--max-time 8"));
        assert!(script.contains("'https://mirror.example.com/v2/library/nginx/tags/list'"));
        assert!(script.contains("__LG_NO_HTTP__"));
        assert!(script.contains("__LG_FETCH_FAIL__"));
    }

    #[test]
    fn tags_transport_failures_become_actionable_registry_errors() {
        let missing = tags_transport_failure(
            "https://docker.1ms.run",
            SshError::docker_failed("查询镜像版本失败：__LG_NO_HTTP__"),
        );
        assert_eq!(missing.code, "REGISTRY_FAILED");
        assert!(missing.message.contains("缺少 curl 与 wget"));

        let timeout = tags_transport_failure(
            "https://docker.1ms.run/",
            SshError::timeout("Docker 命令执行超时"),
        );
        assert_eq!(timeout.code, "REGISTRY_FAILED");
        assert!(timeout.message.contains("docker.1ms.run"));
        assert!(timeout.message.contains("超时"));

        let unreachable = tags_transport_failure(
            "https://docker.1ms.run",
            SshError::docker_failed("查询镜像版本失败：curl: (6) Could not resolve host"),
        );
        assert!(unreachable.message.contains("不可达"));

        let permission = tags_transport_failure(
            "https://docker.1ms.run",
            SshError::docker_permission("读取加速源需要 Docker socket 权限"),
        );
        assert_eq!(permission.code, "DOCKER_PERMISSION");
    }
}

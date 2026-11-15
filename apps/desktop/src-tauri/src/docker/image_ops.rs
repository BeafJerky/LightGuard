//! 镜像导入/构建的本地归档准备，以及本地与仓库 digest 的更新检测。
//! 归档只放在远端 `/tmp`，执行后立即删除；本地临时文件同样随用随删。

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use russh::client::Handle;
use serde_json::Value;

use crate::docker::dto::ImageUpdateStatus;
use crate::docker::exec::{self, ExecOptions};
use crate::ssh::error::SshError;
use crate::ssh::session::ClientHandler;
use crate::ssh::sftp::client;
use crate::ssh::sftp::transfer::{self, upload_part_path};
use crate::ssh::state::AppState;

const MAX_ARCHIVE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_CONTEXT_BYTES: u64 = 1024 * 1024 * 1024;

pub fn validate_archive(path: &str) -> Result<PathBuf, SshError> {
    let path = std::fs::canonicalize(path.trim())
        .map_err(|_| SshError::invalid_input("找不到本地镜像归档"))?;
    let metadata = std::fs::metadata(&path)
        .map_err(|_| SshError::invalid_input("无法读取本地镜像归档"))?;
    if !metadata.is_file() {
        return Err(SshError::invalid_input("请选择 .tar/.tar.gz 归档文件"));
    }
    if metadata.len() > MAX_ARCHIVE_BYTES {
        return Err(SshError::invalid_input("归档超过 4 GiB，请先在本地拆分"));
    }
    Ok(path)
}

pub fn validate_context(path: &str, dockerfile: Option<&str>) -> Result<PathBuf, SshError> {
    let root = std::fs::canonicalize(path.trim())
        .map_err(|_| SshError::invalid_input("找不到构建上下文目录"))?;
    if !root.is_dir() {
        return Err(SshError::invalid_input("构建上下文必须是目录"));
    }
    let name = dockerfile.unwrap_or("Dockerfile").trim();
    if name.is_empty()
        || name.starts_with('/')
        || name.contains('\\')
        || name.split('/').any(|part| part == ".." || part.is_empty())
    {
        return Err(SshError::invalid_input("Dockerfile 路径无效"));
    }
    if !root.join(name).is_file() {
        return Err(SshError::invalid_input(format!("上下文中缺少 {name}")));
    }
    Ok(root)
}

fn remote_temp(prefix: &str, extension: &str) -> String {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or(0);
    format!("/tmp/lightguard-{prefix}-{stamp}.{extension}")
}

async fn upload(
    state: &AppState,
    server_id: &str,
    local: &Path,
    remote: &str,
    on_line: &mut (dyn FnMut(&str) + Send),
) -> Result<(), SshError> {
    let sftp = client::acquire(state, server_id).await?;
    let part = upload_part_path(remote, "lgimport");
    state.register_part(server_id, &part).await;
    let cancel = AtomicBool::new(false);
    let mut last_bucket = 0u64;
    let outcome = match transfer::upload_file(
        &sftp,
        local,
        &part,
        "lgimport",
        &cancel,
        |sent, total| {
            if let (Some(total), true) = (total, last_bucket != 100) {
                if total > 0 {
                    let percent = sent
                        .saturating_mul(100)
                        .checked_div(total)
                        .unwrap_or(0);
                    if percent >= last_bucket + 10 {
                        last_bucket = percent;
                        on_line(&format!("已上传 {percent}%"));
                    }
                }
            }
        },
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(error) => {
            if sftp.remove_file(&part).await.is_ok() {
                state.forget_part(server_id, &part).await;
            }
            return Err(error);
        }
    };
    if let transfer::TransferOutcome::Canceled { .. } = outcome {
        state.forget_part(server_id, &part).await;
        return Err(SshError::transfer_failed("上传已取消"));
    }
    match client::rename(state, server_id, &part, remote).await {
        Ok(()) => {
            state.forget_part(server_id, &part).await;
            Ok(())
        }
        Err(error) => {
            let sftp = client::acquire(state, server_id).await?;
            if sftp.remove_file(&part).await.is_ok() {
                state.forget_part(server_id, &part).await;
            }
            Err(error)
        }
    }
}

pub async fn prepare_import(
    state: &AppState,
    server_id: &str,
    local_path: &str,
    on_line: &mut (dyn FnMut(&str) + Send),
) -> Result<String, SshError> {
    let local = validate_archive(local_path)?;
    let remote = remote_temp("import", "tar");
    upload(state, server_id, &local, &remote, on_line).await?;
    Ok(remote)
}

pub async fn prepare_build(
    state: &AppState,
    server_id: &str,
    context: &str,
    dockerfile: Option<&str>,
    on_line: &mut (dyn FnMut(&str) + Send),
) -> Result<String, SshError> {
    let root = validate_context(context, dockerfile)?;
    let temp = std::env::temp_dir().join(format!(
        "lightguard-context-{}.tar.gz",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|value| value.as_millis())
            .unwrap_or(0)
    ));
    let target = temp.clone();
    let root_for_tar = root.clone();
    let packaged = tokio::task::spawn_blocking(move || {
        std::process::Command::new("tar")
            .arg("-czf")
            .arg(&target)
            .arg("-C")
            .arg(&root_for_tar)
            .arg(".")
            .status()
    })
    .await
    .map_err(|error| SshError::internal(error.to_string()))?;
    if !matches!(packaged, Ok(status) if status.success()) {
        let _ = tokio::fs::remove_file(&temp).await;
        return Err(SshError::docker_failed("打包构建上下文失败"));
    }
    let size = tokio::fs::metadata(&temp)
        .await
        .map(|meta| meta.len())
        .unwrap_or(0);
    if size > MAX_CONTEXT_BYTES {
        let _ = tokio::fs::remove_file(&temp).await;
        return Err(SshError::invalid_input("构建上下文超过 1 GiB"));
    }
    let remote = remote_temp("build", "tar.gz");
    let result = upload(state, server_id, &temp, &remote, on_line).await;
    let _ = tokio::fs::remove_file(&temp).await;
    result?;
    Ok(remote)
}

pub async fn cleanup_remote(state: &AppState, server_id: &str, remote: &str) {
    let _ = client::remove_file(state, server_id, remote).await;
}

fn first_digest(value: &Value) -> Option<String> {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(digest)) = map.get("digest") {
                if digest.starts_with("sha256:") {
                    return Some(digest.clone());
                }
            }
            map.values().find_map(first_digest)
        }
        Value::Array(items) => items.iter().find_map(first_digest),
        _ => None,
    }
}

fn repo_digest(stdout: &str) -> Option<String> {
    let values: Vec<String> = serde_json::from_str(stdout.trim()).ok()?;
    values
        .into_iter()
        .filter_map(|value| value.split_once('@').map(|(_, digest)| digest.to_string()))
        .next()
}

/// 仓库返回的错误常带签名 blob URL；只保留可操作的一行提示，避免把长 URL 抛给界面。
fn registry_error(stderr: &str) -> String {
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("tls") || lower.contains("connection reset") || lower.contains("timeout") {
        return "连接仓库超时或被重置，请检查服务器网络与加速源".to_string();
    }
    if lower.contains("unauthorized") || lower.contains("denied") {
        return "仓库拒绝访问，请先在镜像仓库页登录".to_string();
    }
    let line = stderr
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("无法读取仓库 digest")
        .trim();
    line.chars().take(160).collect()
}

pub async fn check_update(
    handle: &Handle<ClientHandler>,
    reference: &str,
    elevated: bool,
    proxy: Option<&str>,
) -> Result<ImageUpdateStatus, SshError> {
    let local_script = format!(
        "docker image inspect --format '{{{{json .RepoDigests}}}}' {}",
        crate::ssh::sftp::path::shell_quote(reference)
    );
    let local = exec::run(
        handle,
        &local_script,
        ExecOptions {
            timeout: Duration::from_secs(20),
            max_bytes: 64 * 1024,
            stdin: None,
            cancel: None,
            elevated,
        },
        |_| {},
    )
    .await?;
    if local.code != Some(0) {
        return Err(SshError::docker_failed("无法读取本地镜像 digest"));
    }
    let local_digest = repo_digest(&local.stdout_text()).unwrap_or_default();
    if local_digest.is_empty() {
        return Ok(ImageUpdateStatus {
            reference: reference.to_string(),
            state: "unknown".to_string(),
            local_digest,
            remote_digest: String::new(),
            message: "本地镜像没有仓库 digest（可能是本地构建）".to_string(),
        });
    }
    let proxy_env = proxy
        .map(|url| {
            let quoted = crate::ssh::sftp::path::shell_quote(url);
            format!("HTTP_PROXY={quoted} HTTPS_PROXY={quoted} ")
        })
        .unwrap_or_default();
    let remote_script = format!(
        "{proxy_env}docker manifest inspect --verbose {}",
        crate::ssh::sftp::path::shell_quote(reference)
    );
    let remote = exec::run(
        handle,
        &remote_script,
        ExecOptions {
            timeout: Duration::from_secs(60),
            max_bytes: 1024 * 1024,
            stdin: None,
            cancel: None,
            elevated,
        },
        |_| {},
    )
    .await?;
    if remote.code != Some(0) {
        return Ok(ImageUpdateStatus {
            reference: reference.to_string(),
            state: "unknown".to_string(),
            local_digest,
            remote_digest: String::new(),
            message: registry_error(&remote.stderr_text()),
        });
    }
    let parsed: Value = serde_json::from_str(remote.stdout_text().trim()).unwrap_or(Value::Null);
    let remote_digest = first_digest(&parsed).unwrap_or_default();
    if remote_digest.is_empty() {
        return Ok(ImageUpdateStatus {
            reference: reference.to_string(),
            state: "unknown".to_string(),
            local_digest,
            remote_digest,
            message: "仓库返回的清单没有 digest".to_string(),
        });
    }
    let state = if remote_digest == local_digest {
        "current"
    } else {
        "behind"
    };
    Ok(ImageUpdateStatus {
        reference: reference.to_string(),
        state: state.to_string(),
        local_digest,
        remote_digest,
        message: if state == "current" {
            "已是最新".to_string()
        } else {
            "仓库有更新".to_string()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_extraction_prefers_descriptor_digest() {
        let value = serde_json::json!({
            "Ref": "nginx:latest",
            "Descriptor": { "mediaType": "application/vnd.oci.image.index.v1+json", "digest": "sha256:abc" }
        });
        assert_eq!(first_digest(&value).as_deref(), Some("sha256:abc"));
        let list = serde_json::json!([{ "Descriptor": { "digest": "sha256:def" } }]);
        assert_eq!(first_digest(&list).as_deref(), Some("sha256:def"));
    }

    #[test]
    fn repo_digest_reads_first_entry() {
        assert_eq!(
            repo_digest(r#"["nginx@sha256:1234","other@sha256:5678"]"#).as_deref(),
            Some("sha256:1234")
        );
        assert_eq!(repo_digest("[]"), None);
        assert_eq!(repo_digest("not-json"), None);
    }

    #[test]
    fn dockerfile_path_cannot_escape_context() {
        let dir = std::env::temp_dir();
        assert!(validate_context(dir.to_str().unwrap(), Some("../Dockerfile")).is_err());
        assert!(validate_context(dir.to_str().unwrap(), Some("/etc/passwd")).is_err());
    }

    #[test]
    fn registry_errors_drop_signed_urls() {
        let message = registry_error(
            "Get \"https://mirror.example/blob?Expires=1&Signature=abc\": read tcp: connection reset by peer",
        );
        assert_eq!(
            message,
            "连接仓库超时或被重置，请检查服务器网络与加速源"
        );
        assert_eq!(
            registry_error("denied: requested access to the resource is denied"),
            "仓库拒绝访问，请先在镜像仓库页登录"
        );
    }
}
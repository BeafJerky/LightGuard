//! Docker 级写操作：预览阶段完成全部状态校验，执行时严格重校验目标，
//! 每次尝试都落审计；start/stop 可回退，其余不可回退。

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use russh::client::Handle;
use serde::{Deserialize, Serialize};

use crate::db::Database;
use crate::docker::dto::{
    DockerContainer, DockerImage, DockerPreview, DockerWriteInput, DockerWriteResult,
};
use crate::docker::exec::{self, ExecOptions};
use crate::docker::read::classify;
use crate::docker::{parse, validate};
use crate::guard::preview::PREVIEW_TTL;
use crate::guard::write::{audit_lost_after_success, audit_write_failed, outcome_for, record};
use crate::ssh::error::SshError;
use crate::ssh::session::ClientHandler;
use crate::ssh::sftp::client;
use crate::ssh::sftp::path::shell_quote;
use crate::ssh::state::AppState;

/// 写入的依据与展示名都由后端在预览阶段确定：执行时只认这份载荷，
/// 前端改不了目标，也换不了服务器。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WritePayload {
    pub server_id: String,
    /// 输入 kind，例如 `container.stop` / `image.pull`。
    pub kind: String,
    /// 执行依据：容器全 ID，或镜像全 ID（拉取时为镜像引用）。
    pub target: String,
    /// 展示名：容器名或镜像引用。
    pub display: String,
    /// 审计 op，例如 `docker.container.stop`。
    pub audit_op: String,
}

#[derive(Debug, Clone)]
pub struct PreviewView {
    pub operation: String,
    pub target: String,
    pub impact: String,
    pub rollback: String,
    pub irreversible: bool,
    pub details: Vec<String>,
}

pub fn verb_for(kind: &str) -> &'static str {
    match kind {
        "container.start" => "启动容器",
        "container.stop" => "停止容器",
        "container.restart" => "重启容器",
        "container.remove" => "删除容器",
        "image.pull" => "拉取镜像",
        "image.remove" => "删除镜像",
        _ => "Docker 操作",
    }
}

pub fn audit_op(kind: &str) -> String {
    format!("docker.{kind}")
}

pub fn evaluate_container(kind: &str, row: &DockerContainer) -> Result<PreviewView, SshError> {
    let name = if row.name.is_empty() {
        row.short_id.clone()
    } else {
        row.name.clone()
    };
    let running = matches!(row.state.as_str(), "running" | "paused" | "restarting");
    let (operation, impact, rollback, irreversible) = match kind {
        "container.start" => {
            if running {
                return Err(SshError::docker_failed("容器已在运行"));
            }
            (
                "启动容器",
                format!("将启动容器 {name}"),
                "执行后可在审计中回退（停止容器）".to_string(),
                false,
            )
        }
        "container.stop" => {
            if !running {
                return Err(SshError::docker_failed("容器未运行"));
            }
            (
                "停止容器",
                format!("将停止容器 {name}，其中的进程会被终止"),
                "执行后可在审计中回退（启动容器）".to_string(),
                false,
            )
        }
        "container.restart" => {
            if !running {
                return Err(SshError::docker_failed("容器未运行"));
            }
            (
                "重启容器",
                format!("将重启容器 {name}，服务会短暂中断"),
                "不可回退".to_string(),
                false,
            )
        }
        "container.remove" => {
            if running {
                return Err(SshError::docker_failed("请先停止容器再删除"));
            }
            (
                "删除容器",
                format!("将删除容器 {name} 及其可写层"),
                "不可回退".to_string(),
                true,
            )
        }
        _ => return Err(SshError::invalid_input("不支持的容器操作")),
    };
    let mut details = vec![
        format!("容器：{name}"),
        format!("镜像：{}", row.image),
        format!("当前状态：{}", row.status),
    ];
    if kind == "container.remove" {
        details.push("删除后不可恢复，容器内未挂载的数据会丢失".to_string());
    }
    Ok(PreviewView {
        operation: operation.to_string(),
        target: format!("{name}（{}）", row.short_id),
        impact,
        rollback,
        irreversible,
        details,
    })
}

pub fn evaluate_pull(reference: &str) -> PreviewView {
    PreviewView {
        operation: "拉取镜像".to_string(),
        target: reference.to_string(),
        impact: format!("将从镜像仓库拉取 {reference}，可能持续数分钟"),
        rollback: "不可回退；已下载的层会保留".to_string(),
        irreversible: true,
        details: vec![
            "拉取属于写操作，需要先解锁当前服务器".to_string(),
            "取消后已下载的层会保留，可再次拉取续传".to_string(),
        ],
    }
}

/// 按全 ID、短 ID 或 `repo:tag` 在前端拿到的镜像列表里定位一行。
pub fn resolve_image<'a>(reference: &str, rows: &'a [DockerImage]) -> Option<&'a DockerImage> {
    rows.iter().find(|image| {
        image.id == reference
            || image.short_id == reference
            || image.tags.iter().any(|tag| format!("{}:{}", image.repo, tag) == reference)
    })
}

pub fn evaluate_image_remove(
    reference: &str,
    rows: &[DockerImage],
) -> Result<PreviewView, SshError> {
    let Some(image) = resolve_image(reference, rows) else {
        return Err(SshError::docker_failed("镜像不存在或已变化"));
    };
    if image.containers > 0 {
        return Err(SshError::docker_failed("镜像正在被容器使用，无法删除"));
    }
    Ok(PreviewView {
        operation: "删除镜像".to_string(),
        target: reference.to_string(),
        impact: format!("将删除本地镜像 {reference}"),
        rollback: "不可回退".to_string(),
        irreversible: true,
        details: vec![
            format!("镜像 ID：{}", image.short_id),
            format!("大小：{}", image.size),
        ],
    })
}

/// 预览阶段：先把目标的实时状态读回来做全量校验，再把"冻结"的执行依据交给调用方。
pub async fn preview(
    handle: &Handle<ClientHandler>,
    server_id: &str,
    input: &DockerWriteInput,
) -> Result<(WritePayload, PreviewView), SshError> {
    let (view, target) = match input.kind.as_str() {
        "container.start" | "container.stop" | "container.restart" | "container.remove" => {
            let container_id = validate::container_id(&input.target)?;
            let output = exec::run(
                handle,
                "docker ps -a --no-trunc --format '{{json .}}'",
                ExecOptions {
                    timeout: Duration::from_secs(20),
                    max_bytes: 4 * 1024 * 1024,
                    stdin: None,
                    cancel: None,
                },
                |_| {},
            )
            .await?;
            if output.code != Some(0) {
                return Err(classify(&output, "读取容器"));
            }
            let rows = parse::containers(&output.stdout_text());
            let row = rows
                .iter()
                .find(|row| row.id == container_id)
                .ok_or_else(|| SshError::docker_failed("容器不存在或已变化"))?;
            (evaluate_container(&input.kind, row)?, container_id)
        }
        "image.pull" => {
            let reference = validate::image_reference(&input.target)?;
            (evaluate_pull(&reference), reference)
        }
        "image.remove" => {
            let reference = validate::image_reference(&input.target)?;
            let output = exec::run(
                handle,
                "docker images --no-trunc --format '{{json .}}'",
                ExecOptions {
                    timeout: Duration::from_secs(20),
                    max_bytes: 4 * 1024 * 1024,
                    stdin: None,
                    cancel: None,
                },
                |_| {},
            )
            .await?;
            if output.code != Some(0) {
                return Err(classify(&output, "读取镜像"));
            }
            let rows = parse::images(&output.stdout_text());
            // 把预览时看到的镜像冻结成全 ID：执行时标签再被重指向也只删这一张。
            let image = resolve_image(&reference, &rows)
                .ok_or_else(|| SshError::docker_failed("镜像不存在或已变化"))?;
            (evaluate_image_remove(&reference, &rows)?, image.id.clone())
        }
        _ => return Err(SshError::invalid_input("不支持的 Docker 操作")),
    };
    let payload = WritePayload {
        server_id: server_id.to_string(),
        kind: input.kind.clone(),
        target,
        display: view.target.clone(),
        audit_op: audit_op(&input.kind),
    };
    Ok((payload, view))
}

pub fn build_preview(
    preview_id: String,
    kind: &str,
    view: PreviewView,
    expires_at_ms: i64,
) -> DockerPreview {
    DockerPreview {
        preview_id,
        kind: kind.to_string(),
        operation: view.operation,
        target: view.target,
        impact: view.impact,
        rollback: view.rollback,
        irreversible: view.irreversible,
        expires_at_ms,
        details: view.details,
    }
}

pub fn preview_ttl_ms() -> i64 {
    PREVIEW_TTL.as_millis() as i64
}

pub fn action_script(payload: &WritePayload) -> String {
    // 用户输入在预览阶段已经过 `validate::*`，这里再统一走 shell 引用：
    // 任何 kind 都拼不出第二条命令。
    let quoted = shell_quote(&payload.target);
    match payload.kind.as_str() {
        "container.start" => format!("docker start {quoted}"),
        "container.stop" => format!("docker stop {quoted}"),
        "container.restart" => format!("docker restart {quoted}"),
        "container.remove" => format!("docker rm {quoted}"),
        "image.pull" => format!("docker pull {quoted}"),
        "image.remove" => format!("docker image rm {quoted}"),
        _ => String::new(),
    }
}

/// 审计参数只放标识符：容器全 ID / 镜像引用与展示名，绝不包含密码或环境变量。
pub fn audit_params(payload: &WritePayload) -> String {
    if let Some(action) = payload.kind.strip_prefix("container.") {
        serde_json::json!({
            "action": action,
            "containerId": payload.target,
            "name": payload.display,
        })
        .to_string()
    } else {
        serde_json::json!({
            "action": payload.kind.trim_start_matches("image."),
            "image": payload.display,
        })
        .to_string()
    }
}

pub fn rollback_plan(op: &str) -> Option<(&'static str, &'static str)> {
    match op {
        "docker.container.start" => Some(("stop", "docker.container.stop")),
        "docker.container.stop" => Some(("start", "docker.container.start")),
        _ => None,
    }
}

/// 执行阶段：目标重校验 → 固定脚本 → 每个结局都落审计。
/// 取消、非零退出、传输层错误都走各自的审计结果，绝不静默丢结果。
pub(crate) async fn execute(
    state: &AppState,
    db: &Database,
    payload: &WritePayload,
    permit_source: &str,
    rollback_of: Option<i64>,
    on_line: &mut (dyn FnMut(&str) + Send),
    cancel: Option<Arc<AtomicBool>>,
) -> Result<DockerWriteResult, SshError> {
    let started = Instant::now();
    let server_name = db
        .server_name(&payload.server_id)
        .unwrap_or_else(|_| payload.server_id.clone());
    let handle = client::session_handle(state, &payload.server_id).await?;
    // 预览与执行之间目标可能已经变化：先按实时状态重校验，再决定发不发命令。
    verify_target(&handle, payload).await?;
    let script = action_script(payload);
    let timeout = if payload.kind == "image.pull" {
        Duration::from_secs(900)
    } else {
        Duration::from_secs(120)
    };
    let outcome = exec::run(
        &handle,
        &script,
        ExecOptions {
            timeout,
            max_bytes: 4 * 1024 * 1024,
            stdin: None,
            cancel,
        },
        on_line,
    )
    .await;
    let params = audit_params(payload);
    let verb = verb_for(&payload.kind);

    match outcome {
        Ok(output) if output.canceled && output.code.is_none() => {
            let audit_id = record(
                db,
                &payload.server_id,
                &server_name,
                &payload.audit_op,
                &params,
                "failed",
                started,
                permit_source,
                rollback_of,
            )
            .map_err(|audit| audit_write_failed(SshError::docker_failed("用户取消"), &audit, false))?;
            Ok(DockerWriteResult {
                audit_id,
                target: payload.display.clone(),
                result: "failed".to_string(),
                message: "用户取消".to_string(),
            })
        }
        Ok(output) if output.code == Some(0) => {
            let audit_id = record(
                db,
                &payload.server_id,
                &server_name,
                &payload.audit_op,
                &params,
                "success",
                started,
                permit_source,
                rollback_of,
            )
            .map_err(|audit| audit_lost_after_success(&format!("{verb}已执行"), &audit))?;
            Ok(DockerWriteResult {
                audit_id,
                target: payload.display.clone(),
                result: "success".to_string(),
                message: format!("{verb}成功"),
            })
        }
        Ok(output) if output.code.is_none() => {
            // 连接在命令收尾前断开：命令可能已经跑完了，不能按失败记账。
            let error = SshError::docker_failed("连接中断，执行结果未知");
            let audit_id = record(
                db,
                &payload.server_id,
                &server_name,
                &payload.audit_op,
                &params,
                "unknown",
                started,
                permit_source,
                rollback_of,
            )
            .map_err(|audit| audit_write_failed(error.clone(), &audit, true))?;
            Ok(DockerWriteResult {
                audit_id,
                target: payload.display.clone(),
                result: "unknown".to_string(),
                message: error.message,
            })
        }
        Ok(output) => {
            let error = classify(&output, verb);
            let audit_id = record(
                db,
                &payload.server_id,
                &server_name,
                &payload.audit_op,
                &params,
                "failed",
                started,
                permit_source,
                rollback_of,
            )
            .map_err(|audit| audit_write_failed(error.clone(), &audit, false))?;
            Ok(DockerWriteResult {
                audit_id,
                target: payload.display.clone(),
                result: "failed".to_string(),
                message: error.message,
            })
        }
        Err(error) => {
            let result = outcome_for(&error);
            let audit_id = record(
                db,
                &payload.server_id,
                &server_name,
                &payload.audit_op,
                &params,
                result,
                started,
                permit_source,
                rollback_of,
            )
            .map_err(|audit| audit_write_failed(error.clone(), &audit, result == "unknown"))?;
            Ok(DockerWriteResult {
                audit_id,
                target: payload.display.clone(),
                result: result.to_string(),
                message: error.message,
            })
        }
    }
}

/// 执行前的目标重校验。容器比对完整的 64 位 ID（不接受前缀或改名），并重新检查
/// 状态是否仍允许这次操作；镜像则重新列举，确认仍然存在且没有被容器占用。
async fn verify_target(
    handle: &Handle<ClientHandler>,
    payload: &WritePayload,
) -> Result<(), SshError> {
    // 拉取不针对任何已存在的本地对象，没有可重校验的目标。
    if payload.kind == "image.pull" {
        return Ok(());
    }
    if payload.kind.starts_with("container.") {
        let script = format!(
            "docker inspect --format '{{{{.Id}}}}|{{{{.State.Status}}}}' {}",
            shell_quote(&payload.target)
        );
        let output = exec::run(
            handle,
            &script,
            ExecOptions {
                timeout: Duration::from_secs(20),
                max_bytes: 64 * 1024,
                stdin: None,
                cancel: None,
            },
            |_| {},
        )
        .await?;
        if output.code != Some(0) {
            return Err(SshError::docker_failed("目标已变化，请刷新后重试"));
        }
        let text = output.stdout_text();
        let line = text.lines().next().unwrap_or("");
        let (id, status) = line
            .split_once('|')
            .ok_or_else(|| SshError::docker_failed("目标已变化，请刷新后重试"))?;
        if id.trim() != payload.target {
            return Err(SshError::docker_failed("目标已变化，请刷新后重试"));
        }
        let running = matches!(status.trim(), "running" | "paused" | "restarting");
        match payload.kind.as_str() {
            "container.start" if running => {
                return Err(SshError::docker_failed("容器已在运行"))
            }
            "container.stop" | "container.restart" if !running => {
                return Err(SshError::docker_failed("容器未运行"))
            }
            "container.remove" if running => {
                return Err(SshError::docker_failed("请先停止容器再删除"))
            }
            _ => {}
        }
        return Ok(());
    }
    let output = exec::run(
        handle,
        "docker images --no-trunc --format '{{json .}}'",
        ExecOptions {
            timeout: Duration::from_secs(20),
            max_bytes: 4 * 1024 * 1024,
            stdin: None,
            cancel: None,
        },
        |_| {},
    )
    .await?;
    if output.code != Some(0) {
        return Err(classify(&output, "读取镜像"));
    }
    let rows = parse::images(&output.stdout_text());
    // payload.target 已是预览时冻结的镜像全 ID。
    evaluate_image_remove(&payload.target, &rows)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::dto::{DockerContainer, DockerImage};

    fn container(state: &str) -> DockerContainer {
        DockerContainer {
            id: "a".repeat(64),
            short_id: "aaaaaaaaaaaa".to_string(),
            name: "shop-web-1".to_string(),
            image: "nginx:1.27".to_string(),
            status: format!("status {state}"),
            state: state.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn container_preview_rejects_state_conflicts() {
        assert_eq!(
            evaluate_container("container.start", &container("running"))
                .unwrap_err()
                .message,
            "容器已在运行"
        );
        assert_eq!(
            evaluate_container("container.stop", &container("exited"))
                .unwrap_err()
                .message,
            "容器未运行"
        );
        assert_eq!(
            evaluate_container("container.remove", &container("running"))
                .unwrap_err()
                .message,
            "请先停止容器再删除"
        );
        assert!(evaluate_container("container.start", &container("exited")).is_ok());
        assert!(evaluate_container("container.stop", &container("running")).is_ok());
        assert!(evaluate_container("container.restart", &container("running")).is_ok());
        assert!(evaluate_container("container.remove", &container("exited")).is_ok());
    }

    #[test]
    fn remove_preview_is_irreversible_and_names_the_container() {
        let view = evaluate_container("container.remove", &container("exited")).unwrap();
        assert_eq!(view.operation, "删除容器");
        assert!(view.irreversible);
        assert!(view.target.contains("shop-web-1"));
        assert!(view.details.iter().any(|line| line.contains("不可恢复")));
        assert!(view.rollback.contains("不可回退"));
    }

    #[test]
    fn start_and_stop_are_rollbackable() {
        assert_eq!(
            rollback_plan("docker.container.start"),
            Some(("stop", "docker.container.stop"))
        );
        assert_eq!(
            rollback_plan("docker.container.stop"),
            Some(("start", "docker.container.start"))
        );
        assert_eq!(rollback_plan("docker.container.remove"), None);
        assert_eq!(rollback_plan("docker.service.stop"), None);
    }

    #[test]
    fn image_remove_rejects_images_in_use() {
        let image = DockerImage {
            id: "c".repeat(64),
            short_id: "cccccccccccc".to_string(),
            repo: "nginx".to_string(),
            tags: vec!["latest".to_string()],
            size: "187MB".to_string(),
            created_at: "2026-10-01".to_string(),
            containers: 2,
            dangling: false,
        };
        assert_eq!(
            evaluate_image_remove("nginx:latest", &[image])
                .unwrap_err()
                .message,
            "镜像正在被容器使用，无法删除"
        );
    }

    #[test]
    fn image_remove_accepts_available_images_and_rejects_unknown_ones() {
        let image = DockerImage {
            id: "c".repeat(64),
            short_id: "cccccccccccc".to_string(),
            repo: "nginx".to_string(),
            tags: vec!["latest".to_string()],
            size: "187MB".to_string(),
            created_at: "2026-10-01".to_string(),
            containers: 0,
            dangling: false,
        };
        assert!(evaluate_image_remove("nginx:latest", &[image.clone()]).is_ok());
        assert!(evaluate_image_remove("cccccccccccc", &[image.clone()]).is_ok());
        assert_eq!(
            evaluate_image_remove("redis:7", &[image]).unwrap_err().code,
            "DOCKER_FAILED"
        );
    }

    #[test]
    fn audit_params_hold_identifiers_not_secrets() {
        let payload = WritePayload {
            server_id: "srv".to_string(),
            kind: "image.pull".to_string(),
            target: "ghcr.io/acme/api:1.2".to_string(),
            display: "ghcr.io/acme/api:1.2".to_string(),
            audit_op: "docker.image.pull".to_string(),
        };
        let params = audit_params(&payload);
        assert_eq!(
            params,
            r#"{"action":"pull","image":"ghcr.io/acme/api:1.2"}"#
        );
        assert!(!params.to_lowercase().contains("password"));
    }

    #[test]
    fn pull_script_quotes_the_reference() {
        let payload = WritePayload {
            server_id: "srv".to_string(),
            kind: "image.pull".to_string(),
            target: "nginx:1.27".to_string(),
            display: "nginx:1.27".to_string(),
            audit_op: "docker.image.pull".to_string(),
        };
        assert_eq!(action_script(&payload), "docker pull 'nginx:1.27'");
    }
}

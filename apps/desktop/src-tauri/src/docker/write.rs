//! Docker 级写操作：预览阶段完成全部状态校验，执行时严格重校验目标，
//! 每次尝试都落审计；start/stop 可回退，其余不可回退。

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use russh::client::Handle;
use serde::{Deserialize, Serialize};

fn validate_port(value: &str) -> Result<String, SshError> {
    let (spec, proto) = match value.split_once('/') {
        Some((spec, proto)) => (spec, Some(proto)),
        None => (value, None),
    };
    let (host, container) = spec
        .split_once(':')
        .ok_or_else(|| SshError::invalid_input("端口映射格式应为 宿主端口:容器端口"))?;
    let parse_port = |input: &str| -> Result<u16, SshError> {
        input
            .parse::<u16>()
            .ok()
            .filter(|port| *port > 0)
            .ok_or_else(|| SshError::invalid_input("端口必须是 1–65535"))
    };
    parse_port(host.trim())?;
    parse_port(container.trim())?;
    if let Some(proto) = proto {
        if !matches!(proto, "tcp" | "udp") {
            return Err(SshError::invalid_input("端口协议只支持 tcp 或 udp"));
        }
    }
    Ok(value.trim().to_string())
}

fn validate_env(value: &str) -> Result<String, SshError> {
    let (key, val) = value
        .split_once('=')
        .ok_or_else(|| SshError::invalid_input("环境变量格式应为 键=值"))?;
    let key = key.trim();
    if key.is_empty()
        || !key
            .chars()
            .next()
            .map(|ch| ch.is_ascii_alphabetic() || ch == '_')
            .unwrap_or(false)
        || !key
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
    {
        return Err(SshError::invalid_input("环境变量名只能包含字母、数字与下划线"));
    }
    if val.chars().any(char::is_control) || val.len() > 4096 {
        return Err(SshError::invalid_input("环境变量值包含非法字符或过长"));
    }
    Ok(format!("{key}={val}"))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateSpec {
    image: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    ports: Vec<String>,
    #[serde(default)]
    env: Vec<String>,
    #[serde(default)]
    restart: Option<String>,
    #[serde(default)]
    command: Option<String>,
    /// 由 `command` 拆分得到，执行时逐参数引用，避免整串被当成一个程序名。
    #[serde(skip)]
    command_args: Vec<String>,
}

/// 按 POSIX shell 习惯拆分启动命令：空白分隔，支持单/双引号与反斜杠转义。
fn split_command(input: &str) -> Result<Vec<String>, SshError> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut started = false;
    for character in input.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            started = true;
            continue;
        }
        match quote {
            Some(mark) => {
                if character == mark {
                    quote = None;
                } else if character == '\\' && mark == '"' {
                    escaped = true;
                } else {
                    current.push(character);
                }
                started = true;
            }
            None => match character {
                '\'' | '"' => {
                    quote = Some(character);
                    started = true;
                }
                '\\' => {
                    escaped = true;
                    started = true;
                }
                value if value.is_whitespace() => {
                    if started {
                        args.push(std::mem::take(&mut current));
                        started = false;
                    }
                }
                value => {
                    current.push(value);
                    started = true;
                }
            },
        }
    }
    if escaped || quote.is_some() {
        return Err(SshError::invalid_input("启动命令的引号或转义未闭合"));
    }
    if started {
        args.push(current);
    }
    if args.len() > 64 {
        return Err(SshError::invalid_input("启动命令参数过多"));
    }
    if args.iter().any(|arg: &String| arg.len() > 128) {
        return Err(SshError::invalid_input("启动命令的单个参数过长"));
    }
    Ok(args)
}

fn parse_create_spec(input: &DockerWriteInput) -> Result<CreateSpec, SshError> {
    let mut spec: CreateSpec = serde_json::from_value(input.payload.clone())
        .map_err(|_| SshError::invalid_input("新建容器参数无效"))?;
    spec.image = validate::image_reference(&spec.image)?;
    if let Some(name) = &spec.name {
        spec.name = Some(validate::docker_name(name, "容器名")?);
    }
    if spec.ports.len() > 20 || spec.env.len() > 50 {
        return Err(SshError::invalid_input("端口或环境变量数量过多"));
    }
    spec.ports = spec
        .ports
        .iter()
        .map(|port| validate_port(port))
        .collect::<Result<_, _>>()?;
    spec.env = spec
        .env
        .iter()
        .map(|pair| validate_env(pair))
        .collect::<Result<_, _>>()?;
    spec.restart = match spec.restart.as_deref() {
        None | Some("") | Some("no") => Some("no".to_string()),
        Some("always") | Some("unless-stopped") | Some("on-failure") => {
            spec.restart.clone()
        }
        Some(_) => return Err(SshError::invalid_input("不支持的重启策略")),
    };
    if let Some(command) = &spec.command {
        if command.len() > 512 || command.chars().any(char::is_control) {
            return Err(SshError::invalid_input("启动命令包含非法字符或过长"));
        }
        spec.command_args = split_command(command)?;
        if spec.command_args.is_empty() {
            spec.command = None;
        }
    }
    Ok(spec)
}

use crate::db::Database;
use crate::docker::dto::{
    DockerContainer, DockerImage, DockerPreview, DockerWriteInput, DockerWriteResult,
};
use crate::docker::exec::{self, ExecOptions};
use crate::docker::image_ops;
use crate::docker::read::{self, classify};
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
    /// 额外、已校验的参数（如网络驱动）；不进入审计凭据。
    #[serde(default)]
    pub extra: serde_json::Value,
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
        "container.create" => "新建容器",
        "image.pull" => "拉取镜像",
        "image.remove" => "删除镜像",
        "image.push" => "推送镜像",
        "image.prune" => "清理悬空镜像",
        "image.import" => "导入镜像",
        "image.build" => "构建镜像",
        "compose.start" => "启动 Compose 项目",
        "compose.stop" => "停止 Compose 项目",
        "network.create" => "创建网络",
        "network.remove" => "删除网络",
        "network.prune" => "清理未使用网络",
        "volume.create" => "创建数据卷",
        "volume.remove" => "删除数据卷",
        "volume.prune" => "清理未使用数据卷",
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
    elevated: bool,
) -> Result<(WritePayload, PreviewView), SshError> {
    let (view, target, extra) = match input.kind.as_str() {
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
                    elevated,
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
            (
                evaluate_container(&input.kind, row)?,
                container_id,
                serde_json::json!({}),
            )
        }
        "container.create" => {
            let spec = parse_create_spec(input)?;
            let images = read::images(handle, elevated).await?;
            let image = images
                .iter()
                .find(|image| {
                    image.id == spec.image
                        || image.short_id == spec.image
                        || image.repo == spec.image
                        || image
                            .tags
                            .iter()
                            .any(|tag| format!("{}:{}", image.repo, tag) == spec.image)
                })
                .ok_or_else(|| SshError::docker_failed("本地镜像不存在，请先拉取镜像"))?;
            if let Some(name) = &spec.name {
                let containers = read::containers(handle, elevated).await?;
                if containers.iter().any(|row| &row.name == name) {
                    return Err(SshError::docker_failed("容器名已存在"));
                }
            }
            let env_keys: Vec<String> = spec
                .env
                .iter()
                .filter_map(|pair| pair.split_once('=').map(|(key, _)| key.to_string()))
                .collect();
            let mut details = vec![format!("镜像：{}", spec.image)];
            if let Some(name) = &spec.name {
                details.push(format!("名称：{name}"));
            }
            if !spec.ports.is_empty() {
                details.push(format!("端口：{}", spec.ports.join(", ")));
            }
            if !env_keys.is_empty() {
                details.push(format!("环境变量：{}", env_keys.join(", ")));
            }
            details.push(format!(
                "重启策略：{}",
                spec.restart.clone().unwrap_or_else(|| "no".to_string())
            ));
            if let Some(command) = &spec.command {
                details.push(format!("启动命令：{command}"));
            }
            (
                PreviewView {
                    operation: "新建容器".to_string(),
                    target: image.id.clone(),
                    impact: format!(
                        "将以后台模式创建并启动容器{}",
                        spec.name
                            .as_ref()
                            .map(|name| format!(" {name}"))
                            .unwrap_or_default()
                    ),
                    rollback: "不提供回退；可在容器页面手动删除".to_string(),
                    irreversible: false,
                    details,
                },
                image.id.clone(),
                serde_json::json!({
                    "name": spec.name,
                    "ports": spec.ports,
                    "env": spec.env,
                    "restart": spec.restart,
                    "command": spec.command,
                    "args": spec.command_args,
                }),
            )
        }
        "image.pull" => {
            let reference = validate::image_reference(&input.target)?;
            (evaluate_pull(&reference), reference, serde_json::json!({}))
        }
        "image.push" => {
            let reference = validate::image_reference(&input.target)?;
            let rows = read::images(handle, elevated).await?;
            if resolve_image(&reference, &rows).is_none() {
                return Err(SshError::docker_failed("镜像不存在或已变化"));
            }
            (
                PreviewView {
                    operation: "推送镜像".to_string(),
                    target: reference.clone(),
                    impact: format!("将把 {reference} 推送到远端仓库，可能覆盖同名远端标签"),
                    rollback: "不提供回退；需要时重新推送旧标签".to_string(),
                    irreversible: false,
                    details: vec![
                        format!("镜像：{reference}"),
                        "需要已对该仓库执行登录".to_string(),
                    ],
                },
                reference,
                serde_json::json!({}),
            )
        }
        "image.prune" => (
            PreviewView {
                operation: "清理悬空镜像".to_string(),
                target: "悬空镜像".to_string(),
                impact: "将删除所有未被打标签且没有容器引用的镜像层".to_string(),
                rollback: "不可回退".to_string(),
                irreversible: true,
                details: vec!["不会删除有标签的镜像".to_string()],
            },
            String::new(),
            serde_json::json!({}),
        ),
        "image.import" => {
            let local = image_ops::validate_archive(&input.target)?;
            let name = local
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| "image.tar".to_string());
            let size = std::fs::metadata(&local)
                .map(|meta| meta.len())
                .unwrap_or(0);
            (
                PreviewView {
                    operation: "导入镜像".to_string(),
                    target: name.clone(),
                    impact: "将把本地归档上传到服务器并导入，可能新增多个镜像标签".to_string(),
                    rollback: "不提供回退；可手动删除导入的镜像".to_string(),
                    irreversible: false,
                    details: vec![
                        format!("归档：{name}"),
                        format!("大小：{} MiB", size / 1024 / 1024),
                        "归档经加密通道上传，导入后立即删除临时文件".to_string(),
                    ],
                },
                String::new(),
                serde_json::json!({
                    "localPath": local.to_string_lossy(),
                    "name": name,
                }),
            )
        }
        "image.build" => {
            let tag = validate::image_reference(&input.target)?;
            let context = input
                .payload
                .get("contextPath")
                .and_then(|value| value.as_str())
                .ok_or_else(|| SshError::invalid_input("请选择构建上下文目录"))?;
            let dockerfile = input
                .payload
                .get("dockerfile")
                .and_then(|value| value.as_str())
                .unwrap_or("Dockerfile");
            let root = image_ops::validate_context(context, Some(dockerfile))?;
            let folder = root
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_else(|| "context".to_string());
            (
                PreviewView {
                    operation: "构建镜像".to_string(),
                    target: tag.clone(),
                    impact: format!("将用 {folder} 作为上下文构建并标记镜像 {tag}"),
                    rollback: "不提供回退；可手动删除构建出的镜像".to_string(),
                    irreversible: false,
                    details: vec![
                        format!("镜像标签：{tag}"),
                        format!("上下文：{}", root.display()),
                        format!("Dockerfile：{dockerfile}"),
                        "上下文打包上传后经 stdin 构建，结束后删除临时归档".to_string(),
                    ],
                },
                tag,
                serde_json::json!({
                    "contextPath": root.to_string_lossy(),
                    "dockerfile": dockerfile,
                }),
            )
        }
        "image.remove" => {
            let reference = validate::image_reference(&input.target)?;
            let output = exec::run(
                handle,
                "docker images --all --no-trunc --format '{{json .}}'",
                ExecOptions {
                    timeout: Duration::from_secs(20),
                    max_bytes: 4 * 1024 * 1024,
                    stdin: None,
                    cancel: None,
                    elevated,
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
            (
                evaluate_image_remove(&reference, &rows)?,
                image.id.clone(),
                serde_json::json!({}),
            )
        }
        "compose.start" | "compose.stop" => {
            let name = validate::docker_name(&input.target, "Compose 项目名")?;
            let projects = read::compose_projects(handle, elevated).await?;
            let project = projects
                .iter()
                .find(|project| project.name == name)
                .ok_or_else(|| SshError::docker_failed("Compose 项目不存在或已变化"))?;
            let running = project.status.to_ascii_lowercase().contains("running");
            let starting = input.kind == "compose.start";
            if starting && running {
                return Err(SshError::docker_failed("Compose 项目已在运行"));
            }
            if !starting && !running {
                return Err(SshError::docker_failed("Compose 项目未运行"));
            }
            (
                PreviewView {
                    operation: if starting {
                        "启动 Compose 项目"
                    } else {
                        "停止 Compose 项目"
                    }
                    .to_string(),
                    target: name.clone(),
                    impact: if starting {
                        format!("将启动 Compose 项目 {name} 的全部服务")
                    } else {
                        format!("将停止 Compose 项目 {name} 的全部服务，相关进程会被终止")
                    },
                    rollback: if starting {
                        "不提供回退；可在 Compose 页面手动启动".to_string()
                    } else {
                        "不提供回退；可在 Compose 页面手动停止".to_string()
                    },
                    irreversible: false,
                    details: vec![format!("状态：{}", project.status)],
                },
                name,
                serde_json::json!({}),
            )
        }
        "network.create" => {
            let name = validate::docker_name(&input.target, "网络名")?;
            let driver = validate::network_driver(
                input
                    .payload
                    .get("driver")
                    .and_then(|value| value.as_str())
                    .unwrap_or("bridge"),
            )?;
            let rows = read::networks(handle, elevated).await?;
            if rows.iter().any(|row| row.name == name) {
                return Err(SshError::docker_failed("网络已存在"));
            }
            (
                PreviewView {
                    operation: "创建网络".to_string(),
                    target: name.clone(),
                    impact: format!("将创建 Docker 网络 {name}（驱动 {driver}）"),
                    rollback: "不提供回退；可在网络页面手动删除".to_string(),
                    irreversible: false,
                    details: vec![format!("驱动：{driver}")],
                },
                name,
                serde_json::json!({ "driver": driver }),
            )
        }
        "network.remove" => {
            let name = validate::docker_name(&input.target, "网络名")?;
            let rows = read::networks(handle, elevated).await?;
            let row = rows
                .iter()
                .find(|row| row.name == name)
                .ok_or_else(|| SshError::docker_failed("网络不存在或已变化"))?;
            if !row.containers.is_empty() {
                return Err(SshError::docker_failed(format!(
                    "网络仍有 {} 个容器连接，不能删除",
                    row.containers.len()
                )));
            }
            (
                PreviewView {
                    operation: "删除网络".to_string(),
                    target: name.clone(),
                    impact: format!("将删除 Docker 网络 {name}，不可恢复"),
                    rollback: "删除不可回退".to_string(),
                    irreversible: true,
                    details: vec![format!("驱动：{}", row.driver)],
                },
                name,
                serde_json::json!({}),
            )
        }
        "network.prune" => (
            PreviewView {
                operation: "清理未使用网络".to_string(),
                target: "未使用网络".to_string(),
                impact: "将删除所有未被容器使用的自定义网络".to_string(),
                rollback: "删除不可回退".to_string(),
                irreversible: true,
                details: Vec::new(),
            },
            String::new(),
            serde_json::json!({}),
        ),
        "volume.create" => {
            let name = validate::docker_name(&input.target, "数据卷名")?;
            let rows = read::volumes(handle, elevated).await?;
            if rows.iter().any(|row| row.name == name) {
                return Err(SshError::docker_failed("数据卷已存在"));
            }
            (
                PreviewView {
                    operation: "创建数据卷".to_string(),
                    target: name.clone(),
                    impact: format!("将创建 Docker 数据卷 {name}"),
                    rollback: "不提供回退；可在数据卷页面手动删除".to_string(),
                    irreversible: false,
                    details: Vec::new(),
                },
                name,
                serde_json::json!({}),
            )
        }
        "volume.remove" => {
            let name = validate::docker_name(&input.target, "数据卷名")?;
            let rows = read::volumes(handle, elevated).await?;
            let row = rows
                .iter()
                .find(|row| row.name == name)
                .ok_or_else(|| SshError::docker_failed("数据卷不存在或已变化"))?;
            if !row.used_by.is_empty() {
                return Err(SshError::docker_failed(format!(
                    "数据卷仍被 {} 个容器使用，不能删除",
                    row.used_by.len()
                )));
            }
            (
                PreviewView {
                    operation: "删除数据卷".to_string(),
                    target: name.clone(),
                    impact: format!("将删除 Docker 数据卷 {name}，其中数据不可恢复"),
                    rollback: "删除不可回退".to_string(),
                    irreversible: true,
                    details: vec![format!("挂载点：{}", row.mountpoint)],
                },
                name,
                serde_json::json!({}),
            )
        }
        "volume.prune" => (
            PreviewView {
                operation: "清理未使用数据卷".to_string(),
                target: "未使用数据卷".to_string(),
                impact: "将删除所有未被容器使用的数据卷，其中数据不可恢复".to_string(),
                rollback: "删除不可回退".to_string(),
                irreversible: true,
                details: Vec::new(),
            },
            String::new(),
            serde_json::json!({}),
        ),
        _ => return Err(SshError::invalid_input("不支持的 Docker 操作")),
    };
    let payload = WritePayload {
        server_id: server_id.to_string(),
        kind: input.kind.clone(),
        target,
        display: view.target.clone(),
        audit_op: audit_op(&input.kind),
        extra,
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
    action_script_for(payload, None)
}

/// 导入/构建需要在执行前拿到上传后的远端归档路径，因此单独传入。
pub fn action_script_for(payload: &WritePayload, archive: Option<&str>) -> String {
    // 用户输入在预览阶段已经过 `validate::*`，这里再统一走 shell 引用：
    // 任何 kind 都拼不出第二条命令。
    let quoted = shell_quote(&payload.target);
    match payload.kind.as_str() {
        "container.start" => format!("docker start {quoted}"),
        "container.stop" => format!("docker stop {quoted}"),
        "container.restart" => format!("docker restart {quoted}"),
        "container.remove" => format!("docker rm {quoted}"),
        "container.create" => {
            let mut args = vec!["docker run -d".to_string()];
            if let Some(name) = payload.extra.get("name").and_then(|value| value.as_str()) {
                args.push(format!("--name {}", shell_quote(name)));
            }
            if let Some(restart) = payload
                .extra
                .get("restart")
                .and_then(|value| value.as_str())
                .filter(|value| *value != "no")
            {
                args.push(format!("--restart {}", shell_quote(restart)));
            }
            if let Some(ports) = payload.extra.get("ports").and_then(|value| value.as_array()) {
                for port in ports.iter().filter_map(|value| value.as_str()) {
                    args.push(format!("-p {}", shell_quote(port)));
                }
            }
            if let Some(env) = payload.extra.get("env").and_then(|value| value.as_array()) {
                for pair in env.iter().filter_map(|value| value.as_str()) {
                    args.push(format!("-e {}", shell_quote(pair)));
                }
            }
            args.push(quoted.clone());
            let mut command_args = payload
                .extra
                .get("args")
                .and_then(|value| value.as_array())
                .map(|values| {
                    values
                        .iter()
                        .filter_map(|value| value.as_str())
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if command_args.is_empty() {
                // 兼容旧预览：仅有一个字符串命令时保持原样。
                if let Some(command) = payload
                    .extra
                    .get("command")
                    .and_then(|value| value.as_str())
                    .filter(|value| !value.is_empty())
                {
                    command_args.push(command.to_string());
                }
            }
            for argument in command_args {
                args.push(shell_quote(&argument));
            }
            args.join(" ")
        }
        "image.pull" => format!("docker pull {quoted}"),
        "image.remove" => format!("docker image rm {quoted}"),
        "image.push" => format!("docker push {quoted}"),
        "image.prune" => "docker image prune -f".to_string(),
        "image.import" => {
            let Some(remote) = archive else {
                return String::new();
            };
            let remote = shell_quote(remote);
            format!("docker image load -i {remote}; rm -f {remote}")
        }
        "image.build" => {
            let Some(remote) = archive else {
                return String::new();
            };
            let remote = shell_quote(remote);
            let dockerfile = payload
                .extra
                .get("dockerfile")
                .and_then(|value| value.as_str())
                .unwrap_or("Dockerfile");
            let mut script = format!("docker build -t {quoted}");
            if dockerfile != "Dockerfile" {
                script.push_str(&format!(" -f {}", shell_quote(dockerfile)));
            }
            script.push_str(&format!(" - < {remote}; rm -f {remote}"));
            script
        }
        "compose.start" => format!("docker compose -p {quoted} start"),
        "compose.stop" => format!("docker compose -p {quoted} stop"),
        "network.create" => {
            let driver = payload
                .extra
                .get("driver")
                .and_then(|value| value.as_str())
                .unwrap_or("bridge");
            format!(
                "docker network create --driver {} {quoted}",
                shell_quote(driver)
            )
        }
        "network.remove" => format!("docker network rm {quoted}"),
        "network.prune" => "docker network prune -f".to_string(),
        "volume.create" => format!("docker volume create {quoted}"),
        "volume.remove" => format!("docker volume rm {quoted}"),
        "volume.prune" => "docker volume prune -f".to_string(),
        _ => String::new(),
    }
}

/// 审计参数只放标识符：容器全 ID / 镜像引用与展示名，绝不包含密码或环境变量。
pub fn audit_params(payload: &WritePayload) -> String {
    if payload.kind == "container.create" {
        serde_json::json!({
            "action": "create",
            "image": payload.target,
            "name": payload.extra.get("name").and_then(|value| value.as_str()).unwrap_or(""),
        })
        .to_string()
    } else if let Some(action) = payload.kind.strip_prefix("container.") {
        serde_json::json!({
            "action": action,
            "containerId": payload.target,
            "name": payload.display,
        })
        .to_string()
    } else if payload.kind.starts_with("compose.")
        || payload.kind.starts_with("network.")
        || payload.kind.starts_with("volume.")
    {
        serde_json::json!({
            "action": payload.kind,
            "target": payload.display,
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
        "docker.compose.start" => Some(("stop", "docker.compose.stop")),
        "docker.compose.stop" => Some(("start", "docker.compose.start")),
        "docker.network.create" => Some(("remove", "docker.network.remove")),
        "docker.volume.create" => Some(("remove", "docker.volume.remove")),
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
    let elevated = crate::ssh::session::docker_elevated(state, &payload.server_id).await;
    // 预览与执行之间目标可能已经变化：先按实时状态重校验，再决定发不发命令。
    verify_target(&handle, payload, elevated).await?;
    // 导入/构建先把本地归档传到远端 /tmp；上传失败也要落一条失败审计。
    let prepared = match payload.kind.as_str() {
        "image.import" => {
            let local = payload
                .extra
                .get("localPath")
                .and_then(|value| value.as_str())
                .ok_or_else(|| SshError::preview_failed("预览缺少本地归档路径"))?;
            image_ops::prepare_import(state, &payload.server_id, local, on_line)
                .await
                .map(Some)
        }
        "image.build" => {
            let context = payload
                .extra
                .get("contextPath")
                .and_then(|value| value.as_str())
                .ok_or_else(|| SshError::preview_failed("预览缺少构建上下文"))?;
            let dockerfile = payload
                .extra
                .get("dockerfile")
                .and_then(|value| value.as_str());
            image_ops::prepare_build(state, &payload.server_id, context, dockerfile, on_line)
                .await
                .map(Some)
        }
        _ => Ok(None),
    };
    let archive = match prepared {
        Ok(value) => value,
        Err(error) => {
            let params = audit_params(payload);
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
            let _ = audit_id;
            return Err(error);
        }
    };
    let script = action_script_for(payload, archive.as_deref());
    let timeout = match payload.kind.as_str() {
        "image.pull" => Duration::from_secs(900),
        "image.push" | "image.import" | "image.build" => Duration::from_secs(1800),
        _ => Duration::from_secs(120),
    };
    let outcome = exec::run(
        &handle,
        &script,
        ExecOptions {
            timeout,
            max_bytes: if matches!(payload.kind.as_str(), "image.import" | "image.build") {
                8 * 1024 * 1024
            } else {
                4 * 1024 * 1024
            },
            stdin: None,
            cancel,
            elevated,
        },
        on_line,
    )
    .await;
    if let Some(remote) = &archive {
        image_ops::cleanup_remote(state, &payload.server_id, remote).await;
    }
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
    elevated: bool,
) -> Result<(), SshError> {
    // 拉取不针对任何已存在的本地对象；导入/构建/清理的目标由命令自身决定。
    if matches!(
        payload.kind.as_str(),
        "image.pull" | "image.import" | "image.build" | "image.prune"
    ) {
        return Ok(());
    }
    if payload.kind == "container.create"
        || payload.kind.starts_with("compose.")
        || payload.kind.starts_with("network.")
        || payload.kind.starts_with("volume.")
    {
        // 项目/网络/卷的名称在预览与执行间由 Docker 命令自身复核；
        // 这些操作不针对需要冻结的容器全 ID。
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
                elevated,
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
        "docker images --all --no-trunc --format '{{json .}}'",
        ExecOptions {
            timeout: Duration::from_secs(20),
            max_bytes: 4 * 1024 * 1024,
            stdin: None,
            cancel: None,
            elevated,
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
        assert_eq!(
            rollback_plan("docker.compose.start"),
            Some(("stop", "docker.compose.stop"))
        );
        assert_eq!(
            rollback_plan("docker.network.create"),
            Some(("remove", "docker.network.remove"))
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
            extra: serde_json::json!({}),
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
            extra: serde_json::json!({}),
        };
        assert_eq!(action_script(&payload), "docker pull 'nginx:1.27'");
    }

    #[test]
    fn compose_network_volume_scripts_are_fixed() {
        let compose = WritePayload {
            server_id: "srv".to_string(),
            kind: "compose.start".to_string(),
            target: "shop".to_string(),
            display: "shop".to_string(),
            audit_op: "docker.compose.start".to_string(),
            extra: serde_json::json!({}),
        };
        assert_eq!(action_script(&compose), "docker compose -p 'shop' start");

        let network = WritePayload {
            server_id: "srv".to_string(),
            kind: "network.create".to_string(),
            target: "lg-net".to_string(),
            display: "lg-net".to_string(),
            audit_op: "docker.network.create".to_string(),
            extra: serde_json::json!({ "driver": "bridge" }),
        };
        assert_eq!(
            action_script(&network),
            "docker network create --driver 'bridge' 'lg-net'"
        );

        let volume = WritePayload {
            server_id: "srv".to_string(),
            kind: "volume.remove".to_string(),
            target: "lg-vol".to_string(),
            display: "lg-vol".to_string(),
            audit_op: "docker.volume.remove".to_string(),
            extra: serde_json::json!({}),
        };
        assert_eq!(action_script(&volume), "docker volume rm 'lg-vol'");
    }

    #[test]
    fn create_script_splits_command_into_separate_arguments() {
        let payload = WritePayload {
            server_id: "srv".to_string(),
            kind: "container.create".to_string(),
            target: "d9e853e87e55".to_string(),
            display: "lg-create-test".to_string(),
            audit_op: "docker.container.create".to_string(),
            extra: serde_json::json!({
                "name": "lg-create-test",
                "restart": "no",
                "ports": [],
                "env": [],
                "args": ["sleep", "60"],
            }),
        };
        assert_eq!(
            action_script(&payload),
            "docker run -d --name 'lg-create-test' 'd9e853e87e55' 'sleep' '60'"
        );
    }

    #[test]
    fn split_command_handles_quotes_and_rejects_unbalanced_input() {
        assert_eq!(
            split_command("sh -c 'echo hi'").unwrap(),
            vec!["sh".to_string(), "-c".to_string(), "echo hi".to_string()]
        );
        assert_eq!(
            split_command(r#"echo "a b" c\ d"#).unwrap(),
            vec!["echo".to_string(), "a b".to_string(), "c d".to_string()]
        );
        assert_eq!(split_command("   ").unwrap(), Vec::<String>::new());
        assert!(split_command("echo 'x").is_err());
    }

    fn image_payload(kind: &str, target: &str, extra: serde_json::Value) -> WritePayload {
        WritePayload {
            server_id: "srv".to_string(),
            kind: kind.to_string(),
            target: target.to_string(),
            display: target.to_string(),
            audit_op: format!("docker.{kind}"),
            extra,
        }
    }

    #[test]
    fn push_and_prune_scripts_are_fixed() {
        assert_eq!(
            action_script(&image_payload("image.push", "nginx:1.27", serde_json::json!({}))),
            "docker push 'nginx:1.27'"
        );
        assert_eq!(
            action_script(&image_payload("image.prune", "", serde_json::json!({}))),
            "docker image prune -f"
        );
    }

    #[test]
    fn archive_scripts_consume_and_remove_the_remote_file() {
        let import = image_payload(
            "image.import",
            "",
            serde_json::json!({ "localPath": "C:/tmp/a.tar" }),
        );
        assert_eq!(
            action_script_for(&import, Some("/tmp/lightguard-import-1.tar")),
            "docker image load -i '/tmp/lightguard-import-1.tar'; rm -f '/tmp/lightguard-import-1.tar'"
        );
        assert_eq!(action_script(&import), "");

        let build = image_payload(
            "image.build",
            "lg/demo:1",
            serde_json::json!({ "contextPath": "C:/ctx", "dockerfile": "docker/Dockerfile" }),
        );
        assert_eq!(
            action_script_for(&build, Some("/tmp/ctx.tar.gz")),
            "docker build -t 'lg/demo:1' -f 'docker/Dockerfile' - < '/tmp/ctx.tar.gz'; rm -f '/tmp/ctx.tar.gz'"
        );
    }
}

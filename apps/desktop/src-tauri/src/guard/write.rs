//! 写操作的输入校验与执行：远端是 POSIX，所以只拒绝 POSIX 层面真正危险或不可表示的
//! 名称（分隔符、NUL 与控制字符、`.`/`..`、超长）；Windows 保留名不需要在这里拦。
//!
//! 本模块是 v1 唯一会改变远端文件系统的地方，且只做一件事：创建用户指定的一个文件夹，
//! 或回退掉本应用创建的那个文件夹。每次尝试都先落审计，再返回结果。

use std::time::Instant;

use serde::Serialize;

use crate::db::{AuditRow, Database, NewAudit};
use crate::ssh::error::SshError;
use crate::ssh::sftp::client;
use crate::ssh::state::AppState;

/// 单个目录名的字节上限（POSIX `NAME_MAX` 的常见取值）。
pub const MAX_NAME_BYTES: usize = 255;
/// 完整远端路径的字节上限，与 `ssh::sftp::path::MAX_REMOTE_PATH` 保持一致。
pub const MAX_PATH_BYTES: usize = 4096;

pub fn validate_folder_name(name: &str) -> Result<String, SshError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(SshError::preview_failed("文件夹名称不能为空"));
    }
    if trimmed.len() > MAX_NAME_BYTES {
        return Err(SshError::preview_failed("文件夹名称不能超过 255 字节"));
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains('\0') {
        return Err(SshError::preview_failed("文件夹名称不能包含 / \\ 或 NUL"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(SshError::preview_failed("文件夹名称不能包含控制字符"));
    }
    if trimmed == "." || trimmed == ".." {
        return Err(SshError::preview_failed("不能使用 . 或 .. 作为名称"));
    }
    Ok(trimmed.to_string())
}

/// 把已校验的名称拼到目录后面，并再次确认结果没有逃出目标目录。
pub fn join_target(dir: &str, name: &str) -> Result<String, SshError> {
    let name = validate_folder_name(name)?;
    let base = dir.trim_end_matches('/');
    if !base.is_empty() && !base.starts_with('/') {
        return Err(SshError::preview_failed("目标目录必须是绝对路径"));
    }
    if base.chars().any(char::is_control) {
        return Err(SshError::preview_failed("目标路径包含非法字符"));
    }
    if base.split('/').any(|segment| segment == "..") {
        return Err(SshError::preview_failed("目标路径不能包含 .."));
    }
    let path = if base.is_empty() {
        format!("/{name}")
    } else {
        format!("{base}/{name}")
    };
    if path.len() > MAX_PATH_BYTES {
        return Err(SshError::preview_failed("完整路径超过 4096 字节"));
    }
    // 兜底：名称与目录都已排除 `..` 和分隔符，结果只可能是绝对路径。
    if !path.starts_with('/') {
        return Err(SshError::preview_failed("目标路径非法"));
    }
    Ok(path)
}

pub const MKDIR_OP: &str = "fs.mkdir";
pub const MKDIR_ROLLBACK_OP: &str = "fs.mkdir.rollback";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WritePreview {
    pub preview_id: String,
    pub kind: String,
    pub path: String,
    pub operation: String,
    pub target: String,
    pub impact: String,
    pub rollback: String,
    pub irreversible: bool,
    pub expires_at_ms: i64,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteResult {
    pub audit_id: i64,
    pub path: String,
}

#[allow(clippy::too_many_arguments)]
pub fn build_preview(
    preview_id: String,
    kind: &str,
    path: &str,
    operation: &str,
    target: &str,
    impact: String,
    rollback: &str,
    irreversible: bool,
    details: serde_json::Value,
    expires_at_ms: i64,
) -> WritePreview {
    WritePreview {
        preview_id,
        kind: kind.to_string(),
        path: path.to_string(),
        operation: operation.to_string(),
        target: target.to_string(),
        impact,
        rollback: rollback.to_string(),
        irreversible,
        expires_at_ms,
        details,
    }
}

pub fn build_mkdir_preview(preview_id: String, path: &str, expires_at_ms: i64) -> WritePreview {
    build_preview(
        preview_id,
        "mkdir",
        path,
        "新建文件夹",
        path,
        format!("将在 {path} 创建文件夹"),
        "执行后可在审计中回退（仅删除空目录）",
        false,
        serde_json::json!({ "path": path }),
        expires_at_ms,
    )
}

pub fn audit_params(path: &str) -> String {
    serde_json::json!({ "path": path }).to_string()
}

pub fn audit_path(params: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(params)
        .ok()?
        .get("path")?
        .as_str()
        .map(str::to_string)
}

pub const RENAME_OP: &str = "fs.rename";
pub const RENAME_ROLLBACK_OP: &str = "fs.rename.rollback";

/// 只允许在同一目录内改名；父目录由后端从当前路径推导，前端不能借新名称改目录。
pub fn rename_targets(path: &str, new_name: &str) -> Result<(String, String), SshError> {
    absolute_target(path)?;
    let parent = crate::ssh::sftp::path::parent_of(path)
        .ok_or_else(|| SshError::preview_failed("无法定位父目录"))?;
    let name = validate_folder_name(new_name)?;
    let to = join_target(&parent, &name)?;
    if to == path {
        return Err(SshError::preview_failed("新名称与当前名称相同"));
    }
    Ok((path.to_string(), to))
}

pub fn build_rename_preview(
    preview_id: String,
    from: &str,
    to: &str,
    expires_at_ms: i64,
) -> WritePreview {
    build_preview(
        preview_id,
        "rename",
        from,
        "重命名",
        &format!("{from} → {to}"),
        format!("将把 {from} 重命名为 {to}"),
        "执行后可在审计中回退（改回原名称）",
        false,
        serde_json::json!({ "from": from, "to": to }),
        expires_at_ms,
    )
}

pub fn audit_rename_params(from: &str, to: &str) -> String {
    serde_json::json!({ "from": from, "to": to }).to_string()
}

pub fn audit_rename_paths(params: &str) -> Option<(String, String)> {
    let value: serde_json::Value = serde_json::from_str(params).ok()?;
    let from = value.get("from")?.as_str()?.to_string();
    let to = value.get("to")?.as_str()?.to_string();
    Some((from, to))
}

pub const CHMOD_OP: &str = "fs.chmod";
pub const CHMOD_ROLLBACK_OP: &str = "fs.chmod.rollback";

/// 只接受三位八进制 `000`–`777`，或带前导 0 的四位写法（`0755`）。
/// 拒绝 setuid / setgid / sticky，避免把一次“改权限”变成提权入口。
pub fn parse_mode(input: &str) -> Result<u32, SshError> {
    let trimmed = input.trim();
    if trimmed.is_empty()
        || trimmed.len() > 4
        || !trimmed.chars().all(|c| ('0'..='7').contains(&c))
    {
        return Err(SshError::preview_failed("权限必须是 000–777 的八进制数"));
    }
    let value = u32::from_str_radix(trimmed, 8)
        .map_err(|_| SshError::preview_failed("权限必须是 000–777 的八进制数"))?;
    if value > 0o777 {
        return Err(SshError::preview_failed("权限不能包含特殊位"));
    }
    Ok(value)
}

pub fn build_chmod_preview(
    preview_id: String,
    path: &str,
    previous_mode: u32,
    mode: u32,
    expires_at_ms: i64,
) -> WritePreview {
    build_preview(
        preview_id,
        "chmod",
        path,
        "修改权限",
        path,
        format!(
            "将把 {path} 的权限从 {:03o} 改为 {mode:03o}",
            previous_mode & 0o7777
        ),
        "执行后可在审计中恢复原权限",
        false,
        serde_json::json!({
            "path": path,
            "mode": mode,
            "previousMode": previous_mode,
        }),
        expires_at_ms,
    )
}

pub fn rollback_block_reason(row: &AuditRow, has_rollback: bool) -> Option<&'static str> {
    let supported = matches!(row.op.as_str(), MKDIR_OP | RENAME_OP | CHMOD_OP | UPLOAD_OP)
        || matches!(
            row.op.as_str(),
            "docker.container.start"
                | "docker.container.stop"
                | "docker.compose.start"
                | "docker.compose.stop"
                | "docker.network.create"
                | "docker.volume.create"
        );
    if !supported {
        return Some("该操作不支持回退");
    }
    if row.result != "success" {
        return Some("只有成功执行的操作支持回退");
    }
    if row.rollback_of.is_some() {
        return Some("回退记录不能再回退");
    }
    if has_rollback {
        return Some("该操作已回退");
    }
    None
}

/// A request that left the socket but lost the channel may still have run on
/// the server, so those errors must land in the audit as `unknown`.
pub fn outcome_for(error: &SshError) -> &'static str {
    if error.retryable || matches!(error.code, "TIMEOUT" | "SESSION_CLOSED" | "SESSION_NOT_FOUND")
    {
        "unknown"
    } else {
        "failed"
    }
}

/// 落到远端之前的最后一道校验。预览与审计都来自本进程，这里防的是数据被改写后仍然
/// 变成一个远端的 `mkdir`/`rmdir` 调用：只接受绝对、无 `..`、无空段的路径，并且
/// 永远不接受 `/` 本身，回退不会把根目录当成"本应用创建的文件夹"。
pub(crate) fn absolute_target(path: &str) -> Result<(), SshError> {
    if !path.starts_with('/') || path == "/" || path.ends_with('/') {
        return Err(SshError::invalid_input("目标路径必须是绝对路径"));
    }
    if path.len() > MAX_PATH_BYTES || path.chars().any(char::is_control) {
        return Err(SshError::invalid_input("目标路径包含非法字符"));
    }
    if path
        .split('/')
        .skip(1)
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(SshError::invalid_input("目标路径不能包含空段、. 或 .."));
    }
    Ok(())
}

fn failed(action: &str, error: &SshError) -> SshError {
    SshError {
        code: "WRITE_FAILED",
        message: format!("{action}失败：{}", error.message),
        hint: "请检查目标目录权限或磁盘状态后重试。".to_string(),
        stage: None,
        retryable: false,
    }
}

fn failed_unknown(action: &str, error: &SshError) -> SshError {
    SshError::write_failed(format!("{action}时连接中断：{}", error.message))
}

/// 审计行没落库时的统一提示：用户必须知道这次尝试在审计里看不到。
const AUDIT_LOST_HINT: &str = "操作已结束但审计记录写入失败，请在审计列表中确认状态。";

pub(crate) fn record(
    db: &Database,
    server_id: &str,
    server_name: &str,
    op: &str,
    params: &str,
    result: &str,
    started: Instant,
    permit_source: &str,
    rollback_of: Option<i64>,
) -> Result<i64, SshError> {
    db.audit_insert(&NewAudit {
        at_ms: crate::alerts::now_ms(),
        server_id: server_id.to_string(),
        server_name: server_name.to_string(),
        op: op.to_string(),
        params: params.to_string(),
        result: result.to_string(),
        duration_ms: started.elapsed().as_millis() as i64,
        permit_source: permit_source.to_string(),
        rollback_of,
    })
    .map_err(|error| SshError {
        code: error.code,
        message: error.message,
        hint: error.hint,
        stage: None,
        retryable: false,
    })
}

/// 操作本身已经以失败/未知收尾，但审计行没有写进去：保留操作自己的错误码与
/// 消息，把审计失败追加进去，并把底层 DB 错误打到日志里。
pub(crate) fn audit_write_failed(
    operation: SshError,
    audit: &SshError,
    may_have_run: bool,
) -> SshError {
    eprintln!(
        "[lightguard] audit insert failed for {}: {} ({})",
        operation.code, audit.code, audit.message
    );
    let message = operation.message.trim_end_matches('。');
    let hint = if may_have_run {
        format!("操作可能已执行，请在文件列表中刷新核对。{AUDIT_LOST_HINT}")
    } else {
        AUDIT_LOST_HINT.to_string()
    };
    SshError {
        message: format!("{message}；审计记录写入失败：{}", audit.message),
        hint,
        ..operation
    }
}

/// 远端变更已经生效，但审计行没有落库：既没有审计 id，也就没有可回退的记录。
/// 返回值必须同时说明"操作可能已完成"和"没有审计编号"，前端才不会给出回退入口。
pub(crate) fn audit_lost_after_success(action: &str, audit: &SshError) -> SshError {
    eprintln!(
        "[lightguard] audit insert failed after {action}: {} ({})",
        audit.code, audit.message
    );
    let mut error = SshError::write_failed(format!(
        "{action}，但审计记录写入失败：{}；本次操作可能已完成，且没有审计编号，无法在审计中回退。",
        audit.message
    ));
    error.hint = "操作可能已完成，但审计记录写入失败，没有审计编号；请在文件列表中刷新核对，避免重复执行。"
        .to_string();
    error
}

pub(crate) async fn execute_mkdir(
    state: &AppState,
    db: &Database,
    server_id: &str,
    path: &str,
    permit_source: &str,
) -> Result<WriteResult, SshError> {
    absolute_target(path)?;
    let started = Instant::now();
    let server_name = db
        .server_name(server_id)
        .unwrap_or_else(|_| server_id.to_string());
    let params = audit_params(path);
    let outcome = client::create_dir(state, server_id, path).await;
    match outcome {
        Ok(()) => {
            let audit_id = record(
                db,
                server_id,
                &server_name,
                MKDIR_OP,
                &params,
                "success",
                started,
                permit_source,
                None,
            )
            .map_err(|audit| audit_lost_after_success("文件夹已创建", &audit))?;
            Ok(WriteResult {
                audit_id,
                path: path.to_string(),
            })
        }
        Err(error) => {
            let result = outcome_for(&error);
            let operation_error = if result == "unknown" {
                failed_unknown("创建文件夹", &error)
            } else {
                failed("创建文件夹", &error)
            };
            match record(
                db,
                server_id,
                &server_name,
                MKDIR_OP,
                &params,
                result,
                started,
                permit_source,
                None,
            ) {
                Ok(_) => Err(operation_error),
                Err(audit) => Err(audit_write_failed(
                    operation_error,
                    &audit,
                    result == "unknown",
                )),
            }
        }
    }
}

pub(crate) async fn rollback_mkdir(
    state: &AppState,
    db: &Database,
    server_id: &str,
    audit_id: i64,
    path: &str,
    permit_source: &str,
) -> Result<WriteResult, SshError> {
    absolute_target(path)?;
    let started = Instant::now();
    let server_name = db
        .server_name(server_id)
        .unwrap_or_else(|_| server_id.to_string());
    let params = audit_params(path);
    let outcome = client::remove_dir(state, server_id, path).await;
    match outcome {
        Ok(()) => {
            let new_id = record(
                db,
                server_id,
                &server_name,
                MKDIR_ROLLBACK_OP,
                &params,
                "success",
                started,
                permit_source,
                Some(audit_id),
            )
            .map_err(|audit| audit_lost_after_success("文件夹已删除", &audit))?;
            Ok(WriteResult {
                audit_id: new_id,
                path: path.to_string(),
            })
        }
        Err(error) => {
            let result = outcome_for(&error);
            let operation_error = if result == "unknown" {
                failed_unknown("删除文件夹", &error)
            } else {
                failed("删除文件夹", &error)
            };
            match record(
                db,
                server_id,
                &server_name,
                MKDIR_ROLLBACK_OP,
                &params,
                result,
                started,
                permit_source,
                Some(audit_id),
            ) {
                Ok(_) => Err(operation_error),
                Err(audit) => Err(audit_write_failed(
                    operation_error,
                    &audit,
                    result == "unknown",
                )),
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute_rename_inner(
    state: &AppState,
    db: &Database,
    server_id: &str,
    from: &str,
    to: &str,
    permit_source: &str,
    op: &str,
    rollback_of: Option<i64>,
) -> Result<WriteResult, SshError> {
    absolute_target(from)?;
    absolute_target(to)?;
    let started = Instant::now();
    let server_name = db
        .server_name(server_id)
        .unwrap_or_else(|_| server_id.to_string());
    let params = audit_rename_params(from, to);
    let outcome = client::rename(state, server_id, from, to).await;
    match outcome {
        Ok(()) => {
            let audit_id = record(
                db,
                server_id,
                &server_name,
                op,
                &params,
                "success",
                started,
                permit_source,
                rollback_of,
            )
            .map_err(|audit| audit_lost_after_success("重命名已完成", &audit))?;
            Ok(WriteResult {
                audit_id,
                path: to.to_string(),
            })
        }
        Err(error) => {
            let result = outcome_for(&error);
            let operation_error = if result == "unknown" {
                failed_unknown("重命名", &error)
            } else {
                failed("重命名", &error)
            };
            match record(
                db,
                server_id,
                &server_name,
                op,
                &params,
                result,
                started,
                permit_source,
                rollback_of,
            ) {
                Ok(_) => Err(operation_error),
                Err(audit) => Err(audit_write_failed(
                    operation_error,
                    &audit,
                    result == "unknown",
                )),
            }
        }
    }
}

pub(crate) async fn execute_rename(
    state: &AppState,
    db: &Database,
    server_id: &str,
    from: &str,
    to: &str,
    permit_source: &str,
) -> Result<WriteResult, SshError> {
    execute_rename_inner(
        state, db, server_id, from, to, permit_source, RENAME_OP, None,
    )
    .await
}

pub(crate) async fn rollback_rename(
    state: &AppState,
    db: &Database,
    server_id: &str,
    audit_id: i64,
    from: &str,
    to: &str,
    permit_source: &str,
) -> Result<WriteResult, SshError> {
    if client::try_exists(state, server_id, from).await? {
        return Err(SshError::invalid_input("原路径已被占用，无法回退"));
    }
    execute_rename_inner(
        state,
        db,
        server_id,
        to,
        from,
        permit_source,
        RENAME_ROLLBACK_OP,
        Some(audit_id),
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_chmod(
    state: &AppState,
    db: &Database,
    server_id: &str,
    path: &str,
    mode: u32,
    previous_mode: u32,
    permit_source: &str,
    op: &str,
    rollback_of: Option<i64>,
) -> Result<WriteResult, SshError> {
    absolute_target(path)?;
    let started = Instant::now();
    let server_name = db
        .server_name(server_id)
        .unwrap_or_else(|_| server_id.to_string());
    let params = serde_json::json!({
        "path": path,
        "mode": mode,
        "previousMode": previous_mode,
    })
    .to_string();
    let outcome = client::set_permissions(state, server_id, path, mode).await;
    match outcome {
        Ok(()) => {
            let audit_id = record(
                db,
                server_id,
                &server_name,
                op,
                &params,
                "success",
                started,
                permit_source,
                rollback_of,
            )
            .map_err(|audit| audit_lost_after_success("权限已修改", &audit))?;
            Ok(WriteResult {
                audit_id,
                path: path.to_string(),
            })
        }
        Err(error) => {
            let result = outcome_for(&error);
            let operation_error = if result == "unknown" {
                failed_unknown("修改权限", &error)
            } else {
                failed("修改权限", &error)
            };
            match record(
                db,
                server_id,
                &server_name,
                op,
                &params,
                result,
                started,
                permit_source,
                rollback_of,
            ) {
                Ok(_) => Err(operation_error),
                Err(audit) => Err(audit_write_failed(
                    operation_error,
                    &audit,
                    result == "unknown",
                )),
            }
        }
    }
}

pub(crate) async fn rollback_chmod(
    state: &AppState,
    db: &Database,
    server_id: &str,
    audit_id: i64,
    path: &str,
    previous_mode: u32,
    permit_source: &str,
) -> Result<WriteResult, SshError> {
    let current = client::stat_entry(state, server_id, path).await?;
    execute_chmod(
        state,
        db,
        server_id,
        path,
        previous_mode,
        current.mode & 0o7777,
        permit_source,
        CHMOD_ROLLBACK_OP,
        Some(audit_id),
    )
    .await
}

pub const DELETE_OP: &str = "fs.delete";

pub fn build_delete_preview(
    preview_id: String,
    path: &str,
    kind: &str,
    stats: crate::ssh::sftp::delete::DeleteStats,
    expires_at_ms: i64,
) -> WritePreview {
    let impact = if kind == "dir" {
        let prefix = if stats.truncated { "至少 " } else { "" };
        format!(
            "将递归删除 {path}，{prefix}{} 项、{} 字节；删除后无法恢复",
            stats.entries, stats.bytes
        )
    } else {
        format!("将删除文件 {path}；删除后无法恢复")
    };
    build_preview(
        preview_id,
        "delete",
        path,
        "删除",
        path,
        impact,
        "删除不可回退",
        true,
        serde_json::json!({
            "path": path,
            "kind": kind,
            "entries": stats.entries,
            "bytes": stats.bytes,
            "truncated": stats.truncated,
        }),
        expires_at_ms,
    )
}

pub(crate) async fn execute_delete(
    state: &AppState,
    db: &Database,
    server_id: &str,
    path: &str,
    kind: &str,
    permit_source: &str,
) -> Result<WriteResult, SshError> {
    absolute_target(path)?;
    let started = Instant::now();
    let server_name = db
        .server_name(server_id)
        .unwrap_or_else(|_| server_id.to_string());
    let params = serde_json::json!({ "path": path, "kind": kind }).to_string();
    let outcome = if kind == "dir" {
        let path_owned = path.to_string();
        let client = client::acquire(state, server_id).await;
        match client {
            Ok(client) => crate::ssh::sftp::delete::remove_tree(
                &client,
                &path_owned,
                crate::ssh::sftp::delete::MAX_DELETE_DEPTH,
            )
            .await
            .map(|_| ()),
            Err(error) => Err(error),
        }
    } else {
        client::remove_file(state, server_id, path).await
    };
    match outcome {
        Ok(()) => {
            let audit_id = record(
                db,
                server_id,
                &server_name,
                DELETE_OP,
                &params,
                "success",
                started,
                permit_source,
                None,
            )
            .map_err(|audit| audit_lost_after_success("删除已完成", &audit))?;
            Ok(WriteResult {
                audit_id,
                path: path.to_string(),
            })
        }
        Err(error) => {
            let result = outcome_for(&error);
            let operation_error = if result == "unknown" {
                failed_unknown("删除", &error)
            } else {
                failed("删除", &error)
            };
            match record(
                db,
                server_id,
                &server_name,
                DELETE_OP,
                &params,
                result,
                started,
                permit_source,
                None,
            ) {
                Ok(_) => Err(operation_error),
                Err(audit) => Err(audit_write_failed(
                    operation_error,
                    &audit,
                    result == "unknown",
                )),
            }
        }
    }
}

pub const UPLOAD_OP: &str = "fs.upload";
pub const UPLOAD_ROLLBACK_OP: &str = "fs.upload.rollback";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadAction {
    Create,
    Overwrite,
    Skip,
    Rename(String),
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadPlanItem {
    pub name: String,
    pub remote_path: String,
    pub size: u64,
    pub action: String,
    pub renamed_to: Option<String>,
    pub rollbackable: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteProgress {
    pub file_index: usize,
    pub file_name: String,
    pub transferred: u64,
    pub total: Option<u64>,
    pub done: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadJob {
    pub local_path: String,
    pub remote_path: String,
    pub overwrite: bool,
    pub size: u64,
}

/// 冲突策略：skip 默认；overwrite 原文件不可恢复；rename 自动加 ` (n)`。
pub fn plan_upload(name: &str, exists: bool, policy: &str) -> UploadAction {
    if !exists {
        return UploadAction::Create;
    }
    match policy {
        "overwrite" => UploadAction::Overwrite,
        "rename" => UploadAction::Rename(name.to_string()),
        _ => UploadAction::Skip,
    }
}

/// `name (1).ext`；没有扩展名时 `name (1)`。
pub fn renamed_name(name: &str, index: usize) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => {
            format!("{stem} ({index}).{ext}")
        }
        _ => format!("{name} ({index})"),
    }
}

pub fn build_upload_preview(
    preview_id: String,
    dir: &str,
    items: &[UploadPlanItem],
    conflict_policy: &str,
    expires_at_ms: i64,
) -> WritePreview {
    let total: u64 = items.iter().map(|item| item.size).sum();
    let skipped = items.iter().filter(|item| item.action == "skip").count();
    let overwritten = items
        .iter()
        .filter(|item| item.action == "overwrite")
        .count();
    let executable = items.len() - skipped;
    build_preview(
        preview_id,
        "upload",
        dir,
        "上传文件",
        dir,
        format!(
            "将上传 {executable} 个文件（共 {total} 字节）到 {dir}；跳过 {skipped} 个，覆盖 {overwritten} 个"
        ),
        "create / rename 项可在审计中回退（删除新文件）；overwrite 项不可回退",
        false,
        serde_json::json!({
            "dir": dir,
            "conflictPolicy": conflict_policy,
            "files": items,
        }),
        expires_at_ms,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_upload(
    state: &AppState,
    db: &Database,
    server_id: &str,
    transfer_id: &str,
    jobs: &[UploadJob],
    permit_source: &str,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    mut progress: impl FnMut(WriteProgress),
) -> Result<WriteResult, SshError> {
    let client = client::acquire(state, server_id).await?;
    let server_name = db
        .server_name(server_id)
        .unwrap_or_else(|_| server_id.to_string());
    let mut succeeded = 0u32;
    let mut failed = 0u32;
    let mut last_audit = 0i64;

    for (index, job) in jobs.iter().enumerate() {
        if cancel.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        let started = Instant::now();
        let part = crate::ssh::sftp::transfer::upload_part_path(&job.remote_path, transfer_id);
        state.register_part(server_id, &part).await;
        let local = std::path::Path::new(&job.local_path);
        let outcome = crate::ssh::sftp::transfer::upload_file(
            &client,
            local,
            &part,
            transfer_id,
            &cancel,
            |transferred, total| {
                progress(WriteProgress {
                    file_index: index,
                    file_name: job.remote_path.clone(),
                    transferred,
                    total,
                    done: succeeded as usize,
                    failed: failed as usize,
                })
            },
        )
        .await;

        let finalized = match outcome {
            Ok(crate::ssh::sftp::transfer::TransferOutcome::Canceled { .. }) => {
                failed += 1;
                state.forget_part(server_id, &part).await;
                let _ = record(
                    db,
                    server_id,
                    &server_name,
                    UPLOAD_OP,
                    &serde_json::json!({
                        "path": job.remote_path,
                        "size": job.size,
                        "overwrite": job.overwrite,
                        "canceled": true,
                    })
                    .to_string(),
                    "failed",
                    started,
                    permit_source,
                    None,
                );
                break;
            }
            Ok(crate::ssh::sftp::transfer::TransferOutcome::Saved { .. }) => {
                if job.overwrite {
                    let _ = client.remove_file(&job.remote_path).await;
                }
                let renamed = client
                    .rename(&part, &job.remote_path)
                    .await
                    .map_err(|error| {
                        SshError::sftp_failed(format!("重命名上传文件失败：{error}"))
                    });
                if renamed.is_ok() {
                    state.forget_part(server_id, &part).await;
                }
                renamed
            }
            Err(error) => {
                // 连接还在时立即补删；连接已断则保留登记，等重连后清理。
                if client.remove_file(&part).await.is_ok() {
                    state.forget_part(server_id, &part).await;
                }
                Err(error)
            }
        };

        match finalized {
            Ok(()) => {
                last_audit = record(
                    db,
                    server_id,
                    &server_name,
                    UPLOAD_OP,
                    &serde_json::json!({
                        "path": job.remote_path,
                        "size": job.size,
                        "overwrite": job.overwrite,
                    })
                    .to_string(),
                    "success",
                    started,
                    permit_source,
                    None,
                )?;
                succeeded += 1;
            }
            Err(error) => {
                failed += 1;
                let result_kind = outcome_for(&error);
                let _ = record(
                    db,
                    server_id,
                    &server_name,
                    UPLOAD_OP,
                    &serde_json::json!({
                        "path": job.remote_path,
                        "size": job.size,
                        "overwrite": job.overwrite,
                    })
                    .to_string(),
                    result_kind,
                    started,
                    permit_source,
                    None,
                );
                if error.code == "SESSION_CLOSED" || error.code == "SESSION_NOT_FOUND" {
                    return Err(failed_unknown("上传文件", &error));
                }
            }
        }
    }

    if failed > 0 {
        return Err(SshError::write_failed(format!(
            "上传完成：成功 {succeeded} 个，失败 {failed} 个"
        )));
    }
    Ok(WriteResult {
        audit_id: last_audit,
        path: jobs
            .first()
            .map(|job| job.remote_path.clone())
            .unwrap_or_default(),
    })
}

pub(crate) async fn rollback_upload(
    state: &AppState,
    db: &Database,
    server_id: &str,
    audit_id: i64,
    path: &str,
    overwrite: bool,
    permit_source: &str,
) -> Result<WriteResult, SshError> {
    if overwrite {
        return Err(SshError::invalid_input("覆盖上传不支持回退"));
    }
    absolute_target(path)?;
    let started = Instant::now();
    let server_name = db
        .server_name(server_id)
        .unwrap_or_else(|_| server_id.to_string());
    let params = serde_json::json!({ "path": path }).to_string();
    match client::remove_file(state, server_id, path).await {
        Ok(()) => {
            let new_id = record(
                db,
                server_id,
                &server_name,
                UPLOAD_ROLLBACK_OP,
                &params,
                "success",
                started,
                permit_source,
                Some(audit_id),
            )
            .map_err(|audit| audit_lost_after_success("上传文件已删除", &audit))?;
            Ok(WriteResult {
                audit_id: new_id,
                path: path.to_string(),
            })
        }
        Err(error) => {
            let result = outcome_for(&error);
            let operation_error = if result == "unknown" {
                failed_unknown("删除上传文件", &error)
            } else {
                failed("删除上传文件", &error)
            };
            let _ = record(
                db,
                server_id,
                &server_name,
                UPLOAD_ROLLBACK_OP,
                &params,
                result,
                started,
                permit_source,
                Some(audit_id),
            );
            Err(operation_error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_names_and_trims() {
        assert_eq!(validate_folder_name("  reports  ").unwrap(), "reports");
        assert!(validate_folder_name("a b-c_d.1").is_ok());
        assert!(validate_folder_name("报告").is_ok());
    }

    #[test]
    fn rejects_illegal_names() {
        for name in ["", "   ", ".", "..", "a/b", "a\\b", "a\0b"] {
            assert_eq!(
                validate_folder_name(name).unwrap_err().code,
                "PREVIEW_FAILED",
                "name {name:?}"
            );
        }
        let long = "x".repeat(256);
        assert_eq!(
            validate_folder_name(&long).unwrap_err().code,
            "PREVIEW_FAILED"
        );
        let max = "x".repeat(255);
        assert!(validate_folder_name(&max).is_ok());
    }

    #[test]
    fn rejects_control_characters() {
        for name in ["a\tb", "a\nb", "a\u{7f}b", "a\u{1}b"] {
            assert_eq!(
                validate_folder_name(name).unwrap_err().code,
                "PREVIEW_FAILED",
                "name {name:?}"
            );
        }
    }

    #[test]
    fn joins_without_double_slash() {
        assert_eq!(join_target("/", "x").unwrap(), "/x");
        assert_eq!(join_target("/home/u/", "x").unwrap(), "/home/u/x");
        assert_eq!(join_target("/home/u", "x").unwrap(), "/home/u/x");
        let deep = format!("/{}", "d".repeat(4096));
        assert_eq!(join_target(&deep, "x").unwrap_err().code, "PREVIEW_FAILED");
    }

    #[test]
    fn trims_the_name_before_joining() {
        assert_eq!(join_target("/data", "  logs  ").unwrap(), "/data/logs");
    }

    #[test]
    fn rejects_traversal_in_dir_or_name() {
        for dir in ["/data/../etc", "/..", "..", "relative/path", "/data/\0x"] {
            assert_eq!(
                join_target(dir, "x").unwrap_err().code,
                "PREVIEW_FAILED",
                "dir {dir:?}"
            );
        }
        for name in ["..", ".", "../x", "a/b"] {
            assert_eq!(
                join_target("/data", name).unwrap_err().code,
                "PREVIEW_FAILED",
                "name {name:?}"
            );
        }
        // `..hidden` 只是普通文件名，不是上跳。
        assert_eq!(join_target("/data", "..hidden").unwrap(), "/data/..hidden");
    }

    #[test]
    fn empty_dir_falls_back_to_root() {
        assert_eq!(join_target("", "x").unwrap(), "/x");
    }

    #[test]
    fn preview_carries_kind_and_details() {
        let preview = build_mkdir_preview("abc".to_string(), "/data/reports", 1_700_000_060_000);
        assert_eq!(preview.preview_id, "abc");
        assert_eq!(preview.kind, "mkdir");
        assert_eq!(preview.operation, "新建文件夹");
        assert_eq!(preview.target, "/data/reports");
        assert_eq!(preview.impact, "将在 /data/reports 创建文件夹");
        assert!(preview.rollback.contains("审计"));
        assert!(!preview.irreversible);
        assert_eq!(preview.details["path"], "/data/reports");
        assert_eq!(preview.expires_at_ms, 1_700_000_060_000);
    }

    #[test]
    fn rename_targets_validate_name_and_same_path() {
        assert_eq!(
            rename_targets("/data/old", "new").unwrap(),
            ("/data/old".to_string(), "/data/new".to_string())
        );
        assert_eq!(
            rename_targets("/data/old", "old").unwrap_err().code,
            "PREVIEW_FAILED"
        );
        assert_eq!(
            rename_targets("relative", "new").unwrap_err().code,
            "INVALID_INPUT"
        );
        assert_eq!(
            rename_targets("/data/old", "a/b").unwrap_err().code,
            "PREVIEW_FAILED"
        );
    }

    #[test]
    fn rename_preview_is_reversible() {
        let preview = build_rename_preview("p".into(), "/data/old", "/data/new", 7);
        assert_eq!(preview.kind, "rename");
        assert_eq!(preview.target, "/data/old → /data/new");
        assert!(!preview.irreversible);
        assert_eq!(preview.details["from"], "/data/old");
        assert_eq!(preview.details["to"], "/data/new");
        assert!(preview.rollback.contains("回退"));
    }

    #[test]
    fn parses_only_plain_octal_modes() {
        assert_eq!(parse_mode("755").unwrap(), 0o755);
        assert_eq!(parse_mode("0755").unwrap(), 0o755);
        assert_eq!(parse_mode("600").unwrap(), 0o600);
        for bad in ["", "888", "1000", "u+rwx", "-1"] {
            assert_eq!(parse_mode(bad).unwrap_err().code, "PREVIEW_FAILED", "{bad:?}");
        }
    }

    #[test]
    fn chmod_preview_shows_both_modes_and_is_reversible() {
        let preview = build_chmod_preview("p".into(), "/data/x", 0o644, 0o755, 7);
        assert_eq!(preview.kind, "chmod");
        assert!(preview.impact.contains("644"));
        assert!(preview.impact.contains("755"));
        assert!(!preview.irreversible);
        assert_eq!(preview.details["previousMode"], 0o644);
        assert_eq!(preview.details["mode"], 0o755);
    }

    #[test]
    fn delete_preview_is_irreversible() {
        let stats = crate::ssh::sftp::delete::DeleteStats {
            entries: 12,
            bytes: 34,
            truncated: false,
        };
        let preview = build_delete_preview("p".into(), "/data/tmp", "dir", stats, 7);
        assert_eq!(preview.kind, "delete");
        assert!(preview.irreversible);
        assert_eq!(preview.details["entries"], 12);
        assert!(preview.impact.contains("12"));
        assert!(preview.rollback.contains("不可回退"));
    }

    #[test]
    fn upload_conflict_policy_and_renamed_names() {
        assert_eq!(plan_upload("a.txt", false, "skip"), UploadAction::Create);
        assert_eq!(plan_upload("a.txt", true, "skip"), UploadAction::Skip);
        assert_eq!(
            plan_upload("a.txt", true, "overwrite"),
            UploadAction::Overwrite
        );
        assert_eq!(
            plan_upload("a.txt", true, "rename"),
            UploadAction::Rename("a.txt".to_string())
        );
        assert_eq!(renamed_name("a.txt", 2), "a (2).txt");
        assert_eq!(renamed_name("README", 1), "README (1)");
    }

    #[test]
    fn audit_params_round_trip() {
        let params = audit_params("/data/a b");
        assert_eq!(audit_path(&params).as_deref(), Some("/data/a b"));
        assert_eq!(audit_path("not-json"), None);
        assert_eq!(audit_path("{}"), None);
    }

    #[test]
    fn rollback_rejects_wrong_op_result_or_duplicate() {
        let row = AuditRow {
            id: 7,
            at_ms: 1,
            server_id: "srv".to_string(),
            server_name: "web-01".to_string(),
            op: MKDIR_OP.to_string(),
            params: audit_params("/data/x"),
            result: "success".to_string(),
            duration_ms: 5,
            permit_source: "unlock:15m".to_string(),
            rollback_of: None,
        };
        assert_eq!(rollback_block_reason(&row, false), None);
        assert!(rollback_block_reason(&row, true).is_some());
        assert!(rollback_block_reason(&AuditRow { result: "failed".to_string(), ..row.clone() }, false).is_some());
        assert!(rollback_block_reason(
            &AuditRow { op: MKDIR_ROLLBACK_OP.to_string(), ..row.clone() },
            false
        )
        .is_some());
        assert!(rollback_block_reason(&AuditRow { rollback_of: Some(1), ..row }, false).is_some());
    }

    #[test]
    fn session_and_timeout_errors_are_unknown() {
        assert_eq!(outcome_for(&SshError::permission_denied("/x")), "failed");
        assert_eq!(outcome_for(&SshError::sftp_failed("/x")), "failed");
        assert_eq!(outcome_for(&SshError::session_closed()), "unknown");
        assert_eq!(outcome_for(&SshError::session_not_found()), "unknown");
        assert_eq!(outcome_for(&SshError::timeout("慢")), "unknown");
        assert_eq!(outcome_for(&SshError::sftp_channel("断了")), "unknown");
    }

    #[test]
    fn mutations_only_accept_a_plain_absolute_target() {
        for path in ["/data/reports", "/x", "/data/a b"] {
            assert!(absolute_target(path).is_ok(), "path {path:?}");
        }
        for path in [
            "",
            "/",
            "relative",
            "/data/",
            "/data//x",
            "/data/../etc",
            "/data/./x",
            "/data/\0x",
        ] {
            assert_eq!(
                absolute_target(path).unwrap_err().code,
                "INVALID_INPUT",
                "path {path:?}"
            );
        }
        let long = format!("/{}", "d".repeat(MAX_PATH_BYTES));
        assert_eq!(
            absolute_target(&long).unwrap_err().code,
            "INVALID_INPUT"
        );
    }

    #[test]
    fn preview_and_result_serialize_camel_case() {
        let preview =
            serde_json::to_value(build_mkdir_preview("abc".to_string(), "/data", 7)).unwrap();
        assert_eq!(preview["previewId"], "abc");
        assert_eq!(preview["kind"], "mkdir");
        assert_eq!(preview["details"]["path"], "/data");
        assert_eq!(preview["expiresAtMs"], 7);
        assert_eq!(preview["irreversible"], false);
        let result = serde_json::to_value(WriteResult {
            audit_id: 9,
            path: "/data".to_string(),
        })
        .unwrap();
        assert_eq!(result, serde_json::json!({ "auditId": 9, "path": "/data" }));
    }

    #[test]
    fn failed_writes_keep_the_wire_shape() {
        let denied = failed("创建文件夹", &SshError::permission_denied("/x"));
        assert_eq!(denied.code, "WRITE_FAILED");
        assert!(denied.message.contains("创建文件夹失败"));
        assert!(!denied.retryable);
        assert_eq!(denied.stage, None);
        // 断线由 write_failed 提供"可能已执行"的提示，弹窗据此不提供回退。
        let unknown = failed_unknown("创建文件夹", &SshError::session_closed());
        assert_eq!(unknown.code, "WRITE_FAILED");
        assert!(unknown.hint.contains("可能已执行"));
    }

    /// 没有会话就是"请求可能已经发出"的最坏情况：必须在返回 `WRITE_FAILED`
    /// 之前留下 `unknown` 审计行。测试不建任何连接，走的是真实的取用路径。
    #[tokio::test]
    async fn disconnects_record_unknown_audits() {
        let db = Database::in_memory();
        let state = crate::test_support::headless_state(std::path::PathBuf::from("."));

        let error = execute_mkdir(&state, &db, "srv", "/data/x", "unlock:15m")
            .await
            .unwrap_err();
        assert_eq!(error.code, "WRITE_FAILED");
        assert!(error.hint.contains("可能已执行"));

        let rows = db.audit_list(10, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].op, MKDIR_OP);
        assert_eq!(rows[0].result, "unknown");
        assert_eq!(rows[0].params, audit_params("/data/x"));
        assert_eq!(rows[0].permit_source, "unlock:15m");
        assert_eq!(rows[0].server_name, "srv", "缺名字时退回服务器标识");
        assert_eq!(rows[0].rollback_of, None);
    }

    #[tokio::test]
    async fn failed_rollbacks_keep_their_origin() {
        let db = Database::in_memory();
        let state = crate::test_support::headless_state(std::path::PathBuf::from("."));

        let error = rollback_mkdir(&state, &db, "srv", 7, "/data/x", "unlock:5m")
            .await
            .unwrap_err();
        assert_eq!(error.code, "WRITE_FAILED");

        let rows = db.audit_list(10, 0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].op, MKDIR_ROLLBACK_OP);
        assert_eq!(rows[0].result, "unknown");
        assert_eq!(rows[0].rollback_of, Some(7));
    }

    #[tokio::test]
    async fn relative_targets_never_reach_the_audit_or_the_server() {
        let db = Database::in_memory();
        let state = crate::test_support::headless_state(std::path::PathBuf::from("."));

        let error = execute_mkdir(&state, &db, "srv", "../etc", "unlock:5m")
            .await
            .unwrap_err();
        assert_eq!(error.code, "INVALID_INPUT");
        let error = rollback_mkdir(&state, &db, "srv", 7, "/", "unlock:5m")
            .await
            .unwrap_err();
        assert_eq!(error.code, "INVALID_INPUT");
        assert!(db.audit_list(10, 0).unwrap().is_empty());
    }

    /// 审计插入失败不能被 `let _ = record(...)` 吞掉：用户看到失败时，必须同时
    /// 被告知审计里没有这一行。`audit` 表被删掉后插入必然失败。
    #[tokio::test]
    async fn mkdir_audit_failures_are_surfaced() {
        let db = Database::in_memory();
        db.drop_audit_table_for_test();
        let state = crate::test_support::headless_state(std::path::PathBuf::from("."));

        let error = execute_mkdir(&state, &db, "srv", "/data/x", "unlock:15m")
            .await
            .unwrap_err();

        assert_eq!(error.code, "WRITE_FAILED", "审计失败不能改掉写类错误码");
        assert!(error.message.contains("审计记录写入失败"), "{}", error.message);
        assert!(
            error.hint.contains("操作已结束但审计记录写入失败"),
            "{}",
            error.hint
        );
        // 没有会话 → unknown：仍然要让前端知道"可能已执行"。
        assert!(error.hint.contains("可能已执行"), "{}", error.hint);
    }

    #[tokio::test]
    async fn rollback_audit_failures_are_surfaced() {
        let db = Database::in_memory();
        db.drop_audit_table_for_test();
        let state = crate::test_support::headless_state(std::path::PathBuf::from("."));

        let error = rollback_mkdir(&state, &db, "srv", 7, "/data/x", "unlock:5m")
            .await
            .unwrap_err();

        assert_eq!(error.code, "WRITE_FAILED");
        assert!(error.message.contains("审计记录写入失败"), "{}", error.message);
        assert!(
            error.hint.contains("操作已结束但审计记录写入失败"),
            "{}",
            error.hint
        );
    }

    /// 变更已生效但审计没落库时既没有审计 id 也没有回退入口，错误必须说清楚。
    #[test]
    fn lost_audit_after_success_names_the_missing_audit_id() {
        let audit = SshError::internal("database is locked");
        let error = audit_lost_after_success("文件夹已创建", &audit);
        assert_eq!(error.code, "WRITE_FAILED");
        assert!(error.message.contains("文件夹已创建"), "{}", error.message);
        assert!(error.message.contains("审计记录写入失败"), "{}", error.message);
        assert!(error.message.contains("没有审计编号"), "{}", error.message);
        assert!(error.message.contains("可能已完成"), "{}", error.message);
        assert!(error.hint.contains("没有审计编号"), "{}", error.hint);
        assert!(!error.retryable);
        assert_eq!(error.stage, None);
    }

    #[test]
    fn rollback_allows_container_start_and_stop() {
        let row = AuditRow {
            id: 7,
            at_ms: 1,
            server_id: "srv".to_string(),
            server_name: "nas".to_string(),
            op: "docker.container.start".to_string(),
            params: "{}".to_string(),
            result: "success".to_string(),
            duration_ms: 1,
            permit_source: "unlock:5m".to_string(),
            rollback_of: None,
        };
        assert_eq!(rollback_block_reason(&row, false), None);
        assert!(rollback_block_reason(
            &AuditRow { op: "docker.container.restart".to_string(), ..row.clone() },
            false,
        )
        .is_some());
        assert!(rollback_block_reason(&AuditRow { result: "failed".to_string(), ..row }, false).is_some());
    }

    /// 真实服务器夹具：连接 → 建目录 → 重复建（失败落审计）→ 回退。
    ///
    /// 只有 `LG_SSH_HOST` / `LG_SSH_USER` / `LG_SSH_PASSWORD` 全部就绪时才运行；
    /// 否则打印跳过提示后成功返回，保证 `cargo test --lib` 在无凭据环境下依然全绿。
    #[tokio::test]
    async fn real_guard_fixture_mkdir_duplicate_audit_and_rollback() {
        use crate::db::Database;
        use crate::ssh::credentials::CredentialStore;
        use crate::ssh::dto::{AuthKind, ConnectInput};
        use crate::ssh::session;

        fn env() -> Option<(String, String, String, u16)> {
            let host = std::env::var("LG_SSH_HOST").ok()?;
            let user = std::env::var("LG_SSH_USER").ok()?;
            let password = std::env::var("LG_SSH_PASSWORD").ok()?;
            if host.is_empty() || user.is_empty() || password.is_empty() {
                return None;
            }
            let port = std::env::var("LG_SSH_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(22);
            Some((host, user, password, port))
        }

        let Some((host, user, password, port)) = env() else {
            eprintln!("skipping: LG_SSH_* not set");
            return;
        };

        let dir = tempfile::tempdir().unwrap();
        let store = CredentialStore::with_master_key(dir.path().to_path_buf(), [7u8; 32]);
        let outcome = session::connect(
            ConnectInput {
                server_id: "guard-it".to_string(),
                host,
                port,
                user,
                auth: AuthKind::Password,
                secret: password,
                passphrase: None,
                remember: false,
                verify_host_key: false,
                elevate: Default::default(),
            },
            dir.path(),
            &store,
        )
        .await
        .expect("connect");

        let state = crate::test_support::headless_state(dir.path().to_path_buf());
        state.sessions.lock().await.insert(
            "guard-it".to_string(),
            std::sync::Arc::new(tokio::sync::Mutex::new(outcome.entry)),
        );

        let db = Database::in_memory();
        let guard = state.guard.clone();
        // 仅为健全性检查：真正的写入闸门在 guard/commands.rs，execute_mkdir 自身不校验许可。
        guard.unlock("guard-it", 5).expect("unlock");
        assert_eq!(guard.require_permit("guard-it").expect("permit"), 5);
        let _write = guard.begin_write("guard-it").expect("write lock");

        let base = crate::test_support::remote_fixture_dir()
            .unwrap_or_else(|| "/vol1/1000/lg-filetest".to_string());
        let name = format!("lg-guard-{}", crate::alerts::now_ms());
        let target = format!("{base}/{name}");

        let created = execute_mkdir(&state, &db, "guard-it", &target, "unlock:5m")
            .await
            .expect("mkdir");

        // 同名目录再建一次：远端返回失败，落 failed 审计，且不产生第二个目录。
        let duplicate = execute_mkdir(&state, &db, "guard-it", &target, "unlock:5m").await;

        // 回退先于任何断言执行：断言 panic 时远端目录才不会残留。
        let rolled = rollback_mkdir(&state, &db, "guard-it", created.audit_id, &target, "unlock:5m")
            .await;
        let has_rollback = db.audit_has_rollback(created.audit_id).expect("has rollback");
        let rows = db.audit_list(10, 0).expect("audit list");

        let error = duplicate.expect_err("duplicate mkdir must fail");
        assert_eq!(error.code, "WRITE_FAILED", "{error:?}");
        rolled.expect("rollback");
        assert!(has_rollback);

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].op, MKDIR_ROLLBACK_OP);
        assert_eq!(rows[0].result, "success");
        assert_eq!(rows[0].params, audit_params(&target));
        assert_eq!(rows[1].op, MKDIR_OP);
        assert_eq!(rows[1].result, "failed");
        assert_eq!(rows[1].params, audit_params(&target));
        assert_eq!(rows[2].id, created.audit_id);
        assert_eq!(rows[2].op, MKDIR_OP);
        assert_eq!(rows[2].result, "success");
        assert_eq!(rows[2].params, audit_params(&target));

        state.sessions.lock().await.remove("guard-it");
    }

    /// 真实服务器夹具：建目录 → 重命名 → 改权限并回退 → 递归删除。
    /// 无 `LG_SSH_*` 时打印跳过；所有远端操作先收集为 Result，再统一清理与断言。
    #[tokio::test]
    async fn real_guard_fixture_rename_chmod_delete() {
        use crate::db::Database;
        use crate::ssh::credentials::CredentialStore;
        use crate::ssh::dto::{AuthKind, ConnectInput};
        use crate::ssh::session;

        fn env() -> Option<(String, String, String, u16)> {
            let host = std::env::var("LG_SSH_HOST").ok()?;
            let user = std::env::var("LG_SSH_USER").ok()?;
            let password = std::env::var("LG_SSH_PASSWORD").ok()?;
            if host.is_empty() || user.is_empty() || password.is_empty() {
                return None;
            }
            let port = std::env::var("LG_SSH_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(22);
            Some((host, user, password, port))
        }

        let Some((host, user, password, port)) = env() else {
            eprintln!("skipping: LG_SSH_* not set");
            return;
        };

        let dir = tempfile::tempdir().unwrap();
        let store = CredentialStore::with_master_key(dir.path().to_path_buf(), [7u8; 32]);
        let outcome = session::connect(
            ConnectInput {
                server_id: "guard-it".to_string(),
                host,
                port,
                user,
                auth: AuthKind::Password,
                secret: password,
                passphrase: None,
                remember: false,
                verify_host_key: false,
                elevate: Default::default(),
            },
            dir.path(),
            &store,
        )
        .await
        .expect("connect");

        let state = crate::test_support::headless_state(dir.path().to_path_buf());
        state.sessions.lock().await.insert(
            "guard-it".to_string(),
            std::sync::Arc::new(tokio::sync::Mutex::new(outcome.entry)),
        );
        let db = Database::in_memory();
        let guard = state.guard.clone();
        guard.unlock("guard-it", 5).expect("unlock");
        let _write = guard.begin_write("guard-it").expect("write lock");

        let base = crate::test_support::remote_fixture_dir()
            .unwrap_or_else(|| "/vol1/1000/lg-filetest".to_string());
        let stamp = crate::alerts::now_ms();
        let root = format!("{base}/lg-write-{stamp}");
        let nested = format!("{root}/nested");
        let new_name = format!("lg-write-{stamp}-renamed");
        let renamed = format!("{base}/{new_name}");

        let facts: Result<(u32, u32, u32, i64), SshError> = async {
            execute_mkdir(&state, &db, "guard-it", &root, "unlock:5m").await?;
            execute_mkdir(&state, &db, "guard-it", &nested, "unlock:5m").await?;

            let (from, to) = rename_targets(&root, &new_name)?;
            execute_rename(&state, &db, "guard-it", &from, &to, "unlock:5m").await?;
            if crate::ssh::sftp::list::stat(&state, "guard-it", &root)
                .await
                .is_ok()
            {
                return Err(SshError::internal("重命名后旧路径仍存在"));
            }

            let entry = crate::ssh::sftp::list::stat(&state, "guard-it", &renamed).await?;
            let previous = entry.mode & 0o777;
            let chmod = execute_chmod(
                &state,
                &db,
                "guard-it",
                &renamed,
                0o700,
                previous,
                "unlock:5m",
                CHMOD_OP,
                None,
            )
            .await?;
            let after = crate::ssh::sftp::list::stat(&state, "guard-it", &renamed).await?;
            rollback_chmod(
                &state,
                &db,
                "guard-it",
                chmod.audit_id,
                &renamed,
                previous,
                "unlock:5m",
            )
            .await?;
            let restored = crate::ssh::sftp::list::stat(&state, "guard-it", &renamed).await?;
            Ok((
                after.mode & 0o777,
                restored.mode & 0o777,
                previous,
                chmod.audit_id,
            ))
        }
        .await;

        let _ = execute_delete(&state, &db, "guard-it", &renamed, "dir", "unlock:5m").await;
        let _ = execute_delete(&state, &db, "guard-it", &root, "dir", "unlock:5m").await;

        let (mode_after, mode_restored, previous, chmod_audit) = facts.expect("fixture");
        assert_eq!(mode_after, 0o700, "chmod 目标权限");
        assert_eq!(mode_restored, previous, "回退后恢复原权限");

        let rows = db.audit_list(30, 0).expect("audit list");
        assert!(rows
            .iter()
            .any(|row| row.op == RENAME_OP && row.result == "success"));
        assert!(rows
            .iter()
            .any(|row| row.op == CHMOD_OP && row.result == "success"));
        assert!(rows.iter().any(|row| {
            row.op == CHMOD_ROLLBACK_OP
                && row.result == "success"
                && row.rollback_of == Some(chmod_audit)
        }));
        assert!(rows
            .iter()
            .any(|row| row.op == DELETE_OP && row.result == "success"));

        state.sessions.lock().await.remove("guard-it");
    }

    /// 真实服务器上传夹具：本地文件 → 远端 .lgpart → 最终名 → 审计 → 回退删除。
    #[tokio::test]
    async fn real_guard_fixture_upload_and_rollback() {
        use crate::db::Database;
        use crate::ssh::credentials::CredentialStore;
        use crate::ssh::dto::{AuthKind, ConnectInput};
        use crate::ssh::session;

        fn env() -> Option<(String, String, String, u16)> {
            let host = std::env::var("LG_SSH_HOST").ok()?;
            let user = std::env::var("LG_SSH_USER").ok()?;
            let password = std::env::var("LG_SSH_PASSWORD").ok()?;
            if host.is_empty() || user.is_empty() || password.is_empty() {
                return None;
            }
            let port = std::env::var("LG_SSH_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(22);
            Some((host, user, password, port))
        }

        let Some((host, user, password, port)) = env() else {
            eprintln!("skipping: LG_SSH_* not set");
            return;
        };

        let dir = tempfile::tempdir().unwrap();
        let local = dir.path().join("upload-fixture.txt");
        let content = b"lightguard upload fixture\n";
        std::fs::write(&local, content).unwrap();

        let store = CredentialStore::with_master_key(dir.path().to_path_buf(), [7u8; 32]);
        let outcome = session::connect(
            ConnectInput {
                server_id: "guard-upload".to_string(),
                host,
                port,
                user,
                auth: AuthKind::Password,
                secret: password,
                passphrase: None,
                remember: false,
                verify_host_key: false,
                elevate: Default::default(),
            },
            dir.path(),
            &store,
        )
        .await
        .expect("connect");

        let state = crate::test_support::headless_state(dir.path().to_path_buf());
        state.sessions.lock().await.insert(
            "guard-upload".to_string(),
            std::sync::Arc::new(tokio::sync::Mutex::new(outcome.entry)),
        );
        let db = Database::in_memory();
        let guard = state.guard.clone();
        guard.unlock("guard-upload", 5).expect("unlock");
        let _write = guard.begin_write("guard-upload").expect("write lock");

        let base = crate::test_support::remote_fixture_dir()
            .unwrap_or_else(|| "/vol1/1000/lg-filetest".to_string());
        let remote = format!("{base}/lg-upload-{}.txt", crate::alerts::now_ms());
        let jobs = vec![UploadJob {
            local_path: local.display().to_string(),
            remote_path: remote.clone(),
            overwrite: false,
            size: content.len() as u64,
        }];
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

        let outcome = execute_upload(
            &state,
            &db,
            "guard-upload",
            "upload-fixture",
            &jobs,
            "unlock:5m",
            cancel,
            |_| {},
        )
        .await;

        let cleanup = async {
            let _ = execute_delete(&state, &db, "guard-upload", &remote, "file", "unlock:5m").await;
        };

        match outcome {
            Ok(result) => {
                let entry = crate::ssh::sftp::list::stat(&state, "guard-upload", &remote)
                    .await
                    .expect("stat uploaded");
                assert_eq!(entry.size, content.len() as u64);
                let rows = db.audit_list(10, 0).expect("audit list");
                assert!(rows
                    .iter()
                    .any(|row| row.op == UPLOAD_OP && row.result == "success"));
                let rolled = rollback_upload(
                    &state,
                    &db,
                    "guard-upload",
                    result.audit_id,
                    &remote,
                    false,
                    "unlock:5m",
                )
                .await
                .expect("rollback upload");
                assert_eq!(rolled.path, remote);
                assert!(crate::ssh::sftp::list::stat(&state, "guard-upload", &remote)
                    .await
                    .is_err());
            }
            Err(error) => {
                cleanup.await;
                panic!("upload failed: {error:?}");
            }
        }

        cleanup.await;
        state.sessions.lock().await.remove("guard-upload");
    }
}

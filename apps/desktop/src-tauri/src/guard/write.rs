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
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteResult {
    pub audit_id: i64,
    pub path: String,
}

pub fn build_preview(preview_id: String, path: &str, expires_at_ms: i64) -> WritePreview {
    WritePreview {
        preview_id,
        kind: "mkdir".to_string(),
        path: path.to_string(),
        operation: "新建文件夹".to_string(),
        target: path.to_string(),
        impact: format!("将在 {path} 创建文件夹"),
        rollback: "执行后可在审计中回退（仅删除空目录）".to_string(),
        irreversible: false,
        expires_at_ms,
    }
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

pub fn rollback_block_reason(row: &AuditRow, has_rollback: bool) -> Option<&'static str> {
    if row.op != MKDIR_OP {
        return Some("只有新建文件夹支持回退");
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
fn absolute_target(path: &str) -> Result<(), SshError> {
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

fn record(
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
            .map_err(|error| SshError {
                message: format!("文件夹已创建，但审计写入失败：{}", error.message),
                ..error
            })?;
            Ok(WriteResult {
                audit_id,
                path: path.to_string(),
            })
        }
        Err(error) => {
            let result = outcome_for(&error);
            let _ = record(
                db,
                server_id,
                &server_name,
                MKDIR_OP,
                &params,
                result,
                started,
                permit_source,
                None,
            );
            if result == "unknown" {
                Err(failed_unknown("创建文件夹", &error))
            } else {
                Err(failed("创建文件夹", &error))
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
            .map_err(|error| SshError {
                message: format!("目录已删除，但审计写入失败：{}", error.message),
                ..error
            })?;
            Ok(WriteResult {
                audit_id: new_id,
                path: path.to_string(),
            })
        }
        Err(error) => {
            let result = outcome_for(&error);
            let _ = record(
                db,
                server_id,
                &server_name,
                MKDIR_ROLLBACK_OP,
                &params,
                result,
                started,
                permit_source,
                Some(audit_id),
            );
            if result == "unknown" {
                Err(failed_unknown("删除文件夹", &error))
            } else {
                Err(failed("删除文件夹", &error))
            }
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
    fn preview_has_four_sections_and_is_reversible() {
        let preview = build_preview("abc".to_string(), "/data/reports", 1_700_000_060_000);
        assert_eq!(preview.preview_id, "abc");
        assert_eq!(preview.kind, "mkdir");
        assert_eq!(preview.operation, "新建文件夹");
        assert_eq!(preview.target, "/data/reports");
        assert_eq!(preview.impact, "将在 /data/reports 创建文件夹");
        assert!(preview.rollback.contains("审计"));
        assert!(!preview.irreversible);
        assert_eq!(preview.expires_at_ms, 1_700_000_060_000);
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
        let preview = serde_json::to_value(build_preview("abc".to_string(), "/data", 7)).unwrap();
        assert_eq!(preview["previewId"], "abc");
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
        let state = AppState::new(std::path::PathBuf::from("."));

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
        let state = AppState::new(std::path::PathBuf::from("."));

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
        let state = AppState::new(std::path::PathBuf::from("."));

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
}

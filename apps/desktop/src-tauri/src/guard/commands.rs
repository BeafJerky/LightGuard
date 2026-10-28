//! 写操作的前端命令：许可、一次性预览、执行、回退与审计读取。
//!
//! 这里只做参数校验、顺序编排与错误码映射，真正的远端改动全部在 [`super::write`]。
//! 执行顺序遵循规格 §10.3：预览 → 许可 → 会话 → 写锁 → 执行 → 审计。

use std::sync::Arc;

use serde_json::Value;
use tauri::State;

use crate::db::{AuditRow, Database, DbError, DbState};
use crate::ssh::error::SshError;
use crate::ssh::sftp::dto::FileListing;
use crate::ssh::sftp::{client, list};
use crate::ssh::state::AppState;

use super::permit::PermitStatus;
use super::preview::PREVIEW_TTL;
use super::write::{self, build_preview, WritePreview, WriteResult};
use super::GuardService;

fn db_error(error: DbError) -> SshError {
    SshError {
        code: error.code,
        message: error.message,
        hint: error.hint,
        stage: None,
        retryable: false,
    }
}

fn database(state: &State<'_, DbState>) -> Result<Arc<Database>, SshError> {
    state
        .handle()
        .ok_or_else(|| db_error(DbError::unavailable("本地数据库未就绪")))
}

fn preview_failed(error: SshError) -> SshError {
    SshError::preview_failed(error.message)
}

/// 命令边界上的目录校验：空串与相对路径一律拒绝，避免远端把它解析到主目录，
/// 也避免预览与审计里出现不是用户当前所在目录的目标。
fn absolute_dir(dir: &str) -> Result<String, SshError> {
    let trimmed = dir.trim();
    if trimmed.is_empty() {
        return Err(SshError::invalid_input("目标目录不能为空"));
    }
    if !trimmed.starts_with('/') {
        return Err(SshError::invalid_input("目标目录必须是绝对路径"));
    }
    if trimmed.chars().any(char::is_control) {
        return Err(SshError::invalid_input("目标目录包含非法字符"));
    }
    Ok(trimmed.to_string())
}

pub fn page_bounds(limit: i64, offset: i64) -> (i64, i64) {
    (limit.clamp(1, 500), offset.max(0))
}

/// 回退的前置条件：目标仍是当初创建的那个目录，并且已经为空。
///
/// 规范只允许回退"本应用创建的空目录"：这里先确认目录存在、仍是原路径（父目录没被
/// 换成符号链接）且没有内容，再交给 `rmdir` 做最终判定。预先检查不是为了替代
/// `rmdir` 的原子拒绝，而是为了让拒绝有明确原因，也不会在审计里留下一次从未发生
/// 过的远端尝试。判定是纯函数，测试可以直接喂各种列举结果。
pub(crate) fn empty_original_dir(
    path: &str,
    listed: Result<FileListing, SshError>,
) -> Result<(), SshError> {
    let listing = match listed {
        Ok(listing) => listing,
        Err(error) if error.code == "FILE_NOT_FOUND" => {
            return Err(SshError::invalid_input("目标目录已不存在，无法回退"));
        }
        Err(error) => return Err(error),
    };
    if listing.canonical_path != path {
        return Err(SshError::invalid_input("目标路径已变更，无法回退"));
    }
    if !listing.entries.is_empty() {
        return Err(SshError::invalid_input("目标目录非空，无法回退"));
    }
    Ok(())
}

async fn original_dir_still_empty(
    state: &AppState,
    server_id: &str,
    path: &str,
) -> Result<(), SshError> {
    empty_original_dir(path, list::list(state, server_id, path).await)
}

#[tauri::command]
pub async fn write_preview(
    state: State<'_, Arc<AppState>>,
    guard: State<'_, Arc<GuardService>>,
    server_id: String,
    dir: String,
    name: String,
) -> Result<WritePreview, SshError> {
    guard.require_permit(&server_id)?;
    let dir = absolute_dir(&dir)?;
    let name = write::validate_folder_name(&name)?;
    let parent = client::canonicalize(state.inner(), &server_id, &dir)
        .await
        .map_err(preview_failed)?;
    let target = write::join_target(&parent, &name)?;
    if client::try_exists(state.inner(), &server_id, &target)
        .await
        .map_err(preview_failed)?
    {
        return Err(SshError::preview_failed(format!("目标已存在：{target}")));
    }
    let preview_id = guard.store_preview(
        &server_id,
        write::MKDIR_OP,
        serde_json::json!({ "path": target }),
    );
    let expires_at_ms = crate::alerts::now_ms() + PREVIEW_TTL.as_millis() as i64;
    Ok(build_preview(preview_id, &target, expires_at_ms))
}

#[tauri::command]
pub async fn write_execute(
    state: State<'_, Arc<AppState>>,
    guard: State<'_, Arc<GuardService>>,
    db_state: State<'_, DbState>,
    preview_id: String,
) -> Result<WriteResult, SshError> {
    let preview = guard.take_preview(&preview_id)?;
    // 预览缓存不按服务器分区，执行时也只认预览自己记下的服务器与操作：
    // 服务器由后端写入，前端无法用别的服务器的 id 影响执行目标。
    if preview.op != write::MKDIR_OP {
        return Err(SshError::preview_failed(format!(
            "预览操作不受支持：{}",
            preview.op
        )));
    }
    let minutes = guard.require_permit(&preview.server_id)?;
    client::session_handle(state.inner(), &preview.server_id).await?;
    let path = preview
        .payload
        .get("path")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| SshError::preview_failed("预览内容缺少目标路径"))?;
    let db = database(&db_state)?;
    let lock = guard.begin_write(&preview.server_id)?;
    let permit_source = format!("unlock:{minutes}m");
    let outcome =
        write::execute_mkdir(state.inner(), &db, &preview.server_id, &path, &permit_source).await;
    drop(lock);
    outcome
}

#[tauri::command]
pub async fn write_rollback(
    state: State<'_, Arc<AppState>>,
    guard: State<'_, Arc<GuardService>>,
    db_state: State<'_, DbState>,
    audit_id: i64,
) -> Result<WriteResult, SshError> {
    let db = database(&db_state)?;
    let row = db
        .audit_get(audit_id)
        .map_err(db_error)?
        .ok_or_else(|| SshError::invalid_input("审计记录不存在"))?;
    let has_rollback = db.audit_has_rollback(audit_id).map_err(db_error)?;
    if let Some(reason) = write::rollback_block_reason(&row, has_rollback) {
        return Err(SshError::invalid_input(reason));
    }
    let path = write::audit_path(&row.params)
        .ok_or_else(|| SshError::invalid_input("审计记录缺少目标路径"))?;
    let minutes = guard.require_permit(&row.server_id)?;
    client::session_handle(state.inner(), &row.server_id).await?;
    let lock = guard.begin_write(&row.server_id)?;
    let permit_source = format!("unlock:{minutes}m");
    let outcome = match original_dir_still_empty(state.inner(), &row.server_id, &path).await {
        Ok(()) => {
            write::rollback_mkdir(
                state.inner(),
                &db,
                &row.server_id,
                audit_id,
                &path,
                &permit_source,
            )
            .await
        }
        // 前置条件不满足时不落审计行：那不是一次远端尝试，只是拒绝。
        Err(error) => Err(error),
    };
    drop(lock);
    outcome
}

#[tauri::command]
pub fn audit_list(
    db_state: State<'_, DbState>,
    limit: i64,
    offset: i64,
) -> Result<Vec<AuditRow>, SshError> {
    let db = database(&db_state)?;
    let (limit, offset) = page_bounds(limit, offset);
    db.audit_list(limit, offset).map_err(db_error)
}

#[tauri::command]
pub fn permit_status(
    guard: State<'_, Arc<GuardService>>,
    server_id: String,
) -> PermitStatus {
    guard.status(&server_id)
}

#[tauri::command]
pub async fn permit_unlock(
    state: State<'_, Arc<AppState>>,
    guard: State<'_, Arc<GuardService>>,
    server_id: String,
    minutes: u8,
) -> Result<PermitStatus, SshError> {
    // 规格 §9.2：只能在有活跃会话的服务器上解锁。
    client::session_handle(state.inner(), &server_id).await?;
    guard.unlock(&server_id, minutes)
}

#[tauri::command]
pub fn permit_lock(
    guard: State<'_, Arc<GuardService>>,
    server_id: String,
) -> Result<PermitStatus, SshError> {
    guard.lock(&server_id)
}

#[cfg(test)]
mod tests {
    use super::{absolute_dir, empty_original_dir, page_bounds, GuardService};
    use crate::guard::write;
    use crate::ssh::error::SshError;
    use crate::ssh::sftp::dto::{FileEntry, FileKind, FileListing};

    #[test]
    fn audit_page_bounds_are_clamped() {
        assert_eq!(page_bounds(0, -5), (1, 0));
        assert_eq!(page_bounds(200, 400), (200, 400));
        assert_eq!(page_bounds(1000, 0), (500, 0));
    }

    #[test]
    fn preview_dir_must_be_absolute() {
        assert_eq!(absolute_dir("/data/reports").unwrap(), "/data/reports");
        assert_eq!(absolute_dir("/").unwrap(), "/");
        assert_eq!(absolute_dir("  /data  ").unwrap(), "/data");
        for dir in ["", "   ", "data", "./data", "~/data", "/data/\0x"] {
            assert_eq!(
                absolute_dir(dir).unwrap_err().code,
                "INVALID_INPUT",
                "dir {dir:?}"
            );
        }
    }

    fn listing(canonical_path: &str, entries: Vec<FileEntry>) -> FileListing {
        FileListing {
            path: "/data/x".to_string(),
            canonical_path: canonical_path.to_string(),
            entries,
            truncated: false,
        }
    }

    fn file_entry(name: &str) -> FileEntry {
        FileEntry {
            name: name.to_string(),
            path: format!("/data/x/{name}"),
            kind: FileKind::File,
            size: 1,
            mtime: None,
            mode: 0o644,
            mode_text: "-rw-r--r--".to_string(),
            uid: None,
            gid: None,
            symlink_target: None,
            target_kind: None,
        }
    }

    #[test]
    fn rollback_refuses_a_missing_original_dir() {
        let error =
            empty_original_dir("/data/x", Err(SshError::file_not_found("/data/x"))).unwrap_err();
        assert_eq!(error.code, "INVALID_INPUT");
        assert_eq!(error.message, "目标目录已不存在，无法回退");
    }

    #[test]
    fn rollback_refuses_non_empty_or_moved_dirs() {
        assert!(empty_original_dir("/data/x", Ok(listing("/data/x", vec![]))).is_ok());

        let filled = empty_original_dir(
            "/data/x",
            Ok(listing("/data/x", vec![file_entry("a.txt")])),
        )
        .unwrap_err();
        assert_eq!(filled.code, "INVALID_INPUT");
        assert_eq!(filled.message, "目标目录非空，无法回退");

        // 父目录被换成符号链接等情况下，服务端返回的规范路径不再等于原路径。
        let moved = empty_original_dir("/data/x", Ok(listing("/data/y", vec![]))).unwrap_err();
        assert_eq!(moved.code, "INVALID_INPUT");
        assert_eq!(moved.message, "目标路径已变更，无法回退");

        // 其它读取错误原样透出，不伪装成"拒绝回退"。
        let denied =
            empty_original_dir("/data/x", Err(SshError::permission_denied("/data/x"))).unwrap_err();
        assert_eq!(denied.code, "PERMISSION_DENIED");
    }

    /// 命令层的顺序是"取预览 → 校验许可 → …"：预览在真正执行前就被取走，
    /// 同一个 id 不能再次执行，即使随后解锁也一样。
    #[test]
    fn execute_consumes_the_preview_exactly_once() {
        let guard = GuardService::new(None);
        let id = guard.store_preview("srv", write::MKDIR_OP, serde_json::json!({ "path": "/data/x" }));

        let preview = guard.take_preview(&id).unwrap();
        assert_eq!(preview.server_id, "srv");
        assert_eq!(preview.op, write::MKDIR_OP);
        assert_eq!(preview.payload["path"], "/data/x");
        assert_eq!(
            guard.require_permit(&preview.server_id).unwrap_err().code,
            "PERMIT_REQUIRED"
        );

        // 已消费的预览报 PREVIEW_EXPIRED，前端只能重新发起预览。
        assert_eq!(guard.take_preview(&id).unwrap_err().code, "PREVIEW_EXPIRED");
        guard.unlock("srv", 5).unwrap();
        assert_eq!(guard.take_preview(&id).unwrap_err().code, "PREVIEW_EXPIRED");
    }
}

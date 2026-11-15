//! 写操作的前端命令：许可、一次性预览、执行、回退与审计读取。
//!
//! 这里只做参数校验、顺序编排与错误码映射，真正的远端改动全部在 [`super::write`]。
//! 执行顺序遵循规格 §10.3：预览 → 许可 → 会话 → 写锁 → 执行 → 审计。

use std::sync::Arc;

use serde_json::Value;
use tauri::ipc::Channel;
use tauri::State;

use crate::db::{AuditRow, Database, DbError, DbState};
use crate::docker::validate as docker_validate;
use crate::docker::write::{self as docker_write, WritePayload};
use crate::ssh::error::SshError;
use crate::ssh::sftp::dto::FileListing;
use crate::ssh::sftp::{client, list};
use crate::ssh::state::AppState;

use super::permit::PermitStatus;
use super::preview::PREVIEW_TTL;
use super::write::{self, build_mkdir_preview, WritePreview, WriteResult};
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

/// 容器启停的回退请求：只接受 start / stop，且必须能从审计参数还原容器全 ID。
fn docker_rollback_payload(row: &AuditRow) -> Option<WritePayload> {
    let (action, _) = docker_write::rollback_plan(&row.op)?;
    let params: Value = serde_json::from_str(&row.params).ok()?;
    let container_id = params.get("containerId")?.as_str()?;
    let container_id = docker_validate::container_id(container_id).ok()?;
    let display = params
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(&container_id)
        .to_string();
    Some(WritePayload {
        server_id: row.server_id.clone(),
        kind: format!("container.{action}"),
        target: container_id,
        display,
        audit_op: format!("{}.rollback", row.op),
        extra: serde_json::json!({}),
    })
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
    kind: String,
    payload: Value,
) -> Result<WritePreview, SshError> {
    guard.require_permit(&server_id)?;
    match kind.as_str() {
        "mkdir" => {
            let dir = payload_str(&payload, "dir")?;
            let name = payload_str(&payload, "name")?;
            let dir = absolute_dir(dir)?;
            let name = write::validate_folder_name(name)?;
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
            Ok(build_mkdir_preview(preview_id, &target, expires_at_ms))
        }
        "rename" => {
            let path = payload_str(&payload, "path")?;
            let new_name = payload_str(&payload, "newName")?;
            let (from, to) = write::rename_targets(path, new_name)?;
            if !client::try_exists(state.inner(), &server_id, &from)
                .await
                .map_err(preview_failed)?
            {
                return Err(SshError::preview_failed("目标已不存在，请刷新后重试"));
            }
            if client::try_exists(state.inner(), &server_id, &to)
                .await
                .map_err(preview_failed)?
            {
                return Err(SshError::preview_failed(format!("目标已存在：{to}")));
            }
            let preview_id = guard.store_preview(
                &server_id,
                write::RENAME_OP,
                serde_json::json!({ "from": from, "to": to }),
            );
            let expires_at_ms = crate::alerts::now_ms() + PREVIEW_TTL.as_millis() as i64;
            Ok(write::build_rename_preview(
                preview_id,
                &from,
                &to,
                expires_at_ms,
            ))
        }
        "chmod" => {
            let path = payload_str(&payload, "path")?;
            let mode = write::parse_mode(payload_str(&payload, "mode")?)?;
            let entry = client::stat_entry(state.inner(), &server_id, path)
                .await
                .map_err(preview_failed)?;
            if entry.kind == crate::ssh::sftp::dto::FileKind::Symlink {
                return Err(SshError::preview_failed("不支持修改符号链接的权限"));
            }
            let previous_mode = entry.mode & 0o7777;
            if previous_mode == mode {
                return Err(SshError::preview_failed("目标权限与当前一致"));
            }
            let preview_id = guard.store_preview(
                &server_id,
                write::CHMOD_OP,
                serde_json::json!({
                    "path": path,
                    "mode": mode,
                    "previousMode": previous_mode,
                }),
            );
            let expires_at_ms = crate::alerts::now_ms() + PREVIEW_TTL.as_millis() as i64;
            Ok(write::build_chmod_preview(
                preview_id,
                path,
                previous_mode,
                mode,
                expires_at_ms,
            ))
        }
        "delete" => {
            let path = payload_str(&payload, "path")?;
            write::absolute_target(path)?;
            let entry = client::stat_entry(state.inner(), &server_id, path)
                .await
                .map_err(preview_failed)?;
            let kind = if entry.kind == crate::ssh::sftp::dto::FileKind::Dir {
                "dir"
            } else {
                "file"
            };
            let stats = if kind == "dir" {
                let client = client::acquire(state.inner(), &server_id)
                    .await
                    .map_err(preview_failed)?;
                crate::ssh::sftp::delete::count_tree(
                    &client,
                    path,
                    1000,
                    crate::ssh::sftp::delete::MAX_DELETE_DEPTH,
                    std::time::Duration::from_secs(5),
                )
                .await
                .map_err(preview_failed)?
            } else {
                crate::ssh::sftp::delete::DeleteStats {
                    entries: 1,
                    bytes: entry.size,
                    truncated: false,
                }
            };
            let preview_id = guard.store_preview(
                &server_id,
                write::DELETE_OP,
                serde_json::json!({ "path": path, "kind": kind }),
            );
            let expires_at_ms = crate::alerts::now_ms() + PREVIEW_TTL.as_millis() as i64;
            Ok(write::build_delete_preview(
                preview_id,
                path,
                kind,
                stats,
                expires_at_ms,
            ))
        }
        "upload" => {
            let dir = payload_str(&payload, "dir")?;
            let policy = payload
                .get("conflictPolicy")
                .and_then(Value::as_str)
                .unwrap_or("skip");
            let files = payload
                .get("files")
                .and_then(Value::as_array)
                .ok_or_else(|| SshError::invalid_input("预览缺少上传文件清单"))?;
            if files.is_empty() || files.len() > 50 {
                return Err(SshError::preview_failed("一次最多上传 50 个文件"));
            }
            let dir = client::canonicalize(state.inner(), &server_id, dir)
                .await
                .map_err(preview_failed)?;
            let mut items: Vec<write::UploadPlanItem> = Vec::new();
            let mut jobs: Vec<write::UploadJob> = Vec::new();
            let mut skipped = 0usize;
            for file in files {
                let local = file
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or_else(|| SshError::invalid_input("上传项缺少本地路径"))?;
                let raw_name = file
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| SshError::invalid_input("上传项缺少文件名"))?;
                let meta = std::fs::metadata(local)
                    .map_err(|_| SshError::preview_failed("本地文件不可读"))?;
                if !meta.is_file() {
                    return Err(SshError::preview_failed("只支持上传普通文件"));
                }
                let name = write::validate_folder_name(raw_name)?;
                let mut target = write::join_target(&dir, &name)?;
                let exists = client::try_exists(state.inner(), &server_id, &target)
                    .await
                    .map_err(preview_failed)?;
                let action = write::plan_upload(&name, exists, policy);
                let (action_label, renamed_to, rollbackable) = match action {
                    write::UploadAction::Create => ("create", None, true),
                    write::UploadAction::Overwrite => ("overwrite", None, false),
                    write::UploadAction::Skip => ("skip", None, true),
                    write::UploadAction::Rename(_) => {
                        let mut chosen = None;
                        for index in 1..=999 {
                            let candidate = write::renamed_name(&name, index);
                            let candidate_path = write::join_target(&dir, &candidate)?;
                            if !client::try_exists(state.inner(), &server_id, &candidate_path)
                                .await
                                .map_err(preview_failed)?
                            {
                                target = candidate_path;
                                chosen = Some(candidate);
                                break;
                            }
                        }
                        ("rename", chosen, true)
                    }
                };
                if action_label == "skip" {
                    skipped += 1;
                } else {
                    jobs.push(write::UploadJob {
                        local_path: local.to_string(),
                        remote_path: target.clone(),
                        overwrite: action_label == "overwrite",
                        size: meta.len(),
                    });
                }
                items.push(write::UploadPlanItem {
                    name,
                    remote_path: target,
                    size: meta.len(),
                    action: action_label.to_string(),
                    renamed_to,
                    rollbackable,
                });
            }
            if jobs.is_empty() {
                return Err(SshError::preview_failed("没有可上传的文件（全部被跳过）"));
            }
            let preview_id = guard.store_preview(
                &server_id,
                write::UPLOAD_OP,
                serde_json::json!({
                    "dir": dir,
                    "jobs": jobs,
                    "skipped": skipped,
                }),
            );
            let expires_at_ms = crate::alerts::now_ms() + PREVIEW_TTL.as_millis() as i64;
            Ok(write::build_upload_preview(
                preview_id,
                &dir,
                &items,
                policy,
                expires_at_ms,
            ))
        }
        other => Err(SshError::invalid_input(format!("不支持的写操作：{other}"))),
    }
}

fn payload_str<'a>(payload: &'a Value, key: &str) -> Result<&'a str, SshError> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| SshError::invalid_input(format!("预览缺少参数：{key}")))
}

#[tauri::command]
pub async fn write_execute(
    state: State<'_, Arc<AppState>>,
    guard: State<'_, Arc<GuardService>>,
    db_state: State<'_, DbState>,
    preview_id: String,
    on_event: Channel<write::WriteProgress>,
) -> Result<WriteResult, SshError> {
    let preview = guard.take_preview(&preview_id)?;
    // 预览缓存不按服务器分区，执行时也只认预览自己记下的服务器与操作：
    // 服务器由后端写入，前端无法用别的服务器的 id 影响执行目标。
    let minutes = guard.require_permit(&preview.server_id)?;
    client::session_handle(state.inner(), &preview.server_id).await?;
    let db = database(&db_state)?;
    let lock = guard.begin_write(&preview.server_id)?;
    let permit_source = format!("unlock:{minutes}m");
    let outcome = match preview.op.as_str() {
        write::MKDIR_OP => {
            let path = preview
                .payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| SshError::preview_failed("预览内容缺少目标路径"))?;
            write::execute_mkdir(state.inner(), &db, &preview.server_id, path, &permit_source)
                .await
        }
        write::RENAME_OP => {
            let (from, to) = preview
                .payload
                .get("from")
                .and_then(Value::as_str)
                .zip(preview.payload.get("to").and_then(Value::as_str))
                .ok_or_else(|| SshError::preview_failed("预览内容缺少重命名路径"))?;
            write::execute_rename(
                state.inner(),
                &db,
                &preview.server_id,
                from,
                to,
                &permit_source,
            )
            .await
        }
        write::CHMOD_OP => {
            let payload = &preview.payload;
            let path = payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| SshError::preview_failed("预览内容缺少权限目标"))?;
            let mode = payload
                .get("mode")
                .and_then(Value::as_u64)
                .ok_or_else(|| SshError::preview_failed("预览内容缺少目标权限"))?
                as u32;
            let previous_mode = payload
                .get("previousMode")
                .and_then(Value::as_u64)
                .ok_or_else(|| SshError::preview_failed("预览内容缺少原权限"))?
                as u32;
            write::execute_chmod(
                state.inner(),
                &db,
                &preview.server_id,
                path,
                mode,
                previous_mode,
                &permit_source,
                write::CHMOD_OP,
                None,
            )
            .await
        }
        write::DELETE_OP => {
            let path = preview
                .payload
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| SshError::preview_failed("预览内容缺少删除目标"))?;
            let kind = preview
                .payload
                .get("kind")
                .and_then(Value::as_str)
                .ok_or_else(|| SshError::preview_failed("预览内容缺少目标类型"))?;
            write::execute_delete(
                state.inner(),
                &db,
                &preview.server_id,
                path,
                kind,
                &permit_source,
            )
            .await
        }
        write::UPLOAD_OP => {
            let jobs: Vec<write::UploadJob> = serde_json::from_value(
                preview
                    .payload
                    .get("jobs")
                    .cloned()
                    .unwrap_or_else(|| Value::Array(Vec::new())),
            )
            .map_err(|_| SshError::preview_failed("预览内容缺少上传清单"))?;
            if jobs.is_empty() {
                return Err(SshError::preview_failed("没有可上传的文件"));
            }
            let transfer_id = preview_id.clone();
            let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            state.transfers.lock().await.insert(
                transfer_id.clone(),
                crate::ssh::sftp::client::TransferHandle {
                    server_id: preview.server_id.clone(),
                    cancel: cancel.clone(),
                },
            );
            let outcome = write::execute_upload(
                state.inner(),
                &db,
                &preview.server_id,
                &transfer_id,
                &jobs,
                &permit_source,
                cancel,
                |progress| {
                    let _ = on_event.send(progress);
                },
            )
            .await;
            state.transfers.lock().await.remove(&transfer_id);
            outcome
        }
        other => {
            return Err(SshError::preview_failed(format!(
                "预览操作不受支持：{other}"
            )))
        }
    };
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
    if row.op.starts_with("docker.container.") {
        let payload = docker_rollback_payload(&row)
            .ok_or_else(|| SshError::invalid_input("审计记录缺少容器信息"))?;
        let minutes = guard.require_permit(&row.server_id)?;
        client::session_handle(state.inner(), &row.server_id).await?;
        let lock = guard.begin_write(&row.server_id)?;
        let permit_source = format!("unlock:{minutes}m");
        let outcome = docker_write::execute(
            state.inner(),
            &db,
            &payload,
            &permit_source,
            Some(audit_id),
            &mut |_| {},
            None,
        )
        .await;
        drop(lock);
        let outcome = outcome?;
        if outcome.result != "success" {
            // 执行已落审计；失败按错误返回，避免前端误报“已回退”。
            return Err(SshError::docker_failed(outcome.message));
        }
        // 复用 WriteResult.path 承载目标展示名；前端只关心是否成功。
        return Ok(WriteResult {
            audit_id: outcome.audit_id,
            path: outcome.target,
        });
    }

    if row.op == write::UPLOAD_OP {
        let value: Value = serde_json::from_str(&row.params)
            .map_err(|_| SshError::invalid_input("审计记录缺少上传信息"))?;
        let path = value
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| SshError::invalid_input("审计记录缺少上传目标"))?;
        // 旧记录没有 overwrite 字段时按“不可回退”处理，避免误删覆盖前的同名文件。
        let overwrite = value
            .get("overwrite")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let minutes = guard.require_permit(&row.server_id)?;
        client::session_handle(state.inner(), &row.server_id).await?;
        let lock = guard.begin_write(&row.server_id)?;
        let permit_source = format!("unlock:{minutes}m");
        let outcome = write::rollback_upload(
            state.inner(),
            &db,
            &row.server_id,
            audit_id,
            path,
            overwrite,
            &permit_source,
        )
        .await;
        drop(lock);
        return outcome;
    }

    if row.op == write::CHMOD_OP {
        let value: Value = serde_json::from_str(&row.params)
            .map_err(|_| SshError::invalid_input("审计记录缺少权限信息"))?;
        let path = value
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(|| SshError::invalid_input("审计记录缺少权限目标"))?;
        let previous_mode = value
            .get("previousMode")
            .and_then(Value::as_u64)
            .ok_or_else(|| SshError::invalid_input("审计记录缺少原权限"))?
            as u32;
        let minutes = guard.require_permit(&row.server_id)?;
        client::session_handle(state.inner(), &row.server_id).await?;
        let lock = guard.begin_write(&row.server_id)?;
        let permit_source = format!("unlock:{minutes}m");
        let outcome = write::rollback_chmod(
            state.inner(),
            &db,
            &row.server_id,
            audit_id,
            path,
            previous_mode,
            &permit_source,
        )
        .await;
        drop(lock);
        return outcome;
    }

    if row.op == write::RENAME_OP {
        let (from, to) = write::audit_rename_paths(&row.params)
            .ok_or_else(|| SshError::invalid_input("审计记录缺少重命名路径"))?;
        let minutes = guard.require_permit(&row.server_id)?;
        client::session_handle(state.inner(), &row.server_id).await?;
        let lock = guard.begin_write(&row.server_id)?;
        let permit_source = format!("unlock:{minutes}m");
        let outcome = write::rollback_rename(
            state.inner(),
            &db,
            &row.server_id,
            audit_id,
            &from,
            &to,
            &permit_source,
        )
        .await;
        drop(lock);
        return outcome;
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
    use super::{absolute_dir, docker_rollback_payload, empty_original_dir, page_bounds, GuardService};
    use crate::db::AuditRow;
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

    #[test]
    fn docker_rollback_payload_reverses_the_action() {
        let id = "a".repeat(64);
        let row = AuditRow {
            id: 1,
            at_ms: 0,
            server_id: "srv".to_string(),
            server_name: "nas".to_string(),
            op: "docker.container.start".to_string(),
            params: format!(r#"{{"action":"start","containerId":"{id}","name":"web（aaaaaaaaaaaa）"}}"#),
            result: "success".to_string(),
            duration_ms: 1,
            permit_source: "unlock:5m".to_string(),
            rollback_of: None,
        };
        let payload = docker_rollback_payload(&row).unwrap();
        assert_eq!(payload.kind, "container.stop");
        assert_eq!(payload.audit_op, "docker.container.start.rollback");
        assert_eq!(payload.target, id);
        assert_eq!(payload.display, "web（aaaaaaaaaaaa）");

        let restart = AuditRow { op: "docker.container.restart".to_string(), ..row.clone() };
        assert!(docker_rollback_payload(&restart).is_none());
        let empty = AuditRow { params: "{}".to_string(), ..row.clone() };
        assert!(docker_rollback_payload(&empty).is_none());
        let malformed = AuditRow {
            params: r#"{"containerId":"../../../etc/passwd"}"#.to_string(),
            ..row
        };
        assert!(docker_rollback_payload(&malformed).is_none());
    }

    /// 执行方向与落库 op 相反：回退一条 stop 实际执行 start，审计仍记在原 op 上。
    #[test]
    fn docker_rollback_of_a_stop_starts_the_container() {
        let id = "a".repeat(64);
        let stop = AuditRow {
            id: 2,
            at_ms: 0,
            server_id: "srv".to_string(),
            server_name: "nas".to_string(),
            op: "docker.container.stop".to_string(),
            params: format!(r#"{{"action":"stop","containerId":"{id}","name":"web（aaaaaaaaaaaa）"}}"#),
            result: "success".to_string(),
            duration_ms: 1,
            permit_source: "unlock:5m".to_string(),
            rollback_of: None,
        };
        let reversed = docker_rollback_payload(&stop).unwrap();
        assert_eq!(reversed.kind, "container.start");
        assert_eq!(reversed.audit_op, "docker.container.stop.rollback");
        assert_eq!(reversed.target, id);
        assert_eq!(reversed.display, "web（aaaaaaaaaaaa）");
    }
}

use std::sync::Arc;

use tauri::State;

use super::{DbError, DbOp, DbState, LoadResult};
use crate::alerts::AlertService;
use crate::guard::GuardService;

#[tauri::command]
pub fn db_load(state: State<'_, DbState>) -> Result<LoadResult, DbError> {
    state.database()?.load()
}

/// 写库之后同步内存里的服务状态：设置或快照变化会改变告警规则，清空数据会让
/// 所有许可与一次性预览失效——否则清理后仍能凭旧许可写远端。
#[tauri::command]
pub async fn db_write(
    state: State<'_, DbState>,
    alerts: State<'_, Arc<AlertService>>,
    guard: State<'_, Arc<GuardService>>,
    op: DbOp,
) -> Result<(), DbError> {
    let reload = matches!(op, DbOp::SettingsSet { .. } | DbOp::Replace { .. });
    let reset = matches!(op, DbOp::Reset);
    state.database()?.apply(op)?;
    if reload {
        alerts.reload_settings();
    }
    if reset {
        alerts.reset();
        guard.clear_all();
    }
    Ok(())
}

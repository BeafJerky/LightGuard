use tauri::State;

use super::{DbError, DbOp, DbState, LoadResult};

#[tauri::command]
pub fn db_load(state: State<'_, DbState>) -> Result<LoadResult, DbError> {
    state.database()?.load()
}

#[tauri::command]
pub fn db_write(state: State<'_, DbState>, op: DbOp) -> Result<(), DbError> {
    state.database()?.apply(op)
}

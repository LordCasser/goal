//! Maintenance commands: schema version and backup export.

use tauri::State;

use crate::db::{self, Db};
use crate::error::AppResult;

/// Reports the applied migration version. Useful as a wire-up smoke check.
#[tauri::command]
pub fn get_schema_version(db: State<'_, Db>) -> AppResult<i64> {
    db.schema_version()
}

/// Exports a self-contained database copy (WAL checkpointed first).
#[tauri::command]
pub fn export_backup(db: State<'_, Db>, target_path: String) -> AppResult<()> {
    db::backup::export(&db, std::path::Path::new(&target_path))
}

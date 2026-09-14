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

/// Absolute path of the debug log directory (`<data dir>/logs`), so the
/// settings page can offer an entry point for attaching logs to feedback
/// (spec: local-logging, 定位日志). Derived from the data dir so it works
/// even before the logger has written its first line.
#[tauri::command]
pub fn get_debug_log_dir(app: tauri::AppHandle) -> AppResult<String> {
    let dir = db::data_dir(&app)?.join("logs");
    Ok(dir.display().to_string())
}

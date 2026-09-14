//! Settings and telemetry commands.

use tauri::State;

use crate::db::Db;
use crate::error::AppResult;
use crate::service::settings;
use crate::telemetry;

#[tauri::command]
pub fn get_settings(db: State<'_, Db>) -> AppResult<settings::Settings> {
    settings::get(&db)
}

#[tauri::command]
pub fn set_week_start_day(db: State<'_, Db>, day: i64) -> AppResult<()> {
    settings::set_week_start_day(&db, day)
}

#[tauri::command]
pub fn get_telemetry_settings(db: State<'_, Db>) -> AppResult<telemetry::TelemetrySettings> {
    telemetry::get_settings(&db)
}

/// Persists the switch and re-gates the live reporter. The default (unset)
/// stays disabled — the first-run notice is what introduces the toggle.
#[tauri::command]
pub fn set_telemetry_enabled(
    db: State<'_, Db>,
    reporter: State<'_, telemetry::Reporter>,
    errors: State<'_, telemetry::ErrorCategories>,
    enabled: bool,
) -> AppResult<()> {
    telemetry::set_enabled(&db, enabled)?;
    reporter.set_enabled(enabled);
    errors.set_enabled(enabled);
    Ok(())
}

/// Version, migration version and error-category counts — never plan content.
#[tauri::command]
pub fn export_diagnostics(
    db: State<'_, Db>,
    errors: State<'_, telemetry::ErrorCategories>,
) -> AppResult<telemetry::Diagnostics> {
    telemetry::export_diagnostics(&db, &errors)
}

//! Settings commands.

use tauri::State;

use crate::db::Db;
use crate::error::AppResult;
use crate::service::settings;

#[tauri::command]
pub fn get_settings(db: State<'_, Db>) -> AppResult<settings::Settings> {
    settings::get(&db)
}

#[tauri::command]
pub fn set_week_start_day(db: State<'_, Db>, day: i64) -> AppResult<()> {
    settings::set_week_start_day(&db, day)
}

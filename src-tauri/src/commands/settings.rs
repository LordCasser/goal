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

#[tauri::command]
pub fn set_theme(db: State<'_, Db>, theme: String) -> AppResult<()> {
    settings::set_theme(&db, theme)
}

#[tauri::command]
pub fn set_show_relation_lines(db: State<'_, Db>, show: bool) -> AppResult<()> {
    settings::set_show_relation_lines(&db, show)
}

#[tauri::command]
pub fn set_auto_carry_unfinished(db: State<'_, Db>, enabled: bool) -> AppResult<()> {
    settings::set_auto_carry_unfinished(&db, enabled)
}

#[tauri::command]
pub fn set_show_later_count(db: State<'_, Db>, show: bool) -> AppResult<()> {
    settings::set_show_later_count(&db, show)
}

#[tauri::command]
pub fn set_locale(db: State<'_, Db>, locale: String) -> AppResult<()> {
    settings::set_locale(&db, locale)
}

#[tauri::command]
pub fn set_log_level(db: State<'_, Db>, level: String) -> AppResult<()> {
    settings::set_log_level(&db, level)
}

#[tauri::command]
pub fn get_app_flag(db: State<'_, Db>, key: String) -> AppResult<Option<String>> {
    settings::get_app_flag(&db, key)
}

#[tauri::command]
pub fn set_app_flag(db: State<'_, Db>, key: String, value: String) -> AppResult<()> {
    settings::set_app_flag(&db, key, value)
}

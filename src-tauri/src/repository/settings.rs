//! SQL for the `app_settings` key-value aggregate.
//!
//! Non-sensitive settings only (see `docs/architecture.md`, 设置): credentials
//! belong in the system keychain and must never pass through here.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{from_rusqlite, AppResult};

pub const KEY_WEEK_START_DAY: &str = "week_start_day";
pub const KEY_LOG_LEVEL: &str = "log_level";
pub const KEY_THEME: &str = "theme";
pub const KEY_SHOW_RELATION_LINES: &str = "show_relation_lines";
pub const KEY_AUTO_CARRY_UNFINISHED: &str = "auto_carry_unfinished";
pub const KEY_SHOW_LATER_COUNT: &str = "show_later_count";

pub fn get(conn: &Connection, key: &str) -> AppResult<Option<String>> {
    conn.query_row(
        "SELECT value FROM app_settings WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .map_err(from_rusqlite)
}

pub fn set(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO app_settings (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

//! SQL for the `app_settings` key-value aggregate.
//!
//! Non-sensitive settings only (see `docs/architecture.md`, 设置): credentials
//! belong in the system keychain and must never pass through here.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{from_rusqlite, AppResult};

pub const KEY_WEEK_START_DAY: &str = "week_start_day";
pub const KEY_TELEMETRY_ENABLED: &str = "telemetry_enabled";
pub const KEY_TELEMETRY_ANONYMOUS_ID: &str = "telemetry_anonymous_id";

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

/// Sets the value only when the key does not exist yet.
pub fn set_if_absent(conn: &Connection, key: &str, value: &str) -> AppResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO app_settings (key, value) VALUES (?1, ?2)",
        params![key, value],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

//! Settings use cases: read/modify non-sensitive `app_settings` entries.

use rusqlite::Connection;

use crate::db::Db;
use crate::domain::calendar::is_valid_week_start_day;
use crate::error::{AppError, AppResult};
use crate::repository::settings as repo;

/// Inferred when no `week_start_day` is persisted yet. The product default is
/// Monday; the first UI that needs the value persists it once
/// (spec: 首次确定周起始日 — 之后不再变更).
pub const DEFAULT_WEEK_START_DAY: i64 = 1;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Settings {
    /// `Some` once the user's week start day has been determined.
    pub week_start_day: Option<i64>,
    /// Preferred surface theme: `"white"` or `"gray"`; `None` until first
    /// chosen (design.md §4.4 — the default white is a client-side fallback).
    pub theme: Option<String>,
}

pub fn get(db: &Db) -> AppResult<Settings> {
    let conn = db.pool().get()?;
    let week_start_day = repo::get(&conn, repo::KEY_WEEK_START_DAY)?
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| is_valid_week_start_day(*v));
    let theme = repo::get(&conn, repo::KEY_THEME)?.filter(|v| is_valid_theme(v));
    Ok(Settings {
        week_start_day,
        theme,
    })
}

/// The two user-confirmed light themes (design.md §4.4).
pub fn is_valid_theme(theme: &str) -> bool {
    matches!(theme, "white" | "gray")
}

pub fn set_theme(db: &Db, theme: String) -> AppResult<()> {
    if !is_valid_theme(&theme) {
        return Err(AppError::validation(
            "invalid_theme",
            "theme must be \"white\" or \"gray\"",
        ));
    }
    let conn = db.pool().get()?;
    repo::set(&conn, repo::KEY_THEME, &theme)
}

/// Persists the log level and applies it to the running logger so the change
/// takes effect without a restart (spec: local-logging, 级别调整).
pub fn set_log_level(db: &Db, level: String) -> AppResult<()> {
    let parsed = crate::logging::Level::parse(&level).ok_or_else(|| {
        AppError::validation(
            "invalid_log_level",
            "log level must be error, warn, info or debug",
        )
    })?;
    let conn = db.pool().get()?;
    repo::set(
        &conn,
        repo::KEY_LOG_LEVEL,
        parsed.label().to_lowercase().as_str(),
    )?;
    crate::logging::set_level(parsed);
    Ok(())
}

/// Reads a non-sensitive UI flag (one-time hints, dismissed explainer cards).
pub fn get_app_flag(db: &Db, key: String) -> AppResult<Option<String>> {
    validate_flag_key(&key)?;
    let conn = db.pool().get()?;
    repo::get(&conn, &key)
}

pub fn set_app_flag(db: &Db, key: String, value: String) -> AppResult<()> {
    validate_flag_key(&key)?;
    if key == crate::repository::agent::CONTEXT_IDLE_SETTING && !value.parse::<i64>().ok().is_some_and(|v| (1..=1440).contains(&v)) {
        return Err(AppError::validation("invalid_context_timeout", "Context timeout must be a whole number from 1 to 1440 minutes."));
    }
    if value.len() > 1024 {
        return Err(AppError::validation(
            "invalid_flag_value",
            "flag values are capped at 1024 characters",
        ));
    }
    let conn = db.pool().get()?;
    repo::set(&conn, &key, &value)
}

fn validate_flag_key(key: &str) -> AppResult<()> {
    if key.is_empty() || key.len() > 128 {
        return Err(AppError::validation(
            "invalid_flag_key",
            "flag keys must be 1..=128 characters",
        ));
    }
    Ok(())
}

pub fn set_week_start_day(db: &Db, day: i64) -> AppResult<()> {
    if !is_valid_week_start_day(day) {
        return Err(AppError::validation(
            "invalid_week_start_day",
            "week_start_day must be 1 (Monday) through 7 (Sunday)",
        ));
    }
    let conn = db.pool().get()?;
    repo::set(&conn, repo::KEY_WEEK_START_DAY, &day.to_string())
}

/// Reads `week_start_day`, persisting the default on first use so later reads
/// are stable. Must run inside the caller's transaction scope.
pub fn week_start_day_or_default(conn: &Connection) -> AppResult<i64> {
    if let Some(raw) = repo::get(conn, repo::KEY_WEEK_START_DAY)? {
        if let Ok(day) = raw.parse::<i64>() {
            if is_valid_week_start_day(day) {
                return Ok(day);
            }
        }
    }
    repo::set(
        conn,
        repo::KEY_WEEK_START_DAY,
        &DEFAULT_WEEK_START_DAY.to_string(),
    )?;
    Ok(DEFAULT_WEEK_START_DAY)
}

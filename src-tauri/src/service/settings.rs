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
}

pub fn get(db: &Db) -> AppResult<Settings> {
    let conn = db.pool().get()?;
    let week_start_day = repo::get(&conn, repo::KEY_WEEK_START_DAY)?
        .and_then(|v| v.parse::<i64>().ok())
        .filter(|v| is_valid_week_start_day(*v));
    Ok(Settings { week_start_day })
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
    repo::set(conn, repo::KEY_WEEK_START_DAY, &DEFAULT_WEEK_START_DAY.to_string())?;
    Ok(DEFAULT_WEEK_START_DAY)
}

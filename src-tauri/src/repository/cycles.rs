//! SQL for the `cycles` aggregate. One aggregate per module; cross-cycle
//! orchestration and event emission belong to `service`.

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::cycle::{Cycle, CycleType};
use crate::error::{from_rusqlite, AppError, AppResult};

pub fn row_to_cycle(row: &rusqlite::Row<'_>) -> rusqlite::Result<Cycle> {
    let type_str: String = row.get("type")?;
    Ok(Cycle {
        id: row.get("id")?,
        title: row.get("title")?,
        cycle_type: CycleType::parse(&type_str).unwrap_or(CycleType::Session),
        parent_id: row.get("parent_id")?,
        position: row.get("position")?,
        archived: row.get::<_, i64>("archived")? != 0,
        started: row.get::<_, i64>("started")? != 0,
        finished: row.get::<_, i64>("finished")? != 0,
        started_at: row.get("started_at")?,
        finished_at: row.get("finished_at")?,
        duration: row.get("duration")?,
        focused_time: row.get("focused_time")?,
        starts_on: row.get("starts_on")?,
        ends_on: row.get("ends_on")?,
        calendar_key: row.get("calendar_key")?,
        repeat_id: row.get("repeat_id")?,
        created_at: row.get("created_at")?,
    })
}

const CYCLE_COLUMNS: &str = "id, title, type, parent_id, position, archived, started, finished, \
     started_at, finished_at, duration, focused_time, starts_on, ends_on, calendar_key, \
     repeat_id, created_at";

pub struct NewCycle {
    pub id: String,
    pub title: String,
    pub cycle_type: CycleType,
    pub parent_id: Option<String>,
    pub position: i64,
    pub duration: Option<i64>,
    pub starts_on: Option<String>,
    pub ends_on: Option<String>,
    pub calendar_key: Option<String>,
    /// Set on sessions generated from a repeat template.
    pub repeat_id: Option<String>,
    pub created_at: i64,
}

pub fn insert(conn: &Connection, new: &NewCycle) -> AppResult<()> {
    conn.execute(
        "INSERT INTO cycles (id, title, type, parent_id, position, duration, starts_on, \
         ends_on, calendar_key, repeat_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            new.id,
            new.title,
            new.cycle_type.as_str(),
            new.parent_id,
            new.position,
            new.duration,
            new.starts_on,
            new.ends_on,
            new.calendar_key,
            new.repeat_id,
            new.created_at,
        ],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Option<Cycle>> {
    conn.query_row(
        &format!("SELECT {CYCLE_COLUMNS} FROM cycles WHERE id = ?1"),
        params![id],
        row_to_cycle,
    )
    .optional()
    .map_err(from_rusqlite)
}

pub fn require(conn: &Connection, id: &str) -> AppResult<Cycle> {
    get(conn, id)?.ok_or_else(|| AppError::not_found("cycle", id))
}

pub fn get_by_calendar_key(conn: &Connection, key: &str) -> AppResult<Option<Cycle>> {
    conn.query_row(
        &format!("SELECT {CYCLE_COLUMNS} FROM cycles WHERE calendar_key = ?1"),
        params![key],
        row_to_cycle,
    )
    .optional()
    .map_err(from_rusqlite)
}

/// Children of one cycle ordered for display (position first, then creation).
pub fn list_children(conn: &Connection, parent_id: &str) -> AppResult<Vec<Cycle>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {CYCLE_COLUMNS} FROM cycles WHERE parent_id = ?1 AND archived = 0 \
             ORDER BY position ASC, created_at ASC, id ASC"
        ))
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map(params![parent_id], row_to_cycle)
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(rows)
}

/// All non-archived month/week/day cycles — the planner's columns.
pub fn list_planner_cycles(conn: &Connection) -> AppResult<Vec<Cycle>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {CYCLE_COLUMNS} FROM cycles \
             WHERE archived = 0 AND type IN ('month','week','day') \
             ORDER BY created_at ASC, id ASC"
        ))
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map([], row_to_cycle)
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(rows)
}

/// Focus blocks of one day, in column order. Sessions are excluded from
/// `list_planner_cycles` so the planner payload stays column-shaped; the day
/// workspace fetches its own sessions through this query instead.
pub fn list_sessions_by_day(conn: &Connection, day_cycle_id: &str) -> AppResult<Vec<Cycle>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {CYCLE_COLUMNS} FROM cycles \
             WHERE archived = 0 AND type = 'session' AND parent_id = ?1 \
             ORDER BY position ASC, created_at ASC"
        ))
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map(params![day_cycle_id], row_to_cycle)
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(rows)
}

/// Ids of the cycle and all its descendants (the subtree a delete removes).
pub fn subtree_ids(conn: &Connection, id: &str) -> AppResult<Vec<String>> {
    let mut stmt = conn
        .prepare(
            "WITH RECURSIVE sub(id) AS (
                 SELECT id FROM cycles WHERE id = ?1
                 UNION ALL
                 SELECT c.id FROM cycles c JOIN sub s ON c.parent_id = s.id
             )
             SELECT id FROM sub",
        )
        .map_err(from_rusqlite)?;
    let ids = stmt
        .query_map(params![id], |r| r.get::<_, String>(0))
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(ids)
}

pub fn count_descendant_cycles(conn: &Connection, id: &str) -> AppResult<i64> {
    conn.query_row(
        "WITH RECURSIVE sub(id) AS (
             SELECT id FROM cycles WHERE parent_id = ?1
             UNION ALL
             SELECT c.id FROM cycles c JOIN sub s ON c.parent_id = s.id
         )
         SELECT COUNT(*) FROM sub",
        params![id],
        |r| r.get(0),
    )
    .map_err(from_rusqlite)
}

/// Sessions that already started, within `id` or any descendant.
pub fn count_started_sessions(conn: &Connection, id: &str) -> AppResult<i64> {
    conn.query_row(
        "WITH RECURSIVE sub(id) AS (
             SELECT id FROM cycles WHERE id = ?1
             UNION ALL
             SELECT c.id FROM cycles c JOIN sub s ON c.parent_id = s.id
         )
         SELECT COUNT(*) FROM cycles
         WHERE id IN (SELECT id FROM sub) AND type = 'session' AND started = 1",
        params![id],
        |r| r.get(0),
    )
    .map_err(from_rusqlite)
}

pub fn count_tasks(conn: &Connection, id: &str) -> AppResult<i64> {
    conn.query_row(
        "WITH RECURSIVE sub(id) AS (
             SELECT id FROM cycles WHERE id = ?1
             UNION ALL
             SELECT c.id FROM cycles c JOIN sub s ON c.parent_id = s.id
         )
         SELECT COUNT(*) FROM tasks WHERE cycle_id IN (SELECT id FROM sub)",
        params![id],
        |r| r.get(0),
    )
    .map_err(from_rusqlite)
}

/// How many non-archived cycles of the same type were created more recently
/// than `id` (ties broken by id). Basis of the `not_latest_n` deletion guard.
pub fn count_newer_same_type(conn: &Connection, cycle: &Cycle) -> AppResult<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM cycles \
         WHERE type = ?1 AND archived = 0 \
           AND (created_at > ?2 OR (created_at = ?2 AND id > ?3))",
        params![cycle.cycle_type.as_str(), cycle.created_at, cycle.id],
        |r| r.get(0),
    )
    .map_err(from_rusqlite)
}

/// The closest earlier dated sibling (same type, same parent) — "上一周期".
pub fn previous_dated_sibling(conn: &Connection, cycle: &Cycle) -> AppResult<Option<Cycle>> {
    let parent = cycle.parent_id.as_deref().unwrap_or("");
    let starts_on = cycle.starts_on.as_deref().unwrap_or("");
    conn.query_row(
        &format!(
            "SELECT {CYCLE_COLUMNS} FROM cycles \
             WHERE type = ?1 AND parent_id = ?2 AND starts_on IS NOT NULL AND starts_on < ?3 \
             ORDER BY starts_on DESC LIMIT 1"
        ),
        params![cycle.cycle_type.as_str(), parent, starts_on],
        row_to_cycle,
    )
    .optional()
    .map_err(from_rusqlite)
}

pub fn max_position(conn: &Connection, parent_id: &str) -> AppResult<i64> {
    conn.query_row(
        "SELECT COALESCE(MAX(position), -1) FROM cycles WHERE parent_id = ?1 AND archived = 0",
        params![parent_id],
        |r| r.get(0),
    )
    .map_err(from_rusqlite)
}

/// Applies a lifecycle state and its timestamp in one write.
pub fn set_lifecycle(
    conn: &Connection,
    id: &str,
    started: bool,
    finished: bool,
    started_at: Option<i64>,
    finished_at: Option<i64>,
) -> AppResult<()> {
    conn.execute(
        "UPDATE cycles SET started = ?2, finished = ?3, \
         started_at = COALESCE(?4, started_at), finished_at = COALESCE(?5, finished_at) \
         WHERE id = ?1",
        params![id, started as i64, finished as i64, started_at, finished_at],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

/// Adds milliseconds of focus time to one cycle.
pub fn add_focused_time(conn: &Connection, id: &str, delta_ms: i64) -> AppResult<i64> {
    conn.execute(
        "UPDATE cycles SET focused_time = focused_time + ?2 WHERE id = ?1",
        params![id, delta_ms],
    )
    .map_err(from_rusqlite)?;
    conn.query_row(
        "SELECT focused_time FROM cycles WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )
    .map_err(from_rusqlite)
}

/// Rewrites positions to the given id order (0..n).
pub fn reorder(conn: &Connection, ordered_ids: &[String]) -> AppResult<()> {
    for (index, id) in ordered_ids.iter().enumerate() {
        conn.execute(
            "UPDATE cycles SET position = ?2 WHERE id = ?1",
            params![id, index as i64],
        )
        .map_err(from_rusqlite)?;
    }
    Ok(())
}

pub fn update_session_fields(
    conn: &Connection,
    id: &str,
    title: &str,
    duration: Option<i64>,
) -> AppResult<()> {
    conn.execute(
        "UPDATE cycles SET title = ?2, duration = ?3 WHERE id = ?1",
        params![id, title, duration],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

pub fn set_repeat_id(conn: &Connection, cycle_id: &str, repeat_id: Option<&str>) -> AppResult<()> {
    conn.execute(
        "UPDATE cycles SET repeat_id = ?2 WHERE id = ?1",
        params![cycle_id, repeat_id],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

/// Every instance generated from a template.
pub fn list_by_repeat(conn: &Connection, repeat_id: &str) -> AppResult<Vec<Cycle>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {CYCLE_COLUMNS} FROM cycles WHERE repeat_id = ?1 ORDER BY created_at ASC"
        ))
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map(params![repeat_id], row_to_cycle)
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(rows)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM cycles WHERE id = ?1", params![id])
        .map_err(from_rusqlite)?;
    Ok(())
}

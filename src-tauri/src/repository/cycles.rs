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

/// The closest earlier date of the same type; week/day navigation is independent of goal containers.
pub fn previous_dated_sibling(conn: &Connection, cycle: &Cycle) -> AppResult<Option<Cycle>> {
    let parent = cycle.parent_id.as_deref();
    let starts_on = cycle.starts_on.as_deref().unwrap_or("");
    conn.query_row(
        &format!(
            "SELECT {CYCLE_COLUMNS} FROM cycles \
             WHERE type = ?1 AND (type IN ('week', 'day') OR parent_id IS ?2) AND archived = 0 AND starts_on IS NOT NULL AND starts_on < ?3 \
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

/// Removes historical focus time without allowing an inconsistent aggregate
/// to become negative when old data is repaired or a session is deleted.
pub fn subtract_focused_time(conn: &Connection, id: &str, delta_ms: i64) -> AppResult<i64> {
    conn.execute(
        "UPDATE cycles SET focused_time = MAX(0, focused_time - ?2) WHERE id = ?1",
        params![id, delta_ms.max(0)],
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

// --- calendar view (change: add-calendar-time-view) --------------------------
//
// Range reads, cross-date move helpers and the session-schedule column.
// These are appends only: the pre-existing functions above are untouched, so
// the calendar queries below deliberately re-state `CYCLE_COLUMNS`-shaped
// selects instead of editing them.

/// Day cycles whose date (`starts_on`) falls in the inclusive range, ordered
/// by date. The calendar grid maps `starts_on` 1:1 to its cell because a day
/// cycle's date identity is exactly its `starts_on`.
pub fn list_day_cycles_in_range(
    conn: &Connection,
    start_date: &str,
    end_date: &str,
) -> AppResult<Vec<Cycle>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {CYCLE_COLUMNS} FROM cycles \
             WHERE archived = 0 AND type = 'day' \
               AND starts_on IS NOT NULL AND starts_on >= ?1 AND starts_on <= ?2 \
             ORDER BY starts_on ASC, id ASC"
        ))
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map(params![start_date, end_date], row_to_cycle)
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(rows)
}

fn placeholder_list(len: usize) -> String {
    (1..=len)
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Focus blocks of many days in one query: `(parent day id, cycle)` pairs,
/// column order inside each day (`position, created_at, id`). Avoids the
/// per-day N queries the calendar would otherwise need.
pub fn list_sessions_by_days(
    conn: &Connection,
    day_ids: &[String],
) -> AppResult<Vec<(String, Cycle)>> {
    if day_ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT {CYCLE_COLUMNS} FROM cycles \
         WHERE archived = 0 AND type = 'session' AND parent_id IN ({}) \
         ORDER BY position ASC, created_at ASC, id ASC",
        placeholder_list(day_ids.len())
    );
    let mut stmt = conn.prepare(&sql).map_err(from_rusqlite)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(day_ids.iter()), |row| {
            Ok((row.get::<_, String>("parent_id")?, row_to_cycle(row)?))
        })
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(rows)
}

/// One row of `cycles.scheduled_start_at` (migration 0008) joined with the
/// duration it schedules. The schedule deliberately travels through this
/// dedicated projection instead of the `Cycle` struct: adding a field there
/// would force every existing constructor/row-mapper to change; the calendar
/// is the only reader today.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionScheduleRow {
    pub day_cycle_id: String,
    pub session_id: String,
    /// Epoch milliseconds; always present in this projection.
    pub starts_at: i64,
    /// Milliseconds; `None` falls back to zero length at overlap time.
    pub duration: Option<i64>,
}

const SCHEDULE_COLUMNS: &str = "parent_id, id, scheduled_start_at, COALESCE(duration, 0)";

fn row_to_schedule(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionScheduleRow> {
    Ok(SessionScheduleRow {
        day_cycle_id: row.get(0)?,
        session_id: row.get(1)?,
        starts_at: row.get(2)?,
        duration: Some(row.get(3)?),
    })
}

/// Scheduled focus blocks of one day, ordered by start time. Read-only; the
/// overlap detector and the day aggregate both consume this.
pub fn list_scheduled_in_day(
    conn: &Connection,
    day_cycle_id: &str,
) -> AppResult<Vec<SessionScheduleRow>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {SCHEDULE_COLUMNS} FROM cycles \
             WHERE archived = 0 AND type = 'session' AND parent_id = ?1 \
               AND scheduled_start_at IS NOT NULL \
             ORDER BY scheduled_start_at ASC, id ASC"
        ))
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map(params![day_cycle_id], row_to_schedule)
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(rows)
}

/// Scheduled focus blocks of many days in one query (range payload).
pub fn list_scheduled_in_days(
    conn: &Connection,
    day_ids: &[String],
) -> AppResult<Vec<SessionScheduleRow>> {
    if day_ids.is_empty() {
        return Ok(Vec::new());
    }
    let sql = format!(
        "SELECT {SCHEDULE_COLUMNS} FROM cycles \
         WHERE archived = 0 AND type = 'session' AND scheduled_start_at IS NOT NULL \
           AND parent_id IN ({}) \
         ORDER BY scheduled_start_at ASC, id ASC",
        placeholder_list(day_ids.len())
    );
    let mut stmt = conn.prepare(&sql).map_err(from_rusqlite)?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(day_ids.iter()), row_to_schedule)
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(rows)
}

/// Total scheduled duration (ms) of one day, counting every block that has a
/// schedule — including not-yet-started ones (spec: 含未开始已排).
pub fn sum_scheduled_duration(conn: &Connection, day_cycle_id: &str) -> AppResult<i64> {
    conn.query_row(
        "SELECT COALESCE(SUM(COALESCE(duration, 0)), 0) FROM cycles \
         WHERE archived = 0 AND type = 'session' AND parent_id = ?1 \
           AND scheduled_start_at IS NOT NULL",
        params![day_cycle_id],
        |r| r.get(0),
    )
    .map_err(from_rusqlite)
}

/// Writes a focus block's schedule. `scheduled_start_at` is set as given;
/// `duration` is only touched when `Some` (COALESCE semantics, mirroring
/// `set_lifecycle`), so callers that only move the start leave the duration.
pub fn set_session_schedule(
    conn: &Connection,
    session_id: &str,
    scheduled_start_at: Option<i64>,
    duration: Option<i64>,
) -> AppResult<()> {
    conn.execute(
        "UPDATE cycles SET scheduled_start_at = ?2, duration = COALESCE(?3, duration) \
         WHERE id = ?1",
        params![session_id, scheduled_start_at, duration],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

/// Sessions that started and have not finished, within `id` or any descendant.
/// Distinct from `count_started_sessions` (which also counts finished ones):
/// the cross-date merge must not discard a day with a *running* block, while
/// a merely historical started block is movable data.
pub fn count_running_sessions(conn: &Connection, id: &str) -> AppResult<i64> {
    conn.query_row(
        "WITH RECURSIVE sub(id) AS (
             SELECT id FROM cycles WHERE id = ?1
             UNION ALL
             SELECT c.id FROM cycles c JOIN sub s ON c.parent_id = s.id
         )
         SELECT COUNT(*) FROM cycles
         WHERE id IN (SELECT id FROM sub) AND type = 'session' \
           AND started = 1 AND finished = 0",
        params![id],
        |r| r.get(0),
    )
    .map_err(from_rusqlite)
}

/// Rewrites a day cycle's date identity. `None` clears the column — the swap
/// strategy relies on clearing both days first so the partial unique index
/// (`calendar_key IS NOT NULL`) never sees the collision.
pub fn set_day_identity(
    conn: &Connection,
    id: &str,
    calendar_key: Option<&str>,
    starts_on: Option<&str>,
    ends_on: Option<&str>,
) -> AppResult<()> {
    conn.execute(
        "UPDATE cycles SET calendar_key = ?2, starts_on = ?3, ends_on = ?4 WHERE id = ?1",
        params![id, calendar_key, starts_on, ends_on],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

pub fn set_title(conn: &Connection, id: &str, title: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE cycles SET title = ?2 WHERE id = ?1",
        params![id, title],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

/// Re-parents a day cycle (and appends it at the end of the new parent's
/// column) — used when a day takes over a date that lives in another week.
pub fn reparent(
    conn: &Connection,
    id: &str,
    parent_id: Option<&str>,
    position: i64,
) -> AppResult<()> {
    conn.execute(
        "UPDATE cycles SET parent_id = ?2, position = ?3 WHERE id = ?1",
        params![id, parent_id, position],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

/// Moves every focus block (including archived ones) of one day into another
/// day. Position renumbering of the visible blocks is the service layer's job
/// (`reorder`); this only re-parents so nothing is cascade-deleted with the
/// emptied source day. Returns the moved row count.
pub fn move_sessions_between_days(
    conn: &Connection,
    from_day_id: &str,
    to_day_id: &str,
) -> AppResult<u64> {
    conn.execute(
        "UPDATE cycles SET parent_id = ?2 WHERE parent_id = ?1 AND type = 'session'",
        params![from_day_id, to_day_id],
    )
    .map(|moved| moved as u64)
    .map_err(from_rusqlite)
}

/// Moves every task (and its pending-preview snapshots) of one cycle into
/// another, keeping the source's relative order: top-level tasks are offset
/// past the target's current top-level span, so merged tasks append after the
/// target's own (spec: 合并保持 position 顺序). Returns the moved row count.
pub fn move_tasks_between_cycles(
    conn: &Connection,
    from_cycle_id: &str,
    to_cycle_id: &str,
) -> AppResult<u64> {
    let offset: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM tasks \
         WHERE cycle_id = ?1 AND parent_id IS NULL",
            params![to_cycle_id],
            |r| r.get(0),
        )
        .map_err(from_rusqlite)?;
    conn.execute(
        "UPDATE tasks SET position = position + ?2 WHERE cycle_id = ?1 AND parent_id IS NULL",
        params![from_cycle_id, offset],
    )
    .map_err(from_rusqlite)?;
    let moved = conn
        .execute(
            "UPDATE tasks SET cycle_id = ?2 WHERE cycle_id = ?1",
            params![from_cycle_id, to_cycle_id],
        )
        .map_err(from_rusqlite)?;
    // Preview snapshots carry a denormalized cycle_id; keep the cache
    // pointing at the surviving cycle so pending proposals stay reviewable.
    conn.execute(
        "UPDATE task_preview_originals SET cycle_id = ?2 WHERE cycle_id = ?1",
        params![from_cycle_id, to_cycle_id],
    )
    .map_err(from_rusqlite)?;
    Ok(moved as u64)
}

/// Planning intervals intersecting an inclusive analysis range. Archive is
/// lifecycle state, not deletion: past plans remain evidence for retrospective analysis.
pub fn list_planning_cycles_overlapping(conn: &Connection, start: &str, end: &str) -> AppResult<Vec<Cycle>> {
    let mut stmt = conn.prepare(&format!("SELECT {CYCLE_COLUMNS} FROM cycles WHERE id != 'later' AND type != 'session' AND starts_on <= ?2 AND (ends_on > ?1 OR (ends_on IS NULL AND starts_on >= ?1)) ORDER BY starts_on, type, id")).map_err(from_rusqlite)?;
    let result = stmt.query_map(params![start, end], row_to_cycle).map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>().map_err(from_rusqlite)?;
    Ok(result)
}

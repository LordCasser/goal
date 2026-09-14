//! SQL for the `repeats` aggregate.

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::repeat::Repeat;
use crate::error::{from_rusqlite, AppError, AppResult};

fn row_to_repeat(row: &rusqlite::Row<'_>) -> rusqlite::Result<Repeat> {
    Ok(Repeat {
        id: row.get("id")?,
        title: row.get("title")?,
        duration: row.get("duration")?,
        position: row.get("position")?,
        archived: row.get::<_, i64>("archived")? != 0,
    })
}

pub struct NewRepeat {
    pub id: String,
    pub title: String,
    pub duration: i64,
    pub position: i64,
}

pub fn insert(conn: &Connection, new: &NewRepeat) -> AppResult<()> {
    conn.execute(
        "INSERT INTO repeats (id, title, duration, position) VALUES (?1, ?2, ?3, ?4)",
        params![new.id, new.title, new.duration, new.position],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Option<Repeat>> {
    conn.query_row(
        "SELECT id, title, duration, position, archived FROM repeats WHERE id = ?1",
        params![id],
        row_to_repeat,
    )
    .optional()
    .map_err(from_rusqlite)
}

pub fn require(conn: &Connection, id: &str) -> AppResult<Repeat> {
    get(conn, id)?.ok_or_else(|| AppError::not_found("repeat", id))
}

/// Active (non-archived) templates in display order.
pub fn list_active(conn: &Connection) -> AppResult<Vec<Repeat>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, duration, position, archived FROM repeats \
             WHERE archived = 0 ORDER BY position ASC, rowid ASC",
        )
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map([], row_to_repeat)
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(rows)
}

pub fn list_all(conn: &Connection) -> AppResult<Vec<Repeat>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, title, duration, position, archived FROM repeats \
             ORDER BY position ASC, rowid ASC",
        )
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map([], row_to_repeat)
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(rows)
}

pub fn max_position(conn: &Connection) -> AppResult<i64> {
    conn.query_row(
        "SELECT COALESCE(MAX(position), -1) FROM repeats",
        [],
        |r| r.get(0),
    )
    .map_err(from_rusqlite)
}

pub struct RepeatUpdate {
    pub title: Option<String>,
    pub duration: Option<i64>,
    pub position: Option<i64>,
}

impl RepeatUpdate {
    pub fn is_empty(&self) -> bool {
        self.title.is_none() && self.duration.is_none() && self.position.is_none()
    }
}

/// Edits the template row only — instances are never rewritten here
/// (spec: 修改只影响未来).
pub fn update(conn: &Connection, id: &str, update: &RepeatUpdate) -> AppResult<()> {
    if let Some(title) = &update.title {
        conn.execute(
            "UPDATE repeats SET title = ?2 WHERE id = ?1",
            params![id, title],
        )
        .map_err(from_rusqlite)?;
    }
    if let Some(duration) = update.duration {
        conn.execute(
            "UPDATE repeats SET duration = ?2 WHERE id = ?1",
            params![id, duration],
        )
        .map_err(from_rusqlite)?;
    }
    if let Some(position) = update.position {
        conn.execute(
            "UPDATE repeats SET position = ?2 WHERE id = ?1",
            params![id, position],
        )
        .map_err(from_rusqlite)?;
    }
    Ok(())
}

/// "Stop repeating": the template is archived, not deleted.
pub fn archive(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE repeats SET archived = 1 WHERE id = ?1",
        params![id],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

/// Removes the template row. Callers must unlink instances first; the FK's
/// ON DELETE SET NULL is only a safety net.
pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM repeats WHERE id = ?1", params![id]).map_err(from_rusqlite)?;
    Ok(())
}

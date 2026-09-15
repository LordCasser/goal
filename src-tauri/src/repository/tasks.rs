//! SQL for the `tasks` aggregate.

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::proposal::ProposalKind;
use crate::domain::task::{Subtask, Task, ROOT_COLOR_KEYS};
use crate::error::{from_rusqlite, AppError, AppResult};

const TASK_COLUMNS: &str = "id, cycle_id, parent_id, title, subtasks, position, completed, \
     goal_breakdown, needs_refinement, needs_breakdown, root_color_key, copied_from_task_id, \
     proposal, created_at";

fn row_to_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    let subtasks_json: String = row.get("subtasks")?;
    let proposal_str: Option<String> = row.get("proposal")?;
    Ok(Task {
        id: row.get("id")?,
        cycle_id: row.get("cycle_id")?,
        parent_id: row.get("parent_id")?,
        title: row.get("title")?,
        subtasks: serde_json::from_str(&subtasks_json).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                4, // ordinal of `subtasks` in TASK_COLUMNS
                rusqlite::types::Type::Text,
                Box::new(e),
            )
        })?,
        position: row.get("position")?,
        completed: row.get::<_, i64>("completed")? != 0,
        goal_breakdown: row
            .get::<_, Option<String>>("goal_breakdown")?
            .and_then(|s| serde_json::from_str(&s).ok()),
        needs_refinement: row
            .get::<_, Option<i64>>("needs_refinement")?
            .map(|v| v != 0),
        needs_breakdown: row
            .get::<_, Option<i64>>("needs_breakdown")?
            .map(|v| v != 0),
        root_color_key: row.get("root_color_key")?,
        copied_from_task_id: row.get("copied_from_task_id")?,
        proposal: proposal_str.as_deref().and_then(ProposalKind::parse),
        created_at: row.get("created_at")?,
    })
}

fn subtasks_to_json(subtasks: &[Subtask]) -> AppResult<String> {
    serde_json::to_string(subtasks).map_err(|e| AppError::Internal(e.to_string()))
}

pub struct NewTask {
    pub id: String,
    pub cycle_id: String,
    pub parent_id: Option<String>,
    pub title: String,
    pub subtasks: Vec<Subtask>,
    pub position: i64,
    pub completed: bool,
    pub goal_breakdown: Option<serde_json::Value>,
    pub needs_refinement: Option<bool>,
    pub needs_breakdown: Option<bool>,
    pub root_color_key: Option<String>,
    pub copied_from_task_id: Option<String>,
    pub created_at: i64,
}

pub fn insert(conn: &Connection, new: &NewTask) -> AppResult<()> {
    conn.execute(
        "INSERT INTO tasks (id, cycle_id, parent_id, title, subtasks, position, completed, \
         goal_breakdown, needs_refinement, needs_breakdown, root_color_key, \
         copied_from_task_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            new.id,
            new.cycle_id,
            new.parent_id,
            new.title,
            subtasks_to_json(&new.subtasks)?,
            new.position,
            new.completed as i64,
            new.goal_breakdown.as_ref().map(|v| v.to_string()),
            new.needs_refinement.map(|b| b as i64),
            new.needs_breakdown.map(|b| b as i64),
            new.root_color_key,
            new.copied_from_task_id,
            new.created_at,
        ],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Option<Task>> {
    conn.query_row(
        &format!("SELECT {TASK_COLUMNS} FROM tasks WHERE id = ?1"),
        params![id],
        row_to_task,
    )
    .optional()
    .map_err(from_rusqlite)
}

pub fn require(conn: &Connection, id: &str) -> AppResult<Task> {
    get(conn, id)?.ok_or_else(|| AppError::not_found("task", id))
}

/// Visible tasks of a cycle (committed data only, no pending proposals).
pub fn list_visible_by_cycle(conn: &Connection, cycle_id: &str) -> AppResult<Vec<Task>> {
    list_by_cycle_filtered(conn, cycle_id, "AND proposal IS NULL")
}

/// Every task of a cycle including pending agent proposals.
pub fn list_with_proposals_by_cycle(conn: &Connection, cycle_id: &str) -> AppResult<Vec<Task>> {
    list_by_cycle_filtered(conn, cycle_id, "")
}

fn list_by_cycle_filtered(conn: &Connection, cycle_id: &str, extra: &str) -> AppResult<Vec<Task>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {TASK_COLUMNS} FROM tasks WHERE cycle_id = ?1 {extra} \
             ORDER BY position ASC, created_at ASC, id ASC"
        ))
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map(params![cycle_id], row_to_task)
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(rows)
}

/// All pending proposals across cycles, with their cycle ids.
pub fn list_all_proposal_tasks(conn: &Connection) -> AppResult<Vec<Task>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {TASK_COLUMNS} FROM tasks WHERE proposal IS NOT NULL \
             ORDER BY created_at ASC, id ASC"
        ))
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map([], row_to_task)
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(rows)
}

/// The highest-position visible empty row — the one agent creation replaces.
pub fn last_empty_visible_row(conn: &Connection, cycle_id: &str) -> AppResult<Option<Task>> {
    conn.query_row(
        &format!(
            "SELECT {TASK_COLUMNS} FROM tasks \
             WHERE cycle_id = ?1 AND proposal IS NULL AND parent_id IS NULL \
               AND TRIM(title) = '' AND subtasks = '[]' AND completed = 0 \
             ORDER BY position DESC LIMIT 1"
        ),
        params![cycle_id],
        row_to_task,
    )
    .optional()
    .map_err(from_rusqlite)
}

pub fn max_position(conn: &Connection, cycle_id: &str, parent_id: Option<&str>) -> AppResult<i64> {
    conn.query_row(
        "SELECT COALESCE(MAX(position), -1) FROM tasks \
         WHERE cycle_id = ?1 AND parent_id IS ?2",
        params![cycle_id, parent_id],
        |r| r.get(0),
    )
    .map_err(from_rusqlite)
}

/// Pick the least-used palette key in a cycle. Ties follow the stable palette
/// order so successive roots spread across colors deterministically.
pub fn least_used_root_color(conn: &Connection, cycle_id: &str) -> AppResult<String> {
    let mut counts = [0_i64; ROOT_COLOR_KEYS.len()];
    let mut stmt = conn
        .prepare(
            "SELECT root_color_key, COUNT(*) FROM tasks \
             WHERE cycle_id = ?1 AND root_color_key IS NOT NULL \
             GROUP BY root_color_key",
        )
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map(params![cycle_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(from_rusqlite)?;
    for row in rows {
        let (key, count) = row.map_err(from_rusqlite)?;
        if let Some(index) = ROOT_COLOR_KEYS.iter().position(|candidate| *candidate == key) {
            counts[index] = count;
        }
    }
    let index = counts
        .iter()
        .enumerate()
        .min_by_key(|entry| *entry.1)
        .map(|(index, _)| index)
        .unwrap_or(0);
    Ok(ROOT_COLOR_KEYS[index].to_string())
}

pub fn count_by_cycle(conn: &Connection, cycle_id: &str) -> AppResult<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE cycle_id = ?1",
        params![cycle_id],
        |r| r.get(0),
    )
    .map_err(from_rusqlite)
}

pub struct TaskUpdate {
    pub title: Option<String>,
    pub subtasks: Option<Vec<Subtask>>,
    pub completed: Option<bool>,
    pub goal_breakdown: Option<Option<serde_json::Value>>,
    pub needs_refinement: Option<Option<bool>>,
    pub needs_breakdown: Option<Option<bool>>,
    pub parent_id: Option<Option<String>>,
    pub root_color_key: Option<Option<String>>,
    pub cycle_id: Option<String>,
    pub position: Option<i64>,
    pub proposal: Option<Option<ProposalKind>>,
    pub copied_from_task_id: Option<String>,
}

impl TaskUpdate {
    pub fn empty() -> Self {
        Self {
            title: None,
            subtasks: None,
            completed: None,
            goal_breakdown: None,
            needs_refinement: None,
            needs_breakdown: None,
            parent_id: None,
            root_color_key: None,
            cycle_id: None,
            position: None,
            proposal: None,
            copied_from_task_id: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.subtasks.is_none()
            && self.completed.is_none()
            && self.goal_breakdown.is_none()
            && self.needs_refinement.is_none()
            && self.needs_breakdown.is_none()
            && self.parent_id.is_none()
            && self.root_color_key.is_none()
            && self.cycle_id.is_none()
            && self.position.is_none()
            && self.proposal.is_none()
            && self.copied_from_task_id.is_none()
    }
}

/// Applies a partial update. Double options set SQL NULLs explicitly
/// (`goal_breakdown: Some(None)` clears the column).
pub fn update(conn: &Connection, id: &str, update: &TaskUpdate) -> AppResult<()> {
    let mut sets: Vec<&str> = Vec::new();
    let mut args: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut push = |column: &'static str, value: Box<dyn rusqlite::types::ToSql>| {
        args.push(value);
        sets.push(column);
    };

    if let Some(v) = &update.title {
        let v = v.clone();
        push("title", Box::new(v));
    }
    if let Some(v) = &update.subtasks {
        let v = subtasks_to_json(v)?;
        push("subtasks", Box::new(v));
    }
    if let Some(v) = update.completed {
        push("completed", Box::new(v as i64));
    }
    if let Some(v) = &update.goal_breakdown {
        let v = v.as_ref().map(|value| value.to_string());
        push("goal_breakdown", Box::new(v));
    }
    if let Some(v) = update.needs_refinement {
        push("needs_refinement", Box::new(v.map(|b| b as i64)));
    }
    if let Some(v) = update.needs_breakdown {
        push("needs_breakdown", Box::new(v.map(|b| b as i64)));
    }
    if let Some(v) = &update.parent_id {
        let v = v.clone();
        push("parent_id", Box::new(v));
    }
    if let Some(v) = &update.root_color_key {
        let v = v.clone();
        push("root_color_key", Box::new(v));
    }
    if let Some(v) = &update.cycle_id {
        let v = v.clone();
        push("cycle_id", Box::new(v));
    }
    if let Some(v) = update.position {
        push("position", Box::new(v));
    }
    if let Some(v) = update.proposal {
        push(
            "proposal",
            Box::new(v.map(|kind| kind.as_str().to_string())),
        );
    }
    if let Some(v) = &update.copied_from_task_id {
        let v = v.clone();
        push("copied_from_task_id", Box::new(v));
    }

    if sets.is_empty() {
        return Ok(());
    }
    let mut sql = String::from("UPDATE tasks SET ");
    for (i, column) in sets.iter().enumerate() {
        if i > 0 {
            sql.push_str(", ");
        }
        sql.push_str(column);
        sql.push_str(&format!(" = ?{}", i + 1));
    }
    sql.push_str(&format!(" WHERE id = ?{}", sets.len() + 1));

    let mut params_ref: Vec<&dyn rusqlite::types::ToSql> =
        args.iter().map(|b| b.as_ref()).collect();
    let id_param = id.to_string();
    params_ref.push(&id_param);
    conn.execute(&sql, params_ref.as_slice())
        .map_err(from_rusqlite)?;
    Ok(())
}

pub fn reorder(conn: &Connection, ordered_ids: &[String]) -> AppResult<()> {
    for (index, id) in ordered_ids.iter().enumerate() {
        conn.execute(
            "UPDATE tasks SET position = ?2 WHERE id = ?1",
            params![id, index as i64],
        )
        .map_err(from_rusqlite)?;
    }
    Ok(())
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM tasks WHERE id = ?1", params![id])
        .map_err(from_rusqlite)?;
    Ok(())
}

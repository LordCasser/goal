//! SQL for the `task_preview_originals` snapshot aggregate.
//!
//! A snapshot is written once — the first time a row enters preview mode — so
//! repeated agent edits never overwrite the true original
//! (spec: 原始值快照).

use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::proposal::TaskSnapshot;
use crate::domain::task::Task;
use crate::error::{from_rusqlite, AppResult};
use crate::repository::tasks;

pub fn get_snapshot(conn: &Connection, task_id: &str) -> AppResult<Option<TaskSnapshot>> {
    conn.query_row(
        "SELECT original_exists, title, completed, subtasks, position, goal_breakdown, \
         parent_id, root_color_key, needs_refinement, needs_breakdown, created_at \
         FROM task_preview_originals WHERE task_id = ?1",
        params![task_id],
        |row| {
            let original_exists: i64 = row.get("original_exists")?;
            let title: Option<String> = row.get("title")?;
            let completed: Option<i64> = row.get("completed")?;
            let subtasks: Option<String> = row.get("subtasks")?;
            let goal_breakdown: Option<String> = row.get("goal_breakdown")?;
            Ok(TaskSnapshot {
                original_exists: original_exists != 0,
                title,
                completed: completed.map(|v| v != 0),
                subtasks: subtasks.and_then(|s| serde_json::from_str(&s).ok()),
                position: row.get("position")?,
                goal_breakdown: goal_breakdown.and_then(|s| serde_json::from_str(&s).ok()),
                parent_id: row.get("parent_id")?,
                root_color_key: row.get("root_color_key")?,
                created_at: row.get("created_at")?,
                needs_refinement: row
                    .get::<_, Option<i64>>("needs_refinement")?
                    .map(|v| v != 0),
                needs_breakdown: row
                    .get::<_, Option<i64>>("needs_breakdown")?
                    .map(|v| v != 0),
            })
        },
    )
    .optional()
    .map_err(from_rusqlite)
}

pub fn save_snapshot(
    conn: &Connection,
    task_id: &str,
    cycle_id: &str,
    snapshot: &TaskSnapshot,
) -> AppResult<()> {
    let subtasks_json = match &snapshot.subtasks {
        Some(list) => Some(
            serde_json::to_string(list)
                .map_err(|e| crate::error::AppError::Internal(e.to_string()))?,
        ),
        None => None,
    };
    let goal_breakdown_json = snapshot.goal_breakdown.as_ref().map(|v| v.to_string());
    conn.execute(
        "INSERT OR IGNORE INTO task_preview_originals \
         (task_id, cycle_id, original_exists, title, completed, subtasks, position, \
         goal_breakdown, parent_id, root_color_key, needs_refinement, needs_breakdown, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, COALESCE(?13, unixepoch() * 1000))",
        params![
            task_id,
            cycle_id,
            snapshot.original_exists as i64,
            snapshot.title,
            snapshot.completed.map(|b| b as i64),
            subtasks_json,
            snapshot.position,
            goal_breakdown_json,
            snapshot.parent_id,
            snapshot.root_color_key,
            snapshot.needs_refinement.map(|b| b as i64),
            snapshot.needs_breakdown.map(|b| b as i64),
            snapshot.created_at,
        ],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

pub fn delete_snapshot(conn: &Connection, task_id: &str) -> AppResult<()> {
    conn.execute(
        "DELETE FROM task_preview_originals WHERE task_id = ?1",
        params![task_id],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

pub struct ProposalEntry {
    pub task: Task,
    pub snapshot: Option<TaskSnapshot>,
}

/// All pending proposals of one cycle with their snapshots (Keep/Undo all).
pub fn list_entries_by_cycle(conn: &Connection, cycle_id: &str) -> AppResult<Vec<ProposalEntry>> {
    let previews = tasks::list_with_proposals_by_cycle(conn, cycle_id)?;
    let mut entries = Vec::with_capacity(previews.len());
    for task in previews {
        let snapshot = get_snapshot(conn, &task.id)?;
        entries.push(ProposalEntry { task, snapshot });
    }
    Ok(entries)
}

pub fn count_by_cycle(conn: &Connection, cycle_id: &str) -> AppResult<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE cycle_id = ?1 AND proposal IS NOT NULL",
        params![cycle_id],
        |r| r.get(0),
    )
    .map_err(from_rusqlite)
}

/// Restores the row to its snapshotted state and clears the proposal marker.
pub fn restore(conn: &Connection, task_id: &str, snapshot: &TaskSnapshot) -> AppResult<()> {
    let mut update = tasks::TaskUpdate::empty();
    update.title = Some(snapshot.title.clone().unwrap_or_default());
    update.subtasks = Some(snapshot.subtasks.clone().unwrap_or_default());
    update.completed = Some(snapshot.completed.unwrap_or(false));
    update.goal_breakdown = Some(snapshot.goal_breakdown.clone());
    update.needs_refinement = Some(snapshot.needs_refinement);
    update.needs_breakdown = Some(snapshot.needs_breakdown);
    update.parent_id = Some(snapshot.parent_id.clone());
    update.root_color_key = Some(snapshot.root_color_key.clone());
    update.position = snapshot.position;
    update.proposal = Some(None);
    tasks::update(conn, task_id, &update)?;
    delete_snapshot(conn, task_id)?;
    Ok(())
}

//! Agent proposal use cases: preview writes, snapshots, Keep/Revert.
//!
//! Behaviour contract: `openspec/specs/agent-proposals/spec.md`.
//! Agent tools and UI confirmation share this preview contract.

use rusqlite::Connection;

use crate::db::Db;
use crate::domain::proposal::{ProposalKind, TaskSnapshot};
use crate::domain::task::{is_empty_input_row, Subtask, Task};
use crate::error::{AppError, AppResult};
use crate::repository::{cycles as cycles_repo, proposals as repo, tasks as tasks_repo};
use crate::service::Mutation;

/// The full proposed state of a task row, as the agent intends it.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct TaskInput {
    pub title: String,
    #[serde(default)]
    pub subtasks: Vec<Subtask>,
    #[serde(default)]
    pub completed: bool,
    pub goal_breakdown: Option<serde_json::Value>,
    pub needs_refinement: Option<bool>,
    pub needs_breakdown: Option<bool>,
    pub root_color_key: Option<String>,
    pub parent_id: Option<String>,
}

/// Structural page changes must not invalidate snapshots waiting in Coach.
pub(crate) fn ensure_cycle_tree_unlocked(conn: &Connection, cycle_id: &str) -> AppResult<()> {
    for id in cycles_repo::subtree_ids(conn, cycle_id)? {
        for task in tasks_repo::list_with_proposals_by_cycle(conn, &id)? {
            crate::service::tasks::ensure_task_editable(conn, &task.id, true)?;
        }
    }
    Ok(())
}

fn require_mutable_goal_cycle(conn: &Connection, cycle_id: &str) -> AppResult<()> {
    let cycle = cycles_repo::require(conn, cycle_id)?;
    if cycle.cycle_type == crate::domain::cycle::CycleType::Session {
        return Err(AppError::validation(
            "unsupported_cycle_type",
            "Focus blocks do not contain goals",
        ));
    }
    crate::service::cycles::ensure_cycle_mutable(&cycle)?;
    Ok(())
}

fn write_row(conn: &Connection, task_id: &str, input: &TaskInput) -> AppResult<()> {
    let mut update = tasks_repo::TaskUpdate::empty();
    update.title = Some(input.title.clone());
    update.subtasks = Some(input.subtasks.clone());
    update.completed = Some(input.completed);
    update.goal_breakdown = Some(input.goal_breakdown.clone());
    update.needs_refinement = Some(input.needs_refinement);
    update.needs_breakdown = Some(input.needs_breakdown);
    update.root_color_key = Some(input.root_color_key.clone());
    update.parent_id = Some(input.parent_id.clone());
    update.proposal = Some(Some(ProposalKind::Upsert));
    tasks_repo::update(conn, task_id, &update)?;
    Ok(())
}

/// Agent "create goal": if the cycle's list ends with an empty visible row,
/// that row is replaced instead of appended (spec: 新建目标复用空行).
/// The reused row is snapshotted so Undo all restores the empty row.
pub fn apply_upsert_preview(
    db: &Db,
    cycle_id: &str,
    input: &TaskInput,
    now: i64,
) -> AppResult<Mutation<Task>> {
    if input.title.trim().is_empty() {
        return Err(AppError::validation(
            "invalid_title",
            "A goal needs a title",
        ));
    }
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let cycle = cycles_repo::require(&tx, cycle_id)?;
    require_mutable_goal_cycle(&tx, cycle_id)?;
    crate::service::tasks::validate_root_color_key(
        cycle.cycle_type,
        input.root_color_key.as_deref(),
    )?;

    let task_id;
    match tasks_repo::last_empty_visible_row(&tx, cycle_id)? {
        Some(empty_row) if is_empty_input_row(&empty_row) => {
            repo::save_snapshot(
                &tx,
                &empty_row.id,
                cycle_id,
                &TaskSnapshot::capture(&empty_row),
            )?;
            task_id = empty_row.id;
        }
        _ => {
            let new = tasks_repo::NewTask {
                id: uuid::Uuid::new_v4().to_string(),
                cycle_id: cycle_id.to_string(),
                parent_id: None,
                title: String::new(),
                subtasks: Vec::new(),
                position: tasks_repo::max_position(&tx, cycle_id, None)? + 1,
                completed: false,
                goal_breakdown: None,
                needs_refinement: None,
                needs_breakdown: None,
                root_color_key: None,
                copied_from_task_id: None,
                created_at: now,
            };
            tasks_repo::insert(&tx, &new)?;
            repo::save_snapshot(&tx, &new.id, cycle_id, &TaskSnapshot::absent())?;
            task_id = new.id;
        }
    }
    let existing = tasks_repo::require(&tx, &task_id)?;
    let mut staged_input = input.clone();
    if staged_input.root_color_key.is_none() {
        staged_input.root_color_key = if staged_input.parent_id.is_some() {
            None
        } else if let Some(color) = existing.root_color_key {
            Some(color)
        } else {
            crate::service::tasks::default_root_color(&tx, cycle_id, cycle.cycle_type, None)?
        };
    }
    write_row(&tx, &task_id, &staged_input)?;
    let task = tasks_repo::require(&tx, &task_id)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(Mutation::new(task)
        .touching_tasks(cycle_id)
        .touching_proposals(cycle_id))
}

/// Agent "update goal": snapshot the current row once, then stage the edit.
pub fn apply_update_preview(
    db: &Db,
    task_id: &str,
    input: &TaskInput,
) -> AppResult<Mutation<Task>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let existing = tasks_repo::require(&tx, task_id)?;
    if existing.proposal == Some(ProposalKind::Delete) {
        return Err(AppError::conflict(
            "preview_conflict",
            "This task is already staged for deletion",
        ));
    }
    require_mutable_goal_cycle(&tx, &existing.cycle_id)?;
    let first_time = existing.proposal.is_none();
    if first_time {
        repo::save_snapshot(
            &tx,
            task_id,
            &existing.cycle_id,
            &TaskSnapshot::capture(&existing),
        )?;
    }
    write_row(&tx, task_id, input)?;
    let task = tasks_repo::require(&tx, task_id)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok({
        let cycle_id = existing.cycle_id.clone();
        Mutation::new(task)
            .touching_tasks(cycle_id.clone())
            .touching_proposals(cycle_id)
    })
}

/// Agent "delete goal": the row stays visible and revertible
/// (spec: 未确认的删除).
pub fn apply_delete_preview(db: &Db, task_id: &str) -> AppResult<Mutation<()>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let existing = tasks_repo::require(&tx, task_id)?;
    require_mutable_goal_cycle(&tx, &existing.cycle_id)?;
    if existing.proposal.is_none() {
        repo::save_snapshot(
            &tx,
            task_id,
            &existing.cycle_id,
            &TaskSnapshot::capture(&existing),
        )?;
    }
    let mut update = tasks_repo::TaskUpdate::empty();
    update.proposal = Some(Some(ProposalKind::Delete));
    tasks_repo::update(&tx, task_id, &update)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(Mutation::new(())
        .touching_tasks(existing.cycle_id.clone())
        .touching_proposals(existing.cycle_id))
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PreviewSummary {
    pub cycle_id: String,
    pub count: usize,
    /// Pending rows (with their proposed content) for the highlight pass.
    pub tasks: Vec<Task>,
    /// Original values make frontend confirmation reviewable, without a second
    /// proposal store or reconstruction from the model's prose.
    pub originals: std::collections::BTreeMap<String, TaskSnapshot>,
    pub deletion_impacts: std::collections::BTreeMap<String, Vec<String>>,
}

pub fn pending_cycle_ids(db: &Db) -> AppResult<Vec<String>> {
    let conn = db.pool().get()?;
    let mut stmt = conn
        .prepare("SELECT DISTINCT cycle_id FROM tasks WHERE proposal IS NOT NULL ORDER BY cycle_id")
        .map_err(|e| AppError::Db(e.to_string()))?;
    let ids = stmt
        .query_map([], |r| r.get(0))
        .map_err(|e| AppError::Db(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(ids)
}

pub fn get_preview_summary(db: &Db, cycle_id: &str) -> AppResult<PreviewSummary> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let tasks = tasks_repo::list_with_proposals_by_cycle(&tx, cycle_id)?
        .into_iter()
        .filter(|t| t.proposal.is_some())
        .collect::<Vec<_>>();
    let count = tasks.len();
    let mut originals = std::collections::BTreeMap::new();
    let mut deletion_impacts = std::collections::BTreeMap::new();
    for task in &tasks {
        if task.proposal == Some(ProposalKind::Delete) {
            let mut statement = tx.prepare("WITH RECURSIVE descendants(id) AS (
                SELECT id FROM tasks WHERE parent_id = ?1
                UNION SELECT t.id FROM tasks t JOIN descendants d ON t.parent_id = d.id
            ) SELECT title FROM tasks WHERE id IN (SELECT id FROM descendants) ORDER BY cycle_id, position")
                .map_err(|e| AppError::Db(e.to_string()))?;
            let mut titles = statement
                .query_map([&task.id], |row| row.get::<_, String>(0))
                .map_err(|e| AppError::Db(e.to_string()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| AppError::Db(e.to_string()))?;
            let impact = crate::service::deletion::task_impact(&tx, &task.id)?;
            for id in impact.cycle_ids {
                titles.push(cycles_repo::require(&tx, &id)?.title);
            }
            deletion_impacts.insert(task.id.clone(), titles);
        }
        if let Some(snapshot) = repo::get_snapshot(&tx, &task.id)? {
            originals.insert(task.id.clone(), snapshot);
        }
    }
    Ok(PreviewSummary {
        cycle_id: cycle_id.to_string(),
        count,
        tasks,
        originals,
        deletion_impacts,
    })
}

pub(crate) fn keep_one(
    conn: &Connection,
    task_id: &str,
    mutation: &mut Mutation<()>,
) -> AppResult<Option<Task>> {
    let task = match tasks_repo::get(conn, task_id)? {
        Some(t) => t,
        None => return Ok(None),
    };
    if task.proposal.is_none() {
        return Err(AppError::not_found("preview", task_id));
    }
    match task.proposal {
        // A confirmed proposed deletion is a user deletion as well.
        Some(ProposalKind::Delete) => {
            let mut update = tasks_repo::TaskUpdate::empty();
            update.proposal = Some(None);
            tasks_repo::update(conn, task_id, &update)?;
            repo::delete_snapshot(conn, task_id)?;
            let impact = crate::service::deletion::task_impact(conn, task_id)?;
            let cycle = crate::repository::cycles::require(conn, &task.cycle_id)?;
            crate::service::trash::archive_in_tx(conn, "task", task_id, &task.title, &cycle.title, &impact)?;
            mutation.trash_changed = true;
            mutation.merge(crate::service::deletion::prepare_deletion(conn, &impact)?);
            tasks_repo::delete(conn, task_id)?;
        }
        // A confirmed upsert becomes committed data; the snapshot is cleared.
        Some(ProposalKind::Upsert) | None => {
            let mut update = tasks_repo::TaskUpdate::empty();
            update.proposal = Some(None);
            tasks_repo::update(conn, task_id, &update)?;
            repo::delete_snapshot(conn, task_id)?;
            return Ok(Some(tasks_repo::require(conn, task_id)?));
        }
    }
    Ok(Some(task))
}

/// Keep: the staged content becomes committed data and the snapshot is cleared.
pub fn keep_task_preview(db: &Db, task_id: &str) -> AppResult<Mutation<Task>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let mut affected = Mutation::new(());
    let task = keep_one(&tx, task_id, &mut affected)?
        .ok_or_else(|| AppError::not_found("task", task_id))?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    let cycle_id = task.cycle_id.clone();
    let mut mutation = Mutation::new(task)
        .touching_tasks(cycle_id.clone())
        .touching_proposals(cycle_id);
    mutation.merge(affected);
    Ok(mutation)
}

pub(crate) fn undo_one(conn: &Connection, task_id: &str) -> AppResult<Option<String>> {
    let snapshot = repo::get_snapshot(conn, task_id)?;
    let task = tasks_repo::get(conn, task_id)?;
    let cycle_id = task.as_ref().map(|t| t.cycle_id.clone());
    match (snapshot, task) {
        (None, None) => Ok(None),
        (None, Some(task)) => {
            // Proposal marker without a snapshot cannot be reverted precisely;
            // at minimum clear the marker so the row is not stuck.
            if task.proposal.is_some() {
                let mut update = tasks_repo::TaskUpdate::empty();
                update.proposal = Some(None);
                tasks_repo::update(conn, task_id, &update)?;
            }
            Ok(cycle_id)
        }
        (Some(snapshot), _) => {
            if snapshot.original_exists {
                // Restore the exact pre-proposal content and clear the marker.
                repo::restore(conn, task_id, &snapshot)?;
            } else {
                // The row did not exist before the agent: revert removes it.
                // The snapshot row follows via FK cascade.
                tasks_repo::delete(conn, task_id)?;
            }
            Ok(cycle_id)
        }
    }
}

pub fn undo_task_preview(db: &Db, task_id: &str) -> AppResult<Mutation<()>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let cycle_id = undo_one(&tx, task_id)?.ok_or_else(|| AppError::not_found("task", task_id))?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(Mutation::new(())
        .touching_tasks(cycle_id.clone())
        .touching_proposals(cycle_id))
}

pub fn keep_all_previews(db: &Db, cycle_id: &str) -> AppResult<Mutation<usize>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let entries = repo::list_entries_by_cycle(&tx, cycle_id)?;
    let mut kept = 0;
    let mut affected = Mutation::new(());
    for entry in entries {
        if entry.task.proposal.is_some() {
            keep_one(&tx, &entry.task.id, &mut affected)?;
            kept += 1;
        }
    }
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    let mut mutation = Mutation::new(kept)
        .touching_tasks(cycle_id)
        .touching_proposals(cycle_id);
    mutation.merge(affected);
    Ok(mutation)
}

pub fn undo_all_previews(db: &Db, cycle_id: &str) -> AppResult<Mutation<usize>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let entries = repo::list_entries_by_cycle(&tx, cycle_id)?;
    let mut undone = 0;
    for entry in entries {
        if entry.task.proposal.is_some() {
            undo_one(&tx, &entry.task.id)?;
            undone += 1;
        }
    }
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(Mutation::new(undone)
        .touching_tasks(cycle_id)
        .touching_proposals(cycle_id))
}

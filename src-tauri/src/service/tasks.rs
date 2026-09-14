//! Task use cases: add/edit/delete, moving between cycles, cross-level links,
//! root coloring. Behaviour contract: `openspec/specs/task-graph/spec.md`.

use rusqlite::Connection;

use crate::db::Db;
use crate::domain::cycle::CycleType;
use crate::domain::task::{is_valid_root_color_key, Subtask, Task};
use crate::error::{AppError, AppResult};
use crate::repository::{cycles as cycles_repo, tasks as repo};
use crate::service::Mutation;

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct AddTaskArgs {
    pub cycle_id: String,
    pub title: String,
    /// Subtask row this new row hangs under. Must live in the same cycle.
    pub parent_id: Option<String>,
    pub position: Option<i64>,
    pub subtasks: Option<Vec<Subtask>>,
    /// Manual goals default to "needs refinement" (spec: 清晰度标记).
    /// Pass `Some` to override.
    pub needs_refinement: Option<bool>,
    pub needs_breakdown: Option<bool>,
}

/// Whether manually created rows in this cycle type start out as "goals to
/// clarify". Long-term goals (including Later ideas) do; week/day items are
/// plain work items.
fn clarity_defaults(cycle_type: CycleType) -> (Option<bool>, Option<bool>) {
    match cycle_type {
        CycleType::Month => (Some(true), Some(true)),
        _ => (None, None),
    }
}

pub fn add_task(db: &Db, args: &AddTaskArgs, now: i64) -> AppResult<Mutation<Task>> {
    let mut conn = db.pool().get()?;
    let tx = conn.transaction().map_err(|e| AppError::Db(e.to_string()))?;
    let cycle = cycles_repo::require(&tx, &args.cycle_id)?;
    if cycle.cycle_type == CycleType::Session {
        return Err(AppError::validation(
            "unsupported_cycle_type",
            "Focus blocks do not contain tasks",
        ));
    }
    crate::service::cycles::ensure_cycle_mutable(&cycle)?;
    if let Some(parent) = &args.parent_id {
        let parent_task = repo::require(&tx, parent)?;
        if parent_task.cycle_id != args.cycle_id {
            return Err(AppError::validation(
                "invalid_parent_link",
                "A subtask must live in the same cycle as its parent",
            ));
        }
    }
    let (default_refine, default_breakdown) = clarity_defaults(cycle.cycle_type);
    let position = match args.position {
        Some(p) => p,
        None => repo::max_position(&tx, &args.cycle_id, args.parent_id.as_deref())? + 1,
    };
    let new = repo::NewTask {
        id: uuid::Uuid::new_v4().to_string(),
        cycle_id: args.cycle_id.clone(),
        parent_id: args.parent_id.clone(),
        title: args.title.clone(),
        subtasks: args.subtasks.clone().unwrap_or_default(),
        position,
        completed: false,
        goal_breakdown: None,
        needs_refinement: args.needs_refinement.or(default_refine),
        needs_breakdown: args.needs_breakdown.or(default_breakdown),
        root_color_key: None,
        copied_from_task_id: None,
        created_at: now,
    };
    repo::insert(&tx, &new)?;
    let task = repo::require(&tx, &new.id)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(Mutation::new(task).touching_tasks(args.cycle_id.clone()))
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub subtasks: Option<Vec<Subtask>>,
    pub completed: Option<bool>,
    pub goal_breakdown: Option<serde_json::Value>,
    /// `Some(Some(false))` records "evaluated, fine" — distinct from absent.
    pub needs_refinement: Option<Option<bool>>,
    pub needs_breakdown: Option<Option<bool>>,
}

fn validate_patch(patch: &TaskPatch) -> AppResult<()> {
    if let Some(title) = &patch.title {
        if title.trim().is_empty() {
            return Err(AppError::validation("invalid_title", "Title cannot be empty"));
        }
    }
    Ok(())
}

fn apply_patch(conn: &Connection, task_id: &str, patch: &TaskPatch) -> AppResult<Task> {
    let mut update = repo::TaskUpdate::empty();
    update.title = patch.title.clone();
    update.subtasks = patch.subtasks.clone();
    update.completed = patch.completed;
    update.goal_breakdown = patch.goal_breakdown.clone().map(Some);
    update.needs_refinement = patch.needs_refinement;
    update.needs_breakdown = patch.needs_breakdown;
    repo::update(conn, task_id, &update)?;
    repo::require(conn, task_id)
}

pub fn patch_task(db: &Db, task_id: &str, patch: &TaskPatch) -> AppResult<Mutation<Task>> {
    validate_patch(patch)?;
    let mut conn = db.pool().get()?;
    let tx = conn.transaction().map_err(|e| AppError::Db(e.to_string()))?;
    let existing = repo::require(&tx, task_id)?;
    let cycle = crate::service::cycles::ensure_content_mutable(&tx, &existing.cycle_id)?;
    let task = apply_patch(&tx, task_id, patch)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(Mutation::new(task).touching_tasks(cycle.id))
}

/// Full editor save: title is required, everything else optional.
pub fn update_task(
    db: &Db,
    task_id: &str,
    title: String,
    patch: &TaskPatch,
) -> AppResult<Mutation<Task>> {
    let full = TaskPatch {
        title: Some(title),
        ..patch.clone()
    };
    patch_task(db, task_id, &full)
}

pub fn delete_task(db: &Db, task_id: &str) -> AppResult<Mutation<()>> {
    let mut conn = db.pool().get()?;
    let tx = conn.transaction().map_err(|e| AppError::Db(e.to_string()))?;
    let existing = repo::require(&tx, task_id)?;
    let cycle = crate::service::cycles::ensure_content_mutable(&tx, &existing.cycle_id)?;
    // Children rows and preview snapshots go with the row (FK cascades).
    repo::delete(&tx, task_id)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(Mutation::new(()).touching_tasks(cycle.id))
}

pub fn move_task(
    db: &Db,
    task_id: &str,
    target_cycle_id: &str,
    position: Option<i64>,
) -> AppResult<Mutation<Task>> {
    let mut conn = db.pool().get()?;
    let tx = conn.transaction().map_err(|e| AppError::Db(e.to_string()))?;
    let existing = repo::require(&tx, task_id)?;
    crate::service::cycles::ensure_content_mutable(&tx, &existing.cycle_id)?;
    let target = cycles_repo::require(&tx, target_cycle_id)?;
    if target.cycle_type == CycleType::Session {
        return Err(AppError::validation(
            "unsupported_cycle_type",
            "Tasks cannot move into a focus block",
        ));
    }
    crate::service::cycles::ensure_cycle_mutable(&target)?;
    // Colored goals must stay inside long-term cycles; the trigger enforces
    // this too, but the service reports the stable code deterministically.
    if existing.root_color_key.is_some() && target.cycle_type != CycleType::Month {
        return Err(AppError::conflict(
            "root_color_key_requires_long_term_cycle",
            "Colors can only be used on goals inside a Long-term cycle.",
        ));
    }

    let parent_for_position = existing.parent_id.clone();
    let new_position = match position {
        Some(p) => p,
        None => {
            repo::max_position(&tx, target_cycle_id, parent_for_position.as_deref())? + 1
        }
    };

    // The move carries same-cycle descendant rows along so a goal keeps its
    // breakdown; cross-cycle links stay untouched.
    let subtree = task_subtree_ids(&tx, task_id)?;
    for id in &subtree {
        let mut update = repo::TaskUpdate::empty();
        update.cycle_id = Some(target_cycle_id.to_string());
        repo::update(&tx, id, &update)?;
    }
    let mut update = repo::TaskUpdate::empty();
    update.cycle_id = Some(target_cycle_id.to_string());
    update.position = Some(new_position);
    repo::update(&tx, task_id, &update)?;

    let task = repo::require(&tx, task_id)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    let mut mutation = Mutation::new(task).touching_tasks(target_cycle_id);
    mutation.tasks.push(existing.cycle_id);
    Ok(mutation)
}

/// The task plus its descendant rows (same-cycle subtask tree).
fn task_subtree_ids(conn: &Connection, task_id: &str) -> AppResult<Vec<String>> {
    let mut stmt = conn
        .prepare(
            "WITH RECURSIVE sub(id) AS (
                 SELECT id FROM tasks WHERE id = ?1
                 UNION ALL
                 SELECT t.id FROM tasks t JOIN sub s ON t.parent_id = s.id
             )
             SELECT id FROM sub",
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    let ids = stmt
        .query_map([task_id], |r| r.get::<_, String>(0))
        .map_err(|e| AppError::Db(e.to_string()))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(ids)
}

pub fn reorder_tasks(
    db: &Db,
    cycle_id: &str,
    parent_id: Option<&str>,
    ordered_ids: &[String],
) -> AppResult<Mutation<()>> {
    let mut conn = db.pool().get()?;
    let tx = conn.transaction().map_err(|e| AppError::Db(e.to_string()))?;
    crate::service::cycles::ensure_content_mutable(&tx, cycle_id)?;
    for id in ordered_ids {
        let task = repo::require(&tx, id)?;
        if task.cycle_id != cycle_id || task.parent_id.as_deref() != parent_id {
            return Err(AppError::validation(
                "task_not_in_group",
                format!("task {id} does not belong to this sibling group"),
            ));
        }
    }
    repo::reorder(&tx, ordered_ids)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(Mutation::new(()).touching_tasks(cycle_id))
}

/// Cross-level link (weekly item -> long-term goal, daily task -> weekly item).
/// Links are only allowed between adjacent levels within the same branch
/// (spec: 跨层目标链接); `None` unlinks.
pub fn set_task_parent_link(
    db: &Db,
    task_id: &str,
    parent_id: Option<&str>,
) -> AppResult<Mutation<Task>> {
    let mut conn = db.pool().get()?;
    let tx = conn.transaction().map_err(|e| AppError::Db(e.to_string()))?;
    let task = repo::require(&tx, task_id)?;
    let cycle = crate::service::cycles::ensure_content_mutable(&tx, &task.cycle_id)?;

    if let Some(parent_id) = parent_id {
        if parent_id == task_id {
            return Err(AppError::validation(
                "link_to_self",
                "Parent link cannot point to the same task",
            ));
        }
        let parent_task = repo::require(&tx, parent_id)?;
        let parent_cycle = cycles_repo::require(&tx, &parent_task.cycle_id)?;

        // Same-cycle nesting (subtask rows) is free-form; the adjacency rule
        // only governs cross-level links between cycles.
        if parent_cycle.id != cycle.id {
            let adjacent = match (cycle.cycle_type, parent_cycle.cycle_type) {
                (CycleType::Week, CycleType::Month) => true,
                (CycleType::Day, CycleType::Week) => true,
                _ => false,
            };
            if !adjacent {
                return Err(AppError::validation(
                    "link_level_not_adjacent",
                    "Links only work between neighbouring levels (weekly -> long-term, daily -> weekly)",
                ));
            }
            // The link must stay inside one branch of the cycle tree.
            let mut ancestor = cycle.parent_id.clone();
            let mut in_branch = false;
            while let Some(id) = ancestor {
                if id == parent_cycle.id {
                    in_branch = true;
                    break;
                }
                ancestor = cycles_repo::get(&tx, &id)?.and_then(|c| c.parent_id);
            }
            if !in_branch {
                return Err(AppError::validation(
                    "link_cycle_mismatch",
                    "The linked goal must sit above this cycle",
                ));
            }
        }
    }

    let mut update = repo::TaskUpdate::empty();
    update.parent_id = Some(parent_id.map(|p| p.to_string()));
    repo::update(&tx, task_id, &update)?;
    let task = repo::require(&tx, task_id)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    let cycle_id = task.cycle_id.clone();
    Ok(Mutation::new(task).touching_tasks(cycle_id))
}

/// Palette coloring for long-term goals. Clearing (`None`) is always allowed.
pub fn set_task_root_color(
    db: &Db,
    task_id: &str,
    color_key: Option<&str>,
) -> AppResult<Mutation<Task>> {
    let mut conn = db.pool().get()?;
    let tx = conn.transaction().map_err(|e| AppError::Db(e.to_string()))?;
    let task = repo::require(&tx, task_id)?;
    let cycle = crate::service::cycles::ensure_content_mutable(&tx, &task.cycle_id)?;
    if let Some(color) = color_key {
        if !is_valid_root_color_key(color) {
            return Err(AppError::validation(
                "unknown_color_key",
                format!("unknown color key: {color}"),
            ));
        }
        if cycle.cycle_type != CycleType::Month {
            return Err(AppError::conflict(
                "root_color_key_requires_long_term_cycle",
                "Colors can only be used on goals inside a Long-term cycle.",
            ));
        }
    }
    let mut update = repo::TaskUpdate::empty();
    update.root_color_key = Some(color_key.map(|c| c.to_string()));
    repo::update(&tx, task_id, &update)?;
    let task = repo::require(&tx, task_id)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    let cycle_id = task.cycle_id.clone();
    Ok(Mutation::new(task).touching_tasks(cycle_id))
}

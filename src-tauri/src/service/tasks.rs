//! Task use cases: add/edit/delete, moving between cycles, cross-level links,
//! root coloring. Behaviour contract: `openspec/specs/task-graph/spec.md`.

use rusqlite::Connection;

use crate::db::Db;
use crate::domain::cycle::{CycleType, LATER_CYCLE_ID};
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
    /// An explicit color wins over the long-term default allocator.
    pub root_color_key: Option<String>,
    /// Manual goals default to "needs refinement" (spec: 清晰度标记).
    /// Pass `Some` to override.
    pub needs_refinement: Option<bool>,
    pub needs_breakdown: Option<bool>,
}

pub(crate) fn validate_root_color_key(
    cycle_type: CycleType,
    color_key: Option<&str>,
) -> AppResult<()> {
    let Some(color) = color_key else {
        return Ok(());
    };
    if !is_valid_root_color_key(color) {
        return Err(AppError::validation(
            "unknown_color_key",
            format!("unknown color key: {color}"),
        ));
    }
    if cycle_type != CycleType::Month {
        return Err(AppError::conflict(
            "root_color_key_requires_long_term_cycle",
            "Colors can only be used on goals inside a Long-term cycle.",
        ));
    }
    Ok(())
}

/// Allocate a color only for a new root goal in a real long-term cycle.
/// Substeps, week/day rows, and the Later holding container stay uncolored.
pub(crate) fn default_root_color(
    conn: &Connection,
    cycle_id: &str,
    cycle_type: CycleType,
    parent_id: Option<&str>,
) -> AppResult<Option<String>> {
    if cycle_type != CycleType::Month || cycle_id == LATER_CYCLE_ID || parent_id.is_some() {
        return Ok(None);
    }
    Ok(Some(repo::least_used_root_color(conn, cycle_id)?))
}

/// Preview locks belong to task identity, so all editor entry points obey them.
/// A deletion also affects linked descendants through the task foreign key.
pub(crate) fn ensure_task_editable(
    conn: &Connection,
    task_id: &str,
    subtree: bool,
) -> AppResult<()> {
    let locked: bool = conn.query_row(
        "WITH RECURSIVE ancestors(id, parent_id, proposal) AS (
            SELECT id, parent_id, proposal FROM tasks WHERE id = ?1
            UNION SELECT t.id, t.parent_id, t.proposal FROM tasks t JOIN ancestors a ON t.id = a.parent_id
         ), descendants(id, proposal) AS (
            SELECT id, proposal FROM tasks WHERE id = ?1
            UNION SELECT t.id, t.proposal FROM tasks t JOIN descendants d ON t.parent_id = d.id
         ) SELECT EXISTS(SELECT 1 FROM ancestors WHERE (id = ?1 AND proposal IS NOT NULL) OR proposal = 'delete')
             OR (?2 AND EXISTS(SELECT 1 FROM descendants WHERE proposal IS NOT NULL))",
        rusqlite::params![task_id, subtree], |row| row.get(0),
    ).map_err(|e| AppError::Db(e.to_string()))?;
    if locked {
        return Err(AppError::conflict(
            "task_preview_locked",
            "请先在 Coach 中确认或放弃这项改动，再编辑受影响的任务。",
        ));
    }
    Ok(())
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
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let cycle = cycles_repo::require(&tx, &args.cycle_id)?;
    if cycle.cycle_type == CycleType::Session {
        return Err(AppError::validation(
            "unsupported_cycle_type",
            "Focus blocks do not contain tasks",
        ));
    }
    crate::service::cycles::ensure_cycle_mutable(&cycle)?;
    validate_root_color_key(cycle.cycle_type, args.root_color_key.as_deref())?;
    if let Some(parent) = &args.parent_id {
        let parent_task = repo::require(&tx, parent)?;
        ensure_task_editable(&tx, parent, false)?;
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
        None => match args.parent_id.as_deref() {
            Some(parent_id) => repo::max_position(&tx, &args.cycle_id, Some(parent_id))? + 1,
            None => repo::max_visible_root_position(&tx, &args.cycle_id)? + 1,
        },
    };
    let root_color_key = match args.root_color_key.as_deref() {
        Some(color) => Some(color.to_string()),
        None => default_root_color(
            &tx,
            &args.cycle_id,
            cycle.cycle_type,
            args.parent_id.as_deref(),
        )?,
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
        root_color_key,
        copied_from_task_id: None,
        created_at: now,
    };
    repo::insert(&tx, &new)?;
    if args.cycle_id == LATER_CYCLE_ID {
        let kind = match args.parent_id.as_deref() {
            Some(id) => repo::require(&tx, id)?.later_plan_type.unwrap_or(CycleType::Month),
            None => CycleType::Month,
        };
        let mut update = repo::TaskUpdate::empty();
        update.later_plan_type = Some(Some(kind));
        repo::update(&tx, &new.id, &update)?;
    }
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
            return Err(AppError::validation(
                "invalid_title",
                "Title cannot be empty",
            ));
        }
    }
    Ok(())
}

fn apply_patch(
    conn: &Connection,
    task_id: &str,
    patch: &TaskPatch,
    auto_root_color: Option<String>,
) -> AppResult<Task> {
    let mut update = repo::TaskUpdate::empty();
    update.title = patch.title.clone();
    update.subtasks = patch.subtasks.clone();
    update.completed = patch.completed;
    update.goal_breakdown = patch.goal_breakdown.clone().map(Some);
    update.needs_refinement = patch.needs_refinement;
    update.needs_breakdown = patch.needs_breakdown;
    update.root_color_key = auto_root_color.map(Some);
    repo::update(conn, task_id, &update)?;
    repo::require(conn, task_id)
}

pub fn patch_task(db: &Db, task_id: &str, patch: &TaskPatch) -> AppResult<Mutation<Task>> {
    validate_patch(patch)?;
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let existing = repo::require(&tx, task_id)?;
    ensure_task_editable(&tx, task_id, false)?;
    let cycle = crate::service::cycles::ensure_content_mutable(&tx, &existing.cycle_id)?;
    let auto_root_color = if existing.title.trim().is_empty()
        && existing.root_color_key.is_none()
        && patch
            .title
            .as_deref()
            .is_some_and(|title| !title.trim().is_empty())
        && existing.parent_id.is_none()
    {
        default_root_color(
            &tx,
            &existing.cycle_id,
            cycle.cycle_type,
            existing.parent_id.as_deref(),
        )?
    } else {
        None
    };
    let task = apply_patch(&tx, task_id, patch, auto_root_color)?;
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

#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskDeletionPreview {
    pub task_id: String,
    pub descendant_tasks: i64,
    pub total_focus_blocks: i64,
    pub started_focus_count: i64,
    pub confirmation_token: String,
}

pub fn get_task_deletion_preview(db: &Db, task_id: &str) -> AppResult<TaskDeletionPreview> {
    let conn = db.pool().get()?;
    repo::require(&conn, task_id)?;
    let impact = crate::service::deletion::task_impact(&conn, task_id)?;
    Ok(TaskDeletionPreview {
        task_id: task_id.to_string(),
        descendant_tasks: impact.task_ids.len().saturating_sub(1) as i64,
        total_focus_blocks: impact.total_focus_blocks,
        started_focus_count: impact.started_focus_count,
        confirmation_token: impact.token,
    })
}

pub fn delete_task(db: &Db, task_id: &str) -> AppResult<Mutation<()>> {
    delete_task_inner(db, task_id, None, false)
}

/// GUI deletion entry point.  A leaf can be removed directly; a task with
/// descendants must carry the token from its preview.  The impact is
/// recomputed inside the delete transaction.
pub fn delete_task_confirmed(
    db: &Db,
    task_id: &str,
    confirmation_token: Option<&str>,
) -> AppResult<Mutation<()>> {
    delete_task_inner(db, task_id, confirmation_token, true)
}

fn delete_task_inner(
    db: &Db,
    task_id: &str,
    confirmation_token: Option<&str>,
    gui_confirmation: bool,
) -> AppResult<Mutation<()>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let existing = repo::require(&tx, task_id)?;
    ensure_task_editable(&tx, task_id, true)?;
    let impact = crate::service::deletion::task_impact(&tx, task_id)?;
    let has_dependents = impact.task_ids.len() > 1 || !impact.cycle_ids.is_empty();
    crate::service::deletion::require_confirmation(
        confirmation_token,
        &impact.token,
        gui_confirmation && has_dependents,
    )?;
    let cycle = crate::service::cycles::ensure_content_mutable(&tx, &existing.cycle_id)?;
    let mut mutation = crate::service::deletion::prepare_deletion(&tx, &impact)?;
    // Task descendants and linked focus blocks follow through FK cascades.
    repo::delete(&tx, task_id)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    // Keep the target cycle in the invalidation set even if a future schema
    // permits a malformed task row without an owning cycle.
    mutation.tasks.push(cycle.id);
    Ok(mutation)
}

pub fn move_task(
    db: &Db,
    task_id: &str,
    target_cycle_id: &str,
    position: Option<i64>,
) -> AppResult<Mutation<Task>> {
    let mut conn = db.pool().get()?;
    let tx = conn.transaction().map_err(|e| AppError::Db(e.to_string()))?;
    let mutation = move_task_in_tx(&tx, task_id, target_cycle_id, position)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(mutation)
}

fn move_task_in_tx(
    conn: &Connection,
    task_id: &str,
    target_cycle_id: &str,
    position: Option<i64>,
) -> AppResult<Mutation<Task>> {
    let existing = repo::require(conn, task_id)?;
    ensure_task_editable(conn, task_id, true)?;
    let source = crate::service::cycles::ensure_content_mutable(conn, &existing.cycle_id)?;
    let target = cycles_repo::require(conn, target_cycle_id)?;
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

    let mut parent_for_position = existing.parent_id.clone();
    if existing.cycle_id == LATER_CYCLE_ID && target_cycle_id != LATER_CYCLE_ID {
        if let Some(parent_id) = existing.parent_id.as_deref() {
            let parent = repo::require(conn, parent_id)?;
            let parent_cycle = cycles_repo::require(conn, &parent.cycle_id)?;
            let valid = parent_cycle.id != LATER_CYCLE_ID
                && (parent_cycle.id == target_cycle_id
                    || can_link_task_levels(target.cycle_type, parent_cycle.cycle_type));
            if !valid { parent_for_position = None; }
        }
    }
    let new_position = match position {
        Some(p) => p,
        None => repo::max_position(conn, target_cycle_id, parent_for_position.as_deref())? + 1,
    };

    // The move carries same-cycle descendant rows along so a goal keeps its
    // breakdown; cross-cycle links stay untouched.
    let subtree = task_subtree_ids(conn, task_id)?;
    let mut unlinked_sessions = Vec::new();
    for id in &subtree {
        if existing.cycle_id != target_cycle_id {
            unlinked_sessions.extend(cycles_repo::unlink_task(conn, id)?);
        }
        let mut update = repo::TaskUpdate::empty();
        update.cycle_id = Some(target_cycle_id.to_string());
        if target_cycle_id != LATER_CYCLE_ID {
            update.later_plan_type = Some(None);
        } else if existing.cycle_id != LATER_CYCLE_ID {
            update.later_plan_type = Some(Some(source.cycle_type));
        }
        repo::update(conn, id, &update)?;
    }
    let mut update = repo::TaskUpdate::empty();
    update.cycle_id = Some(target_cycle_id.to_string());
    update.position = Some(new_position);
    update.parent_id = Some(parent_for_position);
    repo::update(conn, task_id, &update)?;

    let task = repo::require(conn, task_id)?;
    let mut mutation = Mutation::new(task).touching_tasks(target_cycle_id);
    if !unlinked_sessions.is_empty() {
        mutation.cycles.push(&existing.cycle_id);
        for id in unlinked_sessions {
            mutation.cycles.push(id);
        }
    }
    mutation.tasks.push(existing.cycle_id);
    Ok(mutation)
}

/// Arrange a parked task at its recorded level. Resolve calendar identities and
/// move inside one transaction so a failed move cannot leave an empty plan.
pub fn promote_later_task(
    db: &Db,
    task_id: &str,
    target_cycle_id: Option<&str>,
    today: chrono::NaiveDate,
    now: i64,
) -> AppResult<Mutation<Task>> {
    let mut conn = db.pool().get()?;
    let tx = conn.transaction().map_err(|e| AppError::Db(e.to_string()))?;
    let existing = repo::require(&tx, task_id)?;
    if existing.cycle_id != LATER_CYCLE_ID {
        return Err(AppError::conflict("later_source_required", "This item is no longer in Later."));
    }
    ensure_task_editable(&tx, task_id, true)?;
    let kind = existing.later_plan_type.unwrap_or(CycleType::Month);
    let target = if let Some(id) = target_cycle_id {
        cycles_repo::require(&tx, id)?
    } else {
        let week_start = crate::service::settings::week_start_day_or_default(&tx)? as u32;
        match kind {
            CycleType::Week => crate::service::cycles::get_or_create_week_in_tx(&tx, today, week_start, now)?,
            CycleType::Day => crate::service::cycles::get_or_create_day_in_tx(&tx, today, week_start, now)?,
            _ => return Err(AppError::validation("later_target_required", "Choose a long-term cycle for this item.")),
        }
    };
    if target.id == LATER_CYCLE_ID || target.cycle_type != kind {
        return Err(AppError::validation("later_target_type_mismatch", "Choose a plan matching this item's planning level."));
    }
    if target.archived {
        return Err(AppError::conflict("later_target_unavailable", "The target plan is archived. Restore it before arranging this item."));
    }
    let mut mutation = move_task_in_tx(&tx, task_id, &target.id, None)?;
    mutation.cycles.push(&target.id);
    if kind == CycleType::Day {
        // Match opening a day: only explicitly enabled daily repeats generate.
        crate::service::repeats::generate_for_day_in_tx(&tx, &target.id, now)?;
        if let Some(parent) = target.parent_id { mutation.cycles.push(parent); }
    }
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(mutation)
}

/// The task plus its same-level descendant rows. Later shares storage across
/// levels, so cross-level links there must not turn into movable substeps.
fn task_subtree_ids(conn: &Connection, task_id: &str) -> AppResult<Vec<String>> {
    let mut stmt = conn
        .prepare(
            "WITH RECURSIVE sub(id) AS (
                 SELECT id FROM tasks WHERE id = ?1
                 UNION ALL
                 SELECT t.id FROM tasks t JOIN sub s ON t.parent_id = s.id
                 WHERE t.cycle_id = (SELECT cycle_id FROM tasks WHERE id = ?1)
                   AND (t.cycle_id != 'later' OR COALESCE(t.later_plan_type, 'month') =
                        (SELECT COALESCE(later_plan_type, 'month') FROM tasks WHERE id = ?1))
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

/// The editor renders a task as a visible root when its parent is outside the
/// currently loaded cycle. Reordering that root group must therefore use the
/// same-cycle tree relationship rather than the stored cross-cycle link.
fn task_belongs_to_reorder_group(
    conn: &Connection,
    task: &Task,
    cycle_id: &str,
    parent_id: Option<&str>,
) -> AppResult<bool> {
    if task.cycle_id != cycle_id {
        return Ok(false);
    }

    match parent_id {
        Some(expected_parent_id) => {
            if task.parent_id.as_deref() != Some(expected_parent_id) {
                return Ok(false);
            }
            Ok(repo::get(conn, expected_parent_id)?
                .is_some_and(|parent| parent.cycle_id == cycle_id))
        }
        None => match task.parent_id.as_deref() {
            None => Ok(true),
            Some(stored_parent_id) => Ok(repo::get(conn, stored_parent_id)?
                .is_some_and(|parent| parent.cycle_id != cycle_id)),
        },
    }
}

pub fn reorder_tasks(
    db: &Db,
    cycle_id: &str,
    parent_id: Option<&str>,
    ordered_ids: &[String],
) -> AppResult<Mutation<()>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    crate::service::cycles::ensure_content_mutable(&tx, cycle_id)?;
    for id in ordered_ids {
        let task = repo::require(&tx, id)?;
        ensure_task_editable(&tx, id, false)?;
        if !task_belongs_to_reorder_group(&tx, &task, cycle_id, parent_id)? {
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

fn can_link_task_levels(child: CycleType, parent: CycleType) -> bool {
    matches!(
        (child, parent),
        (CycleType::Week, CycleType::Month)
            | (CycleType::Day, CycleType::Week | CycleType::Month)
    )
}

/// Cross-level link (weekly -> long-term; daily -> weekly or long-term).
/// Links are optional and independent of cycle parents
/// (spec: 跨层目标链接); `None` unlinks.
pub fn set_task_parent_link(
    db: &Db,
    task_id: &str,
    parent_id: Option<&str>,
) -> AppResult<Mutation<Task>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let task = repo::require(&tx, task_id)?;
    ensure_task_editable(&tx, task_id, true)?;
    let cycle = crate::service::cycles::ensure_content_mutable(&tx, &task.cycle_id)?;

    let mut update = repo::TaskUpdate::empty();
    if let Some(parent_id) = parent_id {
        if parent_id == task_id {
            return Err(AppError::validation(
                "link_to_self",
                "Parent link cannot point to the same task",
            ));
        }
        let parent_task = repo::require(&tx, parent_id)?;
        ensure_task_editable(&tx, parent_id, false)?;
        let parent_cycle = cycles_repo::require(&tx, &parent_task.cycle_id)?;
        if parent_cycle.id == cycle.id && task.root_color_key.is_some() {
            update.root_color_key = Some(None);
        }

        // Same-cycle nesting (subtask rows) is free-form; planning levels
        // only constrain cross-cycle ownership.
        if parent_cycle.id != cycle.id {
            if !can_link_task_levels(cycle.cycle_type, parent_cycle.cycle_type) {
                return Err(AppError::validation(
                    "invalid_link_level",
                    "Weekly tasks can link to long-term goals; daily tasks can link to weekly or long-term goals.",
                ));
            }
            if parent_cycle.id == crate::domain::cycle::LATER_CYCLE_ID {
                return Err(AppError::validation(
                    "link_cycle_mismatch",
                    "Move a Later idea into a long-term plan before linking it",
                ));
            }
        }
    }

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
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let task = repo::require(&tx, task_id)?;
    ensure_task_editable(&tx, task_id, false)?;
    let cycle = crate::service::cycles::ensure_content_mutable(&tx, &task.cycle_id)?;
    validate_root_color_key(cycle.cycle_type, color_key)?;
    let mut update = repo::TaskUpdate::empty();
    update.root_color_key = Some(color_key.map(|c| c.to_string()));
    repo::update(&tx, task_id, &update)?;
    let task = repo::require(&tx, task_id)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    let cycle_id = task.cycle_id.clone();
    Ok(Mutation::new(task).touching_tasks(cycle_id))
}

//! Editor workspace use cases: the backend-authored editing content for one
//! or many cycles (spec: `task-graph` — 任务内容的编辑态).

use std::collections::{BTreeMap, HashSet};

use crate::db::Db;
use crate::domain::cycle::{Cycle, CycleType, LATER_CYCLE_ID};
use crate::domain::task::{build_tree, is_empty_input_row, Task, TaskNode};
use crate::error::AppResult;
use crate::repository::{cycles as cycles_repo, tasks as tasks_repo};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EditorWorkspace {
    /// `None` when the cycle does not exist or is not visible — callers
    /// render an empty state instead of an error (spec: 未加载的周期).
    pub cycle: Option<Cycle>,
    pub tasks: Vec<TaskNode>,
    pub work_mix: Option<WorkMix>,
}

/// A current snapshot of top-level commitments, not a time or productivity score.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct WorkMix {
    pub total: usize,
    pub long_term: usize,
    pub weekly_standalone: usize,
    pub daily_standalone: usize,
    pub unresolved: usize,
}

/// Reused by the editor and AI context so both report the same denominator.
pub fn work_mix(
    conn: &rusqlite::Connection,
    cycle: &Cycle,
    tasks: &[Task],
) -> AppResult<Option<WorkMix>> {
    if !matches!(cycle.cycle_type, CycleType::Week | CycleType::Day) {
        return Ok(None);
    }
    let ids: HashSet<&str> = tasks.iter().map(|task| task.id.as_str()).collect();
    let mut mix = WorkMix::default();
    for task in tasks.iter().filter(|task| {
        task.proposal.is_none()
            && (!is_empty_input_row(task)
                || tasks
                    .iter()
                    .any(|child| child.parent_id.as_deref() == Some(task.id.as_str())))
            && !task.parent_id.as_deref().is_some_and(|id| ids.contains(id))
    }) {
        mix.total += 1;
        let mut weekly = cycle.cycle_type == CycleType::Week;
        let mut long_term = false;
        let mut unresolved = false;
        let mut seen = HashSet::from([task.id.clone()]);
        let mut parent_id = task.parent_id.clone();
        while let Some(id) = parent_id {
            if !seen.insert(id.clone()) {
                unresolved = true;
                break;
            }
            let Some(parent) = tasks_repo::get(conn, &id)? else {
                unresolved = true;
                break;
            };
            let Some(parent_cycle) = cycles_repo::get(conn, &parent.cycle_id)? else {
                unresolved = true;
                break;
            };
            if parent.proposal.is_some() || parent_cycle.id == LATER_CYCLE_ID {
                unresolved = true;
                break;
            }
            if parent_cycle.cycle_type == CycleType::Month {
                long_term = true;
                break;
            }
            weekly |= parent_cycle.cycle_type == CycleType::Week;
            parent_id = parent.parent_id;
        }
        if unresolved {
            mix.unresolved += 1;
        } else if long_term {
            mix.long_term += 1;
        } else if weekly {
            mix.weekly_standalone += 1;
        } else {
            mix.daily_standalone += 1;
        }
    }
    Ok(Some(mix))
}

fn workspace_for(conn: &rusqlite::Connection, cycle_id: &str) -> AppResult<EditorWorkspace> {
    let cycle = cycles_repo::get(conn, cycle_id)?;
    let cycle = match cycle {
        Some(cycle) if !cycle.archived => cycle,
        _ => {
            return Ok(EditorWorkspace {
                cycle: None,
                tasks: Vec::new(),
                work_mix: None,
            })
        }
    };
    let mut tasks = tasks_repo::list_with_proposals_by_cycle(conn, cycle_id)?;
    // An ancestor deletion cascades through links, even across planning levels.
    // Project that effect into the editor without creating extra proposals.
    let mut statement = conn
        .prepare(
            "WITH RECURSIVE affected(id) AS (
        SELECT id FROM tasks WHERE proposal = 'delete'
        UNION SELECT t.id FROM tasks t JOIN affected a ON t.parent_id = a.id
    ) SELECT id FROM affected",
        )
        .map_err(|e| crate::error::AppError::Db(e.to_string()))?;
    let deleting = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| crate::error::AppError::Db(e.to_string()))?
        .collect::<Result<HashSet<_>, _>>()
        .map_err(|e| crate::error::AppError::Db(e.to_string()))?;
    for task in &mut tasks {
        if deleting.contains(&task.id) {
            task.proposal = Some(crate::domain::proposal::ProposalKind::Delete);
        }
    }
    Ok(EditorWorkspace {
        work_mix: work_mix(conn, &cycle, &tasks)?,
        cycle: Some(cycle),
        tasks: build_tree(tasks),
    })
}

pub fn get_editor_workspace(db: &Db, cycle_id: &str) -> AppResult<EditorWorkspace> {
    let conn = db.pool().get()?;
    workspace_for(&conn, cycle_id)
}

/// Batch form: results keyed by the requested cycle id; missing cycles map to
/// an empty workspace so one request can cover every open column.
pub fn get_editor_workspaces_by_cycle_ids(
    db: &Db,
    cycle_ids: &[String],
) -> AppResult<BTreeMap<String, EditorWorkspace>> {
    let conn = db.pool().get()?;
    let mut out = BTreeMap::new();
    for id in cycle_ids {
        out.insert(id.clone(), workspace_for(&conn, id)?);
    }
    Ok(out)
}

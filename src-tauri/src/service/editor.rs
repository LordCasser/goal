//! Editor workspace use cases: the backend-authored editing content for one
//! or many cycles (spec: `task-graph` — 任务内容的编辑态).

use std::collections::BTreeMap;

use crate::db::Db;
use crate::domain::cycle::Cycle;
use crate::domain::task::{build_tree, TaskNode};
use crate::error::AppResult;
use crate::repository::{cycles as cycles_repo, tasks as tasks_repo};

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EditorWorkspace {
    /// `None` when the cycle does not exist or is not visible — callers
    /// render an empty state instead of an error (spec: 未加载的周期).
    pub cycle: Option<Cycle>,
    pub tasks: Vec<TaskNode>,
}

fn workspace_for(conn: &rusqlite::Connection, cycle_id: &str) -> AppResult<EditorWorkspace> {
    let cycle = cycles_repo::get(conn, cycle_id)?;
    let cycle = match cycle {
        Some(cycle) if !cycle.archived => cycle,
        _ => {
            return Ok(EditorWorkspace {
                cycle: None,
                tasks: Vec::new(),
            })
        }
    };
    let tasks = tasks_repo::list_visible_by_cycle(conn, cycle_id)?;
    Ok(EditorWorkspace {
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

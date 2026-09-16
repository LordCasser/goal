//! Authoritative deletion impact calculation.
//!
//! The task and cycle trees are separate.  A cycle deletion first removes
//! every task in its cycle subtree, and the task foreign key then cascades to
//! descendants that may live in another cycle.  This module computes that
//! same closure before either a preview or a confirmed mutation and derives a
//! stable token from the current rows so the GUI cannot confirm stale impact.

use std::collections::BTreeSet;

use rusqlite::Connection;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};
use crate::repository::{cycles as cycles_repo, tasks as tasks_repo};

#[derive(Debug, Clone)]
pub struct DeletionImpact {
    /// Cycles deleted by either the cycle tree or a linked task cascade.
    pub cycle_ids: Vec<String>,
    /// The complete task FK cascade, including the root task.
    pub task_ids: Vec<String>,
    /// Every cycle containing one of `task_ids`, used for cache invalidation.
    pub task_cycle_ids: Vec<String>,
    pub descendant_cycles: i64,
    pub total_focus_blocks: i64,
    pub started_focus_count: i64,
    pub token: String,
}

#[derive(Debug, Serialize)]
struct TokenCycle {
    id: String,
    title: String,
    cycle_type: String,
    parent_id: Option<String>,
    position: i64,
    started: bool,
    finished: bool,
    started_at: Option<i64>,
    finished_at: Option<i64>,
    focused_time: i64,
    task_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct TokenTask {
    id: String,
    cycle_id: String,
    parent_id: Option<String>,
    title: String,
    position: i64,
    completed: bool,
    proposal: Option<&'static str>,
    created_at: i64,
}

#[derive(Debug, Serialize)]
struct TokenPayload {
    kind: &'static str,
    target_id: String,
    cycles: Vec<TokenCycle>,
    tasks: Vec<TokenTask>,
}

/// Computes the rows removed by deleting one task.
pub fn task_impact(conn: &Connection, task_id: &str) -> AppResult<DeletionImpact> {
    let task_ids = descendant_task_ids(conn, task_id)?;
    let task_cycle_ids = task_cycle_ids(conn, &task_ids)?;
    let cycle_ids = linked_session_ids(conn, &task_ids)?;
    let mut token_cycle_ids = task_cycle_ids.clone();
    token_cycle_ids.extend(cycle_ids.iter().cloned());
    token_cycle_ids.sort();
    token_cycle_ids.dedup();
    let cycles = cycle_records(conn, &token_cycle_ids)?;
    let started_focus_count = cycles
        .iter()
        .filter(|c| c.cycle_type == "session" && c.started)
        .count() as i64;
    let tasks = task_records(conn, &task_ids)?;
    let token = token("task", task_id, cycles, tasks);
    Ok(DeletionImpact {
        total_focus_blocks: cycle_ids.len() as i64,
        cycle_ids,
        descendant_cycles: 0,
        started_focus_count,
        task_cycle_ids,
        task_ids,
        token,
    })
}

/// Computes the rows removed by deleting one cycle.  Task descendants are
/// followed after selecting the cycle subtree, matching SQLite's
/// `tasks.parent_id ON DELETE CASCADE` semantics even across cycle boundaries.
pub fn cycle_impact(conn: &Connection, cycle_id: &str) -> AppResult<DeletionImpact> {
    let mut cycle_ids = cycles_repo::subtree_ids(conn, cycle_id)?;
    cycle_ids.sort();
    cycle_ids.dedup();
    let task_ids = task_ids_for_cycles(conn, &cycle_ids)?;
    let task_cycle_ids = task_cycle_ids(conn, &task_ids)?;

    let descendant_cycles = cycle_ids.len().saturating_sub(1) as i64;
    cycle_ids.extend(linked_session_ids(conn, &task_ids)?);
    cycle_ids.sort();
    cycle_ids.dedup();

    let mut token_cycle_ids = cycle_ids.clone();
    token_cycle_ids.extend(task_cycle_ids.iter().cloned());
    token_cycle_ids.sort();
    token_cycle_ids.dedup();
    let cycles = cycle_records(conn, &token_cycle_ids)?;
    let total_focus_count = cycles
        .iter()
        .filter(|cycle| cycle.cycle_type == "session")
        .count() as i64;
    let started_focus_count = cycles
        .iter()
        .filter(|cycle| cycle.cycle_type == "session" && cycle.started)
        .count() as i64;
    let tasks = task_records(conn, &task_ids)?;
    let token = token("cycle", cycle_id, cycles, tasks);

    Ok(DeletionImpact {
        descendant_cycles,
        cycle_ids,
        task_ids,
        task_cycle_ids,
        total_focus_blocks: total_focus_count,
        started_focus_count,
        token,
    })
}

/// Undo the removed records' contribution to surviving containers before FK
/// cascades erase their ancestry. All deletion entry points share this path.
pub(crate) fn prepare_deletion(
    conn: &Connection,
    impact: &DeletionImpact,
) -> AppResult<crate::service::Mutation<()>> {
    let deleted: BTreeSet<&str> = impact.cycle_ids.iter().map(String::as_str).collect();
    let mut mutation = crate::service::Mutation::new(());
    for id in &impact.cycle_ids {
        let cycle = cycles_repo::require(conn, id)?;
        mutation.cycles.push(id);
        if cycle.cycle_type == crate::domain::cycle::CycleType::Session {
            if cycle.task_id.is_some() {
                if let Some(day) = &cycle.parent_id {
                    mutation.tasks.push(day);
                }
            }
            let mut parent = cycle.parent_id;
            while let Some(id) = parent {
                let ancestor = cycles_repo::require(conn, &id)?;
                if !deleted.contains(id.as_str()) {
                    if cycle.focused_time > 0 {
                        cycles_repo::subtract_focused_time(conn, &id, cycle.focused_time)?;
                    }
                    mutation.cycles.push(&id);
                }
                parent = ancestor.parent_id;
            }
        } else if let Some(parent) = cycle.parent_id {
            if !deleted.contains(parent.as_str()) {
                mutation.cycles.push(parent);
            }
        }
    }
    for id in &impact.task_cycle_ids {
        mutation.tasks.push(id);
    }
    crate::service::reminders::purge_for_cycle_impact(conn, &impact.cycle_ids, &impact.task_ids)?;
    Ok(mutation)
}

fn linked_session_ids(conn: &Connection, task_ids: &[String]) -> AppResult<Vec<String>> {
    let mut statement = conn
        .prepare("SELECT id FROM cycles WHERE task_id = ?1 AND type = 'session' ORDER BY id")
        .map_err(crate::error::from_rusqlite)?;
    let mut ids = BTreeSet::new();
    for task_id in task_ids {
        let rows = statement
            .query_map([task_id], |row| row.get::<_, String>(0))
            .map_err(crate::error::from_rusqlite)?;
        for row in rows {
            ids.insert(row.map_err(crate::error::from_rusqlite)?);
        }
    }
    Ok(ids.into_iter().collect())
}

/// Validate a GUI confirmation token.  Trusted internal callers use the
/// service wrappers and intentionally bypass this check.
pub fn require_confirmation(supplied: Option<&str>, actual: &str, required: bool) -> AppResult<()> {
    match supplied {
        Some(token) if token == actual => Ok(()),
        Some(_) => Err(AppError::conflict(
            "deletion_impact_changed",
            "The deletion impact changed; review it again before confirming.",
        )),
        None if required => Err(AppError::conflict(
            "deletion_confirmation_required",
            "Review the deletion impact before confirming.",
        )),
        None => Ok(()),
    }
}

fn descendant_task_ids(conn: &Connection, task_id: &str) -> AppResult<Vec<String>> {
    let mut stmt = conn
        .prepare(
            "WITH RECURSIVE descendants(id) AS (
                 SELECT id FROM tasks WHERE id = ?1
                 UNION
                 SELECT t.id FROM tasks t JOIN descendants d ON t.parent_id = d.id
             )
             SELECT id FROM descendants ORDER BY id",
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
    let ids = stmt
        .query_map([task_id], |row| row.get::<_, String>(0))
        .map_err(|e| AppError::Db(e.to_string()))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(ids)
}

fn task_ids_for_cycles(conn: &Connection, cycle_ids: &[String]) -> AppResult<Vec<String>> {
    if cycle_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = std::iter::repeat("?")
        .take(cycle_ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "WITH RECURSIVE seeds(id) AS (
             SELECT id FROM tasks WHERE cycle_id IN ({placeholders})
         ), descendants(id) AS (
             SELECT id FROM seeds
             UNION
             SELECT t.id FROM tasks t JOIN descendants d ON t.parent_id = d.id
         )
         SELECT DISTINCT id FROM descendants ORDER BY id"
    );
    let values = cycle_ids.iter().map(|id| id.as_str()).collect::<Vec<_>>();
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| AppError::Db(e.to_string()))?;
    let ids = stmt
        .query_map(rusqlite::params_from_iter(values), |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| AppError::Db(e.to_string()))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|e| AppError::Db(e.to_string()))?;
    Ok(ids)
}

fn task_cycle_ids(conn: &Connection, task_ids: &[String]) -> AppResult<Vec<String>> {
    let mut ids = BTreeSet::new();
    for task_id in task_ids {
        if let Some(task) = tasks_repo::get(conn, task_id)? {
            ids.insert(task.cycle_id);
        }
    }
    Ok(ids.into_iter().collect())
}

fn cycle_records(conn: &Connection, cycle_ids: &[String]) -> AppResult<Vec<TokenCycle>> {
    let mut records = Vec::with_capacity(cycle_ids.len());
    for id in cycle_ids {
        let cycle = cycles_repo::require(conn, id)?;
        records.push(TokenCycle {
            id: cycle.id,
            title: cycle.title,
            cycle_type: cycle.cycle_type.as_str().to_string(),
            parent_id: cycle.parent_id,
            position: cycle.position,
            started: cycle.started,
            finished: cycle.finished,
            started_at: cycle.started_at,
            finished_at: cycle.finished_at,
            focused_time: cycle.focused_time,
            task_id: cycle.task_id,
        });
    }
    Ok(records)
}

fn task_records(conn: &Connection, task_ids: &[String]) -> AppResult<Vec<TokenTask>> {
    let mut records = Vec::with_capacity(task_ids.len());
    for id in task_ids {
        let task = tasks_repo::require(conn, id)?;
        records.push(TokenTask {
            id: task.id,
            cycle_id: task.cycle_id,
            parent_id: task.parent_id,
            title: task.title,
            position: task.position,
            completed: task.completed,
            proposal: task.proposal.map(|proposal| proposal.as_str()),
            created_at: task.created_at,
        });
    }
    Ok(records)
}

fn token(
    kind: &'static str,
    target_id: &str,
    cycles: Vec<TokenCycle>,
    tasks: Vec<TokenTask>,
) -> String {
    let payload = TokenPayload {
        kind,
        target_id: target_id.to_string(),
        cycles,
        tasks,
    };
    let bytes = serde_json::to_vec(&payload).expect("deletion token payload is serializable");
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_changes_when_row_name_changes_even_if_counts_match() {
        // Integration tests exercise row-level token changes against the real
        // migrated database; this unit test only guards the hash shape.
        let task = |title| TokenTask {
            id: "t".into(),
            cycle_id: "c".into(),
            parent_id: None,
            title,
            position: 0,
            completed: false,
            proposal: None,
            created_at: 0,
        };
        let a = token("task", "t", Vec::new(), vec![task("before".into())]);
        let b = token("task", "t", Vec::new(), vec![task("after".into())]);
        assert_ne!(a, b);
    }
}

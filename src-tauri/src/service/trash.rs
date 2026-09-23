//! Reversible user deletion. One trash entry owns the complete FK cascade of
//! one action; active tables remain the single source for every planner read.

use std::collections::HashSet;

use rusqlite::{params, params_from_iter, types::{Value, ValueRef}, Connection};
use serde::{Deserialize, Serialize};

use crate::{db::Db, error::{AppError, AppResult}, repository::cycles as cycles_repo};
use super::{deletion::DeletionImpact, Mutation};

#[derive(Debug, Clone, Serialize)]
pub struct TrashEntry {
    pub id: String,
    pub kind: String,
    pub target_id: String,
    pub title: String,
    pub origin: String,
    pub deleted_at: i64,
    pub task_count: i64,
    pub cycle_count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
enum StoredValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl StoredValue {
    fn sql_value(&self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Integer(value) => Value::Integer(*value),
            Self::Real(value) => Value::Real(*value),
            Self::Text(value) => Value::Text(value.clone()),
            Self::Blob(value) => Value::Blob(value.clone()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredRow(Vec<(String, StoredValue)>);

impl StoredRow {
    fn text(&self, name: &str) -> Option<&str> {
        self.0.iter().find(|(key, _)| key == name).and_then(|(_, value)| match value {
            StoredValue::Text(value) => Some(value.as_str()),
            _ => None,
        })
    }
    fn integer(&self, name: &str) -> Option<i64> {
        self.0.iter().find(|(key, _)| key == name).and_then(|(_, value)| match value {
            StoredValue::Integer(value) => Some(*value),
            _ => None,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Snapshot {
    cycles: Vec<StoredRow>,
    tasks: Vec<StoredRow>,
    originals: Vec<StoredRow>,
    dismissals: Vec<StoredRow>,
    reviews: Vec<StoredRow>,
    dispositions: Vec<StoredRow>,
    reminders: Vec<StoredRow>,
}

fn db_error(error: rusqlite::Error) -> AppError { AppError::Db(error.to_string()) }

fn select_rows(conn: &Connection, table: &str, filters: &[(&str, &[String])]) -> AppResult<Vec<StoredRow>> {
    let present: Vec<_> = filters.iter().filter(|(_, ids)| !ids.is_empty()).collect();
    if present.is_empty() { return Ok(Vec::new()); }
    // Table and column names are fixed internal call sites, never user input.
    let conditions = present.iter().map(|(column, ids)|
        format!("{column} IN ({})", vec!["?"; ids.len()].join(","))
    ).collect::<Vec<_>>().join(" OR ");
    let sql = format!("SELECT * FROM {table} WHERE {conditions}");
    let args: Vec<&str> = present.iter().flat_map(|(_, ids)| ids.iter().map(String::as_str)).collect();
    let mut statement = conn.prepare(&sql).map_err(db_error)?;
    let names: Vec<String> = statement.column_names().iter().map(|name| (*name).to_string()).collect();
    let rows = statement.query_map(params_from_iter(args), |row| {
        let mut values = Vec::with_capacity(names.len());
        for (index, name) in names.iter().enumerate() {
            let value = match row.get_ref(index)? {
                ValueRef::Null => StoredValue::Null,
                ValueRef::Integer(value) => StoredValue::Integer(value),
                ValueRef::Real(value) => StoredValue::Real(value),
                ValueRef::Text(value) => StoredValue::Text(String::from_utf8_lossy(value).into_owned()),
                ValueRef::Blob(value) => StoredValue::Blob(value.to_vec()),
            };
            values.push((name.clone(), value));
        }
        Ok(StoredRow(values))
    }).map_err(db_error)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)
}

fn snapshot(conn: &Connection, impact: &DeletionImpact) -> AppResult<Snapshot> {
    let cycles = select_rows(conn, "cycles", &[("id", &impact.cycle_ids)])?;
    let tasks = select_rows(conn, "tasks", &[("id", &impact.task_ids)])?;
    let originals = select_rows(conn, "task_preview_originals", &[
        ("task_id", &impact.task_ids), ("cycle_id", &impact.cycle_ids),
    ])?;
    let dismissals = select_rows(conn, "planning_issue_dismissals", &[
        ("task_id", &impact.task_ids), ("cycle_id", &impact.cycle_ids),
    ])?;
    let reviews = select_rows(conn, "cycle_reviews", &[("cycle_id", &impact.cycle_ids)])?;
    let review_ids: Vec<String> = reviews.iter().filter_map(|row| row.text("id").map(str::to_string)).collect();
    let dispositions = select_rows(conn, "cycle_review_dispositions", &[
        ("task_id", &impact.task_ids), ("review_id", &review_ids),
    ])?;
    let mut reminders = select_rows(conn, "reminders", &[
        ("target_id", &impact.task_ids), ("target_id", &impact.cycle_ids),
    ])?;
    reminders.retain(|row| {
        let id = row.text("target_id");
        if row.text("target_kind") == Some("task") {
            id.is_some_and(|id| impact.task_ids.iter().any(|task| task == id))
        } else {
            id.is_some_and(|id| impact.cycle_ids.iter().any(|cycle| cycle == id))
        }
    });
    Ok(Snapshot { cycles, tasks, originals, dismissals, reviews, dispositions, reminders })
}

/// Must run before `prepare_deletion` and the physical delete, in the same
/// transaction. A failed delete rolls back the archive as well.
pub(crate) fn archive_in_tx(
    conn: &Connection, kind: &str, target_id: &str, title: &str,
    origin: &str, impact: &DeletionImpact,
) -> AppResult<()> {
    let snapshot = snapshot(conn, impact)?;
    let json = serde_json::to_string(&snapshot).map_err(|e| AppError::Internal(e.to_string()))?;
    conn.execute(
        "INSERT INTO trash_entries (id, kind, target_id, title, origin, deleted_at, task_count, cycle_count, snapshot_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![uuid::Uuid::new_v4().to_string(), kind, target_id, title, origin,
            super::now_ms(), impact.task_ids.len() as i64, impact.cycle_ids.len() as i64, json],
    ).map_err(db_error)?;
    Ok(())
}

pub fn list(db: &Db) -> AppResult<Vec<TrashEntry>> {
    let conn = db.pool().get()?;
    let mut statement = conn.prepare(
        "SELECT id, kind, target_id, title, origin, deleted_at, task_count, cycle_count
         FROM trash_entries ORDER BY deleted_at DESC, rowid DESC"
    ).map_err(db_error)?;
    let rows = statement.query_map([], |row| Ok(TrashEntry {
        id: row.get(0)?, kind: row.get(1)?, target_id: row.get(2)?,
        title: row.get(3)?, origin: row.get(4)?, deleted_at: row.get(5)?,
        task_count: row.get(6)?, cycle_count: row.get(7)?,
    })).map_err(db_error)?.collect::<rusqlite::Result<Vec<_>>>().map_err(db_error)?;
    Ok(rows)
}

fn insert_row(conn: &Connection, table: &str, row: &StoredRow) -> AppResult<()> {
    let columns = row.0.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>().join(",");
    let placeholders = vec!["?"; row.0.len()].join(",");
    let values: Vec<Value> = row.0.iter().map(|(_, value)| value.sql_value()).collect();
    conn.execute(&format!("INSERT INTO {table} ({columns}) VALUES ({placeholders})"),
        params_from_iter(values)).map_err(|e| AppError::conflict("trash_restore_conflict", e.to_string()))?;
    Ok(())
}

pub fn restore(db: &Db, entry_id: &str) -> AppResult<Mutation<()>> {
    let mut conn = db.pool().get()?;
    let tx = conn.transaction().map_err(db_error)?;
    let json: String = tx.query_row("SELECT snapshot_json FROM trash_entries WHERE id = ?1", [entry_id], |row| row.get(0))
        .map_err(|e| match e { rusqlite::Error::QueryReturnedNoRows => AppError::not_found("trash entry", entry_id), other => db_error(other) })?;
    let snapshot: Snapshot = serde_json::from_str(&json).map_err(|e| AppError::Internal(e.to_string()))?;
    tx.execute_batch("PRAGMA defer_foreign_keys = ON").map_err(db_error)?;
    for (table, rows) in [
        ("cycles", &snapshot.cycles), ("tasks", &snapshot.tasks),
        ("task_preview_originals", &snapshot.originals), ("cycle_reviews", &snapshot.reviews),
        ("cycle_review_dispositions", &snapshot.dispositions),
        ("planning_issue_dismissals", &snapshot.dismissals), ("reminders", &snapshot.reminders),
    ] {
        for row in rows { insert_row(&tx, table, row)?; }
    }
    // Deletion subtracted each removed session from surviving ancestors.
    // Rows restored from this snapshot already contain their former totals.
    let restored: HashSet<&str> = snapshot.cycles.iter().filter_map(|row| row.text("id")).collect();
    let mut mutation = Mutation::new(());
    mutation.trash_changed = true;
    for row in &snapshot.cycles {
        let id = row.text("id").ok_or_else(|| AppError::Internal("archived cycle without ID".into()))?;
        mutation.cycles.push(id);
        if row.text("type") == Some("session") {
            if row.text("task_id").is_some() {
                if let Some(day) = row.text("parent_id") { mutation.tasks.push(day); }
            }
            let focused = row.integer("focused_time").unwrap_or(0);
            let mut parent = row.text("parent_id").map(str::to_string);
            while let Some(id) = parent {
                let ancestor = cycles_repo::require(&tx, &id)
                    .map_err(|_| AppError::conflict("trash_restore_conflict", "The original parent plan is missing. Restore it first."))?;
                if !restored.contains(id.as_str()) {
                    if focused > 0 { cycles_repo::add_focused_time(&tx, &id, focused)?; }
                    mutation.cycles.push(&id);
                }
                parent = ancestor.parent_id;
            }
        } else if let Some(parent) = row.text("parent_id") { mutation.cycles.push(parent); }
    }
    for row in &snapshot.tasks {
        if let Some(cycle_id) = row.text("cycle_id") { mutation.tasks.push(cycle_id); }
    }
    tx.execute("DELETE FROM trash_entries WHERE id = ?1", [entry_id]).map_err(db_error)?;
    tx.commit().map_err(|e| AppError::conflict("trash_restore_conflict", e.to_string()))?;
    Ok(mutation)
}

pub fn delete_permanently(db: &Db, entry_ids: &[String]) -> AppResult<usize> {
    if entry_ids.is_empty() { return Err(AppError::validation("trash_selection_required", "Select items to delete.")); }
    let mut conn = db.pool().get()?;
    let tx = conn.transaction().map_err(db_error)?;
    let mut statement = tx.prepare("DELETE FROM trash_entries WHERE id = ?1").map_err(db_error)?;
    let mut count = 0;
    for id in entry_ids {
        let deleted = statement.execute([id]).map_err(db_error)?;
        if deleted != 1 {
            return Err(AppError::conflict("trash_selection_changed", "The selected trash entries changed. Refresh and try again."));
        }
        count += deleted;
    }
    drop(statement);
    tx.commit().map_err(db_error)?;
    Ok(count)
}

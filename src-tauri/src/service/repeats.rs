//! Repeat template use cases: save a focus block as a template, edit it
//! (future instances only), stop repeating, and materialize a day's instances.
//!
//! Behaviour contract: `openspec/specs/session-repeats/spec.md`.

use rusqlite::{Connection, OptionalExtension};

use crate::db::Db;
use crate::domain::cycle::CycleType;
use crate::domain::repeat::Repeat;
use crate::error::{AppError, AppResult};
use crate::repository::{cycles as cycles_repo, repeats as repo};
use crate::service::Mutation;

pub struct AddRepeatArgs {
    /// The focus block to save. It becomes the first linked instance.
    pub session_id: String,
}

/// "Repeat daily" on a focus block: record title + duration as a template and
/// link the block as the first instance.
pub fn add_repeat(db: &Db, args: &AddRepeatArgs, now: i64) -> AppResult<Mutation<Repeat>> {
    let _ = now;
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let session = cycles_repo::require(&tx, &args.session_id)?;
    if session.cycle_type != CycleType::Session {
        return Err(AppError::validation(
            "unsupported_cycle_type",
            "Only focus blocks can be saved as repeats",
        ));
    }
    let duration = session.duration.filter(|d| *d >= 0).ok_or_else(|| {
        AppError::validation(
            "repeat_duration_required",
            "A focus block needs a duration before it can repeat",
        )
    })?;
    let new = repo::NewRepeat {
        id: uuid::Uuid::new_v4().to_string(),
        title: session.title.clone(),
        duration,
        position: repo::max_position(&tx)? + 1,
    };
    repo::insert(&tx, &new)?;
    cycles_repo::set_repeat_id(&tx, &session.id, Some(&new.id))?;
    let repeat = repo::require(&tx, &new.id)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(Mutation::new(repeat).touching_cycle(session.id))
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct RepeatPatch {
    pub title: Option<String>,
    pub duration: Option<i64>,
    pub position: Option<i64>,
}

/// Edits the template row. Existing instances are never rewritten; the guard
/// below only refuses the edit when linked instances make "which instances are
/// future" undecidable (spec: 无法判定未来实例 → 明确失败).
pub fn update_repeat(db: &Db, repeat_id: &str, patch: &RepeatPatch) -> AppResult<Repeat> {
    if patch.title.as_deref().map(str::trim).map(str::is_empty) == Some(true) {
        return Err(AppError::validation(
            "invalid_title",
            "Title cannot be empty",
        ));
    }
    if let Some(duration) = patch.duration {
        if duration < 0 {
            return Err(AppError::validation(
                "invalid_duration",
                "duration cannot be negative",
            ));
        }
    }
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    repo::require(&tx, repeat_id)?;

    let instances = cycles_repo::list_by_repeat(&tx, repeat_id)?;
    for instance in &instances {
        let dated = match &instance.parent_id {
            Some(day_id) => cycles_repo::get(&tx, day_id)?.and_then(|day| day.starts_on),
            None => None,
        };
        if dated.is_none() {
            return Err(AppError::conflict(
                "repeat_future_unknown",
                "Cannot tell which instances would be affected; edit was not applied",
            ));
        }
    }

    repo::update(
        &tx,
        repeat_id,
        &repo::RepeatUpdate {
            title: patch.title.clone(),
            duration: patch.duration,
            position: patch.position,
        },
    )?;
    let updated = repo::require(&tx, repeat_id)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(updated)
}

/// "Stop repeating": archive the template and unlink every instance. Past
/// focus blocks and their focused time stay exactly as they are.
pub fn stop_repeat(db: &Db, repeat_id: &str) -> AppResult<Mutation<Repeat>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    repo::require(&tx, repeat_id)?;
    let instances = cycles_repo::list_by_repeat(&tx, repeat_id)?;
    for instance in &instances {
        cycles_repo::set_repeat_id(&tx, &instance.id, None)?;
    }
    repo::archive(&tx, repeat_id)?;
    let repeat = repo::require(&tx, repeat_id)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    let mut mutation = Mutation::new(repeat);
    for instance in instances {
        mutation.cycles.push(instance.id);
    }
    Ok(mutation)
}

/// Removes the template row entirely. Instances survive with their source
/// link cleared — unlink first, delete second (spec: 模板被删除).
pub fn remove_repeat(db: &Db, repeat_id: &str) -> AppResult<Mutation<()>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    repo::require(&tx, repeat_id)?;
    let instances = cycles_repo::list_by_repeat(&tx, repeat_id)?;
    for instance in &instances {
        cycles_repo::set_repeat_id(&tx, &instance.id, None)?;
    }
    repo::delete(&tx, repeat_id)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    let mut mutation = Mutation::new(());
    for instance in instances {
        mutation.cycles.push(instance.id);
    }
    Ok(mutation)
}

/// Materializes every active template into the day, idempotently: a template
/// that already produced a session for this day is skipped. New sessions keep
/// the template's order and can diverge freely afterwards.
pub fn generate_day_instances(
    db: &Db,
    day_cycle_id: &str,
    now: i64,
) -> AppResult<Mutation<Vec<String>>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let created = generate_for_day_in_tx(&tx, day_cycle_id, now)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    let mut mutation = Mutation::new(created.clone());
    mutation.tasks.push(day_cycle_id);
    for id in created {
        mutation.cycles.push(id);
    }
    Ok(mutation)
}

/// Transaction-scoped core, shared with `get_or_create_day`.
pub(crate) fn generate_for_day_in_tx(
    conn: &Connection,
    day_cycle_id: &str,
    _now: i64,
) -> AppResult<Vec<String>> {
    let day = cycles_repo::require(conn, day_cycle_id)?;
    if day.cycle_type != CycleType::Day {
        return Err(AppError::validation(
            "invalid_parent_type",
            "Repeat instances materialize inside a day cycle",
        ));
    }
    let templates = repo::list_active(conn)?;
    let mut created = Vec::new();
    for template in templates {
        let already = conn
            .query_row(
                "SELECT 1 FROM cycles WHERE parent_id = ?1 AND repeat_id = ?2 LIMIT 1",
                rusqlite::params![day_cycle_id, template.id],
                |_| Ok(()),
            )
            .optional()
            .map_err(|e| AppError::Db(e.to_string()))?;
        if already.is_some() {
            continue;
        }
        let new = cycles_repo::NewCycle {
            id: uuid::Uuid::new_v4().to_string(),
            title: template.title.clone(),
            cycle_type: CycleType::Session,
            parent_id: Some(day_cycle_id.to_string()),
            position: template.position,
            duration: Some(template.duration),
            starts_on: None,
            ends_on: None,
            calendar_key: None,
            repeat_id: Some(template.id.clone()),
            task_id: None,
            created_at: crate::service::now_ms(),
        };
        cycles_repo::insert(conn, &new)?;
        created.push(new.id);
    }
    Ok(created)
}

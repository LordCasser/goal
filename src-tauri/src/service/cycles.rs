//! Cycle use cases: creation with calendar identity, lifecycle, deletion
//! guards, copying uncompleted work.
//!
//! Behaviour contract: `openspec/specs/planning-cycles/spec.md`.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use rusqlite::Connection;

use crate::db::Db;
use crate::domain::calendar::{
    self, calculate_ends_on, dated_cycle_bounds, day_key, format_date, long_term_key,
    open_long_term_key, parse_date,
    week_key,
};
use crate::domain::cycle::{
    Cycle, CycleType, LifecycleAction, LifecycleState, LATER_CYCLE_ID, LONG_TERM_DURATIONS_MONTHS,
    WEEK_DURATION_MS,
};
use crate::domain::task::{is_empty_input_row, Task};
use crate::error::{AppError, AppResult};
use crate::repository::{cycles as repo, tasks as tasks_repo};
use crate::service::{settings as settings_service, Mutation};

/// How many of the most recent cycles of a type stay deletable. The upstream
/// constant was never recovered from the binary (see
/// `analysis/reports/00-technical-report.md` §"周期删除的具体阈值未提取");
/// 5 keeps recent history cleanable while protecting anything older.
pub const DELETABLE_LATEST_N: i64 = 5;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PlannerState {
    /// All visible month/week/day cycles; the frontend groups them by parent.
    pub cycles: Vec<Cycle>,
    /// The permanent Do Later container.
    pub later: Cycle,
}

pub fn get_planner_state(db: &Db) -> AppResult<PlannerState> {
    let conn = db.pool().get()?;
    let cycles = repo::list_planner_cycles(&conn)?;
    let later = repo::require(&conn, LATER_CYCLE_ID)?;
    Ok(PlannerState { cycles, later })
}

/// Focus blocks of one day cycle, in column order. Read-only listing; every
/// mutation (`add_session`, start/finish, repeats) still emits `cycles:changed`
/// for the day, which is what keeps this query fresh on the frontend.
pub fn list_sessions(db: &Db, day_cycle_id: &str) -> AppResult<Vec<Cycle>> {
    let conn = db.pool().get()?;
    repo::list_sessions_by_day(&conn, day_cycle_id)
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct CreateCycleArgs {
    /// `month` (Long-term) | `week` | `day`. Sessions go through `add_session`.
    pub cycle_type: String,
    pub parent_id: Option<String>,
    /// Long-term only: 1, 3 or 6 product months (28 days each).
    pub duration_months: Option<i64>,
    /// Explicit bounds are mutually exclusive with preset product months.
    pub starts_on: Option<String>,
    pub ends_on: Option<String>,
    pub progress_check: Option<crate::domain::cycle::ProgressCheck>,
    /// Optional title; dated cycles derive one from their bounds when absent.
    pub title: Option<String>,
    /// Day cycles: `YYYY-MM-DD`. Defaults to today (local).
    pub date: Option<String>,
}

/// Creates a strict planning cycle. An existing calendar identity surfaces as
/// `calendar_key_taken` — the caller should reuse the existing cycle
/// (spec: 重复创建同一天).
pub fn create_planning_cycle(
    db: &Db,
    args: &CreateCycleArgs,
    today: NaiveDate,
    now: i64,
) -> AppResult<Mutation<Cycle>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let week_start_day = settings_service::week_start_day_or_default(&tx)?;

    let kind = match args.cycle_type.as_str() {
        "month" => CycleType::Month,
        "week" => CycleType::Week,
        "day" => CycleType::Day,
        other => {
            return Err(AppError::validation(
                "unsupported_cycle_type",
                format!("cannot create a '{other}' cycle directly"),
            ))
        }
    };

    let (parent_id, position, starts_on, ends_on, duration, key, title) = match kind {
        CycleType::Month => {
            if args.parent_id.is_some() {
                return Err(AppError::validation(
                    "invalid_parent_type",
                    "A long-term cycle is a root and cannot have a parent",
                ));
            }
            let (starts_on, ends_on) = match (&args.starts_on, &args.ends_on, args.duration_months)
            {
                (Some(start), Some(end), None) => {
                    let (start, end) = calendar::custom_long_term_bounds(start, end)?;
                    (start, Some(end))
                }
                (Some(start), None, None) => {
                    let start = parse_date(start)
                        .filter(|date| start.len() == 10 && format_date(*date) == start.as_str())
                        .ok_or_else(|| AppError::validation("invalid_date", "Choose a valid start date."))?;
                    (start, None)
                }
                (None, None, Some(months)) if LONG_TERM_DURATIONS_MONTHS.contains(&months) => {
                    (today, Some(calculate_ends_on(today, months)))
                }
                (None, None, Some(_)) => {
                    return Err(AppError::validation(
                        "unsupported_long_term_duration",
                        "Choose a preset duration or custom dates.",
                    ))
                }
                (None, None, None) => {
                    return Err(AppError::validation(
                        "long_term_duration_required",
                        "Choose a preset duration or custom dates.",
                    ))
                }
                _ => {
                    return Err(AppError::validation(
                        "invalid_cycle_range",
                        "Provide both custom dates, without a preset duration.",
                    ))
                }
            };
            let duration = ends_on.map(|end| (end - starts_on).num_days() * crate::domain::cycle::MS_PER_DAY);
            let key = ends_on.map_or_else(|| open_long_term_key(starts_on), |end| long_term_key(starts_on, end));
            let position = next_cycle_position(&tx, None)?;
            (
                None,
                position,
                Some(starts_on),
                ends_on,
                duration,
                Some(key),
                args.title.clone().unwrap_or_else(|| "Long-term".into()),
            )
        }
        CycleType::Week | CycleType::Day => {
            if args.starts_on.is_some() || args.ends_on.is_some() || args.progress_check.is_some() {
                return Err(AppError::validation(
                    "unsupported_cycle_type",
                    "Custom bounds and progress checks belong to long-term cycles.",
                ));
            }

            // Calendar containers may stand alone. Goal ownership belongs to
            // task.parent_id, not to the container's optional parent.
            let parent_id = args.parent_id.clone();
            if let Some(id) = &parent_id {
                let parent = repo::require(&tx, id)?;
                let expected = if kind == CycleType::Week {
                    CycleType::Month
                } else {
                    CycleType::Week
                };
                if parent.cycle_type != expected || parent.id == LATER_CYCLE_ID {
                    return Err(AppError::validation(
                        "invalid_parent_type",
                        "Choose a parent at the preceding time level",
                    ));
                }
                if parent.finished {
                    return Err(AppError::conflict(
                        "parent_cycle_ended",
                        "Cannot create a planning cycle under an ended parent",
                    ));
                }
            }
            let date = match &args.date {
                Some(raw) => parse_date(raw).ok_or_else(|| {
                    AppError::validation("invalid_date", "date must be YYYY-MM-DD")
                })?,
                None => today,
            };
            let (starts_on, ends_on) = dated_cycle_bounds(date, kind, week_start_day as u32, 0)
                .ok_or_else(|| AppError::Internal("dated cycle bounds missing".into()))?;
            let position = next_cycle_position(&tx, parent_id.as_deref())?;
            let (duration, key) = if kind == CycleType::Week {
                (WEEK_DURATION_MS, week_key(starts_on))
            } else {
                (crate::domain::cycle::DAY_DURATION_MS, day_key(starts_on))
            };
            (
                parent_id,
                position,
                Some(starts_on),
                Some(ends_on),
                Some(duration),
                Some(key),
                args.title.clone().unwrap_or_else(|| format_date(starts_on)),
            )
        }
        CycleType::Session => unreachable!("rejected above"),
    };

    let new = repo::NewCycle {
        id: uuid::Uuid::new_v4().to_string(),
        title,
        cycle_type: kind,
        parent_id: parent_id.clone(),
        position,
        duration,
        starts_on: starts_on.map(format_date),
        ends_on: ends_on.map(format_date),
        calendar_key: key,
        repeat_id: None,
        task_id: None,
        created_at: now,
    };
    repo::insert(&tx, &new)?;
    if let Some(check) = args.progress_check.as_ref().filter(|_| kind == CycleType::Month) {
        let start = starts_on.expect("long-term start validated");
        calendar::validate_progress_check(check, start, ends_on)?;
        let json = serde_json::to_string(check).map_err(|e| AppError::Internal(e.to_string()))?;
        tx.execute(
            "UPDATE cycles SET progress_check = ?1 WHERE id = ?2",
            rusqlite::params![json, new.id],
        )
        .map_err(crate::error::from_rusqlite)?;
    }
    let created = repo::require(&tx, &new.id)?;
    let imported = if matches!(kind, CycleType::Week | CycleType::Day)
        && auto_carry_enabled(&tx)?
    {
        !copy_adjacent_unfinished_in_tx(&tx, &created, now, &HashMap::new())?.is_empty()
    } else {
        false
    };
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;

    let mut mutation = Mutation::new(created);
    mutation.cycles.push(new.id.clone());
    if let Some(parent) = parent_id {
        mutation.cycles.push(parent);
    }
    if imported {
        mutation.tasks.push(new.id);
    }
    Ok(mutation)
}

fn auto_carry_enabled(conn: &Connection) -> AppResult<bool> {
    Ok(crate::repository::settings::get(
        conn,
        crate::repository::settings::KEY_AUTO_CARRY_UNFINISHED,
    )?
    .as_deref()
        == Some("true"))
}

/// Copies only the immediately adjacent period, preserving external links
/// unless an explicitly imported weekly parent has a new identity.
fn copy_adjacent_unfinished_in_tx(
    conn: &Connection,
    target: &Cycle,
    now: i64,
    external_parent_map: &HashMap<String, String>,
) -> AppResult<HashMap<String, String>> {
    let start = target
        .starts_on
        .as_deref()
        .and_then(parse_date)
        .ok_or_else(|| AppError::Internal("new dated cycle has no valid start".into()))?;
    let source_key = match target.cycle_type {
        CycleType::Week => week_key(calendar::add_days(start, -7)),
        CycleType::Day => day_key(calendar::add_days(start, -1)),
        _ => return Ok(HashMap::new()),
    };
    let Some(source_cycle) = repo::get_by_calendar_key(conn, &source_key)? else {
        return Ok(HashMap::new());
    };
    if source_cycle.archived {
        return Ok(HashMap::new());
    }
    let source = tasks_repo::list_visible_by_cycle(conn, &source_cycle.id)?;
    let source_ids: std::collections::HashSet<&str> =
        source.iter().map(|task| task.id.as_str()).collect();
    let target_tasks = tasks_repo::list_visible_by_cycle(conn, &target.id)?;
    let target_ids: std::collections::HashSet<String> =
        target_tasks.iter().map(|task| task.id.clone()).collect();
    let mut existing_keys: std::collections::HashSet<(String, Option<String>)> = target_tasks
        .into_iter()
        .filter(|task| {
            !task.title.trim().is_empty()
                && !task.parent_id.as_deref().is_some_and(|id| target_ids.contains(id))
        })
        .map(|task| (task.title.trim().to_string(), task.parent_id))
        .collect();
    let mut included = std::collections::HashSet::new();
    for task in &source {
        if task.completed || task.title.trim().is_empty()
            || task.parent_id.as_deref().is_some_and(|id| source_ids.contains(id))
        {
            continue;
        }
        let target_parent = task.parent_id.as_ref().map(|id| {
            external_parent_map.get(id).cloned().unwrap_or_else(|| id.clone())
        });
        if existing_keys.insert((task.title.trim().to_string(), target_parent)) {
            included.insert(task.id.clone());
        }
    }
    loop {
        let mut added = false;
        for task in &source {
            if task.completed || task.title.trim().is_empty() || included.contains(&task.id) {
                continue;
            }
            if task.parent_id.as_ref().is_some_and(|id| included.contains(id)) {
                included.insert(task.id.clone());
                added = true;
            }
        }
        if !added {
            break;
        }
    }
    let mut remaining: Vec<&Task> = source.iter().filter(|task| included.contains(&task.id)).collect();
    let mut id_map = HashMap::new();
    let mut next_root_position = tasks_repo::max_visible_root_position(conn, &target.id)? + 1;
    while !remaining.is_empty() {
        let mut deferred = Vec::new();
        let mut copied_any = false;
        for original in remaining {
            let Some(parent_id) = original.parent_id.as_deref() else {
                let new_id = uuid::Uuid::new_v4().to_string();
                tasks_repo::insert(conn, &tasks_repo::NewTask {
                    id: new_id.clone(),
                    cycle_id: target.id.clone(),
                    parent_id: None,
                    title: original.title.clone(),
                    subtasks: original.subtasks.clone(),
                    position: next_root_position,
                    completed: false,
                    goal_breakdown: original.goal_breakdown.clone(),
                    needs_refinement: original.needs_refinement,
                    needs_breakdown: original.needs_breakdown,
                    root_color_key: original.root_color_key.clone(),
                    copied_from_task_id: Some(original.id.clone()),
                    created_at: now,
                })?;
                next_root_position += 1;
                id_map.insert(original.id.clone(), new_id);
                copied_any = true;
                continue;
            };
            if source_ids.contains(parent_id) && !id_map.contains_key(parent_id) {
                deferred.push(original);
                continue;
            }
            let parent_id = id_map.get(parent_id).or_else(|| external_parent_map.get(parent_id))
                .cloned().unwrap_or_else(|| parent_id.to_string());
            let is_child = source_ids.contains(original.parent_id.as_deref().unwrap());
            let position = if is_child { original.position } else {
                let position = next_root_position;
                next_root_position += 1;
                position
            };
            let new_id = uuid::Uuid::new_v4().to_string();
            tasks_repo::insert(conn, &tasks_repo::NewTask {
                id: new_id.clone(),
                cycle_id: target.id.clone(),
                parent_id: Some(parent_id),
                title: original.title.clone(),
                subtasks: original.subtasks.clone(),
                position,
                completed: false,
                goal_breakdown: original.goal_breakdown.clone(),
                needs_refinement: original.needs_refinement,
                needs_breakdown: original.needs_breakdown,
                root_color_key: original.root_color_key.clone(),
                copied_from_task_id: Some(original.id.clone()),
                created_at: now,
            })?;
            id_map.insert(original.id.clone(), new_id);
            copied_any = true;
        }
        if !copied_any {
            return Err(AppError::Internal("cycle detected in copied task tree".into()));
        }
        remaining = deferred;
    }
    Ok(id_map)
}

pub(crate) fn next_cycle_position(conn: &Connection, parent_id: Option<&str>) -> AppResult<i64> {
    conn.query_row(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM cycles WHERE parent_id IS ?1 AND archived = 0 AND id != 'later'",
        [parent_id], |r| r.get(0),
    ).map_err(|e| AppError::Db(e.to_string()))
}

/// Opening a date works without a long-term goal; reuse its day and week
/// identities. A newly needed week is independent from long-term containers.
pub fn get_or_create_day(db: &Db, date: NaiveDate, now: i64) -> AppResult<Mutation<Cycle>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let week_start_day = settings_service::week_start_day_or_default(&tx)?;
    let day = get_or_create_day_in_tx(&tx, date, week_start_day as u32, now)?;
    let generated = crate::service::repeats::generate_for_day_in_tx(&tx, &day.id, now)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    let mut mutation = Mutation::new(day.clone()).touching_cycle(day.id.clone());
    if let Some(parent) = day.parent_id {
        mutation.cycles.push(parent);
    }
    if !generated.is_empty() {
        mutation.tasks.push(day.id);
    }
    Ok(mutation)
}

/// Explicit user creation path. Shared ensure helpers stay free of carry-over
/// effects because Later promotion and calendar moves call them too.
pub fn create_day_plan(db: &Db, date: NaiveDate, now: i64) -> AppResult<Mutation<Cycle>> {
    let mut conn = db.pool().get()?;
    let tx = conn.transaction().map_err(|e| AppError::Db(e.to_string()))?;
    let week_start_day = settings_service::week_start_day_or_default(&tx)? as u32;
    let date_str = format_date(date);
    let day_existed = get_day_by_date(&tx, &date_str)?.is_some();
    let week_existed = find_covering_cycle(&tx, CycleType::Week, &date_str)?.is_some();
    let day = get_or_create_day_in_tx(&tx, date, week_start_day, now)?;
    let generated = crate::service::repeats::generate_for_day_in_tx(&tx, &day.id, now)?;
    let mut mutation = Mutation::new(day.clone()).touching_cycle(day.id.clone());
    let mut week_id = None;
    if let Some(parent_id) = day.parent_id.as_deref() {
        mutation.cycles.push(parent_id.to_string());
        week_id = Some(parent_id.to_string());
    }
    if !generated.is_empty() {
        mutation.tasks.push(day.id.clone());
    }
    if auto_carry_enabled(&tx)? && !day_existed {
        let week_map = if !week_existed {
            if let Some(parent_id) = week_id.as_deref() {
                let week = repo::require(&tx, parent_id)?;
                let map = copy_adjacent_unfinished_in_tx(&tx, &week, now, &HashMap::new())?;
                if !map.is_empty() {
                    mutation.tasks.push(parent_id.to_string());
                }
                map
            } else {
                HashMap::new()
            }
        } else {
            HashMap::new()
        };
        if !copy_adjacent_unfinished_in_tx(&tx, &day, now, &week_map)?.is_empty() {
            mutation.tasks.push(day.id.clone());
        }
    }
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(mutation)
}

/// Shared with Calendar moves, inside the caller's transaction.
pub(crate) fn get_or_create_day_in_tx(
    conn: &Connection,
    date: NaiveDate,
    week_start_day: u32,
    now: i64,
) -> AppResult<Cycle> {
    let date_str = format_date(date);
    if let Some(day) = get_day_by_date(conn, &date_str)? {
        return Ok(day);
    }
    let week = match find_covering_cycle(conn, CycleType::Week, &date_str)? {
        Some(week) => week,
        None => get_or_create_week_in_tx(conn, date, week_start_day, now)?,
    };
    let new = repo::NewCycle {
        id: uuid::Uuid::new_v4().to_string(),
        title: date_str.clone(),
        cycle_type: CycleType::Day,
        parent_id: Some(week.id.clone()),
        position: next_cycle_position(conn, Some(&week.id))?,
        duration: Some(crate::domain::cycle::DAY_DURATION_MS),
        starts_on: Some(date_str),
        ends_on: Some(format_date(calendar::add_days(date, 1))),
        calendar_key: Some(day_key(date)),
        repeat_id: None,
        task_id: None,
        created_at: now,
    };
    repo::insert(conn, &new)?;
    repo::require(conn, &new.id)
}

/// Resolve the configured natural week without creating a day as a side effect.
/// The caller owns the transaction so a failed task move also rolls this back.
pub(crate) fn get_or_create_week_in_tx(
    conn: &Connection,
    date: NaiveDate,
    week_start_day: u32,
    now: i64,
) -> AppResult<Cycle> {
    let (start, end) = dated_cycle_bounds(date, CycleType::Week, week_start_day, 0)
        .ok_or_else(|| AppError::Internal("week bounds missing".into()))?;
    let key = week_key(start);
    if let Some(week) = repo::get_by_calendar_key(conn, &key)? {
        return Ok(week);
    }
    let new = repo::NewCycle {
        id: uuid::Uuid::new_v4().to_string(),
        title: format_date(start),
        cycle_type: CycleType::Week,
        parent_id: None,
        position: next_cycle_position(conn, None)?,
        duration: Some(WEEK_DURATION_MS),
        starts_on: Some(format_date(start)),
        ends_on: Some(format_date(end)),
        calendar_key: Some(key),
        repeat_id: None,
        task_id: None,
        created_at: now,
    };
    repo::insert(conn, &new)?;
    repo::require(conn, &new.id)
}

fn find_covering_cycle(conn: &Connection, kind: CycleType, date: &str) -> AppResult<Option<Cycle>> {
    if !matches!(kind, CycleType::Week | CycleType::Month) {
        return Ok(None);
    }
    let all = repo::list_planner_cycles(conn)?;
    Ok(all
        .into_iter()
        .filter(|c| c.cycle_type == kind && !c.finished)
        .filter(|c| match (c.starts_on.as_deref(), c.ends_on.as_deref()) {
            (Some(s), Some(e)) => s <= date && date < e,
            (Some(s), None) if kind == CycleType::Month => s <= date,
            _ => false,
        })
        .min_by_key(|c| c.created_at))
}

fn get_day_by_date(conn: &Connection, date: &str) -> AppResult<Option<Cycle>> {
    repo::get_by_calendar_key(conn, &format!("{}{}", calendar::DAY_KEY_PREFIX, date))
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct AddSessionArgs {
    pub task_id: Option<String>,
    pub day_cycle_id: String,
    pub title: String,
    /// Milliseconds; `None` leaves the focus block without a set duration.
    pub duration_ms: Option<i64>,
    pub position: Option<i64>,
}

pub fn add_session(db: &Db, args: &AddSessionArgs, now: i64) -> AppResult<Mutation<Cycle>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let day = repo::require(&tx, &args.day_cycle_id)?;
    if day.cycle_type != CycleType::Day {
        return Err(AppError::validation(
            "invalid_parent_type",
            "Focus blocks live inside a day cycle",
        ));
    }
    ensure_cycle_mutable(&day)?;
    if let Some(task_id) = &args.task_id {
        let task = tasks_repo::require(&tx, task_id)?;
        if task.cycle_id != day.id || task.title.trim().is_empty() || task.proposal.is_some() {
            return Err(AppError::validation(
                "invalid_focus_task",
                "Choose a committed task with a title in this day",
            ));
        }
        crate::service::tasks::ensure_task_editable(&tx, task_id, false)?;
    }
    let position = match args.position {
        Some(p) => p,
        None => repo::max_position(&tx, &day.id)? + 1,
    };
    let new = repo::NewCycle {
        id: uuid::Uuid::new_v4().to_string(),
        title: args.title.clone(),
        cycle_type: CycleType::Session,
        parent_id: Some(day.id.clone()),
        position,
        duration: args.duration_ms,
        starts_on: None,
        ends_on: None,
        calendar_key: None,
        repeat_id: None,
        task_id: args.task_id.clone(),
        created_at: now,
    };
    repo::insert(&tx, &new)?;
    let session = repo::require(&tx, &new.id)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    let mut mutation = Mutation::new(session);
    mutation.cycles.push(day.id);
    mutation.cycles.push(new.id);
    Ok(mutation)
}

/// Ended pages are review-only (upstream: `cycle_ended`).
pub fn ensure_cycle_mutable(cycle: &Cycle) -> AppResult<()> {
    if cycle.finished {
        return Err(AppError::conflict(
            "cycle_ended",
            "This planner page has ended and can only be reviewed",
        ));
    }
    Ok(())
}

/// Edits a focus block (title, duration). Planning cycles are commitments:
/// their title and duration never change (spec: 时长是创建时的一次性承诺).
pub fn update_session(
    db: &Db,
    cycle_id: &str,
    title: String,
    duration_ms: Option<i64>,
) -> AppResult<Mutation<Cycle>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let target = repo::require(&tx, cycle_id)?;
    if target.cycle_type != CycleType::Session {
        return Err(AppError::validation(
            "cycle_immutable",
            "Planning cycle titles and durations cannot be changed; end the cycle and start a new one",
        ));
    }
    if title.trim().is_empty() {
        return Err(AppError::validation(
            "invalid_title",
            "Title cannot be empty",
        ));
    }
    if target.started && duration_ms.is_some() && duration_ms != target.duration {
        return Err(AppError::validation(
            "cycle_started",
            "A started focus block cannot change its duration",
        ));
    }
    repo::update_session_fields(&tx, cycle_id, &title, duration_ms)?;
    let updated = repo::require(&tx, cycle_id)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    let mut mutation = Mutation::new(updated);
    mutation.cycles.push(cycle_id.to_string());
    mutation.cycles.push(
        target
            .parent_id
            .clone()
            .unwrap_or_else(|| cycle_id.to_string()),
    );
    Ok(mutation)
}

pub fn start_cycle(db: &Db, cycle_id: &str, now: i64) -> AppResult<Mutation<Cycle>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let target = repo::require(&tx, cycle_id)?;
    // A cycle without a set duration cannot enter the lifecycle at all
    // (spec: 启动周期前必须有时长). The Later container's duration of 0
    // counts as "not set".
    let open_long_term = target.cycle_type == CycleType::Month
        && target.starts_on.is_some() && target.ends_on.is_none() && target.duration.is_none();
    if !open_long_term && target.duration.map(|d| d <= 0).unwrap_or(true) {
        return Err(AppError::validation(
            "cycle_duration_required",
            "Cycle duration must be set before starting",
        ));
    }
    let state = target
        .lifecycle()
        .ok_or_else(|| AppError::Internal("cycle in impossible lifecycle state".into()))?;
    crate::domain::cycle::transition(state, LifecycleAction::Start)
        .map_err(|e| AppError::conflict(e.code, e.message))?;
    repo::set_lifecycle(&tx, cycle_id, true, false, Some(now), None)?;

    // A focus block's due notice rides the reminders scheduler: the row is
    // created in the same transaction as the start, and the backend thread
    // delivers it whether or not the app is in the foreground
    // (add-onboarding-and-lifecycle §6.4 / add-reminders-notifications §3.4).
    // quiet_ok = 0 — the end of a focus block is a hard time point.
    if target.cycle_type == CycleType::Session {
        if let Some(duration) = target.duration.filter(|d| *d > 0) {
            crate::repository::reminders::insert(
                &tx,
                &crate::repository::reminders::NewReminder {
                    id: uuid::Uuid::new_v4().to_string(),
                    target_kind: crate::repository::reminders::TargetKind::Session,
                    target_id: cycle_id.to_string(),
                    fire_at: now + duration,
                    quiet_ok: false,
                    created_at: now,
                },
            )?;
        }
    }

    let updated = repo::require(&tx, cycle_id)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    let mut mutation = Mutation::new(updated);
    mutation.cycles.push(cycle_id.to_string());
    mutation.cycles.push(
        target
            .parent_id
            .clone()
            .unwrap_or_else(|| cycle_id.to_string()),
    );
    Ok(mutation)
}

pub fn finish_cycle(db: &Db, cycle_id: &str, now: i64) -> AppResult<Mutation<Cycle>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let target = repo::require(&tx, cycle_id)?;
    crate::service::proposals::ensure_cycle_tree_unlocked(&tx, cycle_id)?;
    let state = target
        .lifecycle()
        .ok_or_else(|| AppError::Internal("cycle in impossible lifecycle state".into()))?;
    crate::domain::cycle::transition(state, LifecycleAction::Finish)
        .map_err(|e| AppError::conflict(e.code, e.message))?;

    // Finishing a focus block accrues its elapsed time into the block and up
    // the chain (day -> week -> month) so the stored `focused_time` reflects
    // the work done (spec: 进度必须可感知).
    let mut touched: Vec<String> = vec![cycle_id.to_string()];
    if target.cycle_type == CycleType::Session {
        let started_at = target.started_at.unwrap_or(now);
        let delta = (now - started_at).max(0);
        if delta > 0 {
            repo::add_focused_time(&tx, cycle_id, delta)?;
            let mut parent = target.parent_id.clone();
            while let Some(pid) = parent {
                repo::add_focused_time(&tx, &pid, delta)?;
                touched.push(pid.clone());
                parent = repo::get(&tx, &pid)?.and_then(|c| c.parent_id);
            }
        }
    }
    repo::set_lifecycle(&tx, cycle_id, true, true, None, Some(now))?;
    let updated = repo::require(&tx, cycle_id)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;

    let mut mutation = Mutation::new(updated);
    if target.task_id.is_some() {
        if let Some(day_id) = &target.parent_id {
            mutation.tasks.push(day_id);
        }
    }
    for id in touched {
        mutation.cycles.push(id);
    }
    Ok(mutation)
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CycleDeletionPreview {
    pub cycle_id: String,
    pub guard_code: Option<String>,
    pub guard_message: Option<String>,
    pub descendant_cycles: i64,
    pub tasks: i64,
    pub started_sessions: i64,
    pub total_focus_blocks: i64,
    pub confirmation_token: String,
}

/// The deletion guards, in evaluation order. `None` = deletable.
fn deletion_guard(conn: &Connection, target: &Cycle) -> AppResult<Option<(String, String)>> {
    if target.id == LATER_CYCLE_ID {
        return Ok(Some((
            "protected_container".into(),
            "The Later list can't be deleted.".into(),
        )));
    }
    // A focus block is an event record, not a planning container. Its own
    // finished/started state must not inherit the container deletion guards;
    // deleting it is handled as a timer/session removal in the same tx below.
    if target.cycle_type == CycleType::Session {
        return Ok(None);
    }
    if target.finished {
        return Ok(Some((
            "past_cycle".into(),
            "Past cycles can't be deleted.".into(),
        )));
    }
    if repo::count_started_sessions(conn, &target.id)? > 0 {
        return Ok(Some((
            "has_started_session".into(),
            "This cycle contains started focus blocks.".into(),
        )));
    }
    if matches!(
        target.cycle_type,
        CycleType::Month | CycleType::Week | CycleType::Day
    ) && repo::count_newer_same_type(conn, target)? >= DELETABLE_LATEST_N
    {
        return Ok(Some((
            "not_latest_n".into(),
            format!("Only the latest {DELETABLE_LATEST_N} cycles can be deleted."),
        )));
    }
    Ok(None)
}

pub fn get_cycle_deletion_preview(db: &Db, cycle_id: &str) -> AppResult<CycleDeletionPreview> {
    let conn = db.pool().get()?;
    let target = repo::require(&conn, cycle_id)?;
    let guard = deletion_guard(&conn, &target)?;
    let impact = crate::service::deletion::cycle_impact(&conn, cycle_id)?;
    let impacted_tasks = impact
        .task_ids
        .iter()
        .map(|id| tasks_repo::require(&conn, id))
        .collect::<AppResult<Vec<_>>>()?;
    let parent_ids: HashSet<&str> = impacted_tasks
        .iter()
        .filter_map(|task| task.parent_id.as_deref())
        .collect();
    let tasks = impacted_tasks
        .iter()
        .filter(|task| {
            task.proposal.is_none()
                && (!is_empty_input_row(task) || parent_ids.contains(task.id.as_str()))
        })
        .count() as i64;
    Ok(CycleDeletionPreview {
        cycle_id: cycle_id.to_string(),
        guard_code: guard.as_ref().map(|(code, _)| code.clone()),
        guard_message: guard.map(|(_, message)| message),
        descendant_cycles: impact.descendant_cycles,
        tasks,
        started_sessions: impact.started_focus_count,
        total_focus_blocks: impact.total_focus_blocks,
        confirmation_token: impact.token,
    })
}

pub fn delete_cycle(db: &Db, cycle_id: &str) -> AppResult<Mutation<()>> {
    delete_cycle_inner(db, cycle_id, None, false, false)
}

/// Trusted user action, such as an approved Coach deletion.
pub fn trash_cycle(db: &Db, cycle_id: &str) -> AppResult<Mutation<()>> {
    delete_cycle_inner(db, cycle_id, None, false, true)
}

/// GUI deletion entry point.  The impact is recomputed inside the same
/// transaction as the mutation, so a preview cannot authorize a changed
/// subtree.  Coach and other trusted internal callers use `delete_cycle`.
pub fn delete_cycle_confirmed(
    db: &Db,
    cycle_id: &str,
    confirmation_token: Option<&str>,
) -> AppResult<Mutation<()>> {
    delete_cycle_inner(db, cycle_id, confirmation_token, true, true)
}

/// Remove the focus blocks currently shown on one day as one transaction.
/// The expected IDs prevent a stale calendar menu from silently deleting new blocks.
pub fn delete_day_focus_blocks(
    db: &Db,
    day_cycle_id: &str,
    expected_ids: &[String],
) -> AppResult<Mutation<usize>> {
    let mut conn = db.pool().get()?;
    let tx = conn.transaction().map_err(|e| AppError::Db(e.to_string()))?;
    let day = repo::require(&tx, day_cycle_id)?;
    if day.cycle_type != CycleType::Day {
        return Err(AppError::validation("invalid_parent_type", "Focus blocks belong to a day plan"));
    }
    let sessions = repo::list_sessions_by_day(&tx, day_cycle_id)?;
    let mut actual: Vec<_> = sessions.iter().map(|session| session.id.clone()).collect();
    let mut expected = expected_ids.to_vec();
    actual.sort();
    expected.sort();
    if actual != expected {
        return Err(AppError::conflict("day_content_changed", "The day's focus blocks changed; review the day and try again"));
    }
    let mut mutation = Mutation::new(sessions.len()).touching_cycle(day_cycle_id);
    let mut impacts = Vec::with_capacity(sessions.len());
    for session in &sessions {
        crate::service::proposals::ensure_cycle_tree_unlocked(&tx, &session.id)?;
        let impact = crate::service::deletion::cycle_impact(&tx, &session.id)?;
        for task_id in &impact.task_ids {
            crate::service::tasks::ensure_task_editable(&tx, task_id, true)?;
        }
        impacts.push(impact);
    }
    if let Some(mut impact) = impacts.into_iter().reduce(|mut combined, next| {
        combined.cycle_ids.extend(next.cycle_ids);
        combined.task_ids.extend(next.task_ids);
        combined.task_cycle_ids.extend(next.task_cycle_ids);
        combined.descendant_cycles += next.descendant_cycles;
        combined.total_focus_blocks += next.total_focus_blocks;
        combined.started_focus_count += next.started_focus_count;
        combined
    }) {
        for ids in [&mut impact.cycle_ids, &mut impact.task_ids, &mut impact.task_cycle_ids] {
            ids.sort();
            ids.dedup();
        }
        let title = if sessions.len() == 1 {
            sessions[0].title.clone()
        } else {
            format!("{} ({})", day.title, sessions.len())
        };
        crate::service::trash::archive_in_tx(&tx, "cycle", day_cycle_id, &title, &day.title, &impact)?;
        mutation.merge(crate::service::deletion::prepare_deletion(&tx, &impact)?);
    }
    for session in sessions {
        repo::delete(&tx, &session.id)?;
    }
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    mutation.trash_changed = mutation.value > 0;
    Ok(mutation)
}

fn delete_cycle_inner(
    db: &Db,
    cycle_id: &str,
    confirmation_token: Option<&str>,
    require_confirmation: bool,
    archive: bool,
) -> AppResult<Mutation<()>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let target = repo::require(&tx, cycle_id)?;
    crate::service::proposals::ensure_cycle_tree_unlocked(&tx, cycle_id)?;
    if let Some((code, message)) = deletion_guard(&tx, &target)? {
        return Err(AppError::conflict(code, message));
    }
    let impact = crate::service::deletion::cycle_impact(&tx, cycle_id)?;
    crate::service::deletion::require_confirmation(
        confirmation_token,
        &impact.token,
        require_confirmation,
    )?;
    // `ensure_cycle_tree_unlocked` covers the cycle subtree.  The explicit
    // pass also covers task descendants linked from another cycle.
    for task_id in &impact.task_ids {
        crate::service::tasks::ensure_task_editable(&tx, task_id, true)?;
    }
    if archive {
        let origin = match target.parent_id.as_deref() {
            Some(parent) => repo::require(&tx, parent)?.title,
            None => "Workspace".to_string(),
        };
        crate::service::trash::archive_in_tx(&tx, "cycle", cycle_id, &target.title, &origin, &impact)?;
    }
    let mut mutation = crate::service::deletion::prepare_deletion(&tx, &impact)?;
    repo::delete(&tx, cycle_id)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;

    mutation.trash_changed = archive;
    Ok(mutation)
}

pub fn reorder_sessions(
    db: &Db,
    day_cycle_id: &str,
    session_ids: &[String],
) -> AppResult<Mutation<()>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let day = repo::require(&tx, day_cycle_id)?;
    if day.cycle_type != CycleType::Day {
        return Err(AppError::validation(
            "invalid_parent_type",
            "Focus blocks are reordered inside a day cycle",
        ));
    }
    for id in session_ids {
        let session = repo::require(&tx, id)?;
        if session.parent_id.as_deref() != Some(day_cycle_id) {
            return Err(AppError::validation(
                "session_not_in_day",
                format!("session {id} does not belong to this day"),
            ));
        }
    }
    repo::reorder(&tx, session_ids)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(Mutation::new(()).touching_cycle(day_cycle_id))
}

/// Copies every visible uncompleted task of the previous dated sibling into
/// `cycle_id`, recording lineage via `copied_from_task_id`
/// (spec: 从上一周期复制未完成项). No previous cycle or nothing uncompleted
/// yields an empty result, not an error.
pub fn copy_uncompleted_from_previous(
    db: &Db,
    cycle_id: &str,
    now: i64,
) -> AppResult<Mutation<Vec<Task>>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let target = repo::require(&tx, cycle_id)?;
    if target.starts_on.is_none() {
        return Err(AppError::validation(
            "cycle_not_dated",
            "Copying needs a dated cycle",
        ));
    }
    let previous = match repo::previous_dated_sibling(&tx, &target)? {
        Some(prev) => prev,
        None => return Ok(Mutation::new(Vec::new()).touching_tasks(cycle_id)),
    };
    let source = tasks_repo::list_visible_by_cycle(&tx, &previous.id)?;
    // The trailing empty input row is a typing affordance, not content: it is
    // excluded from copy scope (design.md §5.1 空行不计入任务数量与复制范围).
    let to_copy: Vec<&Task> = source
        .iter()
        .filter(|t| !t.completed && !t.title.trim().is_empty())
        .collect();
    if to_copy.is_empty() {
        return Ok(Mutation::new(Vec::new()).touching_tasks(cycle_id));
    }

    // The editor keeps one real blank row as the typing affordance. It may
    // already exist when a new cycle is populated (for example, the editor
    // creates it before the user chooses "copy unfinished"). Since the task
    // tree is ordered by position, move those rows after the copied roots so
    // the affordance remains at the bottom of the list.
    // Include pending proposals while checking descendants: a committed blank
    // row with a proposed child is content, not a typing affordance.
    let target_tasks = tasks_repo::list_with_proposals_by_cycle(&tx, cycle_id)?;
    let target_parent_ids: std::collections::HashSet<String> = target_tasks
        .iter()
        .filter_map(|task| task.parent_id.clone())
        .collect();
    let target_empty_rows: Vec<Task> = target_tasks
        .into_iter()
        .filter(|task| {
            task.proposal.is_none()
                && task.parent_id.is_none()
                && is_empty_input_row(task)
                && !target_parent_ids.contains(&task.id)
        })
        .collect();
    let source_ids: std::collections::HashSet<&str> =
        source.iter().map(|t| t.id.as_str()).collect();
    let mut id_map: HashMap<String, String> = HashMap::new();
    let mut created: Vec<Task> = Vec::with_capacity(to_copy.len());
    let mut next_top_position = tasks_repo::max_position(&tx, cycle_id, None)? + 1;

    for original in to_copy {
        let new_parent = match &original.parent_id {
            None => None,
            Some(parent) => {
                if id_map.contains_key(parent) {
                    Some(id_map[parent].clone())
                } else if source_ids.contains(parent.as_str()) {
                    // Its parent was completed and not copied: keep the copy
                    // as a top-level item instead of a dangling reference.
                    None
                } else {
                    // Cross-cycle link (e.g. weekly item -> long-term goal):
                    // preserved so the copy still serves the same goal.
                    Some(parent.clone())
                }
            }
        };
        let position = if new_parent.is_none() {
            let p = next_top_position;
            next_top_position += 1;
            p
        } else {
            original.position
        };
        let new_id = uuid::Uuid::new_v4().to_string();
        let new_task = tasks_repo::NewTask {
            id: new_id.clone(),
            cycle_id: cycle_id.to_string(),
            parent_id: new_parent,
            title: original.title.clone(),
            subtasks: original.subtasks.clone(),
            position,
            completed: false,
            goal_breakdown: original.goal_breakdown.clone(),
            needs_refinement: original.needs_refinement,
            needs_breakdown: original.needs_breakdown,
            root_color_key: original.root_color_key.clone(),
            copied_from_task_id: Some(original.id.clone()),
            created_at: now,
        };
        tasks_repo::insert(&tx, &new_task)?;
        let created_task = tasks_repo::require(&tx, &new_id)?;
        id_map.insert(original.id.clone(), new_id);
        created.push(created_task);
    }
    // Cross-cycle links have a parent_id outside this cycle and therefore are
    // visual roots without matching `parent_id IS NULL`. Include every task
    // position so the input row follows those roots too.
    let mut next_empty_position = tasks_repo::list_with_proposals_by_cycle(&tx, cycle_id)?
        .iter()
        .map(|task| task.position)
        .max()
        .unwrap_or(-1)
        + 1;
    for empty_row in target_empty_rows {
        let mut update = tasks_repo::TaskUpdate::empty();
        update.position = Some(next_empty_position);
        next_empty_position += 1;
        tasks_repo::update(&tx, &empty_row.id, &update)?;
    }
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(Mutation::new(created).touching_tasks(cycle_id))
}

/// Shared helpers for sibling services.
pub fn require_cycle(conn: &Connection, cycle_id: &str) -> AppResult<Cycle> {
    repo::require(conn, cycle_id)
}

pub fn ensure_content_mutable(conn: &Connection, cycle_id: &str) -> AppResult<Cycle> {
    let cycle = repo::require(conn, cycle_id)?;
    ensure_cycle_mutable(&cycle)?;
    Ok(cycle)
}

/// Re-exported for tests of lifecycle ordering rules.
pub fn lifecycle_state_of(target: &Cycle) -> LifecycleState {
    target.lifecycle().unwrap_or(LifecycleState::Finished)
}

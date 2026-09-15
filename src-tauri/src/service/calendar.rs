//! Calendar-view use cases (change: add-calendar-time-view): the date-range
//! projection of day cycles, cross-date moves with explicit conflict
//! strategies, session scheduling on the timeline, and the daily time budget.
//!
//! Behaviour contract: `openspec/changes/add-calendar-time-view` (capability
//! `calendar-view`). Structural facts this module relies on:
//!
//! * a day cycle's date identity is `starts_on` (+ `calendar_key = day:date`),
//!   unique per date via `ux_cycles_calendar_key`;
//! * `cycles.scheduled_start_at` (migration 0008) is a session's planned
//!   wall-clock start, read and written through dedicated projections.
//!
//! Trade-off note: the schedule deliberately does NOT become a `Cycle` field.
//! Adding one would ripple through `Cycle` constructors, the row mapper and
//! every existing call site (the struct is built by hand in
//! `repository::cycles::row_to_cycle`), all outside this change's blast
//! radius; the calendar is the only reader today, so schedules travel in
//! their own query result instead ([`SessionSchedule`]).

use std::collections::HashMap;

use chrono::NaiveDate;
use rusqlite::Connection;

use crate::db::Db;
use crate::domain::calendar::{
    self, add_days, align_range_to_weeks, dates_between, day_key, format_date, parse_date,
    truncate_schedule_to_day, week_key,
};
use crate::domain::cycle::{Cycle, CycleType, DAY_DURATION_MS, WEEK_DURATION_MS};
use crate::error::{AppError, AppResult};
use crate::repository::{cycles as repo, settings as settings_repo};
use crate::service::{cycles as cycles_service, settings as settings_service, Mutation};

// --- shared projections ------------------------------------------------------

/// One focus block's planned wall-clock slot, already clamped to the local day
/// that contains `starts_at` (spec: 跨午夜按时长截断到当日并标注).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SessionSchedule {
    pub session_id: String,
    /// Day cycle the block hangs under (the timeline's day).
    pub day_cycle_id: String,
    /// Planned start, epoch milliseconds.
    pub starts_at: i64,
    /// Planned end, epoch milliseconds; `starts_at + duration` clamped to the
    /// local midnight that ends the day.
    pub ends_at: i64,
    /// Planned duration in milliseconds (the block's commitment, not the
    /// clamped span).
    pub duration_ms: i64,
    /// True when `starts_at + duration_ms` crossed midnight and `ends_at` was
    /// clamped to the day boundary.
    pub truncated: bool,
}

impl SessionSchedule {
    pub fn build(
        day_cycle_id: String,
        session_id: String,
        starts_at: i64,
        duration_ms: i64,
    ) -> Self {
        let (ends_at, truncated) = truncate_schedule_to_day(starts_at, duration_ms);
        Self {
            session_id,
            day_cycle_id,
            starts_at,
            ends_at,
            duration_ms,
            truncated,
        }
    }
}

/// A reported overlap pair (strict intervals: touching or zero-length spans
/// never overlap). Query result only — overlaps are never resolved
/// automatically (spec: 两者都保留并排显示为重叠).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ScheduleOverlap {
    /// The earlier-starting block of the pair.
    pub first: SessionSchedule,
    pub second: SessionSchedule,
}

/// One focus block in the calendar payload: the block plus its schedule, if
/// any. Blocks without a schedule belong in the timeline's staging area.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CalendarSession {
    pub session: Cycle,
    pub schedule: Option<SessionSchedule>,
}

/// One grid cell: a date, the day cycle living on it (if any) and its focus
/// blocks. Padding dates around the requested range are returned as cells too
/// so month/week grids never render a hole (spec: 保证网格不出现空洞).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CalendarDay {
    /// Local date, `YYYY-MM-DD`.
    pub date: String,
    /// Whether the date falls inside the range the caller asked for (padding
    /// cells are false and render dimmed).
    pub in_range: bool,
    pub day_cycle: Option<Cycle>,
    pub sessions: Vec<CalendarSession>,
}

/// `get_calendar_range` result: week-aligned cells for `[start, end]` plus
/// both bounds, so the frontend can tell padding from requested dates.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CalendarRange {
    pub start: String,
    pub end: String,
    pub grid_start: String,
    pub grid_end: String,
    pub week_start_day: i64,
    pub days: Vec<CalendarDay>,
}

// --- §1 range query ----------------------------------------------------------

/// Batch-reads the calendar projection for `[start, end]` (inclusive local
/// dates): every day cycle on those dates with its focus blocks, padded out to
/// whole weeks so grids render without holes. One round of queries — never
/// one per day.
pub fn get_calendar_range(db: &Db, start: &str, end: &str) -> AppResult<CalendarRange> {
    let start = parse_date(start)
        .ok_or_else(|| AppError::validation("invalid_date", "date must be YYYY-MM-DD"))?;
    let end = parse_date(end)
        .ok_or_else(|| AppError::validation("invalid_date", "date must be YYYY-MM-DD"))?;
    if start > end {
        return Err(AppError::validation(
            "invalid_range",
            "start must be on or before end",
        ));
    }

    let conn = db.pool().get()?;
    let week_start_day = settings_service::week_start_day_or_default(&conn)?;
    let (grid_start, grid_end) = align_range_to_weeks(start, end, week_start_day as u32);

    let day_cycles =
        repo::list_day_cycles_in_range(&conn, &format_date(grid_start), &format_date(grid_end))?;
    let day_ids: Vec<String> = day_cycles.iter().map(|c| c.id.clone()).collect();
    let mut cycle_by_date: HashMap<String, Cycle> = day_cycles
        .into_iter()
        .filter_map(|c| c.starts_on.clone().map(|d| (d, c)))
        .collect();
    let sessions = repo::list_sessions_by_days(&conn, &day_ids)?;
    let mut sessions_by_day: HashMap<String, Vec<Cycle>> = HashMap::new();
    for (day_id, session) in sessions {
        sessions_by_day.entry(day_id).or_default().push(session);
    }
    let scheduled = repo::list_scheduled_in_days(&conn, &day_ids)?;
    let schedule_by_session: HashMap<String, SessionSchedule> = scheduled
        .into_iter()
        .map(|row| {
            let schedule = SessionSchedule::build(
                row.day_cycle_id,
                row.session_id.clone(),
                row.starts_at,
                row.duration.unwrap_or(0),
            );
            (row.session_id, schedule)
        })
        .collect();

    let days = dates_between(grid_start, grid_end)
        .into_iter()
        .map(|date| {
            let date_str = format_date(date);
            let day_cycle = cycle_by_date.remove(&date_str);
            let sessions = match &day_cycle {
                Some(day) => sessions_by_day
                    .remove(&day.id)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|session| CalendarSession {
                        schedule: schedule_by_session.get(&session.id).cloned(),
                        session,
                    })
                    .collect(),
                None => Vec::new(),
            };
            CalendarDay {
                in_range: start <= date && date <= end,
                date: date_str,
                day_cycle,
                sessions,
            }
        })
        .collect();

    Ok(CalendarRange {
        start: format_date(start),
        end: format_date(end),
        grid_start: format_date(grid_start),
        grid_end: format_date(grid_end),
        week_start_day,
        days,
    })
}

// --- §2 cross-date moves -----------------------------------------------------

/// How to resolve a `calendar_key` collision when moving a day cycle onto an
/// occupied date. Chosen explicitly by the user — the system never silently
/// overwrites or discards a day (spec: MUST NOT 静默覆盖或丢弃).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MoveStrategy {
    /// Target date is empty: the source's content takes the date over.
    Move,
    /// Fold the source's entries into the target day, then remove the source.
    Merge,
    /// Exchange the two days' date identities.
    Swap,
}

impl MoveStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Move => "move",
            Self::Merge => "merge",
            Self::Swap => "swap",
        }
    }
}

/// What one cross-date move did, so the frontend can re-key its optimistic
/// state (move/merge replace the source cycle id with a surviving day id).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct MoveDayOutcome {
    pub strategy: &'static str,
    /// The cycle the drag started from.
    pub source_day_id: String,
    /// The day now holding the source's date/content (`source` itself for a
    /// swap, the surviving day for move/merge).
    pub target_day_id: String,
    /// Day cycles this move deleted (empty for swap).
    pub deleted_day_ids: Vec<String>,
}

/// Moves a day cycle to `target_date`. With no `strategy`, an empty target is
/// a plain move and an occupied target surfaces as `Conflict("target_exists")`
/// so the caller can ask the user for one (spec: 目标已有计划 → 显式选择).
/// Everything below runs in a single transaction: a rejected move leaves no
/// half-completed state.
pub fn move_day_cycle(
    db: &Db,
    cycle_id: &str,
    target_date: &str,
    strategy: Option<MoveStrategy>,
    now: i64,
) -> AppResult<Mutation<MoveDayOutcome>> {
    let target = parse_date(target_date)
        .ok_or_else(|| AppError::validation("invalid_date", "date must be YYYY-MM-DD"))?;
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let week_start_day = settings_service::week_start_day_or_default(&tx)?;

    let source = repo::require(&tx, cycle_id)?;
    if source.cycle_type != CycleType::Day {
        return Err(AppError::validation(
            "invalid_cycle_type",
            "Only day cycles can move across dates",
        ));
    }
    cycles_service::ensure_cycle_mutable(&source)?;
    let source_date = source
        .starts_on
        .as_deref()
        .and_then(parse_date)
        .ok_or_else(|| {
            AppError::validation("cycle_not_dated", "This day cycle has no date identity")
        })?;

    if source_date == target {
        // Dragging a day onto its own date: nothing to do, nothing to report.
        let outcome = MoveDayOutcome {
            strategy: MoveStrategy::Move.as_str(),
            source_day_id: source.id.clone(),
            target_day_id: source.id.clone(),
            deleted_day_ids: Vec::new(),
        };
        return Ok(Mutation::new(outcome).touching_cycle(source.id));
    }

    let target_day = repo::get_by_calendar_key(&tx, &day_key(target))?;
    let strategy = match (strategy, target_day.is_some()) {
        (Some(strategy), _) => strategy,
        (None, false) => MoveStrategy::Move,
        (None, true) => {
            return Err(AppError::conflict(
                "target_exists",
                "The target date already has a day cycle; choose merge or swap",
            ))
        }
    };

    let mutation = match strategy {
        MoveStrategy::Move => {
            if target_day.is_some() {
                return Err(AppError::conflict(
                    "target_exists",
                    "The target date already has a day cycle; choose merge or swap",
                ));
            }
            move_to_empty_day(&tx, &source, target, week_start_day as u32, now)?
        }
        MoveStrategy::Merge => {
            let target_day =
                target_day.ok_or_else(|| AppError::Internal("target day missing".into()))?;
            merge_days(&tx, &source, &target_day)?
        }
        MoveStrategy::Swap => {
            let target_day =
                target_day.ok_or_else(|| AppError::Internal("target day missing".into()))?;
            swap_days(&tx, &source, &target_day)?
        }
    };
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(mutation)
}

/// The discard guard: a day whose focus block is currently running must not
/// be silently merged away. Finished history moves freely — it is data, not a
/// live timer (spec: 已启动专注块所在的日子不可被静默合并).
fn ensure_no_running_session(conn: &Connection, day: &Cycle) -> AppResult<()> {
    if repo::count_running_sessions(conn, &day.id)? > 0 {
        return Err(AppError::conflict(
            "has_started_session",
            "This day has a running focus block and cannot be discarded",
        ));
    }
    Ok(())
}

/// Fold `source` into `target_day` (merge strategy, and the second half of a
/// move): tasks and focus blocks change owners, the source day is deleted.
fn merge_days(
    tx: &Connection,
    source: &Cycle,
    target_day: &Cycle,
) -> AppResult<Mutation<MoveDayOutcome>> {
    cycles_service::ensure_cycle_mutable(target_day)?;
    ensure_no_running_session(tx, source)?;

    // Column order of both days is captured before the ownership change so
    // the merged column reads "target first, then the source's entries in
    // their original relative order" (spec: 合并保持 position 顺序).
    let target_order: Vec<String> = repo::list_sessions_by_day(tx, &target_day.id)?
        .into_iter()
        .map(|s| s.id)
        .collect();
    let source_order: Vec<String> = repo::list_sessions_by_day(tx, &source.id)?
        .into_iter()
        .map(|s| s.id)
        .collect();

    repo::move_tasks_between_cycles(tx, &source.id, &target_day.id)?;
    repo::move_sessions_between_days(tx, &source.id, &target_day.id)?;
    let mut order = target_order.clone();
    order.extend(source_order.iter().cloned());
    repo::reorder(tx, &order)?;
    let source_parent = source.parent_id.clone();
    repo::delete(tx, &source.id)?;

    let mut mutation = Mutation::new(MoveDayOutcome {
        strategy: MoveStrategy::Merge.as_str(),
        source_day_id: source.id.clone(),
        target_day_id: target_day.id.clone(),
        deleted_day_ids: vec![source.id.clone()],
    });
    mutation.cycles.push(source.id.clone());
    mutation.cycles.push(target_day.id.clone());
    if let Some(parent) = source_parent {
        mutation.cycles.push(parent);
    }
    if let Some(parent) = target_day.parent_id.clone() {
        mutation.cycles.push(parent);
    }
    for id in order {
        mutation.cycles.push(id);
    }
    mutation.tasks.push(target_day.id.clone());
    Ok(mutation)
}

/// Move strategy onto an empty date: create the target day exactly the way
/// `service::cycles::get_or_create_day` does (covering week, else week under
/// the covering long-term cycle, then the day, then repeat materialization)
/// and fold the source into it. The parent-derivation logic is mirrored here
/// rather than reused in place: `get_or_create_day` owns its connection and
/// transaction, and this must complete inside the caller's single transaction.
fn move_to_empty_day(
    tx: &Connection,
    source: &Cycle,
    target: NaiveDate,
    week_start_day: u32,
    now: i64,
) -> AppResult<Mutation<MoveDayOutcome>> {
    ensure_no_running_session(tx, source)?;
    let target_str = format_date(target);

    let week = match find_covering_cycle(tx, CycleType::Week, &target_str)? {
        Some(week) => week,
        None => {
            let month =
                find_covering_cycle(tx, CycleType::Month, &target_str)?.ok_or_else(|| {
                    AppError::validation(
                        "no_covering_long_term_cycle",
                        "No active long-term cycle covers this date",
                    )
                })?;
            let (starts_on, ends_on) =
                calendar::dated_cycle_bounds(target, CycleType::Week, week_start_day, 0)
                    .ok_or_else(|| AppError::Internal("week bounds missing".into()))?;
            let new = repo::NewCycle {
                id: uuid::Uuid::new_v4().to_string(),
                title: format_date(starts_on),
                cycle_type: CycleType::Week,
                parent_id: Some(month.id.clone()),
                position: repo::max_position(tx, &month.id)? + 1,
                duration: Some(WEEK_DURATION_MS),
                starts_on: Some(format_date(starts_on)),
                ends_on: Some(format_date(ends_on)),
                calendar_key: Some(week_key(starts_on)),
                repeat_id: None,
                created_at: now,
            };
            repo::insert(tx, &new)?;
            repo::require(tx, &new.id)?
        }
    };

    let new = repo::NewCycle {
        id: uuid::Uuid::new_v4().to_string(),
        title: target_str.clone(),
        cycle_type: CycleType::Day,
        parent_id: Some(week.id.clone()),
        position: repo::max_position(tx, &week.id)? + 1,
        duration: Some(DAY_DURATION_MS),
        starts_on: Some(target_str),
        ends_on: Some(format_date(add_days(target, 1))),
        calendar_key: Some(day_key(target)),
        repeat_id: None,
        created_at: now,
    };
    repo::insert(tx, &new)?;
    let target_day = repo::require(tx, &new.id)?;
    // Repeat templates materialize on the newly claimed date, same as opening
    // that day in the workspace would (spec: 模板自动出现).
    let generated = crate::service::repeats::generate_for_day_in_tx(tx, &target_day.id, now)?;

    let mut mutation = merge_days(tx, source, &target_day)?;
    mutation.value.strategy = MoveStrategy::Move.as_str();
    for id in generated {
        mutation.cycles.push(id);
    }
    mutation.tasks.push(target_day.id.clone());
    Ok(mutation)
}

/// Swap strategy: the two days exchange date identities (and therefore
/// parents), each keeping its own content. The partial unique index only
/// ignores NULLs, so both keys are cleared before either new value is written
/// — all inside the caller's transaction (spec: 先置空再互换写回).
fn swap_days(
    tx: &Connection,
    source: &Cycle,
    target_day: &Cycle,
) -> AppResult<Mutation<MoveDayOutcome>> {
    cycles_service::ensure_cycle_mutable(target_day)?;

    let (source_key, source_start, source_end) = day_identity(source)?;
    let (target_key, target_start, target_end) = day_identity(target_day)?;

    // Clear both, then write the crossed identities.
    repo::set_day_identity(tx, &source.id, None, None, None)?;
    repo::set_day_identity(tx, &target_day.id, None, None, None)?;
    repo::set_day_identity(
        tx,
        &source.id,
        Some(&target_key),
        Some(&target_start),
        Some(&target_end),
    )?;
    repo::set_day_identity(
        tx,
        &target_day.id,
        Some(&source_key),
        Some(&source_start),
        Some(&source_end),
    )?;

    // Auto-derived titles are the plain date; keep them from going stale.
    // User-authored titles survive the swap untouched.
    if source.title == source_start {
        repo::set_title(tx, &source.id, &target_start)?;
    }
    if target_day.title == target_start {
        repo::set_title(tx, &target_day.id, &source_start)?;
    }

    let source_parent = source.parent_id.clone();
    let target_parent = target_day.parent_id.clone();
    if source_parent != target_parent {
        // Each day moves under the week that covers its new date.
        let new_source_parent = target_parent
            .clone()
            .ok_or_else(|| AppError::Internal("day cycle without a week parent".into()))?;
        let new_target_parent = source_parent
            .clone()
            .ok_or_else(|| AppError::Internal("day cycle without a week parent".into()))?;
        let source_position = repo::max_position(tx, &new_source_parent)? + 1;
        let target_position = repo::max_position(tx, &new_target_parent)? + 1;
        repo::reparent(tx, &source.id, &new_source_parent, source_position)?;
        repo::reparent(tx, &target_day.id, &new_target_parent, target_position)?;
    }

    let mut mutation = Mutation::new(MoveDayOutcome {
        strategy: MoveStrategy::Swap.as_str(),
        source_day_id: source.id.clone(),
        target_day_id: target_day.id.clone(),
        deleted_day_ids: Vec::new(),
    });
    mutation.cycles.push(source.id.clone());
    mutation.cycles.push(target_day.id.clone());
    // Both (possibly new) parents so both planner columns refresh.
    for parent in [source_parent, target_parent] {
        if let Some(parent) = parent {
            mutation.cycles.push(parent);
        }
    }
    Ok(mutation)
}

fn day_identity(day: &Cycle) -> AppResult<(String, String, String)> {
    match (
        day.calendar_key.clone(),
        day.starts_on.clone(),
        day.ends_on.clone(),
    ) {
        (Some(key), Some(start), Some(end)) => Ok((key, start, end)),
        _ => Err(AppError::validation(
            "cycle_not_dated",
            "This day cycle has no date identity",
        )),
    }
}

/// Mirrors `service::cycles::find_covering_cycle` (private there): the
/// earliest-created unfinished cycle of `kind` whose `[starts_on, ends_on)`
/// covers `date`. Kept structurally identical so the calendar's target-day
/// creation follows the same parent-derivation rule as `get_or_create_day`.
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
            _ => false,
        })
        .min_by_key(|c| c.created_at))
}

// --- §3 timeline scheduling --------------------------------------------------

/// Writes a focus block's planned slot: `starts_at` (epoch ms) plus optionally
/// its duration. Omitting `duration_ms` keeps the block's commitment — a
/// drag into the timeline never changes how long the block takes (spec:
/// 该专注块获得开始时间，时长保持不变). A started block can never change its
/// duration (same rule as `update_session`).
pub fn set_session_schedule(
    db: &Db,
    session_id: &str,
    starts_at: i64,
    duration_ms: Option<i64>,
) -> AppResult<Mutation<SessionSchedule>> {
    if starts_at < 0 {
        return Err(AppError::validation(
            "invalid_starts_at",
            "starts_at must be epoch milliseconds",
        ));
    }
    if let Some(duration) = duration_ms {
        if duration < 0 {
            return Err(AppError::validation(
                "invalid_duration",
                "duration_ms must be zero or positive",
            ));
        }
    }
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let session = repo::require(&tx, session_id)?;
    if session.cycle_type != CycleType::Session {
        return Err(AppError::validation(
            "invalid_cycle_type",
            "Schedules attach to focus blocks",
        ));
    }
    cycles_service::ensure_cycle_mutable(&session)?;
    if let Some(duration) = duration_ms {
        if session.started && Some(duration) != session.duration {
            return Err(AppError::conflict(
                "cycle_started",
                "A started focus block cannot change its duration",
            ));
        }
    }
    let duration = duration_ms.or(session.duration).ok_or_else(|| {
        AppError::validation(
            "schedule_duration_required",
            "This focus block has no duration yet; set one before scheduling it",
        )
    })?;
    let day_cycle_id = session
        .parent_id
        .clone()
        .ok_or_else(|| AppError::Internal("session without a day parent".into()))?;

    repo::set_session_schedule(&tx, session_id, starts_at, duration_ms)?;
    let schedule = SessionSchedule::build(
        day_cycle_id.clone(),
        session_id.to_string(),
        starts_at,
        duration,
    );
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;

    let mut mutation = Mutation::new(schedule);
    mutation.cycles.push(session_id.to_string());
    mutation.cycles.push(day_cycle_id);
    Ok(mutation)
}

/// All overlapping schedule pairs of one day, as a query result. Nothing is
/// moved, merged or deleted: conflicting blocks both stay and the timeline
/// renders them as overlapping (spec: 时间冲突).
pub fn get_schedule_overlaps(db: &Db, day_cycle_id: &str) -> AppResult<Vec<ScheduleOverlap>> {
    let conn = db.pool().get()?;
    let day = repo::require(&conn, day_cycle_id)?;
    if day.cycle_type != CycleType::Day {
        return Err(AppError::validation(
            "invalid_parent_type",
            "Schedules are detected within a day cycle",
        ));
    }
    let schedules: Vec<SessionSchedule> = repo::list_scheduled_in_day(&conn, day_cycle_id)?
        .into_iter()
        .map(|row| {
            SessionSchedule::build(
                row.day_cycle_id,
                row.session_id.clone(),
                row.starts_at,
                row.duration.unwrap_or(0),
            )
        })
        .collect();

    // Days hold a handful of blocks; the quadratic sweep keeps the rule
    // (strict interval overlap) the one thing to read.
    let mut overlaps = Vec::new();
    for i in 0..schedules.len() {
        for j in (i + 1)..schedules.len() {
            let a = &schedules[i];
            let b = &schedules[j];
            if a.starts_at < b.ends_at && b.starts_at < a.ends_at {
                overlaps.push(ScheduleOverlap {
                    first: a.clone(),
                    second: b.clone(),
                });
            }
        }
    }
    Ok(overlaps)
}

// --- §4 time budget ----------------------------------------------------------

/// app_settings key for the daily focus budget. The setting lives here rather
/// than in `service::settings` so the calendar capability owns its whole
/// surface (service/settings.rs is not part of this change's blast radius).
const KEY_DAILY_CAPACITY_MINUTES: &str = "daily_capacity_minutes";

/// Reads the configured daily capacity in minutes; `None` when unset (the
/// budget bar stays hidden — spec: 未设置时不显示预算).
pub fn get_daily_capacity_minutes(conn: &Connection) -> AppResult<Option<i64>> {
    Ok(settings_repo::get(conn, KEY_DAILY_CAPACITY_MINUTES)?
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|minutes| (1..=MAX_DAILY_CAPACITY_MINUTES).contains(minutes)))
}

const MAX_DAILY_CAPACITY_MINUTES: i64 = 24 * 60;

/// Persists the daily capacity. `None` clears it: the settings repository has
/// no delete, so an empty value is stored and every reader treats it as unset.
pub fn set_daily_capacity_minutes(db: &Db, minutes: Option<i64>) -> AppResult<()> {
    let conn = db.pool().get()?;
    match minutes {
        Some(minutes) => {
            if !(1..=MAX_DAILY_CAPACITY_MINUTES).contains(&minutes) {
                return Err(AppError::validation(
                    "invalid_capacity",
                    "daily_capacity_minutes must be between 1 and 1440",
                ));
            }
            settings_repo::set(&conn, KEY_DAILY_CAPACITY_MINUTES, &minutes.to_string())
        }
        None => settings_repo::set(&conn, KEY_DAILY_CAPACITY_MINUTES, ""),
    }
}

/// One day's budget: capacity (None = unset) versus the total scheduled focus
/// time, which counts every scheduled block — including ones that have not
/// started yet (spec: 含未开始已排). Days without a day cycle simply have
/// zero scheduled.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TimeBudget {
    pub date: String,
    pub capacity_minutes: Option<i64>,
    pub scheduled_minutes: i64,
}

pub fn get_time_budget(db: &Db, date: &str) -> AppResult<TimeBudget> {
    let date = parse_date(date)
        .ok_or_else(|| AppError::validation("invalid_date", "date must be YYYY-MM-DD"))?;
    let conn = db.pool().get()?;
    let capacity_minutes = get_daily_capacity_minutes(&conn)?;
    let scheduled_minutes = match repo::get_by_calendar_key(&conn, &day_key(date))? {
        Some(day) => repo::sum_scheduled_duration(&conn, &day.id)? / 60_000,
        None => 0,
    };
    Ok(TimeBudget {
        date: format_date(date),
        capacity_minutes,
        scheduled_minutes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::tasks as tasks_repo;
    use crate::service::cycles::{AddSessionArgs, CreateCycleArgs};
    use crate::service::tasks::{add_task, AddTaskArgs};

    /// Wednesday 2026-09-16, matching tests/common (unit tests cannot import
    /// the integration-test crate).
    const TODAY: &str = "2026-09-16";
    const NOW: i64 = 1_700_000_000_000;

    struct Fixture {
        db: Db,
        _dir: tempfile::TempDir,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = crate::db::open_at(&dir.path().join("calendar.db")).expect("db");
        Fixture { db, _dir: dir }
    }

    fn create_long_term(db: &Db, start: &str, months: i64) -> Cycle {
        let today = parse_date(start).expect("date");
        cycles_service::create_planning_cycle(
            db,
            &CreateCycleArgs {
                cycle_type: "month".into(),
                duration_months: Some(months),
                ..Default::default()
            },
            today,
            NOW,
        )
        .expect("long-term created")
        .value
    }

    fn create_week(db: &Db, parent_id: &str, anchor: &str) -> Cycle {
        let today = parse_date(anchor).expect("date");
        cycles_service::create_planning_cycle(
            db,
            &CreateCycleArgs {
                cycle_type: "week".into(),
                parent_id: Some(parent_id.into()),
                ..Default::default()
            },
            today,
            NOW,
        )
        .expect("week created")
        .value
    }

    fn create_day(db: &Db, parent_id: &str, date: &str) -> Cycle {
        cycles_service::create_planning_cycle(
            db,
            &CreateCycleArgs {
                cycle_type: "day".into(),
                parent_id: Some(parent_id.into()),
                date: Some(date.into()),
                ..Default::default()
            },
            parse_date(TODAY).expect("date"),
            NOW,
        )
        .expect("day created")
        .value
    }

    fn add_block(db: &Db, day_id: &str, title: &str, minutes: i64) -> Cycle {
        cycles_service::add_session(
            db,
            &AddSessionArgs {
                day_cycle_id: day_id.into(),
                title: title.into(),
                duration_ms: Some(minutes * 60_000),
                position: None,
            },
            NOW,
        )
        .expect("session added")
        .value
    }

    fn add_day_task(db: &Db, day_id: &str, title: &str) -> crate::domain::task::Task {
        add_task(
            db,
            &AddTaskArgs {
                cycle_id: day_id.into(),
                title: title.into(),
                ..Default::default()
            },
            NOW,
        )
        .expect("task added")
        .value
    }

    fn session_titles(db: &Db, day_id: &str) -> Vec<String> {
        let conn = db.pool().get().expect("conn");
        repo::list_sessions_by_day(&conn, day_id)
            .expect("sessions")
            .into_iter()
            .map(|s| s.title)
            .collect()
    }

    fn day_at(db: &Db, date: &str) -> Option<Cycle> {
        let conn = db.pool().get().expect("conn");
        repo::get_by_calendar_key(&conn, &day_key(parse_date(date).expect("date"))).expect("lookup")
    }

    fn conflict_code<T>(result: AppResult<T>) -> String {
        match result {
            Err(AppError::Conflict { code, .. }) => code,
            Err(other) => panic!("expected Conflict, got {other:?}"),
            Ok(_) => panic!("expected Conflict, got Ok"),
        }
    }

    fn validation_code<T>(result: AppResult<T>) -> String {
        match result {
            Err(AppError::Validation { code, .. }) => code,
            Err(other) => panic!("expected Validation, got {other:?}"),
            Ok(_) => panic!("expected Validation, got Ok"),
        }
    }

    /// One long-term cycle (2026-09-14 … 2026-10-12) with one week
    /// (2026-09-14 … 2026-09-20).
    fn fixture_with_week() -> (Fixture, Cycle, Cycle) {
        let f = fixture();
        let month = create_long_term(&f.db, "2026-09-14", 1);
        let week = create_week(&f.db, &month.id, "2026-09-14");
        (f, month, week)
    }

    // --- §1 range query ------------------------------------------------------

    #[test]
    fn range_covers_cross_month_grid_without_holes() {
        let (f, _month, week) = fixture_with_week();
        let late = create_day(&f.db, &week.id, "2026-09-30");
        let early = create_day(&f.db, &week.id, "2026-10-01");
        add_block(&f.db, &late.id, "a", 30);
        add_block(&f.db, &early.id, "b", 30);

        // A range fully inside October still pads back into September.
        let range = get_calendar_range(&f.db, "2026-10-01", "2026-10-31").expect("range");
        assert_eq!(range.grid_start, "2026-09-28");
        assert_eq!(range.grid_end, "2026-11-01");
        assert_eq!(range.days.len(), 35, "5 whole weeks, no holes");
        assert!(!range.days.first().expect("first").in_range);
        assert!(!range.days.last().expect("last").in_range);

        let by_date: HashMap<String, &CalendarDay> =
            range.days.iter().map(|d| (d.date.clone(), d)).collect();
        let sep30 = by_date.get("2026-09-30").expect("sep 30 cell");
        assert!(sep30.day_cycle.is_some() && sep30.day_cycle.as_ref().unwrap().id == late.id);
        assert_eq!(sep30.sessions.len(), 1);
        assert!(!sep30.in_range, "padding date is outside the request");
        let oct1 = by_date.get("2026-10-01").expect("oct 1 cell");
        assert_eq!(oct1.sessions.len(), 1);
        assert_eq!(oct1.day_cycle.as_ref().expect("oct 1 day").id, early.id);
        assert!(oct1.in_range);
        // Every requested date has a cell — the grid cannot have holes.
        assert_eq!(
            range.days.iter().filter(|d| d.in_range).count(),
            31,
            "all 31 October days present"
        );
    }

    #[test]
    fn range_with_no_day_cycles_returns_empty_cells() {
        let (f, _month, _week) = fixture_with_week();
        let range = get_calendar_range(&f.db, "2026-09-14", "2026-09-20").expect("range");
        assert_eq!(range.days.len(), 7);
        assert!(
            range
                .days
                .iter()
                .all(|d| d.day_cycle.is_none() && d.sessions.is_empty()),
            "every date is an empty cell, none missing"
        );
    }

    #[test]
    fn range_single_day_multiple_blocks_keeps_column_order() {
        let (f, _month, week) = fixture_with_week();
        let day = create_day(&f.db, &week.id, TODAY);
        add_block(&f.db, &day.id, "first", 30);
        add_block(&f.db, &day.id, "second", 45);
        add_block(&f.db, &day.id, "third", 0);

        let range = get_calendar_range(&f.db, TODAY, TODAY).expect("range");
        assert_eq!(range.days.len(), 7, "one day pads to one whole week");
        let cell = range
            .days
            .iter()
            .find(|d| d.date == TODAY)
            .expect("today cell");
        assert!(cell.in_range);
        let titles: Vec<&str> = cell
            .sessions
            .iter()
            .map(|s| s.session.title.as_str())
            .collect();
        assert_eq!(titles, ["first", "second", "third"]);
    }

    #[test]
    fn range_rejects_reversed_and_malformed_bounds() {
        let (f, _month, _week) = fixture_with_week();
        assert_eq!(
            validation_code(get_calendar_range(&f.db, "2026-09-20", "2026-09-14")),
            "invalid_range"
        );
        assert_eq!(
            validation_code(get_calendar_range(&f.db, "nope", "2026-09-14")),
            "invalid_date"
        );
    }

    // --- §2 cross-date moves -------------------------------------------------

    #[test]
    fn move_to_empty_date_creates_target_day_and_moves_content() {
        let (f, month, week) = fixture_with_week();
        let source = create_day(&f.db, &week.id, "2026-09-15");
        add_block(&f.db, &source.id, "block-1", 30);
        add_block(&f.db, &source.id, "block-2", 45);
        add_day_task(&f.db, &source.id, "task-1");

        let mutation =
            move_day_cycle(&f.db, &source.id, "2026-09-16", None, NOW).expect("move succeeds");
        let outcome = mutation.value;
        assert_eq!(outcome.strategy, "move");
        assert_eq!(outcome.deleted_day_ids, vec![source.id.clone()]);
        assert_ne!(outcome.target_day_id, source.id);

        // The date now holds a day cycle with the moved content, in order.
        let target = day_at(&f.db, "2026-09-16").expect("target day exists");
        assert_eq!(target.id, outcome.target_day_id);
        assert_eq!(target.parent_id.as_deref(), Some(week.id.as_str()));
        assert_eq!(session_titles(&f.db, &target.id), ["block-1", "block-2"]);
        let conn = f.db.pool().get().expect("conn");
        let tasks = tasks_repo::list_visible_by_cycle(&conn, &target.id).expect("tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "task-1");
        // The source date is empty again; the old cycle is gone.
        assert!(day_at(&f.db, "2026-09-15").is_none());
        assert!(repo::get(&conn, &source.id).expect("get").is_none());
        // Still inside the same week/month structure.
        assert_eq!(
            month.id,
            repo::get(&conn, &week.id)
                .expect("get")
                .unwrap()
                .parent_id
                .unwrap()
        );
    }

    #[test]
    fn move_creates_missing_week_under_covering_long_term() {
        let (f, month, week) = fixture_with_week();
        let source = create_day(&f.db, &week.id, "2026-09-15");
        add_block(&f.db, &source.id, "block", 30);

        // 2026-09-23 belongs to the following week, which does not exist yet.
        let mutation = move_day_cycle(
            &f.db,
            &source.id,
            "2026-09-23",
            Some(MoveStrategy::Move),
            NOW,
        )
        .expect("move succeeds");
        let target = day_at(&f.db, "2026-09-23").expect("target day exists");
        assert_eq!(target.id, mutation.value.target_day_id);
        let conn = f.db.pool().get().expect("conn");
        let new_week =
            repo::require(&conn, target.parent_id.as_deref().expect("parent")).expect("week");
        assert_eq!(new_week.cycle_type, CycleType::Week);
        assert_eq!(new_week.parent_id.as_deref(), Some(month.id.as_str()));
        assert_eq!(new_week.starts_on.as_deref(), Some("2026-09-21"));
        assert_eq!(session_titles(&f.db, &target.id), ["block"]);
    }

    #[test]
    fn move_without_covering_long_term_is_rejected_without_side_effects() {
        let (f, _month, week) = fixture_with_week();
        let source = create_day(&f.db, &week.id, "2026-09-15");
        add_block(&f.db, &source.id, "block", 30);

        // 2026-10-15 lies past the long-term cycle's end (2026-10-12).
        let error = move_day_cycle(&f.db, &source.id, "2026-10-15", None, NOW)
            .expect_err("no covering long-term cycle");
        assert_eq!(
            validation_code::<()>(Err(error)),
            "no_covering_long_term_cycle"
        );
        // Nothing half-created: the source is intact at its old date.
        assert_eq!(
            day_at(&f.db, "2026-09-15").expect("source intact").id,
            source.id
        );
        assert!(day_at(&f.db, "2026-10-15").is_none());
        assert_eq!(session_titles(&f.db, &source.id), ["block"]);
    }

    #[test]
    fn occupied_target_without_strategy_conflicts() {
        let (f, _month, week) = fixture_with_week();
        let source = create_day(&f.db, &week.id, "2026-09-15");
        let _target = create_day(&f.db, &week.id, "2026-09-16");

        assert_eq!(
            conflict_code(move_day_cycle(&f.db, &source.id, "2026-09-16", None, NOW)),
            "target_exists"
        );
        // An explicit "move" onto an occupied date is the same conflict: move
        // needs an empty target by definition.
        assert_eq!(
            conflict_code(move_day_cycle(
                &f.db,
                &source.id,
                "2026-09-16",
                Some(MoveStrategy::Move),
                NOW
            )),
            "target_exists"
        );
    }

    #[test]
    fn merge_folds_source_into_target_in_position_order_and_deletes_source() {
        let (f, _month, week) = fixture_with_week();
        let source = create_day(&f.db, &week.id, "2026-09-15");
        add_block(&f.db, &source.id, "s-1", 30);
        add_block(&f.db, &source.id, "s-2", 45);
        add_day_task(&f.db, &source.id, "source-task");
        let target = create_day(&f.db, &week.id, "2026-09-16");
        add_block(&f.db, &target.id, "t-1", 30);
        add_day_task(&f.db, &target.id, "target-task");

        let mutation = move_day_cycle(
            &f.db,
            &source.id,
            "2026-09-16",
            Some(MoveStrategy::Merge),
            NOW,
        )
        .expect("merge succeeds");
        let outcome = mutation.value;
        assert_eq!(outcome.strategy, "merge");
        assert_eq!(outcome.target_day_id, target.id);

        // Target column: its own blocks first, then the source's, both in
        // their original relative order; positions renumbered without gaps.
        let conn = f.db.pool().get().expect("conn");
        let merged = repo::list_sessions_by_day(&conn, &target.id).expect("sessions");
        let titles: Vec<&str> = merged.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["t-1", "s-1", "s-2"]);
        assert_eq!(
            merged.iter().map(|s| s.position).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        // Tasks keep the same rule: target's first, source's appended after.
        let tasks = tasks_repo::list_visible_by_cycle(&conn, &target.id).expect("tasks");
        let task_titles: Vec<&str> = tasks.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(task_titles, ["target-task", "source-task"]);
        assert!(tasks[0].position < tasks[1].position);

        // The source day is gone; the merge completed as a whole.
        assert!(repo::get(&conn, &source.id).expect("get").is_none());
        assert!(day_at(&f.db, "2026-09-15").is_none());
    }

    #[test]
    fn merge_with_running_block_conflicts_and_leaves_no_half_state() {
        let (f, _month, week) = fixture_with_week();
        let source = create_day(&f.db, &week.id, "2026-09-15");
        let running = add_block(&f.db, &source.id, "running", 30);
        add_day_task(&f.db, &source.id, "source-task");
        let target = create_day(&f.db, &week.id, "2026-09-16");
        add_block(&f.db, &target.id, "t-1", 30);
        cycles_service::start_cycle(&f.db, &running.id, NOW).expect("started");

        assert_eq!(
            conflict_code(move_day_cycle(
                &f.db,
                &source.id,
                "2026-09-16",
                Some(MoveStrategy::Merge),
                NOW
            )),
            "has_started_session"
        );
        // The rejected merge changed nothing: entries stay on their days.
        assert_eq!(session_titles(&f.db, &source.id), ["running"]);
        assert_eq!(session_titles(&f.db, &target.id), ["t-1"]);
        let conn = f.db.pool().get().expect("conn");
        assert_eq!(
            tasks_repo::list_visible_by_cycle(&conn, &source.id)
                .expect("tasks")
                .len(),
            1
        );
        assert_eq!(
            tasks_repo::list_visible_by_cycle(&conn, &target.id)
                .expect("tasks")
                .len(),
            0
        );
    }

    #[test]
    fn merge_into_ended_day_conflicts() {
        let (f, _month, week) = fixture_with_week();
        let source = create_day(&f.db, &week.id, "2026-09-15");
        add_block(&f.db, &source.id, "s-1", 30);
        let target = create_day(&f.db, &week.id, "2026-09-16");
        add_block(&f.db, &target.id, "t-1", 30);
        // End the target day (start then finish — its block runs first).
        let block = add_block(&f.db, &target.id, "t-2", 30);
        cycles_service::start_cycle(&f.db, &block.id, NOW).expect("start block");
        cycles_service::start_cycle(&f.db, &target.id, NOW).expect("start day");
        cycles_service::finish_cycle(&f.db, &block.id, NOW).expect("finish block");
        cycles_service::finish_cycle(&f.db, &target.id, NOW).expect("finish day");

        assert_eq!(
            conflict_code(move_day_cycle(
                &f.db,
                &source.id,
                "2026-09-16",
                Some(MoveStrategy::Merge),
                NOW
            )),
            "cycle_ended"
        );
    }

    #[test]
    fn move_is_guarded_like_merge_because_it_also_discards_the_source_day() {
        let (f, _month, week) = fixture_with_week();
        let source = create_day(&f.db, &week.id, "2026-09-15");
        let running = add_block(&f.db, &source.id, "running", 30);
        cycles_service::start_cycle(&f.db, &running.id, NOW).expect("started");

        assert_eq!(
            conflict_code(move_day_cycle(&f.db, &source.id, "2026-09-16", None, NOW)),
            "has_started_session"
        );
        assert!(
            day_at(&f.db, "2026-09-16").is_none(),
            "no target day half-created"
        );
    }

    #[test]
    fn swap_exchanges_date_identities_within_a_week() {
        let (f, _month, week) = fixture_with_week();
        let source = create_day(&f.db, &week.id, "2026-09-15");
        add_block(&f.db, &source.id, "s-1", 30);
        let target = create_day(&f.db, &week.id, "2026-09-17");
        add_block(&f.db, &target.id, "t-1", 30);

        let mutation = move_day_cycle(
            &f.db,
            &source.id,
            "2026-09-17",
            Some(MoveStrategy::Swap),
            NOW,
        )
        .expect("swap succeeds");
        assert_eq!(mutation.value.strategy, "swap");
        assert!(mutation.value.deleted_day_ids.is_empty());

        // Each day keeps its content and its id; only the date identity moved.
        let now_on_15 = day_at(&f.db, "2026-09-15").expect("day at 09-15");
        let now_on_17 = day_at(&f.db, "2026-09-17").expect("day at 09-17");
        assert_eq!(now_on_15.id, target.id);
        assert_eq!(now_on_17.id, source.id);
        assert_eq!(session_titles(&f.db, &now_on_15.id), ["t-1"]);
        assert_eq!(session_titles(&f.db, &now_on_17.id), ["s-1"]);
        // Both still live under the same week; auto-derived titles follow.
        assert_eq!(now_on_15.parent_id.as_deref(), Some(week.id.as_str()));
        assert_eq!(now_on_17.parent_id.as_deref(), Some(week.id.as_str()));
        assert_eq!(now_on_17.title, "2026-09-17");
        assert_eq!(now_on_15.title, "2026-09-15");
    }

    #[test]
    fn swap_across_weeks_reparents_both_days() {
        let (f, month, week1) = fixture_with_week();
        let week2 = create_week(&f.db, &month.id, "2026-09-21");
        let source = create_day(&f.db, &week1.id, "2026-09-15");
        let target = create_day(&f.db, &week2.id, "2026-09-22");

        move_day_cycle(
            &f.db,
            &source.id,
            "2026-09-22",
            Some(MoveStrategy::Swap),
            NOW,
        )
        .expect("swap succeeds");

        let on_15 = day_at(&f.db, "2026-09-15").expect("day at 09-15");
        let on_22 = day_at(&f.db, "2026-09-22").expect("day at 09-22");
        assert_eq!(on_15.id, target.id, "the old 09-22 day now owns 09-15");
        assert_eq!(on_22.id, source.id);
        // Each day moved under the week that covers its new date.
        assert_eq!(on_15.parent_id.as_deref(), Some(week1.id.as_str()));
        assert_eq!(on_22.parent_id.as_deref(), Some(week2.id.as_str()));
    }

    #[test]
    fn moving_a_day_onto_itself_is_a_no_op() {
        let (f, _month, week) = fixture_with_week();
        let source = create_day(&f.db, &week.id, "2026-09-15");
        add_block(&f.db, &source.id, "s-1", 30);
        let mutation = move_day_cycle(&f.db, &source.id, "2026-09-15", None, NOW).expect("no-op");
        assert_eq!(mutation.value.target_day_id, source.id);
        assert_eq!(session_titles(&f.db, &source.id), ["s-1"]);
    }

    // --- §3 timeline scheduling ----------------------------------------------

    #[test]
    fn schedule_overlap_detection_reports_pairs_without_touching_data() {
        let (f, _month, week) = fixture_with_week();
        let day = create_day(&f.db, &week.id, TODAY);
        let s1 = add_block(&f.db, &day.id, "s1", 30);
        let s2 = add_block(&f.db, &day.id, "s2", 45);
        let zero = add_block(&f.db, &day.id, "zero", 0);
        let late = add_block(&f.db, &day.id, "late", 120);

        let (day_start, day_end) = calendar::local_day_bounds_ms(NOW);
        let nine = day_start + 9 * 3_600_000;
        set_session_schedule(&f.db, &s1.id, nine, None).expect("schedule s1");
        set_session_schedule(&f.db, &s2.id, nine + 15 * 60_000, None).expect("schedule s2");
        // Zero-length block parked exactly on s1's start: never an overlap.
        set_session_schedule(&f.db, &zero.id, nine, Some(0)).expect("schedule zero");
        // Cross-midnight: starts 23:00 for two hours, clipped to the day.
        let late_schedule = set_session_schedule(&f.db, &late.id, day_end - 3_600_000, None)
            .expect("schedule late");

        assert!(late_schedule.value.truncated);
        assert_eq!(late_schedule.value.ends_at, day_end);
        assert_eq!(late_schedule.value.duration_ms, 120 * 60_000);
        // The untruncated neighbours keep their exact end and flag.
        assert!(is_scheduled(&f.db, &s1.id));

        let overlaps = get_schedule_overlaps(&f.db, &day.id).expect("overlaps");
        assert_eq!(overlaps.len(), 1, "only s1/s2 overlap");
        assert_eq!(overlaps[0].first.session_id, s1.id);
        assert_eq!(overlaps[0].second.session_id, s2.id);

        // A pure query: reading overlaps twice changes nothing.
        let again = get_schedule_overlaps(&f.db, &day.id).expect("overlaps");
        assert_eq!(overlaps, again);
    }

    fn is_scheduled(db: &Db, session_id: &str) -> bool {
        let conn = db.pool().get().expect("conn");
        let raw: Option<i64> = conn
            .query_row(
                "SELECT scheduled_start_at FROM cycles WHERE id = ?1",
                [session_id],
                |r| r.get(0),
            )
            .expect("schedule read");
        raw.is_some()
    }

    #[test]
    fn schedule_keeps_duration_and_enforces_started_rules() {
        let (f, _month, week) = fixture_with_week();
        let day = create_day(&f.db, &week.id, TODAY);
        let block = add_block(&f.db, &day.id, "block", 30);
        let (day_start, _) = calendar::local_day_bounds_ms(NOW);

        // No duration anywhere: scheduling is rejected instead of guessing.
        let naked = cycles_service::add_session(
            &f.db,
            &AddSessionArgs {
                day_cycle_id: day.id.clone(),
                title: "naked".into(),
                duration_ms: None,
                position: None,
            },
            NOW,
        )
        .expect("session")
        .value;
        assert_eq!(
            validation_code(set_session_schedule(&f.db, &naked.id, day_start, None)),
            "schedule_duration_required"
        );

        // Omitting duration keeps the block's commitment and only moves start.
        let mutation =
            set_session_schedule(&f.db, &block.id, day_start + 3_600_000, None).expect("schedule");
        assert_eq!(mutation.value.duration_ms, 30 * 60_000);

        // A started block may move but may not change its duration.
        cycles_service::start_cycle(&f.db, &block.id, NOW).expect("start");
        assert_eq!(
            conflict_code(set_session_schedule(
                &f.db,
                &block.id,
                day_start + 3_600_000,
                Some(45 * 60_000)
            )),
            "cycle_started"
        );
        set_session_schedule(&f.db, &block.id, day_start + 4 * 3_600_000, None)
            .expect("moving a started block is fine");

        // Ended pages are review-only.
        cycles_service::finish_cycle(&f.db, &block.id, NOW).expect("finish");
        assert_eq!(
            conflict_code(set_session_schedule(&f.db, &block.id, day_start, None)),
            "cycle_ended"
        );
    }

    // --- §4 time budget ------------------------------------------------------

    #[test]
    fn budget_covers_unset_under_and_over_states() {
        let (f, _month, week) = fixture_with_week();
        let day = create_day(&f.db, &week.id, TODAY);

        // Unset: capacity is None and a day without blocks has zero planned.
        let budget = get_time_budget(&f.db, TODAY).expect("budget");
        assert_eq!(budget.capacity_minutes, None);
        assert_eq!(budget.scheduled_minutes, 0);

        set_daily_capacity_minutes(&f.db, Some(120)).expect("set capacity");
        let (day_start, _) = calendar::local_day_bounds_ms(NOW);
        let first = add_block(&f.db, &day.id, "first", 30);
        let second = add_block(&f.db, &day.id, "second", 45);
        set_session_schedule(&f.db, &first.id, day_start, None).expect("schedule");
        set_session_schedule(&f.db, &second.id, day_start + 3_600_000, None).expect("schedule");

        // Under: 75 of 120 minutes planned.
        let budget = get_time_budget(&f.db, TODAY).expect("budget");
        assert_eq!(budget.capacity_minutes, Some(120));
        assert_eq!(budget.scheduled_minutes, 75);

        // Over: a not-yet-started but scheduled block counts too (spec: 含未开始已排).
        let third = add_block(&f.db, &day.id, "third", 60);
        set_session_schedule(&f.db, &third.id, day_start + 5 * 3_600_000, None).expect("schedule");
        let budget = get_time_budget(&f.db, TODAY).expect("budget");
        assert_eq!(budget.scheduled_minutes, 135);
        assert!(budget.scheduled_minutes > budget.capacity_minutes.unwrap());

        // Clearing returns to the unset presentation.
        set_daily_capacity_minutes(&f.db, None).expect("clear capacity");
        assert_eq!(
            get_time_budget(&f.db, TODAY)
                .expect("budget")
                .capacity_minutes,
            None
        );
        assert_eq!(
            validation_code(set_daily_capacity_minutes(&f.db, Some(0))),
            "invalid_capacity"
        );
    }

    #[test]
    fn budget_for_a_date_without_day_cycle_is_zero() {
        let (f, _month, _week) = fixture_with_week();
        set_daily_capacity_minutes(&f.db, Some(90)).expect("set capacity");
        let budget = get_time_budget(&f.db, "2026-10-10").expect("budget");
        assert_eq!(budget.capacity_minutes, Some(90));
        assert_eq!(budget.scheduled_minutes, 0);
    }
}

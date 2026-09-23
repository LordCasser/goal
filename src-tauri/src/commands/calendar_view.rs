//! Calendar-view commands (change: add-calendar-time-view).
//!
//! Same contract as every other command module: validate argument shapes,
//! call `service::calendar`, and turn the returned [`Mutation`] into event
//! emissions. The mutating commands (`move_day_cycle`,
//! `set_session_schedule`) emit `cycles:changed` so the workspace and the
//! calendar refresh from the same database truth.

use tauri::State;

use crate::db::Db;
use crate::error::AppResult;
use crate::service::calendar::{
    self, CalendarRange, MoveStrategy, ScheduleOverlap, SessionSchedule, TimeBudget,
};

use super::emit_mutation;

/// Day cycles and focus blocks for `[start, end]`, padded to whole weeks.
#[tauri::command]
pub fn get_calendar_range(
    db: State<'_, Db>,
    start: String,
    end: String,
) -> AppResult<CalendarRange> {
    calendar::get_calendar_range(&db, &start, &end)
}

/// Moves a day cycle to `target_date`; `strategy` resolves an occupied target
/// (`move` / `merge` / `swap`, lowercase). Omitting it on an occupied target
/// surfaces as `Conflict("target_exists")` so the UI can ask the user.
#[tauri::command]
pub fn move_day_cycle(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    cycle_id: String,
    target_date: String,
    strategy: Option<MoveStrategy>,
) -> AppResult<calendar::MoveDayOutcome> {
    let mutation = calendar::move_day_cycle(
        &db,
        &cycle_id,
        &target_date,
        strategy,
        crate::service::now_ms(),
    )?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

/// Places a focus block on the timeline (epoch-ms start; duration optional
/// and kept when omitted). A null start moves it back to Unscheduled.
#[tauri::command]
pub fn set_session_schedule(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    session_id: String,
    starts_at: Option<i64>,
    duration_ms: Option<i64>,
) -> AppResult<Option<SessionSchedule>> {
    let mutation = calendar::set_session_schedule(&db, &session_id, starts_at, duration_ms)?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

#[tauri::command]
pub fn delete_day_focus_blocks(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    day_cycle_id: String,
    expected_ids: Vec<String>,
) -> AppResult<usize> {
    let mutation = crate::service::cycles::delete_day_focus_blocks(&db, &day_cycle_id, &expected_ids)?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

#[tauri::command]
pub fn clear_day_schedules(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    day_cycle_id: String,
    expected_ids: Vec<String>,
) -> AppResult<usize> {
    let mutation = calendar::clear_day_schedules(&db, &day_cycle_id, &expected_ids)?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

/// Overlapping schedule pairs of one day. A pure query — conflicts are shown,
/// never auto-resolved.
#[tauri::command]
pub fn get_schedule_overlaps(
    db: State<'_, Db>,
    day_cycle_id: String,
) -> AppResult<Vec<ScheduleOverlap>> {
    calendar::get_schedule_overlaps(&db, &day_cycle_id)
}

/// One day's budget: capacity (`null` = unset — the UI hides the bar) versus
/// the total scheduled focus time.
#[tauri::command]
pub fn get_time_budget(db: State<'_, Db>, date: String) -> AppResult<TimeBudget> {
    calendar::get_time_budget(&db, &date)
}

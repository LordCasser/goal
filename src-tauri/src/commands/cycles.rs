//! Cycle commands.

use chrono::NaiveDate;
use tauri::State;

use crate::db::Db;
use crate::domain::calendar;
use crate::domain::task::Task;
use crate::error::AppResult;
use crate::service::cycles::{self, AddSessionArgs, CreateCycleArgs};

use super::emit_mutation;

#[tauri::command]
pub fn get_planner_state(db: State<'_, Db>) -> AppResult<cycles::PlannerState> {
    cycles::get_planner_state(&db)
}

/// Focus blocks of one day; empty when the cycle is not a day (or missing).
#[tauri::command]
pub fn list_sessions(
    db: State<'_, Db>,
    day_cycle_id: String,
) -> AppResult<Vec<crate::domain::cycle::Cycle>> {
    cycles::list_sessions(&db, &day_cycle_id)
}

#[tauri::command]
pub fn create_planning_cycle(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    args: CreateCycleArgs,
) -> AppResult<crate::domain::cycle::Cycle> {
    let today = calendar::today_local();
    let mutation = cycles::create_planning_cycle(&db, &args, today, crate::service::now_ms())?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

/// Only focus blocks accept edits; planning cycles are commitments.
#[tauri::command]
pub fn update_cycle(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    cycle_id: String,
    title: String,
    duration_ms: Option<i64>,
) -> AppResult<crate::domain::cycle::Cycle> {
    let mutation = cycles::update_session(&db, &cycle_id, title, duration_ms)?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

#[tauri::command]
pub fn get_cycle_deletion_preview(
    db: State<'_, Db>,
    cycle_id: String,
) -> AppResult<cycles::CycleDeletionPreview> {
    cycles::get_cycle_deletion_preview(&db, &cycle_id)
}

#[tauri::command]
pub fn delete_planning_cycle(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    cycle_id: String,
    confirmation_token: Option<String>,
) -> AppResult<()> {
    let mutation = cycles::delete_cycle_confirmed(&db, &cycle_id, confirmation_token.as_deref())?;
    emit_mutation(&app, &mutation);
    Ok(())
}

#[tauri::command]
pub fn start_cycle(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    cycle_id: String,
) -> AppResult<crate::domain::cycle::Cycle> {
    let mutation = cycles::start_cycle(&db, &cycle_id, crate::service::now_ms())?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

#[tauri::command]
pub fn finish_cycle(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    cycle_id: String,
) -> AppResult<crate::domain::cycle::Cycle> {
    let mutation = cycles::finish_cycle(&db, &cycle_id, crate::service::now_ms())?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

#[tauri::command]
pub fn add_session(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    args: AddSessionArgs,
) -> AppResult<crate::domain::cycle::Cycle> {
    let mutation = cycles::add_session(&db, &args, crate::service::now_ms())?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

#[tauri::command]
pub fn reorder_sessions(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    day_cycle_id: String,
    session_ids: Vec<String>,
) -> AppResult<()> {
    let mutation = cycles::reorder_sessions(&db, &day_cycle_id, &session_ids)?;
    emit_mutation(&app, &mutation);
    Ok(())
}

#[tauri::command]
pub fn copy_uncompleted_from_previous(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    cycle_id: String,
) -> AppResult<Vec<Task>> {
    let mutation =
        cycles::copy_uncompleted_from_previous(&db, &cycle_id, crate::service::now_ms())?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

/// Convenience used by the frontend when it needs today's columns without
/// reimplementing find-or-create.
#[tauri::command]
pub fn ensure_day(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    date: Option<String>,
) -> AppResult<crate::domain::cycle::Cycle> {
    let date = match date {
        Some(raw) => calendar::parse_date(&raw).ok_or_else(|| {
            crate::error::AppError::validation("invalid_date", "date must be YYYY-MM-DD")
        })?,
        None => calendar::today_local(),
    };
    let mutation = cycles::get_or_create_day(&db, date, crate::service::now_ms())?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

#[tauri::command]
pub fn create_day_plan(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    date: String,
) -> AppResult<crate::domain::cycle::Cycle> {
    let date = parse_command_date(&date)?;
    let mutation = cycles::create_day_plan(&db, date, crate::service::now_ms())?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

/// Parsed local date, exposed for tests of command-layer date validation.
#[allow(dead_code)]
pub(crate) fn parse_command_date(raw: &str) -> AppResult<NaiveDate> {
    calendar::parse_date(raw).ok_or_else(|| {
        crate::error::AppError::validation("invalid_date", "date must be YYYY-MM-DD")
    })
}

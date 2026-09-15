//! Do Later commands: park an idea, then pull it into the active plan.

use tauri::State;

use crate::db::Db;
use crate::domain::cycle::LATER_CYCLE_ID;
use crate::domain::task::Task;
use crate::error::{AppError, AppResult};
use crate::service::tasks::{self, AddTaskArgs};

use super::emit_mutation;

#[tauri::command]
pub fn add_later_goal(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    title: String,
) -> AppResult<Task> {
    if title.trim().is_empty() {
        return Err(AppError::validation(
            "invalid_title",
            "A goal needs a title",
        ));
    }
    let args = AddTaskArgs {
        cycle_id: LATER_CYCLE_ID.to_string(),
        title,
        ..Default::default()
    };
    let mutation = tasks::add_task(&db, &args, crate::service::now_ms())?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

/// Pull a parked idea into a planning cycle (typically the long-term one).
#[tauri::command]
pub fn promote_later_goal(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    task_id: String,
    target_cycle_id: String,
) -> AppResult<Task> {
    let mutation = tasks::move_task(&db, &task_id, &target_cycle_id, None)?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

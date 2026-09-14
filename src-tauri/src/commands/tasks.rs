//! Task commands.

use tauri::State;

use crate::db::Db;
use crate::domain::task::Task;
use crate::error::AppResult;
use crate::service::tasks::{self, AddTaskArgs, TaskPatch};

use super::emit_mutation;

#[tauri::command]
pub fn add_task(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    args: AddTaskArgs,
) -> AppResult<Task> {
    let mutation = tasks::add_task(&db, &args, crate::service::now_ms())?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

/// Full editor save: title required, other fields optional.
#[tauri::command]
pub fn update_task(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    task_id: String,
    title: String,
    patch: TaskPatch,
) -> AppResult<Task> {
    let mutation = tasks::update_task(&db, &task_id, title, &patch)?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

/// Partial editor save: every field optional.
#[tauri::command]
pub fn patch_task(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    task_id: String,
    patch: TaskPatch,
) -> AppResult<Task> {
    let mutation = tasks::patch_task(&db, &task_id, &patch)?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

#[tauri::command]
pub fn delete_task(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    task_id: String,
) -> AppResult<()> {
    let mutation = tasks::delete_task(&db, &task_id)?;
    emit_mutation(&app, &mutation);
    Ok(())
}

#[tauri::command]
pub fn move_task(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    task_id: String,
    target_cycle_id: String,
    position: Option<i64>,
) -> AppResult<Task> {
    let mutation = tasks::move_task(&db, &task_id, &target_cycle_id, position)?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

#[tauri::command]
pub fn reorder_tasks(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    cycle_id: String,
    parent_id: Option<String>,
    ordered_ids: Vec<String>,
) -> AppResult<()> {
    let mutation = tasks::reorder_tasks(&db, &cycle_id, parent_id.as_deref(), &ordered_ids)?;
    emit_mutation(&app, &mutation);
    Ok(())
}

/// Cross-level link (weekly -> long-term, daily -> weekly); `None` unlinks.
#[tauri::command]
pub fn set_task_parent_link(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    task_id: String,
    parent_id: Option<String>,
) -> AppResult<Task> {
    let mutation = tasks::set_task_parent_link(&db, &task_id, parent_id.as_deref())?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

/// Long-term goal coloring; `None` clears.
#[tauri::command]
pub fn set_task_root_color(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    task_id: String,
    color_key: Option<String>,
) -> AppResult<Task> {
    let mutation = tasks::set_task_root_color(&db, &task_id, color_key.as_deref())?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

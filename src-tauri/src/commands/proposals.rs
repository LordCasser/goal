//! Agent proposal commands: preview summary and Keep/Revert (single & batch).

use tauri::State;

use crate::db::Db;
use crate::domain::task::Task;
use crate::error::AppResult;
use crate::service::proposals::{self, PreviewSummary};

use super::emit_mutation;

#[tauri::command]
pub fn get_preview_summary(db: State<'_, Db>, cycle_id: String) -> AppResult<PreviewSummary> {
    proposals::get_preview_summary(&db, &cycle_id)
}

#[tauri::command]
pub fn keep_task_preview(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    task_id: String,
) -> AppResult<Task> {
    let mutation = proposals::keep_task_preview(&db, &task_id)?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

#[tauri::command]
pub fn undo_task_preview(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    task_id: String,
) -> AppResult<()> {
    let mutation = proposals::undo_task_preview(&db, &task_id)?;
    emit_mutation(&app, &mutation);
    Ok(())
}

#[tauri::command]
pub fn keep_all_previews(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    cycle_id: String,
) -> AppResult<usize> {
    let mutation = proposals::keep_all_previews(&db, &cycle_id)?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

#[tauri::command]
pub fn undo_all_previews(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    cycle_id: String,
) -> AppResult<usize> {
    let mutation = proposals::undo_all_previews(&db, &cycle_id)?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

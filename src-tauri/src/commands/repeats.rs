//! Repeat template commands.

use tauri::State;

use crate::db::Db;
use crate::domain::repeat::Repeat;
use crate::error::AppResult;
use crate::service::repeats::{self, RepeatPatch};

use super::emit_mutation;

#[tauri::command]
pub fn add_repeat(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    session_id: String,
) -> AppResult<Repeat> {
    let mutation = repeats::add_repeat(
        &db,
        &repeats::AddRepeatArgs { session_id },
        crate::service::now_ms(),
    )?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

/// Edits the template only — future instances pick the change up; history
/// never changes.
#[tauri::command]
pub fn update_repeat(
    db: State<'_, Db>,
    repeat_id: String,
    patch: RepeatPatch,
) -> AppResult<Repeat> {
    repeats::update_repeat(&db, &repeat_id, &patch)
}

#[tauri::command]
pub fn stop_repeat(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    repeat_id: String,
) -> AppResult<Repeat> {
    let mutation = repeats::stop_repeat(&db, &repeat_id)?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

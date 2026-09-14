//! Editor workspace commands (backend-authored editing content).

use std::collections::BTreeMap;

use tauri::State;

use crate::db::Db;
use crate::error::AppResult;
use crate::service::editor::{self, EditorWorkspace};

#[tauri::command]
pub fn get_editor_workspace(db: State<'_, Db>, cycle_id: String) -> AppResult<EditorWorkspace> {
    editor::get_editor_workspace(&db, &cycle_id)
}

/// Batch form: results keyed by cycle id; unknown cycles map to empty.
#[tauri::command]
pub fn get_editor_workspaces_by_cycle_ids(
    db: State<'_, Db>,
    cycle_ids: Vec<String>,
) -> AppResult<BTreeMap<String, EditorWorkspace>> {
    editor::get_editor_workspaces_by_cycle_ids(&db, &cycle_ids)
}

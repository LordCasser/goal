//! Recycle bin IPC. Restores and permanent removal are transactional services.

use tauri::State;
use crate::{db::Db, error::AppResult, service::trash::{self, TrashEntry}};
use super::emit_mutation;

#[tauri::command]
pub fn list_trash(db: State<'_, Db>) -> AppResult<Vec<TrashEntry>> {
    trash::list(&db)
}

#[tauri::command]
pub fn restore_trash_entry(app: tauri::AppHandle<tauri::Wry>, db: State<'_, Db>, entry_id: String) -> AppResult<()> {
    let mutation = trash::restore(&db, &entry_id)?;
    emit_mutation(&app, &mutation);
    Ok(())
}

#[tauri::command]
pub fn delete_trash_entries(app: tauri::AppHandle<tauri::Wry>, db: State<'_, Db>, entry_ids: Vec<String>) -> AppResult<usize> {
    let count = trash::delete_permanently(&db, &entry_ids)?;
    if count > 0 { crate::events::emit_trash_changed(&app); }
    Ok(count)
}

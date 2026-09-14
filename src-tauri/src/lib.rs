//! Planner application entry point.
//!
//! Layering and ownership are described in `docs/architecture.md`.
//! This file only wires state and registers commands; no business rules live here.
//!
//! This application ships with no telemetry of any kind: there is no event
//! collection, no reporting channel and no network call outside AI requests.

pub mod commands;
pub mod db;
pub mod domain;
pub mod error;
pub mod events;
pub mod logging;
pub mod providers;
pub mod repository;
pub mod sampling;
pub mod service;

use tauri::Manager;

/// Boots the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let state = db::init(&handle)?;
            let log_dir = db::data_dir(&handle)?.join("logs");
            let initial_level = persisted_log_level(&state);
            app.manage(state);

            logging::init(log_dir, initial_level);
            logging::info("app", "app started");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // cycles
            commands::cycles::get_planner_state,
            commands::cycles::list_sessions,
            commands::cycles::create_planning_cycle,
            commands::cycles::update_cycle,
            commands::cycles::get_cycle_deletion_preview,
            commands::cycles::delete_planning_cycle,
            commands::cycles::start_cycle,
            commands::cycles::finish_cycle,
            commands::cycles::add_session,
            commands::cycles::reorder_sessions,
            commands::cycles::copy_uncompleted_from_previous,
            commands::cycles::ensure_day,
            // tasks
            commands::tasks::add_task,
            commands::tasks::update_task,
            commands::tasks::patch_task,
            commands::tasks::delete_task,
            commands::tasks::move_task,
            commands::tasks::reorder_tasks,
            commands::tasks::set_task_parent_link,
            commands::tasks::set_task_root_color,
            // editor workspaces
            commands::editor::get_editor_workspace,
            commands::editor::get_editor_workspaces_by_cycle_ids,
            // do later
            commands::later::add_later_goal,
            commands::later::promote_later_goal,
            // agent proposals
            commands::proposals::get_preview_summary,
            commands::proposals::keep_task_preview,
            commands::proposals::undo_task_preview,
            commands::proposals::keep_all_previews,
            commands::proposals::undo_all_previews,
            // repeats
            commands::repeats::add_repeat,
            commands::repeats::update_repeat,
            commands::repeats::stop_repeat,
            // settings
            commands::settings::get_settings,
            commands::settings::set_week_start_day,
            commands::settings::set_theme,
            commands::settings::set_log_level,
            commands::settings::get_app_flag,
            commands::settings::set_app_flag,
            // maintenance
            commands::maintenance::get_schema_version,
            commands::maintenance::export_backup,
            commands::maintenance::get_debug_log_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Reads the persisted `log_level` setting, if present and valid, so the
/// logger can restore the user's chosen verbosity at startup. A read failure
/// just falls back to the default level.
fn persisted_log_level(db: &db::Db) -> Option<logging::Level> {
    let conn = db.pool().get().ok()?;
    let raw = crate::repository::settings::get(&conn, crate::repository::settings::KEY_LOG_LEVEL)
        .ok()
        .flatten()?;
    logging::Level::parse(&raw)
}

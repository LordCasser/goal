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
pub mod repository;
pub mod service;

use tauri::Manager;

/// Boots the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            let state = db::init(&handle)?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // cycles
            commands::cycles::get_planner_state,
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
            // maintenance
            commands::maintenance::get_schema_version,
            commands::maintenance::export_backup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

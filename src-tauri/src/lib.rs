//! Planner application entry point.
//!
//! Layering and ownership are described in `docs/architecture.md`.
//! This file only wires state and registers commands; no business rules live here.

// `pub` so integration tests can drive the real service/repository APIs
// (see docs/architecture.md, 验证入口).
pub mod commands;
pub mod db;
pub mod domain;
pub mod error;
pub mod events;
pub mod repository;
pub mod service;
pub mod telemetry;

use std::sync::Arc;
use std::time::Duration;

use tauri::Manager;

/// Boots the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            let state = db::init(&handle)?;

            // Telemetry stays off unless the persisted switch says otherwise.
            // The sink is unconfigured until an endpoint ships with the AI
            // change; batches are dropped silently in the meantime.
            let reporter = telemetry::Reporter::spawn(
                Arc::new(telemetry::UnconfiguredSink),
                64,
                8,
                Duration::from_secs(10),
            );
            let enabled = telemetry::is_enabled(&state).unwrap_or(false);
            reporter.set_enabled(enabled);
            let errors = telemetry::ErrorCategories::default();
            errors.set_enabled(enabled);

            app.manage(reporter);
            app.manage(errors);
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
            // settings & telemetry
            commands::settings::get_settings,
            commands::settings::set_week_start_day,
            commands::settings::get_telemetry_settings,
            commands::settings::set_telemetry_enabled,
            commands::settings::export_diagnostics,
            // maintenance
            commands::maintenance::get_schema_version,
            commands::maintenance::export_backup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

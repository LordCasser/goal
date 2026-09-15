//! Planner application entry point.
//!
//! Layering and ownership are described in `docs/architecture.md`.
//! This file only wires state and registers commands; no business rules live here.
//!
//! This application ships with no telemetry of any kind: there is no event
//! collection, no reporting channel and no network call outside AI requests.

pub mod ai;
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
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let state = db::init(&handle)?;
            let data_dir = db::data_dir(&handle)?;
            let initial_level = persisted_log_level(&state);
            // The reminder scheduler owns one thread and its own Db handle;
            // started before manage so commands can reach it.
            let scheduler = commands::reminders::start_scheduler(handle.clone(), state.clone());
            app.manage(scheduler);
            app.manage(state);

            // AI settings state (task 4.1): provider metadata from
            // providers.json in the same data directory, credentials from the
            // system keychain. Loaded before `manage` so commands never see a
            // half-initialized store.
            let ai_settings = providers::service::AiSettingsState::load(&data_dir)?;
            app.manage(ai_settings);
            // Planning-issue review cache: keyed by (cycle, content hash), so
            // repeated editor-triggered reviews stay cheap (design D5).
            app.manage(ai::review::IssueCache::default());

            logging::init(data_dir.join("logs"), initial_level);
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
            // ai settings
            commands::ai_settings::get_ai_settings,
            commands::ai_settings::save_provider,
            commands::ai_settings::delete_provider,
            commands::ai_settings::set_active_provider,
            commands::ai_settings::save_provider_api_key,
            commands::ai_settings::remove_provider_api_key,
            commands::ai_settings::test_provider_connection,
            // agent conversations & planning issues
            commands::agent::start_agent_conversation,
            commands::agent::send_agent_message,
            commands::agent::get_agent_conversation,
            commands::agent::get_previous_agent_conversation,
            commands::agent::start_planning,
            commands::agent::start_goal_setting,
            commands::agent::start_prioritization,
            commands::agent::get_planning_issue_report,
            commands::agent::dismiss_planning_issue,
            commands::agent::get_planning_issue_dismissals,
            // reviews (add-review-retrospective)
            commands::reviews::get_cycle_facts,
            commands::reviews::get_cycle_review,
            commands::reviews::save_cycle_review,
            commands::reviews::apply_review_disposition,
            commands::reviews::get_review_summary,
            commands::reviews::export_cycle_review_markdown,
            commands::reviews::save_cycle_review_markdown,
            // calendar view (add-calendar-time-view)
            commands::calendar_view::get_calendar_range,
            commands::calendar_view::move_day_cycle,
            commands::calendar_view::set_session_schedule,
            commands::calendar_view::get_schedule_overlaps,
            commands::calendar_view::get_time_budget,
            // reminders (add-reminders-notifications)
            commands::reminders::set_reminder,
            commands::reminders::list_reminders,
            commands::reminders::update_reminder,
            commands::reminders::delete_reminder,
            commands::reminders::reconcile_reminders,
            commands::reminders::get_missed_summary,
            commands::reminders::acknowledge_missed_summary,
            commands::reminders::get_reminder_settings,
            commands::reminders::set_reminder_settings,
            commands::reminders::request_notification_permission,
            commands::reminders::get_notification_permission,
            // onboarding & lifecycle (add-onboarding-and-lifecycle)
            commands::onboarding::get_onboarding,
            commands::onboarding::complete_onboarding,
            commands::onboarding::reconcile_getting_started_guide,
            commands::onboarding::skip_getting_started_guide,
            commands::onboarding::get_dismissed_hints,
            commands::onboarding::dismiss_hint,
            commands::onboarding::mark_exit_poll_listener_ready,
            commands::onboarding::acknowledge_exit_poll_shown,
            commands::onboarding::submit_exit_poll,
            commands::onboarding::dismiss_exit_poll,
            commands::onboarding::continue_after_exit_poll,
            commands::onboarding::exit_after_exit_poll,
            commands::onboarding::send_feedback,
            commands::onboarding::list_staged_feedback,
            commands::onboarding::get_talk_to_founder_eligibility,
            commands::onboarding::record_talk_to_founder_opened,
            commands::onboarding::close_talk_to_founder,
            commands::onboarding::get_app_version,
            commands::onboarding::check_for_updates,
            commands::onboarding::get_lifecycle_prefs,
            commands::onboarding::set_daily_plan_time,
            commands::onboarding::notify_session_due,
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

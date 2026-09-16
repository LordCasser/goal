//! Onboarding & lifecycle commands (change: add-onboarding-and-lifecycle).
//!
//! Every command is a thin wrapper: argument defaults, one service call, and
//! (for the guide skip) the mutation-to-event bridge. The tests at the bottom
//! drive the same service entry points each command body calls, so the main
//! paths of the IPC surface stay covered (7.5) even though `tauri::State` /
//! `AppHandle` cannot be constructed inside unit tests.

use tauri::State;

use crate::db::Db;
use crate::error::AppResult;
use crate::service::onboarding::{self, ExitPollAnswers};

use super::emit_mutation;

// --- §1 getting-started guide -----------------------------------------------

#[tauri::command]
pub fn get_onboarding(db: State<'_, Db>) -> AppResult<onboarding::GettingStartedGuide> {
    onboarding::get_onboarding(&db)
}

#[tauri::command]
pub fn complete_onboarding(db: State<'_, Db>) -> AppResult<onboarding::GettingStartedGuide> {
    onboarding::complete_onboarding(&db)
}

/// Re-derives the guide from live data; the frontend calls this whenever a
/// `cycles:changed` / `tasks:changed` notice arrives (1.3).
#[tauri::command]
pub fn reconcile_getting_started_guide(
    db: State<'_, Db>,
) -> AppResult<onboarding::GettingStartedGuide> {
    onboarding::reconcile_getting_started_guide(&db)
}

/// Skips the guide and cleans up empty leftovers; the deletion surfaces as
/// `cycles:changed` / `tasks:changed` so open workspaces refresh.
#[tauri::command]
pub fn skip_getting_started_guide(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
) -> AppResult<onboarding::SkipReport> {
    let mutation = onboarding::skip_getting_started_guide(&db, crate::service::now_ms())?;
    emit_mutation(&app, &mutation);
    Ok(mutation.value)
}

// --- §2 one-shot hints -------------------------------------------------------

#[tauri::command]
pub fn get_dismissed_hints(db: State<'_, Db>) -> AppResult<Vec<String>> {
    onboarding::get_dismissed_hints(&db)
}

#[tauri::command]
pub fn dismiss_hint(db: State<'_, Db>, hint_id: String) -> AppResult<()> {
    onboarding::dismiss_hint(&db, &hint_id)?;
    Ok(())
}

// --- §3 exit poll -------------------------------------------------------------

/// The frontend calls this once its exit-poll listener is wired. `force`
/// (`Some(true)`) is the manual menu entry (3.6); the automatic quit path
/// passes `None`/`false` and only presents when the behaviour condition holds
/// and the poll was never shown before.
#[tauri::command]
pub fn mark_exit_poll_listener_ready(
    db: State<'_, Db>,
    force: Option<bool>,
) -> AppResult<onboarding::ExitPollPresentation> {
    onboarding::mark_exit_poll_listener_ready(&db, crate::service::now_ms(), force.unwrap_or(false))
}

#[tauri::command]
pub fn acknowledge_exit_poll_shown(db: State<'_, Db>) -> AppResult<()> {
    onboarding::acknowledge_exit_poll_shown(&db, crate::service::now_ms())
}

/// Stages the answers locally (本版本无上报通道，仅存本地等待显式导出) and
/// moves the poll state machine to `submitted`.
#[tauri::command]
pub fn submit_exit_poll(
    db: State<'_, Db>,
    args: ExitPollAnswers,
) -> AppResult<onboarding::StagedEvent> {
    onboarding::submit_exit_poll(&db, crate::service::now_ms(), &args)
}

/// Records the ignore; the caller (frontend quit flow) exits afterwards.
#[tauri::command]
pub fn dismiss_exit_poll(db: State<'_, Db>) -> AppResult<onboarding::StagedEvent> {
    onboarding::dismiss_exit_poll(&db, crate::service::now_ms())
}

/// Cancels the quit (3.4): the caller keeps the app running.
#[tauri::command]
pub fn continue_after_exit_poll(db: State<'_, Db>) -> AppResult<()> {
    onboarding::continue_after_exit_poll(&db, crate::service::now_ms())
}

/// Confirms the quit (3.4): only records the decision — the actual window
/// close belongs to the frontend quit flow.
#[tauri::command]
pub fn exit_after_exit_poll(db: State<'_, Db>) -> AppResult<()> {
    onboarding::exit_after_exit_poll(&db, crate::service::now_ms())
}

// --- §4 feedback & talk-to-founder --------------------------------------------

/// Stages feedback locally. Empty content is rejected with `empty_feedback`;
/// content can never be lost because nothing leaves the disk (4.2/4.3).
#[tauri::command]
pub fn send_feedback(db: State<'_, Db>, message: String) -> AppResult<onboarding::FeedbackReceipt> {
    onboarding::send_feedback(&db, crate::service::now_ms(), &message)
}

/// The queue as the user sees it: everything staged, awaiting explicit export.
#[tauri::command]
pub fn list_staged_feedback(db: State<'_, Db>) -> AppResult<Vec<onboarding::StagedFeedback>> {
    onboarding::list_staged_feedback(&db)
}

#[tauri::command]
pub fn get_talk_to_founder_eligibility(
    db: State<'_, Db>,
) -> AppResult<onboarding::TalkToFounderEligibility> {
    onboarding::get_talk_to_founder_eligibility(&db)
}

#[tauri::command]
pub fn record_talk_to_founder_opened(db: State<'_, Db>) -> AppResult<i64> {
    onboarding::record_talk_to_founder_opened(&db, crate::service::now_ms())
}

#[tauri::command]
pub fn close_talk_to_founder(db: State<'_, Db>) -> AppResult<()> {
    onboarding::close_talk_to_founder(&db, crate::service::now_ms())
}

// --- §5 update check (degraded) -------------------------------------------------

/// The compiled-in version, for header badges and diagnostics.
#[tauri::command]
pub fn get_app_version() -> String {
    onboarding::app_version().to_string()
}

/// Three-state check: `available` / `up_to_date` / `failed`, plus `disabled`
/// while no `update_endpoint` setting exists (本版本无更新通道).
#[tauri::command]
pub fn check_for_updates(db: State<'_, Db>) -> AppResult<onboarding::UpdateCheck> {
    onboarding::check_for_updates(&db)
}

// --- daily plan time -------------------------------------------------------------

#[tauri::command]
pub fn get_lifecycle_prefs(db: State<'_, Db>) -> AppResult<onboarding::LifecyclePrefs> {
    onboarding::get_lifecycle_prefs(&db)
}

#[tauri::command]
pub fn set_daily_plan_time(db: State<'_, Db>, daily_plan_time: Option<String>) -> AppResult<()> {
    onboarding::set_daily_plan_time(&db, daily_plan_time)
}

// --- §6 focus-block due notification ----------------------------------------------

/// Outcome of a due-notification attempt; never an `Err` for environmental
/// reasons (permission denied, plugin missing) — those are silent skips so a
/// notification problem cannot disturb the focus block state.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum SessionDueOutcome {
    Notified,
    Skipped { reason: &'static str },
}

/// Called by the frontend timer when a focus block reaches its planned end
/// (6.1). Sessions without a set duration never notify (6.3); a denied or
/// unavailable notification permission is a silent skip (6.2).
#[tauri::command]
pub fn notify_session_due(
    _app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    session_id: String,
    title: String,
    duration_ms: Option<i64>,
) -> AppResult<SessionDueOutcome> {
    if !onboarding::should_notify_session_due(duration_ms) {
        return Ok(SessionDueOutcome::Skipped {
            reason: "no_duration",
        });
    }
    let locale = crate::i18n::for_db(&db)?;
    let notification_title = crate::i18n::text(locale, "notification.session", &[]);
    send_due_notification(&session_id, &notification_title, &title)
}

fn send_due_notification(
    session_id: &str,
    notification_title: &str,
    body: &str,
) -> AppResult<SessionDueOutcome> {
    match crate::platform::notifications::send(notification_title, body) {
        Ok(()) => Ok(SessionDueOutcome::Notified),
        Err(error) => {
            crate::logging::warn(
                "onboarding",
                &format!("due notification for {session_id} not submitted: {error}"),
            );
            Ok(SessionDueOutcome::Skipped {
                reason: "permission_denied_or_unavailable",
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Tests — command-level main paths (7.5). `State`/`AppHandle` are tauri
// runtime values, so the suites below exercise exactly the service entry
// points each command body calls, on a real migrated database.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::domain::cycle::{Cycle, CycleType};
    use crate::domain::task::Task;
    use crate::error::AppError;
    use crate::repository::{cycles as cycles_repo, settings as settings_repo};
    use crate::service::cycles::{self, AddSessionArgs, CreateCycleArgs};
    use crate::service::now_ms;
    use crate::service::onboarding::{self, ExitPollAnswers};
    use crate::service::tasks::{self, AddTaskArgs};

    struct TestDb {
        db: crate::db::Db,
        _dir: tempfile::TempDir,
    }

    impl TestDb {
        fn open() -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("planner.db");
            let db = crate::db::open_at(&path).expect("open db");
            Self { db, _dir: dir }
        }
    }

    const NOW: i64 = 1_700_000_000_000;
    const TODAY: &str = "2026-09-16";

    fn today() -> chrono::NaiveDate {
        crate::domain::calendar::parse_date(TODAY).unwrap()
    }

    fn long_term(db: &crate::db::Db) -> Cycle {
        cycles::create_planning_cycle(
            db,
            &CreateCycleArgs {
                cycle_type: "month".into(),
                duration_months: Some(1),
                ..Default::default()
            },
            today(),
            NOW,
        )
        .unwrap()
        .value
    }

    fn add_task(db: &crate::db::Db, cycle_id: &str, title: &str) -> Task {
        tasks::add_task(
            db,
            &AddTaskArgs {
                cycle_id: cycle_id.into(),
                title: title.into(),
                ..Default::default()
            },
            NOW,
        )
        .unwrap()
        .value
    }

    fn finish_focus_block(db: &crate::db::Db, day: &str) {
        let session = cycles::add_session(
            db,
            &AddSessionArgs {
                task_id: None,
                day_cycle_id: day.into(),
                title: "deep work".into(),
                duration_ms: Some(50 * 60 * 1000),
                position: None,
            },
            NOW,
        )
        .unwrap()
        .value;
        cycles::start_cycle(db, &session.id, NOW).unwrap();
        cycles::finish_cycle(db, &session.id, NOW + 31 * 60 * 1000).unwrap();
    }

    // -- guide commands -------------------------------------------------------

    #[test]
    fn get_onboarding_main_path_starts_empty() {
        let test = TestDb::open();
        let guide = onboarding::get_onboarding(&test.db).unwrap();
        assert_eq!(guide.completed, 0);
        assert_eq!(guide.total, 5);
        assert_eq!(guide.steps.len(), 5);
    }

    #[test]
    fn complete_then_reconcile_main_path() {
        let test = TestDb::open();
        onboarding::complete_onboarding(&test.db).unwrap();
        let reconciled = onboarding::reconcile_getting_started_guide(&test.db).unwrap();
        assert_eq!(reconciled.state, onboarding::GuideState::Completed);
    }

    #[test]
    fn skip_main_path_reports_and_emits_deletions() {
        let test = TestDb::open();
        // A leftover untitled cycle with a placeholder task.
        cycles_repo::insert(
            &test.db.pool().get().unwrap(),
            &cycles_repo::NewCycle {
                id: "leftover".into(),
                title: "".into(),
                cycle_type: CycleType::Month,
                parent_id: None,
                position: 0,
                duration: None,
                starts_on: None,
                ends_on: None,
                calendar_key: None,
                repeat_id: None,
                task_id: None,
                created_at: NOW,
            },
        )
        .unwrap();
        let mutation = onboarding::skip_getting_started_guide(&test.db, now_ms()).unwrap();
        // The mutation is what `emit_mutation` turns into cycle/task notices.
        assert!(mutation
            .value
            .deleted_cycle_ids
            .contains(&"leftover".into()));
        assert_eq!(mutation.value.deleted_task_count, 0);
        assert_eq!(
            onboarding::get_onboarding(&test.db).unwrap().state,
            onboarding::GuideState::Skipped
        );
    }

    // -- hints commands ---------------------------------------------------------

    #[test]
    fn hint_dismissal_main_path() {
        let test = TestDb::open();
        onboarding::dismiss_hint(&test.db, "later-explainer").unwrap();
        assert_eq!(
            onboarding::get_dismissed_hints(&test.db).unwrap(),
            vec!["later-explainer".to_string()]
        );
        let err = onboarding::dismiss_hint(&test.db, "bad id").unwrap_err();
        assert!(matches!(err, AppError::Validation { .. }));
    }

    // -- exit poll commands ------------------------------------------------------

    #[test]
    fn exit_poll_lifecycle_main_path() {
        let test = TestDb::open();
        // Condition not met: no dialog, no timestamp.
        let presentation = onboarding::mark_exit_poll_listener_ready(&test.db, NOW, false).unwrap();
        assert!(!presentation.show);

        // Build eligibility: one 31-minute finished block.
        let month = long_term(&test.db);
        let week = cycles::create_planning_cycle(
            &test.db,
            &CreateCycleArgs {
                cycle_type: "week".into(),
                parent_id: Some(month.id.clone()),
                ..Default::default()
            },
            today(),
            NOW,
        )
        .unwrap()
        .value;
        let day = cycles::create_planning_cycle(
            &test.db,
            &CreateCycleArgs {
                cycle_type: "day".into(),
                parent_id: Some(week.id.clone()),
                date: Some(TODAY.into()),
                ..Default::default()
            },
            today(),
            NOW,
        )
        .unwrap()
        .value;
        finish_focus_block(&test.db, &day.id);

        let presentation =
            onboarding::mark_exit_poll_listener_ready(&test.db, NOW + 1, false).unwrap();
        assert!(presentation.show);
        onboarding::acknowledge_exit_poll_shown(&test.db, NOW + 2).unwrap();
        let event = onboarding::submit_exit_poll(
            &test.db,
            NOW + 3,
            &ExitPollAnswers {
                rating: Some(3),
                reason: None,
                detail: Some("keep going".into()),
            },
        )
        .unwrap();
        assert_eq!(event.kind, "exit_poll");
        onboarding::exit_after_exit_poll(&test.db, NOW + 4).unwrap();
        assert_eq!(
            settings_repo::get(
                &test.db.pool().get().unwrap(),
                onboarding::KEY_EXIT_POLL_STATE
            )
            .unwrap(),
            Some("exit_confirmed".into())
        );

        // The continue/dismiss paths share the same shape.
        onboarding::continue_after_exit_poll(&test.db, NOW + 5).unwrap();
        onboarding::dismiss_exit_poll(&test.db, NOW + 6).unwrap();
    }

    // -- feedback commands ---------------------------------------------------------

    #[test]
    fn feedback_main_path_rejects_empty_then_stages() {
        let test = TestDb::open();
        assert!(matches!(
            onboarding::send_feedback(&test.db, NOW, "   "),
            Err(AppError::Validation { code, .. }) if code == "empty_feedback"
        ));
        let receipt = onboarding::send_feedback(&test.db, NOW + 1, "works great").unwrap();
        assert_eq!(receipt.queue_len, 1);
        let staged = onboarding::list_staged_feedback(&test.db).unwrap();
        assert_eq!(staged.len(), 1);
        assert_eq!(staged[0].message, "works great");
    }

    #[test]
    fn talk_to_founder_main_path() {
        let test = TestDb::open();
        let eligibility = onboarding::get_talk_to_founder_eligibility(&test.db).unwrap();
        assert!(!eligibility.eligible);
        assert_eq!(
            onboarding::record_talk_to_founder_opened(&test.db, NOW).unwrap(),
            1
        );
        onboarding::close_talk_to_founder(&test.db, NOW + 1).unwrap();
    }

    // -- updater & prefs commands ----------------------------------------------------

    #[test]
    fn version_and_update_check_commands() {
        let test = TestDb::open();
        assert!(!onboarding::app_version().is_empty());
        assert_eq!(
            onboarding::check_for_updates(&test.db).unwrap(),
            onboarding::UpdateCheck::Disabled
        );
        assert_eq!(
            onboarding::compare_versions("0.1.0", "0.2.0"),
            Ok(std::cmp::Ordering::Less)
        );
    }

    #[test]
    fn lifecycle_prefs_command_main_path() {
        let test = TestDb::open();
        onboarding::set_daily_plan_time(&test.db, Some("09:05".into())).unwrap();
        assert_eq!(
            onboarding::get_lifecycle_prefs(&test.db)
                .unwrap()
                .daily_plan_time,
            Some("09:05".into())
        );
    }

    // -- notify gate --------------------------------------------------------------------

    #[test]
    fn notify_gate_two_states() {
        assert!(!onboarding::should_notify_session_due(None));
        assert!(onboarding::should_notify_session_due(Some(60_000)));
    }
}

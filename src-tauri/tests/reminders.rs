//! Reminder behaviour tests (change: add-reminders-notifications). One test
//! per tasks.md scenario, driven through the same public service APIs the
//! command layer calls.
//! Verification entry point: `cargo test --test reminders`

mod common;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use planner_lib::commands::reminders::SystemNotifier;
use planner_lib::repository::reminders as repo;
use planner_lib::repository::settings as settings_repo;
use planner_lib::service::cycles;
use planner_lib::service::reminders::{
    self, DeliveryOutcome, NotificationContent, PassReport, QuietHours, RecordingNotifier,
    ReminderNotifier, Scheduler, SetReminderArgs, UpdateReminderArgs,
};
use planner_lib::service::tasks;

use common::{add_task, create_day, create_long_term, create_week, TestDb, NOW, TODAY};

fn notifier_granted() -> Arc<RecordingNotifier> {
    Arc::new(RecordingNotifier::with_permission("granted"))
}

fn notifier_denied() -> Arc<RecordingNotifier> {
    Arc::new(RecordingNotifier::with_permission("denied"))
}

struct FailingNotifier;

impl ReminderNotifier for FailingNotifier {
    fn permission(&self) -> String {
        "system_managed".into()
    }

    fn request_permission(&self) -> String {
        self.permission()
    }

    fn notify(&self, _notification: &NotificationContent) -> DeliveryOutcome {
        DeliveryOutcome::Failed("native notification submission failed".into())
    }
}

struct SequenceNotifier {
    outcomes: Mutex<VecDeque<DeliveryOutcome>>,
}

impl SequenceNotifier {
    fn new(outcomes: impl IntoIterator<Item = DeliveryOutcome>) -> Self {
        Self {
            outcomes: Mutex::new(outcomes.into_iter().collect()),
        }
    }
}

impl ReminderNotifier for SequenceNotifier {
    fn permission(&self) -> String {
        "system_managed".into()
    }

    fn request_permission(&self) -> String {
        self.permission()
    }

    fn notify(&self, _notification: &NotificationContent) -> DeliveryOutcome {
        self.outcomes
            .lock()
            .expect("sequence notifier")
            .pop_front()
            .expect("notification outcome")
    }
}

/// Long-term -> week -> day, the smallest real context tasks can live in.
fn day_fixture(db: &TestDb) -> planner_lib::domain::cycle::Cycle {
    let long_term = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &long_term.id, TODAY);
    create_day(&db.db, &week.id, TODAY, NOW)
}

fn set_reminder(
    db: &planner_lib::db::Db,
    target_kind: &str,
    target_id: &str,
    fire_at: i64,
    quiet_ok: bool,
) -> repo::Reminder {
    reminders::set_reminder(
        db,
        &SetReminderArgs {
            target_kind: target_kind.into(),
            target_id: target_id.into(),
            fire_at,
            quiet_ok,
        },
        NOW,
    )
    .expect("reminder set")
}

fn reconcile_with(
    db: &planner_lib::db::Db,
    notifier: &dyn ReminderNotifier,
    now: i64,
) -> PassReport {
    reminders::reconcile(db, notifier, now).expect("reconcile pass")
}

fn start_session(
    db: &planner_lib::db::Db,
    day_id: &str,
    duration_ms: i64,
) -> planner_lib::domain::cycle::Cycle {
    let session = cycles::add_session(
        db,
        &cycles::AddSessionArgs {
            day_cycle_id: day_id.into(),
            title: "Deep work".into(),
            duration_ms: Some(duration_ms),
            position: None,
        },
        NOW,
    )
    .unwrap()
    .value;
    cycles::start_cycle(db, &session.id, NOW).unwrap();
    session
}

/// Wall-clock ms for a local time on the fixed test date.
fn local_ms(day: chrono::NaiveDate, hour: u32, minute: u32) -> i64 {
    day.and_hms_opt(hour, minute, 0)
        .expect("valid time")
        .and_local_timezone(chrono::Local)
        .earliest()
        .expect("resolvable local time")
        .timestamp_millis()
}

fn today_date() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 9, 16).expect("fixed test date")
}

// ---------------------------------------------------------------------------
// §1.5 数据：重复设定复用既有记录；删除目标后提醒被清理
// ---------------------------------------------------------------------------

#[test]
fn setting_the_same_reminder_twice_reuses_the_row() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    let task = add_task(&db.db, &day.id, "Write report", NOW);
    let first = set_reminder(&db.db, "task", &task.id, NOW + 5_000, false);
    let second = set_reminder(&db.db, "task", &task.id, NOW + 5_000, false);

    assert_eq!(first.id, second.id, "the existing row is reused");
    let all = reminders::list_reminders(&db.db, None, repo::StatusFilter::All).unwrap();
    assert_eq!(all.len(), 1, "no second row appears");
}

#[test]
fn setting_an_already_fired_reminder_re_arms_it() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    let task = add_task(&db.db, &day.id, "Write report", NOW);
    let first = set_reminder(&db.db, "task", &task.id, NOW + 5_000, false);
    reconcile_with(&db.db, notifier_granted().as_ref(), NOW + 6_000);
    let fired = repo::get(&db.conn(), &first.id).unwrap().unwrap();
    assert!(fired.fired_at.is_some());

    let rearmed = set_reminder(&db.db, "task", &task.id, NOW + 5_000, false);
    assert_eq!(rearmed.id, first.id, "still the same row");
    assert!(rearmed.fired_at.is_none(), "an explicit re-set fires again");
}

#[test]
fn deleting_a_task_removes_its_reminders() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    let task = add_task(&db.db, &day.id, "Write report", NOW);
    set_reminder(&db.db, "task", &task.id, NOW + 5_000, false);

    tasks::delete_task(&db.db, &task.id).unwrap();

    let all = reminders::list_reminders(&db.db, None, repo::StatusFilter::All).unwrap();
    assert!(all.is_empty(), "the reminder goes with its task (§1.3)");
}

#[test]
fn deleting_a_cycle_subtree_removes_all_attached_reminders() {
    let db = TestDb::open();
    let long_term = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &long_term.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, NOW);
    // An unstarted block: started blocks would trip the deletion guard.
    let session = cycles::add_session(
        &db.db,
        &cycles::AddSessionArgs {
            day_cycle_id: day.id.clone(),
            title: "Deep work".into(),
            duration_ms: Some(900_000),
            position: None,
        },
        NOW,
    )
    .unwrap()
    .value;
    let task = add_task(&db.db, &day.id, "Write report", NOW);
    let other_week = create_week(&db.db, &long_term.id, "2026-09-23");
    let other_task = add_task(&db.db, &other_week.id, "Stay", NOW);

    set_reminder(&db.db, "session", &session.id, NOW + 900_000, false);
    set_reminder(&db.db, "day", &day.id, NOW + 5_000, true);
    set_reminder(&db.db, "task", &task.id, NOW + 6_000, false);
    set_reminder(&db.db, "cycle", &week.id, NOW + 7_000, false);
    let surviving = set_reminder(&db.db, "task", &other_task.id, NOW + 8_000, false);
    assert_eq!(
        reminders::list_reminders(&db.db, None, repo::StatusFilter::Pending)
            .unwrap()
            .len(),
        5
    );

    cycles::delete_cycle(&db.db, &week.id).unwrap();

    let remaining = reminders::list_reminders(&db.db, None, repo::StatusFilter::All).unwrap();
    assert_eq!(
        remaining.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        vec![surviving.id.as_str()],
        "every reminder under the deleted subtree goes; outside ones stay"
    );
}

#[test]
fn reminders_list_scopes_to_a_cycle_subtree() {
    let db = TestDb::open();
    let long_term = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &long_term.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, NOW);
    let task = add_task(&db.db, &day.id, "In subtree", NOW);
    let other_week = create_week(&db.db, &long_term.id, "2026-09-23");
    let other_task = add_task(&db.db, &other_week.id, "Outside", NOW);
    let inside = set_reminder(&db.db, "task", &task.id, NOW + 1_000, false);
    let outside = set_reminder(&db.db, "task", &other_task.id, NOW + 2_000, false);

    let scoped =
        reminders::list_reminders(&db.db, Some(&week.id), repo::StatusFilter::Pending).unwrap();
    let ids: Vec<&str> = scoped.iter().map(|r| r.id.as_str()).collect();
    assert!(ids.contains(&inside.id.as_str()));
    assert!(!ids.contains(&outside.id.as_str()));
}

// ---------------------------------------------------------------------------
// §2.5 调度器：定时器漂移（fire_at 在过去）；批量到期顺序
// ---------------------------------------------------------------------------

#[test]
fn past_due_reminders_fire_on_the_next_pass() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    let task = add_task(&db.db, &day.id, "Write report", NOW);
    let _ = set_reminder(&db.db, "task", &task.id, NOW - 1_000, false);

    let report = reconcile_with(&db.db, notifier_granted().as_ref(), NOW + 9_000);
    assert_eq!(report.notified, 1, "the overdue reminder is not skipped");

    // A second (drifted) pass must not deliver it again.
    let again = reconcile_with(&db.db, notifier_granted().as_ref(), NOW + 20_000);
    assert_eq!(again.notified, 0);
}

#[test]
fn batch_delivery_follows_fire_at_order() {
    let db = TestDb::open();
    let notifier = notifier_granted();
    let day = day_fixture(&db);
    let slow = add_task(&db.db, &day.id, "slow", NOW);
    let first = add_task(&db.db, &day.id, "first", NOW);
    let second = add_task(&db.db, &day.id, "second", NOW);
    set_reminder(&db.db, "task", &slow.id, NOW + 3_000, false);
    set_reminder(&db.db, "task", &first.id, NOW + 1_000, false);
    set_reminder(&db.db, "task", &second.id, NOW + 2_000, false);

    let report = reconcile_with(&db.db, notifier.as_ref(), NOW + 9_000);
    assert_eq!(report.notified, 3);
    let order: Vec<String> = notifier
        .notifications()
        .iter()
        .map(|n| n.target_id.clone())
        .collect();
    assert_eq!(
        order,
        vec![first.id, second.id, slow.id],
        "fire_at ascending"
    );
}

#[test]
fn completed_targets_are_marked_silently_and_missing_targets_are_purged() {
    let db = TestDb::open();
    let long_term = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &long_term.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, NOW);
    let session = start_session(&db.db, &day.id, 900_000);
    let gone = add_task(&db.db, &day.id, "deleted later", NOW);

    let session_reminder = set_reminder(&db.db, "session", &session.id, NOW + 900_000, false);
    let orphan = set_reminder(&db.db, "task", &gone.id, NOW + 1_000, false);

    // Delete the task behind the scheduler's back: the pass is the net.
    planner_lib::repository::tasks::delete(&db.conn(), &gone.id).unwrap();
    // Finish the focus block early: no notification for finished work.
    planner_lib::service::cycles::finish_cycle(&db.db, &session.id, NOW + 1_000).unwrap();

    let notifier = notifier_granted();
    let report = reconcile_with(&db.db, notifier.as_ref(), NOW + 901_000);
    assert_eq!(report.notified, 0);
    assert_eq!(report.silent, 1, "completed target is silently marked");
    assert_eq!(report.purged, 1, "deleted target is purged");
    let fired = repo::get(&db.conn(), &session_reminder.id)
        .unwrap()
        .unwrap();
    assert!(fired.fired_at.is_some());
    assert!(repo::get(&db.conn(), &orphan.id).unwrap().is_none());
    assert!(notifier.notifications().is_empty());
}

// ---------------------------------------------------------------------------
// §3.5 投递与降级：权限两态；免打扰内外；专注块通知不重复
// ---------------------------------------------------------------------------

#[test]
fn granted_permission_delivers_system_notifications() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    let task = add_task(&db.db, &day.id, "Write report", NOW);
    set_reminder(&db.db, "task", &task.id, NOW + 1_000, false);
    let notifier = notifier_granted();

    let report = reconcile_with(&db.db, notifier.as_ref(), NOW + 9_000);
    assert_eq!(report.notified, 1);
    let notifications = notifier.notifications();
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].target_id, task.id);
    assert_eq!(notifications[0].target_kind, "task");
}

#[test]
fn denied_permission_still_records_the_reminder_in_app() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    let task = add_task(&db.db, &day.id, "Write report", NOW);
    set_reminder(&db.db, "task", &task.id, NOW + 1_000, false);
    let notifier = notifier_denied();

    let report = reconcile_with(&db.db, notifier.as_ref(), NOW + 9_000);
    assert_eq!(report.notified, 0, "no system notification");
    assert_eq!(report.in_app_only, 1, "the in-app surface keeps it");
    assert_eq!(
        report.last_error.as_deref(),
        Some("notification_permission_denied"),
        "the denial is recorded for the settings page (§3.3)"
    );
    assert!(notifier.notifications().is_empty());

    let alerts = reminders::list_reminders(&db.db, None, repo::StatusFilter::Fired).unwrap();
    assert_eq!(alerts.len(), 1, "fired but not dismissed = in-app alert");
}

#[test]
fn native_submission_failure_keeps_the_fired_reminder_and_records_error() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    let task = add_task(&db.db, &day.id, "Write report", NOW);
    set_reminder(&db.db, "task", &task.id, NOW + 1_000, false);

    let report = reconcile_with(&db.db, Arc::new(FailingNotifier).as_ref(), NOW + 9_000);
    assert_eq!(report.notified, 0);
    assert_eq!(report.in_app_only, 1);
    assert_eq!(
        report.last_error.as_deref(),
        Some("native notification submission failed")
    );
    assert_eq!(
        reminders::list_reminders(&db.db, None, repo::StatusFilter::Fired)
            .unwrap()
            .len(),
        1,
        "the fired row remains the in-app fallback"
    );
}

#[test]
fn desktop_notifier_reports_system_managed_permission() {
    let notifier = SystemNotifier::new();
    assert_eq!(notifier.permission(), "system_managed");
    assert_eq!(notifier.request_permission(), "system_managed");
}

#[test]
fn empty_scheduler_pass_keeps_the_last_submission_error() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    let task = add_task(&db.db, &day.id, "Write report", NOW);
    set_reminder(&db.db, "task", &task.id, NOW + 1_000, false);

    let scheduler = Scheduler::new(db.db.clone(), Arc::new(FailingNotifier), None);
    let first = scheduler.run_pass(NOW + 9_000).expect("first pass");
    assert_eq!(first.in_app_only, 1);
    assert_eq!(
        scheduler.delivery_status().last_error.as_deref(),
        Some("native notification submission failed")
    );

    let second = scheduler.run_pass(NOW + 10_000).expect("empty pass");
    assert_eq!(second.in_app_only, 0);
    assert_eq!(
        scheduler.delivery_status().last_error.as_deref(),
        Some("native notification submission failed")
    );
}

#[test]
fn successful_submission_clears_a_previous_error_after_an_empty_pass() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    let first_task = add_task(&db.db, &day.id, "First report", NOW);
    set_reminder(&db.db, "task", &first_task.id, NOW + 1_000, false);

    let second_task = add_task(&db.db, &day.id, "Second report", NOW);
    let notifier = Arc::new(SequenceNotifier::new([
        DeliveryOutcome::Failed("native notification submission failed".into()),
        DeliveryOutcome::Delivered,
    ]));
    let scheduler = Scheduler::new(db.db.clone(), notifier, None);

    scheduler.run_pass(NOW + 9_000).expect("failed pass");
    assert_eq!(
        scheduler.delivery_status().last_error.as_deref(),
        Some("native notification submission failed")
    );
    scheduler.run_pass(NOW + 10_000).expect("empty pass");
    assert_eq!(
        scheduler.delivery_status().last_error.as_deref(),
        Some("native notification submission failed")
    );

    set_reminder(&db.db, "task", &second_task.id, NOW + 11_000, false);
    scheduler.run_pass(NOW + 12_000).expect("successful pass");
    assert_eq!(scheduler.delivery_status().last_error, None);
}

#[test]
fn quiet_hours_suppress_only_marked_reminders_inside_the_window() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    reminders::set_settings(
        &db.db,
        Some(QuietHours {
            start: "23:00".into(),
            end: "07:00".into(),
        }),
        None,
    )
    .unwrap();

    let soft_night = add_task(&db.db, &day.id, "soft night", NOW);
    let hard_night = add_task(&db.db, &day.id, "hard night", NOW);
    let soft_day = add_task(&db.db, &day.id, "soft day", NOW);
    // Inside the wrapping window (23:00-07:00) and outside of it.
    let night_ms = local_ms(today_date(), 23, 30);
    let noon_ms = local_ms(today_date(), 12, 0);
    set_reminder(&db.db, "task", &soft_night.id, night_ms, true);
    set_reminder(&db.db, "task", &hard_night.id, night_ms + 1, false);
    set_reminder(&db.db, "task", &soft_day.id, noon_ms, true);

    let notifier = notifier_granted();
    let report = reconcile_with(&db.db, notifier.as_ref(), night_ms + 60_000);
    assert_eq!(
        report.notified, 2,
        "hard reminder and daytime reminder notify"
    );
    assert_eq!(
        report.in_app_only, 1,
        "only the quiet-ok reminder is silenced"
    );
    let targets: Vec<String> = notifier
        .notifications()
        .iter()
        .map(|n| n.target_id.clone())
        .collect();
    assert!(targets.contains(&hard_night.id));
    assert!(targets.contains(&soft_day.id));
    assert!(!targets.contains(&soft_night.id));
}

#[test]
fn quiet_window_boundaries_wrap_midnight() {
    let window = (23 * 60, 7 * 60);
    let today = today_date();
    assert!(reminders::is_in_quiet_window(
        window,
        local_ms(today, 23, 0)
    ));
    assert!(reminders::is_in_quiet_window(
        window,
        local_ms(today, 3, 30)
    ));
    assert!(reminders::is_in_quiet_window(
        window,
        local_ms(today, 6, 59)
    ));
    assert!(!reminders::is_in_quiet_window(
        window,
        local_ms(today, 7, 0)
    ));
    assert!(!reminders::is_in_quiet_window(
        window,
        local_ms(today, 22, 59)
    ));
}

#[test]
fn focus_block_delivery_is_exactly_once() {
    let db = TestDb::open();
    let long_term = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &long_term.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, NOW);
    let session = start_session(&db.db, &day.id, 900_000);
    // Coordination contract with add-onboarding-and-lifecycle §6: the block's
    // end notification is one reminder row at started_at + duration. Setting
    // it twice (two code paths racing) still yields one row.
    let first = set_reminder(&db.db, "session", &session.id, NOW + 900_000, false);
    let second = set_reminder(&db.db, "session", &session.id, NOW + 900_000, false);
    assert_eq!(first.id, second.id);

    let notifier = notifier_granted();
    let report = reconcile_with(&db.db, notifier.as_ref(), NOW + 900_000);
    assert_eq!(report.notified, 1, "delivered once at planned length");
    let again = reconcile_with(&db.db, notifier.as_ref(), NOW + 1_800_000);
    assert_eq!(again.notified, 0, "never a second notification");
    assert_eq!(notifier.notifications().len(), 1);
    assert_eq!(notifier.notifications()[0].target_kind, "session");
}

// ---------------------------------------------------------------------------
// §4.4 启动补偿：无 / 少 / 多；摘要关闭后不再出现
// ---------------------------------------------------------------------------

#[test]
fn missed_summary_is_empty_without_due_reminders() {
    let db = TestDb::open();
    let summary = reminders::collect_missed(&db.db, NOW).unwrap();
    assert_eq!(summary.total, 0);
    assert!(summary.items.is_empty());
    assert!(!summary.has_more);
}

#[test]
fn missed_summary_covers_a_few_reminders_fully() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    let one = add_task(&db.db, &day.id, "one", NOW);
    let two = add_task(&db.db, &day.id, "two", NOW);
    set_reminder(&db.db, "task", &one.id, NOW - 500, false);
    set_reminder(&db.db, "task", &two.id, NOW - 100, false);

    let summary = reminders::collect_missed(&db.db, NOW).unwrap();
    assert_eq!(summary.total, 2);
    assert_eq!(summary.items.len(), 2);
    assert!(!summary.has_more);
    assert_eq!(summary.ids.len(), 2);
    assert_eq!(
        summary.items[0].title.as_deref(),
        Some("one"),
        "ordered by fire_at"
    );
}

#[test]
fn missed_summary_caps_details_at_three() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    for index in 0..5 {
        let task = add_task(&db.db, &day.id, &format!("t{index}"), NOW);
        set_reminder(&db.db, "task", &task.id, NOW - 1_000 + index, false);
    }

    let summary = reminders::collect_missed(&db.db, NOW).unwrap();
    assert_eq!(summary.total, 5);
    assert_eq!(summary.items.len(), 3, "at most three details");
    assert!(summary.has_more, "the rest folds behind the count");
    assert_eq!(summary.ids.len(), 5, "closing can still mark everything");
}

#[test]
fn acknowledged_summary_never_returns() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    let task = add_task(&db.db, &day.id, "one", NOW);
    let reminder = set_reminder(&db.db, "task", &task.id, NOW - 1_000, false);

    let summary = reminders::collect_missed(&db.db, NOW).unwrap();
    assert_eq!(summary.total, 1);
    let acknowledged = reminders::acknowledge_missed(&db.db, &summary.ids, NOW + 1).unwrap();
    assert_eq!(acknowledged, 1);

    let stored = repo::get(&db.conn(), &reminder.id).unwrap().unwrap();
    assert!(stored.dismissed_at.is_some(), "closing marks handled");
    let again = reminders::collect_missed(&db.db, NOW + 2_000).unwrap();
    assert_eq!(again.total, 0, "the summary does not come back (§4.4)");
}

#[test]
fn completed_while_away_is_not_surfaced_in_the_summary() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    let task = add_task(&db.db, &day.id, "done already", NOW);
    set_reminder(&db.db, "task", &task.id, NOW - 1_000, false);
    tasks::patch_task(
        &db.db,
        &task.id,
        &tasks::TaskPatch {
            completed: Some(true),
            ..Default::default()
        },
    )
    .unwrap();

    let summary = reminders::collect_missed(&db.db, NOW).unwrap();
    assert_eq!(summary.total, 0, "completed work needs no catch-up");
}

#[test]
fn reminders_due_while_running_are_not_missed_summary_material() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    let task = add_task(&db.db, &day.id, "live", NOW);
    set_reminder(&db.db, "task", &task.id, NOW + 1_000, false);
    reconcile_with(&db.db, notifier_granted().as_ref(), NOW + 2_000);

    let summary = reminders::collect_missed(&db.db, NOW + 3_000).unwrap();
    assert_eq!(summary.total, 0, "delivered reminders are not re-reported");
}

#[test]
fn scheduler_caches_the_summary_until_acknowledged() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    let task = add_task(&db.db, &day.id, "one", NOW);
    set_reminder(&db.db, "task", &task.id, NOW - 1_000, false);
    let scheduler = Scheduler::new(db.db.clone(), notifier_granted(), None);

    let first = scheduler.collect_missed(NOW).unwrap();
    assert_eq!(first.total, 1);
    // StrictMode double-mount / focus refetch: same summary, no double count.
    let second = scheduler.collect_missed(NOW + 1).unwrap();
    assert_eq!(second, first);

    scheduler.acknowledge_missed(&first.ids, NOW + 2).unwrap();
    let third = scheduler.collect_missed(NOW + 3).unwrap();
    assert_eq!(third.total, 0);
}

// ---------------------------------------------------------------------------
// 调度器线程：到期即投递（§6.2 的自动化等价物）
// ---------------------------------------------------------------------------

#[test]
fn scheduler_thread_delivers_when_the_trigger_time_reaches() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    let task = add_task(&db.db, &day.id, "soon", NOW);
    let notifier = notifier_granted();
    let scheduler = Scheduler::new(db.db.clone(), notifier.clone(), None).spawn();

    // Establish that the newly spawned thread completed its first empty-queue
    // pass and is now allowed to take the long idle wait. The command layer
    // must wake it after committing a new reminder; extending this deadline
    // would only hide the production race.
    let empty_pass_deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while settings_repo::get(&db.conn(), reminders::KEY_LAST_SEEN_AT)
        .expect("read scheduler heartbeat")
        .is_none()
    {
        assert!(
            std::time::Instant::now() < empty_pass_deadline,
            "scheduler did not complete its initial empty-queue pass"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    let fire_at = planner_lib::service::now_ms() + 120;
    set_reminder(&db.db, "task", &task.id, fire_at, false);
    // This is the command-layer boundary used by set/update/delete reminder
    // commands after their transaction and invalidation event are complete.
    scheduler.wake();

    // Poll instead of a fixed sleep: the suite runs tests in parallel, so
    // the scheduler thread may not be scheduled promptly. Fails fast when
    // healthy, tolerates load.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let notifications = loop {
        let notifications = notifier.notifications();
        if notifications.len() == 1 || std::time::Instant::now() > deadline {
            break notifications;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    assert_eq!(notifications.len(), 1, "the single timer fires on time");
    assert_eq!(notifications[0].target_id, task.id);
    assert!(
        notifier.notifications().len() <= 1,
        "and exactly once, not per timer tick"
    );
    drop(scheduler);
}

// ---------------------------------------------------------------------------
// 提醒的可管理性：修改时间后旧时间失效；冲突复用
// ---------------------------------------------------------------------------

#[test]
fn rescheduling_invalidates_the_old_trigger_time() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    let task = add_task(&db.db, &day.id, "reschedule me", NOW);
    let reminder = set_reminder(&db.db, "task", &task.id, NOW + 1_000, false);

    let updated = reminders::update_reminder(
        &db.db,
        &reminder.id,
        &UpdateReminderArgs {
            fire_at: NOW + 60_000,
            quiet_ok: Some(true),
        },
    )
    .unwrap();
    assert_eq!(updated.id, reminder.id);
    assert_eq!(updated.fire_at, NOW + 60_000);
    assert!(updated.quiet_ok);

    let notifier = notifier_granted();
    let at_old_time = reconcile_with(&db.db, notifier.as_ref(), NOW + 2_000);
    assert_eq!(at_old_time.notified, 0, "the old time no longer fires");
    let at_new_time = reconcile_with(&db.db, notifier.as_ref(), NOW + 61_000);
    assert_eq!(at_new_time.notified, 1, "the new time fires");
}

#[test]
fn rescheduling_onto_an_existing_reminder_reuses_that_row() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    let task = add_task(&db.db, &day.id, "same target", NOW);
    let first = set_reminder(&db.db, "task", &task.id, NOW + 1_000, false);
    let second = set_reminder(&db.db, "task", &task.id, NOW + 2_000, false);

    let merged = reminders::update_reminder(
        &db.db,
        &first.id,
        &UpdateReminderArgs {
            fire_at: NOW + 2_000,
            quiet_ok: None,
        },
    )
    .unwrap();
    assert_eq!(merged.id, second.id, "the existing row wins");
    let all = reminders::list_reminders(&db.db, None, repo::StatusFilter::Pending).unwrap();
    assert_eq!(all.len(), 1);
}

#[test]
fn fired_reminders_cannot_be_rescheduled() {
    let db = TestDb::open();
    let day = day_fixture(&db);
    let task = add_task(&db.db, &day.id, "already fired", NOW);
    let reminder = set_reminder(&db.db, "task", &task.id, NOW - 1_000, false);
    reconcile_with(&db.db, notifier_granted().as_ref(), NOW);

    let err = reminders::update_reminder(
        &db.db,
        &reminder.id,
        &UpdateReminderArgs {
            fire_at: NOW + 1_000,
            quiet_ok: None,
        },
    )
    .unwrap_err();
    assert!(matches!(err, planner_lib::error::AppError::Conflict { .. }));
}

// ---------------------------------------------------------------------------
// §1.4 设置项：读写与校验；§5.2 日计划提醒
// ---------------------------------------------------------------------------

#[test]
fn settings_round_trip_and_validation() {
    let db = TestDb::open();
    let initial = reminders::get_settings(&db.db).unwrap();
    assert_eq!(initial.quiet_hours, None);
    assert_eq!(initial.daily_plan_time, None);

    reminders::set_settings(
        &db.db,
        Some(QuietHours {
            start: "23:00".into(),
            end: "07:00".into(),
        }),
        Some("08:30".into()),
    )
    .unwrap();
    let stored = reminders::get_settings(&db.db).unwrap();
    assert_eq!(
        stored.quiet_hours,
        Some(QuietHours {
            start: "23:00".into(),
            end: "07:00".into()
        })
    );
    assert_eq!(stored.daily_plan_time.as_deref(), Some("08:30"));

    reminders::set_settings(&db.db, None, None).unwrap();
    let cleared = reminders::get_settings(&db.db).unwrap();
    assert_eq!(cleared.quiet_hours, None);
    assert_eq!(cleared.daily_plan_time, None);

    for broken in ["25:00", "7:00", "0700", "aa:bb", "12:60"] {
        assert!(
            reminders::parse_hh_mm(broken).is_none(),
            "'{broken}' is not a valid HH:MM"
        );
    }
    assert_eq!(reminders::parse_hh_mm("23:07"), Some(23 * 60 + 7));

    let same_bounds = QuietHours {
        start: "09:00".into(),
        end: "09:00".into(),
    };
    let err = reminders::set_settings(&db.db, Some(same_bounds), None).unwrap_err();
    assert!(matches!(
        err,
        planner_lib::error::AppError::Validation { .. }
    ));
}

#[test]
fn daily_plan_reminder_is_ensured_for_existing_days_only() {
    let db = TestDb::open();
    reminders::set_settings(&db.db, None, Some("23:59".into())).unwrap();

    // No day cycle yet: nothing to attach to, and none is auto-created.
    let real_now = planner_lib::service::now_ms();
    assert!(!reminders::ensure_daily_plan_reminder(&db.db, real_now).unwrap());

    let long_term = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &long_term.id, TODAY);
    // The daily plan attaches to *today* in local time, so the fixture day
    // must be the real local date, not the fixed test date.
    let real_today = planner_lib::domain::calendar::today_local();
    let real_today_str = planner_lib::domain::calendar::format_date(real_today);
    let day = create_day(&db.db, &week.id, &real_today_str, NOW);

    assert!(reminders::ensure_daily_plan_reminder(&db.db, real_now).unwrap());
    assert!(
        !reminders::ensure_daily_plan_reminder(&db.db, real_now + 1).unwrap(),
        "idempotent per day"
    );
    let pending = reminders::list_reminders(&db.db, None, repo::StatusFilter::Pending).unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].target_kind, repo::TargetKind::Day);
    assert_eq!(pending[0].target_id, day.id);
    assert!(pending[0].quiet_ok, "the daily plan nudge is suppressible");
    let expected = local_ms(real_today, 23, 59);
    assert_eq!(pending[0].fire_at, expected);
}

// ---------------------------------------------------------------------------
// Notifier trait 契约（记录器的降级语义）与目标校验
// ---------------------------------------------------------------------------

#[test]
fn recording_notifier_reports_denial_as_data() {
    let recorder = RecordingNotifier::with_permission("denied");
    let outcome = recorder.notify(&NotificationContent {
        reminder_id: "r1".into(),
        target_kind: "task".into(),
        target_id: "t1".into(),
        title: "t".into(),
        body: "b".into(),
    });
    assert_eq!(outcome, DeliveryOutcome::PermissionDenied);
    assert_eq!(recorder.permission_token(), "denied");
    assert!(recorder.notifications().is_empty());

    let granted = RecordingNotifier::with_permission("granted");
    let outcome = granted.notify(&NotificationContent {
        reminder_id: "r1".into(),
        target_kind: "task".into(),
        target_id: "t1".into(),
        title: "t".into(),
        body: "b".into(),
    });
    assert_eq!(outcome, DeliveryOutcome::Delivered);
    assert_eq!(granted.notifications().len(), 1);
}

#[test]
fn set_reminder_requires_an_existing_target() {
    let db = TestDb::open();
    let err = reminders::set_reminder(
        &db.db,
        &SetReminderArgs {
            target_kind: "task".into(),
            target_id: "missing".into(),
            fire_at: NOW,
            quiet_ok: false,
        },
        NOW,
    )
    .unwrap_err();
    assert!(matches!(err, planner_lib::error::AppError::NotFound { .. }));

    let err = reminders::set_reminder(
        &db.db,
        &SetReminderArgs {
            target_kind: "calendar".into(),
            target_id: "x".into(),
            fire_at: NOW,
            quiet_ok: false,
        },
        NOW,
    )
    .unwrap_err();
    assert!(matches!(
        err,
        planner_lib::error::AppError::Validation { .. }
    ));
}

#[test]
fn day_target_requires_a_day_cycle() {
    let db = TestDb::open();
    let long_term = create_long_term(&db.db, TODAY, 1);
    let err = reminders::set_reminder(
        &db.db,
        &SetReminderArgs {
            target_kind: "day".into(),
            target_id: long_term.id.clone(),
            fire_at: NOW,
            quiet_ok: false,
        },
        NOW,
    )
    .unwrap_err();
    assert!(matches!(err, planner_lib::error::AppError::NotFound { .. }));
}

/// onboarding §6.4: starting a focus block schedules its due notice on the
/// backend inside the start transaction, so it fires through the scheduler
/// whether or not the app is in the foreground — no frontend timer involved.
#[test]
fn session_start_schedules_background_due_notice() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, NOW);
    let session = cycles::add_session(
        &db.db,
        &cycles::AddSessionArgs {
            day_cycle_id: day.id.clone(),
            title: "background block".into(),
            duration_ms: Some(900_000),
            position: None,
        },
        NOW,
    )
    .unwrap()
    .value;
    let started_at = NOW + 5_000;
    cycles::start_cycle(&db.db, &session.id, started_at).unwrap();

    let notifier = notifier_granted();
    // Not due yet: the notice must not fire one minute before the end.
    let early = reconcile_with(&db.db, notifier.as_ref(), started_at + 60_000 - 1);
    assert_eq!(early.notified, 0);

    // At started_at + duration the backend scheduler delivers by itself.
    let pass = reconcile_with(&db.db, notifier.as_ref(), started_at + 900_000);
    assert_eq!(pass.notified, 1, "fires without any frontend involvement");
    assert_eq!(notifier.notifications()[0].target_kind, "session");

    // Finishing afterwards stays silent; the fired reminder never rearms.
    cycles::finish_cycle(&db.db, &session.id, started_at + 1_800_000).unwrap();
    let after = reconcile_with(&db.db, notifier.as_ref(), started_at + 3_600_000);
    assert_eq!(after.notified, 0);
}

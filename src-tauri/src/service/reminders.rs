//! Reminder use cases (change: add-reminders-notifications): definition,
//! scheduling, delivery degradation and startup compensation.
//!
//! Behaviour contract: `openspec/changes/add-reminders-notifications/`
//! (specs/reminders). Design decisions live in that change's `design.md`:
//! D1 in-process scheduling + startup compensation, D2 reminders carry no
//! business logic, D3 quiet hours suppress the system notification but never
//! the record, D4 focus-block end notifications converge onto this scheduler.
//!
//! Coordination point with `add-onboarding-and-lifecycle` §6: a focus block's
//! end notification is carried by a reminder with `target_kind = 'session'`
//! and `fire_at = started_at + duration` (one row per block thanks to the
//! `(target_kind, target_id, fire_at)` UNIQUE index). That change *creates*
//! those rows when a block starts; this module *delivers* them exactly once
//! (claim-first `fired_at` write), silently when the block already finished,
//! and purges them when the block is deleted. The block-end behavior stays
//! "notify once at planned length; no duration, no notification".
//!
//! The scheduler is a plain OS thread with one Condvar-timed sleep (see
//! [`Scheduler`]): no extra async runtime dependency, and every wake-up
//! re-reads the database, so timer drift and machine sleep are corrected by
//! the next pass plus explicit `reconcile` calls.

use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use chrono::{Local, TimeZone, Timelike};
use rusqlite::Connection;

use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::repository::{cycles as cycles_repo, reminders as repo, settings as settings_repo};
use crate::service::now_ms;

// ---------------------------------------------------------------------------
// Settings (tasks §1.4)
// ---------------------------------------------------------------------------

/// `app_settings` key: `"HH:MM-HH:MM"`, e.g. `"23:00-07:00"`. Absent = no
/// quiet hours. The window may wrap midnight (start > end).
pub const KEY_QUIET_HOURS: &str = "quiet_hours";
/// `app_settings` key: `"HH:MM"` — the daily plan reminder time. Absent = off.
pub const KEY_DAILY_PLAN_TIME: &str = "daily_plan_time";
/// `app_settings` key: last observed wall-clock (ms). Written on every
/// scheduler pass; the startup compensation reads it as "when the app was
/// last seen running" (tasks §4.1).
pub const KEY_LAST_SEEN_AT: &str = "last_seen_at";

/// Quiet-hours window, `HH:MM` bounds, as it travels across IPC. Stored as
/// the single string `"{start}-{end}"` in `app_settings`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QuietHours {
    /// Inclusive start, `HH:MM`.
    pub start: String,
    /// Exclusive end, `HH:MM`. `start != end`; `start > end` wraps midnight.
    pub end: String,
}

/// Parses `"HH:MM"` into minutes since midnight. Strict: two zero-padded
/// components, `HH < 24`, `MM < 60`.
pub fn parse_hh_mm(raw: &str) -> Option<i64> {
    let (h, m) = raw.split_once(':')?;
    if h.len() != 2
        || m.len() != 2
        || !h.bytes().all(|b| b.is_ascii_digit())
        || !m.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let hour: i64 = h.parse().ok()?;
    let minute: i64 = m.parse().ok()?;
    if hour > 23 || minute > 59 {
        return None;
    }
    Some(hour * 60 + minute)
}

/// Parses the stored `"HH:MM-HH:MM"` form into `(start, end)` minutes.
pub fn parse_quiet_hours(raw: &str) -> Option<(i64, i64)> {
    let (start, end) = raw.split_once('-')?;
    let start = parse_hh_mm(start)?;
    let end = parse_hh_mm(end)?;
    (start != end).then_some((start, end))
}

/// Local minutes-since-midnight of a wall-clock timestamp. Ambiguous local
/// times (DST transitions) resolve to "not in the window" — suppression is a
/// courtesy, never a reason to fail a pass.
fn local_minute_of_day(at_ms: i64) -> Option<i64> {
    let dt = Local.timestamp_millis_opt(at_ms).single()?;
    Some((dt.time().num_seconds_from_midnight() / 60) as i64)
}

/// Whether `at_ms` falls into the half-open window `[start, end)`, in local
/// time. A wrapping window (`start > end`) covers midnight.
pub fn is_in_quiet_window(window: (i64, i64), at_ms: i64) -> bool {
    let Some(minute) = local_minute_of_day(at_ms) else {
        return false;
    };
    let (start, end) = window;
    if start < end {
        minute >= start && minute < end
    } else {
        minute >= start || minute < end
    }
}

/// The reminder settings snapshot, as it travels across IPC.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ReminderSettings {
    pub quiet_hours: Option<QuietHours>,
    pub daily_plan_time: Option<String>,
}

pub fn get_settings(db: &Db) -> AppResult<ReminderSettings> {
    let conn = db.pool().get()?;
    let quiet_hours = settings_repo::get(&conn, KEY_QUIET_HOURS)?
        .and_then(|raw| parse_quiet_hours(&raw))
        .map(|(start, end)| QuietHours {
            start: format_hh_mm(start),
            end: format_hh_mm(end),
        });
    let daily_plan_time =
        settings_repo::get(&conn, KEY_DAILY_PLAN_TIME)?.filter(|raw| parse_hh_mm(raw).is_some());
    Ok(ReminderSettings {
        quiet_hours,
        daily_plan_time,
    })
}

fn format_hh_mm(minutes: i64) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// Full replace: every field is written (`None` = off), so the caller always
/// sends the complete desired state read from `get_reminder_settings`.
pub fn set_settings(
    db: &Db,
    quiet_hours: Option<QuietHours>,
    daily_plan_time: Option<String>,
) -> AppResult<()> {
    let quiet_raw = match &quiet_hours {
        Some(window) => {
            let start = parse_hh_mm(&window.start).ok_or_else(|| {
                AppError::validation("invalid_quiet_hours", "quiet hours must be HH:MM-HH:MM")
            })?;
            let end = parse_hh_mm(&window.end).ok_or_else(|| {
                AppError::validation("invalid_quiet_hours", "quiet hours must be HH:MM-HH:MM")
            })?;
            if start == end {
                return Err(AppError::validation(
                    "invalid_quiet_hours",
                    "quiet hours start and end cannot be identical",
                ));
            }
            format!("{}-{}", window.start, window.end)
        }
        None => String::new(),
    };
    if let Some(time) = &daily_plan_time {
        if parse_hh_mm(time).is_none() {
            return Err(AppError::validation(
                "invalid_daily_plan_time",
                "daily plan time must be HH:MM",
            ));
        }
    }
    let conn = db.pool().get()?;
    if quiet_hours.is_some() {
        settings_repo::set(&conn, KEY_QUIET_HOURS, &quiet_raw)?;
    } else {
        settings_repo::set(&conn, KEY_QUIET_HOURS, "")?;
    }
    settings_repo::set(
        &conn,
        KEY_DAILY_PLAN_TIME,
        daily_plan_time.as_deref().unwrap_or(""),
    )?;
    Ok(())
}

/// The parsed quiet window, if valid and enabled.
fn quiet_window(conn: &Connection) -> AppResult<Option<(i64, i64)>> {
    Ok(settings_repo::get(conn, KEY_QUIET_HOURS)?.and_then(|raw| parse_quiet_hours(&raw)))
}

// ---------------------------------------------------------------------------
// Definition: set / update / delete / list (spec: 提醒的创建与目标, 可管理性)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Deserialize)]
pub struct SetReminderArgs {
    /// `"task" | "session" | "day" | "cycle"`.
    pub target_kind: String,
    pub target_id: String,
    /// Milliseconds since the Unix epoch. May be in the past (fires on the
    /// next pass); the scheduler never depends on when it was defined.
    pub fire_at: i64,
    /// Whether quiet hours may silence this reminder's system notification.
    pub quiet_ok: bool,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct UpdateReminderArgs {
    pub fire_at: i64,
    /// `None` keeps the row's current flag.
    pub quiet_ok: Option<bool>,
}

fn require_target(conn: &Connection, kind: repo::TargetKind, target_id: &str) -> AppResult<()> {
    match kind {
        repo::TargetKind::Task => {
            if tasks_repo_exists(conn, target_id)? {
                return Ok(());
            }
        }
        repo::TargetKind::Session => {
            if let Some(cycle) = cycles_repo::get(conn, target_id)? {
                if crate::domain::cycle::CycleType::Session == cycle.cycle_type {
                    return Ok(());
                }
            }
        }
        repo::TargetKind::Day => {
            if let Some(cycle) = cycles_repo::get(conn, target_id)? {
                if crate::domain::cycle::CycleType::Day == cycle.cycle_type {
                    return Ok(());
                }
            }
        }
        repo::TargetKind::Cycle => {
            if cycles_repo::get(conn, target_id)?.is_some() {
                return Ok(());
            }
        }
    }
    Err(AppError::not_found(kind.as_str(), target_id))
}

fn tasks_repo_exists(conn: &Connection, id: &str) -> AppResult<bool> {
    Ok(crate::repository::tasks::get(conn, id)?.is_some())
}

/// Sets (or re-arms) one reminder. The reuse rule (spec: 重复设定同一提醒):
/// an existing row for the same `(target, fire_at)` is reused — never a
/// second row — and a row that already fired re-arms so an explicit re-set
/// fires again.
pub fn set_reminder(db: &Db, args: &SetReminderArgs, now: i64) -> AppResult<repo::Reminder> {
    let kind = repo::TargetKind::parse(&args.target_kind).ok_or_else(|| {
        AppError::validation(
            "invalid_target_kind",
            "target_kind must be task, session, day or cycle",
        )
    })?;
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    require_target(&tx, kind, &args.target_id)?;

    let reminder = match repo::find_for_target(&tx, kind, &args.target_id, args.fire_at)? {
        Some(existing) => {
            repo::rearm(&tx, &existing.id, args.quiet_ok)?;
            repo::require(&tx, &existing.id)?
        }
        None => {
            let new = repo::NewReminder {
                id: uuid::Uuid::new_v4().to_string(),
                target_kind: kind,
                target_id: args.target_id.clone(),
                fire_at: args.fire_at,
                quiet_ok: args.quiet_ok,
                created_at: now,
            };
            repo::insert(&tx, &new)?;
            repo::require(&tx, &new.id)?
        }
    };
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(reminder)
}

/// Re-points a pending reminder (spec: 修改提醒时间 — the old time stops
/// existing). Colliding with another row of the same target reuses that row
/// instead of erroring, matching the set-time reuse rule.
pub fn update_reminder(
    db: &Db,
    reminder_id: &str,
    args: &UpdateReminderArgs,
) -> AppResult<repo::Reminder> {
    let quiet_ok = args.quiet_ok;
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let existing = repo::require(&tx, reminder_id)?;
    if !existing.is_pending() {
        return Err(AppError::conflict(
            "reminder_already_fired",
            "A reminder that has fired can no longer be rescheduled",
        ));
    }
    let updated = match repo::find_for_target(
        &tx,
        existing.target_kind,
        &existing.target_id,
        args.fire_at,
    )? {
        Some(other) if other.id != existing.id => {
            // The new time belongs to an existing reminder of the same
            // target: drop the edited row and re-arm that one.
            repo::delete(&tx, &existing.id)?;
            if other.is_pending() {
                if let Some(quiet_ok) = quiet_ok {
                    repo::set_quiet_ok(&tx, &other.id, quiet_ok)?;
                }
            } else {
                repo::rearm(&tx, &other.id, quiet_ok.unwrap_or(other.quiet_ok))?;
            }
            repo::require(&tx, &other.id)?
        }
        _ => {
            let quiet = quiet_ok.unwrap_or(existing.quiet_ok);
            repo::update_trigger(&tx, &existing.id, args.fire_at, quiet)?;
            repo::require(&tx, &existing.id)?
        }
    };
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
    Ok(updated)
}

/// Removes a reminder. Fired rows may be deleted too (housekeeping of the
/// in-app list).
pub fn delete_reminder(db: &Db, reminder_id: &str) -> AppResult<()> {
    let conn = db.pool().get()?;
    repo::require(&conn, reminder_id)?;
    repo::delete(&conn, reminder_id)
}

/// Lists reminders; `cycle_id` scopes to a cycle subtree (its sessions/days,
/// the cycle itself, and its tasks); `status` defaults to pending.
pub fn list_reminders(
    db: &Db,
    cycle_id: Option<&str>,
    status: repo::StatusFilter,
) -> AppResult<Vec<repo::Reminder>> {
    let conn = db.pool().get()?;
    repo::list(&conn, cycle_id, status)
}

#[derive(serde::Serialize)]
pub struct ReminderDisplay {
    #[serde(flatten)]
    pub reminder: repo::Reminder,
    pub title: Option<String>,
}

pub fn list_reminders_with_titles(
    db: &Db,
    cycle_id: Option<&str>,
    status: repo::StatusFilter,
) -> AppResult<Vec<ReminderDisplay>> {
    let conn = db.pool().get()?;
    repo::list(&conn, cycle_id, status)?
        .into_iter()
        .map(|reminder| {
            let title = resolve_target(&conn, reminder.target_kind, &reminder.target_id)?.title;
            Ok(ReminderDisplay { reminder, title })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Deletion cleanup (tasks §1.3) — wired into the service delete paths.
// ---------------------------------------------------------------------------

/// Removes every reminder pointing at one task. Called from
/// `service::tasks::delete_task` inside the same transaction.
pub fn purge_for_task(conn: &Connection, task_id: &str) -> AppResult<usize> {
    repo::purge_for_target(conn, repo::TargetKind::Task, task_id)
}

/// Removes reminders for the complete task FK cascade, including descendants
/// stored in other cycles.
pub fn purge_for_task_impact(conn: &Connection, task_ids: &[String]) -> AppResult<usize> {
    if task_ids.is_empty() {
        return Ok(0);
    }
    repo::purge_for_cycle_impact(conn, &[], task_ids)
}

/// Removes every reminder attached to a deleted cycle subtree: cycle targets
/// in the subtree (sessions, days, cycles) plus tasks living in those cycles.
/// Called from `service::cycles::delete_cycle` inside the same transaction.
pub fn purge_for_cycle(conn: &Connection, cycle_id: &str) -> AppResult<usize> {
    let subtree = cycles_repo::subtree_ids(conn, cycle_id)?;
    repo::purge_for_cycle_subtree(conn, &subtree)
}

/// Removes reminders for a cycle subtree and all task rows the database will
/// cascade when that subtree is deleted.
pub fn purge_for_cycle_impact(
    conn: &Connection,
    cycle_ids: &[String],
    task_ids: &[String],
) -> AppResult<usize> {
    repo::purge_for_cycle_impact(conn, cycle_ids, task_ids)
}

// ---------------------------------------------------------------------------
// Delivery (tasks §2.4, §3; spec: 触发与投递, 免打扰时段, 权限与降级)
// ---------------------------------------------------------------------------

/// Content of one system notification. Reminders carry no business logic
/// (design D2) — the target's own row provides title and copy.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NotificationContent {
    pub reminder_id: String,
    pub target_kind: String,
    pub target_id: String,
    pub title: String,
    pub body: String,
}

/// What the system notification layer reports after one attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeliveryOutcome {
    Delivered,
    /// Permission missing: the in-app surface stays the delivery channel
    /// (spec: 权限被拒绝 — MUST NOT silently vanish).
    PermissionDenied,
    /// The OS rejected the notification (e.g. disabled after being granted).
    Failed(String),
}

/// Injection seam for delivery. Production wires
/// [`crate::commands::reminders::SystemNotifier`] (platform notification
/// adapter);
/// tests wire [`RecordingNotifier`].
pub trait ReminderNotifier: Send + Sync {
    /// Current permission as a stable token. Desktop implementations may
    /// return `"system_managed"` when the OS does not expose a query API.
    fn permission(&self) -> String;
    /// Requests permission when the platform supports it, otherwise returns
    /// the platform's stable `system_managed` state.
    fn request_permission(&self) -> String;
    /// Attempts one system notification; never panics, failure is data.
    fn notify(&self, notification: &NotificationContent) -> DeliveryOutcome;
}

/// Target state at delivery time (spec: 目标已完成 → 静默标记; 已删除 → 清理).
struct TargetInfo {
    exists: bool,
    completed: bool,
    title: Option<String>,
}

fn resolve_target(
    conn: &Connection,
    kind: repo::TargetKind,
    target_id: &str,
) -> AppResult<TargetInfo> {
    let info = match kind {
        repo::TargetKind::Task => {
            crate::repository::tasks::get(conn, target_id)?.map(|task| TargetInfo {
                exists: true,
                completed: task.completed,
                title: Some(task.title),
            })
        }
        repo::TargetKind::Session | repo::TargetKind::Day | repo::TargetKind::Cycle => {
            cycles_repo::get(conn, target_id)?.map(|cycle| TargetInfo {
                exists: true,
                completed: cycle.finished,
                title: Some(cycle.title),
            })
        }
    };
    Ok(info.unwrap_or(TargetInfo {
        exists: false,
        completed: false,
        title: None,
    }))
}

/// Builds the notification copy for one reminder. `None` = target missing.
fn describe(
    conn: &Connection,
    reminder: &repo::Reminder,
    info: &TargetInfo,
) -> Option<NotificationContent> {
    let title = info.title.clone().unwrap_or_default();
    let locale = crate::i18n::current(conn).unwrap_or(crate::i18n::Locale::En);
    let body = match reminder.target_kind {
        repo::TargetKind::Task => crate::i18n::text(locale, "notification.task", &[]),
        repo::TargetKind::Session => crate::i18n::text(locale, "notification.session", &[]),
        repo::TargetKind::Day => {
            let n = crate::repository::tasks::list_visible_by_cycle(conn, &reminder.target_id)
                .map(|tasks| tasks.len())
                .unwrap_or(0);
            crate::i18n::text(
                locale,
                if n == 1 {
                    "notification.day_one"
                } else {
                    "notification.day_other"
                },
                &[("count", n.to_string())],
            )
        }
        repo::TargetKind::Cycle => crate::i18n::text(locale, "notification.cycle", &[]),
    };
    Some(NotificationContent {
        reminder_id: reminder.id.clone(),
        target_kind: reminder.target_kind.as_str().to_string(),
        target_id: reminder.target_id.clone(),
        title,
        body,
    })
}

/// One reconcile pass report.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct PassReport {
    /// System notifications handed to the OS.
    pub notified: usize,
    /// Reminders processed as in-app-only (quiet hours or failed/denied
    /// delivery) — still recorded, still visible (design D3, §3.3).
    pub in_app_only: usize,
    /// Silently marked because the target completed (spec: 目标已完成).
    pub silent: usize,
    /// Purged because the target no longer exists (spec: 目标已删除).
    pub purged: usize,
    /// Last delivery failure of this pass, if any (stable token for
    /// permission denial, otherwise the OS error message).
    pub last_error: Option<String>,
}

/// Reconciles the due queue once (spec: 触发与投递): delivers everything with
/// `fire_at <= now AND fired_at IS NULL`, in `fire_at` ascending order, each
/// row claimed exactly once. Also re-arms the daily plan reminder and bumps
/// `last_seen_at`. Safe to run concurrently from the timer thread, the
/// reconcile command and tests — claims decide.
pub fn reconcile(db: &Db, notifier: &dyn ReminderNotifier, now: i64) -> AppResult<PassReport> {
    ensure_daily_plan_reminder(db, now)?;
    let conn = db.pool().get()?;
    let window = quiet_window(&conn)?;
    let due = repo::due(&conn, now)?;
    let mut report = PassReport::default();

    for reminder in due {
        let info = resolve_target(&conn, reminder.target_kind, &reminder.target_id)?;
        if !info.exists {
            repo::delete(&conn, &reminder.id)?;
            report.purged += 1;
            continue;
        }
        if info.completed {
            // Finished work needs no nudge (对外行为不变: a manually finished
            // focus block never notifies). Claimed, never delivered.
            if repo::claim_fired(&conn, &reminder.id, now)? {
                report.silent += 1;
            }
            continue;
        }
        // Claim first: whichever pass wins delivers, the others skip. This is
        // what keeps concurrent reconcile + timer from duplicating a focus
        // block notification (tasks §3.5).
        if !repo::claim_fired(&conn, &reminder.id, now)? {
            continue;
        }
        let suppressed = reminder.quiet_ok
            && window
                .map(|w| is_in_quiet_window(w, reminder.fire_at))
                .unwrap_or(false);
        if suppressed {
            report.in_app_only += 1;
            continue;
        }
        if let Some(content) = describe(&conn, &reminder, &info) {
            match notifier.notify(&content) {
                DeliveryOutcome::Delivered => report.notified += 1,
                DeliveryOutcome::PermissionDenied => {
                    report.in_app_only += 1;
                    report.last_error = Some("notification_permission_denied".to_string());
                }
                DeliveryOutcome::Failed(message) => {
                    report.in_app_only += 1;
                    report.last_error = Some(message);
                }
            }
        }
    }

    settings_repo::set(&conn, KEY_LAST_SEEN_AT, &now.to_string())?;
    Ok(report)
}

/// Ensures today's daily-plan reminder exists (tasks §5.2, 沿用
/// `daily_plan_time`). Only attaches to a day cycle that already exists —
/// creating planning pages is the planner's job, not the reminder's. The
/// row is `quiet_ok` (a soft nudge) and idempotent per day via the UNIQUE
/// `(target, fire_at)` index. Public for tests; failures inside the
/// scheduler pass are logged, never fatal.
pub fn ensure_daily_plan_reminder(db: &Db, now: i64) -> AppResult<bool> {
    let conn = db.pool().get()?;
    let Some(minutes) =
        settings_repo::get(&conn, KEY_DAILY_PLAN_TIME)?.and_then(|raw| parse_hh_mm(&raw))
    else {
        return Ok(false);
    };
    let Some(today) = local_date_of(now) else {
        return Ok(false);
    };
    let Some(day) =
        cycles_repo::get_by_calendar_key(&conn, &crate::domain::calendar::day_key(today))?
    else {
        return Ok(false);
    };
    let fire_at = today
        .and_hms_opt((minutes / 60) as u32, (minutes % 60) as u32, 0)
        .and_then(|t| Local.from_local_datetime(&t).single())
        .map(|dt| dt.timestamp_millis());
    let Some(fire_at) = fire_at else {
        return Ok(false);
    };
    if repo::find_for_target(&conn, repo::TargetKind::Day, &day.id, fire_at)?.is_some() {
        return Ok(false);
    }
    repo::insert(
        &conn,
        &repo::NewReminder {
            id: uuid::Uuid::new_v4().to_string(),
            target_kind: repo::TargetKind::Day,
            target_id: day.id.clone(),
            fire_at,
            quiet_ok: true,
            created_at: now,
        },
    )?;
    Ok(true)
}

fn local_date_of(now: i64) -> Option<chrono::NaiveDate> {
    Local
        .timestamp_millis_opt(now)
        .single()
        .map(|dt| dt.date_naive())
}

// ---------------------------------------------------------------------------
// Startup compensation (tasks §4; spec: 错过提醒的启动补偿)
// ---------------------------------------------------------------------------

/// One aggregated line of the missed summary.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MissedItem {
    pub id: String,
    pub target_kind: repo::TargetKind,
    pub target_id: String,
    /// Resolved target title, when the target still exists.
    pub title: Option<String>,
    pub fire_at: i64,
}

/// The startup summary: how many reminders came due while the app was not
/// running (`fire_at ∈ (last_seen_at, now]`), at most three details, and the
/// full id list so closing the summary can mark everything handled.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct MissedSummary {
    pub total: usize,
    pub items: Vec<MissedItem>,
    pub has_more: bool,
    /// Every summarized reminder — `acknowledge_missed` marks these dismissed.
    pub ids: Vec<String>,
}

const MISSED_DETAIL_LIMIT: usize = 3;

/// Claims every due-but-unfired reminder from the away-window as "delivered
/// into the summary" (fired_at = now, no system notification — the summary
/// itself is the delivery, design D1). Deleted targets are purged; targets
/// that completed while away are silently marked and *not* surfaced.
///
/// Running it twice is safe and yields an empty summary the second time,
/// which is exactly the "摘要关闭后不再出现" guarantee.
pub fn collect_missed(db: &Db, now: i64) -> AppResult<MissedSummary> {
    let conn = db.pool().get()?;
    let since = settings_repo::get(&conn, KEY_LAST_SEEN_AT)?
        .and_then(|raw| raw.parse::<i64>().ok())
        .unwrap_or(0);
    let due = repo::due_between(&conn, since, now)?;

    let mut summary = MissedSummary::default();
    for reminder in due {
        let info = resolve_target(&conn, reminder.target_kind, &reminder.target_id)?;
        if !info.exists {
            repo::delete(&conn, &reminder.id)?;
            continue;
        }
        if !repo::claim_fired(&conn, &reminder.id, now)? {
            continue;
        }
        if info.completed {
            continue;
        }
        summary.ids.push(reminder.id.clone());
        if summary.items.len() < MISSED_DETAIL_LIMIT {
            summary.items.push(MissedItem {
                id: reminder.id.clone(),
                target_kind: reminder.target_kind,
                target_id: reminder.target_id.clone(),
                title: info.title,
                fire_at: reminder.fire_at,
            });
        }
    }
    summary.total = summary.ids.len();
    summary.has_more = summary.total > MISSED_DETAIL_LIMIT;
    settings_repo::set(&conn, KEY_LAST_SEEN_AT, &now.to_string())?;
    Ok(summary)
}

/// Closes the summary: every listed reminder becomes dismissed (spec: 摘要被
/// 关闭 → 标记已处理). Also used by the in-app alert surface to dismiss
/// single fired reminders.
pub fn acknowledge_missed(db: &Db, ids: &[String], now: i64) -> AppResult<usize> {
    let conn = db.pool().get()?;
    repo::mark_dismissed(&conn, ids, now)
}

// ---------------------------------------------------------------------------
// Scheduler (tasks §2.1–2.3; spec: 触发与投递)
// ---------------------------------------------------------------------------

/// Upper bound on one sleep, even with nothing due: keeps the daily plan
/// reminder and day rollovers fresh without any user interaction, and bounds
/// the worst-case delivery lag after a machine sleep that no focus event
/// reports.
const MAX_WAIT_MS: i64 = 60_000;

/// Delivery bookkeeping shared between the scheduler thread and the
/// permission commands (tasks §3.1 状态缓存, §3.3 状态记录).
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct DeliveryStatus {
    pub permission: Option<String>,
    pub last_error: Option<String>,
    pub last_delivery_at: Option<i64>,
}

struct SchedulerCore {
    db: Db,
    notifier: Arc<dyn ReminderNotifier>,
    /// Called after a pass (or a collect) changed state, so the frontend can
    /// refresh the in-app surface. Production: emit `reminders:changed`.
    on_change: Option<Box<dyn Fn() + Send + Sync>>,
    state: Mutex<SchedulerShared>,
    signal: Condvar,
}

#[derive(Default)]
struct SchedulerShared {
    woken: bool,
    status: DeliveryStatus,
    /// The cached missed summary so repeated collects (React StrictMode
    /// double-mount, focus refetches) keep returning the same summary until
    /// it is acknowledged or dismissed.
    missed: Option<MissedSummary>,
}

/// The in-process reminder scheduler (tasks §2.1): one timer thread, one
/// database-derived sleep per cycle, an explicit wake for reconcile. Not
/// one-timer-per-reminder.
///
/// Thread choice: a plain `std::thread` sleeping on a [`Condvar`]. The work
/// is blocking SQLite anyway; `tauri::async_runtime` is available but does
/// not expose a sleep, and pulling `tokio::time` into non-test code just for
/// this loop would add a runtime dependency for nothing. Correctness never
/// relies on the timer: every wake-up re-reads the database, and the frontend
/// additionally reconciles on app wake / window focus / settings changes.
#[derive(Clone)]
pub struct Scheduler {
    core: Arc<SchedulerCore>,
}

impl Scheduler {
    pub fn new(
        db: Db,
        notifier: Arc<dyn ReminderNotifier>,
        on_change: Option<Box<dyn Fn() + Send + Sync>>,
    ) -> Self {
        Self {
            core: Arc::new(SchedulerCore {
                db,
                notifier,
                on_change,
                state: Mutex::new(SchedulerShared::default()),
                signal: Condvar::new(),
            }),
        }
    }

    /// Collects reminders missed while the app was closed before the timer
    /// thread can claim them as live deliveries. Detached after startup.
    pub fn spawn(self) -> AppResult<Self> {
        self.collect_missed(now_ms())?;
        let core = self.core.clone();
        let spawned = std::thread::Builder::new()
            .name("reminders-scheduler".into())
            .spawn(move || run_loop(core));
        if let Err(err) = spawned {
            crate::logging::error(
                "reminders",
                &format!("scheduler thread failed to start: {err}"),
            );
        }
        Ok(self)
    }

    /// Reconcile entry (tasks §2.2): wakes the thread so a pass runs now.
    /// Non-blocking, idempotent while a pass is already running.
    pub fn wake(&self) {
        let mut state = self.core.state.lock().expect("scheduler state");
        state.woken = true;
        self.core.signal.notify_one();
    }

    /// Cached startup compensation (tasks §4.1): the first call collects and
    /// remembers; later calls return the same summary until acknowledged.
    pub fn collect_missed(&self, now: i64) -> AppResult<MissedSummary> {
        {
            let state = self.core.state.lock().expect("scheduler state");
            if let Some(cached) = &state.missed {
                return Ok(cached.clone());
            }
        }
        let summary = collect_missed(&self.core.db, now)?;
        if summary.total > 0 {
            let mut state = self.core.state.lock().expect("scheduler state");
            state.missed = Some(summary.clone());
            drop(state);
            self.signal_change();
        }
        Ok(summary)
    }

    /// Closes the summary (tasks §4.3) and drops the cache.
    pub fn acknowledge_missed(&self, ids: &[String], now: i64) -> AppResult<usize> {
        let acknowledged = acknowledge_missed(&self.core.db, ids, now)?;
        let mut state = self.core.state.lock().expect("scheduler state");
        state.missed = None;
        drop(state);
        if acknowledged > 0 {
            self.signal_change();
        }
        Ok(acknowledged)
    }

    /// Reports the platform notification permission state (tasks §3.1).
    pub fn request_permission(&self) -> String {
        let token = self.core.notifier.request_permission();
        let mut state = self.core.state.lock().expect("scheduler state");
        state.status.permission = Some(token.clone());
        token
    }

    /// Platform permission state plus the last recorded delivery failure (§3.3).
    pub fn delivery_status(&self) -> DeliveryStatus {
        let mut state = self.core.state.lock().expect("scheduler state");
        if state.status.permission.is_none() {
            state.status.permission = Some(self.core.notifier.permission());
        }
        state.status.clone()
    }

    fn record_report(&self, report: &PassReport) {
        let mut state = self.core.state.lock().expect("scheduler state");
        update_delivery_status(
            &mut state.status,
            self.core.notifier.as_ref(),
            report,
            now_ms(),
        );
        let changed = pass_changed(report);
        drop(state);
        if changed {
            self.signal_change();
        }
    }

    fn signal_change(&self) {
        if let Some(on_change) = &self.core.on_change {
            on_change();
        }
    }

    /// Runs one reconcile pass synchronously — the timer thread's body, also
    /// used by tests to drive the queue deterministically.
    pub fn run_pass(&self, now: i64) -> AppResult<PassReport> {
        let report = reconcile(&self.core.db, self.core.notifier.as_ref(), now)?;
        self.record_report(&report);
        Ok(report)
    }
}

fn run_loop(core: Arc<SchedulerCore>) {
    loop {
        let now = now_ms();
        match reconcile(&core.db, core.notifier.as_ref(), now) {
            Ok(report) => {
                let mut state = core.state.lock().expect("scheduler state");
                update_delivery_status(&mut state.status, core.notifier.as_ref(), &report, now);
                let changed = pass_changed(&report);
                drop(state);
                if changed {
                    if let Some(on_change) = &core.on_change {
                        on_change();
                    }
                }
            }
            Err(err) => {
                crate::logging::error("reminders", &format!("scheduler pass failed: {err}"))
            }
        }

        let wait =
            Duration::from_millis(next_wait_ms(&core.db).unwrap_or(MAX_WAIT_MS).max(0) as u64);
        let current = core.state.lock().expect("scheduler state");
        // Sleep until woken or the next trigger (capped by MAX_WAIT_MS).
        let (mut guard, _timed_out) = core
            .signal
            .wait_timeout_while(current, wait, |shared| !shared.woken)
            .expect("scheduler signal");
        guard.woken = false;
        drop(guard);
    }
}

fn update_delivery_status(
    status: &mut DeliveryStatus,
    notifier: &dyn ReminderNotifier,
    report: &PassReport,
    delivery_at: i64,
) {
    status.permission = Some(notifier.permission());
    if let Some(error) = &report.last_error {
        status.last_error = Some(error.clone());
    } else if report.notified > 0 {
        status.last_error = None;
    }
    if report.notified + report.in_app_only > 0 {
        status.last_delivery_at = Some(delivery_at);
    }
}

fn pass_changed(report: &PassReport) -> bool {
    report.notified + report.in_app_only + report.silent + report.purged > 0
}

/// Milliseconds until the next pending trigger, capped to [`MAX_WAIT_MS`].
fn next_wait_ms(db: &Db) -> Option<i64> {
    let conn = db.pool().get().ok()?;
    repo::next_pending_fire_at(&conn)
        .ok()
        .flatten()
        .map(|next| (next - now_ms()).clamp(0, MAX_WAIT_MS))
}

// ---------------------------------------------------------------------------
// Test double (tasks §3.5: 权限授予/拒绝两态)
// ---------------------------------------------------------------------------

/// A notifier that records instead of notifying, with a configurable
/// permission state. Test seam for the granted/denied matrix; also a
/// stand-in for "system notifications unavailable".
#[derive(Default)]
pub struct RecordingNotifier {
    state: Mutex<RecordedState>,
}

#[derive(Default)]
struct RecordedState {
    permission: String,
    notifications: Vec<NotificationContent>,
}

impl RecordingNotifier {
    /// Granted by default; pass `"denied"` to simulate rejection.
    pub fn with_permission(permission: &str) -> Self {
        Self {
            state: Mutex::new(RecordedState {
                permission: permission.to_string(),
                ..RecordedState::default()
            }),
        }
    }

    /// Every recorded system notification, in delivery order.
    pub fn notifications(&self) -> Vec<NotificationContent> {
        self.state.lock().expect("recorder").notifications.clone()
    }

    pub fn permission_token(&self) -> String {
        self.state.lock().expect("recorder").permission.clone()
    }
}

impl ReminderNotifier for RecordingNotifier {
    fn permission(&self) -> String {
        self.state.lock().expect("recorder").permission.clone()
    }

    fn request_permission(&self) -> String {
        self.permission()
    }

    fn notify(&self, notification: &NotificationContent) -> DeliveryOutcome {
        let mut state = self.state.lock().expect("recorder");
        if state.permission != "granted" {
            return DeliveryOutcome::PermissionDenied;
        }
        state.notifications.push(notification.clone());
        DeliveryOutcome::Delivered
    }
}

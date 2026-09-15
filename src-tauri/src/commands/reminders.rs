//! Reminder commands (change: add-reminders-notifications).
//!
//! Also owns the production wiring of the reminders scheduler: the
//! tauri-plugin-notification backed notifier and the `reminders:changed`
//! invalidation event. See the coordinator notes in `lib.rs` for the two
//! setup lines this module expects.

use std::sync::{Arc, Mutex};

use tauri::{Emitter, Manager, Runtime, State};
use tauri_plugin_notification::NotificationExt;

use crate::db::Db;
use crate::error::AppResult;
use crate::repository::reminders as repo;
use crate::service::reminders::{
    self, DeliveryStatus, MissedSummary, QuietHours, ReminderSettings, Scheduler, SetReminderArgs,
    UpdateReminderArgs,
};

/// Invalidation notice fired whenever reminders state changed (delivery,
/// dismissal, definition). Payload is empty — the database stays the source
/// of truth (architecture decision D4).
pub const REMINDERS_CHANGED: &str = "reminders:changed";

fn emit_event(app: &tauri::AppHandle<tauri::Wry>) {
    let _ = app.emit(REMINDERS_CHANGED, ());
}

fn emit_changed(app: &tauri::AppHandle<tauri::Wry>) {
    emit_event(app);
    if let Some(scheduler) = app.try_state::<Scheduler>() {
        scheduler.wake();
    }
}

/// System notification delivery backed by tauri-plugin-notification, with an
/// in-memory permission cache (tasks §3.1) so reads never hit the OS.
pub struct SystemNotifier<R: Runtime> {
    app: tauri::AppHandle<R>,
    cache: Mutex<Option<String>>,
}

impl<R: Runtime> SystemNotifier<R> {
    pub fn new(app: tauri::AppHandle<R>) -> Self {
        Self {
            app,
            cache: Mutex::new(None),
        }
    }
}

impl<R: Runtime> reminders::ReminderNotifier for SystemNotifier<R> {
    fn permission(&self) -> String {
        if let Some(cached) = self.cache.lock().expect("permission cache").clone() {
            return cached;
        }
        let token = self
            .app
            .notification()
            .permission_state()
            .map(|state| state.to_string())
            .unwrap_or_else(|_| "prompt".to_string());
        *self.cache.lock().expect("permission cache") = Some(token.clone());
        token
    }

    fn request_permission(&self) -> String {
        let token = self
            .app
            .notification()
            .request_permission()
            .map(|state| state.to_string())
            .unwrap_or_else(|_| "denied".to_string());
        *self.cache.lock().expect("permission cache") = Some(token.clone());
        token
    }

    fn notify(&self, notification: &reminders::NotificationContent) -> reminders::DeliveryOutcome {
        if self.permission() != "granted" {
            return reminders::DeliveryOutcome::PermissionDenied;
        }
        self.app
            .notification()
            .builder()
            .title(&notification.title)
            .body(&notification.body)
            .show()
            .map(|_| reminders::DeliveryOutcome::Delivered)
            .unwrap_or_else(|err| reminders::DeliveryOutcome::Failed(err.to_string()))
    }
}

/// Builds and starts the scheduler for the running app. The coordinator calls
/// this once in `setup` and manages the returned handle:
///
/// ```ignore
/// let scheduler = commands::reminders::start_scheduler(app.handle().clone(), state.clone());
/// app.manage(scheduler);
/// ```
pub fn start_scheduler(app: tauri::AppHandle<tauri::Wry>, db: Db) -> Scheduler {
    let notifier: Arc<dyn reminders::ReminderNotifier> = Arc::new(SystemNotifier::new(app.clone()));
    let emit_app = app;
    Scheduler::new(db, notifier, Some(Box::new(move || emit_event(&emit_app)))).spawn()
}

fn parse_status(status: Option<&str>) -> AppResult<repo::StatusFilter> {
    match status {
        None | Some("pending") => Ok(repo::StatusFilter::Pending),
        Some("fired") => Ok(repo::StatusFilter::Fired),
        Some("all") => Ok(repo::StatusFilter::All),
        Some(other) => Err(crate::error::AppError::validation(
            "invalid_status_filter",
            format!("unknown status filter '{other}'; use pending, fired or all"),
        )),
    }
}

#[tauri::command]
pub fn set_reminder(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    args: SetReminderArgs,
) -> AppResult<repo::Reminder> {
    let reminder = reminders::set_reminder(&db, &args, crate::service::now_ms())?;
    emit_changed(&app);
    Ok(reminder)
}

/// `status`: `"pending"` (default) | `"fired"` (in-app alerts awaiting
/// dismissal) | `"all"`. `cycle_id` scopes to that cycle's subtree.
#[tauri::command]
pub fn list_reminders(
    db: State<'_, Db>,
    cycle_id: Option<String>,
    status: Option<String>,
) -> AppResult<Vec<repo::Reminder>> {
    let filter = parse_status(status.as_deref())?;
    reminders::list_reminders(&db, cycle_id.as_deref(), filter)
}

#[tauri::command]
pub fn update_reminder(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    reminder_id: String,
    args: UpdateReminderArgs,
) -> AppResult<repo::Reminder> {
    let reminder = reminders::update_reminder(&db, &reminder_id, &args)?;
    emit_changed(&app);
    Ok(reminder)
}

#[tauri::command]
pub fn delete_reminder(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    reminder_id: String,
) -> AppResult<()> {
    reminders::delete_reminder(&db, &reminder_id)?;
    emit_changed(&app);
    Ok(())
}

/// Reconcile entry (tasks §2.2): the frontend calls this on app wake and
/// window focus; the scheduler re-reads the due queue immediately.
#[tauri::command]
pub fn reconcile_reminders(scheduler: State<'_, Scheduler>) -> AppResult<()> {
    scheduler.wake();
    Ok(())
}

/// Startup compensation (tasks §4.1/§4.2). Cached: repeated calls return the
/// same summary until it is acknowledged. Deliberately no event here — the
/// scheduler signals `reminders:changed` exactly once when the summary is
/// first collected; emitting per call would loop with the refetch it causes.
#[tauri::command]
pub fn get_missed_summary(scheduler: State<'_, Scheduler>) -> AppResult<MissedSummary> {
    scheduler.collect_missed(crate::service::now_ms())
}

/// Closes the summary (or dismisses in-app alerts): everything listed in
/// `ids` is marked dismissed (tasks §4.3).
#[tauri::command]
pub fn acknowledge_missed_summary(
    app: tauri::AppHandle<tauri::Wry>,
    scheduler: State<'_, Scheduler>,
    ids: Vec<String>,
) -> AppResult<usize> {
    let acknowledged = scheduler.acknowledge_missed(&ids, crate::service::now_ms())?;
    if acknowledged > 0 {
        emit_changed(&app);
    }
    Ok(acknowledged)
}

#[tauri::command]
pub fn get_reminder_settings(db: State<'_, Db>) -> AppResult<ReminderSettings> {
    reminders::get_settings(&db)
}

#[derive(Default, serde::Deserialize)]
pub struct ReminderSettingsArgs {
    /// `null` disables quiet hours.
    pub quiet_hours: Option<QuietHours>,
    /// `null` disables the daily plan reminder.
    pub daily_plan_time: Option<String>,
}

/// Full replace of the reminder settings; also wakes the scheduler so a
/// changed daily plan time is reconciled immediately (tasks §2.2).
#[tauri::command]
pub fn set_reminder_settings(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    scheduler: State<'_, Scheduler>,
    args: ReminderSettingsArgs,
) -> AppResult<()> {
    reminders::set_settings(&db, args.quiet_hours, args.daily_plan_time)?;
    scheduler.wake();
    emit_changed(&app);
    Ok(())
}

/// Asks the OS for notification permission and caches the answer (§3.1).
#[tauri::command]
pub fn request_notification_permission(scheduler: State<'_, Scheduler>) -> AppResult<String> {
    Ok(scheduler.request_permission())
}

/// Cached permission plus the last delivery failure, for the settings page
/// guidance (§3.3).
#[tauri::command]
pub fn get_notification_permission(scheduler: State<'_, Scheduler>) -> AppResult<DeliveryStatus> {
    Ok(scheduler.delivery_status())
}

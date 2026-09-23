//! IPC boundary. Commands validate argument shapes, call `service`, and turn
//! the returned [`Mutation`] into event emissions. They never touch SQL
//! directly; the full command contract lives in `docs/architecture.md`.

pub mod agent;
pub mod ai_settings;
pub mod calendar_view;
pub mod cycles;
#[cfg(target_os = "windows")]
pub mod desktop;
pub mod editor;
pub mod later;
pub mod maintenance;
pub mod onboarding;
pub mod proposals;
pub mod reminders;
pub mod repeats;
pub mod reviews;
pub mod settings;
pub mod tasks;
pub mod trash;

use tauri::{Manager, Runtime};

use crate::events;
use crate::service::Mutation;

/// Emits the invalidation notices carried by a mutation. Payloads never
/// contain business data (architecture decision D4).
fn emit_mutation<R: Runtime, T>(app: &tauri::AppHandle<R>, mutation: &Mutation<T>) {
    events::emit_cycles_changed(app, &mutation.cycles);
    events::emit_tasks_changed(app, &mutation.tasks);
    if let Some(cycle_id) = &mutation.proposal_cycle {
        events::emit_proposals_changed(app, cycle_id);
    }
    if mutation.trash_changed {
        events::emit_trash_changed(app);
    }
    // Mutations can create or remove focus-block reminders in the same
    // transaction. Wake the managed scheduler after the mutation events so a
    // newly committed trigger is not left behind its empty-queue sleep.
    if let Some(scheduler) = app.try_state::<crate::service::reminders::Scheduler>() {
        scheduler.wake();
    }
}

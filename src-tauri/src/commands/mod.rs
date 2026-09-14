//! IPC boundary. Commands validate argument shapes, call `service`, and turn
//! the returned [`Mutation`] into event emissions. They never touch SQL
//! directly; the full command contract lives in `docs/architecture.md`.

pub mod ai_settings;
pub mod cycles;
pub mod editor;
pub mod later;
pub mod maintenance;
pub mod proposals;
pub mod repeats;
pub mod settings;
pub mod tasks;

use tauri::Runtime;

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
}

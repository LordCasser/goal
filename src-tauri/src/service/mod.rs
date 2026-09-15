//! Use-case orchestration: transaction boundaries, cross-aggregate rules and
//! event emission. Commands call into this layer and nothing below it directly.
//!
//! Services do not touch Tauri: a mutation returns which cycles changed
//! ([`Mutation`]) and the command layer turns that into event emissions. The
//! payload contract stays "invalidation notice only".

pub mod calendar;
pub mod cycles;
pub mod editor;
pub mod onboarding;
pub mod proposals;
pub mod reminders;
pub mod repeats;
pub mod reviews;
pub mod settings;
pub mod tasks;

use crate::events::CycleIdSet;

/// Milliseconds since the Unix epoch — the wall-clock unit used by
/// `created_at` / `started_at` / `finished_at` / `duration` / `focused_time`.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Result of one use case: the produced value plus the invalidation notices
/// the command layer should emit.
#[derive(Debug)]
pub struct Mutation<T> {
    pub value: T,
    /// Cycles whose structure changed (created/deleted/reordered/lifecycle).
    pub cycles: CycleIdSet,
    /// Cycles whose tasks changed.
    pub tasks: CycleIdSet,
    /// The single cycle whose pending-proposal count changed, if any.
    pub proposal_cycle: Option<String>,
}

impl<T> Mutation<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            cycles: CycleIdSet::new(),
            tasks: CycleIdSet::new(),
            proposal_cycle: None,
        }
    }
    pub fn touching_cycle(mut self, cycle_id: impl Into<String>) -> Self {
        self.cycles.push(cycle_id);
        self
    }

    pub fn touching_tasks(mut self, cycle_id: impl Into<String>) -> Self {
        self.tasks.push(cycle_id);
        self
    }

    pub fn touching_proposals(mut self, cycle_id: impl Into<String>) -> Self {
        self.proposal_cycle = Some(cycle_id.into());
        self
    }

    pub fn merge(&mut self, other: Mutation<()>) {
        for id in other.cycles.ids() {
            self.cycles.push(id);
        }
        for id in other.tasks.ids() {
            self.tasks.push(id);
        }
        if self.proposal_cycle.is_none() {
            self.proposal_cycle = other.proposal_cycle;
        }
    }
}

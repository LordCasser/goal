//! Typed event emitters.
//!
//! Events are *invalidation notices only*: the payload never carries business
//! data, so the database stays the single source of truth
//! (see `docs/architecture.md`, decision D4).

use serde::Serialize;
use tauri::{Emitter, Runtime};

pub const CYCLES_CHANGED: &str = "cycles:changed";
pub const TASKS_CHANGED: &str = "tasks:changed";
pub const PROPOSALS_CHANGED: &str = "proposals:changed";

#[derive(Debug, Clone, Serialize)]
pub struct CycleIdsPayload {
    pub cycle_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CycleIdPayload {
    pub cycle_id: String,
}

/// Deduplicated set of cycle ids collected during one use case, emitted once.
#[derive(Debug, Default)]
pub struct CycleIdSet(Vec<String>);

impl CycleIdSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, id: impl Into<String>) {
        let id = id.into();
        if !self.0.iter().any(|existing| existing == &id) {
            self.0.push(id);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn ids(&self) -> Vec<String> {
        self.0.clone()
    }
}

pub fn emit_cycles_changed<R: Runtime>(app: &tauri::AppHandle<R>, ids: &CycleIdSet) {
    if ids.is_empty() {
        return;
    }
    let _ = app.emit(
        CYCLES_CHANGED,
        CycleIdsPayload {
            cycle_ids: ids.ids(),
        },
    );
}

pub fn emit_tasks_changed<R: Runtime>(app: &tauri::AppHandle<R>, ids: &CycleIdSet) {
    if ids.is_empty() {
        return;
    }
    let _ = app.emit(
        TASKS_CHANGED,
        CycleIdsPayload {
            cycle_ids: ids.ids(),
        },
    );
}

pub fn emit_proposals_changed<R: Runtime>(app: &tauri::AppHandle<R>, cycle_id: &str) {
    let _ = app.emit(
        PROPOSALS_CHANGED,
        CycleIdPayload {
            cycle_id: cycle_id.to_string(),
        },
    );
}

/// Agent conversation change notice (add-ai-planning-core task 9.3): the
/// payload identifies the conversation; business data stays in the database.
pub const AGENT_CONVERSATION_UPDATED: &str = "agent:conversation_updated";

#[derive(Debug, Clone, Serialize)]
pub struct AgentConversationPayload {
    pub conversation_id: String,
    pub revision: i64,
}

pub fn emit_agent_conversation_updated<R: Runtime>(
    app: &tauri::AppHandle<R>,
    conversation_id: &str,
    revision: i64,
) {
    let _ = app.emit(
        AGENT_CONVERSATION_UPDATED,
        AgentConversationPayload {
            conversation_id: conversation_id.to_string(),
            revision,
        },
    );
}

//! Agent proposal semantics: preview state, original-value snapshots, apply/revert.
//!
//! Behaviour contract: `openspec/specs/agent-proposals/spec.md`.
//! No AI is wired up in this change; the mechanism is exercised through the
//! proposal service so the contract is already enforceable.

use serde::{Deserialize, Serialize};

use super::task::{Subtask, Task};

/// Marks a row that is visible but not yet accepted by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProposalKind {
    Upsert,
    Delete,
}

impl ProposalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Upsert => "upsert",
            Self::Delete => "delete",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "upsert" => Some(Self::Upsert),
            "delete" => Some(Self::Delete),
            _ => None,
        }
    }
}

/// Pre-proposal state of one task row, stored in `task_preview_originals`.
///
/// `original_exists = false` records "the task did not exist before the agent
/// touched it" — reverting must delete the row. Otherwise every proposal-
/// controlled field is the value it had when it entered preview mode. User
/// notes are not proposal-controlled and remain on the task row. `None` for fields
/// that were SQL `NULL` (`Some(None)` in the snapshot, flattened to `None`
/// columns in storage).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskSnapshot {
    pub original_exists: bool,
    pub title: Option<String>,
    pub completed: Option<bool>,
    pub subtasks: Option<Vec<Subtask>>,
    pub position: Option<i64>,
    pub goal_breakdown: Option<serde_json::Value>,
    pub parent_id: Option<String>,
    pub root_color_key: Option<String>,
    pub created_at: Option<i64>,
    pub needs_refinement: Option<bool>,
    pub needs_breakdown: Option<bool>,
}

impl TaskSnapshot {
    /// Snapshot of a task that already existed.
    pub fn capture(task: &Task) -> Self {
        Self {
            original_exists: true,
            title: Some(task.title.clone()),
            completed: Some(task.completed),
            subtasks: Some(task.subtasks.clone()),
            position: Some(task.position),
            goal_breakdown: task.goal_breakdown.clone(),
            parent_id: task.parent_id.clone(),
            root_color_key: task.root_color_key.clone(),
            created_at: Some(task.created_at),
            needs_refinement: task.needs_refinement,
            needs_breakdown: task.needs_breakdown,
        }
    }

    /// Snapshot of "this row did not exist before".
    pub fn absent() -> Self {
        Self {
            original_exists: false,
            title: None,
            completed: None,
            subtasks: None,
            position: None,
            goal_breakdown: None,
            parent_id: None,
            root_color_key: None,
            created_at: None,
            needs_refinement: None,
            needs_breakdown: None,
        }
    }

    /// Whether `task` still equals what this snapshot describes. Used to tell
    /// "the user accepted the change as proposed" apart from "the content was
    /// edited again after the proposal" without revision numbers.
    pub fn matches(&self, task: &Task) -> bool {
        if !self.original_exists {
            return false;
        }
        self.title.as_deref() == Some(task.title.as_str())
            && self.completed == Some(task.completed)
            && self.subtasks.as_deref() == Some(task.subtasks.as_slice())
            && self.position == Some(task.position)
            && self.goal_breakdown == task.goal_breakdown
            && self.parent_id == task.parent_id
            && self.root_color_key == task.root_color_key
            && self.created_at == Some(task.created_at)
            && self.needs_refinement == task.needs_refinement
            && self.needs_breakdown == task.needs_breakdown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_task() -> Task {
        Task {
            id: "t1".into(),
            cycle_id: "c1".into(),
            parent_id: None,
            title: "original".into(),
            note: String::new(),
            subtasks: vec![Subtask::new("step", false)],
            position: 2,
            completed: false,
            goal_breakdown: None,
            needs_refinement: Some(true),
            needs_breakdown: Some(true),
            root_color_key: Some("teal".into()),
            copied_from_task_id: None,
            later_plan_type: None,
            proposal: None,
            created_at: 1234,
        }
    }

    #[test]
    fn capture_preserves_every_snapshotted_field() {
        let task = sample_task();
        let snap = TaskSnapshot::capture(&task);
        assert!(snap.original_exists);
        assert_eq!(snap.title.as_deref(), Some("original"));
        assert_eq!(snap.root_color_key.as_deref(), Some("teal"));
        assert_eq!(snap.needs_refinement, Some(true));
        assert_eq!(snap.position, Some(2));
        assert!(snap.matches(&task));
    }

    #[test]
    fn capture_keeps_none_fields_distinct_from_values() {
        let mut task = sample_task();
        task.goal_breakdown = None;
        task.needs_refinement = None;
        let snap = TaskSnapshot::capture(&task);
        assert_eq!(snap.goal_breakdown, None);
        assert_eq!(snap.needs_refinement, None);
        assert!(snap.matches(&task));
    }

    #[test]
    fn edited_task_no_longer_matches_snapshot() {
        let mut task = sample_task();
        let snap = TaskSnapshot::capture(&task);
        task.title = "renamed by agent".into();
        assert!(!snap.matches(&task));
        task.title = "original".into();
        task.completed = true;
        assert!(!snap.matches(&task));
    }

    #[test]
    fn absent_snapshot_never_matches() {
        assert!(!TaskSnapshot::absent().matches(&sample_task()));
    }

    #[test]
    fn snapshot_serializes_for_storage() {
        let snap = TaskSnapshot::capture(&sample_task());
        let json = serde_json::to_string(&snap).unwrap();
        let back: TaskSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back, snap);
        let absent = TaskSnapshot::absent();
        let json = serde_json::to_string(&absent).unwrap();
        assert!(json.contains("false"));
    }
}

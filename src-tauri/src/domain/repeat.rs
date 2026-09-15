//! Repeat templates: saved focus blocks that materialize into each new day.
//!
//! Behaviour contract: `openspec/specs/session-repeats/spec.md`.
//! Templates never share mutable state with their instances: editing a
//! template touches only the `repeats` row, and removing one only clears
//! `cycles.repeat_id` on past instances (never deletes them).

use serde::Serialize;

/// One row of `repeats`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Repeat {
    pub id: String,
    pub title: String,
    /// Milliseconds; copied into newly generated sessions.
    pub duration: i64,
    pub position: i64,
    /// Archived templates ("Stop repeating") stop generating instances.
    pub archived: bool,
}

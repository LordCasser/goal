//! AI planning capabilities: LLM abstraction, agent conversations and the
//! planning engines (change: `add-ai-planning-core`).
//!
//! Layering: `llm` is the provider-facing abstraction over the sampling
//! layer; `breakdown` is the deterministic GoalBreakdown engine (structure,
//! merging, clarity derivation); the remaining engines arrive with their
//! tasks and will live beside them.

pub mod agent;
pub mod breakdown;
pub mod llm;
pub mod prioritization;
pub mod review;
pub mod tools;

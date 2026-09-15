//! AI planning capabilities: LLM abstraction, agent conversations and the
//! planning engines (change: `add-ai-planning-core`).
//!
//! Layering: `llm` is the provider-facing abstraction over the sampling
//! layer; agent conversation, skills and engines arrive with later tasks and
//! will live beside it.

pub mod llm;

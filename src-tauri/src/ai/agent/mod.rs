//! Agent conversation and skills (add-ai-planning-core §2–§4).
//!
//! `prompt` owns the five instruction blocks; `context` builds the injected
//! `<context>` payload; `turn` is the bounded tool-calling loop that persists
//! messages in one ordered pass.

pub mod context;
pub mod prompt;
pub mod prompt_xml;
pub mod turn;

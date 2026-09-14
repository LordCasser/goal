//! Provider configuration layer: `providers.json` next to `planner.db`.
//!
//! Non-sensitive metadata only (BYOK, design: add-ai-access-and-voice D1/D4);
//! credentials live in the system keychain and never pass through here.

pub mod config;
pub mod credentials;

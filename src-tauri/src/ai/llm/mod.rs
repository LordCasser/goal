//! LLM abstraction (add-ai-planning-core §1): one provider-facing surface for
//! the agent loop, two implementations.
//!
//! [`LlmProvider`] boxes its futures so it stays usable as a trait object
//! behind Tauri managed state without pulling in an async-trait dependency.
//! [`ResolvedProvider`] is the deterministic BYOK resolution over
//! `providers.json` + keychain; [`RealProvider`] adapts it to the sampling
//! layer, and [`FakeProvider`] scripts deterministic turns for tests so the
//! four planning capabilities run fully offline.

mod fake;
mod real;
mod resolved;
mod tools;
pub mod types;

pub use fake::{FakeProvider, FakeTurn};
pub use real::RealProvider;
pub use resolved::{resolve, ResolvedProvider};
pub use tools::{to_spec, tool_definitions};
pub use types::{
    AgentMessage, AgentRequest, AgentResponse, AgentRole, AgentSkill, LlmRequest, ToolCallRecord,
    ToolDef, Usage,
};

use crate::sampling::SamplingError;

/// Boxed future so `LlmProvider` can be used as `dyn LlmProvider`.
pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

pub trait LlmProvider: Send + Sync {
    /// One conversational round: request in, text + tool calls out.
    fn generate_agent(&self, req: AgentRequest) -> BoxFuture<Result<AgentResponse, AgentError>>;
    /// Single structured prompt (clarity-style evaluation), JSON out.
    fn generate_json(&self, req: LlmRequest) -> BoxFuture<Result<serde_json::Value, AgentError>>;
}

/// Errors the agent loop understands; `code()` is the IPC-visible token
/// (design: add-ai-planning-core, 错误映射).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentError {
    /// The resolved provider failed; the sampling error carries the
    /// classification (auth_failed / rate_limited / protocol_error / …).
    Provider(SamplingError),
    /// No active provider is configured — recoverable by configuring one.
    NoActiveProvider,
    /// The active provider declares `supports_tools: false` but the skill
    /// requires tools; the loop degrades to conversation mode instead.
    UnsupportedTools,
    Internal(String),
}

impl AgentError {
    pub fn code(&self) -> String {
        match self {
            AgentError::Provider(e) => e.code().to_string(),
            AgentError::NoActiveProvider => "no_active_provider".to_string(),
            AgentError::UnsupportedTools => "unsupported_tools".to_string(),
            AgentError::Internal(_) => "internal".to_string(),
        }
    }
}

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentError::Provider(e) => write!(f, "provider error: {e}"),
            AgentError::NoActiveProvider => write!(f, "no active AI provider is configured"),
            AgentError::UnsupportedTools => {
                write!(f, "the active provider does not support tool calls")
            }
            AgentError::Internal(message) => write!(f, "{message}"),
        }
    }
}

impl From<SamplingError> for AgentError {
    fn from(e: SamplingError) -> Self {
        AgentError::Provider(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_error_codes_map_to_ipc_tokens() {
        assert_eq!(AgentError::NoActiveProvider.code(), "no_active_provider");
        assert_eq!(AgentError::UnsupportedTools.code(), "unsupported_tools");
        assert_eq!(AgentError::Internal("x".into()).code(), "internal");
        let unreachable = AgentError::Provider(SamplingError::ProviderUnreachable {
            message: "refused".into(),
        });
        assert_eq!(unreachable.code(), "provider_unreachable");
    }
}

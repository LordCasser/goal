//! Scripted provider for offline tests (task §1.4).
//!
//! Not behind `#[cfg(test)]` or a feature flag on purpose: the conversation
//! and engine tests of later tasks (and any integration test) need it too,
//! and a `cfg(test)` gate would hide it from them. It is test doubles all
//! the way down — production wiring only ever sees [`super::RealProvider`].

use std::collections::VecDeque;
use std::sync::Mutex;

use super::types::{AgentRequest, AgentResponse, LlmRequest, ToolCallRecord};
use super::{AgentError, BoxFuture, LlmProvider};

/// One pre-scripted agent turn.
#[derive(Debug, Clone, PartialEq)]
pub enum FakeTurn {
    /// The model answers with plain text.
    Text(String),
    /// The model issues tool calls (and no text).
    Calls(Vec<ToolCallRecord>),
    /// The provider call fails with this error.
    Fail(AgentError),
}

/// Replays a fixed script: each [`FakeProvider::generate_agent`] call pops
/// the next [`FakeTurn`]; `generate_json` returns a preset value. Running
/// off the end of the script (or asking for JSON without a preset) is a
/// typed error, so an under-provisioned test fails loudly instead of
/// hanging or silently returning empty turns.
#[derive(Default)]
pub struct FakeProvider {
    script: Mutex<VecDeque<FakeTurn>>,
    json: Option<serde_json::Value>,
}

impl FakeProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// A provider that answers agent turns from `script`, oldest first.
    pub fn with_script(script: Vec<FakeTurn>) -> Self {
        Self {
            script: Mutex::new(script.into()),
            json: None,
        }
    }

    /// A provider whose `generate_json` returns `value`.
    pub fn with_json(value: serde_json::Value) -> Self {
        Self {
            script: Mutex::new(VecDeque::new()),
            json: Some(value),
        }
    }

    fn next_turn(&self) -> Option<FakeTurn> {
        self.script
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front()
    }
}

impl LlmProvider for FakeProvider {
    fn generate_agent<'a>(
        &'a self,
        _req: AgentRequest,
    ) -> BoxFuture<'a, Result<AgentResponse, AgentError>> {
        Box::pin(async move {
            match self.next_turn() {
                Some(FakeTurn::Text(text)) => Ok(AgentResponse {
                    text,
                    tool_calls: Vec::new(),
                    usage: None,
                }),
                Some(FakeTurn::Calls(calls)) => Ok(AgentResponse {
                    text: String::new(),
                    tool_calls: calls,
                    usage: None,
                }),
                Some(FakeTurn::Fail(error)) => Err(error),
                None => Err(AgentError::Internal(
                    "fake provider script exhausted".into(),
                )),
            }
        })
    }

    fn generate_json<'a>(
        &'a self,
        _req: LlmRequest,
    ) -> BoxFuture<'a, Result<serde_json::Value, AgentError>> {
        Box::pin(async move {
            self.json
                .clone()
                .ok_or_else(|| AgentError::Internal("fake provider has no preset JSON".into()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::llm::types::{AgentMessage, AgentRole, AgentSkill, ToolDef};

    fn request() -> AgentRequest {
        AgentRequest {
            skill: AgentSkill::None,
            system: String::new(),
            context_block: String::new(),
            history: vec![AgentMessage {
                role: AgentRole::User,
                content: "hi".into(),
                tool_calls: vec![],
                tool_call_id: None,
            }],
            user_message: "hi".into(),
            tools: vec![ToolDef::new("t", "d", serde_json::json!({}))],
            max_tokens: None,
        }
    }

    fn call(id: &str) -> ToolCallRecord {
        ToolCallRecord {
            id: id.into(),
            name: "start_planning".into(),
            arguments: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn script_plays_in_order_through_the_trait_object() {
        let provider = FakeProvider::with_script(vec![
            FakeTurn::Text("hello".into()),
            FakeTurn::Calls(vec![call("call_1"), call("call_2")]),
            FakeTurn::Fail(AgentError::NoActiveProvider),
        ]);
        // Called through `&dyn` to prove the trait is object-safe as used.
        let provider: &dyn LlmProvider = &provider;

        let first = provider.generate_agent(request()).await.unwrap();
        assert_eq!(first.text, "hello");
        assert!(first.tool_calls.is_empty());
        assert_eq!(first.usage, None);

        let second = provider.generate_agent(request()).await.unwrap();
        assert_eq!(second.text, "");
        assert_eq!(second.tool_calls.len(), 2);
        assert_eq!(second.tool_calls[0].id, "call_1");
        assert_eq!(second.tool_calls[1].id, "call_2");

        let third = provider.generate_agent(request()).await.unwrap_err();
        assert_eq!(third, AgentError::NoActiveProvider);
        assert_eq!(third.code(), "no_active_provider");

        // Off the end: loud failure, not a silent empty turn.
        let fourth = provider.generate_agent(request()).await.unwrap_err();
        assert!(matches!(fourth, AgentError::Internal(_)));
    }

    #[tokio::test]
    async fn json_returns_the_preset_value_every_time() {
        let provider = FakeProvider::with_json(serde_json::json!({ "clarity": 3 }));
        let prompt = LlmRequest {
            system: String::new(),
            prompt: "rate".into(),
            max_tokens: None,
        };
        assert_eq!(
            provider.generate_json(prompt.clone()).await.unwrap(),
            serde_json::json!({ "clarity": 3 })
        );
        assert_eq!(
            provider.generate_json(prompt).await.unwrap(),
            serde_json::json!({ "clarity": 3 }),
            "the preset is not consumed"
        );
    }

    #[tokio::test]
    async fn default_provider_fails_loudly() {
        let provider = FakeProvider::default();
        assert!(matches!(
            provider.generate_agent(request()).await,
            Err(AgentError::Internal(_))
        ));
        assert!(matches!(
            provider
                .generate_json(LlmRequest {
                    system: String::new(),
                    prompt: String::new(),
                    max_tokens: None,
                })
                .await,
            Err(AgentError::Internal(_))
        ));
    }
}

//! The real provider: adapts the agent shapes onto the sampling layer
//! (task §1.3).
//!
//! This module owns every format decision the sampling layer cannot express:
//! [`crate::sampling::SamplingMessage`] knows only user/assistant text, so
//! the system instruction, the `<context>` block and past tool-call pairs are
//! serialized into ordinary turns here — that is the adapter's job, the
//! sampling layer stays untouched. All mapping logic lives in pure functions
//! so it is testable without a network; only the thin stream loop in the
//! trait implementation touches HTTP.

use futures_util::StreamExt;

use crate::providers::service::sampling_api_format;
use crate::sampling::{
    self, MessageRole, SamplingEvent, SamplingMessage, SamplingRequest, Timeouts, ToolSpec,
};

use super::resolved::ResolvedProvider;
use super::tools;
use super::types::{AgentMessage, AgentRequest, AgentRole, LlmRequest, ToolCallRecord, Usage};
use super::{AgentError, AgentResponse, BoxFuture, LlmProvider};

/// LLM provider backed by the resolved BYOK configuration.
pub struct RealProvider {
    pub resolved: ResolvedProvider,
}

impl RealProvider {
    pub fn new(resolved: ResolvedProvider) -> Self {
        Self { resolved }
    }

    /// Shared request scaffold: endpoint, protocol, model and credential come
    /// from the resolved provider; `max_tokens` follows the documented
    /// resolution order — explicit request value > model configuration >
    /// adapter default (only the anthropic adapter has one).
    fn base_request(
        &self,
        messages: Vec<SamplingMessage>,
        tool_specs: Vec<ToolSpec>,
        max_tokens: Option<u64>,
    ) -> SamplingRequest {
        SamplingRequest {
            base_url: self.resolved.config.base_url.clone(),
            api_format: sampling_api_format(self.resolved.config.api_format),
            model: self.resolved.model.model_id.clone(),
            api_key: self.resolved.api_key.clone(),
            messages,
            tools: tool_specs,
            max_tokens: max_tokens.or(Some(self.resolved.model.max_output_tokens)),
        }
    }
}

impl LlmProvider for RealProvider {
    fn generate_agent<'a>(
        &'a self,
        req: AgentRequest,
    ) -> BoxFuture<'a, Result<AgentResponse, AgentError>> {
        Box::pin(async move {
            let request = build_agent_request(&self.resolved, req)?;
            let response = run_stream(request).await?;
            Ok(response)
        })
    }

    fn generate_json<'a>(
        &'a self,
        req: LlmRequest,
    ) -> BoxFuture<'a, Result<serde_json::Value, AgentError>> {
        Box::pin(async move {
            let request = self.base_request(json_messages(&req), Vec::new(), req.max_tokens);
            let response = run_stream(request).await?;
            parse_json_output(&response.text)
        })
    }
}

/// Sends one request and folds its event stream into an [`AgentResponse`].
async fn run_stream(request: SamplingRequest) -> Result<AgentResponse, AgentError> {
    let mut stream = sampling::sample(request, Timeouts::default())
        .await
        .map_err(AgentError::from)?;
    let mut aggregator = ResponseAggregator::default();
    while let Some(item) = stream.next().await {
        aggregator.push(item.map_err(AgentError::from)?);
    }
    aggregator.finish()
}

/// Builds the sampling request for one agent turn.
///
/// Message layout (the sampling layer has no system role):
/// 1. a leading user message carrying the system instruction and the context
///    block;
/// 2. the replayed history in order — assistant tool calls are appended to
///    the assistant text as a `<tool_calls>` JSON block, tool results become
///    user messages wrapped in `<tool_result>`;
/// 3. the user's message.
fn build_agent_request(
    resolved: &ResolvedProvider,
    req: AgentRequest,
) -> Result<SamplingRequest, AgentError> {
    if !req.tools.is_empty() && !resolved.tools_supported {
        // The agent layer normally degrades before asking; this guard keeps
        // the provider honest if it does not.
        return Err(AgentError::UnsupportedTools);
    }
    let tool_specs: Vec<ToolSpec> = req.tools.iter().map(tools::to_spec).collect();
    let mut messages = vec![bootstrap_message(&req.system, &req.context_block)];
    messages.extend(render_history(&req.history));
    messages.push(SamplingMessage {
        role: MessageRole::User,
        content: req.user_message,
    });
    Ok(SamplingRequest {
        base_url: resolved.config.base_url.clone(),
        api_format: sampling_api_format(resolved.config.api_format),
        model: resolved.model.model_id.clone(),
        api_key: resolved.api_key.clone(),
        messages,
        tools: tool_specs,
        max_tokens: req.max_tokens.or(Some(resolved.model.max_output_tokens)),
    })
}

/// The leading message: system instruction and context block joined by a
/// blank line; either side may be empty.
fn bootstrap_message(system: &str, context_block: &str) -> SamplingMessage {
    let mut content = String::new();
    if !system.is_empty() {
        content.push_str(system);
    }
    if !context_block.is_empty() {
        if !content.is_empty() {
            content.push_str("\n\n");
        }
        content.push_str(context_block);
    }
    SamplingMessage {
        role: MessageRole::User,
        content,
    }
}

/// Replays stored history as user/assistant text turns (see
/// [`build_agent_request`] for the conventions).
fn render_history(history: &[AgentMessage]) -> Vec<SamplingMessage> {
    history
        .iter()
        .map(|message| match message.role {
            AgentRole::User => SamplingMessage {
                role: MessageRole::User,
                content: message.content.clone(),
            },
            AgentRole::Assistant => {
                let mut content = message.content.clone();
                if !message.tool_calls.is_empty() {
                    if !content.is_empty() {
                        content.push('\n');
                    }
                    content.push_str(&render_tool_calls(&message.tool_calls));
                }
                SamplingMessage {
                    role: MessageRole::Assistant,
                    content,
                }
            }
            AgentRole::Tool => {
                let content = match &message.tool_call_id {
                    Some(id) => format!(
                        "<tool_result tool_call_id=\"{id}\">\n{}\n</tool_result>",
                        message.content
                    ),
                    None => format!("<tool_result>\n{}\n</tool_result>", message.content),
                };
                SamplingMessage {
                    role: MessageRole::User,
                    content,
                }
            }
        })
        .collect()
}

/// Serializes assistant tool calls as one JSON-per-line block the model can
/// recognize as its own past calls.
fn render_tool_calls(calls: &[ToolCallRecord]) -> String {
    let mut out = String::from("<tool_calls>\n");
    for call in calls {
        let line = serde_json::json!({
            "id": call.id,
            "name": call.name,
            "arguments": call.arguments,
        });
        out.push_str(&line.to_string());
        out.push('\n');
    }
    out.push_str("</tool_calls>");
    out
}

/// Message layout for structured output: one user message, no tools.
fn json_messages(req: &LlmRequest) -> Vec<SamplingMessage> {
    let mut content = String::new();
    if !req.system.is_empty() {
        content.push_str(&req.system);
        content.push_str("\n\n");
    }
    content.push_str(&req.prompt);
    vec![SamplingMessage {
        role: MessageRole::User,
        content,
    }]
}

/// Parses a model's answer as one JSON value, tolerating the common
/// markdown-fence wrapper. Anything else is a typed failure — a clarity
/// score built from prose would be worse than an error.
fn parse_json_output(text: &str) -> Result<serde_json::Value, AgentError> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
        return Ok(value);
    }
    if let Some(stripped) = trimmed.strip_prefix("```") {
        let body = stripped.strip_suffix("```").unwrap_or(stripped);
        let body = body.strip_prefix("json").unwrap_or(body).trim();
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
            return Ok(value);
        }
    }
    Err(AgentError::Internal(format!(
        "model did not return valid JSON: {}",
        snippet(trimmed)
    )))
}

/// Short, boundary-safe preview of the offending output for the error line.
fn snippet(text: &str) -> String {
    const LIMIT: usize = 100;
    if text.chars().count() <= LIMIT {
        text.to_string()
    } else {
        let cut: String = text.chars().take(LIMIT).collect();
        format!("{cut}…")
    }
}

/// Folds sampling events into one response. Tool arguments must parse as
/// JSON objects; the sampling layer guarantees this, a violation is kept as
/// an error instead of being silently dropped.
#[derive(Default)]
struct ResponseAggregator {
    text: String,
    tool_calls: Vec<ToolCallRecord>,
    usage: Option<Usage>,
    error: Option<AgentError>,
}

impl ResponseAggregator {
    fn push(&mut self, event: SamplingEvent) {
        match event {
            SamplingEvent::TextDelta { text } => self.text.push_str(&text),
            SamplingEvent::ToolCall {
                id,
                name,
                arguments,
            } => match serde_json::from_str::<serde_json::Value>(&arguments) {
                Ok(arguments) => self.tool_calls.push(ToolCallRecord {
                    id,
                    name,
                    arguments,
                }),
                Err(e) => {
                    if self.error.is_none() {
                        self.error = Some(AgentError::Internal(format!(
                            "tool '{name}' returned malformed JSON arguments: {e}"
                        )));
                    }
                }
            },
            SamplingEvent::Usage {
                input_tokens,
                output_tokens,
            } => self.usage = Some(Usage {
                input_tokens,
                output_tokens,
            }),
            SamplingEvent::Finished { .. } => {}
        }
    }

    fn finish(self) -> Result<AgentResponse, AgentError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        Ok(AgentResponse {
            text: self.text,
            tool_calls: self.tool_calls,
            usage: self.usage,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(tools_supported: bool) -> ResolvedProvider {
        let model = crate::providers::config::ModelConfig {
            model_id: "m1".into(),
            context_window: 8192,
            max_output_tokens: 4096,
            input_types: vec![crate::providers::config::InputType::Text],
            output_types: vec![crate::providers::config::OutputType::Text],
            supports_tools: tools_supported,
        };
        ResolvedProvider {
            config: crate::providers::config::ProviderConfig {
                id: "p1".into(),
                name: "Test".into(),
                base_url: "http://127.0.0.1:9/v1".into(),
                api_format: crate::providers::config::ApiFormat::OpenaiChatCompletions,
                extra_headers: vec![],
                models: vec![model.clone()],
                created_at: 0,
                archived: false,
            },
            model,
            api_key: None,
            tools_supported,
        }
    }

    fn call(id: &str, name: &str, arguments: serde_json::Value) -> ToolCallRecord {
        ToolCallRecord {
            id: id.into(),
            name: name.into(),
            arguments,
        }
    }

    fn request(tools: Vec<super::super::types::ToolDef>) -> AgentRequest {
        AgentRequest {
            skill: super::super::types::AgentSkill::LongTermPlanning,
            system: "You plan.".into(),
            context_block: "<context>\n  <cycle />\n</context>".into(),
            history: vec![
                super::super::types::AgentMessage {
                    role: AgentRole::User,
                    content: "help me plan".into(),
                    tool_calls: vec![],
                    tool_call_id: None,
                },
                super::super::types::AgentMessage {
                    role: AgentRole::Assistant,
                    content: "I'll start planning.".into(),
                    tool_calls: vec![call("call_1", "start_planning", serde_json::json!({}))],
                    tool_call_id: None,
                },
                super::super::types::AgentMessage {
                    role: AgentRole::Tool,
                    content: "{\"activated_skill\":\"long_term_planning\"}".into(),
                    tool_calls: vec![],
                    tool_call_id: Some("call_1".into()),
                },
            ],
            user_message: "go".into(),
            tools,
            max_tokens: None,
        }
    }

    #[test]
    fn agent_request_maps_onto_the_sampling_request() {
        let request = request(vec![super::super::types::ToolDef::new(
            "get_cycle_context",
            "read",
            serde_json::json!({ "type": "object" }),
        )]);
        let sampling = build_agent_request(&resolved(true), request).unwrap();

        assert_eq!(sampling.base_url, "http://127.0.0.1:9/v1");
        assert_eq!(sampling.api_format.label(), "openai_chat_completions");
        assert_eq!(sampling.model, "m1");
        assert_eq!(sampling.api_key, None);
        assert_eq!(sampling.max_tokens, Some(4096), "model config fills in");
        assert_eq!(sampling.tools.len(), 1);
        assert_eq!(sampling.tools[0].name, "get_cycle_context");
        assert_eq!(
            sampling.tools[0].input_schema,
            serde_json::json!({ "type": "object" })
        );

        // bootstrap + 3 history + user message
        assert_eq!(sampling.messages.len(), 5);
        let bootstrap = &sampling.messages[0];
        assert_eq!(bootstrap.role, MessageRole::User);
        assert!(bootstrap.content.starts_with("You plan."));
        assert!(bootstrap.content.contains("<context>"));
        assert!(bootstrap.content.contains("</context>"));

        assert_eq!(sampling.messages[1].role, MessageRole::User);
        assert_eq!(sampling.messages[1].content, "help me plan");

        let assistant = &sampling.messages[2];
        assert_eq!(assistant.role, MessageRole::Assistant);
        assert!(assistant.content.contains("I'll start planning."));
        assert!(assistant.content.contains("<tool_calls>"));
        assert!(assistant.content.contains("\"name\":\"start_planning\""));
        assert!(assistant.content.contains("</tool_calls>"));

        let tool_result = &sampling.messages[3];
        assert_eq!(tool_result.role, MessageRole::User);
        assert!(tool_result
            .content
            .contains("<tool_result tool_call_id=\"call_1\">"));
        assert!(tool_result.content.contains("activated_skill"));

        let last = &sampling.messages[4];
        assert_eq!(last.role, MessageRole::User);
        assert_eq!(last.content, "go");
    }

    #[test]
    fn explicit_max_tokens_win_over_model_configuration() {
        let mut request = request(vec![]);
        request.max_tokens = Some(64);
        let sampling = build_agent_request(&resolved(true), request).unwrap();
        assert_eq!(sampling.max_tokens, Some(64));
    }

    #[test]
    fn tools_are_rejected_when_the_model_cannot_use_them() {
        let request = request(vec![super::super::types::ToolDef::new(
            "t",
            "d",
            serde_json::json!({}),
        )]);
        let error = build_agent_request(&resolved(false), request).unwrap_err();
        assert_eq!(error, AgentError::UnsupportedTools);
        assert_eq!(error.code(), "unsupported_tools");
    }

    #[test]
    fn tool_less_requests_pass_through_without_the_flag() {
        let sampling = build_agent_request(&resolved(false), request(vec![])).unwrap();
        assert!(sampling.tools.is_empty());
        assert_eq!(sampling.messages.len(), 5);
    }

    #[test]
    fn aggregator_collects_text_tool_calls_and_usage() {
        let mut aggregator = ResponseAggregator::default();
        aggregator.push(SamplingEvent::TextDelta { text: "he".into() });
        aggregator.push(SamplingEvent::TextDelta { text: "llo".into() });
        aggregator.push(SamplingEvent::ToolCall {
            id: "call_9".into(),
            name: "create_goal".into(),
            arguments: "{\"title\":\"x\"}".into(),
        });
        aggregator.push(SamplingEvent::Usage {
            input_tokens: Some(12),
            output_tokens: Some(34),
        });
        aggregator.push(SamplingEvent::Finished {
            reason: crate::sampling::StopReason::ToolUse,
        });

        let response = aggregator.finish().unwrap();
        assert_eq!(response.text, "hello");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "call_9");
        assert_eq!(response.tool_calls[0].arguments, serde_json::json!({ "title": "x" }));
        assert_eq!(
            response.usage,
            Some(Usage {
                input_tokens: Some(12),
                output_tokens: Some(34),
            })
        );
    }

    #[test]
    fn aggregator_rejects_malformed_tool_arguments() {
        let mut aggregator = ResponseAggregator::default();
        aggregator.push(SamplingEvent::ToolCall {
            id: "call_1".into(),
            name: "broken".into(),
            arguments: "not json".into(),
        });
        let error = aggregator.finish().unwrap_err();
        assert!(matches!(error, AgentError::Internal(_)));
        assert!(matches!(
            &error,
            AgentError::Internal(message) if message.contains("broken")
        ));
    }

    #[test]
    fn json_output_tolerates_plain_and_fenced_answers() {
        assert_eq!(
            parse_json_output("{\"clarity\": 3}").unwrap(),
            serde_json::json!({ "clarity": 3 })
        );
        assert_eq!(
            parse_json_output("```json\n{\"a\": 1}\n```").unwrap(),
            serde_json::json!({ "a": 1 })
        );
        assert_eq!(
            parse_json_output("```\n[1, 2]\n```").unwrap(),
            serde_json::json!([1, 2])
        );
        assert_eq!(parse_json_output("  \n 42 \n ").unwrap(), serde_json::json!(42));

        let error = parse_json_output("I think clarity is fairly good.").unwrap_err();
        assert!(matches!(error, AgentError::Internal(_)));
        let long = format!("{} {}", "x".repeat(200), "still not json");
        let AgentError::Internal(message) = parse_json_output(&long).unwrap_err() else {
            panic!("expected internal error");
        };
        assert!(
            message.chars().count() < long.chars().count(),
            "the snippet is truncated: {message}"
        );
    }

    #[test]
    fn json_request_is_a_single_user_message_without_tools() {
        let sampling = json_messages(&LlmRequest {
            system: "Rate clarity.".into(),
            prompt: "Goal: be fit".into(),
            max_tokens: None,
        });
        assert_eq!(sampling.len(), 1);
        assert_eq!(sampling[0].role, MessageRole::User);
        assert!(sampling[0].content.starts_with("Rate clarity."));
        assert!(sampling[0].content.ends_with("Goal: be fit"));
    }
}

//! `openai_chat_completions` adapter (task §3.3).
//!
//! `POST {base}/chat/completions` with `Authorization: Bearer` (design D2) —
//! the primary format of local runtimes (Ollama / LM Studio / llama.cpp /
//! vLLM). Tool calls arrive as `delta.tool_calls` fragments identified by
//! `index`; id, name and argument shards are reassembled and delivered once
//! the stream terminates (task §3.1).

use std::collections::{BTreeMap, VecDeque};

use serde_json::{json, Value};

use super::client::{
    emit_finished, endpoint, sanitize_message_with_secrets, PreparedRequest, ProtocolDecoder,
    ToolCallBuffer,
};
use super::sse::{SseEvent, SseParser};
use super::types::{SamplingError, SamplingEvent, SamplingRequest, StopReason};

/// Sentinel terminating the stream (design D2 SSE shape).
const DONE: &str = "[DONE]";

/// Builds the wire request and decoder for the OpenAI Chat Completions
/// protocol.
pub(crate) fn prepare(request: &SamplingRequest) -> Result<PreparedRequest, SamplingError> {
    let mut headers = Vec::new();
    if let Some(key) = request.api_key.as_deref().filter(|key| !key.is_empty()) {
        headers.push(("authorization", format!("Bearer {key}")));
    }

    // Sampling preferences stay unset (task §3.7); this format gets no
    // `max_tokens` default — the field is sent only when explicitly given.
    let mut body = json!({
        "model": request.model,
        "messages": request
            .messages
            .iter()
            .map(|message| json!({ "role": message.role.label(), "content": message.content }))
            .collect::<Vec<_>>(),
        "stream": true,
        // Ask the provider to attach token usage to the final chunk.
        "stream_options": { "include_usage": true },
    });
    if !request.tools.is_empty() {
        body["tools"] = json!(request
            .tools
            .iter()
            .map(|tool| json!({
                "type": "function",
                "function": {
                    "name": tool.name,
                    "description": tool.description,
                    "parameters": tool.input_schema,
                },
            }))
            .collect::<Vec<_>>());
    }
    if let Some(max_tokens) = request.max_tokens {
        body["max_tokens"] = json!(max_tokens);
    }

    Ok(PreparedRequest {
        url: endpoint(&request.base_url, "/chat/completions"),
        headers,
        body,
        decoder: Box::new(Decoder {
            parser: SseParser::new(),
            secrets: request.redaction_secrets(),
            tools: BTreeMap::new(),
            finish_reason: None,
            usage_input: None,
            usage_output: None,
            terminated: false,
        }),
    })
}

/// Decodes the Chat Completions SSE stream into sampling events.
struct Decoder {
    parser: SseParser,
    /// Kept only to scrub server-echoed secrets out of error messages.
    secrets: Vec<String>,
    /// Tool calls by `delta.tool_calls[].index`, buffered until `[DONE]`.
    tools: BTreeMap<u64, ToolCallBuffer>,
    finish_reason: Option<StopReason>,
    usage_input: Option<u64>,
    usage_output: Option<u64>,
    terminated: bool,
}

type Out = VecDeque<Result<SamplingEvent, SamplingError>>;

impl Decoder {
    /// Flags the stream failed and queues the classified error; later chunks
    /// are ignored, and `finish` stays silent because `terminated` is set.
    fn fail(&mut self, out: &mut Out, message: impl Into<String>) {
        self.terminated = true;
        out.push_back(Err(SamplingError::ProtocolError {
            message: message.into(),
        }));
    }

    fn handle(&mut self, event: SseEvent, out: &mut Out) {
        if self.terminated {
            return;
        }
        if event.data.trim() == DONE {
            self.finish_sequence(out);
            return;
        }
        let Ok(value) = serde_json::from_str::<Value>(&event.data) else {
            self.fail(out, "SSE data line is not valid JSON");
            return;
        };
        if let Some(message) = value.pointer("/error/message").and_then(Value::as_str) {
            self.fail(out, sanitize_message_with_secrets(message, &self.secrets));
            return;
        }
        self.capture_usage(&value);
        // Providers stream a single choice; usage-only chunks carry none.
        let Some(choice) = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|c| c.first())
        else {
            return;
        };
        if let Some(text) = choice.pointer("/delta/content").and_then(Value::as_str) {
            out.push_back(Ok(SamplingEvent::TextDelta {
                text: text.to_string(),
            }));
        }
        self.capture_tool_fragments(choice, out);
        if let Some(reason) = choice.get("finish_reason").and_then(Value::as_str) {
            self.finish_reason = Some(map_finish_reason(reason));
        }
    }

    /// Records usage from the final chunk; `null` values never overwrite
    /// what an earlier chunk already reported.
    fn capture_usage(&mut self, value: &Value) {
        if let Some(usage) = value.get("usage").and_then(Value::as_object) {
            if let Some(tokens) = usage.get("prompt_tokens").and_then(Value::as_u64) {
                self.usage_input = Some(tokens);
            }
            if let Some(tokens) = usage.get("completion_tokens").and_then(Value::as_u64) {
                self.usage_output = Some(tokens);
            }
        }
    }

    /// Reassembles `delta.tool_calls` fragments: the first shard carries id
    /// and name, later shards only argument deltas (task §3.1: no tool call
    /// surfaces half-assembled).
    fn capture_tool_fragments(&mut self, choice: &Value, out: &mut Out) {
        let Some(calls) = choice
            .pointer("/delta/tool_calls")
            .and_then(Value::as_array)
        else {
            return;
        };
        for call in calls {
            let Some(index) = call.get("index").and_then(Value::as_u64) else {
                self.fail(out, "tool_calls fragment without index");
                return;
            };
            let entry = self.tools.entry(index).or_insert(ToolCallBuffer {
                id: String::new(),
                name: String::new(),
                arguments: String::new(),
            });
            if let Some(id) = call
                .get("id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                entry.id = id.to_string();
            }
            if let Some(name) = call
                .pointer("/function/name")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                entry.name = name.to_string();
            }
            if let Some(fragment) = call.pointer("/function/arguments").and_then(Value::as_str) {
                entry.arguments.push_str(fragment);
            }
        }
    }

    /// Emits the buffered tail: usage, tool calls, termination.
    fn finish_sequence(&mut self, out: &mut Out) {
        self.terminated = true;
        let reason = self.finish_reason.unwrap_or(if self.tools.is_empty() {
            StopReason::Other
        } else {
            StopReason::ToolUse
        });
        emit_finished(
            self.usage_input,
            self.usage_output,
            std::mem::take(&mut self.tools).into_values(),
            reason,
            out,
        );
    }
}

impl ProtocolDecoder for Decoder {
    fn feed(&mut self, chunk: &[u8], out: &mut Out) {
        for event in self.parser.feed(chunk) {
            self.handle(event, out);
        }
    }

    fn finish(&mut self, out: &mut Out) {
        for event in self.parser.finish() {
            self.handle(event, out);
        }
        if !self.terminated {
            // Some gateways close without the `[DONE]` sentinel; a seen
            // `finish_reason` is enough to terminate cleanly.
            if self.finish_reason.is_some() {
                self.finish_sequence(out);
            } else {
                out.push_back(Err(SamplingError::ProtocolError {
                    message: "stream ended before [DONE] or finish_reason".into(),
                }));
            }
        }
    }
}

fn map_finish_reason(raw: &str) -> StopReason {
    match raw {
        "stop" => StopReason::Stop,
        "tool_calls" | "function_call" => StopReason::ToolUse,
        "length" => StopReason::MaxTokens,
        _ => StopReason::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sampling::client::test_support::drive_decoder;
    use crate::sampling::types::{ApiFormat, SamplingMessage};

    #[test]
    fn stream_without_done_or_finish_reason_is_a_protocol_error() {
        let events = drive_decoder(prepare(&request()).unwrap().decoder, "data: {}\n\n");
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            Err(SamplingError::ProtocolError { .. })
        ));
    }

    #[test]
    fn missing_done_sentinel_tolerates_seen_finish_reason() {
        let payload = concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"hi\"},\"finish_reason\":\"stop\"}]}\n\n",
        );
        let events = drive_decoder(prepare(&request()).unwrap().decoder, payload);
        assert_eq!(
            events,
            vec![
                Ok(SamplingEvent::TextDelta { text: "hi".into() }),
                Ok(SamplingEvent::Finished {
                    reason: StopReason::Stop
                }),
            ]
        );
    }

    #[test]
    fn tool_fragments_without_index_are_a_protocol_error() {
        let payload = concat!(
            "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"function\":{\"arguments\":\"{}\"}}]}}]}\n\n",
        );
        let events = drive_decoder(prepare(&request()).unwrap().decoder, payload);
        assert!(events
            .iter()
            .any(|event| matches!(event, Err(SamplingError::ProtocolError { .. }))));
    }

    #[test]
    fn body_omits_max_tokens_when_unset() {
        let mut req = request();
        req.messages = vec![SamplingMessage {
            role: crate::sampling::types::MessageRole::User,
            content: "hi".into(),
        }];
        let prepared = prepare(&req).unwrap();
        assert!(prepared.body.get("max_tokens").is_none());
        assert!(prepared.body.get("temperature").is_none());
        assert_eq!(
            prepared.body["stream_options"]["include_usage"],
            json!(true)
        );
        assert_eq!(prepared.url, "https://provider.test/v1/chat/completions");

        req.max_tokens = Some(512);
        assert_eq!(prepare(&req).unwrap().body["max_tokens"], json!(512));
    }

    fn request() -> SamplingRequest {
        SamplingRequest {
            connection: Default::default(),
            base_url: "https://provider.test/v1".into(),
            api_format: ApiFormat::OpenaiChatCompletions,
            model: "gpt-test".into(),
            api_key: Some("secret-key".into()),
            extra_headers: vec![],
            messages: vec![],
            tools: vec![],
            max_tokens: None,
        }
    }
}

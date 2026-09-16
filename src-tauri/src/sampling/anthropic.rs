//! `anthropic_messages` adapter (task §3.2).
//!
//! `POST {base}/messages` with `x-api-key` and a fixed `anthropic-version`
//! header (design D2). The SSE blocks carry an `event:` line naming each
//! event; tool arguments arrive as `input_json_delta` fragments and are
//! buffered until `message_stop` (task §3.1).

use std::collections::{BTreeMap, VecDeque};

use serde_json::{json, Value};

use super::client::{
    emit_finished, endpoint, sanitize_message_with_secrets, PreparedRequest, ProtocolDecoder,
    ToolCallBuffer,
};
use super::sse::{SseEvent, SseParser};
use super::types::{SamplingError, SamplingEvent, SamplingRequest, StopReason};

/// Anthropic requires this version header on every request (design D2).
const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Anthropic makes `max_tokens` mandatory; when neither the request nor the
/// model configuration supplied one the adapter fills this default (task
/// §3.7 resolution order).
const DEFAULT_MAX_TOKENS: u64 = 4096;

/// Builds the wire request and decoder for the Anthropic Messages protocol.
pub(crate) fn prepare(request: &SamplingRequest) -> Result<PreparedRequest, SamplingError> {
    let mut headers = Vec::new();
    if let Some(key) = request.api_key.as_deref().filter(|key| !key.is_empty()) {
        headers.push(("x-api-key", key.to_string()));
    }
    headers.push(("anthropic-version", ANTHROPIC_VERSION.to_string()));

    // Sampling preferences stay unset (task §3.7): no temperature/top_p, so
    // upstream defaults apply; only `max_tokens` reaches the wire.
    let mut body = json!({
        "model": request.model,
        "max_tokens": request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        "messages": request
            .messages
            .iter()
            .map(|message| json!({ "role": message.role.label(), "content": message.content }))
            .collect::<Vec<_>>(),
        "stream": true,
    });
    if !request.tools.is_empty() {
        body["tools"] = json!(request
            .tools
            .iter()
            .map(|tool| json!({
                "name": tool.name,
                "description": tool.description,
                "input_schema": tool.input_schema,
            }))
            .collect::<Vec<_>>());
    }

    Ok(PreparedRequest {
        url: endpoint(&request.base_url, "/messages"),
        headers,
        body,
        decoder: Box::new(Decoder {
            parser: SseParser::new(),
            secrets: request.redaction_secrets(),
            usage_input: None,
            usage_output: None,
            stop_reason: None,
            tools: BTreeMap::new(),
            terminated: false,
        }),
    })
}

/// Decodes the Anthropic Messages SSE stream into sampling events.
struct Decoder {
    parser: SseParser,
    /// Kept only to scrub server-echoed secrets out of error messages.
    secrets: Vec<String>,
    usage_input: Option<u64>,
    usage_output: Option<u64>,
    stop_reason: Option<StopReason>,
    /// Tool calls by content-block index, buffered until `message_stop`.
    tools: BTreeMap<u64, ToolCallBuffer>,
    terminated: bool,
}

type Out = VecDeque<Result<SamplingEvent, SamplingError>>;

impl Decoder {
    /// Flags the stream failed and queues the classified error; later blocks
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
        let Ok(value) = serde_json::from_str::<Value>(&event.data) else {
            self.fail(out, "SSE data line is not valid JSON");
            return;
        };
        // The `event:` line names the block; fall back to the payload's own
        // `type` field (both carry the same name on this protocol).
        let kind = event
            .event
            .as_deref()
            .or(value.get("type").and_then(Value::as_str))
            .unwrap_or_default();
        match kind {
            "message_start" => {
                self.usage_input = value
                    .pointer("/message/usage/input_tokens")
                    .and_then(Value::as_u64);
            }
            "content_block_start" => self.block_start(&value, out),
            "content_block_delta" => self.delta(&value, out),
            "message_delta" => {
                if let Some(reason) = value.pointer("/delta/stop_reason").and_then(Value::as_str) {
                    self.stop_reason = Some(map_stop_reason(reason));
                }
                if let Some(tokens) = value
                    .pointer("/usage/output_tokens")
                    .and_then(Value::as_u64)
                {
                    self.usage_output = Some(tokens);
                }
            }
            "message_stop" => {
                self.terminated = true;
                emit_finished(
                    self.usage_input,
                    self.usage_output,
                    std::mem::take(&mut self.tools).into_values(),
                    self.stop_reason.unwrap_or(StopReason::Other),
                    out,
                );
            }
            "error" => {
                let message = value
                    .pointer("/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("provider streamed an error event");
                let sanitized = sanitize_message_with_secrets(message, &self.secrets);
                self.fail(out, sanitized);
            }
            // `ping`, `content_block_stop` and future event kinds carry
            // nothing this layer needs; unknown well-formed blocks are
            // skipped silently.
            _ => {}
        }
    }

    /// Registers a `tool_use` block; text blocks need no bookkeeping.
    fn block_start(&mut self, value: &Value, out: &mut Out) {
        if value.pointer("/content_block/type").and_then(Value::as_str) != Some("tool_use") {
            return;
        }
        let Some(index) = value.get("index").and_then(Value::as_u64) else {
            self.fail(out, "content_block_start without index");
            return;
        };
        let (Some(id), Some(name)) = (
            value.pointer("/content_block/id").and_then(Value::as_str),
            value.pointer("/content_block/name").and_then(Value::as_str),
        ) else {
            self.fail(out, "tool_use block without id or name");
            return;
        };
        self.tools.insert(
            index,
            ToolCallBuffer {
                id: id.to_string(),
                name: name.to_string(),
                arguments: String::new(),
            },
        );
    }

    fn delta(&mut self, value: &Value, out: &mut Out) {
        match value
            .pointer("/delta/type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "text_delta" => {
                let Some(text) = value.pointer("/delta/text").and_then(Value::as_str) else {
                    self.fail(out, "text_delta without text");
                    return;
                };
                out.push_back(Ok(SamplingEvent::TextDelta {
                    text: text.to_string(),
                }));
            }
            "input_json_delta" => {
                let Some(index) = value.get("index").and_then(Value::as_u64) else {
                    self.fail(out, "input_json_delta without index");
                    return;
                };
                let Some(fragment) = value.pointer("/delta/partial_json").and_then(Value::as_str)
                else {
                    self.fail(out, "input_json_delta without partial_json");
                    return;
                };
                match self.tools.get_mut(&index) {
                    Some(entry) => entry.arguments.push_str(fragment),
                    None => self.fail(out, "input_json_delta for unknown block index"),
                }
            }
            // thinking/signature deltas and future kinds: skipped.
            _ => {}
        }
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
            out.push_back(Err(SamplingError::ProtocolError {
                message: "stream ended without message_stop".into(),
            }));
        }
    }
}

fn map_stop_reason(raw: &str) -> StopReason {
    match raw {
        "end_turn" | "stop_sequence" => StopReason::Stop,
        "tool_use" => StopReason::ToolUse,
        "max_tokens" => StopReason::MaxTokens,
        _ => StopReason::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sampling::client::test_support::drive_decoder;

    #[test]
    fn stream_without_message_stop_is_a_protocol_error() {
        let payload = "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":3}}}\n\n";
        let events = drive_decoder(prepare(&request()).unwrap().decoder, payload);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            events[0],
            Err(SamplingError::ProtocolError { .. })
        ));
    }

    #[test]
    fn input_json_delta_for_unknown_block_is_a_protocol_error() {
        let payload = concat!(
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":4,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}\n\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        );
        let events = drive_decoder(prepare(&request()).unwrap().decoder, payload);
        assert!(events
            .iter()
            .any(|event| matches!(event, Err(SamplingError::ProtocolError { .. }))));
    }

    #[test]
    fn max_tokens_defaults_to_4096_when_absent() {
        let prepared = prepare(&request()).unwrap();
        assert_eq!(prepared.body["max_tokens"], serde_json::json!(4096));
        assert!(prepared.body.get("temperature").is_none());
        assert_eq!(prepared.url, "https://provider.test/v1/messages");
    }

    fn request() -> SamplingRequest {
        SamplingRequest {
            base_url: "https://provider.test/v1".into(),
            api_format: crate::sampling::types::ApiFormat::AnthropicMessages,
            model: "claude-3".into(),
            api_key: Some("secret-key".into()),
            extra_headers: vec![],
            messages: vec![],
            tools: vec![],
            max_tokens: None,
        }
    }
}

//! `openai_responses` adapter (task §3.4).
//!
//! `POST {base}/responses` with `Authorization: Bearer` (design D2). Text
//! arrives via `response.output_text.delta`; function calls are opened by
//! `response.output_item.added` and their arguments streamed through
//! `response.function_call_arguments.delta`. Well-formed events this layer
//! does not need (reasoning, refusal, code-interpreter, content parts, …)
//! are skipped silently; malformed payloads and unterminated streams are
//! protocol errors (design D3).

use std::collections::{BTreeMap, VecDeque};

use serde_json::{json, Value};

use super::client::{
    emit_finished, endpoint, sanitize_message_with_secrets, PreparedRequest, ProtocolDecoder,
    ToolCallBuffer,
};
use super::sse::{SseEvent, SseParser};
use super::types::{SamplingError, SamplingEvent, SamplingRequest, StopReason};

/// Builds the wire request and decoder for the OpenAI Responses protocol.
pub(crate) fn prepare(request: &SamplingRequest) -> Result<PreparedRequest, SamplingError> {
    let mut headers = Vec::new();
    if let Some(key) = request.api_key.as_deref().filter(|key| !key.is_empty()) {
        headers.push(("authorization", format!("Bearer {key}")));
    }

    // Sampling preferences stay unset (task §3.7); `max_output_tokens` is
    // sent only when explicitly given — no default on this format.
    let mut body = json!({
        "model": request.model,
        // A turn's content is typed per role: user input is `input_text`,
        // prior assistant output is `output_text`.
        "input": request.messages.iter().map(|message| {
            let content_type = match message.role {
                crate::sampling::types::MessageRole::User => "input_text",
                crate::sampling::types::MessageRole::Assistant => "output_text",
            };
            json!({
                "role": message.role.label(),
                "content": [{ "type": content_type, "text": message.content }],
            })
        }).collect::<Vec<_>>(),
        "stream": true,
    });
    if !request.tools.is_empty() {
        body["tools"] = json!(request
            .tools
            .iter()
            .map(|tool| json!({
                "type": "function",
                "name": tool.name,
                "description": tool.description,
                "parameters": tool.input_schema,
            }))
            .collect::<Vec<_>>());
    }
    if let Some(max_tokens) = request.max_tokens {
        body["max_output_tokens"] = json!(max_tokens);
    }

    Ok(PreparedRequest {
        url: endpoint(&request.base_url, "/responses"),
        headers,
        body,
        decoder: Box::new(Decoder {
            parser: SseParser::new(),
            secrets: request.redaction_secrets(),
            tools: BTreeMap::new(),
            usage_input: None,
            usage_output: None,
            terminated: false,
        }),
    })
}

/// Decodes the Responses SSE stream into sampling events.
struct Decoder {
    parser: SseParser,
    /// Kept only to scrub server-echoed secrets out of error messages.
    secrets: Vec<String>,
    /// Function calls by `output_index`, buffered until completion.
    tools: BTreeMap<u64, ToolCallBuffer>,
    usage_input: Option<u64>,
    usage_output: Option<u64>,
    terminated: bool,
}

type Out = VecDeque<Result<SamplingEvent, SamplingError>>;

impl Decoder {
    /// Flags the stream failed and queues the classified error; later events
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
            "response.output_text.delta" => {
                let Some(text) = value.get("delta").and_then(Value::as_str) else {
                    self.fail(out, "response.output_text.delta without delta");
                    return;
                };
                out.push_back(Ok(SamplingEvent::TextDelta {
                    text: text.to_string(),
                }));
            }
            "response.output_item.added" => self.item_added(&value, out),
            "response.function_call_arguments.delta" => {
                let Some(index) = value.get("output_index").and_then(Value::as_u64) else {
                    self.fail(
                        out,
                        "response.function_call_arguments.delta without output_index",
                    );
                    return;
                };
                let Some(fragment) = value.get("delta").and_then(Value::as_str) else {
                    self.fail(out, "response.function_call_arguments.delta without delta");
                    return;
                };
                match self.tools.get_mut(&index) {
                    Some(entry) => entry.arguments.push_str(fragment),
                    None => self.fail(out, "function_call_arguments.delta for unknown output item"),
                }
            }
            "response.function_call_arguments.done" => {
                let Some(index) = value.get("output_index").and_then(Value::as_u64) else {
                    self.fail(
                        out,
                        "response.function_call_arguments.done without output_index",
                    );
                    return;
                };
                if let Some(entry) = self.tools.get_mut(&index) {
                    // The done event carries the complete string — it
                    // overwrites whatever the shards assembled.
                    entry.arguments = value
                        .get("arguments")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                }
            }
            "response.completed" => self.complete(&value, false, out),
            "response.incomplete" => self.complete(&value, true, out),
            "response.failed" => {
                let message = value
                    .pointer("/response/error/message")
                    .and_then(Value::as_str)
                    .unwrap_or("response failed");
                let sanitized = sanitize_message_with_secrets(message, &self.secrets);
                self.fail(out, sanitized);
            }
            "error" => {
                let message = value
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("provider streamed an error event");
                let sanitized = sanitize_message_with_secrets(message, &self.secrets);
                self.fail(out, sanitized);
            }
            // created / in_progress / content_part / output_text.done /
            // output_item.done / refusal / reasoning / …: skipped silently.
            _ => {}
        }
    }

    /// Registers a `function_call` output item; message and reasoning items
    /// need no bookkeeping.
    fn item_added(&mut self, value: &Value, out: &mut Out) {
        if value.pointer("/item/type").and_then(Value::as_str) != Some("function_call") {
            return;
        }
        let Some(index) = value.get("output_index").and_then(Value::as_u64) else {
            self.fail(out, "response.output_item.added without output_index");
            return;
        };
        let Some(name) = value.pointer("/item/name").and_then(Value::as_str) else {
            self.fail(out, "function_call item without name");
            return;
        };
        // `call_id` is the handle the follow-up output references; some
        // builds only expose the item `id`.
        let id = value
            .pointer("/item/call_id")
            .or_else(|| value.pointer("/item/id"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        self.tools.insert(
            index,
            ToolCallBuffer {
                id: id.to_string(),
                name: name.to_string(),
                arguments: value
                    .pointer("/item/arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            },
        );
    }

    /// Terminal path for `response.completed` / `response.incomplete`:
    /// records usage and emits the buffered tail.
    fn complete(&mut self, value: &Value, incomplete: bool, out: &mut Out) {
        self.terminated = true;
        self.usage_input = value
            .pointer("/response/usage/input_tokens")
            .and_then(Value::as_u64);
        self.usage_output = value
            .pointer("/response/usage/output_tokens")
            .and_then(Value::as_u64);
        let reason = if !self.tools.is_empty() {
            StopReason::ToolUse
        } else if incomplete {
            match value
                .pointer("/response/incomplete_details/reason")
                .and_then(Value::as_str)
            {
                Some("max_output_tokens") => StopReason::MaxTokens,
                _ => StopReason::Other,
            }
        } else {
            StopReason::Stop
        };
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
            out.push_back(Err(SamplingError::ProtocolError {
                message: "stream ended before response.completed".into(),
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sampling::client::test_support::drive_decoder;
    use crate::sampling::types::{ApiFormat, MessageRole, SamplingMessage};

    #[test]
    fn stream_without_completion_event_is_a_protocol_error() {
        let payload = "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n";
        let events = drive_decoder(prepare(&request()).unwrap().decoder, payload);
        assert!(events
            .iter()
            .any(|event| matches!(event, Err(SamplingError::ProtocolError { .. }))));
    }

    #[test]
    fn unknown_well_formed_events_are_skipped_silently() {
        let payload = concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\"}}\n\n",
            "event: response.reasoning_text.delta\n",
            "data: {\"type\":\"response.reasoning_text.delta\",\"delta\":\"thinking\"}\n\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"hi\"}\n\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"usage\":{\"input_tokens\":2,\"output_tokens\":1}}}\n\n",
        );
        let events = drive_decoder(prepare(&request()).unwrap().decoder, payload);
        assert_eq!(
            events,
            vec![
                Ok(SamplingEvent::TextDelta { text: "hi".into() }),
                Ok(SamplingEvent::Usage {
                    input_tokens: Some(2),
                    output_tokens: Some(1)
                }),
                Ok(SamplingEvent::Finished {
                    reason: StopReason::Stop
                }),
            ]
        );
    }

    #[test]
    fn input_maps_roles_to_typed_content() {
        let mut req = request();
        req.messages = vec![
            SamplingMessage {
                role: MessageRole::User,
                content: "hi".into(),
            },
            SamplingMessage {
                role: MessageRole::Assistant,
                content: "hello".into(),
            },
        ];
        let prepared = prepare(&req).unwrap();
        assert_eq!(
            prepared.body["input"][0]["content"][0]["type"],
            json!("input_text")
        );
        assert_eq!(
            prepared.body["input"][1]["content"][0]["type"],
            json!("output_text")
        );
        assert!(prepared.body.get("max_output_tokens").is_none());
        assert_eq!(prepared.url, "https://provider.test/v1/responses");

        req.max_tokens = Some(512);
        assert_eq!(prepare(&req).unwrap().body["max_output_tokens"], json!(512));
    }

    fn request() -> SamplingRequest {
        SamplingRequest {
            connection: Default::default(),
            base_url: "https://provider.test/v1".into(),
            api_format: ApiFormat::OpenaiResponses,
            model: "gpt-test".into(),
            api_key: Some("secret-key".into()),
            extra_headers: vec![],
            messages: vec![],
            tools: vec![],
            max_tokens: None,
        }
    }
}

//! HTTP glue shared by the three protocol adapters (tasks §3.5/3.6).
//!
//! [`sample`] sends the request, classifies connection and HTTP-status
//! failures eagerly (design D3), then hands back a [`SamplingStream`] that
//! decodes SSE chunks through the adapter's decoder. Deadlines are layered
//! per design D5: connect and idle gaps get the short budget, the whole
//! generation the long one. This layer never retries — retry policy belongs
//! to upper layers.

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{Stream, StreamExt};
use reqwest::StatusCode;

use crate::logging;

use super::types::{
    ApiFormat, SamplingError, SamplingEvent, SamplingRequest, StopReason, Timeouts,
};

/// Log-module tag for the sampling channel (`[INFO] [sampling] …`).
const MODULE: &str = "sampling";

/// Cap for provider-supplied text carried in error messages, so a hostile or
/// chatty provider can neither flood logs nor smuggle long payloads across
/// the IPC boundary.
const MESSAGE_LIMIT: usize = 300;

/// Everything one protocol adapter contributes to a request: where to send,
/// how to authenticate and how to decode the streamed body (design D2).
pub(crate) struct PreparedRequest {
    pub url: String,
    /// Header pairs applied verbatim; an auth header appears here only when
    /// an API key exists — local endpoints send none.
    pub headers: Vec<(&'static str, String)>,
    pub body: serde_json::Value,
    pub decoder: Box<dyn ProtocolDecoder + Send>,
}

/// Incremental decoder from raw SSE body bytes to sampling events. `feed`
/// runs per network chunk; `finish` runs once at body end and either emits
/// the buffered tail (usage, tool calls, termination) or a protocol error
/// when the stream never reached its protocol-specific terminal event.
pub(crate) trait ProtocolDecoder: Send {
    fn feed(&mut self, chunk: &[u8], out: &mut VecDeque<Result<SamplingEvent, SamplingError>>);
    fn finish(&mut self, out: &mut VecDeque<Result<SamplingEvent, SamplingError>>);
}

/// Assembles one tool call from streamed fragments (task §3.1: tool calls
/// are never surfaced half-assembled, so arguments are validated whole).
pub(crate) struct ToolCallBuffer {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl ToolCallBuffer {
    /// Produces the finished [`SamplingEvent::ToolCall`], defaulting absent
    /// arguments to `{}` and rejecting fragments that never became valid
    /// JSON.
    pub fn finish(&self) -> Result<SamplingEvent, SamplingError> {
        let arguments = if self.arguments.trim().is_empty() {
            "{}".to_string()
        } else {
            self.arguments.clone()
        };
        match serde_json::from_str::<serde_json::Value>(&arguments) {
            Ok(_) => Ok(SamplingEvent::ToolCall {
                id: self.id.clone(),
                name: self.name.clone(),
                arguments,
            }),
            Err(_) => Err(SamplingError::ProtocolError {
                message: format!("tool '{}' streamed malformed JSON arguments", self.name),
            }),
        }
    }
}

/// Emits the shared tail sequence: usage first, then buffered tool calls in
/// arrival order, then termination (task §3.1: tool calls are delivered
/// together after the stream ends).
pub(crate) fn emit_finished(
    usage_input: Option<u64>,
    usage_output: Option<u64>,
    tools: impl IntoIterator<Item = ToolCallBuffer>,
    reason: StopReason,
    out: &mut VecDeque<Result<SamplingEvent, SamplingError>>,
) {
    if usage_input.is_some() || usage_output.is_some() {
        out.push_back(Ok(SamplingEvent::Usage {
            input_tokens: usage_input,
            output_tokens: usage_output,
        }));
    }
    for tool in tools {
        out.push_back(tool.finish());
    }
    out.push_back(Ok(SamplingEvent::Finished { reason }));
}

/// Truncates provider-supplied text on a char boundary.
fn truncate(text: &str) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= MESSAGE_LIMIT {
        trimmed.to_string()
    } else {
        trimmed.chars().take(MESSAGE_LIMIT).collect()
    }
}

/// Replaces the API key inside provider-controlled text before it enters an
/// error message (defense in depth on top of log-side redaction).
fn redact_secret(text: &str, api_key: Option<&str>) -> String {
    match api_key.filter(|key| !key.is_empty()) {
        Some(key) if text.contains(key) => text.replace(key, "[REDACTED]"),
        _ => text.to_string(),
    }
}

/// Prepares provider-supplied text for an error message: key redaction plus
/// a length cap.
pub(crate) fn sanitize_message(text: &str, api_key: Option<&str>) -> String {
    truncate(&redact_secret(text, api_key))
}

/// Joins a base URL (scheme + optional path prefix) with an endpoint path
/// (design D2). A trailing slash on the base or a leading slash on the path
/// never yields `//`.
pub(crate) fn endpoint(base_url: &str, path: &str) -> String {
    format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    )
}

/// Sends one streaming sampling request and returns its decoded event
/// stream.
///
/// Connection and HTTP-status failures are classified before any event is
/// produced; mid-stream failures surface as the stream's final `Err` item.
pub async fn sample(
    request: SamplingRequest,
    timeouts: Timeouts,
) -> Result<SamplingStream, SamplingError> {
    if let Some(key) = request.api_key.as_deref().filter(|key| !key.is_empty()) {
        // Registered before anything can log, so even a key echoed back by a
        // hostile provider is masked on disk.
        logging::register_secret(key);
    }
    logging::debug(
        MODULE,
        &format!(
            "request started: format={}, model={}, tools={}",
            request.api_format.label(),
            request.model,
            request.tools.len()
        ),
    );

    let prepared = match request.api_format {
        ApiFormat::AnthropicMessages => super::anthropic::prepare(&request),
        ApiFormat::OpenaiChatCompletions => super::openai_chat::prepare(&request),
        ApiFormat::OpenaiResponses => super::openai_responses::prepare(&request),
    }?;

    // Layered deadlines (design D5): the short budget bounds the connect
    // phase and idle gaps between chunks, the long one the whole request
    // from connect until the body is fully read.
    let client = reqwest::Client::builder()
        .connect_timeout(timeouts.connect_idle)
        .read_timeout(timeouts.connect_idle)
        .timeout(timeouts.total_generate)
        .build()
        .map_err(|e| SamplingError::ProviderUnreachable {
            message: e.to_string(),
        })?;

    let mut builder = client
        .post(&prepared.url)
        .header(reqwest::header::ACCEPT, "text/event-stream")
        .json(&prepared.body);
    for (name, value) in &prepared.headers {
        builder = builder.header(*name, value.as_str());
    }

    let response = match builder.send().await {
        Ok(response) => response,
        Err(error) => {
            let classified = classify_transport(error);
            log_failure(&classified);
            return Err(classified);
        }
    };

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        let classified = classify_http_status(status, &body, request.api_key.as_deref());
        log_failure(&classified);
        return Err(classified);
    }

    let inner = response
        .bytes_stream()
        .map(|chunk| chunk.map(|bytes| bytes.to_vec()));
    Ok(SamplingStream {
        inner: Box::pin(inner),
        decoder: prepared.decoder,
        pending: VecDeque::new(),
        terminated: false,
    })
}

/// Event stream of one in-flight sampling request. Drives the protocol
/// decoder over the raw body chunks; usage, tool calls and termination
/// arrive once the stream reaches its terminal event (task §3.1).
pub struct SamplingStream {
    inner: Pin<Box<dyn Stream<Item = Result<Vec<u8>, reqwest::Error>> + Send>>,
    decoder: Box<dyn ProtocolDecoder + Send>,
    pending: VecDeque<Result<SamplingEvent, SamplingError>>,
    /// Set once the upstream body ended or failed; the stream yields `None`
    /// after the pending tail (including a possible error) is drained.
    terminated: bool,
}

// Manual impl because the boxed fields are not `Debug`; nothing about the
// in-flight request (bodies, keys) is printed.
impl std::fmt::Debug for SamplingStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SamplingStream")
            .field("pending", &self.pending.len())
            .field("terminated", &self.terminated)
            .finish()
    }
}

impl Stream for SamplingStream {
    type Item = Result<SamplingEvent, SamplingError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // `SamplingStream` is Unpin (it only owns boxed/pinned fields), so
        // the struct can be borrowed mutably and its fields addressed
        // disjointly.
        let this = self.get_mut();
        loop {
            if let Some(item) = this.pending.pop_front() {
                log_outcome(&item);
                return Poll::Ready(Some(item));
            }
            if this.terminated {
                return Poll::Ready(None);
            }
            match this.inner.as_mut().poll_next(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Some(Ok(chunk))) => this.decoder.feed(&chunk, &mut this.pending),
                Poll::Ready(Some(Err(error))) => {
                    this.terminated = true;
                    let classified = classify_transport(error);
                    log_failure(&classified);
                    this.pending.push_back(Err(classified));
                }
                Poll::Ready(None) => {
                    this.terminated = true;
                    this.decoder.finish(&mut this.pending);
                }
            }
        }
    }
}

/// Maps a reqwest failure onto design-D3 classes: timeouts, connection
/// failures and remaining transport faults.
fn classify_transport(error: reqwest::Error) -> SamplingError {
    if error.is_timeout() {
        SamplingError::Timeout
    } else if error.is_builder() {
        // e.g. an unusable base_url — rejected before any bytes hit the wire.
        SamplingError::InvalidRequest {
            message: sanitize_message(&error.to_string(), None),
        }
    } else {
        SamplingError::ProviderUnreachable {
            message: error.to_string(),
        }
    }
}

/// Maps a non-2xx response onto design-D3 classes. Anthropic and both OpenAI
/// shapes nest the human-readable text under `error.message`; unknown bodies
/// fall back to raw (truncated) text.
fn classify_http_status(status: StatusCode, body: &str, api_key: Option<&str>) -> SamplingError {
    if status.is_server_error() {
        return SamplingError::ProviderUnreachable {
            message: format!("server error: HTTP {status}"),
        };
    }
    let provider_message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.to_string());
    match status.as_u16() {
        401 | 403 => SamplingError::AuthFailed,
        429 => SamplingError::RateLimited,
        _ => SamplingError::InvalidRequest {
            message: sanitize_message(&provider_message, api_key),
        },
    }
}

/// Records a classified failure on the sampling log channel (debug level:
/// diagnostics only; never includes message bodies or keys).
fn log_failure(error: &SamplingError) {
    logging::debug(MODULE, &format!("request failed: code={}", error.code()));
}

/// Logs stream completion once the terminal event reaches the consumer.
fn log_outcome(item: &Result<SamplingEvent, SamplingError>) {
    match item {
        Ok(SamplingEvent::Finished { reason }) => logging::debug(
            MODULE,
            &format!("request finished: reason={}", reason.label()),
        ),
        Err(error) => log_failure(error),
        _ => {}
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Shared helpers for adapter unit tests: drive a decoder over a static
    //! SSE payload without any server.

    use super::*;

    /// Feeds `payload` through `decoder` in one chunk and finishes it,
    /// returning everything it produced.
    pub(crate) fn drive_decoder(
        mut decoder: Box<dyn ProtocolDecoder>,
        payload: &str,
    ) -> Vec<Result<SamplingEvent, SamplingError>> {
        let mut out = VecDeque::new();
        decoder.feed(payload.as_bytes(), &mut out);
        decoder.finish(&mut out);
        out.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_message_redacts_key_and_caps_length() {
        let key = "sk-secret-1";
        let long: String = "x".repeat(MESSAGE_LIMIT + 50);
        let sanitized = sanitize_message(&format!("echo {key} then {long}"), Some(key));
        assert!(!sanitized.contains(key));
        assert!(sanitized.contains("[REDACTED]"));
        assert_eq!(sanitized.chars().count(), MESSAGE_LIMIT);
        // Unrelated messages pass through untouched.
        assert_eq!(sanitize_message("plain", Some(key)), "plain");
    }

    #[test]
    fn endpoint_joins_base_and_path_without_double_slash() {
        assert_eq!(
            endpoint("https://api.example.com/v1", "/messages"),
            "https://api.example.com/v1/messages"
        );
        assert_eq!(
            endpoint("http://127.0.0.1:8080/v1/", "/chat/completions"),
            "http://127.0.0.1:8080/v1/chat/completions"
        );
    }

    #[test]
    fn tool_call_buffer_rejects_malformed_arguments() {
        let buffer = ToolCallBuffer {
            id: "call_1".into(),
            name: "tool".into(),
            arguments: "{\"a\": ".into(),
        };
        assert!(matches!(
            buffer.finish(),
            Err(SamplingError::ProtocolError { .. })
        ));
    }

    #[test]
    fn tool_call_buffer_defaults_empty_arguments_to_object() {
        let buffer = ToolCallBuffer {
            id: "call_1".into(),
            name: "tool".into(),
            arguments: String::new(),
        };
        assert_eq!(
            buffer.finish().unwrap(),
            SamplingEvent::ToolCall {
                id: "call_1".into(),
                name: "tool".into(),
                arguments: "{}".into()
            }
        );
    }
}

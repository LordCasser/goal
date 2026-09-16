//! Shared request and event types for the sampling layer (task §3.1).
//!
//! These are the app's neutral shapes: each protocol adapter maps them onto
//! its wire format, and the future IPC layer converts between provider
//! configuration and [`SamplingRequest`]. Nothing here mentions a specific
//! vendor.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// Wire protocol a provider speaks; decides endpoint path, auth header and
/// SSE decoding (design D2). Values serialize snake_case so provider
/// configuration round-trips them unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiFormat {
    AnthropicMessages,
    OpenaiChatCompletions,
    OpenaiResponses,
}

impl ApiFormat {
    /// Stable lower-snake label for log lines (never contains secrets).
    pub fn label(self) -> &'static str {
        match self {
            Self::AnthropicMessages => "anthropic_messages",
            Self::OpenaiChatCompletions => "openai_chat_completions",
            Self::OpenaiResponses => "openai_responses",
        }
    }
}

/// Author of one conversation turn. This change ships text sampling; system
/// prompts and multimodal content belong to later changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
}

impl MessageRole {
    /// Lowercase wire label shared by all three protocols.
    pub fn label(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
        }
    }
}

/// One conversation turn. Text-only by design (task §3.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SamplingMessage {
    pub role: MessageRole,
    pub content: String,
}

/// Tool definition in this app's neutral shape; adapters map it onto each
/// protocol's tool schema and the JSON schema passes through untouched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// One sampling request: base_url + api_format + model + api_key plus the
/// conversation. Everything else in provider configuration is metadata that
/// does not reach the wire (design D1).
#[derive(Clone)]
pub struct SamplingRequest {
    /// Includes scheme and path prefix, e.g. `https://api.example.com/v1`.
    pub base_url: String,
    pub api_format: ApiFormat,
    pub model: String,
    /// Local endpoints may have none; then no auth header is sent.
    pub api_key: Option<String>,
    /// Provider-specific request headers resolved from the system credential
    /// store. Values are kept in memory only for the duration of a request.
    pub extra_headers: Vec<(String, String)>,
    pub messages: Vec<SamplingMessage>,
    pub tools: Vec<ToolSpec>,
    /// Resolution order (task §3.7): an explicit request value wins; only
    /// the `anthropic_messages` adapter fills a default when `None` (the
    /// protocol makes the field mandatory).
    pub max_tokens: Option<u64>,
}

impl std::fmt::Debug for SamplingRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SamplingRequest")
            .field("base_url", &self.base_url)
            .field("api_format", &self.api_format)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .field(
                "extra_headers",
                &self
                    .extra_headers
                    .iter()
                    .map(|(name, _)| (name, "[REDACTED]"))
                    .collect::<Vec<_>>(),
            )
            .field("messages", &self.messages)
            .field("tools", &self.tools)
            .field("max_tokens", &self.max_tokens)
            .finish()
    }
}

impl SamplingRequest {
    /// Returns the exact values that must be scrubbed from provider-controlled
    /// errors and stream events. Header names are intentionally omitted.
    pub(crate) fn redaction_secrets(&self) -> Vec<String> {
        let mut secrets = Vec::with_capacity(self.extra_headers.len() + 1);
        if let Some(key) = self.api_key.as_deref().filter(|key| !key.is_empty()) {
            secrets.push(key.to_string());
        }
        secrets.extend(
            self.extra_headers
                .iter()
                .filter(|(_, value)| !value.is_empty())
                .map(|(_, value)| value.clone()),
        );
        secrets
    }
}

/// Why a finished stream ended, normalized across protocols.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    Stop,
    ToolUse,
    MaxTokens,
    Other,
}

impl StopReason {
    /// Stable lower-snake label for log lines.
    pub fn label(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::ToolUse => "tool_use",
            Self::MaxTokens => "max_tokens",
            Self::Other => "other",
        }
    }
}

/// Neutral event stream produced by every adapter. Tool calls are delivered
/// together after the stream's terminal event, never half-assembled (task
/// §3.1); text deltas stream as they arrive.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SamplingEvent {
    TextDelta {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: String,
    },
    Usage {
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
    },
    Finished {
        reason: StopReason,
    },
}

/// Classified failure of one sampling request (design D3). The class decides
/// whether upper layers may retry; this layer never retries on its own.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SamplingError {
    /// HTTP 401/403 — the caller should check the API key.
    #[error("authentication failed")]
    AuthFailed,
    /// Other HTTP 4xx — the provider rejected the request. The message comes
    /// from the provider (truncated) and can never contain the API key.
    #[error("invalid request: {message}")]
    InvalidRequest { message: String },
    /// HTTP 429 — retry with back-off.
    #[error("rate limited")]
    RateLimited,
    /// Connection failure or server-side 5xx — transient.
    #[error("provider unreachable: {message}")]
    ProviderUnreachable { message: String },
    /// Connect, idle or total generation deadline exceeded (design D5).
    #[error("request timed out")]
    Timeout,
    /// The response body violates the protocol; the request fails and is
    /// never retried (design D3).
    #[error("protocol error: {message}")]
    ProtocolError { message: String },
}

impl SamplingError {
    /// Stable IPC code (design D3); pairs with [`SamplingError::to_app_error`].
    pub fn code(&self) -> &'static str {
        match self {
            Self::AuthFailed => "auth_failed",
            Self::InvalidRequest { .. } => "invalid_request",
            Self::RateLimited => "rate_limited",
            Self::ProviderUnreachable { .. } => "provider_unreachable",
            Self::Timeout => "timeout",
            Self::ProtocolError { .. } => "protocol_error",
        }
    }

    /// Presents the classified failure across the IPC boundary. The
    /// `Validation` variant is the `AppError` slot that carries an arbitrary
    /// `{code, message}` pair; the serialized shape is what the frontend
    /// maps, and the message never contains the API key.
    pub fn to_app_error(&self) -> AppError {
        AppError::Validation {
            code: self.code().to_string(),
            message: self.to_string(),
        }
    }
}

/// Layered deadlines (design D5): a short one for connection establishment
/// and idle gaps between SSE chunks — local big models can be slow to first
/// token — and a long one covering the whole generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timeouts {
    pub connect_idle: Duration,
    pub total_generate: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            connect_idle: Duration::from_secs(30),
            total_generate: Duration::from_secs(300),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_format_serializes_snake_case() {
        for (format, label) in [
            (ApiFormat::AnthropicMessages, "anthropic_messages"),
            (ApiFormat::OpenaiChatCompletions, "openai_chat_completions"),
            (ApiFormat::OpenaiResponses, "openai_responses"),
        ] {
            assert_eq!(
                serde_json::to_value(format).unwrap(),
                serde_json::json!(label)
            );
            assert_eq!(
                format,
                serde_json::from_value(serde_json::json!(label)).unwrap()
            );
            assert_eq!(format.label(), label);
        }
    }

    #[test]
    fn error_codes_cover_design_d3_classes() {
        let cases = [
            (SamplingError::AuthFailed, "auth_failed"),
            (
                SamplingError::InvalidRequest {
                    message: "m".into(),
                },
                "invalid_request",
            ),
            (SamplingError::RateLimited, "rate_limited"),
            (
                SamplingError::ProviderUnreachable {
                    message: "m".into(),
                },
                "provider_unreachable",
            ),
            (SamplingError::Timeout, "timeout"),
            (
                SamplingError::ProtocolError {
                    message: "m".into(),
                },
                "protocol_error",
            ),
        ];
        for (error, code) in cases {
            assert_eq!(error.code(), code);
            let app = error.to_app_error();
            match app {
                AppError::Validation { code: c, message } => {
                    assert_eq!(c, code);
                    // The display text is the message; no key material involved.
                    assert!(!message.is_empty());
                }
                other => panic!("expected Validation, got {other:?}"),
            }
        }
    }

    #[test]
    fn default_timeouts_match_design_d5() {
        let timeouts = Timeouts::default();
        assert_eq!(timeouts.connect_idle, Duration::from_secs(30));
        assert_eq!(timeouts.total_generate, Duration::from_secs(300));
    }

    #[test]
    fn request_debug_redacts_api_key_and_header_values() {
        let request = SamplingRequest {
            base_url: "http://localhost".into(),
            api_format: ApiFormat::OpenaiResponses,
            model: "model".into(),
            api_key: Some("api-key-secret".into()),
            extra_headers: vec![("x-route".into(), "route-secret".into())],
            messages: vec![],
            tools: vec![],
            max_tokens: None,
        };
        let debug = format!("{request:?}");
        assert!(!debug.contains("api-key-secret"));
        assert!(!debug.contains("route-secret"));
        assert!(debug.contains("[REDACTED]"));
    }
}

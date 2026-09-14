//! Sampling clients for the three BYOK API formats (task §3).
//!
//! One request abstraction in, one event stream out; per-protocol adapters
//! own endpoint path, auth header and SSE decoding (design D2/D5). Entry
//! point: [`sample`]. Failures are classified per design D3 and never
//! retried here — retry policy belongs to upper layers.

mod anthropic;
mod client;
mod openai_chat;
mod openai_responses;
mod sse;
mod types;

#[cfg(test)]
mod tests;

pub use client::{sample, SamplingStream};
pub use types::{
    ApiFormat, MessageRole, SamplingError, SamplingEvent, SamplingMessage, SamplingRequest,
    StopReason, Timeouts, ToolSpec,
};

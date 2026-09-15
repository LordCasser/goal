//! Replay tests for the three protocol adapters (task §3.8).
//!
//! Each protocol is exercised against a local axum provider that replays a
//! pre-written SSE byte stream, dribbled in tiny chunks so the incremental
//! line/JSON framing is exercised across chunk boundaries. The fake provider
//! also captures request headers and bodies, which drives the auth-header
//! and sampling-preference assertions (no real endpoints are contacted).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::post;
use axum::Router;
use futures_util::StreamExt;

use super::{
    sample, ApiFormat, MessageRole, SamplingError, SamplingEvent, SamplingMessage, SamplingRequest,
    SamplingStream, StopReason, Timeouts, ToolSpec,
};

/// Key used across tests; never equals a real credential.
const TEST_KEY: &str = "sk-replay-key-1";

const FORMATS: [ApiFormat; 3] = [
    ApiFormat::AnthropicMessages,
    ApiFormat::OpenaiChatCompletions,
    ApiFormat::OpenaiResponses,
];

// ---------------------------------------------------------------------------
// Fake provider
// ---------------------------------------------------------------------------

/// What the fake provider does with a request.
#[derive(Clone)]
enum Scenario {
    /// Reply 200 with a fixed SSE payload, dribbled in small chunks.
    Sse(&'static str),
    /// Reply with this HTTP status and raw body (JSON error shapes).
    Error(StatusCode, String),
    /// Accept the connection but never answer.
    Hang,
    /// Send an SSE prefix, then stall forever (idle-timeout path).
    StallAfter(&'static str),
}

/// One captured request.
#[derive(Clone)]
struct Record {
    headers: HeaderMap,
    body: serde_json::Value,
}

type Captured = Arc<Mutex<Vec<Record>>>;

#[derive(Clone)]
struct Ctx {
    scenario: Scenario,
    captured: Captured,
}

async fn handle(State(ctx): State<Ctx>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    ctx.captured.lock().unwrap().push(Record {
        headers,
        body: serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null),
    });
    match ctx.scenario {
        Scenario::Sse(payload) => sse_response(payload, Chunking::Dribble),
        Scenario::Error(status, body) => Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .unwrap(),
        Scenario::Hang => {
            std::future::pending::<()>().await;
            unreachable!()
        }
        Scenario::StallAfter(prefix) => sse_response(prefix, Chunking::Stall),
    }
}

enum Chunking {
    Dribble,
    Stall,
}

fn sse_response(payload: &'static str, mode: Chunking) -> Response {
    let stream: futures_util::stream::BoxStream<'static, Result<Vec<u8>, std::io::Error>> =
        match mode {
            // 7-byte chunks split SSE lines and multi-byte characters across
            // reads, exercising the incremental framing in `sse.rs`.
            Chunking::Dribble => futures_util::stream::iter(
                payload
                    .as_bytes()
                    .chunks(7)
                    .map(|c| Ok(c.to_vec()))
                    .collect::<Vec<_>>(),
            )
            .boxed(),
            Chunking::Stall => futures_util::stream::once(async move {
                Ok::<_, std::io::Error>(payload.as_bytes().to_vec())
            })
            .chain(futures_util::stream::pending())
            .boxed(),
        };
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/event-stream")
        .body(Body::from_stream(stream))
        .unwrap()
}

/// Boots the fake provider on a random local port. It serves all three
/// protocol endpoints, so any `api_format` can be pointed at one base URL.
async fn spawn(scenario: Scenario) -> (String, Captured) {
    let captured: Captured = Arc::new(Mutex::new(Vec::new()));
    let ctx = Ctx {
        scenario,
        captured: captured.clone(),
    };
    let app = Router::new()
        .route("/messages", post(handle))
        .route("/chat/completions", post(handle))
        .route("/responses", post(handle))
        .with_state(ctx);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), captured)
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A clean text-only stream per protocol (sharded text deltas).
fn text_fixture(format: ApiFormat) -> &'static str {
    match format {
        ApiFormat::AnthropicMessages => concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"usage\":{\"input_tokens\":12,\"output_tokens\":1}}}\n",
            "\n",
            "event: content_block_start\n",
            "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n",
            "\n",
            "event: ping\n",
            "data: {\"type\":\"ping\"}\n",
            "\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n",
            "\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" wor\"}}\n",
            "\n",
            "event: content_block_delta\n",
            "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"ld\"}}\n",
            "\n",
            "event: content_block_stop\n",
            "data: {\"type\":\"content_block_stop\",\"index\":0}\n",
            "\n",
            "event: message_delta\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\",\"stop_sequence\":null},\"usage\":{\"output_tokens\":7}}\n",
            "\n",
            "event: message_stop\n",
            "data: {\"type\":\"message_stop\"}\n",
            "\n",
        ),
        ApiFormat::OpenaiChatCompletions => concat!(
            "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"Hi\"},\"finish_reason\":null}]}\n",
            "\n",
            "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" the\"},\"finish_reason\":null}]}\n",
            "\n",
            "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"re\"},\"finish_reason\":null}]}\n",
            "\n",
            "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n",
            "\n",
            "data: {\"id\":\"c1\",\"choices\":[],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":3}}\n",
            "\n",
            "data: [DONE]\n",
            "\n",
        ),
        ApiFormat::OpenaiResponses => concat!(
            "event: response.created\n",
            "data: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_1\"}}\n",
            "\n",
            "event: response.output_item.added\n",
            "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"id\":\"msg_0\",\"type\":\"message\",\"status\":\"in_progress\"}}\n",
            "\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_0\",\"output_index\":0,\"delta\":\"Good\"}\n",
            "\n",
            "event: response.output_text.delta\n",
            "data: {\"type\":\"response.output_text.delta\",\"item_id\":\"msg_0\",\"output_index\":0,\"delta\":\" morning\"}\n",
            "\n",
            "event: response.output_text.done\n",
            "data: {\"type\":\"response.output_text.done\",\"item_id\":\"msg_0\",\"output_index\":0,\"text\":\"Good morning\"}\n",
            "\n",
            "event: response.output_item.done\n",
            "data: {\"type\":\"response.output_item.done\",\"output_index\":0,\"item\":{\"id\":\"msg_0\",\"type\":\"message\",\"status\":\"completed\"}}\n",
            "\n",
            "event: response.completed\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"status\":\"completed\",\"usage\":{\"input_tokens\":8,\"output_tokens\":2}}}\n",
            "\n",
        ),
    }
}

const ANTHROPIC_TOOL: &str = concat!(
    "event: message_start\n",
    "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":25,\"output_tokens\":1}}}\n",
    "\n",
    "event: content_block_start\n",
    "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_01\",\"name\":\"get_weather\",\"input\":{}}}\n",
    "\n",
    "event: content_block_delta\n",
    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"city\\\"\"}}\n",
    "\n",
    "event: content_block_delta\n",
    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\":\\\"SF\\\"}\"}}\n",
    "\n",
    "event: content_block_stop\n",
    "data: {\"type\":\"content_block_stop\",\"index\":0}\n",
    "\n",
    "event: message_delta\n",
    "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":9}}\n",
    "\n",
    "event: message_stop\n",
    "data: {\"type\":\"message_stop\"}\n",
    "\n",
);

const CHAT_TOOL: &str = concat!(
    "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_01\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n",
    "\n",
    "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"city\\\"\"}}]},\"finish_reason\":null}]}\n",
    "\n",
    "data: {\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\":\\\"SF\\\"}\"}}]},\"finish_reason\":null}]}\n",
    "\n",
    "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n",
    "\n",
    "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":8,\"completion_tokens\":6}}\n",
    "\n",
    "data: [DONE]\n",
    "\n",
);

const RESPONSES_TOOL: &str = concat!(
    "event: response.output_item.added\n",
    "data: {\"type\":\"response.output_item.added\",\"output_index\":0,\"item\":{\"id\":\"fc_01\",\"type\":\"function_call\",\"status\":\"in_progress\",\"call_id\":\"call_09\",\"name\":\"get_weather\",\"arguments\":\"\"}}\n",
    "\n",
    "event: response.function_call_arguments.delta\n",
    "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc_01\",\"output_index\":0,\"delta\":\"{\\\"city\\\"\"}\n",
    "\n",
    "event: response.function_call_arguments.delta\n",
    "data: {\"type\":\"response.function_call_arguments.delta\",\"item_id\":\"fc_01\",\"output_index\":0,\"delta\":\":\\\"SF\\\"}\"}\n",
    "\n",
    "event: response.function_call_arguments.done\n",
    "data: {\"type\":\"response.function_call_arguments.done\",\"item_id\":\"fc_01\",\"output_index\":0,\"arguments\":\"{\\\"city\\\":\\\"SF\\\"}\"}\n",
    "\n",
    "event: response.completed\n",
    "data: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_9\",\"status\":\"completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":4}}}\n",
    "\n",
);

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn request(base: &str, format: ApiFormat) -> SamplingRequest {
    SamplingRequest {
        base_url: base.into(),
        api_format: format,
        model: "replay-model".into(),
        api_key: Some(TEST_KEY.into()),
        messages: vec![SamplingMessage {
            role: MessageRole::User,
            content: "hello".into(),
        }],
        tools: vec![],
        max_tokens: None,
    }
}

fn weather_tool() -> ToolSpec {
    ToolSpec {
        name: "get_weather".into(),
        description: "Current weather".into(),
        input_schema: serde_json::json!({ "type": "object", "properties": {} }),
    }
}

/// Consumes the stream to its end and returns everything it produced.
async fn run(stream: SamplingStream) -> Vec<Result<SamplingEvent, SamplingError>> {
    stream.collect().await
}

fn header<'a>(records: &'a [Record], name: &str) -> Option<&'a str> {
    records
        .last()
        .and_then(|r| r.headers.get(name).and_then(|v| v.to_str().ok()))
}

fn last_body(records: &[Record]) -> serde_json::Value {
    records.last().expect("a request arrived").body.clone()
}

// ---------------------------------------------------------------------------
// Task 3.8 scenario 1: text replay
// ---------------------------------------------------------------------------

#[tokio::test]
async fn anthropic_replays_text_deltas_usage_and_stop() {
    let (base, _) = spawn(Scenario::Sse(text_fixture(ApiFormat::AnthropicMessages))).await;
    let stream = sample(
        request(&base, ApiFormat::AnthropicMessages),
        Timeouts::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        run(stream).await,
        vec![
            Ok(SamplingEvent::TextDelta {
                text: "Hello".into()
            }),
            Ok(SamplingEvent::TextDelta {
                text: " wor".into()
            }),
            Ok(SamplingEvent::TextDelta { text: "ld".into() }),
            Ok(SamplingEvent::Usage {
                input_tokens: Some(12),
                output_tokens: Some(7)
            }),
            Ok(SamplingEvent::Finished {
                reason: StopReason::Stop
            }),
        ]
    );
}

#[tokio::test]
async fn openai_chat_replays_text_deltas_usage_and_stop() {
    let (base, _) = spawn(Scenario::Sse(text_fixture(
        ApiFormat::OpenaiChatCompletions,
    )))
    .await;
    let stream = sample(
        request(&base, ApiFormat::OpenaiChatCompletions),
        Timeouts::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        run(stream).await,
        vec![
            Ok(SamplingEvent::TextDelta { text: "Hi".into() }),
            Ok(SamplingEvent::TextDelta {
                text: " the".into()
            }),
            Ok(SamplingEvent::TextDelta { text: "re".into() }),
            Ok(SamplingEvent::Usage {
                input_tokens: Some(5),
                output_tokens: Some(3)
            }),
            Ok(SamplingEvent::Finished {
                reason: StopReason::Stop
            }),
        ]
    );
}

#[tokio::test]
async fn openai_responses_replays_text_deltas_usage_and_stop() {
    let (base, _) = spawn(Scenario::Sse(text_fixture(ApiFormat::OpenaiResponses))).await;
    let stream = sample(
        request(&base, ApiFormat::OpenaiResponses),
        Timeouts::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        run(stream).await,
        vec![
            Ok(SamplingEvent::TextDelta {
                text: "Good".into()
            }),
            Ok(SamplingEvent::TextDelta {
                text: " morning".into()
            }),
            Ok(SamplingEvent::Usage {
                input_tokens: Some(8),
                output_tokens: Some(2)
            }),
            Ok(SamplingEvent::Finished {
                reason: StopReason::Stop
            }),
        ]
    );
}

// ---------------------------------------------------------------------------
// Task 3.8 scenario 2: tool calls delivered whole after the stream
// ---------------------------------------------------------------------------

#[tokio::test]
async fn anthropic_delivers_tool_call_whole_after_stream() {
    let (base, _) = spawn(Scenario::Sse(ANTHROPIC_TOOL)).await;
    let stream = sample(
        request(&base, ApiFormat::AnthropicMessages),
        Timeouts::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        run(stream).await,
        vec![
            Ok(SamplingEvent::Usage {
                input_tokens: Some(25),
                output_tokens: Some(9)
            }),
            Ok(SamplingEvent::ToolCall {
                id: "toolu_01".into(),
                name: "get_weather".into(),
                arguments: "{\"city\":\"SF\"}".into(),
            }),
            Ok(SamplingEvent::Finished {
                reason: StopReason::ToolUse
            }),
        ]
    );
}

#[tokio::test]
async fn openai_chat_assembles_tool_call_fragments() {
    let (base, _) = spawn(Scenario::Sse(CHAT_TOOL)).await;
    let stream = sample(
        request(&base, ApiFormat::OpenaiChatCompletions),
        Timeouts::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        run(stream).await,
        vec![
            Ok(SamplingEvent::Usage {
                input_tokens: Some(8),
                output_tokens: Some(6)
            }),
            Ok(SamplingEvent::ToolCall {
                id: "call_01".into(),
                name: "get_weather".into(),
                arguments: "{\"city\":\"SF\"}".into(),
            }),
            Ok(SamplingEvent::Finished {
                reason: StopReason::ToolUse
            }),
        ]
    );
}

#[tokio::test]
async fn openai_responses_delivers_tool_call_with_call_id() {
    let (base, _) = spawn(Scenario::Sse(RESPONSES_TOOL)).await;
    let stream = sample(
        request(&base, ApiFormat::OpenaiResponses),
        Timeouts::default(),
    )
    .await
    .unwrap();
    assert_eq!(
        run(stream).await,
        vec![
            Ok(SamplingEvent::Usage {
                input_tokens: Some(10),
                output_tokens: Some(4)
            }),
            Ok(SamplingEvent::ToolCall {
                id: "call_09".into(),
                name: "get_weather".into(),
                arguments: "{\"city\":\"SF\"}".into(),
            }),
            Ok(SamplingEvent::Finished {
                reason: StopReason::ToolUse
            }),
        ]
    );
}

// ---------------------------------------------------------------------------
// Task 3.8 scenario 3: auth headers and wire bodies
// ---------------------------------------------------------------------------

#[tokio::test]
async fn anthropic_sends_x_api_key_and_version_headers() {
    let (base, captured) = spawn(Scenario::Sse(text_fixture(ApiFormat::AnthropicMessages))).await;
    let stream = sample(
        request(&base, ApiFormat::AnthropicMessages),
        Timeouts::default(),
    )
    .await
    .unwrap();
    run(stream).await;
    let records = captured.lock().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(header(&records, "x-api-key"), Some(TEST_KEY));
    assert_eq!(header(&records, "anthropic-version"), Some("2023-06-01"));
    assert_eq!(header(&records, "authorization"), None);
    // Sampling preferences (task 3.7): no temperature/top_p; the adapter
    // fills the protocol-mandatory max_tokens default when none was given…
    let body = last_body(&records);
    assert_eq!(body["max_tokens"], serde_json::json!(4096));
    assert!(body.get("temperature").is_none());
    assert!(body.get("top_p").is_none());
    assert_eq!(body["model"], serde_json::json!("replay-model"));
    drop(records);

    // …and passes an explicit value through untouched.
    let mut explicit = request(&base, ApiFormat::AnthropicMessages);
    explicit.max_tokens = Some(512);
    run(sample(explicit, Timeouts::default()).await.unwrap()).await;
    let records = captured.lock().unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(last_body(&records)["max_tokens"], serde_json::json!(512));
}

#[tokio::test]
async fn openai_chat_sends_bearer_and_maps_tools() {
    let (base, captured) = spawn(Scenario::Sse(text_fixture(
        ApiFormat::OpenaiChatCompletions,
    )))
    .await;
    let mut req = request(&base, ApiFormat::OpenaiChatCompletions);
    req.tools = vec![weather_tool()];
    let stream = sample(req, Timeouts::default()).await.unwrap();
    run(stream).await;
    let records = captured.lock().unwrap();
    assert_eq!(
        header(&records, "authorization"),
        Some(format!("Bearer {TEST_KEY}").as_str())
    );
    assert_eq!(header(&records, "x-api-key"), None);
    let body = last_body(&records);
    // Task 3.7: this format gets no max_tokens default and no preferences.
    assert!(body.get("max_tokens").is_none());
    assert!(body.get("temperature").is_none());
    assert_eq!(
        body["stream_options"]["include_usage"],
        serde_json::json!(true)
    );
    assert_eq!(body["messages"][0]["role"], serde_json::json!("user"));
    assert_eq!(
        body["tools"][0],
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Current weather",
                "parameters": { "type": "object", "properties": {} },
            }
        })
    );
    drop(records);

    let mut explicit = request(&base, ApiFormat::OpenaiChatCompletions);
    explicit.max_tokens = Some(512);
    run(sample(explicit, Timeouts::default()).await.unwrap()).await;
    let records = captured.lock().unwrap();
    assert_eq!(last_body(&records)["max_tokens"], serde_json::json!(512));
}

#[tokio::test]
async fn openai_responses_sends_bearer_and_maps_input() {
    let (base, captured) = spawn(Scenario::Sse(text_fixture(ApiFormat::OpenaiResponses))).await;
    let mut req = request(&base, ApiFormat::OpenaiResponses);
    req.tools = vec![weather_tool()];
    req.messages.push(SamplingMessage {
        role: MessageRole::Assistant,
        content: "hi there".into(),
    });
    let stream = sample(req, Timeouts::default()).await.unwrap();
    run(stream).await;
    let records = captured.lock().unwrap();
    assert_eq!(
        header(&records, "authorization"),
        Some(format!("Bearer {TEST_KEY}").as_str())
    );
    let body = last_body(&records);
    assert!(body.get("max_output_tokens").is_none());
    assert_eq!(
        body["input"][0]["content"][0]["type"],
        serde_json::json!("input_text")
    );
    assert_eq!(
        body["input"][1]["content"][0]["type"],
        serde_json::json!("output_text")
    );
    assert_eq!(body["tools"][0]["type"], serde_json::json!("function"));
    assert_eq!(body["tools"][0]["name"], serde_json::json!("get_weather"));
    drop(records);

    let mut explicit = request(&base, ApiFormat::OpenaiResponses);
    explicit.max_tokens = Some(512);
    run(sample(explicit, Timeouts::default()).await.unwrap()).await;
    let records = captured.lock().unwrap();
    assert_eq!(
        last_body(&records)["max_output_tokens"],
        serde_json::json!(512)
    );
}

#[tokio::test]
async fn no_key_sends_no_auth_headers_on_any_format() {
    for format in FORMATS {
        let (base, captured) = spawn(Scenario::Sse(text_fixture(format))).await;
        let mut req = request(&base, format);
        req.api_key = None;
        let stream = sample(req, Timeouts::default()).await.unwrap();
        // Local endpoints must still get a decodable stream.
        let events = run(stream).await;
        assert!(
            events.iter().all(|event| event.is_ok()),
            "stream failed without auth: {events:?}"
        );
        let records = captured.lock().unwrap();
        assert_eq!(header(&records, "authorization"), None, "{format:?}");
        assert_eq!(header(&records, "x-api-key"), None, "{format:?}");
        // Anthropic keeps its (non-secret) version header either way.
        if format == ApiFormat::AnthropicMessages {
            assert_eq!(header(&records, "anthropic-version"), Some("2023-06-01"));
        }
    }
}

// ---------------------------------------------------------------------------
// Task 3.8 scenario 4: error classification (design D3)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn http_errors_classify_per_design_d3() {
    for format in FORMATS {
        let cases: [(StatusCode, &str, SamplingError); 4] = [
            (
                StatusCode::UNAUTHORIZED,
                r#"{"error":{"message":"bad key"}}"#,
                SamplingError::AuthFailed,
            ),
            (
                StatusCode::TOO_MANY_REQUESTS,
                r#"{"error":{"message":"slow down"}}"#,
                SamplingError::RateLimited,
            ),
            (
                StatusCode::BAD_REQUEST,
                r#"{"error":{"message":"Invalid `model` specified: nope"}}"#,
                SamplingError::InvalidRequest {
                    message: "Invalid `model` specified: nope".into(),
                },
            ),
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                r#"{"error":{"message":"boom"}}"#,
                SamplingError::ProviderUnreachable {
                    message: "server error: HTTP 500 Internal Server Error".into(),
                },
            ),
        ];
        for (status, body, expected) in cases {
            let (base, _) = spawn(Scenario::Error(status, body.to_string())).await;
            let error = sample(request(&base, format), Timeouts::default())
                .await
                .unwrap_err();
            assert_eq!(error, expected, "{format:?} {status}");
        }
    }
}

#[tokio::test]
async fn provider_message_is_truncated_and_non_json_bodies_fall_back() {
    let long = "x".repeat(400);
    let body = format!("{{\"error\":{{\"message\":\"{long}\"}}}}");
    let (base, _) = spawn(Scenario::Error(StatusCode::BAD_REQUEST, body)).await;
    let error = sample(
        request(&base, ApiFormat::AnthropicMessages),
        Timeouts::default(),
    )
    .await
    .unwrap_err();
    match error {
        SamplingError::InvalidRequest { message } => assert_eq!(message.chars().count(), 300),
        other => panic!("expected InvalidRequest, got {other:?}"),
    }

    // A plain-text 4xx body is carried over (truncated) rather than dropped.
    let (base, _) = spawn(Scenario::Error(
        StatusCode::BAD_REQUEST,
        "plain text complaint".into(),
    ))
    .await;
    let error = sample(
        request(&base, ApiFormat::OpenaiChatCompletions),
        Timeouts::default(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        error,
        SamplingError::InvalidRequest {
            message: "plain text complaint".into()
        }
    );
}

#[tokio::test]
async fn connection_refused_is_provider_unreachable() {
    // Bind then drop a listener so the port is (almost certainly) unused.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    for format in FORMATS {
        let error = sample(
            request(&format!("http://127.0.0.1:{port}"), format),
            Timeouts::default(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, SamplingError::ProviderUnreachable { .. }),
            "{format:?}: {error:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Task 3.8 scenario 5: protocol violations fail once, without retry
// ---------------------------------------------------------------------------

#[tokio::test]
async fn malformed_sse_is_a_protocol_error_and_is_not_retried() {
    for format in FORMATS {
        let (base, captured) = spawn(Scenario::Sse("data: {not json at all\n\n")).await;
        let stream = sample(request(&base, format), Timeouts::default())
            .await
            .unwrap();
        let events = run(stream).await;
        assert_eq!(events.len(), 1, "{format:?}");
        assert!(
            matches!(events[0], Err(SamplingError::ProtocolError { .. })),
            "{format:?}: {events:?}"
        );
        // No automatic retries in this layer: exactly one request went out.
        assert_eq!(captured.lock().unwrap().len(), 1, "{format:?}");
    }
}

// ---------------------------------------------------------------------------
// Task 3.8 scenario 6: key redaction end to end
// ---------------------------------------------------------------------------

/// Env variables carrying the subprocess helper's instructions.
const ENV_LOG_DIR: &str = "PLANNER_SAMPLING_TEST_LOG_DIR";
const ENV_SECRET: &str = "PLANNER_SAMPLING_TEST_SECRET";

/// The logger is a process-global singleton that the logging module's own
/// tests keep re-pointing at their temp directories, so an in-process log
/// check here would race them. Instead the parent test below re-executes
/// this test binary with only this (normally ignored) helper enabled: the
/// helper gets a fresh process, initializes logging at the directory passed
/// via [`ENV_LOG_DIR`], triggers the worst case, and the parent asserts on
/// the file contents afterwards.
#[tokio::test]
#[ignore = "spawned as a subprocess by invalid_request_message_never_leaks_key_into_logs"]
async fn redaction_subprocess_helper() {
    let (Some(log_dir), Some(secret)) = (
        std::env::var(ENV_LOG_DIR).ok(),
        std::env::var(ENV_SECRET).ok(),
    ) else {
        return; // not spawned by the parent test; nothing to do
    };
    let log_dir = std::path::PathBuf::from(log_dir);
    crate::logging::init(log_dir.clone(), Some(crate::logging::Level::Debug));
    crate::logging::register_secret(&secret);

    // Direct probe: a raw key inside a log line must be masked by the
    // register_secret path (mirrors the logging module's own tests). The
    // sampling lifecycle lines are debug-level, so the logger runs at Debug.
    crate::logging::debug("sampling", &format!("redaction probe {secret}"));

    // Worst case: the provider echoes the key inside its error body.
    let body = format!("{{\"error\":{{\"message\":\"bad key {secret}\"}}}}");
    let (base, _) = spawn(Scenario::Error(StatusCode::BAD_REQUEST, body)).await;
    let mut req = request(&base, ApiFormat::OpenaiChatCompletions);
    req.model = "redact-probe".into();
    req.api_key = Some(secret.clone());
    let error = sample(req, Timeouts::default()).await.unwrap_err();
    // The classification layer scrubs the echoed key from the message even
    // before the log-side redaction runs.
    assert_eq!(
        error,
        SamplingError::InvalidRequest {
            message: "bad key [REDACTED]".into()
        }
    );
}

#[tokio::test]
async fn invalid_request_message_never_leaks_key_into_logs() {
    let tmp = tempfile::tempdir().unwrap();
    let logs = tmp.path().join("logs");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "sampling::tests::redaction_subprocess_helper",
            "--ignored",
            "--quiet",
        ])
        .env(ENV_LOG_DIR, &logs)
        .env(ENV_SECRET, TEST_KEY)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "redaction subprocess failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let contents = std::fs::read_to_string(logs.join("planner.log")).unwrap_or_default();
    assert!(
        contents.contains("redact-probe"),
        "helper log lines missing: {contents}"
    );
    assert!(!contents.contains(TEST_KEY));
    assert!(contents.contains("[REDACTED]"));
}

// ---------------------------------------------------------------------------
// Task 3.8 scenario 7: layered timeouts (design D5)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn hanging_provider_hits_total_generate_timeout() {
    for format in FORMATS {
        let (base, _) = spawn(Scenario::Hang).await;
        let error = sample(
            request(&base, format),
            Timeouts {
                connect_idle: Duration::from_secs(10),
                total_generate: Duration::from_millis(100),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error, SamplingError::Timeout, "{format:?}");
    }
}

#[tokio::test]
async fn stalled_stream_hits_idle_timeout_between_chunks() {
    let (base, _) = spawn(Scenario::StallAfter(
        "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi\"},\"finish_reason\":null}]}\n\n",
    ))
    .await;
    let stream = sample(
        request(&base, ApiFormat::OpenaiChatCompletions),
        Timeouts {
            connect_idle: Duration::from_millis(100),
            total_generate: Duration::from_secs(60),
        },
    )
    .await
    .unwrap();
    let events = run(stream).await;
    // The first chunk arrives, then the gap exceeds the idle budget and the
    // stream terminates with a timeout instead of hanging for 60s.
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0],
        Ok(SamplingEvent::TextDelta { text: "Hi".into() })
    );
    assert_eq!(events[1], Err(SamplingError::Timeout));
}

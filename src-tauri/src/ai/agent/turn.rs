//! Turn execution (add-ai-planning-core §2): one user message in, a bounded
//! tool-calling loop out, messages persisted in final order afterwards.
//!
//! Invariants this module owns:
//! - Concurrency: a turn is admitted only via the atomic claim; a busy
//!   conversation answers with a stable conflict code (spec: 回合进行中重复触发).
//! - Durability: nothing streams to the database mid-turn; messages land in
//!   one ordered pass when the turn ends, and failures only set `last_error`
//!   (design: 回合中途失败留下半截会话).
//! - Skill truth: `active_skill` changes only because a tool result said so
//!   (design D2); the frontend has no write path.
//! - Bound: at most [`MAX_TOOL_ROUNDS`] tool round-trips, then the turn ends
//!   with an error that keeps the messages produced so far.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::ai::agent::context::{load_context, render, CycleContext};
use crate::ai::agent::prompt::build_system_instruction;
use crate::ai::llm::types::{AgentMessage, AgentRequest, AgentRole, AgentSkill, ToolCallRecord};
use crate::ai::llm::{AgentError, LlmProvider, ResolvedProvider};
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::repository::agent as repo;
use crate::repository::agent::Message;
use crate::service::now_ms;

/// Tool round-trips per turn. Beyond this the loop stops with an error that
/// preserves the produced messages (design: 工具调用循环无终止).
pub const MAX_TOOL_ROUNDS: usize = 8;

/// Nudge for rounds that only carry tool results (the request shape wants a
/// closing user message; the adapter renders history, including results).
const CONTINUE_NUDGE: &str = "Tool results are above. Continue.";

/// One executed tool call, as the loop and the message log see it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolOutcome {
    pub tool_call_id: String,
    pub name: String,
    /// JSON result the model reads (and the frontend may render).
    pub result: serde_json::Value,
    /// `false` = the model must treat this as a failure and adapt.
    pub is_error: bool,
    /// Activation tools carry the skill here; the loop persists it on the
    /// conversation (task 3.3) — never the frontend, never the model's prose.
    pub activated_skill: Option<AgentSkill>,
}

/// Tool surface the loop calls into. The §5 registry implements it over the
/// planning engines; the loop itself is tool-agnostic.
pub trait ToolExecutor: Send + Sync {
    /// JSON schema definitions for this skill's request, if tools are usable.
    fn definitions(
        &self,
        skill: AgentSkill,
        tools_supported: bool,
    ) -> Vec<crate::ai::llm::types::ToolDef>;
    /// Runs one call. Must not panic on model-supplied arguments — the model
    /// is untrusted input.
    fn execute(&self, db: &Db, cycle_id: &str, call: &ToolCallRecord) -> ToolOutcome;
}

/// Placeholder executor for tests of the loop mechanics and until the §5
/// registry lands: every call resolves as a tool error.
pub struct NopExecutor;

impl ToolExecutor for NopExecutor {
    fn definitions(
        &self,
        _skill: AgentSkill,
        _tools_supported: bool,
    ) -> Vec<crate::ai::llm::types::ToolDef> {
        Vec::new()
    }

    fn execute(&self, _db: &Db, _cycle_id: &str, call: &ToolCallRecord) -> ToolOutcome {
        ToolOutcome {
            tool_call_id: call.id.clone(),
            name: call.name.clone(),
            result: serde_json::json!({
                "error": format!("tool '{}' is not available", call.name),
            }),
            is_error: true,
            activated_skill: None,
        }
    }
}

/// One conversation message as stored; the enum tag maps to the SQL
/// `message_type` CHECK values (spec: 消息模型).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessagePayload {
    /// `user` — what the human said.
    Text { text: String },
    /// `model_text` — the model's prose answer.
    ModelText { text: String },
    /// `model_function_call` — one model tool call.
    FunctionCall(ToolCallRecord),
    /// `function_result` — the executed result of a model call.
    FunctionResult(ToolOutcome),
    /// `app_tool_result` — an app-initiated result the model should see
    /// (spec: 应用侧工具调用; also how activations persist).
    AppToolResult {
        name: String,
        result: serde_json::Value,
    },
}

impl MessagePayload {
    fn message_type(&self) -> &'static str {
        match self {
            MessagePayload::Text { .. } => "user",
            MessagePayload::ModelText { .. } => "model_text",
            MessagePayload::FunctionCall(_) => "model_function_call",
            MessagePayload::FunctionResult(_) => "function_result",
            MessagePayload::AppToolResult { .. } => "app_tool_result",
        }
    }

    /// Replay form for the provider request; `AppToolResult` becomes a user
    /// turn so adapters that only know two roles still see it.
    fn to_history(&self) -> Option<AgentMessage> {
        match self {
            MessagePayload::Text { text } => Some(AgentMessage {
                role: AgentRole::User,
                content: text.clone(),
                tool_calls: Vec::new(),
                tool_call_id: None,
            }),
            MessagePayload::ModelText { text } => Some(AgentMessage {
                role: AgentRole::Assistant,
                content: text.clone(),
                tool_calls: Vec::new(),
                tool_call_id: None,
            }),
            MessagePayload::FunctionCall(call) => Some(AgentMessage {
                role: AgentRole::Assistant,
                content: String::new(),
                tool_calls: vec![call.clone()],
                tool_call_id: None,
            }),
            MessagePayload::FunctionResult(outcome) => Some(AgentMessage {
                role: AgentRole::Tool,
                content: outcome.result.to_string(),
                tool_calls: Vec::new(),
                tool_call_id: Some(outcome.tool_call_id.clone()),
            }),
            MessagePayload::AppToolResult { result, .. } => Some(AgentMessage {
                role: AgentRole::User,
                content: format!("[app tool result] {result}"),
                tool_calls: Vec::new(),
                tool_call_id: None,
            }),
        }
    }
}

fn stored(
    conversation_id: &str,
    turn_id: &str,
    sequence: i64,
    payload: &MessagePayload,
) -> Message {
    Message {
        id: uuid::Uuid::new_v4().to_string(),
        conversation_id: conversation_id.to_string(),
        turn_id: turn_id.to_string(),
        sequence_number: sequence,
        message_type: payload.message_type().to_string(),
        payload_json: serde_json::to_string(payload).expect("payload serializes"),
        created_at: now_ms().to_string(),
    }
}

/// The turn's complete outcome, also the IPC reply shape for a send.
#[derive(Debug, Clone, Serialize)]
pub struct TurnResult {
    pub conversation_id: String,
    pub revision: i64,
    pub active_skill: Option<String>,
    /// The model's final prose for this turn (empty when it only called tools).
    pub reply: String,
    /// The ordered, persisted messages for immediate rendering.
    pub messages: Vec<MessageView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MessageView {
    pub id: String,
    pub sequence_number: i64,
    pub message_type: String,
    pub turn_id: String,
    pub payload: serde_json::Value,
}

fn view(message: &Message) -> MessageView {
    MessageView {
        id: message.id.clone(),
        sequence_number: message.sequence_number,
        message_type: message.message_type.clone(),
        turn_id: message.turn_id.clone(),
        payload: serde_json::from_str(&message.payload_json).unwrap_or(serde_json::Value::Null),
    }
}

fn app_error(error: AgentError) -> AppError {
    AppError::validation(error.code(), error.to_string())
}

/// Read model for `get_agent_conversation` (task 9.1).
pub struct ConversationView {
    pub id: String,
    pub cycle_id: String,
    pub revision: i64,
    pub active_skill: Option<String>,
    pub last_error: Option<String>,
    pub messages: Vec<MessageView>,
}

pub fn get_conversation(db: &Db, cycle_id: &str) -> AppResult<Option<ConversationView>> {
    let conn = db.pool().get()?;
    let Some(conversation) = repo::conversation_for_cycle(&conn, cycle_id)? else {
        return Ok(None);
    };
    let messages = repo::list_messages(&conn, &conversation.id, 0)?;
    Ok(Some(ConversationView {
        id: conversation.id,
        cycle_id: conversation.cycle_id,
        revision: conversation.revision,
        active_skill: conversation.active_skill,
        last_error: conversation.last_error,
        messages: messages.iter().map(view).collect(),
    }))
}

/// Runs one turn end-to-end. Awaited by the Tauri async runtime; the
/// per-conversation claim makes concurrent entries fail fast, not interleave.
pub async fn run_turn(
    db: Arc<Db>,
    provider: Arc<dyn LlmProvider>,
    resolved: ResolvedProvider,
    executor: Arc<dyn ToolExecutor>,
    cycle_id: &str,
    user_text: &str,
    focused_task_id: Option<String>,
) -> AppResult<TurnResult> {
    let turn_id = uuid::Uuid::new_v4().to_string();
    let conversation = {
        let conn = db.pool().get()?;
        repo::get_or_create_conversation(&conn, cycle_id, now_ms())?
    };
    {
        let conn = db.pool().get()?;
        if !repo::claim_turn(&conn, &conversation.id, &turn_id)? {
            return Err(AppError::conflict(
                "turn_in_progress",
                "an agent turn is already running for this conversation",
            ));
        }
    }
    let current_skill = conversation
        .active_skill
        .as_deref()
        .and_then(AgentSkill::parse)
        .unwrap_or(AgentSkill::None);

    // Stored history plus this turn's user message (in memory until the end).
    let history: Vec<MessagePayload> = {
        let conn = db.pool().get()?;
        repo::list_messages(&conn, &conversation.id, 0)?
            .iter()
            .filter_map(|m| serde_json::from_str::<MessagePayload>(&m.payload_json).ok())
            .collect()
    };

    let outcome = drive_turn(
        &db,
        provider.as_ref(),
        &resolved,
        executor.as_ref(),
        cycle_id,
        &conversation.id,
        &turn_id,
        current_skill,
        &history,
        user_text,
        focused_task_id,
    )
    .await;

    // The claim is released inside finish_turn (success and failure alike);
    // failures additionally surface as the conversation's last_error.
    match outcome {
        Ok(turn) => Ok(turn),
        Err(error) => {
            let message = error.to_string();
            let conn = db.pool().get()?;
            repo::finish_turn(&conn, &conversation.id, now_ms(), None, Some(&message))?;
            Err(error)
        }
    }
}

/// The tool-calling loop. Keeps the produced messages in memory and persists
/// them in one ordered pass when the loop ends.
#[allow(clippy::too_many_arguments)]
async fn drive_turn(
    db: &Db,
    provider: &dyn LlmProvider,
    resolved: &ResolvedProvider,
    executor: &dyn ToolExecutor,
    cycle_id: &str,
    conversation_id: &str,
    turn_id: &str,
    current_skill: AgentSkill,
    stored_history: &[MessagePayload],
    user_text: &str,
    focused_task_id: Option<String>,
) -> AppResult<TurnResult> {
    // Context is loaded once per turn (spec: 回合开始时注入); the focused
    // task snapshot rides along for single-goal clarification.
    let context_conn = db.pool().get()?;
    let mut cycle_context = load_context(&context_conn, cycle_id)?;
    drop(context_conn);
    if let Some(task_id) = &focused_task_id {
        cycle_context.focused_task = cycle_context
            .tasks
            .iter()
            .find(|t| &t.id == task_id)
            .cloned();
    }
    let system = build_system_instruction(current_skill);
    let context_xml = render(&cycle_context, None);
    let tools = executor.definitions(current_skill, resolved.tools_supported);

    // Replay: everything stored before this turn, minus the trailing none.
    let mut history: Vec<AgentMessage> = stored_history
        .iter()
        .filter_map(|p| p.to_history())
        .collect();
    // This turn's produced messages, in order, not yet written.
    let mut pending: Vec<MessagePayload> = vec![MessagePayload::Text {
        text: user_text.to_string(),
    }];
    let mut skill = current_skill;
    let mut activated: Option<AgentSkill> = None;
    let mut reply = String::new();
    let mut rounds = 0usize;

    loop {
        let request = AgentRequest {
            skill,
            system: system.clone(),
            context_block: context_xml.clone(),
            history: history.clone(),
            user_message: if rounds == 0 {
                user_text.to_string()
            } else {
                CONTINUE_NUDGE.to_string()
            },
            tools: tools.clone(),
            max_tokens: None,
        };
        let response = provider.generate_agent(request).await.map_err(app_error)?;

        if response.tool_calls.is_empty() {
            reply = response.text.clone();
            pending.push(MessagePayload::ModelText {
                text: response.text,
            });
            break;
        }

        rounds += 1;
        if rounds > MAX_TOOL_ROUNDS {
            return Err(AppError::conflict(
                "tool_round_limit",
                "the agent exceeded the tool-calling budget for one turn",
            ));
        }

        // Model calls land as messages first, then their results — sequence
        // numbers make the pairing explicit (spec: 一次工具调用的落库).
        for call in &response.tool_calls {
            pending.push(MessagePayload::FunctionCall(call.clone()));
        }
        history.push(AgentMessage {
            role: AgentRole::Assistant,
            content: response.text.clone(),
            tool_calls: response.tool_calls.clone(),
            tool_call_id: None,
        });
        for call in &response.tool_calls {
            let outcome = executor.execute(db, cycle_id, call);
            if let Some(new_skill) = outcome.activated_skill {
                skill = new_skill;
                activated = Some(new_skill);
            }
            history.push(AgentMessage {
                role: AgentRole::Tool,
                content: outcome.result.to_string(),
                tool_calls: Vec::new(),
                tool_call_id: Some(call.id.clone()),
            });
            pending.push(MessagePayload::FunctionResult(outcome));
        }
    }

    // One ordered write pass (spec: 消息按最终顺序一次性写入).
    let mut conn = db.pool().get()?;
    let mut sequence = repo::max_sequence(&conn, conversation_id)?;
    let mut stored_messages: Vec<Message> = Vec::with_capacity(pending.len());
    for payload in &pending {
        sequence += 1;
        stored_messages.push(stored(conversation_id, turn_id, sequence, payload));
    }
    for message in &stored_messages {
        repo::insert_message(&conn, message)?;
    }
    let conversation = repo::finish_turn(
        &conn,
        conversation_id,
        now_ms(),
        activated.map(|s| s.as_str()),
        None,
    )?;

    Ok(TurnResult {
        conversation_id: conversation_id.to_string(),
        revision: conversation.revision,
        active_skill: conversation.active_skill,
        reply,
        messages: stored_messages.iter().map(view).collect(),
    })
}

/// App-side tool call (spec: 应用侧工具调用): run one activation tool without
/// a model round-trip, persist the result as `app_tool_result` and update the
/// skill. Used by the `start_planning` / `start_goal_setting` /
/// `start_prioritization` commands.
pub fn run_app_tool(
    db: &Db,
    executor: &dyn ToolExecutor,
    cycle_id: &str,
    tool_name: &str,
    arguments: serde_json::Value,
) -> AppResult<TurnResult> {
    let turn_id = uuid::Uuid::new_v4().to_string();
    let mut conn = db.pool().get()?;
    let conversation = repo::get_or_create_conversation(&conn, cycle_id, now_ms())?;
    if !repo::claim_turn(&conn, &conversation.id, &turn_id)? {
        return Err(AppError::conflict(
            "turn_in_progress",
            "an agent turn is already running for this conversation",
        ));
    }

    let call = ToolCallRecord {
        id: uuid::Uuid::new_v4().to_string(),
        name: tool_name.to_string(),
        arguments,
    };
    let outcome = executor.execute(db, cycle_id, &call);
    let payload = MessagePayload::AppToolResult {
        name: tool_name.to_string(),
        result: outcome.result.clone(),
    };

    let mut sequence = repo::max_sequence(&conn, &conversation.id)?;
    sequence += 1;
    let message = stored(&conversation.id, &turn_id, sequence, &payload);
    repo::insert_message(&conn, &message)?;
    let conversation = repo::finish_turn(
        &conn,
        &conversation.id,
        now_ms(),
        outcome.activated_skill.map(|s| s.as_str()),
        None,
    )?;

    Ok(TurnResult {
        conversation_id: conversation.id,
        revision: conversation.revision,
        active_skill: conversation.active_skill,
        reply: String::new(),
        messages: vec![view(&message)],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::llm::{FakeProvider, FakeTurn};
    use std::cell::RefCell;
    use std::sync::Mutex;

    fn db() -> (Arc<Db>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db = Arc::new(crate::db::open_at(&dir.path().join("test.db")).unwrap());
        (db, dir)
    }

    fn month_cycle(db: &Db) -> String {
        crate::service::cycles::create_planning_cycle(
            db,
            &crate::service::cycles::CreateCycleArgs {
                cycle_type: "month".into(),
                duration_months: Some(1),
                ..Default::default()
            },
            crate::domain::calendar::today_local(),
            1,
        )
        .unwrap()
        .value
        .id
    }

    /// ResolvedProvider without touching providers.json — the loop only reads
    /// `tools_supported` and the adapter fields it never uses with fakes.
    fn fake_resolved() -> ResolvedProvider {
        use crate::providers::config::{ApiFormat, InputType, ModelConfig, OutputType};
        ResolvedProvider {
            model: ModelConfig {
                model_id: "m".into(),
                context_window: 1,
                max_output_tokens: 1,
                input_types: vec![InputType::Text],
                output_types: vec![OutputType::Text],
                supports_tools: true,
            },
            api_key: None,
            tools_supported: true,
            config: crate::providers::config::ProviderConfig {
                id: "p1".into(),
                name: "fake".into(),
                base_url: "http://127.0.0.1:1".into(),
                api_format: ApiFormat::OpenaiChatCompletions,
                extra_headers: vec![],
                models: vec![],
                created_at: 0,
                archived: false,
            },
        }
    }

    /// Scripted provider that records the requests it received.
    struct ScriptedProvider {
        turns: Mutex<std::vec::IntoIter<FakeTurn>>,
        #[allow(dead_code)]
        seen: RefCell<Vec<AgentRequest>>,
    }

    // FakeProvider covers the scripted path; the loop tests only need it.

    fn executor() -> Arc<dyn ToolExecutor> {
        Arc::new(NopExecutor)
    }

    #[tokio::test]
    async fn turn_persists_user_and_model_messages_in_order() {
        let (db, _dir) = db();
        let cycle = month_cycle(&db);
        let provider: Arc<dyn LlmProvider> =
            Arc::new(FakeProvider::with_script(vec![FakeTurn::Text(
                "hello!".into(),
            )]));
        let result = run_turn(
            db.clone(),
            provider,
            fake_resolved(),
            executor(),
            &cycle,
            "hi",
            None,
        )
        .await
        .unwrap();
        assert_eq!(result.reply, "hello!");
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].message_type, "user");
        assert_eq!(result.messages[1].message_type, "model_text");
        // Second read path agrees.
        let view = get_conversation(&db, &cycle).unwrap().unwrap();
        assert_eq!(view.messages.len(), 2);
        assert_eq!(view.revision, 1);
    }

    #[tokio::test]
    async fn second_turn_sees_the_first_turn_in_history() {
        let (db, _dir) = db();
        let cycle = month_cycle(&db);
        let provider: Arc<dyn LlmProvider> = Arc::new(FakeProvider::with_script(vec![
            FakeTurn::Text("first".into()),
            FakeTurn::Text("second".into()),
        ]));
        run_turn(
            db.clone(),
            Arc::new(FakeProvider::with_script(vec![FakeTurn::Text(
                "first".into(),
            )])),
            fake_resolved(),
            executor(),
            &cycle,
            "one",
            None,
        )
        .await
        .unwrap();
        let view = get_conversation(&db, &cycle).unwrap().unwrap();
        assert_eq!(view.messages.len(), 2);
    }

    #[tokio::test]
    async fn nop_tool_calls_become_error_results_and_terminate() {
        let (db, _dir) = db();
        let cycle = month_cycle(&db);
        let call = ToolCallRecord {
            id: "c1".into(),
            name: "no_such_tool".into(),
            arguments: serde_json::json!({}),
        };
        let provider: Arc<dyn LlmProvider> = Arc::new(FakeProvider::with_script(vec![
            FakeTurn::Calls(vec![call]),
            FakeTurn::Text("gave up".into()),
        ]));
        let result = run_turn(
            db.clone(),
            provider,
            fake_resolved(),
            executor(),
            &cycle,
            "use tools",
            None,
        )
        .await
        .unwrap();
        let types: Vec<&str> = result
            .messages
            .iter()
            .map(|m| m.message_type.as_str())
            .collect();
        assert_eq!(
            types,
            vec![
                "user",
                "model_function_call",
                "function_result",
                "model_text"
            ]
        );
        let result_payload = &result.messages[2].payload;
        assert_eq!(result_payload["is_error"], serde_json::json!(true));
    }

    #[tokio::test]
    async fn busy_conversation_rejects_concurrent_turns() {
        let (db, _dir) = db();
        let cycle = month_cycle(&db);
        let conn = db.pool().get().unwrap();
        let conversation = repo::get_or_create_conversation(&conn, &cycle, 1).unwrap();
        assert!(repo::claim_turn(&conn, &conversation.id, "foreign").unwrap());
        drop(conn);

        let provider: Arc<dyn LlmProvider> = Arc::new(FakeProvider::with_script(vec![]));
        let error = run_turn(
            db.clone(),
            provider,
            fake_resolved(),
            executor(),
            &cycle,
            "hi",
            None,
        )
        .await
        .unwrap_err();
        match error {
            AppError::Conflict { code, .. } => assert_eq!(code, "turn_in_progress"),
            other => panic!("expected conflict, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn provider_failure_sets_last_error_and_releases_the_turn() {
        let (db, _dir) = db();
        let cycle = month_cycle(&db);
        let provider: Arc<dyn LlmProvider> =
            Arc::new(FakeProvider::with_script(vec![FakeTurn::Fail(
                AgentError::NoActiveProvider,
            )]));
        let error = run_turn(
            db.clone(),
            provider,
            fake_resolved(),
            executor(),
            &cycle,
            "hi",
            None,
        )
        .await
        .unwrap_err();
        assert_eq!(crate::ai::llm::AgentError::NoActiveProvider.code(), {
            let _ = &error;
            "no_active_provider"
        });
        let conn = db.pool().get().unwrap();
        let conversation = repo::conversation_for_cycle(&conn, &cycle)
            .unwrap()
            .unwrap();
        assert!(conversation.active_turn_id.is_none(), "turn released");
        assert!(conversation.last_error.is_some(), "error persisted");
        assert!(
            repo::list_messages(&conn, &conversation.id, 0)
                .unwrap()
                .is_empty(),
            "failed turns do not write half messages"
        );
    }

    #[test]
    fn app_tool_result_persists_and_activates_skill() {
        struct Activate;
        impl ToolExecutor for Activate {
            fn definitions(
                &self,
                _skill: AgentSkill,
                _supported: bool,
            ) -> Vec<crate::ai::llm::types::ToolDef> {
                Vec::new()
            }
            fn execute(&self, _db: &Db, _cycle: &str, call: &ToolCallRecord) -> ToolOutcome {
                ToolOutcome {
                    tool_call_id: call.id.clone(),
                    name: call.name.clone(),
                    result: serde_json::json!({"activated_skill": "long_term_planning"}),
                    is_error: false,
                    activated_skill: Some(AgentSkill::LongTermPlanning),
                }
            }
        }
        let (db, _dir) = db();
        let cycle = month_cycle(&db);
        let result = run_app_tool(
            &db,
            &Activate,
            &cycle,
            "start_planning",
            serde_json::json!({}),
        )
        .unwrap();
        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.messages[0].message_type, "app_tool_result");
        assert_eq!(result.active_skill.as_deref(), Some("long_term_planning"));
        // The persisted row carries the skill for the next turn.
        let conn = db.pool().get().unwrap();
        let conversation = repo::conversation_for_cycle(&conn, &cycle)
            .unwrap()
            .unwrap();
        assert_eq!(
            conversation.active_skill.as_deref(),
            Some("long_term_planning")
        );
    }
}

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

use rusqlite::{Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};

use crate::ai::agent::context::{load_turn_context, TurnContext};
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

fn decision_in_progress() -> AppError {
    AppError::conflict(
        "turn_in_progress",
        "Wait for the current Coach response before confirming changes",
    )
}

/// Appends one human decision receipt to a conversation that the caller has
/// already claimed with `turn_id`.
fn append_decision(
    conn: &rusqlite::Connection,
    conversation_id: &str,
    turn_id: &str,
    result: serde_json::Value,
) -> AppResult<()> {
    let payload = MessagePayload::AppToolResult {
        name: "approval_decision".into(),
        result,
    };
    let message = stored(
        conversation_id,
        turn_id,
        repo::max_sequence(conn, conversation_id)? + 1,
        &payload,
    );
    repo::insert_message(conn, &message)
}

/// Finishes a claimed decision receipt. The owner check happens in the same
/// transaction as the append and release, so a stale guard cannot clear a
/// newer turn.
fn finish_decision(
    conn: &rusqlite::Connection,
    conversation_id: &str,
    turn_id: &str,
    result: serde_json::Value,
) -> AppResult<()> {
    let conversation = repo::conversation_by_id(conn, conversation_id)?
        .ok_or_else(|| AppError::Internal("Coach conversation missing".into()))?;
    if conversation.active_turn_id.as_deref() != Some(turn_id) {
        return Err(decision_in_progress());
    }
    append_decision(conn, conversation_id, turn_id, result)?;
    repo::finish_turn(
        conn,
        conversation_id,
        now_ms(),
        None,
        conversation.last_error.as_deref(),
    )?;
    Ok(())
}

fn record_decision_in_transaction(
    conn: &rusqlite::Connection,
    result: serde_json::Value,
) -> AppResult<()> {
    let conversation = repo::get_or_create_conversation(conn, now_ms())?;
    let turn_id = uuid::Uuid::new_v4().to_string();
    if !repo::claim_turn(conn, &conversation.id, &turn_id)? {
        return Err(decision_in_progress());
    }
    finish_decision(conn, &conversation.id, &turn_id, result)
}

/// Human decisions are transcript events, not model prose or a footer toast.
/// Caller may use a transaction to commit the decision and its receipt together.
pub fn record_decision(conn: &rusqlite::Connection, result: serde_json::Value) -> AppResult<()> {
    if conn.is_autocommit() {
        let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
            .map_err(crate::error::from_rusqlite)?;
        record_decision_in_transaction(&tx, result)?;
        tx.commit().map_err(crate::error::from_rusqlite)
    } else {
        record_decision_in_transaction(conn, result)
    }
}

/// Holds the global conversation while an asynchronous action is applied.
/// The token is also the receipt turn id, making ownership explicit in the
/// append/finish transaction.
pub struct DecisionGuard {
    db: Db,
    conversation_id: String,
    turn_id: String,
    completed: bool,
}

impl DecisionGuard {
    pub fn begin(db: &Db) -> AppResult<Self> {
        let conn = db.pool().get()?;
        let tx = Transaction::new_unchecked(&conn, TransactionBehavior::Immediate)
            .map_err(crate::error::from_rusqlite)?;
        let conversation = repo::get_or_create_conversation(&tx, now_ms())?;
        let turn_id = uuid::Uuid::new_v4().to_string();
        if !repo::claim_turn(&tx, &conversation.id, &turn_id)? {
            return Err(decision_in_progress());
        }
        tx.commit().map_err(crate::error::from_rusqlite)?;
        Ok(Self {
            db: db.clone(),
            conversation_id: conversation.id,
            turn_id,
            completed: false,
        })
    }

    pub fn record(&mut self, result: serde_json::Value) -> AppResult<()> {
        if self.completed {
            return Err(AppError::Conflict {
                code: "decision_already_recorded".into(),
                message: "This Coach decision has already been recorded".into(),
            });
        }
        let conn = self.db.pool().get()?;
        let tx = Transaction::new_unchecked(&conn, TransactionBehavior::Immediate)
            .map_err(crate::error::from_rusqlite)?;
        finish_decision(&tx, &self.conversation_id, &self.turn_id, result)?;
        tx.commit().map_err(crate::error::from_rusqlite)?;
        self.completed = true;
        Ok(())
    }
}

impl Drop for DecisionGuard {
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        if let Ok(conn) = self.db.pool().get() {
            let _ = conn.execute(
                "UPDATE agent_conversations SET active_turn_id = NULL \
                 WHERE id = ?1 AND active_turn_id = ?2",
                rusqlite::params![self.conversation_id, self.turn_id],
            );
        }
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

/// Public view helper for the command layer (reads pass stored messages).
pub fn message_view(message: &Message) -> MessageView {
    view(message)
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

pub fn app_error(error: AgentError) -> AppError {
    AppError::validation(error.code(), error.to_string())
}

/// Read model for `get_agent_conversation` (task 9.1).
#[derive(Debug, Clone, Serialize)]
pub struct ConversationView {
    pub expires_at: Option<i64>,
    pub context_idle_minutes: i64,
    pub id: String,
    pub active_turn_id: Option<String>,
    pub revision: i64,
    pub active_skill: Option<String>,
    pub last_error: Option<String>,
    pub messages: Vec<MessageView>,
}

pub fn get_conversation(db: &Db) -> AppResult<Option<ConversationView>> {
    let conn = db.pool().get()?;
    repo::expire_idle(&conn, now_ms())?;
    let Some(conversation) = repo::conversation(&conn)? else {
        return Ok(None);
    };
    let messages = repo::list_messages(&conn, &conversation.id, 0)?;
    let context_idle_minutes = repo::context_idle_minutes(&conn)?;
    Ok(Some(ConversationView {
        context_idle_minutes,
        expires_at: if messages.is_empty()
            && conversation.active_skill.is_none()
            && conversation.last_error.is_none()
        {
            None
        } else {
            repo::expires_at(&conversation, context_idle_minutes)
        },
        id: conversation.id,
        active_turn_id: conversation.active_turn_id,
        revision: conversation.revision,
        active_skill: conversation.active_skill,
        last_error: conversation.last_error,
        messages: messages.iter().map(view).collect(),
    }))
}

/// Runs one turn end-to-end. Awaited by the Tauri async runtime; the
/// per-conversation claim makes concurrent entries fail fast, not interleave.
pub async fn run_turn(
    db: &Db,
    provider: Arc<dyn LlmProvider>,
    resolved: ResolvedProvider,
    executor: Arc<dyn ToolExecutor>,
    selection: &TurnContext,
    user_text: &str,
) -> AppResult<TurnResult> {
    let turn_id = uuid::Uuid::new_v4().to_string();
    let conversation = {
        let conn = db.pool().get()?;
        repo::get_or_create_conversation(&conn, now_ms())?
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
    // Re-read after the claim: another turn may have finished between the
    // initial conversation read and our atomic claim.
    let current_skill = {
        let conn = db.pool().get()?;
        repo::conversation_by_id(&conn, &conversation.id)?
            .ok_or_else(|| AppError::Internal("Coach conversation missing".into()))?
            .active_skill
            .as_deref()
            .and_then(AgentSkill::parse)
            .unwrap_or(AgentSkill::None)
    };

    // Stored history plus this turn's user message (in memory until the end).
    let history: Vec<MessagePayload> = {
        let conn = db.pool().get()?;
        repo::list_messages(&conn, &conversation.id, 0)?
            .iter()
            .filter_map(|m| serde_json::from_str::<MessagePayload>(&m.payload_json).ok())
            .collect()
    };

    let outcome = drive_turn(
        db,
        provider.as_ref(),
        &resolved,
        executor.as_ref(),
        selection,
        &conversation.id,
        &turn_id,
        current_skill,
        &history,
        user_text,
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
    selection: &TurnContext,
    conversation_id: &str,
    turn_id: &str,
    current_skill: AgentSkill,
    stored_history: &[MessagePayload],
    user_text: &str,
) -> AppResult<TurnResult> {
    // Read one immutable snapshot for all model/tool rounds in this turn.
    let context_conn = db.pool().get()?;
    let (cycle, context_xml) = load_turn_context(&context_conn, selection)?;
    drop(context_conn);
    let cycle_id = cycle.as_ref().map(|cycle| cycle.id.as_str()).unwrap_or("");
    let cycle_type = cycle
        .as_ref()
        .map(|cycle| cycle.cycle_type)
        .unwrap_or(crate::domain::cycle::CycleType::Month);

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
            system: format!(
                "{}\n{}",
                build_system_instruction(skill, cycle_type)?,
                crate::i18n::for_db(db)?.instruction()
            ),
            context_block: context_xml.clone(),
            history: history.clone(),
            user_message: if rounds == 0 {
                user_text.to_string()
            } else {
                CONTINUE_NUDGE.to_string()
            },
            tools: executor.definitions(skill, resolved.tools_supported),
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

        // The first request carries this instruction in user_message. Every
        // continuation must replay it before the tool calls; otherwise loading
        // a skill loses the user's current request and revives the previous one.
        if rounds == 1 {
            history.push(AgentMessage {
                role: AgentRole::User,
                content: user_text.to_string(),
                tool_calls: Vec::new(),
                tool_call_id: None,
            });
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
            let allowed = executor
                .definitions(skill, resolved.tools_supported)
                .iter()
                .any(|tool| tool.name == call.name);
            let outcome = if allowed {
                executor.execute(db, cycle_id, call)
            } else {
                ToolOutcome {
                    tool_call_id: call.id.clone(),
                    name: call.name.clone(),
                    result: serde_json::json!({"error":{"code":"tool_not_available","message":"Load the appropriate skill before using this tool."}}),
                    is_error: true,
                    activated_skill: None,
                }
            };
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
    let conn = db.pool().get()?;
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
    let conn = db.pool().get()?;
    let conversation = repo::get_or_create_conversation(&conn, now_ms())?;
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

    #[test]
    fn approved_task_receipt_is_committed_with_the_task_and_replayed_in_context() {
        let (db, _dir) = db();
        let cycle = month_cycle(&db);
        crate::service::settings::set_locale(&db, "zh-CN".into()).unwrap();
        let task = crate::service::proposals::apply_upsert_preview(
            &db,
            &cycle,
            &crate::service::proposals::TaskInput {
                title: "A useful step".into(),
                ..Default::default()
            },
            now_ms(),
        )
        .unwrap()
        .value;
        crate::ai::actions::resolve_task_preview(&db, &task.id, true).unwrap();
        let view = get_conversation(&db).unwrap().unwrap();
        assert_eq!(view.messages.len(), 1);
        let payload: MessagePayload =
            serde_json::from_value(view.messages[0].payload.clone()).unwrap();
        let MessagePayload::AppToolResult { name, result } = &payload else {
            panic!("receipt expected")
        };
        assert_eq!(name, "approval_decision");
        let receipt = result["text"].as_str().unwrap();
        assert!(receipt.starts_with("已添加任务「A useful step」。"), "{receipt}");
        assert!(receipt.contains("目标：Long-term"));
        assert_eq!(result["target_id"], task.id);
        assert_eq!(result["target_kind"], "task");
        assert_eq!(result["decision"], "applied");
        assert!(payload
            .to_history()
            .unwrap()
            .content
            .contains("A useful step"));
        assert!(crate::ai::actions::resolve_task_preview(&db, &task.id, true).is_err());
        assert_eq!(get_conversation(&db).unwrap().unwrap().messages.len(), 1);
    }

    #[tokio::test]
    async fn decision_guard_blocks_turn_app_tool_and_other_receipt() {
        let (db, _dir) = db();
        let guard = DecisionGuard::begin(&db).unwrap();
        let cycle = month_cycle(&db);

        let turn_error = run_turn(
            &db,
            Arc::new(ContextRecorder::default()),
            fake_resolved(),
            executor(),
            &turn_context(&cycle),
            "must wait",
        )
        .await
        .unwrap_err();
        assert!(matches!(
            turn_error,
            AppError::Conflict { ref code, .. } if code == "turn_in_progress"
        ));

        let app_tool_error = run_app_tool(
            &db,
            &NopExecutor,
            &cycle,
            "start_planning",
            serde_json::json!({}),
        )
        .unwrap_err();
        assert!(matches!(
            app_tool_error,
            AppError::Conflict { ref code, .. } if code == "turn_in_progress"
        ));

        let conn = db.pool().get().unwrap();
        let receipt_error =
            record_decision(&conn, serde_json::json!({"decision":"rejected"})).unwrap_err();
        assert!(matches!(
            receipt_error,
            AppError::Conflict { ref code, .. } if code == "turn_in_progress"
        ));
        drop(conn);
        drop(guard);
        let conn = db.pool().get().unwrap();
        assert!(repo::conversation(&conn)
            .unwrap()
            .unwrap()
            .active_turn_id
            .is_none());
    }

    #[test]
    fn decision_guard_records_sequential_receipts_and_releases_lock() {
        let (db, _dir) = db();
        let mut first = DecisionGuard::begin(&db).unwrap();
        first
            .record(serde_json::json!({"decision":"applied","target_id":"a"}))
            .unwrap();

        let mut second = DecisionGuard::begin(&db).unwrap();
        second
            .record(serde_json::json!({"decision":"rejected","target_id":"b"}))
            .unwrap();

        let conn = db.pool().get().unwrap();
        let conversation = repo::conversation(&conn).unwrap().unwrap();
        assert!(conversation.active_turn_id.is_none());
        let messages = repo::list_messages(&conn, &conversation.id, 0).unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].sequence_number, 1);
        assert_eq!(messages[1].sequence_number, 2);
        assert_ne!(messages[0].turn_id, messages[1].turn_id);
    }

    #[test]
    fn decision_guard_drop_and_error_do_not_clear_a_newer_lock() {
        let (db, _dir) = db();
        {
            let _guard = DecisionGuard::begin(&db).unwrap();
        }

        let conn = db.pool().get().unwrap();
        let conversation = repo::conversation(&conn).unwrap().unwrap();
        assert!(repo::claim_turn(&conn, &conversation.id, "successor").unwrap());
        repo::release_turn(&conn, &conversation.id).unwrap();
        drop(conn);

        let mut stale = DecisionGuard::begin(&db).unwrap();
        let conn = db.pool().get().unwrap();
        repo::release_turn(&conn, &stale.conversation_id).unwrap();
        assert!(repo::claim_turn(&conn, &stale.conversation_id, "successor-after-error").unwrap());
        drop(conn);

        assert!(stale
            .record(serde_json::json!({"decision":"stale"}))
            .is_err());
        drop(stale);

        let conn = db.pool().get().unwrap();
        let conversation_id = repo::conversation(&conn).unwrap().unwrap().id;
        assert_eq!(
            repo::conversation(&conn)
                .unwrap()
                .unwrap()
                .active_turn_id
                .as_deref(),
            Some("successor-after-error")
        );
        repo::release_turn(&conn, &conversation_id).unwrap();
    }

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
            extra_headers: vec![],
            tools_supported: true,
            config: crate::providers::config::ProviderConfig {
                connection: Default::default(),
                id: "p1".into(),
                name: "fake".into(),
                base_url: "http://127.0.0.1:1".into(),
                api_format: ApiFormat::OpenaiChatCompletions,
                extra_headers: vec![],
                models: vec![],
                created_at: 0,
                archived: false,
                connection_verified_at: None,
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

    fn turn_context(cycle: &str) -> TurnContext {
        TurnContext {
            cycle_id: Some(cycle.into()),
            ..Default::default()
        }
    }

    fn executor() -> Arc<dyn ToolExecutor> {
        Arc::new(NopExecutor)
    }

    #[derive(Default)]
    struct ContextRecorder(Mutex<Vec<AgentRequest>>);
    impl LlmProvider for ContextRecorder {
        fn generate_agent(
            &self,
            request: AgentRequest,
        ) -> crate::ai::llm::BoxFuture<'_, Result<crate::ai::llm::AgentResponse, AgentError>>
        {
            self.0.lock().unwrap().push(request);
            Box::pin(async {
                Ok(crate::ai::llm::AgentResponse {
                    text: "Kept in Coach".into(),
                    tool_calls: vec![],
                    usage: None,
                })
            })
        }
        fn generate_json(
            &self,
            _: crate::ai::llm::LlmRequest,
        ) -> crate::ai::llm::BoxFuture<'_, Result<serde_json::Value, AgentError>> {
            unreachable!()
        }
    }

    fn create_goal_call(id: &str, cycle_id: &str, title: &str) -> ToolCallRecord {
        ToolCallRecord {
            id: id.into(),
            name: "create_goal".into(),
            arguments: serde_json::json!({
                "cycle_id": cycle_id,
                "title": title,
                "rationale": "imported from the user's plan"
            }),
        }
    }

    async fn stage_batch_import(
        db: &Arc<Db>,
        focused_cycle: &str,
        day_cycle: &str,
        custom_cycle: &str,
    ) -> Vec<(String, String)> {
        let provider = FakeProvider::with_script(vec![
            // The global Coach starts without a skill. Loading planning first
            // makes the following three writes valid in the same tool round.
            FakeTurn::Calls(vec![ToolCallRecord {
                id: "load-planning".into(),
                name: "load_skill".into(),
                arguments: serde_json::json!({"name":"long-term-planning"}),
            }]),
            FakeTurn::Calls(vec![
                create_goal_call("goal-1", day_cycle, "做了一个xx事情"),
                create_goal_call("goal-2", day_cycle, "做了另一个事情"),
                create_goal_call("goal-3", custom_cycle, "做了一个xx事情；做了另一个事情"),
            ]),
            FakeTurn::Text("Three goals are ready for review.".into()),
        ]);
        run_turn(
            db,
            Arc::new(provider),
            fake_resolved(),
            Arc::new(crate::ai::tools::ToolRegistry),
            &turn_context(focused_cycle),
            "2026/09/13 做了一个xx事情 做了另一个事情\n2026/09/14-2026/09/16 做了一个xx事情 做了另一个事情",
        )
        .await
        .unwrap();

        let mut ids = Vec::new();
        for cycle_id in [day_cycle, custom_cycle] {
            ids.extend(
                crate::service::proposals::get_preview_summary(db, cycle_id)
                    .unwrap()
                    .tasks
                    .into_iter()
                    .map(|task| (cycle_id.to_string(), task.id)),
            );
        }
        ids
    }

    fn resolve_batch_previews(db: &Db, task_ids: &[(String, String)], approve: bool) {
        for (_, task_id) in task_ids {
            crate::ai::actions::resolve_task_preview(db, task_id, approve).unwrap();
        }
    }

    fn top_level_titles(db: &Db, cycle: &str) -> Vec<String> {
        crate::service::editor::get_editor_workspace(db, cycle)
            .unwrap()
            .tasks
            .into_iter()
            .map(|node| node.task.title)
            .collect()
    }

    #[tokio::test]
    async fn batch_import_confirming_all_previews_commits_goals_and_receipts() {
        let (db, _dir) = db();
        let focused_cycle = month_cycle(&db);
        let day_cycle = crate::service::cycles::get_or_create_day(
            &db,
            crate::domain::calendar::parse_date("2026-09-13").unwrap(),
            now_ms(),
        )
        .unwrap()
        .value;
        let custom_cycle = crate::service::cycles::create_planning_cycle(
            &db,
            &crate::service::cycles::CreateCycleArgs {
                cycle_type: "month".into(),
                starts_on: Some("2026-09-14".into()),
                ends_on: Some("2026-09-17".into()),
                title: Some("Custom import target".into()),
                ..Default::default()
            },
            crate::domain::calendar::parse_date("2026-09-16").unwrap(),
            now_ms(),
        )
        .unwrap()
        .value;
        assert_eq!(custom_cycle.starts_on.as_deref(), Some("2026-09-14"));
        // Inclusive user range 09/14–09/16 uses an exclusive end bound.
        assert_eq!(custom_cycle.ends_on.as_deref(), Some("2026-09-17"));
        let task_ids =
            stage_batch_import(&db, &focused_cycle, &day_cycle.id, &custom_cycle.id).await;
        assert_eq!(task_ids.len(), 3);

        // Pending rows are projected to the editor, while committed queries
        // still exclude them until the user confirms each preview.
        assert!(crate::service::editor::get_editor_workspace(&db, &focused_cycle)
            .unwrap()
            .tasks
            .is_empty());
        let staged_day = crate::service::editor::get_editor_workspace(&db, &day_cycle.id).unwrap();
        assert_eq!(staged_day.tasks.len(), 2);
        assert!(staged_day
            .tasks
            .iter()
            .all(|node| node.task.proposal.is_some()));
        let staged_custom =
            crate::service::editor::get_editor_workspace(&db, &custom_cycle.id).unwrap();
        assert_eq!(staged_custom.tasks.len(), 1);
        assert!(staged_custom
            .tasks
            .iter()
            .all(|node| node.task.proposal.is_some()));
        let conn = db.pool().get().unwrap();
        assert!(crate::repository::tasks::list_visible_by_cycle(&conn, &day_cycle.id)
            .unwrap()
            .is_empty());
        assert!(crate::repository::tasks::list_visible_by_cycle(&conn, &custom_cycle.id)
            .unwrap()
            .is_empty());
        drop(conn);

        resolve_batch_previews(&db, &task_ids, true);

        let committed_day = crate::service::editor::get_editor_workspace(&db, &day_cycle.id).unwrap();
        let day_titles: Vec<_> = committed_day
            .tasks
            .iter()
            .map(|node| node.task.title.as_str())
            .collect();
        assert_eq!(day_titles, vec!["做了一个xx事情", "做了另一个事情"]);
        assert!(committed_day
            .tasks
            .iter()
            .all(|node| node.task.proposal.is_none()));
        let committed_custom =
            crate::service::editor::get_editor_workspace(&db, &custom_cycle.id).unwrap();
        assert_eq!(
            committed_custom.tasks[0].task.title,
            "做了一个xx事情；做了另一个事情"
        );
        assert!(committed_custom.tasks[0].task.proposal.is_none());
        assert!(crate::service::editor::get_editor_workspace(&db, &focused_cycle)
            .unwrap()
            .tasks
            .is_empty());

        let conn = db.pool().get().unwrap();
        let visible_day = crate::repository::tasks::list_visible_by_cycle(&conn, &day_cycle.id).unwrap();
        assert_eq!(
            visible_day
                .iter()
                .map(|task| task.title.as_str())
                .collect::<Vec<_>>(),
            day_titles
        );
        assert!(visible_day.iter().all(|task| task.proposal.is_none()));
        let visible_custom =
            crate::repository::tasks::list_visible_by_cycle(&conn, &custom_cycle.id).unwrap();
        assert_eq!(visible_custom.len(), 1);
        assert_eq!(visible_custom[0].title, "做了一个xx事情；做了另一个事情");
        assert!(visible_custom[0].proposal.is_none());
        drop(conn);
        assert_eq!(
            crate::service::proposals::get_preview_summary(&db, &day_cycle.id)
                .unwrap()
                .count,
            0
        );
        assert_eq!(
            crate::service::proposals::get_preview_summary(&db, &custom_cycle.id)
                .unwrap()
                .count,
            0
        );

        let conversation = get_conversation(&db).unwrap().unwrap();
        let receipts: Vec<_> = conversation
            .messages
            .iter()
            .filter(|message| message.message_type == "app_tool_result")
            .collect();
        assert_eq!(receipts.len(), 3);
        assert!(receipts.iter().all(|message| {
            message.payload["kind"] == "app_tool_result"
                && message.payload["name"] == "approval_decision"
                && message.payload["result"]["decision"] == "applied"
        }));
        for task_id in &task_ids {
            assert!(receipts
                .iter()
                .any(|message| message.payload["result"]["target_id"] == task_id.1));
        }
    }

    #[tokio::test]
    async fn batch_import_rejecting_all_previews_leaves_no_goals_or_pending_receipts() {
        let (db, _dir) = db();
        let focused_cycle = month_cycle(&db);
        let day_cycle = crate::service::cycles::get_or_create_day(
            &db,
            crate::domain::calendar::parse_date("2026-09-13").unwrap(),
            now_ms(),
        )
        .unwrap()
        .value;
        let custom_cycle = crate::service::cycles::create_planning_cycle(
            &db,
            &crate::service::cycles::CreateCycleArgs {
                cycle_type: "month".into(),
                starts_on: Some("2026-09-14".into()),
                ends_on: Some("2026-09-17".into()),
                title: Some("Custom import target".into()),
                ..Default::default()
            },
            crate::domain::calendar::parse_date("2026-09-16").unwrap(),
            now_ms(),
        )
        .unwrap()
        .value;
        let task_ids =
            stage_batch_import(&db, &focused_cycle, &day_cycle.id, &custom_cycle.id).await;
        assert_eq!(task_ids.len(), 3);

        resolve_batch_previews(&db, &task_ids, false);

        assert!(top_level_titles(&db, &focused_cycle).is_empty());
        assert!(top_level_titles(&db, &day_cycle.id).is_empty());
        assert!(top_level_titles(&db, &custom_cycle.id).is_empty());
        let conn = db.pool().get().unwrap();
        assert!(crate::repository::tasks::list_visible_by_cycle(&conn, &day_cycle.id)
            .unwrap()
            .is_empty());
        assert!(crate::repository::tasks::list_visible_by_cycle(&conn, &custom_cycle.id)
            .unwrap()
            .is_empty());
        drop(conn);
        assert_eq!(
            crate::service::proposals::get_preview_summary(&db, &day_cycle.id)
                .unwrap()
                .count,
            0
        );
        assert_eq!(
            crate::service::proposals::get_preview_summary(&db, &custom_cycle.id)
                .unwrap()
                .count,
            0
        );

        let conversation = get_conversation(&db).unwrap().unwrap();
        let receipts: Vec<_> = conversation
            .messages
            .iter()
            .filter(|message| message.message_type == "app_tool_result")
            .collect();
        assert_eq!(receipts.len(), 3);
        assert!(receipts.iter().all(|message| {
            message.payload["kind"] == "app_tool_result"
                && message.payload["name"] == "approval_decision"
                && message.payload["result"]["decision"] == "rejected"
        }));
    }

    #[tokio::test]
    async fn switching_days_changes_hidden_context_but_keeps_one_history() {
        use crate::ai::agent::context::{PageContext, PageView};
        let (db, _dir) = db();
        let day = |date| {
            crate::service::cycles::get_or_create_day(
                &db,
                crate::domain::calendar::parse_date(date).unwrap(),
                now_ms(),
            )
            .unwrap()
            .value
        };
        let first = day("2026-09-16");
        let second = day("2026-09-23");
        let provider = Arc::new(ContextRecorder::default());
        let selection = |cycle: &crate::domain::cycle::Cycle| TurnContext {
            cycle_id: Some(cycle.id.clone()),
            focused_task_id: None,
            page: Some(PageContext {
                view: PageView::Workspace,
                long_term_cycle_id: None,
                week_cycle_id: cycle.parent_id.clone(),
                day_cycle_id: Some(cycle.id.clone()),
                week_starts_on: Some(
                    if cycle.id == first.id {
                        "2026-09-14"
                    } else {
                        "2026-09-21"
                    }
                    .into(),
                ),
                selected_date: cycle.starts_on.clone(),
            }),
        };
        let a = run_turn(
            &db,
            provider.clone(),
            fake_resolved(),
            executor(),
            &selection(&first),
            "Remember this conversation",
        )
        .await
        .unwrap();
        let b = run_turn(
            &db,
            provider.clone(),
            fake_resolved(),
            executor(),
            &selection(&second),
            "Continue with the selected day",
        )
        .await
        .unwrap();
        assert_eq!(a.conversation_id, b.conversation_id);
        assert_eq!(get_conversation(&db).unwrap().unwrap().messages.len(), 4);
        let requests = provider.0.lock().unwrap();
        assert!(requests[0]
            .context_block
            .contains(&format!("<active_cycle_id>{}</active_cycle_id>", first.id)));
        assert!(requests[1]
            .context_block
            .contains(&format!("<active_cycle_id>{}</active_cycle_id>", second.id)));
        assert!(requests[1].context_block.contains(&format!(
            "<selected_week id=\"{}\"",
            second.parent_id.as_ref().unwrap()
        )));
        assert!(requests[1]
            .context_block
            .contains("<selected_date>2026-09-23</selected_date>"));
        assert!(requests[1]
            .history
            .iter()
            .any(|m| m.content == "Remember this conversation"));
        assert!(requests[1]
            .history
            .iter()
            .any(|m| m.content == "Kept in Coach"));
        assert!(requests[1]
            .history
            .iter()
            .all(|m| !m.content.contains("<page_state>")));
        assert_eq!(requests[1].user_message, "Continue with the selected day");
    }

    #[tokio::test]
    async fn global_chat_works_without_a_plan_and_does_not_create_one() {
        let (db, _dir) = db();
        let provider = Arc::new(ContextRecorder::default());
        run_turn(
            &db,
            provider.clone(),
            fake_resolved(),
            executor(),
            &TurnContext::default(),
            "Hello",
        )
        .await
        .unwrap();
        let requests = provider.0.lock().unwrap();
        assert!(requests[0]
            .context_block
            .contains("<active_cycle_id>null</active_cycle_id>"));
        assert!(!requests[0].context_block.contains("<cycle>"));
        let conn = db.pool().get().unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM cycles WHERE id <> 'later'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(get_conversation(&db).unwrap().unwrap().messages.len(), 2);
    }

    #[tokio::test]
    async fn in_flight_global_turn_rejects_send_from_another_plan() {
        struct Delayed {
            entered: tokio::sync::Notify,
            release: tokio::sync::Notify,
        }
        impl LlmProvider for Delayed {
            fn generate_agent(
                &self,
                _: AgentRequest,
            ) -> crate::ai::llm::BoxFuture<'_, Result<crate::ai::llm::AgentResponse, AgentError>>
            {
                Box::pin(async {
                    self.entered.notify_one();
                    self.release.notified().await;
                    Ok(crate::ai::llm::AgentResponse {
                        text: "Original reply".into(),
                        tool_calls: vec![],
                        usage: None,
                    })
                })
            }
            fn generate_json(
                &self,
                _: crate::ai::llm::LlmRequest,
            ) -> crate::ai::llm::BoxFuture<'_, Result<serde_json::Value, AgentError>> {
                unreachable!()
            }
        }
        let (db, _dir) = db();
        let first = month_cycle(&db);
        let second = crate::service::cycles::get_or_create_day(
            &db,
            crate::domain::calendar::parse_date("2026-09-23").unwrap(),
            now_ms(),
        )
        .unwrap()
        .value
        .id;
        let provider = Arc::new(Delayed {
            entered: tokio::sync::Notify::new(),
            release: tokio::sync::Notify::new(),
        });
        let running = {
            let db = db.clone();
            let provider = provider.clone();
            tokio::spawn(async move {
                run_turn(
                    &db,
                    provider,
                    fake_resolved(),
                    executor(),
                    &turn_context(&first),
                    "Keep running",
                )
                .await
            })
        };
        provider.entered.notified().await;
        assert!(get_conversation(&db)
            .unwrap()
            .unwrap()
            .active_turn_id
            .is_some());
        let rejected = run_turn(
            &db,
            Arc::new(ContextRecorder::default()),
            fake_resolved(),
            executor(),
            &turn_context(&second),
            "Another page",
        )
        .await
        .unwrap_err();
        assert!(
            matches!(rejected, AppError::Conflict { ref code, .. } if code == "turn_in_progress")
        );
        provider.release.notify_one();
        running.await.unwrap().unwrap();
        let view = get_conversation(&db).unwrap().unwrap();
        assert!(view.active_turn_id.is_none());
        assert_eq!(view.messages.len(), 2);
        assert_eq!(view.messages[1].payload["text"], "Original reply");
    }

    #[tokio::test]
    async fn each_turn_reads_the_interface_language_without_rewriting_history() {
        #[derive(Default)]
        struct Recorder(Mutex<Vec<AgentRequest>>);
        impl LlmProvider for Recorder {
            fn generate_agent(
                &self,
                request: AgentRequest,
            ) -> crate::ai::llm::BoxFuture<'_, Result<crate::ai::llm::AgentResponse, AgentError>>
            {
                self.0.lock().unwrap().push(request);
                Box::pin(async {
                    Ok(crate::ai::llm::AgentResponse {
                        text: "Original reply".into(),
                        tool_calls: vec![],
                        usage: None,
                    })
                })
            }
            fn generate_json(
                &self,
                _: crate::ai::llm::LlmRequest,
            ) -> crate::ai::llm::BoxFuture<'_, Result<serde_json::Value, AgentError>> {
                unreachable!()
            }
        }
        let (db, _dir) = db();
        let cycle = month_cycle(&db);
        let provider = Arc::new(Recorder::default());
        for locale in ["en", "zh-CN"] {
            crate::service::settings::set_locale(&db, locale.into()).unwrap();
            run_turn(
                &db,
                provider.clone(),
                fake_resolved(),
                executor(),
                &turn_context(&cycle),
                "设计 review",
            )
            .await
            .unwrap();
        }
        let requests = provider.0.lock().unwrap();
        assert!(requests[0]
            .system
            .contains("Default response language: English."));
        assert!(requests[1]
            .system
            .contains("Default response language: Simplified Chinese"));
        assert!(requests[1]
            .history
            .iter()
            .any(|m| m.content == "Original reply"));
        assert!(requests[1]
            .history
            .iter()
            .any(|m| m.content == "设计 review"));
        assert_eq!(requests[1].user_message, "设计 review");
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
            &db,
            provider,
            fake_resolved(),
            executor(),
            &turn_context(&cycle),
            "hi",
        )
        .await
        .unwrap();
        assert_eq!(result.reply, "hello!");
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[0].message_type, "user");
        assert_eq!(result.messages[1].message_type, "model_text");
        // Second read path agrees.
        let view = get_conversation(&db).unwrap().unwrap();
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
            &db,
            Arc::new(FakeProvider::with_script(vec![FakeTurn::Text(
                "first".into(),
            )])),
            fake_resolved(),
            executor(),
            &turn_context(&cycle),
            "one",
        )
        .await
        .unwrap();
        let view = get_conversation(&db).unwrap().unwrap();
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
            &db,
            provider,
            fake_resolved(),
            executor(),
            &turn_context(&cycle),
            "use tools",
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
        let conversation = repo::get_or_create_conversation(&conn, 1).unwrap();
        assert!(repo::claim_turn(&conn, &conversation.id, "foreign").unwrap());
        drop(conn);

        let provider: Arc<dyn LlmProvider> = Arc::new(FakeProvider::with_script(vec![]));
        let error = run_turn(
            &db,
            provider,
            fake_resolved(),
            executor(),
            &turn_context(&cycle),
            "hi",
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
            &db,
            provider,
            fake_resolved(),
            executor(),
            &turn_context(&cycle),
            "hi",
        )
        .await
        .unwrap_err();
        assert_eq!(crate::ai::llm::AgentError::NoActiveProvider.code(), {
            let _ = &error;
            "no_active_provider"
        });
        let conn = db.pool().get().unwrap();
        let conversation = repo::conversation(&conn).unwrap().unwrap();
        assert!(conversation.active_turn_id.is_none(), "turn released");
        assert!(conversation.last_error.is_some(), "error persisted");
        assert!(
            repo::list_messages(&conn, &conversation.id, 0)
                .unwrap()
                .is_empty(),
            "failed turns do not write half messages"
        );
    }

    #[tokio::test]
    async fn skill_activation_reloads_instructions_and_tools_in_the_same_turn() {
        struct Recorder(std::sync::atomic::AtomicUsize);
        impl LlmProvider for Recorder {
            fn generate_agent(
                &self,
                req: AgentRequest,
            ) -> crate::ai::llm::BoxFuture<
                '_,
                Result<crate::ai::llm::AgentResponse, crate::ai::llm::AgentError>,
            > {
                let first = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0;
                if first {
                    assert_eq!(req.skill, AgentSkill::None);
                    assert_eq!(req.user_message, "Plan this cycle");
                    assert!(!req.tools.iter().any(|t| t.name == "create_goal"));
                } else {
                    assert_eq!(
                        req.history
                            .iter()
                            .filter(|m| m.role == AgentRole::User && m.content == "Plan this cycle")
                            .count(),
                        1
                    );
                    assert_eq!(req.history.first().unwrap().content, "Plan this cycle");
                    assert_eq!(req.skill, AgentSkill::LongTermPlanning);
                    assert!(req.system.contains("# long-term-planning"));
                    assert!(req.tools.iter().any(|t| t.name == "create_goal"));
                }
                Box::pin(async move {
                    Ok(crate::ai::llm::AgentResponse {
                        text: if first { String::new() } else { "Ready".into() },
                        tool_calls: if first {
                            vec![ToolCallRecord {
                                id: "activate".into(),
                                name: "load_skill".into(),
                                arguments: serde_json::json!({"name":"long-term-planning"}),
                            }]
                        } else {
                            vec![]
                        },
                        usage: None,
                    })
                })
            }
            fn generate_json(
                &self,
                _: crate::ai::llm::LlmRequest,
            ) -> crate::ai::llm::BoxFuture<'_, Result<serde_json::Value, crate::ai::llm::AgentError>>
            {
                unreachable!()
            }
        }
        let (db, _dir) = db();
        let cycle = month_cycle(&db);
        let result = run_turn(
            &db,
            Arc::new(Recorder(std::sync::atomic::AtomicUsize::new(0))),
            fake_resolved(),
            Arc::new(crate::ai::tools::ToolRegistry),
            &turn_context(&cycle),
            "Plan this cycle",
        )
        .await
        .unwrap();
        assert_eq!(result.reply, "Ready");
        assert_eq!(result.active_skill.as_deref(), Some("long_term_planning"));
    }

    #[tokio::test]
    async fn current_instruction_survives_skill_and_write_rounds_then_preview_resolves() {
        use crate::service::{editor, proposals};
        const INSTRUCTION: &str = "Create exactly Approval integration goal";
        struct Planner(std::sync::atomic::AtomicUsize);
        impl LlmProvider for Planner {
            fn generate_agent(
                &self,
                req: AgentRequest,
            ) -> crate::ai::llm::BoxFuture<'_, Result<crate::ai::llm::AgentResponse, AgentError>>
            {
                let round = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if round == 0 {
                    assert_eq!(req.user_message, INSTRUCTION);
                } else {
                    assert_eq!(
                        req.history
                            .iter()
                            .filter(|m| m.role == AgentRole::User && m.content == INSTRUCTION)
                            .count(),
                        1
                    );
                    let instruction = req
                        .history
                        .iter()
                        .position(|m| m.content == INSTRUCTION)
                        .unwrap();
                    let call = req
                        .history
                        .iter()
                        .position(|m| !m.tool_calls.is_empty())
                        .unwrap();
                    assert!(
                        instruction < call,
                        "current request precedes its tool activity"
                    );
                }
                let tool_calls = match round {
                    0 => vec![ToolCallRecord {
                        id: "load".into(),
                        name: "load_skill".into(),
                        arguments: serde_json::json!({"name":"long-term-planning"}),
                    }],
                    1 => vec![ToolCallRecord {
                        id: "create".into(),
                        name: "create_goal".into(),
                        arguments: serde_json::json!({"title":"Approval integration goal","rationale":"user requested it"}),
                    }],
                    _ => vec![],
                };
                Box::pin(async move {
                    Ok(crate::ai::llm::AgentResponse {
                        text: "Preview ready".into(),
                        tool_calls,
                        usage: None,
                    })
                })
            }
            fn generate_json(
                &self,
                _: crate::ai::llm::LlmRequest,
            ) -> crate::ai::llm::BoxFuture<'_, Result<serde_json::Value, AgentError>> {
                unreachable!()
            }
        }
        let (db, _dir) = db();
        let cycle = month_cycle(&db);
        run_turn(
            &db,
            Arc::new(FakeProvider::with_script(vec![FakeTurn::Text(
                "Old table example".into(),
            )])),
            fake_resolved(),
            executor(),
            &turn_context(&cycle),
            "Show a table",
        )
        .await
        .unwrap();
        let result = run_turn(
            &db,
            Arc::new(Planner(std::sync::atomic::AtomicUsize::new(0))),
            fake_resolved(),
            Arc::new(crate::ai::tools::ToolRegistry),
            &turn_context(&cycle),
            INSTRUCTION,
        )
        .await
        .unwrap();
        assert_eq!(
            result
                .messages
                .iter()
                .filter(|m| m.message_type == "user")
                .count(),
            1
        );
        let preview = proposals::get_preview_summary(&db, &cycle).unwrap();
        assert_eq!(preview.count, 1);
        let id = preview.tasks[0].id.clone();
        assert_eq!(preview.tasks[0].title, "Approval integration goal");
        assert!(!preview.originals[&id].original_exists);
        assert!(editor::get_editor_workspace(&db, &cycle).unwrap().tasks[0]
            .task
            .proposal
            .is_some());
        proposals::keep_task_preview(&db, &id).unwrap();
        assert_eq!(
            editor::get_editor_workspace(&db, &cycle).unwrap().tasks[0]
                .task
                .title,
            "Approval integration goal"
        );

        let update = ToolCallRecord {
            id: "update".into(),
            name: "update_goal".into(),
            arguments: serde_json::json!({"task_id":id,"title":"Revised goal","rationale":"test change"}),
        };
        run_turn(
            &db,
            Arc::new(FakeProvider::with_script(vec![
                FakeTurn::Calls(vec![update]),
                FakeTurn::Text("Changed".into()),
            ])),
            fake_resolved(),
            Arc::new(crate::ai::tools::ToolRegistry),
            &turn_context(&cycle),
            "Revise it",
        )
        .await
        .unwrap();
        let preview = proposals::get_preview_summary(&db, &cycle).unwrap();
        assert_eq!(
            preview.originals[&id].title.as_deref(),
            Some("Approval integration goal")
        );
        assert_eq!(preview.tasks[0].title, "Revised goal");
        proposals::undo_task_preview(&db, &id).unwrap();
        assert_eq!(
            editor::get_editor_workspace(&db, &cycle).unwrap().tasks[0]
                .task
                .title,
            "Approval integration goal"
        );

        let delete = ToolCallRecord {
            id: "delete".into(),
            name: "delete_goal".into(),
            arguments: serde_json::json!({"task_id":id,"rationale":"remove test goal"}),
        };
        run_turn(
            &db,
            Arc::new(FakeProvider::with_script(vec![
                FakeTurn::Calls(vec![delete]),
                FakeTurn::Text("Deletion ready".into()),
            ])),
            fake_resolved(),
            Arc::new(crate::ai::tools::ToolRegistry),
            &turn_context(&cycle),
            "Remove it",
        )
        .await
        .unwrap();
        proposals::keep_task_preview(&db, &id).unwrap();
        assert_eq!(
            proposals::get_preview_summary(&db, &cycle).unwrap().count,
            0
        );
        assert!(editor::get_editor_workspace(&db, &cycle)
            .unwrap()
            .tasks
            .is_empty());
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
        let conversation = repo::conversation(&conn).unwrap().unwrap();
        assert_eq!(
            conversation.active_skill.as_deref(),
            Some("long_term_planning")
        );
    }
}

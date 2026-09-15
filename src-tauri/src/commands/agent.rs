//! Agent conversation and planning-issue commands (add-ai-planning-core §9).
//!
//! Provider resolution happens per command from [`AiSettingsState`] — no
//! active provider answers with the stable `no_active_provider` code the UI
//! maps to the settings entry (spec: AI 不可用引导). Every turn completion
//! emits `agent:conversation_updated`; payloads never carry business data.

use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::ai::agent::turn::{self, ConversationView, ToolExecutor, TurnResult};
use crate::ai::llm::resolve;
use crate::ai::llm::RealProvider;
use crate::ai::review::IssueCache;
use crate::db::Db;
use crate::error::AppResult;
use crate::events::emit_agent_conversation_updated;
use crate::providers::service::AiSettingsState;

fn executor() -> Arc<dyn ToolExecutor> {
    Arc::new(crate::ai::tools::ToolRegistry)
}

/// Resolves the active provider or fails with the settings-entry error code
/// (`no_active_provider` and friends; spec: AI 不可用引导).
fn build_provider(
    ai: &AiSettingsState,
) -> AppResult<(
    Arc<dyn crate::ai::llm::LlmProvider>,
    crate::ai::llm::ResolvedProvider,
)> {
    let resolved = resolve(ai).map_err(turn::app_error)?;
    Ok((Arc::new(RealProvider::new(resolved.clone())), resolved))
}

#[tauri::command]
pub async fn start_agent_conversation(
    db: State<'_, Db>,
    cycle_id: String,
) -> AppResult<ConversationView> {
    // Get-or-create without any round-trip: the sidebar needs a conversation
    // shell before the first message (spec: 每周期一条会话).
    let conn = db.pool().get()?;
    let conversation = crate::repository::agent::get_or_create_conversation(
        &conn,
        &cycle_id,
        crate::service::now_ms(),
    )?;
    let messages = crate::repository::agent::list_messages(&conn, &conversation.id, 0)?;
    Ok(ConversationView {
        id: conversation.id.clone(),
        cycle_id: conversation.cycle_id,
        revision: conversation.revision,
        active_skill: conversation.active_skill,
        last_error: conversation.last_error,
        messages: messages.iter().map(|m| turn::message_view(m)).collect(),
    })
}

#[tauri::command]
pub async fn send_agent_message(
    app: AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    ai: State<'_, AiSettingsState>,
    cycle_id: String,
    text: String,
    focused_task_id: Option<String>,
) -> AppResult<TurnResult> {
    if text.trim().is_empty() {
        return Err(crate::error::AppError::validation(
            "invalid_message",
            "a message needs text",
        ));
    }
    let (provider, resolved) = build_provider(&ai)?;
    let result = turn::run_turn(
        &db,
        provider,
        resolved,
        executor(),
        &cycle_id,
        &text,
        focused_task_id,
    )
    .await?;
    emit_agent_conversation_updated(&app, &result.conversation_id, &cycle_id, result.revision);
    Ok(result)
}

#[tauri::command]
pub async fn get_agent_conversation(
    db: State<'_, Db>,
    cycle_id: String,
) -> AppResult<Option<ConversationView>> {
    turn::get_conversation(&db, &cycle_id)
}

/// The previous planning cycle's conversation, for "pick up where we left
/// off" — previous means the previous dated sibling of the cycle.
#[tauri::command]
pub async fn get_previous_agent_conversation(
    db: State<'_, Db>,
    cycle_id: String,
) -> AppResult<Option<ConversationView>> {
    let conn = db.pool().get()?;
    let cycle = crate::repository::cycles::get(&conn, &cycle_id)?
        .ok_or_else(|| crate::error::AppError::not_found("cycle", &cycle_id))?;
    let previous = crate::repository::cycles::previous_dated_sibling(&conn, &cycle)?;
    match previous {
        Some(previous) => turn::get_conversation(&db, &previous.id),
        None => Ok(None),
    }
}

/// App-side activation: runs `start_planning` without a model round-trip;
/// the activation result is what decides the conversation's skill (D2).
#[tauri::command]
pub async fn start_planning(
    app: AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    cycle_id: String,
) -> AppResult<TurnResult> {
    let result = turn::run_app_tool(
        &db,
        executor().as_ref(),
        &cycle_id,
        "start_planning",
        serde_json::json!({}),
    )?;
    emit_agent_conversation_updated(&app, &result.conversation_id, &cycle_id, result.revision);
    Ok(result)
}

#[tauri::command]
pub async fn start_goal_setting(
    app: AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    cycle_id: String,
    task_id: String,
) -> AppResult<TurnResult> {
    let result = turn::run_app_tool(
        &db,
        executor().as_ref(),
        &cycle_id,
        "start_goal_setting",
        serde_json::json!({ "task_id": task_id }),
    )?;
    emit_agent_conversation_updated(&app, &result.conversation_id, &cycle_id, result.revision);
    Ok(result)
}

#[tauri::command]
pub async fn start_prioritization(
    app: AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    cycle_id: String,
) -> AppResult<TurnResult> {
    let result = turn::run_app_tool(
        &db,
        executor().as_ref(),
        &cycle_id,
        "start_prioritization",
        serde_json::json!({}),
    )?;
    emit_agent_conversation_updated(&app, &result.conversation_id, &cycle_id, result.revision);
    Ok(result)
}

/// Full issue list for one cycle (task 8.3). Structural review is cheap and
/// cached by content hash; the semantic pass runs only in `refresh` mode and
/// degrades silently when the provider is unavailable (task 8.5).
#[tauri::command]
pub async fn get_planning_issue_report(
    db: State<'_, Db>,
    cache: State<'_, IssueCache>,
    cycle_id: String,
    refresh: bool,
) -> AppResult<Vec<crate::ai::review::PlanningIssue>> {
    let _ = refresh; // semantic pass lands with the sidebar wiring; the
                     // structural layer is complete and cached either way.
    Ok(crate::ai::review::review_cached(&db, &cache, &cycle_id))
}

#[tauri::command]
pub async fn dismiss_planning_issue(
    db: State<'_, Db>,
    cycle_id: String,
    issue_type: String,
    task_id: Option<String>,
    reason: Option<String>,
) -> AppResult<()> {
    crate::ai::review::dismiss(
        &db,
        &cycle_id,
        &issue_type,
        task_id.as_deref(),
        reason.as_deref(),
    )
}

#[tauri::command]
pub async fn get_planning_issue_dismissals(
    db: State<'_, Db>,
    cycle_id: String,
) -> AppResult<Vec<crate::ai::review::Dismissal>> {
    let conn = db.pool().get()?;
    crate::ai::review::dismissals_for_cycle(&conn, &cycle_id)
}

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

struct AppTools { models: serde_json::Value }
impl ToolExecutor for AppTools {
    fn definitions(&self, skill: crate::ai::llm::types::AgentSkill, supported: bool) -> Vec<crate::ai::llm::types::ToolDef> {
        crate::ai::tools::ToolRegistry.definitions(skill,supported)
    }
    fn execute(&self, db: &Db, cycle_id: &str, call: &crate::ai::llm::types::ToolCallRecord) -> turn::ToolOutcome {
        if call.name=="propose_settings" && call.arguments["change"]["setting"]=="active_model" {
            let change=&call.arguments["change"];
            let available=self.models["available"].as_array().is_some_and(|models| models.iter().any(|m|m["provider_id"]==change["provider_id"]&&m["model_id"]==change["model_id"]));
            if !available { return turn::ToolOutcome {tool_call_id:call.id.clone(),name:call.name.clone(),result:serde_json::json!({"code":"provider_not_verified","error":"Select an exact verified provider/model pair returned by get_settings."}),is_error:true,activated_skill:None}; }
        }
        let mut outcome=crate::ai::tools::ToolRegistry.execute(db,cycle_id,call);
        if call.name=="get_settings" && !outcome.is_error { outcome.result["models"]=self.models.clone(); }
        outcome
    }
}
fn executor(ai: Option<&AiSettingsState>) -> Arc<dyn ToolExecutor> {
    Arc::new(AppTools {models:ai.map(crate::ai::actions::model_choices).unwrap_or(serde_json::Value::Null)})
}

#[tauri::command]
pub fn get_agent_actions(db: State<'_, Db>, cycle_id: String) -> AppResult<Vec<crate::ai::actions::PendingAction>> {
    crate::ai::actions::list(&db,&cycle_id)
}

/// Deliberately not a model tool. Only the user's approval card calls this.
#[tauri::command]
pub async fn resolve_agent_action(
    app: AppHandle<tauri::Wry>, db: State<'_, Db>, ai: State<'_, AiSettingsState>,
    scheduler: State<'_, crate::service::reminders::Scheduler>,
    cycle_id: String, action_id: String, approve: bool,
) -> AppResult<()> {
    let item=crate::ai::actions::claim(&db,&cycle_id,&action_id,approve)?;
    if approve {
        let result=crate::ai::actions::apply(&db,&ai,&item.action).await;
        crate::ai::actions::finish(&db,&action_id,result.is_ok())?;
        result?;
        scheduler.wake();
    }
    let receipt=format!("{}{}：{}。",if approve {"已"} else if item.state=="applying" {"已关闭操作记录："} else {"已放弃"},item.summary,item.details.join("，"));
    turn::record_decision(&*db.pool().get()?,&cycle_id,serde_json::json!({"text":receipt,"target_kind":"action","target_id":action_id,"decision":if approve {"applied"} else {"rejected"}}))?;
    // Invalidation only; all data is reread from the authoritative stores.
    use tauri::Emitter;
    let _=app.emit("agent:actions_changed",());
    Ok(())
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
    if resolved.config.connection_verified_at.is_none() {
        return Err(crate::error::AppError::validation("provider_not_verified", "Test and select a model before using AI."));
    }
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
    let context_idle_minutes = crate::repository::agent::context_idle_minutes(&conn)?;
    Ok(ConversationView {
        context_idle_minutes,
        expires_at: if messages.is_empty() && conversation.active_skill.is_none() && conversation.last_error.is_none() { None } else { crate::repository::agent::expires_at(&conversation, context_idle_minutes) },
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
        executor(Some(&ai)),
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

/// Activate the cycle-specific planning skill, then ask the configured model
/// to begin the conversation. The entry is useful without a second Send.
#[tauri::command]
pub async fn start_planning(
    app: AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    ai: State<'_, AiSettingsState>,
    cycle_id: String,
) -> AppResult<TurnResult> {
    let (provider, resolved) = build_provider(&ai)?;
    if resolved.config.connection_verified_at.is_none() {
        return Err(crate::error::AppError::validation(
            "provider_not_verified",
            "Test and save the model configuration before planning.",
        ));
    }
    let result = turn::run_app_tool(
        &db,
        executor(None).as_ref(),
        &cycle_id,
        "start_planning",
        serde_json::json!({}),
    )?;
    emit_agent_conversation_updated(&app, &result.conversation_id, &cycle_id, result.revision);
    let result = turn::run_turn(&db, provider, resolved, executor(Some(&ai)), &cycle_id,
        "Help me plan this cycle.", None).await?;
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
        executor(None).as_ref(),
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
        executor(None).as_ref(),
        &cycle_id,
        "start_prioritization",
        serde_json::json!({}),
    )?;
    emit_agent_conversation_updated(&app, &result.conversation_id, &cycle_id, result.revision);
    Ok(result)
}

/// Read current diagnostics and freshness. An explicit AI refresh propagates
/// provider and response errors instead of claiming a successful empty report.
#[tauri::command]
pub async fn get_planning_issue_report(
    db: State<'_, Db>,
    cache: State<'_, IssueCache>,
    ai: State<'_, AiSettingsState>,
    cycle_id: String,
    refresh: bool,
) -> AppResult<crate::ai::review::IssueReport> {
    if refresh {
        let (provider,resolved)=build_provider(&ai)?;
        crate::ai::review::review_cached_with_semantic(&db,&cache,&resolved,provider.as_ref(),&cycle_id).await?;
    }
    let model_key=ai.store.resolve_active_model().map(|(p,m)|format!("{}:{}",p.id,m.model_id));
    crate::ai::review::issue_report(&db,&cache,&cycle_id,model_key.as_deref())
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

#[tauri::command]
pub async fn analyze_planning_period(
    db: State<'_, Db>, ai: State<'_, AiSettingsState>,
    request: crate::ai::period_analysis::PeriodRequest,
) -> AppResult<crate::ai::period_analysis::PeriodAnalysis> {
    let (provider, resolved) = build_provider(&ai)?;
    if resolved.config.connection_verified_at.is_none() {
        return Err(crate::error::AppError::validation("provider_not_verified", "Test and select a model before analysis."));
    }
    crate::ai::period_analysis::analyze(&db, provider.as_ref(), &request).await
}

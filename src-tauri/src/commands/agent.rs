//! Agent conversation and planning-issue commands (add-ai-planning-core §9).
//!
//! Provider resolution happens per command from [`AiSettingsState`] — no
//! active provider answers with the stable `no_active_provider` code the UI
//! maps to the settings entry (spec: AI 不可用引导). Every turn completion
//! emits `agent:conversation_updated`; payloads never carry business data.

use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::ai::agent::context::{PageContext, TurnContext};
use crate::ai::agent::turn::{self, ConversationView, ToolExecutor, TurnResult};
use crate::ai::llm::resolve;
use crate::ai::llm::RealProvider;
use crate::ai::review::IssueCache;
use crate::db::Db;
use crate::error::AppResult;
use crate::events::emit_agent_conversation_updated;
use crate::providers::service::AiSettingsState;

struct AppTools {
    models: serde_json::Value,
}
impl ToolExecutor for AppTools {
    fn definitions(
        &self,
        skill: crate::ai::llm::types::AgentSkill,
        supported: bool,
    ) -> Vec<crate::ai::llm::types::ToolDef> {
        crate::ai::tools::ToolRegistry.definitions(skill, supported)
    }
    fn execute(
        &self,
        db: &Db,
        cycle_id: &str,
        call: &crate::ai::llm::types::ToolCallRecord,
    ) -> turn::ToolOutcome {
        if call.name == "propose_settings" && call.arguments["change"]["setting"] == "active_model"
        {
            let change = &call.arguments["change"];
            let available = self.models["available"].as_array().is_some_and(|models| {
                models.iter().any(|m| {
                    m["provider_id"] == change["provider_id"] && m["model_id"] == change["model_id"]
                })
            });
            if !available {
                return turn::ToolOutcome {
                    tool_call_id: call.id.clone(),
                    name: call.name.clone(),
                    result: serde_json::json!({"code":"provider_not_verified","error":"Select an exact verified provider/model pair returned by get_settings."}),
                    is_error: true,
                    activated_skill: None,
                };
            }
        }
        let mut outcome = crate::ai::tools::ToolRegistry.execute(db, cycle_id, call);
        if call.name == "get_settings" && !outcome.is_error {
            outcome.result["models"] = self.models.clone();
        }
        outcome
    }
}
fn executor(ai: Option<&AiSettingsState>) -> Arc<dyn ToolExecutor> {
    Arc::new(AppTools {
        models: ai
            .map(crate::ai::actions::model_choices)
            .unwrap_or(serde_json::Value::Null),
    })
}

#[tauri::command]
pub fn get_agent_actions(db: State<'_, Db>) -> AppResult<Vec<crate::ai::actions::PendingAction>> {
    crate::ai::actions::list_all(&db)
}

/// Deliberately not a model tool. Only the user's approval card calls this.
#[tauri::command]
pub async fn resolve_agent_action(
    app: AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    ai: State<'_, AiSettingsState>,
    scheduler: State<'_, crate::service::reminders::Scheduler>,
    cycle_id: String,
    action_id: String,
    approve: bool,
) -> AppResult<()> {
    let mut decision_guard = turn::DecisionGuard::begin(&db)?;
    let item = crate::ai::actions::claim(&db, &cycle_id, &action_id, approve)?;
    if approve {
        let result = crate::ai::actions::apply(&db, &ai, &item.action).await;
        crate::ai::actions::finish(&db, &action_id, result.is_ok())?;
        result?;
        scheduler.wake();
    }
    let locale = crate::i18n::for_db(&db)?;
    let summary = if item.summary_key.is_empty() {
        item.summary.clone()
    } else {
        crate::i18n::render_message(
            locale,
            &crate::i18n::LocalizedMessage::new(item.summary_key.clone(), serde_json::json!({})),
        )
    };
    let details = item
        .details
        .iter()
        .map(|detail| crate::i18n::render_message(locale, detail))
        .collect::<Vec<_>>();
    let operation = serde_json::to_value(&item.action)
        .ok()
        .and_then(|value| value.get("change").cloned())
        .and_then(|value| {
            value
                .get("operation")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .or_else(|| {
            serde_json::to_value(&item.action).ok().and_then(|value| {
                value
                    .get("change")
                    .and_then(|change| change.get("setting"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
        })
        .unwrap_or_else(|| "day_move".into());
    let decision = if approve {
        "applied"
    } else if item.state == "applying" {
        "closed"
    } else {
        "rejected"
    };
    let status_key = format!("backend-actions:receipt.{decision}");
    let status = crate::i18n::render_message(
        locale,
        &crate::i18n::LocalizedMessage::new(status_key, serde_json::json!({})),
    );
    let receipt = crate::i18n::render_message(
        locale,
        &crate::i18n::LocalizedMessage::new(
            "backend-actions:receipt.action",
            serde_json::json!({"status": status, "summary": summary, "details": details.join(if locale == crate::i18n::Locale::ZhCn { "，" } else { ", " })}),
        ),
    );
    let result = serde_json::json!({"summary_key":item.summary_key,"details":item.details,"decision":decision,"operation":operation});
    decision_guard.record(
        serde_json::json!({"text":receipt,"result":result,"target_kind":"action","target_id":action_id,"decision":decision}),
    )?;
    // Invalidation only; all data is reread from the authoritative stores.
    use tauri::Emitter;
    let _ = app.emit("agent:actions_changed", ());
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
        return Err(crate::error::AppError::validation(
            "provider_not_verified",
            "Test and select a model before using AI.",
        ));
    }
    Ok((Arc::new(RealProvider::new(resolved.clone())), resolved))
}

#[tauri::command]
pub async fn start_agent_conversation(db: State<'_, Db>) -> AppResult<ConversationView> {
    let conn = db.pool().get()?;
    crate::repository::agent::get_or_create_conversation(&conn, crate::service::now_ms())?;
    drop(conn);
    turn::get_conversation(&db)?
        .ok_or_else(|| crate::error::AppError::Internal("Coach conversation missing".into()))
}

#[tauri::command]
pub async fn send_agent_message(
    app: AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    ai: State<'_, AiSettingsState>,
    cycle_id: Option<String>,
    text: String,
    focused_task_id: Option<String>,
    page_context: Option<PageContext>,
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
        &TurnContext {
            cycle_id,
            focused_task_id,
            page: page_context,
        },
        &text,
    )
    .await?;
    emit_agent_conversation_updated(&app, &result.conversation_id, result.revision);
    Ok(result)
}

#[tauri::command]
pub async fn get_agent_conversation(db: State<'_, Db>) -> AppResult<Option<ConversationView>> {
    turn::get_conversation(&db)
}

/// Activate the cycle-specific planning skill, then ask the configured model
/// to begin the conversation. The entry is useful without a second Send.
#[tauri::command]
pub async fn start_planning(
    app: AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    ai: State<'_, AiSettingsState>,
    cycle_id: String,
    page_context: Option<PageContext>,
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
    emit_agent_conversation_updated(&app, &result.conversation_id, result.revision);
    let result = turn::run_turn(
        &db,
        provider,
        resolved,
        executor(Some(&ai)),
        &TurnContext {
            cycle_id: Some(cycle_id.clone()),
            focused_task_id: None,
            page: page_context,
        },
        &crate::i18n::text(crate::i18n::for_db(&db)?, "prompt.plan", &[]),
    )
    .await?;
    emit_agent_conversation_updated(&app, &result.conversation_id, result.revision);
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
    emit_agent_conversation_updated(&app, &result.conversation_id, result.revision);
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
    emit_agent_conversation_updated(&app, &result.conversation_id, result.revision);
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
        let (provider, resolved) = build_provider(&ai)?;
        crate::ai::review::review_cached_with_semantic(
            &db,
            &cache,
            &resolved,
            provider.as_ref(),
            &cycle_id,
        )
        .await?;
    }
    let model_key = ai
        .store
        .resolve_active_model()
        .map(|(p, m)| format!("{}:{}", p.id, m.model_id));
    crate::ai::review::issue_report(&db, &cache, &cycle_id, model_key.as_deref())
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
    db: State<'_, Db>,
    ai: State<'_, AiSettingsState>,
    request: crate::ai::period_analysis::PeriodRequest,
) -> AppResult<crate::ai::period_analysis::PeriodAnalysis> {
    let (provider, resolved) = build_provider(&ai)?;
    if resolved.config.connection_verified_at.is_none() {
        return Err(crate::error::AppError::validation(
            "provider_not_verified",
            "Test and select a model before analysis.",
        ));
    }
    crate::ai::period_analysis::analyze(&db, provider.as_ref(), &request).await
}

//! The planning tool set (add-ai-planning-core §5): the [`ToolRegistry`]
//! implements [`ToolExecutor`] over the read paths, the planning engines and
//! the preview layer.
//!
//! Behaviour contracts by section:
//!
//! * §5.1 read tools never mutate; `get_cycle_context` reuses the §4
//!   `load_context`/`render` pair unchanged.
//! * §5.2 activation tools carry the skill on
//!   [`ToolOutcome::activated_skill`] — the turn executor persists it on the
//!   conversation (design D2); the tool layer never writes `active_skill`
//!   itself, and `start_planning` dispatches through
//!   [`planning_skill_for_cycle`] so session cycles answer with the stable
//!   `unsupported_cycle_type` code.
//! * §5.3 write tools **always** go through `service::proposals` (design D3):
//!   the tool layer has no "am I in preview?" logic and no direct task write.
//!   Every write tool requires a `rationale`, and — per spec (工具结果对模型
//!   报告成功) — reports success with `status: "proposed"`; the pending
//!   confirm/revert loop belongs to the human interface, never to the model.
//! * Task transfers and non-task writes use typed, human-approved actions.
//!   Domain services execute them only after a GUI decision, preserving IDs.
//! * §5.6 tool arguments deserialize strictly (`deny_unknown_fields` on every
//!   args struct): an unknown field or a mistyped value is a tool **error
//!   result**, never a panic and never a silently ignored field. The nested
//!   `GoalBreakdownUpdate` stays intentionally lenient about unknown keys (it
//!   is the §6 engine's wire type); a no-op update is rejected downstream as
//!   `empty_update`, so nothing silent survives.
//! * Prioritization (`update_prioritization_breakdown`) stages a typed action:
//!   the five-bucket document is whole-row state on `cycles` (design D6), so
//!   the action carries the incremental update and applies the same merge and
//!   validation only after GUI approval. `start_prioritization` recomputes
//!   `pending_review` from the cycle's visible tasks so a stale pending list
//!   can never leak out.

use std::collections::{HashMap, HashSet};

use serde::Deserialize;
use serde_json::json;

use crate::ai::actions::{self, Action, PrioritizationAction};
use crate::ai::agent::context::{load_context, render};
use crate::ai::agent::prompt::planning_skill_for_cycle;
use crate::ai::agent::turn::{ToolExecutor, ToolOutcome};
use crate::ai::breakdown;
use crate::ai::llm::types::{AgentSkill, ToolCallRecord, ToolDef};
use crate::ai::prioritization;
use crate::db::Db;
use crate::domain::cycle::{Cycle, CycleType};
use crate::domain::task::{render_subtasks_markdown, Task};
use crate::error::{AppError, AppResult};
use crate::repository::{cycles as cycles_repo, tasks as tasks_repo};
use crate::service::now_ms;
use crate::service::proposals::{self, TaskInput};
use crate::service::reviews as reviews_service;

/// The stateless planning tool registry. All state lives in the database; the
/// session-bound `cycle_id` arrives per call.
pub struct ToolRegistry;

// ---------------------------------------------------------------------------
// Strict argument shapes (§5.6)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GetCycleContextArgs {
    #[serde(default)]
    cycle_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GetTaskDetailsArgs {
    task_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StartPlanningArgs {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct LoadSkillArgs {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StartGoalSettingArgs {
    task_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StartPrioritizationArgs {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct StartReviewArgs {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateGoalArgs {
    /// Omit to use the focused cycle; otherwise use an ID from list_cycles.
    #[serde(default)]
    cycle_id: Option<String>,
    title: String,
    /// Contract-required; consumed by the proposal audit trail, not read here.
    #[allow(dead_code)]
    rationale: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateGoalArgs {
    task_id: String,
    /// Absent keeps the current title.
    #[serde(default)]
    title: Option<String>,
    completed: Option<bool>,
    subtasks: Option<Vec<crate::domain::task::Subtask>>,
    /// Contract-required; consumed by the proposal audit trail, not read here.
    #[allow(dead_code)]
    rationale: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteGoalArgs {
    task_id: String,
    /// Contract-required; consumed by the proposal audit trail, not read here.
    #[allow(dead_code)]
    rationale: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateGoalBreakdownArgs {
    task_id: String,
    /// §6 wire type: `null` clears, absent keeps, no-op rejected downstream.
    update: breakdown::GoalBreakdownUpdate,
    /// Contract-required; consumed by the proposal audit trail, not read here.
    #[allow(dead_code)]
    rationale: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdatePrioritizationBreakdownArgs {
    /// §7 wire type: absent buckets survive, `Some(vec![])` clears one.
    update: prioritization::PrioritizationBreakdownUpdate,
    /// Contract-required; consumed by the proposal audit trail, not read here.
    #[allow(dead_code)]
    rationale: String,
}

// ---------------------------------------------------------------------------
// ToolExecutor
// ---------------------------------------------------------------------------

impl ToolExecutor for ToolRegistry {
    /// Every tool-capable model can choose a workflow. Planning skills
    /// expose proposal tools; analysis/diagnosis expose only reads and loading.
    /// The executor checks this same list before running any model call.
    fn definitions(&self, skill: AgentSkill, tools_supported: bool) -> Vec<ToolDef> {
        if !tools_supported {
            return Vec::new();
        }
        let mut definitions = vec![def_get_cycle_context(), def_get_task_details()];
        match skill {
            AgentSkill::None => {}
            AgentSkill::PeriodAnalysis => definitions.push(def_get_period_context()),
            AgentSkill::PlanningIssues => definitions.push(def_get_planning_issues()),
            _ => {
                definitions.extend([
                    def_create_goal(),
                    def_update_goal(),
                    def_delete_goal(),
                    def_update_goal_breakdown(),
                ]);
                if skill == AgentSkill::Prioritization {
                    definitions.push(def_update_prioritization_breakdown());
                }
                if skill == AgentSkill::Review {
                    definitions.push(def_start_review());
                }
            }
        }
        definitions.extend(crate::ai::tool_catalog::definitions(skill));
        definitions.push(def_load_skill());
        definitions
    }

    /// Runs one call. Model-supplied arguments are untrusted input: every
    /// failure path — including argument parsing — becomes an `is_error`
    /// result, never a panic (§5.6).
    fn execute(&self, db: &Db, cycle_id: &str, call: &ToolCallRecord) -> ToolOutcome {
        let outcome = self.run(db, cycle_id, call);
        match outcome {
            Ok((result, activated_skill)) => ToolOutcome {
                tool_call_id: call.id.clone(),
                name: call.name.clone(),
                result,
                is_error: false,
                activated_skill,
            },
            Err(error) => ToolOutcome {
                tool_call_id: call.id.clone(),
                name: call.name.clone(),
                result: error_payload(&error),
                is_error: true,
                activated_skill: None,
            },
        }
    }
}

impl ToolRegistry {
    /// Dispatch + per-tool execution. The `Option<AgentSkill>` in the success
    /// path is what the turn executor persists as `active_skill` (D2).
    fn run(
        &self,
        db: &Db,
        cycle_id: &str,
        call: &ToolCallRecord,
    ) -> AppResult<(serde_json::Value, Option<AgentSkill>)> {
        if [
            "create_goal",
            "update_goal",
            "delete_goal",
            "update_goal_breakdown",
            "update_prioritization_breakdown",
        ]
        .contains(&call.name.as_str())
            && call
                .arguments
                .get("rationale")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|r| r.trim().is_empty())
        {
            return Err(AppError::validation(
                "missing_reason",
                "A nonempty rationale is required",
            ));
        }
        match call.name.as_str() {
            "load_skill" => {
                let args: LoadSkillArgs = parse_args(call)?;
                let skill = crate::ai::skills::Skill::parse(&args.name).ok_or_else(|| {
                    AppError::validation(
                        "unknown_skill",
                        "Choose a skill name from the tool catalog.",
                    )
                })?;
                let instructions = crate::ai::skills::load(skill)?;
                Ok((
                    json!({ "skill": skill.name(), "instructions": instructions }),
                    Some(skill.agent_skill()),
                ))
            }
            "get_period_context" => {
                let args: crate::ai::period_analysis::PeriodRequest = parse_args(call)?;
                Ok((
                    serde_json::to_value(crate::ai::period_analysis::facts(db, &args)?)
                        .map_err(|e| AppError::Internal(e.to_string()))?,
                    None,
                ))
            }
            "get_planning_issues" => {
                let _: StartPlanningArgs = parse_args(call)?;
                Ok((
                    json!({ "issues": crate::ai::review::review_cycle(db, cycle_id) }),
                    None,
                ))
            }
            // -- read (5.1) ------------------------------------------------
            "get_cycle_context" => {
                let args: GetCycleContextArgs = parse_args(call)?;
                Ok((get_cycle_context(db, cycle_id, &args)?, None))
            }
            "get_task_details" => {
                let args: GetTaskDetailsArgs = parse_args(call)?;
                Ok((get_task_details(db, &args)?, None))
            }
            // -- activation (5.2) -----------------------------------------
            "start_planning" => {
                let _: StartPlanningArgs = parse_args(call)?;
                let result = start_planning(db, cycle_id)?;
                let skill = result
                    .get("activated_skill")
                    .and_then(serde_json::Value::as_str)
                    .and_then(AgentSkill::parse);
                Ok((result, skill))
            }
            "start_goal_setting" => {
                let args: StartGoalSettingArgs = parse_args(call)?;
                Ok((
                    start_goal_setting(db, &args)?,
                    Some(AgentSkill::GoalSetting),
                ))
            }
            "start_prioritization" => {
                let _: StartPrioritizationArgs = parse_args(call)?;
                Ok((
                    start_prioritization(db, cycle_id)?,
                    Some(AgentSkill::Prioritization),
                ))
            }
            "start_review" => {
                let _: StartReviewArgs = parse_args(call)?;
                let result = start_review(db, cycle_id)?;
                let skill = result
                    .get("activated_skill")
                    .and_then(serde_json::Value::as_str)
                    .and_then(AgentSkill::parse);
                Ok((result, skill))
            }
            // -- writes, all preview-backed (5.3) --------------------------
            "create_goal" => {
                let args: CreateGoalArgs = parse_args(call)?;
                Ok((create_goal(db, cycle_id, &args)?, None))
            }
            "update_goal" => {
                let args: UpdateGoalArgs = parse_args(call)?;
                Ok((update_goal(db, &args)?, None))
            }
            "delete_goal" => {
                let args: DeleteGoalArgs = parse_args(call)?;
                Ok((delete_goal(db, &args)?, None))
            }
            "update_goal_breakdown" => {
                let args: UpdateGoalBreakdownArgs = parse_args(call)?;
                Ok((update_goal_breakdown(db, &args)?, None))
            }
            "update_prioritization_breakdown" => {
                let args: UpdatePrioritizationBreakdownArgs = parse_args(call)?;
                Ok((update_prioritization_breakdown(db, cycle_id, &args)?, None))
            }
            _ => Ok((crate::ai::tool_catalog::execute(db, cycle_id, call)?, None)),
        }
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Deserializes one tool's arguments strictly. Any deviation — unknown field,
/// wrong type, non-object payload — is a validation error with the stable
/// `invalid_arguments` code (§5.6), surfaced to the model as an error result.
fn parse_args<T: serde::de::DeserializeOwned>(call: &ToolCallRecord) -> AppResult<T> {
    serde_json::from_value(call.arguments.clone()).map_err(|error| {
        AppError::validation(
            "invalid_arguments",
            format!("invalid arguments for tool '{}': {error}", call.name),
        )
    })
}

/// Maps an [`AppError`] onto the JSON error result the model reads. Stable
/// codes: service validation/conflict codes pass through (`unsupported_cycle_type`,
/// `cycle_ended`, `empty_update`, `missing_reason`, `preview_conflict`, …);
/// missing rows become `task_not_found` / `cycle_not_found` (spec: task 9.4's
/// error-code table).
fn error_payload(error: &AppError) -> serde_json::Value {
    let code = match error {
        AppError::Validation { code, .. } | AppError::Conflict { code, .. } => code.clone(),
        AppError::NotFound { entity, .. } => match entity.as_str() {
            "task" => "task_not_found".to_string(),
            "cycle" => "cycle_not_found".to_string(),
            other => format!("{other}_not_found"),
        },
        AppError::Db(_) => "db_error".to_string(),
        AppError::Internal(_) => "internal".to_string(),
    };
    json!({ "error": error.to_string(), "code": code })
}

/// The full per-task detail block shared by `get_task_details` and
/// `start_goal_setting`: every task field plus the rendered subtask
/// checklist, the raw `goal_breakdown` value, the engine-derived missing
/// fields and the clarity flags (§5.1 / goal-clarification 单任务上下文).
fn task_details(task: &Task) -> serde_json::Value {
    let raw_breakdown = task
        .goal_breakdown
        .clone()
        .unwrap_or(serde_json::Value::Null);
    let parsed = breakdown::GoalBreakdown::from_value(&raw_breakdown);
    let missing: Vec<&str> = breakdown::missing_fields(&parsed)
        .iter()
        .map(|field| field.field_path())
        .collect();
    let mut detail = serde_json::to_value(task).unwrap_or(serde_json::Value::Null);
    if let Some(object) = detail.as_object_mut() {
        object.insert(
            "subtasks_markdown".to_string(),
            json!(render_subtasks_markdown(&task.subtasks)),
        );
        object.insert("goal_breakdown".to_string(), raw_breakdown);
        object.insert("missing_fields".to_string(), json!(missing));
        object.insert("needs_refinement".to_string(), json!(task.needs_refinement));
        object.insert("needs_breakdown".to_string(), json!(task.needs_breakdown));
    }
    detail
}

/// Cycle key for model-facing renders. Mirrors `context.rs`'s `cycle_key`:
/// the stored calendar identity when present, `session:<id>` for focus
/// blocks, the row id as a last resort.
fn cycle_key_label(cycle: &Cycle) -> String {
    match cycle.cycle_type {
        CycleType::Session => format!("session:{}", cycle.id),
        _ => cycle
            .calendar_key
            .clone()
            .unwrap_or_else(|| cycle.id.clone()),
    }
}

fn titles_of(tasks: &[Task]) -> HashMap<String, String> {
    tasks
        .iter()
        .map(|task| (task.id.clone(), task.title.clone()))
        .collect()
}

/// Builds the proposed-row input that carries a task's current content —
/// `apply_update_preview` stages the FULL row state, so partial tools (title
/// rename, breakdown write-back) must re-submit the untouched fields.
fn carry_over_input(task: &Task) -> TaskInput {
    TaskInput {
        title: task.title.clone(),
        subtasks: task.subtasks.clone(),
        completed: task.completed,
        goal_breakdown: task.goal_breakdown.clone(),
        needs_refinement: task.needs_refinement,
        needs_breakdown: task.needs_breakdown,
        root_color_key: task.root_color_key.clone(),
        parent_id: task.parent_id.clone(),
    }
}

// ---------------------------------------------------------------------------
// §5.1 read tools
// ---------------------------------------------------------------------------

/// Render a focused or explicitly identified cycle through the shared context loader.
/// Calendar labels are display data; only real cycle IDs are accepted.
fn get_cycle_context(
    db: &Db,
    cycle_id: &str,
    args: &GetCycleContextArgs,
) -> AppResult<serde_json::Value> {
    let cycle_id = args.cycle_id.as_deref().unwrap_or(cycle_id);
    let conn = db.pool().get()?;
    let context = load_context(&conn, cycle_id)?;
    Ok(json!({ "context": render(&context, None) }))
}

/// `get_task_details` — one task's full detail block (see [`task_details`]).
/// Unknown ids answer with the `task_not_found` code.
fn get_task_details(db: &Db, args: &GetTaskDetailsArgs) -> AppResult<serde_json::Value> {
    let conn = db.pool().get()?;
    let task = tasks_repo::require(&conn, &args.task_id)?;
    Ok(task_details(&task))
}

// ---------------------------------------------------------------------------
// §5.2 activation tools
// ---------------------------------------------------------------------------

/// `start_planning` — dispatches on the bound cycle's type through
/// [`planning_skill_for_cycle`]: month/week/day each activate their workflow; `session` → the `unsupported_cycle_type` error
/// whose text is the spec's exact sentence. Success carries the rendered
/// context block plus `activated_skill`; the turn executor (not this module)
/// persists the skill on the conversation.
fn start_planning(db: &Db, cycle_id: &str) -> AppResult<serde_json::Value> {
    let conn = db.pool().get()?;
    let cycle = cycles_repo::require(&conn, cycle_id)?;
    let skill = planning_skill_for_cycle(cycle.cycle_type.as_str())
        .map_err(|message| AppError::validation("unsupported_cycle_type", message))?;
    let context = load_context(&conn, cycle_id)?;
    Ok(json!({
        "activated_skill": skill.as_str(),
        "context": render(&context, None),
        "message": "Planning skill activated.",
    }))
}

/// `start_goal_setting` — activates the goal-clarification skill for one
/// existing task and returns its full detail block (same shape as
/// `get_task_details`). Unknown ids answer `task_not_found`.
fn start_goal_setting(db: &Db, args: &StartGoalSettingArgs) -> AppResult<serde_json::Value> {
    let conn = db.pool().get()?;
    let task = tasks_repo::require(&conn, &args.task_id)?;
    drop(conn);
    Ok(json!({
        "activated_skill": AgentSkill::GoalSetting.as_str(),
        "task": task_details(&task),
        "message": "Goal clarification activated.",
    }))
}

/// `start_review` (add-review-retrospective §5.1–§5.3) — activates the
/// review skill for the bound cycle. Session cycles answer with the stable
/// `unsupported_cycle_type` code and never activate the skill. The result
/// carries the query-derived facts of the cycle (the model quotes them, it
/// never computes them) plus the **most recent** other review's conclusion —
/// one review, never the full history. The turn executor persists the skill
/// on the conversation; this layer does not write `active_skill`.
fn start_review(db: &Db, cycle_id: &str) -> AppResult<serde_json::Value> {
    let conn = db.pool().get()?;
    let cycle = cycles_repo::require(&conn, cycle_id)?;
    if cycle.cycle_type == CycleType::Session {
        return Err(AppError::validation(
            "unsupported_cycle_type",
            "Reviews are not supported for session cycles.",
        ));
    }
    let facts = reviews_service::compute_facts(&conn, cycle_id)?;
    let previous_review = reviews_service::latest_review_context(&conn, cycle_id)?;
    Ok(json!({
        "activated_skill": AgentSkill::Review.as_str(),
        "facts": facts,
        "previous_review": previous_review,
        "message": "Review activated.",
    }))
}

/// `start_prioritization` — activates the prioritization skill and returns
/// the persisted five-bucket document plus `pending_review` recomputed from
/// the cycle's visible tasks (§7.3: bucketed tasks stay put, everything
/// unclaimed becomes pending) and the model-readable render of that state.
fn start_prioritization(db: &Db, cycle_id: &str) -> AppResult<serde_json::Value> {
    let conn = db.pool().get()?;
    let cycle = cycles_repo::require(&conn, cycle_id)?;
    let tasks = tasks_repo::list_visible_by_cycle(&conn, cycle_id)?;
    let existing = prioritization::load_for_cycle(&conn, cycle_id)?.unwrap_or_default();
    let with_candidates = prioritization::candidates_for(&tasks, &existing);
    let rendered = prioritization::render_for_model(
        &with_candidates,
        &titles_of(&tasks),
        &cycle_key_label(&cycle),
    );
    Ok(json!({
        "activated_skill": AgentSkill::Prioritization.as_str(),
        "breakdown": prioritization::to_value(&existing),
        "pending_review": with_candidates.pending_review,
        "rendered": rendered,
        "message": "Prioritization activated.",
    }))
}

// ---------------------------------------------------------------------------
// §5.3 write tools — all through the preview layer (design D3)
// ---------------------------------------------------------------------------

/// `create_goal` — stages a goal in the focused or explicitly identified cycle via
/// [`proposals::apply_upsert_preview`]. The trailing-empty-row reuse (§5.4)
/// lives inside that service function: an empty visible row at the end of the
/// list is snapshotted and replaced instead of appending a new row. Clarity
/// defaults mirror `service::tasks::clarity_defaults` so an agent-created
/// long-term goal starts exactly like a manually typed one (needs
/// refinement / needs breakdown). The result reports success with the new
/// task id — the confirm step belongs to the interface, never to the model
/// (spec: 工具结果对模型报告成功).
fn create_goal(db: &Db, cycle_id: &str, args: &CreateGoalArgs) -> AppResult<serde_json::Value> {
    let cycle_id = args.cycle_id.as_deref().unwrap_or(cycle_id);
    let long_term = {
        let conn = db.pool().get()?;
        cycles_repo::require(&conn, cycle_id)?.cycle_type == CycleType::Month
    };
    let (needs_refinement, needs_breakdown) = if long_term {
        (Some(true), Some(true))
    } else {
        (None, None)
    };
    let input = TaskInput {
        title: args.title.clone(),
        needs_refinement,
        needs_breakdown,
        ..TaskInput::default()
    };
    let mutation = proposals::apply_upsert_preview(db, cycle_id, &input, now_ms())?;
    Ok(json!({
        "status": "proposed",
        "task_id": mutation.value.id,
        "title": mutation.value.title,
    }))
}

/// `update_goal` — stages a title change (or a no-title re-confirm) via
/// [`proposals::apply_update_preview`]. The staged row is the full proposed
/// state, so every other field is carried over from the current row
/// untouched. Errors: `task_not_found`, `invalid_title`, `preview_conflict`
/// (row already staged for deletion), `cycle_ended`, `unsupported_cycle_type`.
fn update_goal(db: &Db, args: &UpdateGoalArgs) -> AppResult<serde_json::Value> {
    if let Some(title) = &args.title {
        if title.trim().is_empty() {
            return Err(AppError::validation(
                "invalid_title",
                "Title cannot be empty",
            ));
        }
    }
    let current = {
        let conn = db.pool().get()?;
        tasks_repo::require(&conn, &args.task_id)?
    };
    let mut input = carry_over_input(&current);
    if let Some(title) = &args.title {
        input.title = title.clone();
    }
    if let Some(completed) = args.completed {
        input.completed = completed;
    }
    if let Some(subtasks) = &args.subtasks {
        input.subtasks = subtasks.clone();
    }
    let mutation = proposals::apply_update_preview(db, &args.task_id, &input)?;
    Ok(json!({"status":"proposed", "task_id":mutation.value.id, "title":mutation.value.title}))
}

/// `delete_goal` — stages a deletion via [`proposals::apply_delete_preview`];
/// the row stays visible (strikethrough) and revertible until the user
/// decides (spec: 未确认的删除). Result reports success; nothing is physically
/// removed at this point.
fn delete_goal(db: &Db, args: &DeleteGoalArgs) -> AppResult<serde_json::Value> {
    proposals::apply_delete_preview(db, &args.task_id)?;
    Ok(json!({ "status": "proposed", "task_id": args.task_id }))
}

/// `update_goal_breakdown` (§6.5) — merges the submitted partial update onto
/// the task's current breakdown ([`breakdown::merge`], which rejects no-ops
/// with `empty_update`) and stages the merged document through
/// [`proposals::apply_update_preview`] — the goal_breakdown column is an
/// ordinary staged field of the full proposed row, so the preview path is the
/// same as for every other task write (D3). Clarity flags are re-derived by
/// the engine, never by the model: `needs_refinement` follows
/// [`breakdown::derive_clarity`], while `needs_breakdown` is deliberately
/// preserved — it may only be cleared after the user confirmed the proposed
/// steps and they were written back (spec: 分解已完成), which no tool call
/// represents. The reply carries the §6.5 `TaskContextSnapshot`.
fn update_goal_breakdown(db: &Db, args: &UpdateGoalBreakdownArgs) -> AppResult<serde_json::Value> {
    let current = {
        let conn = db.pool().get()?;
        tasks_repo::require(&conn, &args.task_id)?
    };
    let base = breakdown::GoalBreakdown::from_value(
        &current
            .goal_breakdown
            .clone()
            .unwrap_or(serde_json::Value::Null),
    );
    let merged = breakdown::merge(&base, &args.update)?;
    let derived = breakdown::derive_clarity(&merged);

    let input = TaskInput {
        needs_refinement: Some(derived.needs_refinement),
        // Preserved on purpose: only a user-confirmed step write-back may
        // clear it (spec: needs_breakdown 清除条件), and that happens in the
        // interface, not through this tool.
        needs_breakdown: current.needs_breakdown,
        goal_breakdown: Some(merged.to_value()),
        ..carry_over_input(&current)
    };
    let mutation = proposals::apply_update_preview(db, &args.task_id, &input)?;
    let missing: Vec<&str> = breakdown::missing_fields(&merged)
        .iter()
        .map(|field| field.field_path())
        .collect();
    Ok(json!({
        "status": "proposed",
        "task_id": args.task_id,
        "task_context_snapshot": {
            "missing_fields": missing,
            "needs_refinement": derived.needs_refinement,
            "needs_breakdown": mutation.value.needs_breakdown,
        },
    }))
}

/// `update_prioritization_breakdown` (§7.4) — validates the per-bucket update
/// against the cycle's visible task ids using [`prioritization::merge`], then
/// stages the typed whole-document action. The cycle row is written only by
/// the existing GUI approval path; no model call can commit the conclusion.
fn update_prioritization_breakdown(
    db: &Db,
    cycle_id: &str,
    args: &UpdatePrioritizationBreakdownArgs,
) -> AppResult<serde_json::Value> {
    let conn = db.pool().get()?;
    let cycle = cycles_repo::require(&conn, cycle_id)?;
    let tasks = tasks_repo::list_visible_by_cycle(&conn, cycle_id)?;
    let candidates: HashSet<String> = tasks.iter().map(|task| task.id.clone()).collect();
    let existing = prioritization::load_for_cycle(&conn, cycle_id)?.unwrap_or_default();
    let merged = prioritization::merge(&existing, &args.update, &candidates)?;
    let rendered =
        prioritization::render_for_model(&merged, &titles_of(&tasks), &cycle_key_label(&cycle));
    let mut result = actions::stage(
        db,
        cycle_id,
        Action::Prioritization(PrioritizationAction::Update {
            cycle_id: cycle_id.to_string(),
            update: args.update.clone(),
        }),
        &args.rationale,
    )?;
    result["breakdown"] = prioritization::to_value(&merged);
    result["rendered"] = json!(rendered);
    Ok(result)
}

// ---------------------------------------------------------------------------
// Tool definitions (assembler input for the request layer)
// ---------------------------------------------------------------------------

fn object_schema(properties: serde_json::Value, required: &[&str]) -> serde_json::Value {
    let mut schema = json!({
        "type": "object",
        "properties": properties,
        "additionalProperties": false,
    });
    if !required.is_empty() {
        schema["required"] = json!(required);
    }
    schema
}

fn def_get_cycle_context() -> ToolDef {
    ToolDef::new(
        "get_cycle_context",
        "Read a cycle context including task IDs and parent/child links. Omit cycle_id for the focused cycle; otherwise use an ID returned by list_cycles.",
        object_schema(
            json!({ "cycle_id": { "type": ["string", "null"], "description": "Omit for the focused cycle, or pass an existing cycle ID" } }),
            &[],
        ),
    )
}

fn def_get_task_details() -> ToolDef {
    ToolDef::new(
        "get_task_details",
        "Read one task in full: every field, its subtasks as a Markdown checklist, \
         the raw goal_breakdown value, the computed missing_fields list and the \
         clarity flags.",
        object_schema(json!({ "task_id": { "type": "string" } }), &["task_id"]),
    )
}

fn def_start_review() -> ToolDef {
    ToolDef::new(
        "start_review",
        "Activate the cycle review for the current cycle: receive its facts \
         (completion, focused time, linked lower-level items, unfinished items) \
         and the most recent previous review's conclusion, then walk the user \
         through the fixed review questions one at a time. Focus blocks \
         (sessions) do not support reviews.",
        object_schema(json!({}), &[]),
    )
}

fn def_create_goal() -> ToolDef {
    ToolDef::new(
        "create_goal",
        "Propose a new goal in the focused or explicitly identified cycle. Report it as pending GUI confirmation. An empty \
         trailing row in the list is reused instead of appending.",
        object_schema(
            json!({
                "cycle_id": { "type": ["string", "null"], "description": "Omit for the focused cycle, or pass an existing cycle ID from list_cycles" },
                "title": { "type": "string", "description": "the goal's title, in the user's language" },
                "rationale": { "type": "string", "description": "why this goal, naming what the user said" },
            }),
            &["title", "rationale"],
        ),
    )
}

fn def_update_goal() -> ToolDef {
    let mut def = ToolDef::new(
        "update_goal",
        "Propose title, completion state or a replacement checklist. Omitted fields are preserved. Read task details first. Report pending GUI confirmation, not an applied change.",
        object_schema(
            json!({
                "task_id": { "type": "string" },
                "title": { "type": "string", "minLength":1, "description": "Refined title; omit to preserve" },
                "completed": {"type":"boolean", "description":"Explicit desired completion state; never a toggle"},
                "subtasks": {"type":"array","maxItems":100,"items":{"$ref":"#/$defs/subtask"},"description":"Full replacement checklist; [] clears. Read details before editing."},
                "rationale": { "type": "string", "description": "what prompted the rename" },
            }),
            &["task_id", "rationale"],
        ),
    );
    def.input_schema["$defs"] = json!({"subtask":{"type":"object","properties":{"title":{"type":"string"},"completed":{"type":"boolean"},"children":{"type":"array","maxItems":100,"items":{"$ref":"#/$defs/subtask"}}},"required":["title","completed"],"additionalProperties":false}});
    def
}

fn def_delete_goal() -> ToolDef {
    ToolDef::new(
        "delete_goal",
        "Propose removing a goal from the current cycle. The row stays visible and \
         revertible until the user confirms; report the proposal as done.",
        object_schema(
            json!({
                "task_id": { "type": "string" },
                "rationale": { "type": "string", "description": "why the goal should go" },
            }),
            &["task_id", "rationale"],
        ),
    )
}

fn def_update_goal_breakdown() -> ToolDef {
    ToolDef::new(
        "update_goal_breakdown",
        "Write collected goal facts into a task's structured breakdown. Send only \
         the fields that change: a value sets it, null clears it, omitted fields \
         keep their value. An update that addresses no field is rejected. Never \
         invent values — everything must come from the user's answers.",
        object_schema(
            json!({
                "task_id": { "type": "string" },
                "update": {
                    "type": "object",
                    "properties": {
                        "context": { "type": ["object", "null"], "description": "{clarification?, background?, stakeholders?} or null to clear the part" },
                        "output": { "type": ["object", "null"], "description": "{value?} or null" },
                        "outcome": { "type": ["object", "null"], "description": "{value?, verification_method?, controlled_by_user?} or null" },
                        "scope": { "type": ["object", "null"], "description": "{effort?, fully_decomposed?} or null" },
                    },
                },
                "rationale": { "type": "string", "description": "which user answers this records" },
            }),
            &["task_id", "update", "rationale"],
        ),
    )
}

fn def_update_prioritization_breakdown() -> ToolDef {
    ToolDef::new(
        "update_prioritization_breakdown",
        "Propose prioritization conclusions for GUI approval. Send only the \
         buckets that change — omitted buckets keep their content, an empty \
         array clears one. Every placed task needs a reason in the user's own \
         words; never invent one. The result is pending until the user confirms \
         it in Coach.",
        object_schema(
            json!({
                "update": {
                    "type": "object",
                    "properties": {
                        "big_wins": { "type": "array", "items": { "type": "object", "required": ["task_id", "reason"], "properties": { "task_id": { "type": "string" }, "reason": { "type": "string" } } } },
                        "bottlenecks": { "type": "array", "items": { "type": "object", "required": ["task_id", "reason"], "properties": { "task_id": { "type": "string" }, "reason": { "type": "string" } } } },
                        "non_negotiables": { "type": "array", "items": { "type": "object", "required": ["task_id", "reason"], "properties": { "task_id": { "type": "string" }, "reason": { "type": "string" } } } },
                        "deprioritized": { "type": "array", "items": { "type": "object", "required": ["task_id", "reason"], "properties": { "task_id": { "type": "string" }, "reason": { "type": "string" } } } },
                        "pending_review": { "type": "array", "items": { "type": "string" } },
                    },
                },
                "rationale": { "type": "string", "description": "the sorting decision this records" },
            }),
            &["update", "rationale"],
        ),
    )
}

// ---------------------------------------------------------------------------
// Tests (§5.7): one happy path per tool plus the contract edges, all offline
// against a throwaway database (same fixture style as `agent/turn.rs`).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::proposal::ProposalKind;
    use crate::repository::proposals as proposals_repo;
    use crate::service::cycles::{
        add_session, create_planning_cycle, finish_cycle, AddSessionArgs, CreateCycleArgs,
    };
    use crate::service::tasks::{add_task, AddTaskArgs};

    fn db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = crate::db::open_at(&dir.path().join("test.db")).expect("open db");
        (dir, db)
    }

    fn call(name: &str, arguments: serde_json::Value) -> ToolCallRecord {
        ToolCallRecord {
            id: format!("call-{name}"),
            name: name.to_string(),
            arguments,
        }
    }

    fn run(db: &Db, cycle_id: &str, name: &str, arguments: serde_json::Value) -> ToolOutcome {
        ToolRegistry.execute(db, cycle_id, &call(name, arguments))
    }

    fn error_code(outcome: &ToolOutcome) -> String {
        assert!(outcome.is_error, "expected an error result: {outcome:?}");
        outcome.result["code"]
            .as_str()
            .expect("error results carry a code")
            .to_string()
    }

    fn today() -> chrono::NaiveDate {
        crate::domain::calendar::today_local()
    }

    fn month_cycle(db: &Db) -> String {
        month_cycle_of(db, 1)
    }

    /// Month cycles are calendar-identified (`long_term_key(start, end)` is
    /// UNIQUE), so a second long-term cycle in the same database needs a
    /// different duration (1 / 3 / 6 product months) to coexist with the
    /// first — the move tests rely on that for a source/target pair.
    fn month_cycle_of(db: &Db, months: i64) -> String {
        create_planning_cycle(
            db,
            &CreateCycleArgs {
                cycle_type: "month".into(),
                duration_months: Some(months),
                ..Default::default()
            },
            today(),
            1,
        )
        .expect("month cycle")
        .value
        .id
    }

    fn week_cycle(db: &Db, parent: &str) -> String {
        create_planning_cycle(
            db,
            &CreateCycleArgs {
                cycle_type: "week".into(),
                parent_id: Some(parent.into()),
                ..Default::default()
            },
            today(),
            1,
        )
        .expect("week cycle")
        .value
        .id
    }

    fn day_cycle(db: &Db, parent: &str) -> String {
        create_planning_cycle(
            db,
            &CreateCycleArgs {
                cycle_type: "day".into(),
                parent_id: Some(parent.into()),
                ..Default::default()
            },
            today(),
            1,
        )
        .expect("day cycle")
        .value
        .id
    }

    fn session_cycle(db: &Db) -> String {
        let month = month_cycle(db);
        let week = week_cycle(db, &month);
        let day = day_cycle(db, &week);
        add_session(
            db,
            &AddSessionArgs {
                day_cycle_id: day,
                title: "focus".into(),
                ..Default::default()
            },
            1,
        )
        .expect("session")
        .value
        .id
    }

    fn task(db: &Db, cycle_id: &str, title: &str) -> String {
        add_task(
            db,
            &AddTaskArgs {
                cycle_id: cycle_id.into(),
                title: title.into(),
                ..Default::default()
            },
            1,
        )
        .expect("task")
        .value
        .id
    }

    fn visible_titles(db: &Db, cycle_id: &str) -> Vec<String> {
        let conn = db.pool().get().unwrap();
        tasks_repo::list_visible_by_cycle(&conn, cycle_id)
            .unwrap()
            .into_iter()
            .map(|task| task.title)
            .collect()
    }

    fn row(db: &Db, task_id: &str) -> Task {
        let conn = db.pool().get().unwrap();
        tasks_repo::require(&conn, task_id).unwrap()
    }

    // -- 5.1 read tools ------------------------------------------------------

    #[test]
    fn get_cycle_context_defaults_to_focus_and_accepts_explicit_ids() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db);

        let outcome = run(&db, &cycle, "get_cycle_context", json!({}));
        assert!(!outcome.is_error);
        assert_eq!(outcome.activated_skill, None);
        let context = outcome.result["context"].as_str().unwrap();
        assert!(context.starts_with("<context>"));
        assert!(context.contains("<cycle_type>long_term</cycle_type>"));
        assert!(context.contains("<current_date_and_time>"));

        // The "current" sentinel behaves like omission.
        let outcome = run(
            &db,
            &cycle,
            "get_cycle_context",
            json!({ "cycle_id": cycle }),
        );
        assert!(!outcome.is_error);

        // Any other key is a documented tool error, not a wrong-cycle read.
        let outcome = run(
            &db,
            &cycle,
            "get_cycle_context",
            json!({ "cycle_id": "week:2026-W38" }),
        );
        assert_eq!(error_code(&outcome), "cycle_not_found");
    }

    #[test]
    fn get_task_details_reports_fields_and_missing_list() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db);
        let task_id = task(&db, &cycle, "Learn to sail");

        let outcome = run(
            &db,
            &cycle,
            "get_task_details",
            json!({ "task_id": task_id }),
        );
        assert!(!outcome.is_error);
        assert_eq!(outcome.result["title"], "Learn to sail");
        assert_eq!(outcome.result["cycle_id"], cycle.as_str());
        assert_eq!(outcome.result["goal_breakdown"], serde_json::Value::Null);
        assert_eq!(outcome.result["subtasks_markdown"], "");
        let missing = outcome.result["missing_fields"].as_array().unwrap();
        for expected in [
            "context.clarification",
            "output.value",
            "outcome.value",
            "outcome.verification_method",
        ] {
            assert!(
                missing.iter().any(|value| value == expected),
                "missing must list {expected}"
            );
        }
        assert_eq!(outcome.result["needs_refinement"], serde_json::json!(true));
        assert_eq!(outcome.result["needs_breakdown"], serde_json::json!(true));

        let outcome = run(
            &db,
            &cycle,
            "get_task_details",
            json!({ "task_id": "ghost" }),
        );
        assert_eq!(error_code(&outcome), "task_not_found");
    }

    // -- 5.2 activation tools ------------------------------------------------

    #[test]
    fn start_planning_dispatches_by_cycle_type() {
        let (_dir, db) = db();
        let month = month_cycle(&db);
        let outcome = run(&db, &month, "start_planning", json!({}));
        assert!(!outcome.is_error);
        assert_eq!(outcome.activated_skill, Some(AgentSkill::LongTermPlanning));
        assert_eq!(outcome.result["activated_skill"], "long_term_planning");
        assert!(outcome.result["context"]
            .as_str()
            .unwrap()
            .contains("<context>"));

        let week = week_cycle(&db, &month);
        let outcome = run(&db, &week, "start_planning", json!({}));
        assert_eq!(outcome.activated_skill, Some(AgentSkill::WeeklyPlanning));

        let day = day_cycle(&db, &week);
        let outcome = run(&db, &day, "start_planning", json!({}));
        assert_eq!(outcome.activated_skill, Some(AgentSkill::DailyPlanning));
    }

    #[test]
    fn start_planning_rejects_session_cycles_with_the_spec_sentence() {
        let (_dir, db) = db();
        let session = session_cycle(&db);
        let outcome = run(&db, &session, "start_planning", json!({}));
        assert_eq!(error_code(&outcome), "unsupported_cycle_type");
        assert!(
            outcome.result["error"]
                .as_str()
                .unwrap()
                .contains("Agent mutations are not supported for session cycles."),
            "spec error text missing: {outcome:?}"
        );
        assert_eq!(outcome.activated_skill, None);
    }

    #[test]
    fn start_goal_setting_returns_details_and_rejects_unknown_tasks() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db);
        let task_id = task(&db, &cycle, "Vague wish");

        let outcome = run(
            &db,
            &cycle,
            "start_goal_setting",
            json!({ "task_id": task_id }),
        );
        assert!(!outcome.is_error);
        assert_eq!(outcome.activated_skill, Some(AgentSkill::GoalSetting));
        assert_eq!(outcome.result["activated_skill"], "goal_setting");
        assert_eq!(outcome.result["task"]["title"], "Vague wish");
        assert!(
            outcome.result["task"]["missing_fields"]
                .as_array()
                .unwrap()
                .len()
                >= 4
        );

        let outcome = run(
            &db,
            &cycle,
            "start_goal_setting",
            json!({ "task_id": "ghost" }),
        );
        assert_eq!(error_code(&outcome), "task_not_found");
    }

    #[test]
    fn start_prioritization_returns_breakdown_and_recomputed_pending() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db);
        let placed = task(&db, &cycle, "already sorted");
        let pending_a = task(&db, &cycle, "pending a");
        let pending_b = task(&db, &cycle, "pending b");
        let mut existing = prioritization::PrioritizationBreakdown::default();
        existing.big_wins.push(prioritization::BucketItem::new(
            placed.clone(),
            "用户说这是最重要的",
        ));
        prioritization::store_for_cycle(&db.pool().get().unwrap(), &cycle, &existing).unwrap();

        let outcome = run(&db, &cycle, "start_prioritization", json!({}));
        assert!(!outcome.is_error);
        assert_eq!(outcome.activated_skill, Some(AgentSkill::Prioritization));
        assert_eq!(
            outcome.result["breakdown"]["big_wins"][0]["reason"],
            "用户说这是最重要的"
        );
        let pending = outcome.result["pending_review"].as_array().unwrap();
        let pending: Vec<&str> = pending.iter().map(|id| id.as_str().unwrap()).collect();
        assert_eq!(pending, vec![pending_a.as_str(), pending_b.as_str()]);
        assert!(outcome.result["rendered"]
            .as_str()
            .unwrap()
            .contains("<prioritization"));
    }

    // -- 5.3/5.4 write tools: every assertion checks the preview layer -------

    #[test]
    fn create_goal_stages_a_preview_and_never_touches_committed_data() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db);

        let outcome = run(
            &db,
            &cycle,
            "create_goal",
            json!({ "title": "Ship the beta", "rationale": "用户说这个季度必须发出测试版" }),
        );
        assert!(!outcome.is_error);
        assert_eq!(outcome.result["status"], "proposed");
        let task_id = outcome.result["task_id"].as_str().unwrap().to_string();

        // Committed view sees nothing; the staged row is proposal-only.
        assert!(visible_titles(&db, &cycle).is_empty(), "no committed row");
        let staged = row(&db, &task_id);
        assert_eq!(staged.title, "Ship the beta");
        assert_eq!(staged.proposal, Some(ProposalKind::Upsert));
        assert_eq!(staged.needs_refinement, Some(true), "long-term default");
        assert_eq!(staged.needs_breakdown, Some(true));

        // The snapshot marks the row as agent-created (revert = delete).
        let conn = db.pool().get().unwrap();
        let snapshot = proposals_repo::get_snapshot(&conn, &task_id)
            .unwrap()
            .unwrap();
        assert!(!snapshot.original_exists);
    }

    #[test]
    fn create_goal_reuses_a_trailing_empty_row() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db);
        let empty_id = task(&db, &cycle, "  ");

        let outcome = run(
            &db,
            &cycle,
            "create_goal",
            json!({ "title": "Renewed goal", "rationale": "用户补充了目标描述" }),
        );
        assert!(!outcome.is_error);
        assert_eq!(outcome.result["task_id"], empty_id.as_str(), "row reused");
        let staged = row(&db, &empty_id);
        assert_eq!(staged.title, "Renewed goal");
        assert_eq!(staged.proposal, Some(ProposalKind::Upsert));
        // The pre-existing empty row is what revert restores.
        let conn = db.pool().get().unwrap();
        let snapshot = proposals_repo::get_snapshot(&conn, &empty_id)
            .unwrap()
            .unwrap();
        assert!(snapshot.original_exists);
        assert_eq!(snapshot.title.as_deref(), Some("  "));
    }

    #[test]
    fn update_goal_stages_only_in_preview() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db);
        let task_id = task(&db, &cycle, "Old title");

        let outcome = run(
            &db,
            &cycle,
            "update_goal",
            json!({ "task_id": task_id, "title": "Sharper title", "rationale": "用户明确了产出" }),
        );
        assert!(!outcome.is_error);
        assert_eq!(outcome.result["status"], "proposed");

        // The staged row leaves the committed view until the user keeps it.
        assert!(visible_titles(&db, &cycle).is_empty(), "no committed row");
        let staged = row(&db, &task_id);
        assert_eq!(staged.title, "Sharper title");
        assert_eq!(staged.proposal, Some(ProposalKind::Upsert));
        let conn = db.pool().get().unwrap();
        let snapshot = proposals_repo::get_snapshot(&conn, &task_id)
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.title.as_deref(), Some("Old title"));

        // A title-only call that omits `title` keeps the current one.
        let outcome = run(
            &db,
            &cycle,
            "update_goal",
            json!({ "task_id": task_id, "rationale": "re-confirm" }),
        );
        assert!(!outcome.is_error);
        assert_eq!(row(&db, &task_id).title, "Sharper title");

        // Empty titles are rejected like the manual editor's patch.
        let outcome = run(
            &db,
            &cycle,
            "update_goal",
            json!({ "task_id": task_id, "title": "   ", "rationale": "nope" }),
        );
        assert_eq!(error_code(&outcome), "invalid_title");

        let outcome = run(
            &db,
            &cycle,
            "update_goal",
            json!({ "task_id": "ghost", "title": "x", "rationale": "r" }),
        );
        assert_eq!(error_code(&outcome), "task_not_found");
    }

    #[test]
    fn delete_goal_stages_a_revertible_delete() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db);
        let task_id = task(&db, &cycle, "Doomed goal");

        let outcome = run(
            &db,
            &cycle,
            "delete_goal",
            json!({ "task_id": task_id, "rationale": "用户说这个目标已经不重要了" }),
        );
        assert!(!outcome.is_error);
        assert_eq!(outcome.result["status"], "proposed");

        assert!(visible_titles(&db, &cycle).is_empty(), "hidden from view");
        let staged = row(&db, &task_id);
        assert_eq!(staged.proposal, Some(ProposalKind::Delete), "row survives");
        let conn = db.pool().get().unwrap();
        let snapshot = proposals_repo::get_snapshot(&conn, &task_id)
            .unwrap()
            .unwrap();
        assert_eq!(snapshot.title.as_deref(), Some("Doomed goal"));
    }

    // -- update_goal_breakdown (6.5) ------------------------------------------

    #[test]
    fn update_goal_breakdown_merges_and_reports_the_snapshot() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db);
        let task_id = task(&db, &cycle, "Fuzzy goal");

        let outcome = run(
            &db,
            &cycle,
            "update_goal_breakdown",
            json!({
                "task_id": task_id,
                "update": {
                    "context": { "clarification": "为什么现在学" },
                    "output": { "value": "一份可执行的训练计划" }
                },
                "rationale": "用户的两个回答",
            }),
        );
        assert!(!outcome.is_error);
        assert_eq!(outcome.result["status"], "proposed");

        let snapshot = &outcome.result["task_context_snapshot"];
        let missing = snapshot["missing_fields"].as_array().unwrap();
        let missing: Vec<&str> = missing.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(!missing.contains(&"context.clarification"));
        assert!(!missing.contains(&"output.value"));
        assert!(missing.contains(&"outcome.value"));
        assert!(missing.contains(&"outcome.verification_method"));
        // No outcome yet → the title is not ready → refinement stays needed.
        assert_eq!(snapshot["needs_refinement"], serde_json::json!(true));
        assert_eq!(snapshot["needs_breakdown"], serde_json::json!(true));

        // The merged document reached the staged row, other fields intact.
        let staged = row(&db, &task_id);
        assert_eq!(staged.proposal, Some(ProposalKind::Upsert));
        let breakdown_value = staged.goal_breakdown.expect("breakdown staged");
        assert_eq!(breakdown_value["context"]["clarification"], "为什么现在学");
        assert_eq!(breakdown_value["output"]["value"], "一份可执行的训练计划");
        assert!(staged.needs_refinement == Some(true));

        // A second update keeps the first one's fields (merge semantics).
        let outcome = run(
            &db,
            &cycle,
            "update_goal_breakdown",
            json!({
                "task_id": task_id,
                "update": { "outcome": { "value": "能独立完成一次短航" } },
                "rationale": "用户补充了结果",
            }),
        );
        assert!(!outcome.is_error);
        let staged = row(&db, &task_id);
        let breakdown_value = staged.goal_breakdown.unwrap();
        assert_eq!(breakdown_value["context"]["clarification"], "为什么现在学");
        assert_eq!(breakdown_value["outcome"]["value"], "能独立完成一次短航");

        // Empty updates pass the engine's rejection straight through.
        let outcome = run(
            &db,
            &cycle,
            "update_goal_breakdown",
            json!({ "task_id": task_id, "update": {}, "rationale": "nothing" }),
        );
        assert_eq!(error_code(&outcome), "empty_update");
    }

    // -- update_prioritization_breakdown (7.4) --------------------------------

    #[tokio::test]
    async fn update_prioritization_breakdown_requires_approval_and_reject_preserves_state() {
        let (dir, db) = db();
        let cycle = month_cycle(&db);
        let t1 = task(&db, &cycle, "first");
        let t2 = task(&db, &cycle, "second");

        let outcome = run(
            &db,
            &cycle,
            "update_prioritization_breakdown",
            json!({
                "update": { "big_wins": [ { "task_id": t1, "reason": "用户说这是最能改变现状的一件事" } ] },
                "rationale": "第一轮排序",
            }),
        );
        assert!(!outcome.is_error);
        assert_eq!(outcome.result["status"], "proposed");
        let action_id = outcome.result["action_id"].as_str().unwrap();
        assert_eq!(
            outcome.result["breakdown"]["big_wins"][0]["task_id"],
            t1.as_str()
        );
        assert!(outcome.result["rendered"]
            .as_str()
            .unwrap()
            .contains("big_wins"));

        // A direct model tool call only stages a typed action; it cannot write
        // the whole-cycle document before the user confirms it.
        let conn = db.pool().get().unwrap();
        let stored = prioritization::load_for_cycle(&conn, &cycle).unwrap();
        assert!(
            stored.is_none(),
            "model call must not persist prioritization"
        );
        drop(conn);

        // Rejecting the pending action leaves the committed document empty.
        crate::ai::actions::claim(&db, &cycle, action_id, false).unwrap();
        let conn = db.pool().get().unwrap();
        assert!(prioritization::load_for_cycle(&conn, &cycle)
            .unwrap()
            .is_none());
        drop(conn);

        // A new valid proposal can be approved through the same generic
        // action path; only that path writes the cycle row.
        let outcome = run(
            &db,
            &cycle,
            "update_prioritization_breakdown",
            json!({
                "update": { "big_wins": [ { "task_id": t1, "reason": "用户说这是最能改变现状的一件事" } ] },
                "rationale": "第一轮排序",
            }),
        );
        let action_id = outcome.result["action_id"].as_str().unwrap();
        let item = crate::ai::actions::claim(&db, &cycle, action_id, true).unwrap();
        let ai = crate::providers::service::AiSettingsState::load(dir.path()).unwrap();
        crate::ai::actions::apply(&db, &ai, &item.action)
            .await
            .unwrap();
        crate::ai::actions::finish(&db, action_id, true).unwrap();
        let conn = db.pool().get().unwrap();
        let stored = prioritization::load_for_cycle(&conn, &cycle)
            .unwrap()
            .unwrap();
        assert_eq!(stored.big_wins.len(), 1);
        assert_eq!(stored.big_wins[0].reason, "用户说这是最能改变现状的一件事");
        drop(conn);
        // Task data is untouched by the prioritization write.
        assert_eq!(row(&db, &t1).proposal, None);

        // A rejected valid proposal preserves the approved document. An empty
        // reason is rejected before an action is staged at all.
        let outcome = run(
            &db,
            &cycle,
            "update_prioritization_breakdown",
            json!({
                "update": { "bottlenecks": [ { "task_id": t2, "reason": "用户说这会堵住交付" } ] },
                "rationale": "补充瓶颈",
            }),
        );
        assert!(!outcome.is_error);
        let action_id = outcome.result["action_id"].as_str().unwrap();
        crate::ai::actions::claim(&db, &cycle, action_id, false).unwrap();
        let conn = db.pool().get().unwrap();
        let stored = prioritization::load_for_cycle(&conn, &cycle)
            .unwrap()
            .unwrap();
        assert!(
            stored.bottlenecks.is_empty(),
            "reject must preserve approved state"
        );
        drop(conn);

        // An empty reason remains a validation error and must not create an
        // action or alter the approved document.
        let outcome = run(
            &db,
            &cycle,
            "update_prioritization_breakdown",
            json!({
                "update": { "bottlenecks": [ { "task_id": t2, "reason": "   " } ] },
                "rationale": "bad round",
            }),
        );
        assert_eq!(error_code(&outcome), "missing_reason");
        assert!(crate::ai::actions::list(&db, &cycle).unwrap().is_empty());
    }

    // -- start_review (add-review-retrospective §5) ----------------------------

    #[test]
    fn start_review_activates_the_review_skill_with_facts_and_latest_conclusion() {
        let (_dir, db) = db();
        let older = month_cycle(&db);
        let current = month_cycle_of(&db, 3);
        task(&db, &current, "current item");
        let older_task = task(&db, &older, "older item");
        crate::service::tasks::patch_task(
            &db,
            &older_task,
            &crate::service::tasks::TaskPatch {
                completed: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        // An older review (not the one under review) plus an even older one:
        // the context must carry only the most recent other review.
        let oldest = month_cycle_of(&db, 6);
        crate::service::reviews::save_cycle_review(
            &db,
            &crate::service::reviews::SaveReviewArgs {
                cycle_id: oldest,
                answers: vec![],
            },
            100,
        )
        .unwrap();
        crate::service::reviews::save_cycle_review(
            &db,
            &crate::service::reviews::SaveReviewArgs {
                cycle_id: older.clone(),
                answers: vec![crate::service::reviews::ReviewAnswerInput {
                    id: "what_went_well".into(),
                    status: "answered".into(),
                    text: "steady shipping".into(),
                }],
            },
            200,
        )
        .unwrap();

        let outcome = run(&db, &current, "start_review", json!({}));
        assert!(!outcome.is_error);
        assert_eq!(outcome.activated_skill, Some(AgentSkill::Review));
        assert_eq!(outcome.result["activated_skill"], "review");

        // Facts of the bound cycle, query-derived.
        let facts = &outcome.result["facts"];
        assert_eq!(facts["cycle_id"], current.as_str());
        assert_eq!(facts["total_items"], serde_json::json!(1));
        assert_eq!(facts["completed_items"], serde_json::json!(0));
        assert_eq!(facts["incomplete"][0]["title"], "current item");

        // Exactly one previous review — the most recent one, not the history.
        let previous = &outcome.result["previous_review"];
        assert_eq!(previous["cycle_id"], older.as_str());
        let answers = previous["answers"].as_array().unwrap();
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0]["text"], "steady shipping");
    }

    #[test]
    fn start_review_has_no_previous_review_on_the_first_cycle() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db);
        let outcome = run(&db, &cycle, "start_review", json!({}));
        assert!(!outcome.is_error);
        assert!(outcome.result["previous_review"].is_null());
        assert_eq!(
            outcome.result["facts"]["has_content"],
            serde_json::json!(false)
        );
    }

    #[test]
    fn start_review_rejects_session_cycles_without_activating() {
        let (_dir, db) = db();
        let session = session_cycle(&db);
        let outcome = run(&db, &session, "start_review", json!({}));
        assert_eq!(error_code(&outcome), "unsupported_cycle_type");
        assert_eq!(outcome.activated_skill, None);
        assert!(outcome.result["error"]
            .as_str()
            .unwrap()
            .contains("session"));
    }

    #[test]
    fn start_review_arguments_are_strict() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db);
        let outcome = run(&db, &cycle, "start_review", json!({ "cycle_id": "x" }));
        assert_eq!(error_code(&outcome), "invalid_arguments");
    }

    // -- 5.6 strict arguments --------------------------------------------------

    #[test]
    fn strict_arguments_reject_unknown_fields_and_type_mismatches() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db);

        // Unknown top-level field → tool error, not a silent ignore.
        let outcome = run(
            &db,
            &cycle,
            "create_goal",
            json!({ "title": "x", "rationale": "r", "due_date": "2026-10-01" }),
        );
        assert_eq!(error_code(&outcome), "invalid_arguments");
        assert!(outcome.result["error"]
            .as_str()
            .unwrap()
            .contains("unknown field"));

        // Wrong type → tool error.
        let outcome = run(&db, &cycle, "get_task_details", json!({ "task_id": 123 }));
        assert_eq!(error_code(&outcome), "invalid_arguments");

        // Missing required field → tool error.
        let outcome = run(&db, &cycle, "delete_goal", json!({ "task_id": "t" }));
        assert_eq!(error_code(&outcome), "invalid_arguments");

        // Nested breakdown type mismatch → tool error.
        let task_id = task(&db, &cycle, "t");
        let outcome = run(
            &db,
            &cycle,
            "update_goal_breakdown",
            json!({ "task_id": task_id, "update": { "output": { "value": 42 } }, "rationale": "r" }),
        );
        assert_eq!(error_code(&outcome), "invalid_arguments");

        // Unknown tool names are tool errors too.
        let outcome = run(&db, &cycle, "format_disk", json!({}));
        assert_eq!(error_code(&outcome), "unknown_tool");
    }

    #[test]
    fn model_loads_workflows_on_demand_and_analysis_tools_are_read_only() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db);
        let outcome = run(&db, &cycle, "load_skill", json!({"name":"period-analysis"}));
        assert_eq!(outcome.activated_skill, Some(AgentSkill::PeriodAnalysis));
        assert!(outcome.result["instructions"]
            .as_str()
            .unwrap()
            .contains("read-only"));
        let outcome = run(&db, &cycle, "load_skill", json!({"name":"daily-planning"}));
        assert_eq!(outcome.activated_skill, Some(AgentSkill::DailyPlanning));
        let bad = run(&db, &cycle, "load_skill", json!({"name":"../../outside"}));
        assert!(bad.is_error);
        assert!(bad.activated_skill.is_none());
        for skill in [AgentSkill::PeriodAnalysis, AgentSkill::PlanningIssues] {
            let tools = ToolRegistry.definitions(skill, true);
            assert!(tools.iter().any(|t| t.name == "load_skill"));
            assert!(!tools
                .iter()
                .any(|t| t.name == "create_goal" || t.name == "delete_goal"));
        }
    }

    // -- definitions ------------------------------------------------------------

    #[test]
    fn definitions_match_the_skill_state() {
        let registry = ToolRegistry;
        let skills = [
            AgentSkill::None,
            AgentSkill::GoalSetting,
            AgentSkill::LongTermPlanning,
            AgentSkill::ShortTermPlanning,
            AgentSkill::WeeklyPlanning,
            AgentSkill::DailyPlanning,
            AgentSkill::Prioritization,
            AgentSkill::Review,
            AgentSkill::PeriodAnalysis,
            AgentSkill::PlanningIssues,
        ];
        let mut all = std::collections::BTreeSet::new();
        for skill in skills {
            let defs = registry.definitions(skill, true);
            assert!(defs.len() <= 19);
            assert!(registry.definitions(skill, false).is_empty());
            let names: Vec<_> = defs.iter().map(|d| d.name.as_str()).collect();
            assert!(names.contains(&"load_skill"));
            assert!(!names.contains(&"move_goal"));
            assert!(!names.contains(&"resolve_agent_action"));
            assert_eq!(
                names.contains(&"propose_settings"),
                skill == AgentSkill::None
            );
            if matches!(
                skill,
                AgentSkill::PeriodAnalysis | AgentSkill::PlanningIssues
            ) {
                assert!(!names.iter().any(|n| n.starts_with("propose_")
                    || n.starts_with("update_")
                    || n.starts_with("create_")
                    || n.starts_with("delete_")));
            }
            for def in defs {
                assert!(!def.description.is_empty());
                assert_eq!(def.input_schema["type"], "object");
                assert_eq!(def.input_schema["additionalProperties"], false);
                all.insert(def.name);
            }
        }
        assert_eq!(all.len(), 24);
    }

    // -- write tools respect ended cycles via the service rule -----------------

    #[test]
    fn write_tools_refuse_ended_cycles_through_the_service_rule() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db);
        finish_cycle(&db, &cycle, {
            // A NotStarted cycle cannot finish through the lifecycle; put
            // it into Started first so finish_cycle marks it ended.
            let conn = db.pool().get().unwrap();
            crate::repository::cycles::set_lifecycle(&conn, &cycle, true, false, Some(1), None)
                .unwrap();
            crate::service::now_ms()
        })
        .unwrap();

        let outcome = run(
            &db,
            &cycle,
            "create_goal",
            json!({ "title": "late", "rationale": "r" }),
        );
        assert_eq!(error_code(&outcome), "cycle_ended");
    }
}

fn def_load_skill() -> ToolDef {
    ToolDef::new("load_skill", "Load the workflow that fits the user's current request. You may switch skills during a conversation. This changes the following turn's instructions and tools, not the selected plan or any task.", json!({"type":"object", "properties":{"name":{"type":"string", "enum":["coach","goal-clarification","long-term-planning","weekly-planning","daily-planning","prioritization","cycle-review","period-analysis","planning-issues"]}}, "required":["name"], "additionalProperties":false}))
}
fn def_get_period_context() -> ToolDef {
    ToolDef::new("get_period_context", "Read exact inclusive calendar dates for a quarter, half-year, year or custom period, across all plans including archived ones. Ask first when boundaries are ambiguous. Facts are current snapshots, not past completion events. No plans are changed.", json!({"type":"object", "properties":{"start_date":{"type":"string","description":"YYYY-MM-DD inclusive"},"end_date":{"type":"string","description":"YYYY-MM-DD inclusive"},"question":{"type":"string","description":"The user's requested analysis dimensions"}},"required":["start_date","end_date","question"],"additionalProperties":false}))
}
fn def_get_planning_issues() -> ToolDef {
    ToolDef::new("get_planning_issues", "Read deterministic issues in the focused plan. Follow with context-based diagnosis using the planning-issues skill; do not score the user.", json!({"type":"object","properties":{},"additionalProperties":false}))
}

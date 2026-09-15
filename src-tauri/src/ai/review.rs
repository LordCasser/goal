//! Planning review — 边写边审 (add-ai-planning-core §8, spec:
//! `openspec/specs/planning-issues/spec.md`).
//!
//! The system inspects the plan *while* the user edits it and surfaces
//! concrete, actionable issues — never a score (spec: 诊断而非评分). Six
//! issue types, reported per cycle and optionally per task:
//!
//! | type | layer | trigger |
//! | --- | --- | --- |
//! | `too_many_goals` | structure | Long-term cycle holds more than [`MAX_GOALS_PER_LONG_TERM_CYCLE`] goals |
//! | `too_many_tasks` | structure | week/day cycle holds more than [`MAX_TASKS_PER_CYCLE`] items |
//! | `too_much_work` | structure | day cycle has more than [`MAX_UNCOMPLETED_DAY_TASKS`] uncompleted tasks (a proxy for "estimated work exceeds one day") |
//! | `not_sure_what_to_do_next` | structure | task flagged `needs_breakdown`, or a goal without any subtask/child step |
//! | `missing_something` | structure | goal breakdown has missing fields (reuses the [`crate::ai::breakdown`] engine) |
//! | `not_useful_for_needs` | semantic | the LLM judges a still-ambiguous goal unclear for the user's needs |
//!
//! Layering (design D5):
//!
//! 1. [`review_cycle`] — deterministic structure checks only: no LLM, no
//!    errors, unit-testable.
//! 2. [`review_cycle_semantic`] — the optional LLM layer; any provider or
//!    parse failure degrades to an empty report (spec: 审查不可用 → 静默跳过).
//! 3. [`review_cached`] — composes both behind the content-hash cache.
//!
//! **规范红线**: review must never block or break the editing flow —
//! [`review_cycle`] has no `Err` path (internal failures log at debug level
//! and yield an empty report), issues never gate any write, and the review
//! result is an idempotent replacement, not an accumulation.
//!
//! Debouncing is the frontend's rhythm: the caller stops typing for
//! [`DEBOUNCE_HINT_MS`] before invoking [`review_cached`]; the backend cache
//! merely makes a repeated call cheap. The cache is keyed by
//! `(cycle_id, content_hash)` so unchanged content never re-runs anything.
//!
//! Usability signals (spec: 收集产品自身的可用性信号): the `reason` of a
//! dismissal doubles as a free-form signal ("Planning felt like too much
//! work", "Not sure how to use it", …). It is persisted with the dismissal
//! row and can be exported alongside user feedback later — the app itself
//! has no reporting channel.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::ai::breakdown::{self, GoalBreakdown, MissingField};
use crate::ai::llm::{LlmProvider, LlmRequest, ResolvedProvider};
use crate::db::Db;
use crate::domain::cycle::{Cycle, CycleType};
use crate::domain::task::{is_empty_input_row, Task};
use crate::error::{from_rusqlite, AppError, AppResult};
use crate::logging;

const MODULE: &str = "ai::review";

// ---------------------------------------------------------------------------
// Thresholds — first-version heuristics, deliberately blunt
// ---------------------------------------------------------------------------

/// 首版启发值：一个 Long-term（month）周期同时推进的目标上限。超过即为
/// `too_many_goals`。不是承诺，只是首版里"明显塞太多"的分界线。
pub const MAX_GOALS_PER_LONG_TERM_CYCLE: usize = 5;

/// 首版启发值：一个周/日周期里可见条目的上限。超过即为 `too_many_tasks`。
pub const MAX_TASKS_PER_CYCLE: usize = 12;

/// 首版启发值：日周期未完成任务数的上限，作为"预估工时总和超过一日"的
/// 代理指标（真实工时没有可靠来源；条目数是最诚实的可用信号）。超过即为
/// `too_much_work`。
pub const MAX_UNCOMPLETED_DAY_TASKS: usize = 8;

/// 建议的前端去抖间隔（毫秒）。**去抖是前端调用侧的职责**——输入停止
/// [`DEBOUNCE_HINT_MS`] 后再发起审查；后端不计时，只靠内容哈希缓存把重复
/// 调用变得便宜（design D5）。
pub const DEBOUNCE_HINT_MS: u64 = 800;

/// 缓存条目上限。按 `(cycle_id, hash)` 键控，正常使用远达不到；超限时
/// 简化处理为整表清空（LRU-ish：最旧的条目本来就是最冷的周期，全部重算
/// 一次的代价可接受，不值得维护真正的 LRU 结构）。
pub const CACHE_MAX_ENTRIES: usize = 128;

/// Usability-signal reason the spec names explicitly: the user felt planning
/// itself was too much work. Stored verbatim on the dismissal row.
pub const SIGNAL_PLANNING_FELT_LIKE_TOO_MUCH_WORK: &str = "planning_felt_like_too_much_work";
/// Usability-signal reason: the user is not sure how to use the issue panel.
pub const SIGNAL_NOT_SURE_HOW_TO_USE: &str = "not_sure_how_to_use";

// ---------------------------------------------------------------------------
// 8.2 / 8.3 Issue types and report
// ---------------------------------------------------------------------------

/// The six review issue types (spec: 问题类型). The serde wire value and the
/// persisted `planning_issue_dismissals.issue_type` string are the snake_case
/// enum name itself (e.g. `too_many_goals`) — the code string *is* the enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueType {
    TooManyGoals,
    TooManyTasks,
    TooMuchWork,
    NotSureWhatToDoNext,
    MissingSomething,
    NotUsefulForNeeds,
}

impl IssueType {
    /// Stable lower label: the serde wire value and the persisted value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TooManyGoals => "too_many_goals",
            Self::TooManyTasks => "too_many_tasks",
            Self::TooMuchWork => "too_much_work",
            Self::NotSureWhatToDoNext => "not_sure_what_to_do_next",
            Self::MissingSomething => "missing_something",
            Self::NotUsefulForNeeds => "not_useful_for_needs",
        }
    }

    /// Inverse of [`IssueType::as_str`]; unknown values yield `None` so a
    /// dismissal written by a newer binary is skipped, not guessed.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "too_many_goals" => Some(Self::TooManyGoals),
            "too_many_tasks" => Some(Self::TooManyTasks),
            "too_much_work" => Some(Self::TooMuchWork),
            "not_sure_what_to_do_next" => Some(Self::NotSureWhatToDoNext),
            "missing_something" => Some(Self::MissingSomething),
            "not_useful_for_needs" => Some(Self::NotUsefulForNeeds),
            _ => None,
        }
    }

    /// Short human-facing label following the spec's own vocabulary
    /// (目标过多 / 任务过多 / 工作量过大 / 不清楚下一步 / 缺少必要的东西 /
    /// 对当前需求没用). A diagnosis naming its location, never a rating.
    pub fn label(self) -> &'static str {
        match self {
            Self::TooManyGoals => "目标过多",
            Self::TooManyTasks => "任务过多",
            Self::TooMuchWork => "工作量过大",
            Self::NotSureWhatToDoNext => "不清楚下一步",
            Self::MissingSomething => "缺少必要的东西",
            Self::NotUsefulForNeeds => "对当前需求没用",
        }
    }
}

/// One concrete planning issue: where it is and what to do about it. A
/// `task_id` of `None` marks a cycle-level issue; otherwise the issue belongs
/// to that single task and gets its own dismiss entry (spec: 单任务层面的问题).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningIssue {
    pub issue_type: IssueType,
    pub cycle_id: String,
    pub task_id: Option<String>,
    pub title: String,
    pub detail: String,
}

// ---------------------------------------------------------------------------
// 8.3 Structure checks — deterministic, no LLM
// ---------------------------------------------------------------------------

/// Reviews one cycle's structure checks only: deterministic rules over the
/// cycle and its visible tasks. Pure function so thresholds are unit-testable
/// without a database; [`review_cycle`] is its thin I/O wrapper.
///
/// Result order is deterministic: cycle-level issues first (fixed type
/// order), then per-task issues in task order. Details of per-task issues are
/// grouped by task — each task yields at most one issue per type, with the
/// reasons combined into one detail string.
pub fn structural_issues(cycle: &Cycle, tasks: &[Task]) -> Vec<PlanningIssue> {
    let mut issues = Vec::new();

    // Empty input rows (trailing blank lines) are noise, never findings.
    let visible: Vec<&Task> = tasks.iter().filter(|t| !is_empty_input_row(t)).collect();
    let goals: Vec<&Task> = visible
        .iter()
        .copied()
        .filter(|t| t.parent_id.is_none())
        .collect();

    // too_many_goals: a Long-term (month) cycle holding too many goals.
    if cycle.cycle_type.is_long_term() && goals.len() > MAX_GOALS_PER_LONG_TERM_CYCLE {
        issues.push(PlanningIssue {
            issue_type: IssueType::TooManyGoals,
            cycle_id: cycle.id.clone(),
            task_id: None,
            title: IssueType::TooManyGoals.label().to_string(),
            detail: format!(
                "这个周期同时推进 {} 个目标，超过首版上限 {}：{}。删掉或推迟几个，才能有真正的进展。",
                goals.len(),
                MAX_GOALS_PER_LONG_TERM_CYCLE,
                join_titles(goals.iter().copied()),
            ),
        });
    }

    // too_many_tasks: week/day cycles holding too many items.
    if matches!(cycle.cycle_type, CycleType::Week | CycleType::Day)
        && visible.len() > MAX_TASKS_PER_CYCLE
    {
        issues.push(PlanningIssue {
            issue_type: IssueType::TooManyTasks,
            cycle_id: cycle.id.clone(),
            task_id: None,
            title: IssueType::TooManyTasks.label().to_string(),
            detail: format!(
                "这个周期排了 {} 项，超过首版上限 {}：{}。",
                visible.len(),
                MAX_TASKS_PER_CYCLE,
                join_titles(visible.iter().copied()),
            ),
        });
    }

    // too_much_work: uncompleted day tasks are the work-hour proxy.
    if cycle.cycle_type == CycleType::Day {
        let uncompleted: Vec<&Task> = visible.iter().copied().filter(|t| !t.completed).collect();
        if uncompleted.len() > MAX_UNCOMPLETED_DAY_TASKS {
            issues.push(PlanningIssue {
                issue_type: IssueType::TooMuchWork,
                cycle_id: cycle.id.clone(),
                task_id: None,
                title: IssueType::TooMuchWork.label().to_string(),
                detail: format!(
                    "今天还有 {} 项未完成，超过一日的合理量（{} 项）。把做不完的挪走或删掉。",
                    uncompleted.len(),
                    MAX_UNCOMPLETED_DAY_TASKS,
                ),
            });
        }
    }

    // Per-task checks, in task order.
    for task in &visible {
        if task.completed {
            continue;
        }
        // not_sure_what_to_do_next: the flag says so, or a goal without any
        // executable step (no checklist entry and no child task row).
        let has_children = tasks
            .iter()
            .any(|t| t.parent_id.as_deref() == Some(task.id.as_str()));
        if task.needs_breakdown == Some(true) {
            issues.push(PlanningIssue {
                issue_type: IssueType::NotSureWhatToDoNext,
                cycle_id: cycle.id.clone(),
                task_id: Some(task.id.clone()),
                title: IssueType::NotSureWhatToDoNext.label().to_string(),
                detail: format!(
                    "「{}」仍被标记为需要分解；确认或补上它的下一步。",
                    task.title.trim()
                ),
            });
        } else if cycle.cycle_type.is_long_term()
            && task.parent_id.is_none()
            && task.subtasks.is_empty()
            && !has_children
        {
            issues.push(PlanningIssue {
                issue_type: IssueType::NotSureWhatToDoNext,
                cycle_id: cycle.id.clone(),
                task_id: Some(task.id.clone()),
                title: IssueType::NotSureWhatToDoNext.label().to_string(),
                detail: format!(
                    "「{}」还没有任何可执行的下一步；先补一个第一步。",
                    task.title.trim()
                ),
            });
        }

        // missing_something: goals are judged by the breakdown engine's
        // missing-field set; per-task details group all missing fields.
        if cycle.cycle_type.is_long_term() && task.parent_id.is_none() {
            let missing = breakdown::missing_fields(&breakdown_of(task));
            if !missing.is_empty() {
                issues.push(PlanningIssue {
                    issue_type: IssueType::MissingSomething,
                    cycle_id: cycle.id.clone(),
                    task_id: Some(task.id.clone()),
                    title: IssueType::MissingSomething.label().to_string(),
                    detail: format!(
                        "「{}」还缺少：{}。",
                        task.title.trim(),
                        missing
                            .iter()
                            .map(|field| field.label())
                            .collect::<Vec<_>>()
                            .join("、")
                    ),
                });
            }
        }
    }

    issues
}

/// Full structural review of one cycle: the deterministic half of the review,
/// never touching an LLM. **This function has no `Err` path** (spec: 问题不
/// 阻塞主流程, 审查不可用 → 静默跳过): any internal failure (pool, missing
/// cycle, broken rows) logs at debug level and yields an empty report.
pub fn review_cycle(db: &Db, cycle_id: &str) -> Vec<PlanningIssue> {
    match load_snapshot(db, cycle_id) {
        Some((cycle, tasks)) => structural_issues(&cycle, &tasks),
        None => Vec::new(),
    }
}

fn join_titles<'a>(tasks: impl Iterator<Item = &'a Task>) -> String {
    tasks
        .map(|t| t.title.trim())
        .filter(|title| !title.is_empty())
        .collect::<Vec<_>>()
        .join("、")
}

/// Lenient breakdown read; a dirty JSON column degrades to the empty
/// breakdown exactly like the persistence contract in `ai::breakdown`.
fn breakdown_of(task: &Task) -> GoalBreakdown {
    static NULL: serde_json::Value = serde_json::Value::Null;
    GoalBreakdown::from_value(task.goal_breakdown.as_ref().unwrap_or(&NULL))
}

/// Loads the review inputs; any failure is logged and collapses to `None` so
/// every caller keeps its never-errors contract.
fn load_snapshot(db: &Db, cycle_id: &str) -> Option<(Cycle, Vec<Task>)> {
    let conn = match db.pool().get() {
        Ok(conn) => conn,
        Err(e) => {
            logging::debug(MODULE, &format!("review skipped, pool unavailable: {e}"));
            return None;
        }
    };
    let cycle = match crate::repository::cycles::get(&conn, cycle_id) {
        Ok(Some(cycle)) => cycle,
        Ok(None) => {
            logging::debug(MODULE, &format!("review skipped, unknown cycle {cycle_id}"));
            return None;
        }
        Err(e) => {
            logging::debug(MODULE, &format!("review skipped, cycle load failed: {e}"));
            return None;
        }
    };
    let tasks = match crate::repository::tasks::list_visible_by_cycle(&conn, cycle_id) {
        Ok(tasks) => tasks,
        Err(e) => {
            logging::debug(MODULE, &format!("review skipped, task load failed: {e}"));
            return None;
        }
    };
    Some((cycle, tasks))
}

// ---------------------------------------------------------------------------
// Semantic check — the optional LLM layer
// ---------------------------------------------------------------------------

/// Semantic review of one cycle with the configured provider: the goals that
/// are still ambiguous (`needs_refinement` and no `context.clarification` —
/// design D5's 只审可审项) are sent to the model, which reports the ones it
/// judges unclear for the user's needs as `not_useful_for_needs` issues.
///
/// **Degradation contract (task §8.5, spec: 审查不可用)**: no candidates, a
/// provider error, or an unparseable response each yield an empty `Vec` with
/// a debug log — never an error, never a modal. Model-reported task ids are
/// checked against the candidate set so hallucinated ids cannot materialize
/// as issues.
pub async fn review_cycle_semantic(
    db: &Db,
    resolved: &ResolvedProvider,
    provider: &dyn LlmProvider,
    cycle_id: &str,
) -> Vec<PlanningIssue> {
    let Some((_cycle, tasks)) = load_snapshot(db, cycle_id) else {
        return Vec::new();
    };
    semantic_issues(
        provider,
        cycle_id,
        &tasks,
        Some(resolved.model.max_output_tokens),
    )
    .await
}

/// Provider-facing core of the semantic check over already-loaded tasks so
/// the cached entry points can reuse one snapshot for hash and review.
async fn semantic_issues(
    provider: &dyn LlmProvider,
    cycle_id: &str,
    tasks: &[Task],
    max_tokens: Option<u64>,
) -> Vec<PlanningIssue> {
    let candidates: Vec<&Task> = tasks
        .iter()
        .filter(|t| t.parent_id.is_none() && !is_empty_input_row(t) && !t.completed)
        .filter(|t| t.needs_refinement == Some(true))
        .filter(|t| {
            breakdown::missing_fields(&breakdown_of(t))
                .contains(&MissingField::ContextClarification)
        })
        .collect();
    if candidates.is_empty() {
        return Vec::new();
    }

    let request = LlmRequest {
        system: "你是计划审查助手。只输出 JSON，不要输出其他文本。".to_string(),
        prompt: semantic_prompt(&candidates),
        max_tokens,
    };
    let value = match provider.generate_json(request).await {
        Ok(value) => value,
        Err(e) => {
            logging::debug(MODULE, &format!("semantic review skipped: {e}"));
            return Vec::new();
        }
    };
    parse_semantic_response(cycle_id, &value, &candidates)
}

/// Builds the semantic-review prompt: goal titles plus a breakdown summary
/// (the missing-field set), nothing else — no filler text, matching the
/// prioritization renderer's restraint.
fn semantic_prompt(candidates: &[&Task]) -> String {
    let mut prompt = String::new();
    prompt.push_str(
        "请判断下列目标是否表述得足够清楚、能否对应用户的真实需求。\
         只报告你确定不清楚的目标，不要评分。\n\n",
    );
    prompt.push_str(
        "只以 JSON 回答：{\"unclear_goals\": [{\"task_id\": \"…\", \"reason\": \"…\"}]}；\
         全部清楚时回答 {\"unclear_goals\": []}。\n\n目标列表：\n",
    );
    for task in candidates {
        let missing = breakdown::missing_fields(&breakdown_of(task));
        let missing_text = if missing.is_empty() {
            String::new()
        } else {
            format!(
                "（缺少：{}）",
                missing
                    .iter()
                    .map(|field| field.label())
                    .collect::<Vec<_>>()
                    .join("、")
            )
        };
        prompt.push_str(&format!(
            "- [{}] {}{}\n",
            task.id,
            task.title.trim(),
            missing_text
        ));
    }
    prompt
}

/// Parses `{"unclear_goals": [{"task_id", "reason"}]}`. Anything off-shape is
/// a silent degradation to an empty report (task §8.5); entries for ids that
/// are not in the candidate set are dropped, and duplicate ids collapse.
fn parse_semantic_response(
    cycle_id: &str,
    value: &serde_json::Value,
    candidates: &[&Task],
) -> Vec<PlanningIssue> {
    let Some(entries) = value
        .get("unclear_goals")
        .and_then(serde_json::Value::as_array)
    else {
        logging::debug(
            MODULE,
            "semantic review response has no unclear_goals array",
        );
        return Vec::new();
    };

    let mut issues = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for entry in entries {
        let Some(task_id) = entry.get("task_id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Some(reason) = entry.get("reason").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Some(task) = candidates.iter().find(|t| t.id == task_id) else {
            continue; // hallucinated id: never becomes an issue
        };
        if !seen.insert(task_id) {
            continue;
        }
        issues.push(PlanningIssue {
            issue_type: IssueType::NotUsefulForNeeds,
            cycle_id: cycle_id.to_string(),
            task_id: Some(task.id.clone()),
            title: IssueType::NotUsefulForNeeds.label().to_string(),
            detail: reason.trim().to_string(),
        });
    }
    issues
}

// ---------------------------------------------------------------------------
// 8.4 Dismissals — persistence, scopes, filtering
// ---------------------------------------------------------------------------

/// One persisted dismissal of a planning issue. `task_id: None` is a
/// cycle-level dismissal (hides the whole type in this cycle); otherwise only
/// that single task is hidden. `reason` carries the user-chosen free-form
/// signal (see the module docs on usability signals).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dismissal {
    pub id: String,
    pub cycle_id: String,
    pub issue_type: IssueType,
    pub task_id: Option<String>,
    pub reason: Option<String>,
    pub created_at: String,
}

/// Persists a dismissal (spec: 忽略的持久化与作用域).
///
/// `issue_type` is the persisted code string — the snake_case enum name
/// ([`IssueType::as_str`]); an unknown string is a validation error rather
/// than a silently unmatchable row.
///
/// Scopes: `task_id: None` is cycle-level — the same (cycle, type) keeps
/// exactly one row, so an earlier row is deleted before inserting. The
/// `UNIQUE (cycle_id, issue_type, task_id)` constraint cannot express that
/// for NULL `task_id` (SQLite treats NULLs as mutually distinct — see the
/// 0005 migration notes), which is why the delete happens here instead.
/// A task-level dismissal is idempotent: an existing (cycle, type, task) row
/// is left untouched (kept reason, kept created_at) instead of duplicating.
pub fn dismiss(
    db: &Db,
    cycle_id: &str,
    issue_type: &str,
    task_id: Option<&str>,
    reason: Option<&str>,
) -> AppResult<()> {
    let issue_type = IssueType::parse(issue_type).ok_or_else(|| {
        AppError::validation(
            "unknown_issue_type",
            format!("unknown planning issue type: {issue_type}"),
        )
    })?;
    let conn = db.pool().get()?;
    match task_id {
        None => {
            conn.execute(
                "DELETE FROM planning_issue_dismissals \
                 WHERE cycle_id = ?1 AND issue_type = ?2 AND task_id IS NULL",
                params![cycle_id, issue_type.as_str()],
            )
            .map_err(from_rusqlite)?;
        }
        Some(task_id) => {
            let exists: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM planning_issue_dismissals \
                 WHERE cycle_id = ?1 AND issue_type = ?2 AND task_id = ?3",
                    params![cycle_id, issue_type.as_str(), task_id],
                    |row| row.get(0),
                )
                .map_err(from_rusqlite)?;
            if exists > 0 {
                return Ok(());
            }
        }
    }
    conn.execute(
        "INSERT INTO planning_issue_dismissals (id, cycle_id, issue_type, task_id, reason) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            uuid::Uuid::new_v4().to_string(),
            cycle_id,
            issue_type.as_str(),
            task_id,
            reason,
        ],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

/// Every dismissal recorded for one cycle, oldest first. Rows with an
/// `issue_type` this binary does not know (forward compatibility — the column
/// has no CHECK by design) are skipped rather than guessed.
pub fn dismissals_for_cycle(conn: &Connection, cycle_id: &str) -> AppResult<Vec<Dismissal>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, cycle_id, issue_type, task_id, reason, created_at \
             FROM planning_issue_dismissals WHERE cycle_id = ?1 \
             ORDER BY created_at ASC, id ASC",
        )
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map(params![cycle_id], |row| {
            let type_str: String = row.get("issue_type")?;
            let Some(issue_type) = IssueType::parse(&type_str) else {
                // Unknown type from a newer binary: skip the row.
                return Ok(None);
            };
            Ok(Some(Dismissal {
                id: row.get("id")?,
                cycle_id: row.get("cycle_id")?,
                issue_type,
                task_id: row.get("task_id")?,
                reason: row.get("reason")?,
                created_at: row.get("created_at")?,
            }))
        })
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<Option<Dismissal>>>>()
        .map_err(from_rusqlite)?;
    Ok(rows.into_iter().flatten().collect())
}

/// Applies dismissal scopes to a fresh report (spec: 忽略的持久化与作用域):
/// a cycle-level dismissal hides its type across the whole cycle; a
/// task-level dismissal hides only that task's issue, leaving the same type
/// on other tasks visible. Pure function over the two lists.
pub fn filter_dismissed(
    issues: Vec<PlanningIssue>,
    dismissals: &[Dismissal],
) -> Vec<PlanningIssue> {
    let cycle_level: HashSet<IssueType> = dismissals
        .iter()
        .filter(|d| d.task_id.is_none())
        .map(|d| d.issue_type)
        .collect();
    let task_level: HashSet<(&IssueType, &str)> = dismissals
        .iter()
        .filter_map(|d| d.task_id.as_deref().map(|task_id| (&d.issue_type, task_id)))
        .collect();

    issues
        .into_iter()
        .filter(|issue| {
            if cycle_level.contains(&issue.issue_type) {
                return false;
            }
            match &issue.task_id {
                Some(task_id) => !task_level.contains(&(&issue.issue_type, task_id.as_str())),
                None => true,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 8.1 Content-hash cache
// ---------------------------------------------------------------------------

/// Hashes the review-relevant content of a cycle: the cycle id plus, per
/// task, title + completed + the two needs flags + position. `std`'s
/// `DefaultHasher` is enough — the hash only ever compares within one cache.
/// Breakdown edits re-trigger review through the needs flags the engine
/// derives; a pure rewording inside the JSON column does not (first-version
/// trade-off, design D5's 内容哈希比对).
pub fn content_hash(cycle_id: &str, tasks: &[Task]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    cycle_id.hash(&mut hasher);
    for task in tasks {
        task.title.hash(&mut hasher);
        task.completed.hash(&mut hasher);
        task.needs_refinement.hash(&mut hasher);
        task.needs_breakdown.hash(&mut hasher);
        task.position.hash(&mut hasher);
    }
    hasher.finish()
}

/// Review results keyed by `(cycle_id, content_hash)` (design D5): unchanged
/// content is answered from the cache without touching the LLM again. Bounded
/// by [`CACHE_MAX_ENTRIES`]; see that constant for the eviction trade-off.
pub struct IssueCache {
    entries: std::sync::Mutex<HashMap<(String, u64), Vec<PlanningIssue>>>,
}

impl Default for IssueCache {
    fn default() -> Self {
        Self::new()
    }
}

impl IssueCache {
    pub fn new() -> Self {
        Self {
            entries: std::sync::Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self, cycle_id: &str, hash: u64) -> Option<Vec<PlanningIssue>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&(cycle_id.to_string(), hash))
            .cloned()
    }

    pub fn put(&self, cycle_id: &str, hash: u64, issues: Vec<PlanningIssue>) {
        let mut entries = self
            .entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if entries.len() >= CACHE_MAX_ENTRIES
            && !entries.contains_key(&(cycle_id.to_string(), hash))
        {
            entries.clear();
        }
        entries.insert((cycle_id.to_string(), hash), issues);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }
}

/// The always-available entry point the review UI/call-side uses after its
/// own [`DEBOUNCE_HINT_MS`] debounce: the deterministic structure checks
/// behind the content-hash cache (design D5). A cache hit returns the
/// previous report without recomputing anything; a miss runs the structure
/// checks and replaces the cache entry — an idempotent replacement, never an
/// accumulation. Like every review entry point, this has no `Err` path.
///
/// The LLM-backed semantic pass is deliberately **not** on this path — it is
/// optional (spec: 审查不可用 → 静默跳过) and lives in
/// [`review_cached_with_semantic`], which the caller invokes when a provider
/// is configured (e.g. an explicit refresh). Both write the same
/// `(cycle_id, hash)` keyspace, so a semantic refresh upgrades later cache
/// hits to the combined report.
pub fn review_cached(db: &Db, cache: &IssueCache, cycle_id: &str) -> Vec<PlanningIssue> {
    let Some((cycle, tasks)) = load_snapshot(db, cycle_id) else {
        return Vec::new();
    };
    let hash = content_hash(cycle_id, &tasks);
    if let Some(hit) = cache.get(cycle_id, hash) {
        return hit;
    }
    let issues = structural_issues(&cycle, &tasks);
    cache.put(cycle_id, hash, issues.clone());
    issues
}

/// The composed review for callers with a usable provider: structure checks
/// plus the semantic pass behind the same content-hash cache. Unchanged
/// content never reaches the LLM twice (asserted with counting providers in
/// the tests); a dead provider degrades to the structural report only.
pub async fn review_cached_with_semantic(
    db: &Db,
    cache: &IssueCache,
    resolved: &ResolvedProvider,
    provider: &dyn LlmProvider,
    cycle_id: &str,
) -> Vec<PlanningIssue> {
    let Some((cycle, tasks)) = load_snapshot(db, cycle_id) else {
        return Vec::new();
    };
    let hash = content_hash(cycle_id, &tasks);
    if let Some(hit) = cache.get(cycle_id, hash) {
        return hit;
    }
    let mut issues = structural_issues(&cycle, &tasks);
    issues.extend(
        semantic_issues(
            provider,
            cycle_id,
            &tasks,
            Some(resolved.model.max_output_tokens),
        )
        .await,
    );
    cache.put(cycle_id, hash, issues.clone());
    issues
}

// ---------------------------------------------------------------------------
// 8.6 Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::llm::{
        AgentError, AgentRequest, AgentResponse, BoxFuture, FakeProvider, FakeTurn, LlmProvider,
        LlmRequest, ResolvedProvider,
    };
    use crate::providers::config::{ApiFormat, InputType, ModelConfig, OutputType, ProviderConfig};
    use crate::repository::tasks::NewTask;
    use rusqlite::Connection;

    // -- fixtures -----------------------------------------------------------

    /// Opens a fully migrated throwaway database, mirroring
    /// `ai::prioritization::tests::open_test_db`.
    fn open_test_db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = crate::db::open_at(&dir.path().join("planner.db")).expect("open db");
        (dir, db)
    }

    fn insert_cycle(conn: &Connection, id: &str, cycle_type: &str) {
        conn.execute(
            "INSERT INTO cycles (id, title, type, position) VALUES (?1, 'Plan', ?2, 0)",
            [id, cycle_type],
        )
        .expect("insert cycle");
    }

    fn new_task(id: &str, cycle_id: &str, title: &str, position: i64) -> NewTask {
        NewTask {
            id: id.into(),
            cycle_id: cycle_id.into(),
            parent_id: None,
            title: title.into(),
            subtasks: vec![],
            position,
            completed: false,
            goal_breakdown: None,
            needs_refinement: None,
            needs_breakdown: None,
            root_color_key: None,
            copied_from_task_id: None,
            created_at: 0,
        }
    }

    fn insert_task(conn: &Connection, task: &NewTask) {
        crate::repository::tasks::insert(conn, task).expect("insert task");
    }

    fn cycle_of(cycle_type: CycleType) -> Cycle {
        Cycle {
            id: "c1".into(),
            title: "Plan".into(),
            cycle_type,
            parent_id: None,
            position: 0,
            archived: false,
            started: false,
            finished: false,
            started_at: None,
            finished_at: None,
            duration: None,
            focused_time: 0,
            starts_on: None,
            ends_on: None,
            calendar_key: None,
            repeat_id: None,
            created_at: 0,
        }
    }

    fn goal(id: &str, position: i64) -> Task {
        Task {
            id: id.into(),
            cycle_id: "c1".into(),
            parent_id: None,
            title: format!("goal {id}"),
            subtasks: vec![],
            position,
            completed: false,
            goal_breakdown: None,
            needs_refinement: None,
            needs_breakdown: None,
            root_color_key: None,
            copied_from_task_id: None,
            proposal: None,
            created_at: 0,
        }
    }

    fn day_task(id: &str, position: i64) -> Task {
        goal(id, position)
    }

    fn type_counts(issues: &[PlanningIssue], issue_type: IssueType) -> usize {
        issues.iter().filter(|i| i.issue_type == issue_type).count()
    }

    fn resolved_stub() -> ResolvedProvider {
        ResolvedProvider {
            config: ProviderConfig {
                id: "p1".into(),
                name: "Test".into(),
                base_url: "http://127.0.0.1:9/v1".into(),
                api_format: ApiFormat::AnthropicMessages,
                extra_headers: vec![],
                models: vec![ModelConfig {
                    model_id: "m1".into(),
                    context_window: 8192,
                    max_output_tokens: 2048,
                    input_types: vec![InputType::Text],
                    output_types: vec![OutputType::Text],
                    supports_tools: true,
                }],
                created_at: 0,
                archived: false,
            },
            model: ModelConfig {
                model_id: "m1".into(),
                context_window: 8192,
                max_output_tokens: 2048,
                input_types: vec![InputType::Text],
                output_types: vec![OutputType::Text],
                supports_tools: true,
            },
            api_key: None,
            tools_supported: true,
        }
    }

    /// Wraps [`FakeProvider`] and counts `generate_json` calls — the LLM
    /// traffic meter for the cache tests.
    struct CountingProvider {
        inner: FakeProvider,
        json_calls: std::sync::atomic::AtomicUsize,
    }

    impl CountingProvider {
        fn with_json(value: serde_json::Value) -> Self {
            Self {
                inner: FakeProvider::with_json(value),
                json_calls: std::sync::atomic::AtomicUsize::new(0),
            }
        }

        fn json_calls(&self) -> usize {
            self.json_calls.load(std::sync::atomic::Ordering::SeqCst)
        }
    }

    impl LlmProvider for CountingProvider {
        fn generate_agent<'a>(
            &'a self,
            req: AgentRequest,
        ) -> BoxFuture<'a, Result<AgentResponse, AgentError>> {
            self.inner.generate_agent(req)
        }

        fn generate_json<'a>(
            &'a self,
            req: LlmRequest,
        ) -> BoxFuture<'a, Result<serde_json::Value, AgentError>> {
            self.json_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.inner.generate_json(req)
        }
    }

    /// Always fails with a provider-level error, standing in for "AI 不可用".
    struct FailingProvider;

    impl LlmProvider for FailingProvider {
        fn generate_agent<'a>(
            &'a self,
            _req: AgentRequest,
        ) -> BoxFuture<'a, Result<AgentResponse, AgentError>> {
            Box::pin(async { Err(provider_error()) })
        }

        fn generate_json<'a>(
            &'a self,
            _req: LlmRequest,
        ) -> BoxFuture<'a, Result<serde_json::Value, AgentError>> {
            Box::pin(async { Err(provider_error()) })
        }
    }

    fn provider_error() -> AgentError {
        AgentError::Provider(crate::sampling::SamplingError::ProviderUnreachable {
            message: "provider down".into(),
        })
    }

    // -- 8.2 issue type identity --------------------------------------------

    #[test]
    fn issue_type_wire_value_is_the_snake_case_enum_name() {
        for (issue_type, code) in [
            (IssueType::TooManyGoals, "too_many_goals"),
            (IssueType::TooManyTasks, "too_many_tasks"),
            (IssueType::TooMuchWork, "too_much_work"),
            (IssueType::NotSureWhatToDoNext, "not_sure_what_to_do_next"),
            (IssueType::MissingSomething, "missing_something"),
            (IssueType::NotUsefulForNeeds, "not_useful_for_needs"),
        ] {
            assert_eq!(issue_type.as_str(), code);
            assert_eq!(IssueType::parse(code), Some(issue_type));
            assert_eq!(
                serde_json::to_value(issue_type).unwrap(),
                serde_json::json!(code)
            );
        }
        assert_eq!(IssueType::parse("bogus"), None);
    }

    // -- 8.2/8.3 structure checks, one boundary case per threshold ----------

    #[test]
    fn too_many_goals_fires_past_the_cap_in_a_long_term_cycle_only() {
        let month = cycle_of(CycleType::Month);
        let five: Vec<Task> = (0..5).map(|i| goal(&format!("g{i}"), i)).collect();
        assert_eq!(
            type_counts(&structural_issues(&month, &five), IssueType::TooManyGoals),
            0,
            "five goals stay at the cap"
        );

        let six: Vec<Task> = (0..6).map(|i| goal(&format!("g{i}"), i)).collect();
        let issues = structural_issues(&month, &six);
        assert_eq!(type_counts(&issues, IssueType::TooManyGoals), 1);
        assert_eq!(issues[0].task_id, None, "cycle-level issue");
        assert!(issues[0].detail.contains("6"), "detail names the count");

        // A week cycle is not judged on goal count.
        let week = cycle_of(CycleType::Week);
        assert_eq!(
            type_counts(&structural_issues(&week, &six), IssueType::TooManyGoals),
            0
        );
    }

    #[test]
    fn too_many_tasks_fires_for_week_and_day_cycles() {
        let many: Vec<Task> = (0..13).map(|i| day_task(&format!("t{i}"), i)).collect();
        for cycle_type in [CycleType::Week, CycleType::Day] {
            let issues = structural_issues(&cycle_of(cycle_type), &many);
            assert_eq!(type_counts(&issues, IssueType::TooManyTasks), 1);
            assert_eq!(issues[0].task_id, None);
        }

        let twelve: Vec<Task> = (0..12).map(|i| day_task(&format!("t{i}"), i)).collect();
        assert_eq!(
            type_counts(
                &structural_issues(&cycle_of(CycleType::Day), &twelve),
                IssueType::TooManyTasks
            ),
            0,
            "twelve items stay at the cap"
        );
    }

    #[test]
    fn too_much_work_counts_uncompleted_day_tasks_only() {
        let day = cycle_of(CycleType::Day);
        let mut nine: Vec<Task> = (0..9).map(|i| day_task(&format!("t{i}"), i)).collect();
        assert_eq!(
            type_counts(&structural_issues(&day, &nine), IssueType::TooMuchWork),
            1,
            "nine uncompleted tasks read as more than one day"
        );

        nine[0].completed = true;
        assert_eq!(
            type_counts(&structural_issues(&day, &nine), IssueType::TooMuchWork),
            0,
            "completing one task drops below the day's budget"
        );

        // A week cycle carries the same load without the day-work verdict.
        let week = cycle_of(CycleType::Week);
        nine[0].completed = false;
        assert_eq!(
            type_counts(&structural_issues(&week, &nine), IssueType::TooMuchWork),
            0
        );
    }

    #[test]
    fn not_sure_what_to_do_next_flags_the_breakdown_flag_and_stepless_goals() {
        let month = cycle_of(CycleType::Month);

        let mut flagged = goal("g1", 0);
        flagged.needs_breakdown = Some(true);
        let issues = structural_issues(&month, &[flagged]);
        assert_eq!(
            type_counts(&issues, IssueType::NotSureWhatToDoNext),
            1,
            "needs_breakdown=true is a direct hit"
        );

        // A stepless goal: no checklist entry and no child row.
        let stepless = goal("g2", 0);
        let issues = structural_issues(&month, &[stepless]);
        assert_eq!(
            type_counts(&issues, IssueType::NotSureWhatToDoNext),
            1,
            "a goal without any subtask has no next step"
        );

        // A checklist entry clears the verdict.
        let mut with_subtask = goal("g3", 0);
        with_subtask.subtasks = vec![crate::domain::task::Subtask::new("first step", false)];
        let issues = structural_issues(&month, &[with_subtask]);
        assert_eq!(type_counts(&issues, IssueType::NotSureWhatToDoNext), 0);

        // So does a child task row.
        let mut parent = goal("g4", 0);
        let mut child = goal("g4a", 0);
        child.parent_id = Some("g4".into());
        parent.needs_breakdown = None;
        let issues = structural_issues(&month, &[parent, child]);
        assert_eq!(type_counts(&issues, IssueType::NotSureWhatToDoNext), 0);

        // Plain day tasks without the flag are not "no next step" material.
        let day = cycle_of(CycleType::Day);
        let plain = day_task("t1", 0);
        let issues = structural_issues(&day, &[plain]);
        assert_eq!(type_counts(&issues, IssueType::NotSureWhatToDoNext), 0);
    }

    #[test]
    fn missing_something_lists_the_missing_fields_per_task() {
        let month = cycle_of(CycleType::Month);

        let bare = goal("g1", 0);
        let issues = structural_issues(&month, &[bare]);
        assert_eq!(type_counts(&issues, IssueType::MissingSomething), 1);
        // One grouped issue per task, listing the engine's missing fields.
        let missing = issues
            .iter()
            .find(|i| i.issue_type == IssueType::MissingSomething)
            .expect("missing-something issue");
        assert!(missing.detail.contains("产出物"));
        assert!(missing.detail.contains("目标澄清"));
        assert_eq!(missing.task_id.as_deref(), Some("g1"));

        // A fully filled breakdown clears the verdict.
        let mut filled = goal("g2", 0);
        filled.goal_breakdown = Some(serde_json::json!({
            "context": { "clarification": "why", "background": "bg", "stakeholders": "vendor" },
            "output": { "value": "thing" },
            "outcome": { "value": "change", "verification_method": "how" },
            "scope": { "effort": "one day" }
        }));
        let issues = structural_issues(&month, &[filled]);
        assert_eq!(type_counts(&issues, IssueType::MissingSomething), 0);
    }

    // -- 8.1 content hash ----------------------------------------------------

    #[test]
    fn content_hash_follows_only_the_reviewed_content() {
        let base: Vec<Task> = vec![day_task("t1", 0), day_task("t2", 1)];
        let same = content_hash("c1", &base);
        assert_eq!(
            same,
            content_hash("c1", &base),
            "equal content hashes identically"
        );

        let retitled = base.clone();
        let mut retitled = retitled;
        retitled[0].title = "renamed".into();
        assert_ne!(same, content_hash("c1", &retitled), "title matters");

        let mut completed = base.clone();
        completed[1].completed = true;
        assert_ne!(same, content_hash("c1", &completed), "completed matters");

        let mut flagged = base.clone();
        flagged[0].needs_breakdown = Some(true);
        assert_ne!(same, content_hash("c1", &flagged), "needs flags matter");

        let mut moved = base.clone();
        moved[0].position = 7;
        assert_ne!(same, content_hash("c1", &moved), "position matters");

        assert_ne!(
            same,
            content_hash("c2", &base),
            "the cycle id is part of the key"
        );
    }

    // -- 8.1/8.5 cached review with the LLM counter --------------------------

    #[test]
    fn review_cached_answers_from_the_cache_without_recomputing() {
        let (_dir, db) = open_test_db();
        let conn = db.pool().get().expect("conn");
        insert_cycle(&conn, "c1", "month");
        for i in 0..6 {
            insert_task(
                &conn,
                &new_task(&format!("g{i}"), "c1", &format!("goal {i}"), i),
            );
        }

        // Seed the cache with a sentinel report under the current content
        // hash; the call must answer from the cache instead of recomputing
        // the cycle (observable: the sentinel survives verbatim).
        let cache = IssueCache::new();
        let tasks = crate::repository::tasks::list_visible_by_cycle(&conn, "c1").expect("tasks");
        let hash = content_hash("c1", &tasks);
        let sentinel = PlanningIssue {
            issue_type: IssueType::TooManyTasks,
            cycle_id: "c1".into(),
            task_id: None,
            title: IssueType::TooManyTasks.label().to_string(),
            detail: "cache sentinel".into(),
        };
        cache.put("c1", hash, vec![sentinel.clone()]);

        let report = review_cached(&db, &cache, "c1");
        assert_eq!(report, vec![sentinel], "a cache hit answers as stored");

        // Unknown cycles degrade to an empty report, not an error.
        assert!(review_cached(&db, &cache, "nope").is_empty());
    }

    #[tokio::test]
    async fn review_cached_second_call_with_same_content_skips_the_llm() {
        let (_dir, db) = open_test_db();
        let conn = db.pool().get().expect("conn");
        insert_cycle(&conn, "c1", "day");
        let mut task = new_task("t1", "c1", "vague thing", 0);
        task.needs_refinement = Some(true);
        insert_task(&conn, &task);

        let provider = CountingProvider::with_json(serde_json::json!({
            "unclear_goals": [{ "task_id": "t1", "reason": "没有可验证的结果" }]
        }));
        let resolved = resolved_stub();
        let cache = IssueCache::new();

        let first = review_cached_with_semantic(&db, &cache, &resolved, &provider, "c1").await;
        assert_eq!(provider.json_calls(), 1);
        assert_eq!(type_counts(&first, IssueType::NotUsefulForNeeds), 1);

        // Same content: served from the cache, no second LLM call.
        let second = review_cached_with_semantic(&db, &cache, &resolved, &provider, "c1").await;
        assert_eq!(provider.json_calls(), 1, "cache hit must not call the LLM");
        assert_eq!(first, second, "the cached report is returned as-is");
    }

    #[tokio::test]
    async fn review_cached_recomputes_when_content_changes() {
        let (_dir, db) = open_test_db();
        let conn = db.pool().get().expect("conn");
        insert_cycle(&conn, "c1", "day");
        let mut task = new_task("t1", "c1", "vague thing", 0);
        task.needs_refinement = Some(true);
        insert_task(&conn, &task);

        let provider = CountingProvider::with_json(serde_json::json!({ "unclear_goals": [] }));
        let resolved = resolved_stub();
        let cache = IssueCache::new();

        review_cached_with_semantic(&db, &cache, &resolved, &provider, "c1").await;
        assert_eq!(provider.json_calls(), 1);

        conn.execute(
            "UPDATE tasks SET title = 'renamed thing' WHERE id = 't1'",
            [],
        )
        .expect("retitle");

        review_cached_with_semantic(&db, &cache, &resolved, &provider, "c1").await;
        assert_eq!(
            provider.json_calls(),
            2,
            "a changed content hash must recompute"
        );
    }

    #[test]
    fn cache_clears_itself_when_over_the_entry_cap() {
        let cache = IssueCache::new();
        for i in 0..(CACHE_MAX_ENTRIES + 1) {
            cache.put(&format!("c{i}"), i as u64, Vec::new());
        }
        assert!(
            cache.len() <= CACHE_MAX_ENTRIES,
            "the cache never grows past its cap"
        );
    }

    // -- semantic layer -------------------------------------------------------

    #[tokio::test]
    async fn semantic_review_reports_unclear_goals_and_drops_hallucinations() {
        let mut candidate = goal("g1", 0);
        candidate.needs_refinement = Some(true);
        let mut settled = goal("g2", 1);
        settled.needs_refinement = Some(false);

        let provider = FakeProvider::with_json(serde_json::json!({
            "unclear_goals": [
                { "task_id": "g1", "reason": "看不出要解决什么问题" },
                { "task_id": "ghost", "reason": "不存在的任务" },
                { "task_id": "g2", "reason": "已完成澄清，不成立" }
            ]
        }));
        let issues = semantic_issues(&provider, "c1", &[candidate, settled], None).await;

        assert_eq!(issues.len(), 1, "only the real candidate survives");
        assert_eq!(issues[0].issue_type, IssueType::NotUsefulForNeeds);
        assert_eq!(issues[0].task_id.as_deref(), Some("g1"));
        assert_eq!(issues[0].detail, "看不出要解决什么问题");
        assert_eq!(issues[0].title, "对当前需求没用");
    }

    #[tokio::test]
    async fn semantic_review_sends_only_reviewable_candidates() {
        let mut candidate = goal("g1", 0);
        candidate.needs_refinement = Some(true);
        let not_flagged = goal("g2", 1);
        let tasks = vec![candidate, not_flagged];

        let provider = CountingProvider::with_json(serde_json::json!({ "unclear_goals": [] }));
        semantic_issues(&provider, "c1", &tasks, None).await;
        assert_eq!(
            provider.json_calls(),
            1,
            "one request when candidates exist"
        );

        // 只审可审项: without any candidate no request is sent at all.
        let provider = CountingProvider::with_json(serde_json::json!({ "unclear_goals": [] }));
        semantic_issues(&provider, "c1", &[goal("g3", 0)], None).await;
        assert_eq!(provider.json_calls(), 0, "no candidates, no request");
    }

    #[tokio::test]
    async fn semantic_review_degrades_to_empty_on_provider_error() {
        // A real candidate (flagged, clarification missing) so the provider is
        // actually reached before it fails.
        let mut candidate = goal("g1", 0);
        candidate.needs_refinement = Some(true);
        let tasks = vec![candidate];
        let issues = semantic_issues(&FailingProvider, "c1", &tasks, None).await;
        assert!(
            issues.is_empty(),
            "provider error must degrade to an empty report"
        );

        // The scripted fail variant of the fake provider maps to the same
        // silent degradation.
        let provider = FakeProvider::with_script(vec![FakeTurn::Fail(provider_error())]);
        let issues = semantic_issues(&provider, "c1", &tasks, None).await;
        assert!(issues.is_empty());
    }

    #[tokio::test]
    async fn semantic_review_degrades_to_empty_on_unparseable_response() {
        let mut candidate = goal("g1", 0);
        candidate.needs_refinement = Some(true);
        let tasks = vec![candidate];
        for garbage in [
            serde_json::json!("a plain string"),
            serde_json::json!({ "wrong": [] }),
            serde_json::json!({ "unclear_goals": "not an array" }),
            serde_json::json!({ "unclear_goals": [{ "task_id": "g1" }] }), // reason missing
        ] {
            let provider = FakeProvider::with_json(garbage);
            let issues = semantic_issues(&provider, "c1", &tasks, None).await;
            assert!(issues.is_empty(), "garbage must degrade to empty");
        }
    }

    #[tokio::test]
    async fn provider_failure_leaves_the_structural_report_untouched() {
        let (_dir, db) = open_test_db();
        let conn = db.pool().get().expect("conn");
        insert_cycle(&conn, "c1", "month");
        for i in 0..6 {
            insert_task(
                &conn,
                &new_task(&format!("g{i}"), "c1", &format!("goal {i}"), i),
            );
        }
        // One flagged goal becomes a semantic candidate, so the dead provider
        // is genuinely reached by the semantic pass.
        conn.execute("UPDATE tasks SET needs_refinement = 1 WHERE id = 'g0'", [])
            .expect("flag goal");

        let resolved = resolved_stub();
        // 不可用的供应商：结构审查照常给出问题，语义层静默为空。
        let structural = review_cycle(&db, "c1");
        assert_eq!(type_counts(&structural, IssueType::TooManyGoals), 1);

        let mut combined = structural.clone();
        combined.extend(review_cycle_semantic(&db, &resolved, &FailingProvider, "c1").await);
        assert_eq!(
            type_counts(&combined, IssueType::TooManyGoals),
            1,
            "the structural verdict survives the dead provider"
        );
        assert!(combined
            .iter()
            .all(|issue| issue.issue_type != IssueType::NotUsefulForNeeds));
    }

    // -- review_cycle is infallible -------------------------------------------

    #[tokio::test]
    async fn review_cycle_survives_garbage_and_unknown_cycles() {
        let (_dir, db) = open_test_db();
        let conn = db.pool().get().expect("conn");

        // Unknown cycle: an empty report, no panic, no error.
        assert!(review_cycle(&db, "nope").is_empty());

        // A dirty breakdown column must not break the review — the lenient
        // parse degrades it to the empty breakdown.
        insert_cycle(&conn, "c1", "month");
        let mut task = new_task("g1", "c1", "goal one", 0);
        task.goal_breakdown = Some(serde_json::json!("not even an object"));
        insert_task(&conn, &task);
        let issues = review_cycle(&db, "c1");
        assert_eq!(type_counts(&issues, IssueType::MissingSomething), 1);
    }

    // -- 8.4 dismissals --------------------------------------------------------

    #[test]
    fn task_level_dismiss_is_idempotent() {
        let (_dir, db) = open_test_db();
        let conn = db.pool().get().expect("conn");
        insert_cycle(&conn, "c1", "month");
        // Task-level dismissals reference real task rows (FK).
        insert_task(&conn, &new_task("t1", "c1", "goal one", 0));
        insert_task(&conn, &new_task("t2", "c1", "goal two", 1));

        dismiss(
            &db,
            "c1",
            IssueType::NotSureWhatToDoNext.as_str(),
            Some("t1"),
            Some("够了"),
        )
        .expect("first dismiss");
        dismiss(
            &db,
            "c1",
            IssueType::NotSureWhatToDoNext.as_str(),
            Some("t1"),
            Some("再忽略一次"),
        )
        .expect("repeat dismiss");

        let dismissals = dismissals_for_cycle(&conn, "c1").expect("load");
        assert_eq!(dismissals.len(), 1, "the same task+type dismisses once");
        assert_eq!(dismissals[0].task_id.as_deref(), Some("t1"));
        assert_eq!(
            dismissals[0].reason.as_deref(),
            Some("够了"),
            "the first record is kept"
        );

        // A different task of the same type gets its own row.
        dismiss(
            &db,
            "c1",
            IssueType::NotSureWhatToDoNext.as_str(),
            Some("t2"),
            None,
        )
        .expect("second task");
        assert_eq!(dismissals_for_cycle(&conn, "c1").expect("load").len(), 2);
    }

    #[test]
    fn cycle_level_dismiss_keeps_exactly_one_row_per_type() {
        let (_dir, db) = open_test_db();
        let conn = db.pool().get().expect("conn");
        insert_cycle(&conn, "c1", "month");

        dismiss(
            &db,
            "c1",
            IssueType::TooManyGoals.as_str(),
            None,
            Some("就这么多"),
        )
        .expect("first dismiss");
        dismiss(
            &db,
            "c1",
            IssueType::TooManyGoals.as_str(),
            None,
            Some(SIGNAL_PLANNING_FELT_LIKE_TOO_MUCH_WORK),
        )
        .expect("second dismiss replaces the first");

        let dismissals = dismissals_for_cycle(&conn, "c1").expect("load");
        assert_eq!(
            dismissals.len(),
            1,
            "cycle-level uniqueness is service-kept"
        );
        assert_eq!(dismissals[0].task_id, None);
        assert_eq!(
            dismissals[0].reason.as_deref(),
            Some(SIGNAL_PLANNING_FELT_LIKE_TOO_MUCH_WORK),
            "the newest record wins"
        );
        assert_eq!(dismissals[0].issue_type, IssueType::TooManyGoals);
        assert!(!dismissals[0].created_at.is_empty(), "created_at is stored");

        // Another type lives beside it independently.
        dismiss(&db, "c1", IssueType::TooManyTasks.as_str(), None, None).expect("other type");
        assert_eq!(dismissals_for_cycle(&conn, "c1").expect("load").len(), 2);

        // An unknown issue-type code is rejected instead of persisted as an
        // unmatchable row.
        let error = dismiss(&db, "c1", "bogus_type", None, None).expect_err("unknown type");
        match error {
            AppError::Validation { code, .. } => assert_eq!(code, "unknown_issue_type"),
            other => panic!("expected validation error, got {other:?}"),
        }
        assert_eq!(dismissals_for_cycle(&conn, "c1").expect("load").len(), 2);
    }

    #[test]
    fn filter_dismissed_respects_both_scopes() {
        let cycle_id = "c1".to_string();
        let make = |issue_type: IssueType, task_id: Option<&str>| PlanningIssue {
            issue_type,
            cycle_id: cycle_id.clone(),
            task_id: task_id.map(Into::into),
            title: issue_type.label().to_string(),
            detail: "d".into(),
        };
        let issues = vec![
            make(IssueType::TooManyGoals, None),
            make(IssueType::NotSureWhatToDoNext, Some("t1")),
            make(IssueType::NotSureWhatToDoNext, Some("t2")),
            make(IssueType::MissingSomething, Some("t2")),
        ];

        let dismissal = |issue_type: IssueType, task_id: Option<&str>| Dismissal {
            id: format!("d{}", issue_type.as_str()),
            cycle_id: cycle_id.clone(),
            issue_type,
            task_id: task_id.map(Into::into),
            reason: None,
            created_at: String::new(),
        };

        // Task-level scope: only that task's issue disappears.
        let filtered = filter_dismissed(
            issues.clone(),
            &[dismissal(IssueType::NotSureWhatToDoNext, Some("t1"))],
        );
        assert_eq!(filtered.len(), 3);
        assert!(filtered.iter().all(|i| i.task_id.as_deref() != Some("t1")
            || i.issue_type != IssueType::NotSureWhatToDoNext));
        assert_eq!(
            type_counts(&filtered, IssueType::NotSureWhatToDoNext),
            1,
            "the same type on other tasks keeps showing"
        );

        // Cycle-level scope: the whole type disappears from the cycle.
        let filtered = filter_dismissed(issues, &[dismissal(IssueType::NotSureWhatToDoNext, None)]);
        assert_eq!(type_counts(&filtered, IssueType::NotSureWhatToDoNext), 0);
        assert_eq!(type_counts(&filtered, IssueType::TooManyGoals), 1);
        assert_eq!(type_counts(&filtered, IssueType::MissingSomething), 1);
    }

    #[test]
    fn dismissals_round_trip_through_the_table() {
        let (_dir, db) = open_test_db();
        let conn = db.pool().get().expect("conn");
        insert_cycle(&conn, "c1", "month");
        insert_task(&conn, &new_task("t1", "c1", "goal one", 0));

        dismiss(
            &db,
            "c1",
            IssueType::MissingSomething.as_str(),
            Some("t1"),
            Some(SIGNAL_NOT_SURE_HOW_TO_USE),
        )
        .expect("dismiss");
        let dismissals = dismissals_for_cycle(&conn, "c1").expect("load");
        assert_eq!(dismissals.len(), 1);
        assert_eq!(dismissals[0].issue_type, IssueType::MissingSomething);
        assert_eq!(dismissals[0].task_id.as_deref(), Some("t1"));
        assert_eq!(
            dismissals[0].reason.as_deref(),
            Some(SIGNAL_NOT_SURE_HOW_TO_USE)
        );

        // Unknown cycle: no rows, no error.
        assert!(dismissals_for_cycle(&conn, "ghost")
            .expect("empty for unknown cycle")
            .is_empty());
    }
}

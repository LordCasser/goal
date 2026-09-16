//! Onboarding & lifecycle use cases (change: add-onboarding-and-lifecycle).
//!
//! Sections: getting-started guide derivation (§1), one-shot hints (§2),
//! exit poll (§3), feedback & talk-to-founder (§4), degraded update check
//! (§5), daily-plan-time preference and the notify gate (§6).
//!
//! 隐私边界（本实现，务必保持）：应用除用户配置的 AI 供应商外零出网。
//! 退出调查与反馈的"上报"在本版本没有通道——所有事件只追加进 `app_settings`
//! 的本地队列（`staged_events` / `staged_feedback`），**本版本无上报通道，
//! 数据仅存本地，等待用户显式导出**。队列封顶防膨胀；读取命令
//! （`list_staged_feedback` 等）就是未来导出入口的雏形。
//!
//! 自动更新同样降级：本版本无更新通道（无端点拉取、无下载、无签名校验），
//! 只保留语义化版本比较纯函数与本地版本自省；`update_endpoint` 设置键
//! 预留给用户配置，未配置时检查更新返回 `disabled`。安装路径不存在，
//! "校验失败拒绝安装"被"根本不安装"严格满足。
//!
//! Behaviour contracts: `openspec/specs/onboarding-guidance/spec.md` and
//! `openspec/specs/app-lifecycle/spec.md`.

use std::cmp::Ordering;

use rusqlite::Connection;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::domain::cycle::LATER_CYCLE_ID;
use crate::error::{from_rusqlite, AppError, AppResult};
use crate::repository::{cycles as cycles_repo, settings as settings_repo, tasks as tasks_repo};
use crate::service::Mutation;

// ---------------------------------------------------------------------------
// app_settings keys (all non-sensitive kv; no new migration)
// ---------------------------------------------------------------------------

/// `active` (absent) | `completed` | `skipped` — see [`GuideState`].
pub const KEY_GUIDE_STATE: &str = "onboarding.guide_state";
/// JSON array of dismissed hint ids (§2).
pub const KEY_DISMISSED_HINTS: &str = "onboarding.dismissed_hints";
/// Milliseconds timestamp of the last exit-poll presentation (§3.2).
pub const KEY_EXIT_POLL_SHOWN_AT: &str = "exit_poll.shown_at";
/// Exit-poll lifecycle marker: shown/acknowledged/submitted/dismissed/
/// continued/exit_confirmed (§3.3/§3.4).
pub const KEY_EXIT_POLL_STATE: &str = "exit_poll.state";
/// Local queue of exit-poll outcomes (§3.5). 无上报通道，仅存本地。
pub const KEY_STAGED_EVENTS: &str = "staged_events";
/// Local queue of feedback entries (§4). 无上报通道，仅存本地。
pub const KEY_STAGED_FEEDBACK: &str = "staged_feedback";
/// Anonymous identifier (uuid v4) attached to staged items (§3.5/§4.1).
pub const KEY_ANONYMOUS_ID: &str = "feedback.anonymous_id";
/// First time the talk-to-founder channel was opened (§4.5).
pub const KEY_TALK_FOUNDER_FIRST_OPENED_AT: &str = "talk_to_founder.first_opened_at";
/// How many times the channel was opened (§4.5).
pub const KEY_TALK_FOUNDER_OPEN_COUNT: &str = "talk_to_founder.open_count";
/// Last time the channel was closed (§4.5).
pub const KEY_TALK_FOUNDER_CLOSED_AT: &str = "talk_to_founder.closed_at";
/// Reserved for a user-configured update endpoint (§5). Unused this build:
/// 本版本无更新通道，端点由用户配置后才启用。
pub const KEY_UPDATE_ENDPOINT: &str = "update_endpoint";
/// Local preference for the daily planning moment, `HH:MM`.
pub const KEY_DAILY_PLAN_TIME: &str = "daily_plan_time";

// ---------------------------------------------------------------------------
// §1 Getting-started guide — five steps, derived from real data
// ---------------------------------------------------------------------------

/// Fixed step ids; order defines the display order and the progress count.
pub const GUIDE_STEP_IDS: [&str; 5] = [
    "set_long_term_goal",
    "add_later_item",
    "link_week_to_long_term",
    "link_day_to_week",
    "complete_focus_block",
];

/// The focus-block step counts as done once a finished session carries at
/// least 30 minutes of `focused_time`.
pub const FOCUS_STEP_MIN_MS: i64 = 30 * 60 * 1000;

/// Whole-guide state. `Skipped` persists and suppresses any proactive
/// display; `Completed` is the user's explicit "done with this".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GuideState {
    Active,
    Completed,
    Skipped,
}

impl GuideState {
    fn from_stored(raw: Option<String>) -> Self {
        match raw.as_deref() {
            Some("completed") => Self::Completed,
            Some("skipped") => Self::Skipped,
            _ => Self::Active,
        }
    }
}

/// Per-step status: derived `NotDone`/`Done`; `Skipped` only ever appears
/// while the whole guide is skipped (未完成 / 已完成 / 已跳过).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    #[serde(rename = "not_done")]
    NotDone,
    Done,
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuideStep {
    pub id: String,
    pub title: String,
    pub detail: String,
    pub status: StepStatus,
}

#[derive(Debug, Clone, Serialize)]
pub struct GettingStartedGuide {
    pub state: GuideState,
    /// How many steps currently count as done ("N of 5").
    pub completed: usize,
    pub total: usize,
    pub steps: Vec<GuideStep>,
}

fn step_copy(id: &str) -> (&'static str, &'static str) {
    match id {
        "set_long_term_goal" => (
            "Set a clear long-term goal",
            "Create a long-term cycle and clarify one goal inside it.",
        ),
        "add_later_item" => (
            "Add something to Do Later",
            "Park at least one idea in the Later list.",
        ),
        "link_week_to_long_term" => (
            "Connect a weekly goal to the long-term goal",
            "Link a weekly item under a long-term goal.",
        ),
        "link_day_to_week" => (
            "Connect a daily task to the weekly goal",
            "Link a daily task under a weekly item.",
        ),
        "complete_focus_block" => (
            "Complete a 30-minute focus block",
            "Finish a focus block with 30+ minutes of focused time.",
        ),
        other => unreachable!("unknown guide step id: {other}"),
    }
}

fn scalar_exists(conn: &Connection, sql: &str) -> AppResult<bool> {
    conn.query_row(sql, [], |r| r.get::<_, i64>(0))
        .map(|v| v != 0)
        .map_err(from_rusqlite)
}

/// Step 1 — a clarified long-term goal: a visible goal row inside a
/// non-archived month cycle (Later excluded) whose clarity flag settled on
/// `needs_refinement = false` (lifecycle or manual clarification).
fn step_long_term_goal_done(conn: &Connection) -> AppResult<bool> {
    scalar_exists(
        conn,
        "SELECT EXISTS(SELECT 1 FROM tasks t JOIN cycles c ON c.id = t.cycle_id \
         WHERE c.type = 'month' AND c.archived = 0 AND c.id != 'later' \
           AND t.proposal IS NULL AND t.needs_refinement = 0 AND TRIM(t.title) != '')",
    )
}

/// Step 2 — the Later container holds at least one real item (an empty input
/// row does not count).
fn step_later_item_done(conn: &Connection) -> AppResult<bool> {
    scalar_exists(
        conn,
        &format!(
            "SELECT EXISTS(SELECT 1 FROM tasks \
             WHERE cycle_id = '{LATER_CYCLE_ID}' AND proposal IS NULL AND TRIM(title) != '')"
        ),
    )
}

/// Steps 3/4 — cross-level links: a week task parented by a long-term goal
/// (day task by a weekly item). Pending proposals never count.
fn cross_level_link_exists(conn: &Connection, child: &str, parent: &str) -> AppResult<bool> {
    scalar_exists(
        conn,
        &format!(
            "SELECT EXISTS(SELECT 1 FROM tasks ct JOIN tasks pt ON pt.id = ct.parent_id \
             JOIN cycles cc ON cc.id = ct.cycle_id JOIN cycles pc ON pc.id = pt.cycle_id \
             WHERE cc.type = '{child}' AND pc.type = '{parent}' \
               AND ct.proposal IS NULL AND pt.proposal IS NULL \
               AND cc.archived = 0 AND pc.archived = 0)"
        ),
    )
}

/// Step 5 — a finished focus block with 30+ minutes of focused time.
fn step_focus_block_done(conn: &Connection) -> AppResult<bool> {
    scalar_exists(
        conn,
        &format!(
            "SELECT EXISTS(SELECT 1 FROM cycles \
             WHERE type = 'session' AND finished = 1 AND focused_time >= {FOCUS_STEP_MIN_MS})"
        ),
    )
}

/// Derives the five achievement flags from live data. Stateless on purpose:
/// deleted data rolls its step back (1.4) and every reconcile recomputes
/// from scratch (1.3), so no cached progress can go stale.
fn derive_step_flags(conn: &Connection) -> AppResult<[bool; 5]> {
    Ok([
        step_long_term_goal_done(conn)?,
        step_later_item_done(conn)?,
        cross_level_link_exists(conn, "week", "month")?,
        cross_level_link_exists(conn, "day", "week")?,
        step_focus_block_done(conn)?,
    ])
}

fn build_guide(conn: &Connection) -> AppResult<GettingStartedGuide> {
    let state = GuideState::from_stored(settings_repo::get(conn, KEY_GUIDE_STATE)?);
    let flags = derive_step_flags(conn)?;
    let mut steps = Vec::with_capacity(GUIDE_STEP_IDS.len());
    let mut completed = 0usize;
    for (id, done) in GUIDE_STEP_IDS.iter().zip(flags.iter()) {
        let (title, detail) = step_copy(id);
        let status = match (state, done) {
            (_, true) => StepStatus::Done,
            (GuideState::Skipped, false) => StepStatus::Skipped,
            _ => StepStatus::NotDone,
        };
        if *done {
            completed += 1;
        }
        steps.push(GuideStep {
            id: (*id).to_string(),
            title: title.to_string(),
            detail: detail.to_string(),
            status,
        });
    }
    Ok(GettingStartedGuide {
        state,
        completed,
        total: GUIDE_STEP_IDS.len(),
        steps,
    })
}

/// Current guide payload. Progress is derived on every read.
pub fn get_onboarding(db: &Db) -> AppResult<GettingStartedGuide> {
    let conn = db.pool().get()?;
    build_guide(&conn)
}

/// Re-check after cycle/task change events (1.3) — same derivation as
/// [`get_onboarding`]; the command layer exposes both names so the frontend
/// can express intent (refetch vs re-check) without extra semantics.
pub fn reconcile_getting_started_guide(db: &Db) -> AppResult<GettingStartedGuide> {
    get_onboarding(db)
}

/// Marks the guide completed (header may stop displaying). Idempotent.
pub fn complete_onboarding(db: &Db) -> AppResult<GettingStartedGuide> {
    let conn = db.pool().get()?;
    if GuideState::from_stored(settings_repo::get(&conn, KEY_GUIDE_STATE)?) != GuideState::Skipped {
        settings_repo::set(&conn, KEY_GUIDE_STATE, "completed")?;
    }
    build_guide(&conn)
}

#[derive(Debug, Clone, Serialize)]
pub struct SkipReport {
    /// Cycles removed by the skip cleanup (empty leftovers only).
    pub deleted_cycle_ids: Vec<String>,
    /// Empty leftover task rows removed (their parent cycles are in
    /// `deleted_cycle_ids` or remain untouched).
    pub deleted_task_count: usize,
}

/// Skips the whole guide (1.5): persists the state so the header never
/// proactively shows again, and cleans up onboarding leftovers (1.6):
///
/// - empty tasks: blank title, nothing completed, no checklist content, no
///   child rows — the seeded placeholder rows;
/// - empty cycles: untitled, never started, zero focus time, not the Later
///   container, not attached to a repeat template (非重复容器), no child
///   cycles and no tasks left in the subtree.
///
/// Guided data that grew real content is never touched (spec: 引导与真实
/// 数据隔离). Cycles created through the service always carry a title, so
/// for databases written by this app the cleanup is a no-op safety net for
/// seeded/legacy placeholder rows.
pub fn skip_getting_started_guide(db: &Db, _now: i64) -> AppResult<Mutation<SkipReport>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;

    let deleted_tasks = cleanup_empty_tasks(&tx)?;
    let deleted_cycles = cleanup_empty_cycles(&tx)?;
    settings_repo::set(&tx, KEY_GUIDE_STATE, "skipped")?;

    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;

    let mut mutation = Mutation::new(SkipReport {
        deleted_cycle_ids: deleted_cycles
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<Vec<String>>(),
        deleted_task_count: deleted_tasks.1.len(),
    });
    for cycle_id in &deleted_tasks.0 {
        mutation.tasks.push(cycle_id.clone());
    }
    for (id, parent) in &deleted_cycles {
        mutation.cycles.push(id.clone());
        if let Some(parent) = parent {
            mutation.cycles.push(parent.clone());
        }
    }
    Ok(mutation)
}

/// Removes empty placeholder task rows; returns `(cycle_id, task_id)` pairs.
fn cleanup_empty_tasks(conn: &Connection) -> AppResult<(Vec<String>, Vec<String>)> {
    let mut stmt = conn
        .prepare(
            "SELECT id, cycle_id FROM tasks t \
             WHERE TRIM(t.title) = '' AND t.completed = 0 AND t.proposal IS NULL \
               AND t.subtasks = '[]' \
               AND NOT EXISTS (SELECT 1 FROM tasks k WHERE k.parent_id = t.id)",
        )
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    let mut cycle_ids = Vec::new();
    let mut task_ids = Vec::new();
    for (id, cycle_id) in rows {
        tasks_repo::delete(conn, &id)?;
        cycle_ids.push(cycle_id);
        task_ids.push(id);
    }
    Ok((cycle_ids, task_ids))
}

/// Removes empty leftover cycles; returns `(id, parent_id)` pairs. Evaluation
/// happens after the empty-task pass so a cycle that only held placeholders
/// qualifies in the same skip.
fn cleanup_empty_cycles(conn: &Connection) -> AppResult<Vec<(String, Option<String>)>> {
    let mut stmt = conn
        .prepare(
            "SELECT c.id, c.parent_id FROM cycles c \
             WHERE c.id != 'later' AND c.repeat_id IS NULL AND TRIM(c.title) = '' \
               AND c.started = 0 AND c.focused_time = 0 \
               AND NOT EXISTS (SELECT 1 FROM cycles k WHERE k.parent_id = c.id) \
               AND NOT EXISTS (SELECT 1 FROM tasks t WHERE t.cycle_id = c.id)",
        )
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    let mut deleted = Vec::new();
    for (id, parent) in rows {
        // Subtree-wide belt-and-braces: the direct-child checks above make
        // this zero already, but the aggregate stays authoritative if the
        // candidate query ever loosens.
        if cycles_repo::count_tasks(conn, &id)? > 0 {
            continue;
        }
        cycles_repo::delete(conn, &id)?;
        deleted.push((id, parent));
    }
    Ok(deleted)
}

// ---------------------------------------------------------------------------
// §2 One-shot hints — dismissed-id persistence
// ---------------------------------------------------------------------------

/// Every hint id ever dismissed, sorted for stable comparisons.
pub fn get_dismissed_hints(db: &Db) -> AppResult<Vec<String>> {
    let conn = db.pool().get()?;
    dismissed_hints(&conn)
}

fn dismissed_hints(conn: &Connection) -> AppResult<Vec<String>> {
    match settings_repo::get(conn, KEY_DISMISSED_HINTS)? {
        None => Ok(Vec::new()),
        Some(raw) => serde_json::from_str(&raw)
            .map_err(|e| AppError::Internal(format!("corrupt dismissed hints: {e}"))),
    }
}

/// Records a hint as dismissed (2.2). The registry itself (id + trigger +
/// copy) lives in the frontend; the backend only validates the id shape so a
/// typo cannot smuggle arbitrary data into settings.
pub fn dismiss_hint(db: &Db, hint_id: &str) -> AppResult<Vec<String>> {
    validate_hint_id(hint_id)?;
    let conn = db.pool().get()?;
    let mut ids = dismissed_hints(&conn)?;
    if !ids.iter().any(|existing| existing == hint_id) {
        ids.push(hint_id.to_string());
        ids.sort();
        settings_repo::set(
            &conn,
            KEY_DISMISSED_HINTS,
            &serde_json::to_string(&ids).map_err(|e| AppError::Internal(e.to_string()))?,
        )?;
    }
    Ok(ids)
}

fn validate_hint_id(hint_id: &str) -> AppResult<()> {
    let ok = !hint_id.is_empty()
        && hint_id.len() <= 64
        && hint_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if ok {
        Ok(())
    } else {
        Err(AppError::validation(
            "invalid_hint_id",
            "hint ids are 1..=64 characters of [a-zA-Z0-9_-]",
        ))
    }
}

// ---------------------------------------------------------------------------
// §3 Exit poll — condition, one-shot guard, local staging
// ---------------------------------------------------------------------------

/// Enough engagement to be worth asking: either three finished focus blocks
/// or 30 accumulated minutes of focus.
pub const EXIT_POLL_MIN_FINISHED_SESSIONS: i64 = 3;
pub const EXIT_POLL_MIN_FOCUSED_MS: i64 = 30 * 60 * 1000;

#[derive(Debug, Clone, Serialize)]
pub struct ExitPollPresentation {
    pub show: bool,
    /// `eligible` | `already_shown` | `not_eligible` — why (not) presenting.
    pub reason: &'static str,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ExitPollAnswers {
    pub rating: Option<i64>,
    pub reason: Option<String>,
    pub detail: Option<String>,
}

/// Whether the behaviour-based condition holds (3.1).
pub fn exit_poll_eligible(conn: &Connection) -> AppResult<bool> {
    let finished: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cycles WHERE type = 'session' AND finished = 1",
            [],
            |r| r.get(0),
        )
        .map_err(from_rusqlite)?;
    if finished >= EXIT_POLL_MIN_FINISHED_SESSIONS {
        return Ok(true);
    }
    let focused: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(focused_time), 0) FROM cycles WHERE type = 'session'",
            [],
            |r| r.get(0),
        )
        .map_err(from_rusqlite)?;
    Ok(focused >= EXIT_POLL_MIN_FOCUSED_MS)
}

/// The frontend signals readiness, then receives its presentation decision.
/// Nothing is ever pushed before this call, so an unready frontend cannot
/// lose the poll (3.7): the decision is derived here, once, at ready-time.
/// `force` is the manual menu entry (3.6): it bypasses both guards but still
/// records the shown timestamp so the automatic path stays once-only.
pub fn mark_exit_poll_listener_ready(
    db: &Db,
    now: i64,
    force: bool,
) -> AppResult<ExitPollPresentation> {
    let conn = db.pool().get()?;
    if !force {
        if settings_repo::get(&conn, KEY_EXIT_POLL_SHOWN_AT)?.is_some() {
            return Ok(ExitPollPresentation {
                show: false,
                reason: "already_shown",
            });
        }
        if !exit_poll_eligible(&conn)? {
            return Ok(ExitPollPresentation {
                show: false,
                reason: "not_eligible",
            });
        }
    }
    settings_repo::set(&conn, KEY_EXIT_POLL_SHOWN_AT, &now.to_string())?;
    settings_repo::set(&conn, KEY_EXIT_POLL_STATE, "shown")?;
    Ok(ExitPollPresentation {
        show: true,
        reason: "eligible",
    })
}

/// The dialog is on screen (3.3).
pub fn acknowledge_exit_poll_shown(db: &Db, _now: i64) -> AppResult<()> {
    let conn = db.pool().get()?;
    settings_repo::set(&conn, KEY_EXIT_POLL_STATE, "acknowledged")?;
    Ok(())
}

/// User submitted answers: stage the outcome locally and move the state
/// machine on (3.4/3.5). 本版本无上报通道：条目写入 `staged_events` 队列，
/// 附带版本与匿名标识，等待用户显式导出。
pub fn submit_exit_poll(db: &Db, now: i64, answers: &ExitPollAnswers) -> AppResult<StagedEvent> {
    let conn = db.pool().get()?;
    let payload = serde_json::to_value(answers).map_err(|e| AppError::Internal(e.to_string()))?;
    let event = stage_event(&conn, now, "exit_poll", payload)?;
    settings_repo::set(&conn, KEY_EXIT_POLL_STATE, "submitted")?;
    Ok(event)
}

/// User closed the panel without answering: record the ignore locally, then
/// the app may exit (3.4).
pub fn dismiss_exit_poll(db: &Db, now: i64) -> AppResult<StagedEvent> {
    let conn = db.pool().get()?;
    let event = stage_event(&conn, now, "exit_poll_dismissed", serde_json::Value::Null)?;
    settings_repo::set(&conn, KEY_EXIT_POLL_STATE, "dismissed")?;
    Ok(event)
}

/// User chose to stay (3.4): not a survey outcome — state only, no queue
/// entry.
pub fn continue_after_exit_poll(db: &Db, _now: i64) -> AppResult<()> {
    let conn = db.pool().get()?;
    settings_repo::set(&conn, KEY_EXIT_POLL_STATE, "continued")?;
    Ok(())
}

/// User confirmed the exit after submitting (3.4): the frontend performs the
/// actual quit; this only records the decision.
pub fn exit_after_exit_poll(db: &Db, _now: i64) -> AppResult<()> {
    let conn = db.pool().get()?;
    settings_repo::set(&conn, KEY_EXIT_POLL_STATE, "exit_confirmed")?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Staged local queues (shared by §3 and §4)
// ---------------------------------------------------------------------------

/// Cap per queue so settings rows stay small; oldest entries drop first.
const STAGED_QUEUE_CAP: usize = 200;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedEvent {
    pub id: String,
    pub kind: String,
    pub created_at: i64,
    pub app_version: String,
    pub os: String,
    pub anonymous_id: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StagedFeedback {
    pub id: String,
    pub created_at: i64,
    pub app_version: String,
    pub os: String,
    pub anonymous_id: String,
    pub message: String,
}

/// The app version this build was compiled from (§5 self-introspection and
/// the diagnostic context on every staged item).
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// OS label for diagnostic context; no version probe exists without adding a
/// dependency, so arch accompanies the kernel name.
fn os_label() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Reads (creating on first use) the stable anonymous identifier.
fn ensure_anonymous_id(conn: &Connection) -> AppResult<String> {
    if let Some(id) = settings_repo::get(conn, KEY_ANONYMOUS_ID)? {
        if !id.trim().is_empty() {
            return Ok(id);
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    settings_repo::set(conn, KEY_ANONYMOUS_ID, &id)?;
    Ok(id)
}

fn read_queue<T: DeserializeOwned>(conn: &Connection, key: &str) -> AppResult<Vec<T>> {
    match settings_repo::get(conn, key)? {
        None => Ok(Vec::new()),
        Some(raw) => serde_json::from_str(&raw)
            .map_err(|e| AppError::Internal(format!("corrupt queue {key}: {e}"))),
    }
}

fn write_queue<T: Serialize>(conn: &Connection, key: &str, entries: &[T]) -> AppResult<()> {
    let raw = serde_json::to_string(entries).map_err(|e| AppError::Internal(e.to_string()))?;
    settings_repo::set(conn, key, &raw)
}

fn stage_event(
    conn: &Connection,
    now: i64,
    kind: &str,
    payload: serde_json::Value,
) -> AppResult<StagedEvent> {
    let anonymous_id = ensure_anonymous_id(conn)?;
    let event = StagedEvent {
        id: uuid::Uuid::new_v4().to_string(),
        kind: kind.to_string(),
        created_at: now,
        app_version: app_version().to_string(),
        os: os_label(),
        anonymous_id,
        payload,
    };
    let mut queue = read_queue::<StagedEvent>(conn, KEY_STAGED_EVENTS)?;
    queue.push(event.clone());
    if queue.len() > STAGED_QUEUE_CAP {
        queue.drain(..queue.len() - STAGED_QUEUE_CAP);
    }
    write_queue(conn, KEY_STAGED_EVENTS, &queue)?;
    Ok(event)
}

// ---------------------------------------------------------------------------
// §4 Feedback & talk-to-founder
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct FeedbackReceipt {
    pub staged: StagedFeedback,
    pub queue_len: usize,
}

/// Stages user feedback locally (4.1). Empty content is rejected with a
/// stable code (4.2); because staging always succeeds on the local disk and
/// nothing is ever "sent", content can never be lost to a failed upload —
/// the queue keeps it until the user exports (4.3).
pub fn send_feedback(db: &Db, now: i64, message: &str) -> AppResult<FeedbackReceipt> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation(
            "empty_feedback",
            "Please enter your feedback.",
        ));
    }
    let conn = db.pool().get()?;
    let anonymous_id = ensure_anonymous_id(&conn)?;
    let staged = StagedFeedback {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: now,
        app_version: app_version().to_string(),
        os: os_label(),
        anonymous_id,
        message: trimmed.to_string(),
    };
    let mut queue = read_queue::<StagedFeedback>(&conn, KEY_STAGED_FEEDBACK)?;
    queue.push(staged.clone());
    if queue.len() > STAGED_QUEUE_CAP {
        queue.drain(..queue.len() - STAGED_QUEUE_CAP);
    }
    write_queue(&conn, KEY_STAGED_FEEDBACK, &queue)?;
    Ok(FeedbackReceipt {
        queue_len: queue.len(),
        staged,
    })
}

/// Everything still waiting in the feedback queue — the retry/export view.
pub fn list_staged_feedback(db: &Db) -> AppResult<Vec<StagedFeedback>> {
    let conn = db.pool().get()?;
    read_queue::<StagedFeedback>(&conn, KEY_STAGED_FEEDBACK)
}

#[derive(Debug, Clone, Serialize)]
pub struct TalkToFounderEligibility {
    pub eligible: bool,
    /// The support id the user can copy into the founder channel (4.4) —
    /// the stable anonymous identifier.
    pub support_id: String,
    pub opened_at: Option<i64>,
    pub open_count: i64,
}

/// 资格 = 至少一次复盘或一个完成周期，从数据推导：`cycle_reviews` 里存在
/// 任一复盘快照，或存在任一已结束（finished）的规划周期。本实现自查数据，
/// 不依赖用户申报。
pub fn get_talk_to_founder_eligibility(db: &Db) -> AppResult<TalkToFounderEligibility> {
    let conn = db.pool().get()?;
    let has_review = conn
        .query_row("SELECT EXISTS(SELECT 1 FROM cycle_reviews)", [], |r| {
            r.get::<_, i64>(0)
        })
        .map(|v| v != 0)
        .map_err(from_rusqlite)?;
    let has_finished_cycle = scalar_exists(
        &conn,
        "SELECT EXISTS(SELECT 1 FROM cycles \
         WHERE type IN ('month','week','day') AND finished = 1)",
    )?;
    let opened_at = settings_repo::get(&conn, KEY_TALK_FOUNDER_FIRST_OPENED_AT)?
        .and_then(|raw| raw.parse::<i64>().ok());
    let open_count = settings_repo::get(&conn, KEY_TALK_FOUNDER_OPEN_COUNT)?
        .and_then(|raw| raw.parse::<i64>().ok())
        .unwrap_or(0);
    Ok(TalkToFounderEligibility {
        eligible: has_review || has_finished_cycle,
        support_id: ensure_anonymous_id(&conn)?,
        opened_at,
        open_count,
    })
}

/// Records that the user opened the founder channel; returns the open count.
pub fn record_talk_to_founder_opened(db: &Db, now: i64) -> AppResult<i64> {
    let conn = db.pool().get()?;
    let count = settings_repo::get(&conn, KEY_TALK_FOUNDER_OPEN_COUNT)?
        .and_then(|raw| raw.parse::<i64>().ok())
        .unwrap_or(0)
        + 1;
    settings_repo::set(&conn, KEY_TALK_FOUNDER_OPEN_COUNT, &count.to_string())?;
    if settings_repo::get(&conn, KEY_TALK_FOUNDER_FIRST_OPENED_AT)?.is_none() {
        settings_repo::set(&conn, KEY_TALK_FOUNDER_FIRST_OPENED_AT, &now.to_string())?;
    }
    Ok(count)
}

pub fn close_talk_to_founder(db: &Db, now: i64) -> AppResult<()> {
    let conn = db.pool().get()?;
    settings_repo::set(&conn, KEY_TALK_FOUNDER_CLOSED_AT, &now.to_string())
}

// ---------------------------------------------------------------------------
// §5 Update check — degraded to pure comparison + self-introspection
// ---------------------------------------------------------------------------

/// Three-state feedback of 5.4 plus the degraded `disabled` state: nothing
/// here blocks normal use, and 本版本无更新通道（无拉取、无下载、无签名校验），
/// `update_endpoint` 留给用户配置，配置后由未来版本启用真实检查。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum UpdateCheck {
    /// No endpoint configured — the check feature is off.
    Disabled,
    /// Endpoint configured and the local version compares as latest. (This
    /// build never reaches the network, so today only `disabled`/`failed`
    /// occur at runtime; the variant pins the contract for the real channel.)
    UpToDate,
    Available {
        latest_version: String,
    },
    Failed {
        code: String,
        message: String,
    },
}

/// Reads the reserved endpoint and answers the check. An endpoint without a
/// channel reports a stable failure instead of pretending success.
pub fn check_for_updates(db: &Db) -> AppResult<UpdateCheck> {
    let conn = db.pool().get()?;
    match settings_repo::get(&conn, KEY_UPDATE_ENDPOINT)? {
        None => Ok(UpdateCheck::Disabled),
        Some(_) => Ok(UpdateCheck::Failed {
            code: "update_channel_not_available".into(),
            message: "An update endpoint is configured, but this build has no update channel."
                .into(),
        }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SemVer {
    major: u64,
    minor: u64,
    patch: u64,
    pre: Vec<PreIdentifier>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PreIdentifier {
    Numeric(u64),
    Alphanumeric(String),
}

fn parse_u64(text: &str) -> Result<u64, String> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("invalid numeric component: {text:?}"));
    }
    text.parse::<u64>()
        .map_err(|e| format!("numeric component out of range: {text:?}: {e}"))
}

/// Parses `major.minor.patch[-pre][+build]` (build metadata ignored, leading
/// `v` tolerated). Errors carry a readable reason for the caller.
fn parse_version(input: &str) -> Result<SemVer, String> {
    let trimmed = input.trim().trim_start_matches('v');
    let core = trimmed
        .split('+')
        .next()
        .ok_or_else(|| "empty version".to_string())?;
    let (core, pre) = match core.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (core, None),
    };
    let mut parts = core.split('.');
    let mut next_number = || -> Result<u64, String> {
        let part = parts
            .next()
            .ok_or_else(|| "missing numeric component".to_string())?;
        parse_u64(part)
    };
    let major = next_number()?;
    let minor = next_number()?;
    let patch = next_number()?;
    if parts.next().is_some() {
        return Err(format!("too many components: {input:?}"));
    }
    let mut pre_ids = Vec::new();
    if let Some(pre) = pre {
        if pre.is_empty() {
            return Err(format!("empty prerelease: {input:?}"));
        }
        for id in pre.split('.') {
            if id.is_empty() {
                return Err(format!("empty prerelease identifier: {input:?}"));
            }
            if id.bytes().all(|b| b.is_ascii_digit()) {
                pre_ids.push(PreIdentifier::Numeric(parse_u64(id)?));
            } else {
                if !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
                    return Err(format!("illegal prerelease identifier: {id:?}"));
                }
                pre_ids.push(PreIdentifier::Alphanumeric(id.to_string()));
            }
        }
    }
    Ok(SemVer {
        major,
        minor,
        patch,
        pre: pre_ids,
    })
}

/// Semantic-version ordering with prerelease handling (5.2):
/// `1.0.0-alpha < 1.0.0-alpha.1 < 1.0.0-beta < 1.0.0`, numeric identifiers
/// compare by value and always rank below alphanumeric ones, build metadata
/// is ignored. Returns `Err` with a readable reason when either side does
/// not parse.
pub fn compare_versions(a: &str, b: &str) -> Result<Ordering, String> {
    let left = parse_version(a)?;
    let right = parse_version(b)?;
    Ok(left
        .major
        .cmp(&right.major)
        .then(left.minor.cmp(&right.minor))
        .then(left.patch.cmp(&right.patch))
        .then_with(|| compare_prerelease(&left.pre, &right.pre)))
}

fn compare_prerelease(left: &[PreIdentifier], right: &[PreIdentifier]) -> Ordering {
    match (left.is_empty(), right.is_empty()) {
        (true, true) => return Ordering::Equal,
        // A release outranks any of its prereleases.
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }
    for (l, r) in left.iter().zip(right.iter()) {
        let ord = match (l, r) {
            (PreIdentifier::Numeric(l), PreIdentifier::Numeric(r)) => l.cmp(r),
            (PreIdentifier::Alphanumeric(l), PreIdentifier::Alphanumeric(r)) => l.cmp(r),
            // Numeric identifiers always have lower precedence.
            (PreIdentifier::Numeric(_), PreIdentifier::Alphanumeric(_)) => Ordering::Less,
            (PreIdentifier::Alphanumeric(_), PreIdentifier::Numeric(_)) => Ordering::Greater,
        };
        if ord != Ordering::Equal {
            return ord;
        }
    }
    // A larger set of identifiers outranks a prefix of itself.
    left.len().cmp(&right.len())
}

// ---------------------------------------------------------------------------
// Daily plan time preference + §6 notify gate
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LifecyclePrefs {
    /// `HH:MM` local time the user reserved for planning the day; `None`
    /// until chosen.
    pub daily_plan_time: Option<String>,
}

pub fn get_lifecycle_prefs(db: &Db) -> AppResult<LifecyclePrefs> {
    let conn = db.pool().get()?;
    let daily_plan_time =
        settings_repo::get(&conn, KEY_DAILY_PLAN_TIME)?.filter(|v| is_valid_hhmm(v));
    Ok(LifecyclePrefs { daily_plan_time })
}

/// Sets (or clears with `None`) the daily planning moment.
pub fn set_daily_plan_time(db: &Db, time: Option<String>) -> AppResult<()> {
    let conn = db.pool().get()?;
    match time {
        None => {
            if settings_repo::get(&conn, KEY_DAILY_PLAN_TIME)?.is_some() {
                // app_settings has no delete helper and an empty value reads
                // as "unset" through the validators above.
                settings_repo::set(&conn, KEY_DAILY_PLAN_TIME, "")?;
            }
            Ok(())
        }
        Some(time) => {
            if !is_valid_hhmm(&time) {
                return Err(AppError::validation(
                    "invalid_daily_plan_time",
                    "daily_plan_time must be HH:MM (00:00 through 23:59)",
                ));
            }
            settings_repo::set(&conn, KEY_DAILY_PLAN_TIME, &time)
        }
    }
}

fn is_valid_hhmm(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 5 || bytes[2] != b':' {
        return false;
    }
    let hour: u32 = match text[..2].parse() {
        Ok(h) => h,
        Err(_) => return false,
    };
    let minute: u32 = match text[3..].parse() {
        Ok(m) => m,
        Err(_) => return false,
    };
    hour <= 23 && minute <= 59
}

/// §6 gate: only sessions with a set (positive) duration produce a due
/// notification (无时长的专注块不产生通知). The permission-denied path never
/// errors upward — the command maps it to a silent skip.
pub fn should_notify_session_due(duration_ms: Option<i64>) -> bool {
    duration_ms.map(|d| d > 0).unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::cycles::{self, AddSessionArgs};
    use crate::service::tasks::{self, AddTaskArgs, TaskPatch};

    struct TestDb {
        db: Db,
        _dir: tempfile::TempDir,
    }

    impl TestDb {
        fn open() -> Self {
            let dir = tempfile::tempdir().expect("tempdir");
            let path = dir.path().join("planner.db");
            let db = crate::db::open_at(&path).expect("open db");
            Self { db, _dir: dir }
        }

        fn conn_for_test(&self) -> r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager> {
            self.db.pool().get().expect("pool connection")
        }
    }

    const NOW: i64 = 1_700_000_000_000;
    const TODAY: &str = "2026-09-16";

    fn today() -> chrono::NaiveDate {
        crate::domain::calendar::parse_date(TODAY).unwrap()
    }

    fn long_term(db: &Db) -> crate::domain::cycle::Cycle {
        cycles::create_planning_cycle(
            db,
            &cycles::CreateCycleArgs {
                cycle_type: "month".into(),
                duration_months: Some(1),
                ..Default::default()
            },
            today(),
            NOW,
        )
        .unwrap()
        .value
    }

    fn week_under(db: &Db, parent: &str) -> crate::domain::cycle::Cycle {
        cycles::create_planning_cycle(
            db,
            &cycles::CreateCycleArgs {
                cycle_type: "week".into(),
                parent_id: Some(parent.into()),
                ..Default::default()
            },
            today(),
            NOW,
        )
        .unwrap()
        .value
    }

    fn day_under(db: &Db, parent: &str) -> crate::domain::cycle::Cycle {
        cycles::create_planning_cycle(
            db,
            &cycles::CreateCycleArgs {
                cycle_type: "day".into(),
                parent_id: Some(parent.into()),
                date: Some(TODAY.into()),
                ..Default::default()
            },
            today(),
            NOW,
        )
        .unwrap()
        .value
    }

    fn add_task(db: &Db, cycle_id: &str, title: &str) -> crate::domain::task::Task {
        tasks::add_task(
            db,
            &AddTaskArgs {
                cycle_id: cycle_id.into(),
                title: title.into(),
                ..Default::default()
            },
            NOW,
        )
        .unwrap()
        .value
    }

    /// Finishes a 31-minute focus block inside `day`.
    fn finish_focus_block(db: &Db, day: &str) -> crate::domain::cycle::Cycle {
        let session = cycles::add_session(
            db,
            &AddSessionArgs {
                task_id: None,
                day_cycle_id: day.into(),
                title: "deep work".into(),
                duration_ms: Some(50 * 60 * 1000),
                position: None,
            },
            NOW,
        )
        .unwrap()
        .value;
        cycles::start_cycle(db, &session.id, NOW).unwrap();
        cycles::finish_cycle(db, &session.id, NOW + 31 * 60 * 1000)
            .unwrap()
            .value
    }

    fn step_status(guide: &GettingStartedGuide, id: &str) -> StepStatus {
        guide
            .steps
            .iter()
            .find(|s| s.id == id)
            .unwrap_or_else(|| panic!("missing step {id}"))
            .status
    }

    fn link(db: &Db, child: &crate::domain::task::Task, parent: &crate::domain::task::Task) {
        tasks::set_task_parent_link(db, &child.id, Some(&parent.id)).unwrap();
    }

    // -- §1: fresh state, per-step achievement, rollback ---------------------

    #[test]
    fn fresh_database_starts_at_zero_of_five() {
        let test = TestDb::open();
        let guide = get_onboarding(&test.db).unwrap();
        assert_eq!(guide.state, GuideState::Active);
        assert_eq!(guide.completed, 0);
        assert_eq!(guide.total, 5);
        assert!(guide.steps.iter().all(|s| s.status == StepStatus::NotDone));
    }

    #[test]
    fn clarified_long_term_goal_completes_step_one_only() {
        let test = TestDb::open();
        let month = long_term(&test.db);
        let goal = add_task(&test.db, &month.id, "Ship the rebuild");
        // Manual goals start "needs refinement"; clarification settles it.
        tasks::patch_task(
            &test.db,
            &goal.id,
            &TaskPatch {
                needs_refinement: Some(Some(false)),
                ..Default::default()
            },
        )
        .unwrap();
        let guide = get_onboarding(&test.db).unwrap();
        assert_eq!(step_status(&guide, "set_long_term_goal"), StepStatus::Done);
        assert_eq!(guide.completed, 1);
        assert_eq!(step_status(&guide, "add_later_item"), StepStatus::NotDone);
    }

    #[test]
    fn later_item_completes_step_two() {
        let test = TestDb::open();
        add_task(&test.db, LATER_CYCLE_ID, "Someday maybe");
        let guide = get_onboarding(&test.db).unwrap();
        assert_eq!(step_status(&guide, "add_later_item"), StepStatus::Done);
    }

    #[test]
    fn cross_level_links_complete_steps_three_and_four() {
        let test = TestDb::open();
        let month = long_term(&test.db);
        let goal = add_task(&test.db, &month.id, "Big goal");
        let week = week_under(&test.db, &month.id);
        let weekly = add_task(&test.db, &week.id, "Week step");
        link(&test.db, &weekly, &goal);
        let day = day_under(&test.db, &week.id);
        let daily = add_task(&test.db, &day.id, "Today step");
        link(&test.db, &daily, &weekly);

        let guide = get_onboarding(&test.db).unwrap();
        assert_eq!(
            step_status(&guide, "link_week_to_long_term"),
            StepStatus::Done
        );
        assert_eq!(step_status(&guide, "link_day_to_week"), StepStatus::Done);
    }

    #[test]
    fn finished_half_hour_focus_block_completes_step_five() {
        let test = TestDb::open();
        let month = long_term(&test.db);
        let week = week_under(&test.db, &month.id);
        let day = day_under(&test.db, &week.id);
        finish_focus_block(&test.db, &day.id);

        let guide = get_onboarding(&test.db).unwrap();
        assert_eq!(
            step_status(&guide, "complete_focus_block"),
            StepStatus::Done
        );
    }

    #[test]
    fn short_focus_block_does_not_complete_step_five() {
        let test = TestDb::open();
        let month = long_term(&test.db);
        let week = week_under(&test.db, &month.id);
        let day = day_under(&test.db, &week.id);
        let session = cycles::add_session(
            &test.db,
            &AddSessionArgs {
                task_id: None,
                day_cycle_id: day.id.clone(),
                title: "too short".into(),
                duration_ms: Some(50 * 60 * 1000),
                position: None,
            },
            NOW,
        )
        .unwrap()
        .value;
        cycles::start_cycle(&test.db, &session.id, NOW).unwrap();
        cycles::finish_cycle(&test.db, &session.id, NOW + 10 * 60 * 1000).unwrap();

        let guide = get_onboarding(&test.db).unwrap();
        assert_eq!(
            step_status(&guide, "complete_focus_block"),
            StepStatus::NotDone
        );
    }

    #[test]
    fn deleting_evidence_rolls_the_step_back() {
        let test = TestDb::open();
        let month = long_term(&test.db);
        let goal = add_task(&test.db, &month.id, "Rollback target");
        tasks::patch_task(
            &test.db,
            &goal.id,
            &TaskPatch {
                needs_refinement: Some(Some(false)),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            step_status(&get_onboarding(&test.db).unwrap(), "set_long_term_goal"),
            StepStatus::Done
        );

        tasks::delete_task(&test.db, &goal.id).unwrap();
        assert_eq!(
            step_status(&get_onboarding(&test.db).unwrap(), "set_long_term_goal"),
            StepStatus::NotDone
        );
    }

    #[test]
    fn reconcile_matches_get_and_is_event_safe() {
        let test = TestDb::open();
        add_task(&test.db, LATER_CYCLE_ID, "Later note");
        let reconciled = reconcile_getting_started_guide(&test.db).unwrap();
        let fetched = get_onboarding(&test.db).unwrap();
        assert_eq!(reconciled.completed, fetched.completed);
        assert_eq!(reconciled.completed, 1);
    }

    // -- §1: complete & skip --------------------------------------------------

    #[test]
    fn complete_onboarding_persists_and_is_idempotent() {
        let test = TestDb::open();
        let guide = complete_onboarding(&test.db).unwrap();
        assert_eq!(guide.state, GuideState::Completed);
        let again = complete_onboarding(&test.db).unwrap();
        assert_eq!(again.state, GuideState::Completed);
        // Completing must not lock out a later skip.
        skip_getting_started_guide(&test.db, NOW).unwrap();
        assert_eq!(get_onboarding(&test.db).unwrap().state, GuideState::Skipped);
    }

    #[test]
    fn skip_persists_and_marks_undone_steps_skipped() {
        let test = TestDb::open();
        add_task(&test.db, LATER_CYCLE_ID, "Kept item");
        let mutation = skip_getting_started_guide(&test.db, NOW).unwrap();
        assert!(mutation.value.deleted_cycle_ids.is_empty());
        assert_eq!(mutation.value.deleted_task_count, 0);

        let guide = get_onboarding(&test.db).unwrap();
        assert_eq!(guide.state, GuideState::Skipped);
        assert_eq!(step_status(&guide, "add_later_item"), StepStatus::Done);
        assert_eq!(
            step_status(&guide, "set_long_term_goal"),
            StepStatus::Skipped
        );
    }

    #[test]
    fn skip_cleanup_removes_empty_cycles_and_tasks_without_touching_real_data() {
        let test = TestDb::open();
        // Leftovers: an untitled, never-started month cycle with only an
        // empty placeholder task; and a repeat-generated session (repeat_id
        // set) that must survive.
        let leftover = crate::repository::cycles::insert(
            &test.conn_for_test(),
            &crate::repository::cycles::NewCycle {
                id: "leftover-month".into(),
                title: "  ".into(),
                cycle_type: crate::domain::cycle::CycleType::Month,
                parent_id: None,
                position: 99,
                duration: None,
                starts_on: None,
                ends_on: None,
                calendar_key: None,
                repeat_id: None,
                task_id: None,
                created_at: NOW,
            },
        )
        .unwrap();
        let _ = leftover;
        crate::repository::tasks::insert(
            &test.conn_for_test(),
            &crate::repository::tasks::NewTask {
                id: "leftover-task".into(),
                cycle_id: "leftover-month".into(),
                parent_id: None,
                title: "".into(),
                subtasks: vec![],
                position: 0,
                completed: false,
                goal_breakdown: None,
                needs_refinement: None,
                needs_breakdown: None,
                root_color_key: None,
                copied_from_task_id: None,
                created_at: NOW,
            },
        )
        .unwrap();

        let real = long_term(&test.db);
        let real_goal = add_task(&test.db, &real.id, "Real goal");
        let report = skip_getting_started_guide(&test.db, NOW).unwrap().value;

        assert_eq!(report.deleted_cycle_ids, vec!["leftover-month"]);
        assert_eq!(report.deleted_task_count, 1);
        assert!(
            crate::repository::cycles::get(&test.conn_for_test(), "leftover-month")
                .unwrap()
                .is_none()
        );
        assert!(
            crate::repository::tasks::get(&test.conn_for_test(), "leftover-task")
                .unwrap()
                .is_none()
        );
        assert!(
            crate::repository::cycles::get(&test.conn_for_test(), &real.id)
                .unwrap()
                .is_some()
        );
        assert!(
            crate::repository::tasks::get(&test.conn_for_test(), &real_goal.id)
                .unwrap()
                .is_some()
        );
        // The Later container always survives.
        assert!(
            crate::repository::cycles::get(&test.conn_for_test(), LATER_CYCLE_ID)
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn skip_cleanup_keeps_repeat_linked_and_titled_cycles() {
        let test = TestDb::open();
        let conn = test.conn_for_test();
        // The template row must exist: cycles.repeat_id is a real FK.
        crate::repository::repeats::insert(
            &conn,
            &crate::repository::repeats::NewRepeat {
                id: "tpl-1".into(),
                title: "Weekly deep work".into(),
                duration: 50 * 60 * 1000,
                position: 0,
            },
        )
        .unwrap();
        crate::repository::cycles::insert(
            &conn,
            &crate::repository::cycles::NewCycle {
                id: "repeat-child".into(),
                title: "".into(),
                cycle_type: crate::domain::cycle::CycleType::Session,
                parent_id: None,
                position: 0,
                duration: None,
                starts_on: None,
                ends_on: None,
                calendar_key: None,
                repeat_id: Some("tpl-1".into()),
                task_id: None,
                created_at: NOW,
            },
        )
        .unwrap();
        crate::repository::cycles::insert(
            &conn,
            &crate::repository::cycles::NewCycle {
                id: "titled-month".into(),
                title: "Someone's month".into(),
                cycle_type: crate::domain::cycle::CycleType::Month,
                parent_id: None,
                position: 1,
                duration: None,
                starts_on: None,
                ends_on: None,
                calendar_key: None,
                repeat_id: None,
                task_id: None,
                created_at: NOW,
            },
        )
        .unwrap();

        let report = skip_getting_started_guide(&test.db, NOW).unwrap().value;
        assert!(report.deleted_cycle_ids.is_empty());
        assert_eq!(report.deleted_task_count, 0);
        assert!(crate::repository::cycles::get(&conn, "repeat-child")
            .unwrap()
            .is_some());
        assert!(crate::repository::cycles::get(&conn, "titled-month")
            .unwrap()
            .is_some());
    }

    // -- §2: hints -------------------------------------------------------------

    #[test]
    fn hints_start_empty_and_persist_dismissals() {
        let test = TestDb::open();
        assert!(get_dismissed_hints(&test.db).unwrap().is_empty());
        dismiss_hint(&test.db, "later-explainer").unwrap();
        dismiss_hint(&test.db, "guide-explainer").unwrap();
        // Duplicate dismissal is a no-op.
        dismiss_hint(&test.db, "later-explainer").unwrap();
        assert_eq!(
            get_dismissed_hints(&test.db).unwrap(),
            vec!["guide-explainer".to_string(), "later-explainer".to_string()]
        );
    }

    #[test]
    fn hint_ids_are_validated() {
        let test = TestDb::open();
        assert!(dismiss_hint(&test.db, "").is_err());
        assert!(dismiss_hint(&test.db, "bad id with spaces").is_err());
        assert!(dismiss_hint(&test.db, &"x".repeat(65)).is_err());
        assert!(dismiss_hint(&test.db, "ok-id_1").is_ok());
    }

    // -- §3: exit poll ----------------------------------------------------------

    #[test]
    fn exit_poll_not_eligible_on_a_fresh_database() {
        let test = TestDb::open();
        let presentation = mark_exit_poll_listener_ready(&test.db, NOW, false).unwrap();
        assert!(!presentation.show);
        assert_eq!(presentation.reason, "not_eligible");
        assert!(
            settings_repo::get(&test.conn_for_test(), KEY_EXIT_POLL_SHOWN_AT)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn exit_poll_eligible_after_three_finished_sessions_or_thirty_minutes() {
        let test = TestDb::open();
        let month = long_term(&test.db);
        let week = week_under(&test.db, &month.id);
        let day = day_under(&test.db, &week.id);
        for i in 0..3 {
            let session = cycles::add_session(
                &test.db,
                &AddSessionArgs {
                    task_id: None,
                    day_cycle_id: day.id.clone(),
                    title: format!("block {i}"),
                    duration_ms: Some(25 * 60 * 1000),
                    position: None,
                },
                NOW + i,
            )
            .unwrap()
            .value;
            cycles::start_cycle(&test.db, &session.id, NOW).unwrap();
            // 3 finished short blocks accumulate 30 minutes in total.
            cycles::finish_cycle(&test.db, &session.id, NOW + 10 * 60 * 1000).unwrap();
        }
        let conn = test.conn_for_test();
        assert!(exit_poll_eligible(&conn).unwrap());
    }

    #[test]
    fn exit_poll_shows_once_and_records_the_timestamp() {
        let test = TestDb::open();
        let month = long_term(&test.db);
        let week = week_under(&test.db, &month.id);
        let day = day_under(&test.db, &week.id);
        finish_focus_block(&test.db, &day.id);

        let first = mark_exit_poll_listener_ready(&test.db, NOW, false).unwrap();
        assert!(first.show);
        let shown_at = settings_repo::get(&test.conn_for_test(), KEY_EXIT_POLL_SHOWN_AT)
            .unwrap()
            .unwrap();
        assert_eq!(shown_at, NOW.to_string());

        // Second quit: never auto-shown again (3.2), even after more usage.
        finish_focus_block(&test.db, &day.id);
        let second = mark_exit_poll_listener_ready(&test.db, NOW + 1, false).unwrap();
        assert!(!second.show);
        assert_eq!(second.reason, "already_shown");
    }

    #[test]
    fn exit_poll_manual_force_bypasses_guards_but_still_records() {
        let test = TestDb::open();
        let forced = mark_exit_poll_listener_ready(&test.db, NOW, true).unwrap();
        assert!(forced.show);
        assert!(
            settings_repo::get(&test.conn_for_test(), KEY_EXIT_POLL_SHOWN_AT)
                .unwrap()
                .is_some()
        );
        // After a forced show the automatic path is suppressed too.
        let auto = mark_exit_poll_listener_ready(&test.db, NOW + 1, false).unwrap();
        assert!(!auto.show);
        assert_eq!(auto.reason, "already_shown");
    }

    #[test]
    fn exit_poll_outcomes_reach_the_local_queue_and_state_machine() {
        let test = TestDb::open();
        let month = long_term(&test.db);
        let week = week_under(&test.db, &month.id);
        let day = day_under(&test.db, &week.id);
        finish_focus_block(&test.db, &day.id);

        assert!(
            mark_exit_poll_listener_ready(&test.db, NOW, false)
                .unwrap()
                .show
        );
        acknowledge_exit_poll_shown(&test.db, NOW + 1).unwrap();
        assert_eq!(
            settings_repo::get(&test.conn_for_test(), KEY_EXIT_POLL_STATE).unwrap(),
            Some("acknowledged".into())
        );

        let event = submit_exit_poll(
            &test.db,
            NOW + 2,
            &ExitPollAnswers {
                rating: Some(2),
                reason: Some("missing feature".into()),
                detail: None,
            },
        )
        .unwrap();
        assert_eq!(event.kind, "exit_poll");
        assert_eq!(event.app_version, app_version());
        assert!(!event.anonymous_id.is_empty());
        assert_eq!(event.payload["rating"], 2);
        let queue = read_queue::<StagedEvent>(&test.conn_for_test(), KEY_STAGED_EVENTS).unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(
            settings_repo::get(&test.conn_for_test(), KEY_EXIT_POLL_STATE).unwrap(),
            Some("submitted".into())
        );

        let dismissed = dismiss_exit_poll(&test.db, NOW + 3).unwrap();
        assert_eq!(dismissed.kind, "exit_poll_dismissed");
        assert_eq!(
            settings_repo::get(&test.conn_for_test(), KEY_EXIT_POLL_STATE).unwrap(),
            Some("dismissed".into())
        );
    }

    #[test]
    fn continue_and_exit_record_state_without_queue_entries() {
        let test = TestDb::open();
        continue_after_exit_poll(&test.db, NOW).unwrap();
        assert_eq!(
            settings_repo::get(&test.conn_for_test(), KEY_EXIT_POLL_STATE).unwrap(),
            Some("continued".into())
        );
        exit_after_exit_poll(&test.db, NOW + 1).unwrap();
        assert_eq!(
            settings_repo::get(&test.conn_for_test(), KEY_EXIT_POLL_STATE).unwrap(),
            Some("exit_confirmed".into())
        );
        assert!(
            read_queue::<StagedEvent>(&test.conn_for_test(), KEY_STAGED_EVENTS)
                .unwrap()
                .is_empty()
        );
    }

    // -- §4: feedback & talk-to-founder -----------------------------------------

    #[test]
    fn feedback_rejects_blank_and_stages_with_context() {
        let test = TestDb::open();
        let err = send_feedback(&test.db, NOW, "   \n\t").unwrap_err();
        match err {
            AppError::Validation { code, message } => {
                assert_eq!(code, "empty_feedback");
                assert!(message.contains("Please enter your feedback."));
            }
            other => panic!("expected validation, got {other:?}"),
        }
        assert!(list_staged_feedback(&test.db).unwrap().is_empty());

        let receipt = send_feedback(&test.db, NOW + 1, "  Love the focus blocks  ").unwrap();
        assert_eq!(receipt.queue_len, 1);
        assert_eq!(receipt.staged.message, "Love the focus blocks");
        assert_eq!(receipt.staged.app_version, app_version());
        assert!(!receipt.staged.os.is_empty());
        assert_eq!(receipt.staged.os, os_label());

        let queued = list_staged_feedback(&test.db).unwrap();
        assert_eq!(queued.len(), 1);
        assert_eq!(queued[0].id, receipt.staged.id);
        // Retry simply stages another entry; nothing was lost.
        send_feedback(&test.db, NOW + 2, "Love the focus blocks").unwrap();
        assert_eq!(list_staged_feedback(&test.db).unwrap().len(), 2);
    }

    #[test]
    fn feedback_keeps_an_anonymous_id_stable() {
        let test = TestDb::open();
        let first = send_feedback(&test.db, NOW, "one").unwrap();
        let second = send_feedback(&test.db, NOW + 1, "two").unwrap();
        assert_eq!(first.staged.anonymous_id, second.staged.anonymous_id);
    }

    #[test]
    fn talk_to_founder_requires_a_review_or_a_finished_cycle() {
        let test = TestDb::open();
        let fresh = get_talk_to_founder_eligibility(&test.db).unwrap();
        assert!(!fresh.eligible);
        assert!(!fresh.support_id.is_empty());

        // A finished planning cycle qualifies.
        let month = long_term(&test.db);
        let week = week_under(&test.db, &month.id);
        let day = day_under(&test.db, &week.id);
        cycles::start_cycle(&test.db, &day.id, NOW).unwrap();
        cycles::finish_cycle(&test.db, &day.id, NOW + 1000).unwrap();
        assert!(get_talk_to_founder_eligibility(&test.db).unwrap().eligible);
    }

    #[test]
    fn talk_to_founder_open_and_close_are_recorded() {
        let test = TestDb::open();
        assert_eq!(record_talk_to_founder_opened(&test.db, NOW).unwrap(), 1);
        assert_eq!(record_talk_to_founder_opened(&test.db, NOW + 1).unwrap(), 2);
        let state = get_talk_to_founder_eligibility(&test.db).unwrap();
        assert_eq!(state.open_count, 2);
        assert_eq!(state.opened_at, Some(NOW));
        close_talk_to_founder(&test.db, NOW + 2).unwrap();
    }

    // -- §5: version comparison & degraded check ---------------------------------

    #[test]
    fn version_comparison_boundaries() {
        use compare_versions as cmp;
        let ord = |a: &str, b: &str| cmp(a, b).unwrap();
        // same version
        assert_eq!(ord("1.0.0", "1.0.0"), Ordering::Equal);
        assert_eq!(ord("v1.2.3", "1.2.3"), Ordering::Equal);
        // build metadata ignored
        assert_eq!(ord("1.0.0+build.5", "1.0.0"), Ordering::Equal);
        // numeric field boundaries
        assert_eq!(ord("1.2.3", "1.10.0"), Ordering::Less);
        assert_eq!(ord("1.9.9", "2.0.0"), Ordering::Less);
        assert_eq!(ord("0.9.0", "1.0.0"), Ordering::Less);
        assert_eq!(ord("2.0.1", "2.0.0"), Ordering::Greater);
        // prereleases sort before the release
        assert_eq!(ord("1.0.0-alpha", "1.0.0"), Ordering::Less);
        assert_eq!(ord("1.0.0-alpha", "1.0.0-alpha.1"), Ordering::Less);
        assert_eq!(ord("1.0.0-alpha.1", "1.0.0-beta"), Ordering::Less);
        assert_eq!(ord("1.0.0-beta", "1.0.0"), Ordering::Less);
        assert_eq!(ord("1.0.0-rc.2", "1.0.0-rc.10"), Ordering::Less);
        // numeric identifiers rank below alphanumeric ones
        assert_eq!(ord("1.0.0-1", "1.0.0-alpha"), Ordering::Less);
        // a prerelease of an older version never beats a newer release
        assert_eq!(ord("1.0.0-alpha", "1.0.1"), Ordering::Less);
    }

    #[test]
    fn version_comparison_rejects_malformed_input() {
        assert!(compare_versions("1.0", "1.0.0").is_err());
        assert!(compare_versions("one.two.three", "1.0.0").is_err());
        assert!(compare_versions("1.0.0.1", "1.0.0").is_err());
        assert!(compare_versions("1.0.0-", "1.0.0").is_err());
        assert!(compare_versions("", "1.0.0").is_err());
    }

    #[test]
    fn update_check_is_disabled_without_endpoint() {
        let test = TestDb::open();
        assert_eq!(check_for_updates(&test.db).unwrap(), UpdateCheck::Disabled);
    }

    #[test]
    fn update_check_reports_channel_unavailable_with_endpoint_set() {
        let test = TestDb::open();
        settings_repo::set(
            &test.conn_for_test(),
            KEY_UPDATE_ENDPOINT,
            "https://example.test",
        )
        .unwrap();
        match check_for_updates(&test.db).unwrap() {
            UpdateCheck::Failed { code, .. } => assert_eq!(code, "update_channel_not_available"),
            other => panic!("expected failed, got {other:?}"),
        }
    }

    // -- daily plan time -----------------------------------------------------------

    #[test]
    fn daily_plan_time_validates_hhmm() {
        let test = TestDb::open();
        assert_eq!(get_lifecycle_prefs(&test.db).unwrap().daily_plan_time, None);
        set_daily_plan_time(&test.db, Some("07:30".into())).unwrap();
        assert_eq!(
            get_lifecycle_prefs(&test.db).unwrap().daily_plan_time,
            Some("07:30".into())
        );
        for bad in ["24:00", "12:60", "7:30", "0730", "07-30", ""] {
            assert!(set_daily_plan_time(&test.db, Some(bad.into())).is_err());
        }
        set_daily_plan_time(&test.db, None).unwrap();
        assert_eq!(get_lifecycle_prefs(&test.db).unwrap().daily_plan_time, None);
    }

    // -- §6: notify gate -------------------------------------------------------------

    #[test]
    fn session_due_gate_requires_a_positive_duration() {
        assert!(!should_notify_session_due(None));
        assert!(!should_notify_session_due(Some(0)));
        assert!(!should_notify_session_due(Some(-1000)));
        assert!(should_notify_session_due(Some(25 * 60 * 1000)));
    }
}

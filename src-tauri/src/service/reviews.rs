//! Cycle review use cases (change: add-review-retrospective): fact
//! computation, snapshot save/read, unfinished-item dispositions, cross-cycle
//! summary and Markdown export.
//!
//! Behaviour contract: `openspec/specs/review-retrospective/spec.md` (delta in
//! `openspec/changes/add-review-retrospective/specs/review-retrospective/`).
//!
//! Invariants this module owns:
//! * Facts are **query results, never model output**: completion, focused
//!   time, link coverage and the unfinished list come from the database and
//!   are frozen into the snapshot at save time (§事实快照不可追溯改写).
//!   Reads return the snapshot verbatim; later data changes never rewrite it.
//! * One review per cycle (`cycle_reviews.cycle_id` UNIQUE): saving again is
//!   an overwrite that keeps the row id and `created_at`, so the recorded
//!   dispositions survive a re-save.
//! * Completion counting (§事实先于判断 / §2.3 mixed levels): a cycle is
//!   measured by its **leaf items** — a top-level row expands into the
//!   lower-cycle items linked to it (week item → long-term goal, day task →
//!   week item) until a row without lower links, or a completed row, is
//!   reached. Each leaf is therefore counted exactly once; a goal linked from
//!   three week items contributes those three items, never itself. Empty
//!   input rows (design.md §5.1) and pending proposals are never counted. A
//!   cycle with no countable content reports `has_content = false` and a
//!   `completion_rate` of `None` — the UI shows「无可复盘内容」, not 0%.
//! * Dispositions (§未完成项的去向): `carry` copies the item (with its
//!   same-cycle subtree) into the next dated sibling using the same lineage
//!   semantics as `cycles::copy_uncompleted_from_previous`
//!   (`copied_from_task_id`, copies start uncompleted); `later` moves the
//!   item — subtree intact — into the Do Later container via
//!   `service::tasks::move_task`; `drop` records the decision without any
//!   copy or move. Undecided items get no row at all.

use std::collections::{HashMap, HashSet};

use chrono::TimeZone;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::db::Db;
use crate::domain::cycle::{Cycle, CycleType, LATER_CYCLE_ID};
use crate::domain::task::{is_empty_input_row, Task};
use crate::error::{AppError, AppResult};
use crate::repository::{cycles as cycles_repo, reviews as repo, tasks as tasks_repo};
use crate::service::tasks as tasks_service;
use crate::service::Mutation;

// ---------------------------------------------------------------------------
// The fixed question set (§结构化提问)
// ---------------------------------------------------------------------------

/// One entry of the fixed review question set. `id` is the stable wire key on
/// every answer; `text` is the user-facing prompt. The review skill's system
/// prompt (`ai::agent::prompt::REVIEW_PROMPT`) presents the same ids.
pub struct ReviewQuestion {
    pub id: &'static str,
    pub text: &'static str,
}

/// The four spec-mandated questions, in asking order.
pub const REVIEW_QUESTIONS: [ReviewQuestion; 4] = [
    ReviewQuestion {
        id: "what_went_well",
        text: "这个周期什么做得好？",
    },
    ReviewQuestion {
        id: "what_held_you_back",
        text: "什么拖住了你？",
    },
    ReviewQuestion {
        id: "where_plan_diverged",
        text: "计划与实际偏离在哪里？",
    },
    ReviewQuestion {
        id: "one_change_next",
        text: "下一周期要改变的一件事是什么？",
    },
];

pub fn question_text(id: &str) -> Option<&'static str> {
    REVIEW_QUESTIONS
        .iter()
        .find(|question| question.id == id)
        .map(|question| question.text)
}

/// Answer status wire values (§3.3): `{ id, status: answered|skipped, text }`.
pub const STATUS_ANSWERED: &str = "answered";
pub const STATUS_SKIPPED: &str = "skipped";

/// Review `kind` values: `facts_only` when every question was skipped (spec:
/// 全部跳过 → 结论标记为「仅事实」), `full` otherwise.
pub const KIND_FULL: &str = "full";
pub const KIND_FACTS_ONLY: &str = "facts_only";

// ---------------------------------------------------------------------------
// Facts (§2)
// ---------------------------------------------------------------------------

/// One unfinished leaf item of the cycle, as the disposition UI addresses it.
/// `cycle_type` is the item's own cycle, which may be a lower level than the
/// reviewed cycle (a month review lists its weeks' items).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IncompleteItem {
    pub task_id: String,
    pub title: String,
    pub cycle_id: String,
    pub cycle_type: String,
}

/// The query-derived facts frozen into every review snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CycleReviewFacts {
    pub cycle_id: String,
    /// False when the cycle has no countable content — the「无可复盘内容」
    /// branch that must not render as 0% (§2.2).
    pub has_content: bool,
    pub total_items: i64,
    pub completed_items: i64,
    /// `None` exactly when `has_content` is false.
    pub completion_rate: Option<f64>,
    /// Accumulated focus milliseconds of the cycle row (sessions accrue up
    /// the chain on finish).
    pub focused_time_ms: i64,
    /// Direct lower-cycle items linked to this cycle's rows (§2.1 链接覆盖率).
    pub linked_lower_items: i64,
    pub incomplete: Vec<IncompleteItem>,
}

/// The visible rows whose `parent_id` points at `task_id` from another cycle
/// — the cross-level links a unit expands into.
fn linked_lower_tasks(conn: &Connection, task_id: &str) -> AppResult<Vec<Task>> {
    let ids = repo::linked_lower_task_ids(conn, task_id)?;
    let mut tasks = Vec::with_capacity(ids.len());
    for id in ids {
        tasks.push(tasks_repo::require(conn, &id)?);
    }
    Ok(tasks)
}

/// Walks one row down its link tree and appends the measurement leaves. A row
/// without lower links — and any **completed** row — is a leaf: a completed
/// item closes its branch because its outcome is already settled. Empty input
/// rows (typing affordances) never become leaves; a row whose only links are
/// empty rows stays a leaf itself.
fn collect_units(conn: &Connection, task: &Task, out: &mut Vec<Task>) -> AppResult<()> {
    if task.completed {
        out.push(task.clone());
        return Ok(());
    }
    let linked: Vec<Task> = linked_lower_tasks(conn, &task.id)?
        .into_iter()
        .filter(|child| !is_empty_input_row(child))
        .collect();
    if linked.is_empty() {
        out.push(task.clone());
    } else {
        for child in linked {
            collect_units(conn, &child, out)?;
        }
    }
    Ok(())
}

/// Computes the current facts of one planning cycle. Pure read: nothing here
/// writes, and the result is only persisted by [`save_cycle_review`].
pub fn compute_facts(conn: &Connection, cycle_id: &str) -> AppResult<CycleReviewFacts> {
    let cycle = cycles_repo::require(conn, cycle_id)?;
    let rows = tasks_repo::list_visible_by_cycle(conn, cycle_id)?;
    let row_ids: HashSet<&str> = rows.iter().map(|task| task.id.as_str()).collect();

    let mut units: Vec<Task> = Vec::new();
    let mut linked_lower_items: i64 = 0;
    for row in &rows {
        if is_empty_input_row(row) {
            continue;
        }
        // Direct lower-cycle links of this cycle's rows (链接覆盖率).
        let linked = linked_lower_tasks(conn, &row.id)?;
        linked_lower_items += linked.len() as i64;
        // Top-level rows start a unit; same-cycle subtask rows hang under
        // their parent's unit — unless lower items link to them directly, in
        // which case they become a branch of their own so no linked leaf is
        // silently dropped from the count.
        let is_root = row
            .parent_id
            .as_deref()
            .map_or(true, |parent| !row_ids.contains(parent));
        if is_root || !linked.is_empty() {
            collect_units(conn, row, &mut units)?;
        }
    }

    let total = units.len() as i64;
    let completed = units.iter().filter(|task| task.completed).count() as i64;
    let incomplete = units
        .iter()
        .filter(|task| !task.completed)
        .map(|task| IncompleteItem {
            task_id: task.id.clone(),
            title: task.title.clone(),
            cycle_id: task.cycle_id.clone(),
            cycle_type: cycle_type_of(conn, &task.cycle_id),
        })
        .collect();
    Ok(CycleReviewFacts {
        cycle_id: cycle.id,
        has_content: total > 0,
        total_items: total,
        completed_items: completed,
        completion_rate: if total > 0 {
            Some(completed as f64 / total as f64)
        } else {
            None
        },
        focused_time_ms: cycle_focused_time(conn, cycle_id)?,
        linked_lower_items,
        incomplete,
    })
}

fn cycle_focused_time(conn: &Connection, cycle_id: &str) -> AppResult<i64> {
    Ok(cycles_repo::require(conn, cycle_id)?.focused_time)
}

/// Best-effort cycle type for an item's own cycle; a missing cycle (deleted
/// between the walk and this lookup) degrades to `unknown`.
fn cycle_type_of(conn: &Connection, cycle_id: &str) -> String {
    cycles_repo::get(conn, cycle_id)
        .ok()
        .flatten()
        .map(|cycle| cycle.cycle_type.as_str().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

// ---------------------------------------------------------------------------
// Save / read (§3)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnswerOutcome {
    pub id: String,
    pub status: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct SaveReviewArgs {
    pub cycle_id: String,
    /// Any subset of the fixed questions — a partial save is a legal state
    /// the user continues later (spec: 未决定即离开 / partially-saved).
    #[serde(default)]
    pub answers: Vec<ReviewAnswerInput>,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct ReviewAnswerInput {
    pub id: String,
    pub status: String,
    #[serde(default)]
    pub text: String,
}

/// One recorded outcome for an unfinished item.
pub const DISPOSITION_CARRY: &str = "carry";
pub const DISPOSITION_LATER: &str = "later";
pub const DISPOSITION_DROP: &str = "drop";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DispositionRecord {
    pub task_id: String,
    pub disposition: String,
}

/// The stored review as IPC returns it — every value comes from the frozen
/// snapshot, never from a re-computation (§3.2).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CycleReviewView {
    pub id: String,
    pub cycle_id: String,
    pub kind: String,
    pub is_final: bool,
    pub facts: CycleReviewFacts,
    pub answers: Vec<AnswerOutcome>,
    pub dispositions: Vec<DispositionRecord>,
    pub snapshot_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

fn parse_answers(json: &str) -> AppResult<Vec<AnswerOutcome>> {
    serde_json::from_str(json).map_err(|error| {
        AppError::Internal(format!("stored review answers are not valid JSON: {error}"))
    })
}

fn parse_facts(json: &str) -> AppResult<CycleReviewFacts> {
    serde_json::from_str(json).map_err(|error| {
        AppError::Internal(format!("stored review facts are not valid JSON: {error}"))
    })
}

fn to_view(row: repo::ReviewRow, conn: &Connection) -> AppResult<CycleReviewView> {
    let dispositions = repo::list_dispositions(conn, &row.id)?
        .into_iter()
        .map(|(task_id, disposition)| DispositionRecord {
            task_id,
            disposition,
        })
        .collect();
    Ok(CycleReviewView {
        id: row.id,
        cycle_id: row.cycle_id,
        kind: row.kind,
        is_final: row.is_final,
        facts: parse_facts(&row.facts_json)?,
        answers: parse_answers(&row.answers_json)?,
        dispositions,
        snapshot_at: row.snapshot_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// The saved review of one cycle, or `None` when it was never reviewed.
pub fn get_cycle_review(db: &Db, cycle_id: &str) -> AppResult<Option<CycleReviewView>> {
    let conn = db.pool().get()?;
    match repo::get_by_cycle(&conn, cycle_id)? {
        Some(row) => Ok(Some(to_view(row, &conn)?)),
        None => Ok(None),
    }
}

fn validate_answers(answers: &[ReviewAnswerInput]) -> AppResult<()> {
    let mut seen: HashSet<&str> = HashSet::new();
    for answer in answers {
        if !seen.insert(answer.id.as_str()) {
            return Err(AppError::validation(
                "duplicate_answer",
                format!("question '{}' answered twice", answer.id),
            ));
        }
        if question_text(&answer.id).is_none() {
            return Err(AppError::validation(
                "unknown_question",
                format!("unknown review question id '{}'", answer.id),
            ));
        }
        if answer.status != STATUS_ANSWERED && answer.status != STATUS_SKIPPED {
            return Err(AppError::validation(
                "invalid_answer_status",
                format!("answer status must be '{STATUS_ANSWERED}' or '{STATUS_SKIPPED}'"),
            ));
        }
        if answer.status == STATUS_ANSWERED && answer.text.trim().is_empty() {
            return Err(AppError::validation(
                "invalid_answer",
                "an answered question needs text",
            ));
        }
    }
    Ok(())
}

/// Creates or overwrites the cycle's review, freezing the current facts and
/// the given answers into the snapshot. Deliberately emits **no events**: a
/// review changes no cycle/task data, so the Mutation/emit pattern has
/// nothing to broadcast — the panel and the entry-point hook invalidate their
/// own queries after saving.
pub fn save_cycle_review(db: &Db, args: &SaveReviewArgs, now: i64) -> AppResult<CycleReviewView> {
    validate_answers(&args.answers)?;
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;

    let cycle = cycles_repo::require(&tx, &args.cycle_id)?;
    if cycle.cycle_type == CycleType::Session {
        return Err(AppError::validation(
            "unsupported_cycle_type",
            "Focus blocks do not have cycle reviews",
        ));
    }

    let facts = compute_facts(&tx, &args.cycle_id)?;
    let kind = if args
        .answers
        .iter()
        .any(|answer| answer.status == STATUS_ANSWERED)
    {
        KIND_FULL
    } else {
        KIND_FACTS_ONLY
    };
    // A review is terminal when the cycle has already ended; an early review
    // stays marked non-final (spec: 提前复盘…标注为非终结复盘).
    let is_final = cycle.finished;
    let facts_json =
        serde_json::to_string(&facts).map_err(|e| AppError::Internal(e.to_string()))?;
    let answers_json = serde_json::to_string(
        &args
            .answers
            .iter()
            .map(|answer| AnswerOutcome {
                id: answer.id.clone(),
                status: answer.status.clone(),
                text: answer.text.clone(),
            })
            .collect::<Vec<_>>(),
    )
    .map_err(|e| AppError::Internal(e.to_string()))?;

    match repo::get_by_cycle(&tx, &args.cycle_id)? {
        Some(_) => {
            repo::update_by_cycle(
                &tx,
                &args.cycle_id,
                kind,
                is_final,
                &facts_json,
                &answers_json,
                now,
                now,
            )?;
            let row = repo::get_by_cycle(&tx, &args.cycle_id)?.expect("row just updated");
            tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
            let conn = db.pool().get()?;
            Ok(to_view(row, &conn)?)
        }
        None => {
            let new = repo::NewReview {
                id: uuid::Uuid::new_v4().to_string(),
                cycle_id: args.cycle_id.clone(),
                kind: kind.to_string(),
                is_final,
                facts_json,
                answers_json,
                snapshot_at: now,
                created_at: now,
                updated_at: now,
            };
            repo::insert(&tx, &new)?;
            let row = repo::get_by_cycle(&tx, &args.cycle_id)?.expect("row just inserted");
            tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
            let conn = db.pool().get()?;
            Ok(to_view(row, &conn)?)
        }
    }
}

// ---------------------------------------------------------------------------
// Dispositions (§4)
// ---------------------------------------------------------------------------

fn parse_disposition(value: &str) -> AppResult<&'static str> {
    match value {
        DISPOSITION_CARRY => Ok(DISPOSITION_CARRY),
        DISPOSITION_LATER => Ok(DISPOSITION_LATER),
        DISPOSITION_DROP => Ok(DISPOSITION_DROP),
        other => Err(AppError::validation(
            "invalid_disposition",
            format!("unknown disposition '{other}'"),
        )),
    }
}

/// The closest later dated sibling (same type, same parent) —「下一周期」,
/// the mirror of `repository::cycles::previous_dated_sibling`.
fn next_dated_sibling(conn: &Connection, cycle: &Cycle) -> AppResult<Option<Cycle>> {
    let parent = cycle.parent_id.as_deref().unwrap_or("");
    let starts_on = cycle.starts_on.as_deref().unwrap_or("");
    conn.query_row(
        "SELECT id, title, type, parent_id, position, archived, started, finished, \
         started_at, finished_at, duration, focused_time, starts_on, ends_on, calendar_key, \
         repeat_id, created_at \
         FROM cycles \
         WHERE type = ?1 AND parent_id = ?2 AND starts_on IS NOT NULL AND starts_on > ?3 \
         ORDER BY starts_on ASC LIMIT 1",
        rusqlite::params![cycle.cycle_type.as_str(), parent, starts_on],
        cycles_repo::row_to_cycle,
    )
    .optional()
    .map_err(crate::error::from_rusqlite)
}

/// Visible rows of the item's own cycle, parents before children, starting at
/// `root` — the same-cycle subtree a carry copies (spec: 跨层级搬运不丢子项).
fn same_cycle_subtree(conn: &Connection, root: &Task) -> AppResult<Vec<Task>> {
    let rows = tasks_repo::list_visible_by_cycle(conn, &root.cycle_id)?;
    let ids: HashSet<&str> = rows.iter().map(|task| task.id.as_str()).collect();
    let mut children_of: HashMap<&str, Vec<&Task>> = HashMap::new();
    for task in &rows {
        if let Some(parent) = task.parent_id.as_deref() {
            if ids.contains(parent) {
                children_of.entry(parent).or_default().push(task);
            }
        }
    }
    let mut out = vec![root.clone()];
    let mut index = 0;
    while index < out.len() {
        let current = out[index].clone();
        if let Some(children) = children_of.get(current.id.as_str()) {
            for child in children {
                out.push((*child).clone());
            }
        }
        index += 1;
    }
    Ok(out)
}

/// carry: copies the item — same-cycle subtree included — into the next dated
/// sibling, mirroring `cycles::copy_uncompleted_from_previous`'s semantics:
/// fresh rows, `copied_from_task_id` lineage on every copy, copies start
/// uncompleted, cross-cycle parent links of the root are preserved, and
/// top-level copies append after the target's existing rows. The disposition
/// row is written in the same transaction.
fn carry_item(db: &Db, review_id: &str, task_id: &str, now: i64) -> AppResult<Mutation<()>> {
    let mut conn = db.pool().get()?;
    let tx = conn
        .transaction()
        .map_err(|e| AppError::Db(e.to_string()))?;
    let root = tasks_repo::require(&tx, task_id)?;
    let item_cycle = cycles_repo::require(&tx, &root.cycle_id)?;
    let target = next_dated_sibling(&tx, &item_cycle)?.ok_or_else(|| {
        AppError::conflict(
            "no_next_cycle",
            "There is no next cycle to carry this item into; create it first.",
        )
    })?;

    let subtree = same_cycle_subtree(&tx, &root)?;
    let mut id_map: HashMap<String, String> = HashMap::new();
    let mut next_top_position = tasks_repo::max_position(&tx, &target.id, None)? + 1;

    for original in &subtree {
        let is_root = original.id == root.id;
        let new_parent = if is_root {
            // Same-cycle parents cannot happen (the item is a cycle root);
            // cross-cycle links carry over so the copy serves the same goal.
            original.parent_id.clone()
        } else {
            // Parents are copied before their children, so the mapping is
            // always present for a same-cycle descendant.
            original
                .parent_id
                .as_ref()
                .and_then(|parent| id_map.get(parent).cloned())
        };
        let position = if is_root {
            let position = next_top_position;
            next_top_position += 1;
            position
        } else {
            original.position
        };
        let new_id = uuid::Uuid::new_v4().to_string();
        let new_task = tasks_repo::NewTask {
            id: new_id.clone(),
            cycle_id: target.id.clone(),
            parent_id: new_parent,
            title: original.title.clone(),
            subtasks: original.subtasks.clone(),
            position,
            completed: false,
            goal_breakdown: original.goal_breakdown.clone(),
            needs_refinement: original.needs_refinement,
            needs_breakdown: original.needs_breakdown,
            root_color_key: original.root_color_key.clone(),
            copied_from_task_id: Some(original.id.clone()),
            created_at: now,
        };
        tasks_repo::insert(&tx, &new_task)?;
        id_map.insert(original.id.clone(), new_id);
    }
    repo::set_disposition(&tx, review_id, task_id, DISPOSITION_CARRY)?;
    tx.commit().map_err(|e| AppError::Db(e.to_string()))?;

    let mut mutation = Mutation::new(()).touching_tasks(target.id.clone());
    mutation.tasks.push(root.cycle_id);
    Ok(mutation)
}

fn unit_mutation<T>(mutation: Mutation<T>) -> Mutation<()> {
    let mut out = Mutation::new(());
    for id in mutation.tasks.ids() {
        out.tasks.push(id);
    }
    for id in mutation.cycles.ids() {
        out.cycles.push(id);
    }
    out
}

/// Records one unfinished item's outcome and performs its side effect.
/// Applying the same disposition twice is a no-op; changing a decision
/// replaces the record — an already-performed move or copy is not reversed
/// (the spec defines one-way outcomes, not an undo).
///
/// No events beyond the touched cycle/task sets: the disposition itself lives
/// on the review, not on the workspace columns.
pub fn apply_review_disposition(
    db: &Db,
    cycle_id: &str,
    task_id: &str,
    disposition: &str,
    now: i64,
) -> AppResult<Mutation<()>> {
    let disposition = parse_disposition(disposition)?;
    let (review, existing) = {
        let conn = db.pool().get()?;
        let review = repo::get_by_cycle(&conn, cycle_id)?
            .ok_or_else(|| AppError::not_found("review", cycle_id))?;
        let existing = repo::get_disposition(&conn, &review.id, task_id)?;
        (review, existing)
    };
    // The item must exist (and stay addressable); it may live in a lower
    // cycle than the reviewed one.
    {
        let conn = db.pool().get()?;
        tasks_repo::require(&conn, task_id)?;
    }
    if existing.as_deref() == Some(disposition) {
        return Ok(Mutation::new(()));
    }

    let mut mutation = Mutation::new(());
    match disposition {
        DISPOSITION_CARRY => {
            let carried = carry_item(db, &review.id, task_id, now)?;
            mutation = unit_mutation(carried);
        }
        DISPOSITION_LATER => {
            // move_task carries the same-cycle subtree along and enforces the
            // usual mutability rules (an ended source cycle refuses the move
            // with the stable `cycle_ended` code).
            let moved = tasks_service::move_task(db, task_id, LATER_CYCLE_ID, None)?;
            mutation = unit_mutation(moved);
            let conn = db.pool().get()?;
            repo::set_disposition(&conn, &review.id, task_id, disposition)?;
        }
        DISPOSITION_DROP => {
            // No copy, no move: the item stays in its (historical) cycle and
            // the decision is the record (spec: 放弃 → 不出现在下一周期，
            // 也不进入暂存容器).
            let conn = db.pool().get()?;
            repo::set_disposition(&conn, &review.id, task_id, disposition)?;
        }
        _ => unreachable!("parse_disposition covers every arm"),
    }
    Ok(mutation)
}

// ---------------------------------------------------------------------------
// Summary (§跨周期汇总) and export (§复盘可导出)
// ---------------------------------------------------------------------------

/// One point of the cross-cycle trend, read from the **saved snapshots** —
/// never recomputed from current data.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReviewSummaryPoint {
    pub cycle_id: String,
    pub cycle_title: String,
    pub cycle_type: String,
    pub kind: String,
    pub is_final: bool,
    pub completion_rate: Option<f64>,
    pub focused_time_ms: i64,
    pub snapshot_at: i64,
}

pub fn review_summary(db: &Db) -> AppResult<Vec<ReviewSummaryPoint>> {
    let conn = db.pool().get()?;
    repo::list_summary(&conn)?
        .into_iter()
        .map(|row| {
            let facts = parse_facts(&row.facts_json)?;
            Ok(ReviewSummaryPoint {
                cycle_id: row.cycle_id,
                cycle_title: row.cycle_title,
                cycle_type: row.cycle_type,
                kind: row.kind,
                is_final: row.is_final,
                completion_rate: facts.completion_rate,
                focused_time_ms: facts.focused_time_ms,
                snapshot_at: row.snapshot_at,
            })
        })
        .collect()
}

fn format_duration_ms(ms: i64) -> String {
    let minutes = ms / 60_000;
    let hours = minutes / 60;
    let rest = minutes % 60;
    match (hours, rest) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

fn format_snapshot_at(ms: i64) -> String {
    chrono::Local
        .timestamp_opt(ms, 0)
        .single()
        .map(|time| time.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| ms.to_string())
}

fn disposition_label(disposition: &str) -> &'static str {
    match disposition {
        DISPOSITION_CARRY => "Carried into the next cycle",
        DISPOSITION_LATER => "Moved back to Do Later",
        DISPOSITION_DROP => "Dropped",
        _ => "Unknown outcome",
    }
}

/// Renders the saved review as Markdown. Every optional section (answers,
/// outcomes) is emitted only when it has content — no empty headings
/// (§7.1). Returns `review_not_found` when the cycle has no saved review.
pub fn export_cycle_review_markdown(db: &Db, cycle_id: &str) -> AppResult<String> {
    let conn = db.pool().get()?;
    let row = repo::get_by_cycle(&conn, cycle_id)?
        .ok_or_else(|| AppError::not_found("review", cycle_id))?;
    let cycle = cycles_repo::require(&conn, &row.cycle_id)?;
    let facts = parse_facts(&row.facts_json)?;
    let answers = parse_answers(&row.answers_json)?;
    let dispositions = repo::list_dispositions(&conn, &row.id)?;

    let finality = if row.is_final {
        "final review"
    } else {
        "interim review (cycle not finished)"
    };
    let mut out = String::new();
    out.push_str(&format!("# Cycle Review — {}\n\n", cycle.title));
    out.push_str(&format!(
        "_Snapshotted {} · {finality}_\n\n",
        format_snapshot_at(row.snapshot_at)
    ));

    out.push_str("## Facts\n\n");
    if facts.has_content {
        let percent = (facts.completion_rate.unwrap_or(0.0) * 100.0).round() as i64;
        out.push_str(&format!(
            "- Completion: {}/{} ({percent}%)\n",
            facts.completed_items, facts.total_items
        ));
    } else {
        out.push_str("- This cycle has no reviewable content.\n");
    }
    out.push_str(&format!(
        "- Focused time: {}\n",
        format_duration_ms(facts.focused_time_ms)
    ));
    out.push_str(&format!(
        "- Lower-level items linked: {}\n",
        facts.linked_lower_items
    ));
    if !facts.incomplete.is_empty() {
        out.push_str(&format!(
            "- Unfinished items ({}):\n",
            facts.incomplete.len()
        ));
        for item in &facts.incomplete {
            out.push_str(&format!("  - [ ] {}\n", item.title));
        }
    }

    let exportable: Vec<&AnswerOutcome> = answers
        .iter()
        .filter(|answer| {
            answer.status == STATUS_ANSWERED
                || (answer.status == STATUS_SKIPPED && !answer.text.trim().is_empty())
        })
        .collect();
    if !exportable.is_empty() {
        out.push_str("\n## Answers\n\n");
        for answer in exportable {
            let question = question_text(&answer.id).unwrap_or(answer.id.as_str());
            out.push_str(&format!("### {question}\n\n"));
            if answer.status == STATUS_ANSWERED {
                out.push_str(&format!("{}\n", answer.text.trim()));
            } else {
                out.push_str(&format!("_(skipped)_ {}\n", answer.text.trim()));
            }
        }
    }

    if !dispositions.is_empty() {
        out.push_str("\n## Unfinished item outcomes\n\n");
        for (task_id, disposition) in &dispositions {
            let title = repo::task_title(&conn, task_id)?
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| task_id.clone());
            out.push_str(&format!("- {title} → {}\n", disposition_label(disposition)));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Context injection for the review skill (§5.3): the most recent other
// review's conclusion plus nothing else — never the full history.
// ---------------------------------------------------------------------------

/// Builds the model-facing conclusion of the most recent review that is not
/// `exclude_cycle_id`'s own, or `None` when there is none yet.
pub fn latest_review_context(
    conn: &Connection,
    exclude_cycle_id: &str,
) -> AppResult<Option<serde_json::Value>> {
    let Some(row) = repo::latest_other(conn, exclude_cycle_id)? else {
        return Ok(None);
    };
    let answers: Vec<AnswerOutcome> = parse_answers(&row.answers_json)?;
    let answered: Vec<&AnswerOutcome> = answers
        .iter()
        .filter(|answer| answer.status == STATUS_ANSWERED)
        .collect();
    let cycle_title = cycles_repo::require(conn, &row.cycle_id)?.title;
    let dispositions = repo::list_dispositions(conn, &row.id)?
        .into_iter()
        .map(|(task_id, disposition)| {
            let title = repo::task_title(conn, &task_id)?
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| task_id.clone());
            Ok(serde_json::json!({
                "task_id": task_id,
                "title": title,
                "disposition": disposition,
            }))
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(Some(serde_json::json!({
        "cycle_id": row.cycle_id,
        "cycle_title": cycle_title,
        "snapshot_at": row.snapshot_at,
        "kind": row.kind,
        "answers": answered,
        "dispositions": dispositions,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::calendar::today_local;
    use crate::service::cycles::{create_planning_cycle, CreateCycleArgs};
    use crate::service::tasks::{add_task, set_task_parent_link, AddTaskArgs};

    fn db() -> (tempfile::TempDir, Db) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = crate::db::open_at(&dir.path().join("test.db")).expect("open db");
        (dir, db)
    }

    fn month_cycle(db: &Db, months: i64) -> String {
        create_planning_cycle(
            db,
            &CreateCycleArgs {
                cycle_type: "month".into(),
                duration_months: Some(months),
                ..Default::default()
            },
            today_local(),
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
            today_local(),
            1,
        )
        .expect("week cycle")
        .value
        .id
    }

    fn day_cycle(db: &Db, parent: &str, date: chrono::NaiveDate) -> String {
        create_planning_cycle(
            db,
            &CreateCycleArgs {
                cycle_type: "day".into(),
                parent_id: Some(parent.into()),
                date: Some(date.format("%Y-%m-%d").to_string()),
                ..Default::default()
            },
            today_local(),
            1,
        )
        .expect("day cycle")
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

    fn complete(db: &Db, task_id: &str) {
        crate::service::tasks::patch_task(
            db,
            task_id,
            &crate::service::tasks::TaskPatch {
                completed: Some(true),
                ..Default::default()
            },
        )
        .expect("complete task");
    }

    fn link(db: &Db, task_id: &str, parent_id: &str) {
        set_task_parent_link(db, task_id, Some(parent_id)).expect("link");
    }

    fn answer(id: &str, status: &str, text: &str) -> ReviewAnswerInput {
        ReviewAnswerInput {
            id: id.into(),
            status: status.into(),
            text: text.into(),
        }
    }

    fn facts_of(db: &Db, cycle_id: &str) -> CycleReviewFacts {
        let conn = db.pool().get().unwrap();
        compute_facts(&conn, cycle_id).unwrap()
    }

    // -- §2 facts -------------------------------------------------------------

    #[test]
    fn empty_cycle_reports_no_content_instead_of_zero_percent() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db, 1);
        // The typing affordance row must not turn into content either.
        task(&db, &cycle, "  ");

        let facts = facts_of(&db, &cycle);
        assert!(!facts.has_content);
        assert_eq!(facts.total_items, 0);
        assert_eq!(facts.completion_rate, None);
        assert!(facts.incomplete.is_empty());
    }

    #[test]
    fn all_completed_and_all_uncompleted_rates() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db, 1);
        let done = task(&db, &cycle, "done");
        complete(&db, &done);
        let facts = facts_of(&db, &cycle);
        assert_eq!((facts.total_items, facts.completed_items), (1, 1));
        assert_eq!(facts.completion_rate, Some(1.0));
        assert!(facts.incomplete.is_empty());

        let pending = task(&db, &cycle, "pending");
        let facts = facts_of(&db, &cycle);
        assert_eq!((facts.total_items, facts.completed_items), (2, 1));
        assert_eq!(facts.completion_rate, Some(0.5));
        assert_eq!(
            facts
                .incomplete
                .iter()
                .map(|i| i.task_id.as_str())
                .collect::<Vec<_>>(),
            vec![pending.as_str()]
        );
    }

    #[test]
    fn long_term_cycle_aggregates_linked_lower_items_without_double_counting() {
        let (_dir, db) = db();
        let month = month_cycle(&db, 1);
        let week = week_cycle(&db, &month);
        let day = day_cycle(&db, &week, today_local());

        // Goal with two linked week items (one done): the goal itself is not
        // a unit — its linked items are.
        let goal = task(&db, &month, "goal");
        let w1 = task(&db, &week, "week item 1");
        let w2 = task(&db, &week, "week item 2");
        link(&db, &w1, &goal);
        link(&db, &w2, &goal);
        complete(&db, &w1);

        // Second goal, unlinked: counts itself.
        let lone_goal = task(&db, &month, "lone goal");

        // Week item with a linked day task: the leaf is the day task.
        let w3 = task(&db, &week, "week item 3");
        let d1 = task(&db, &day, "day task 1");
        link(&db, &d1, &w3);

        let facts = facts_of(&db, &month);
        // Units reachable from the month's rows: w1(done), w2, lone_goal —
        // the unlinked week item w3 (and its day task) belong to the week's
        // own review, not the month's.
        assert_eq!(facts.total_items, 3);
        assert_eq!(facts.completed_items, 1);
        assert!((facts.completion_rate.unwrap() - 1.0 / 3.0).abs() < 1e-9);
        // Only w1 + w2 link down into the month's rows.
        assert_eq!(facts.linked_lower_items, 2);
        let unfinished: Vec<&str> = facts
            .incomplete
            .iter()
            .map(|item| item.task_id.as_str())
            .collect();
        assert_eq!(unfinished, vec![w2.as_str(), lone_goal.as_str()]);
        // The week-level item's own review measures at its own level.
        let week_facts = facts_of(&db, &week);
        assert_eq!(week_facts.total_items, 3, "w3 expands into d1");
        assert_eq!(week_facts.completed_items, 1);
    }

    #[test]
    fn completed_linked_item_closes_its_branch() {
        let (_dir, db) = db();
        let month = month_cycle(&db, 1);
        let week = week_cycle(&db, &month);
        let day = day_cycle(&db, &week, today_local());
        let goal = task(&db, &month, "goal");
        let w1 = task(&db, &week, "week item");
        link(&db, &w1, &goal);
        let d1 = task(&db, &day, "day task");
        link(&db, &d1, &w1);
        complete(&db, &w1);

        let facts = facts_of(&db, &month);
        assert_eq!(facts.total_items, 1, "completed week item is the leaf");
        assert_eq!(facts.completed_items, 1);
    }

    #[test]
    fn deleted_child_lets_the_parent_become_the_unit_again() {
        let (_dir, db) = db();
        let month = month_cycle(&db, 1);
        let week = week_cycle(&db, &month);
        let goal = task(&db, &month, "goal");
        let w1 = task(&db, &week, "week item");
        link(&db, &w1, &goal);
        assert_eq!(facts_of(&db, &month).total_items, 1);

        crate::service::tasks::delete_task(&db, &w1).expect("delete");
        let facts = facts_of(&db, &month);
        assert_eq!(facts.total_items, 1);
        assert_eq!(facts.incomplete[0].task_id, goal, "goal counts itself");
        assert_eq!(facts.linked_lower_items, 0);
    }

    #[test]
    fn empty_linked_rows_are_skipped() {
        let (_dir, db) = db();
        let month = month_cycle(&db, 1);
        let week = week_cycle(&db, &month);
        let goal = task(&db, &month, "goal");
        let empty = task(&db, &week, "  ");
        link(&db, &empty, &goal);

        let facts = facts_of(&db, &month);
        assert_eq!(facts.total_items, 1, "empty linked row is not content");
        assert_eq!(facts.incomplete[0].task_id, goal);
    }

    // -- §3 save / snapshot ----------------------------------------------------

    #[test]
    fn resaving_overwrites_the_same_row_and_keeps_created_at() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db, 1);
        task(&db, &cycle, "item");

        let first = save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: cycle.clone(),
                answers: vec![answer("what_went_well", STATUS_ANSWERED, "shipped")],
            },
            100,
        )
        .unwrap();
        let second = save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: cycle.clone(),
                answers: vec![answer("what_went_well", STATUS_ANSWERED, "shipped harder")],
            },
            200,
        )
        .unwrap();
        assert_eq!(first.id, second.id, "re-review is an overwrite");
        assert_eq!(first.created_at, second.created_at);
        assert_eq!(second.snapshot_at, 200);
        assert_eq!(second.answers[0].text, "shipped harder");
        assert_eq!(second.kind, KIND_FULL);
    }

    #[test]
    fn snapshot_survives_later_data_changes() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db, 1);
        let item = task(&db, &cycle, "item");
        save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: cycle.clone(),
                answers: vec![],
            },
            100,
        )
        .unwrap();

        complete(&db, &item);
        crate::repository::cycles::add_focused_time(&db.pool().get().unwrap(), &cycle, 5_000)
            .unwrap();

        let view = get_cycle_review(&db, &cycle).unwrap().unwrap();
        assert_eq!(view.facts.completed_items, 0, "frozen at save time");
        assert_eq!(view.facts.completion_rate, Some(0.0));
        assert_eq!(view.facts.focused_time_ms, 0);
        assert_eq!(view.facts.incomplete.len(), 1);
    }

    #[test]
    fn partial_save_continues_with_a_second_save() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db, 1);
        task(&db, &cycle, "item");

        let partial = save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: cycle.clone(),
                answers: vec![answer("what_went_well", STATUS_ANSWERED, "focus held")],
            },
            100,
        )
        .unwrap();
        assert_eq!(partial.answers.len(), 1);
        assert_eq!(partial.kind, KIND_FULL);

        let continued = save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: cycle.clone(),
                answers: vec![
                    answer("what_went_well", STATUS_ANSWERED, "focus held"),
                    answer("what_held_you_back", STATUS_SKIPPED, "user skipped"),
                ],
            },
            200,
        )
        .unwrap();
        assert_eq!(continued.answers.len(), 2);
        assert_eq!(continued.answers[1].status, STATUS_SKIPPED);
    }

    #[test]
    fn all_skipped_marks_facts_only_and_interim_finality() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db, 1);
        task(&db, &cycle, "item");

        let view = save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: cycle.clone(),
                answers: vec![answer("what_went_well", STATUS_SKIPPED, "not now")],
            },
            100,
        )
        .unwrap();
        assert_eq!(view.kind, KIND_FACTS_ONLY);
        assert!(!view.is_final, "early review of an open cycle");

        crate::repository::cycles::set_lifecycle(
            &db.pool().get().unwrap(),
            &cycle,
            true,
            true,
            Some(1),
            Some(2),
        )
        .unwrap();
        let finished = save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: cycle.clone(),
                answers: vec![],
            },
            300,
        )
        .unwrap();
        assert!(finished.is_final, "ended cycle reviews are final");
        assert_eq!(finished.kind, KIND_FACTS_ONLY, "no answers at all");
    }

    #[test]
    fn answers_are_validated() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db, 1);
        let err = save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: cycle.clone(),
                answers: vec![answer("made_up", STATUS_ANSWERED, "x")],
            },
            1,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Validation { ref code, .. } if code == "unknown_question"));

        let err = save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: cycle.clone(),
                answers: vec![answer("what_went_well", STATUS_ANSWERED, "   ")],
            },
            1,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Validation { ref code, .. } if code == "invalid_answer"));

        let err = save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: cycle.clone(),
                answers: vec![
                    answer("what_went_well", STATUS_SKIPPED, ""),
                    answer("what_went_well", STATUS_ANSWERED, "dup"),
                ],
            },
            1,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Validation { ref code, .. } if code == "duplicate_answer"));
    }

    // -- §4 dispositions --------------------------------------------------------

    fn next_day(base: chrono::NaiveDate) -> chrono::NaiveDate {
        base + chrono::Duration::days(1)
    }

    #[test]
    fn carry_copies_into_the_next_cycle_with_lineage_and_keeps_the_original() {
        let (_dir, db) = db();
        let month = month_cycle(&db, 1);
        let week = week_cycle(&db, &month);
        let today = today_local();
        let day1 = day_cycle(&db, &week, today);
        let day2 = day_cycle(&db, &week, next_day(today));
        let item = task(&db, &day1, "carry me");

        save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: day1.clone(),
                answers: vec![],
            },
            100,
        )
        .unwrap();

        let mutation = apply_review_disposition(&db, &day1, &item, DISPOSITION_CARRY, 200).unwrap();
        assert!(mutation.tasks.ids().contains(&day2));

        let copies =
            crate::repository::tasks::list_visible_by_cycle(&db.pool().get().unwrap(), &day2)
                .unwrap();
        assert_eq!(copies.len(), 1);
        assert_eq!(copies[0].title, "carry me");
        assert_eq!(
            copies[0].copied_from_task_id.as_deref(),
            Some(item.as_str())
        );
        assert!(!copies[0].completed);
        // Original stays in its historical cycle.
        let original = crate::repository::tasks::require(&db.pool().get().unwrap(), &item).unwrap();
        assert_eq!(original.cycle_id, day1);

        // Recorded exactly once; applying again is a no-op.
        let conn = db.pool().get().unwrap();
        let review = crate::repository::reviews::get_by_cycle(&conn, &day1)
            .unwrap()
            .unwrap();
        assert_eq!(
            crate::repository::reviews::list_dispositions(&conn, &review.id).unwrap(),
            vec![(item.clone(), DISPOSITION_CARRY.to_string())]
        );
        let copies_before =
            crate::repository::tasks::list_visible_by_cycle(&db.pool().get().unwrap(), &day2)
                .unwrap()
                .len();
        drop(conn);
        apply_review_disposition(&db, &day1, &item, DISPOSITION_CARRY, 300).unwrap();
        let conn = db.pool().get().unwrap();
        let copies_after =
            crate::repository::tasks::list_visible_by_cycle(&db.pool().get().unwrap(), &day2)
                .unwrap()
                .len();
        assert_eq!(copies_before, copies_after, "repeat carry is a no-op");
    }

    #[test]
    fn carry_keeps_the_same_cycle_subtree() {
        let (_dir, db) = db();
        let month = month_cycle(&db, 1);
        let week = week_cycle(&db, &month);
        let today = today_local();
        let day1 = day_cycle(&db, &week, today);
        let day2 = day_cycle(&db, &week, next_day(today));
        let item = task(&db, &day1, "parent item");
        let child = add_task(
            &db,
            &AddTaskArgs {
                cycle_id: day1.clone(),
                title: "child step".into(),
                parent_id: Some(item.clone()),
                ..Default::default()
            },
            1,
        )
        .unwrap()
        .value
        .id;
        save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: day1.clone(),
                answers: vec![],
            },
            100,
        )
        .unwrap();

        apply_review_disposition(&db, &day1, &item, DISPOSITION_CARRY, 200).unwrap();
        let conn = db.pool().get().unwrap();
        let copies = crate::repository::tasks::list_visible_by_cycle(&conn, &day2).unwrap();
        assert_eq!(copies.len(), 2, "subtree carried along");
        let copy_root = copies.iter().find(|t| t.parent_id.is_none()).unwrap();
        let copy_child = copies.iter().find(|t| t.parent_id.is_some()).unwrap();
        assert_eq!(copy_child.parent_id.as_deref(), Some(copy_root.id.as_str()));
        assert_eq!(
            copy_child.copied_from_task_id.as_deref(),
            Some(child.as_str())
        );
    }

    #[test]
    fn later_moves_the_item_into_the_later_container() {
        let (_dir, db) = db();
        let month = month_cycle(&db, 1);
        let week = week_cycle(&db, &month);
        let item = task(&db, &week, "send to later");
        let child = add_task(
            &db,
            &AddTaskArgs {
                cycle_id: week.clone(),
                title: "still attached".into(),
                parent_id: Some(item.clone()),
                ..Default::default()
            },
            1,
        )
        .unwrap()
        .value
        .id;
        save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: week.clone(),
                answers: vec![],
            },
            100,
        )
        .unwrap();

        apply_review_disposition(&db, &week, &item, DISPOSITION_LATER, 200).unwrap();
        let conn = db.pool().get().unwrap();
        let moved = crate::repository::tasks::require(&conn, &item).unwrap();
        assert_eq!(moved.cycle_id, "later");
        let moved_child = crate::repository::tasks::require(&conn, &child).unwrap();
        assert_eq!(moved_child.cycle_id, "later", "tree structure kept");
        assert!(
            crate::repository::tasks::list_visible_by_cycle(&conn, &week)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn drop_records_without_copying_or_moving() {
        let (_dir, db) = db();
        let month = month_cycle(&db, 1);
        let week = week_cycle(&db, &month);
        let item = task(&db, &week, "give up");
        save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: week.clone(),
                answers: vec![],
            },
            100,
        )
        .unwrap();

        apply_review_disposition(&db, &week, &item, DISPOSITION_DROP, 200).unwrap();
        let conn = db.pool().get().unwrap();
        let original = crate::repository::tasks::require(&conn, &item).unwrap();
        assert_eq!(original.cycle_id, week, "stays put");
        let review = crate::repository::reviews::get_by_cycle(&conn, &week)
            .unwrap()
            .unwrap();
        assert_eq!(
            crate::repository::reviews::list_dispositions(&conn, &review.id).unwrap(),
            vec![(item.clone(), DISPOSITION_DROP.to_string())]
        );
    }

    #[test]
    fn undecided_items_get_no_disposition_row() {
        let (_dir, db) = db();
        let month = month_cycle(&db, 1);
        let week = week_cycle(&db, &month);
        let decided = task(&db, &week, "decided");
        let undecided = task(&db, &week, "undecided");
        save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: week.clone(),
                answers: vec![],
            },
            100,
        )
        .unwrap();

        apply_review_disposition(&db, &week, &decided, DISPOSITION_DROP, 200).unwrap();
        let conn = db.pool().get().unwrap();
        let review = crate::repository::reviews::get_by_cycle(&conn, &week)
            .unwrap()
            .unwrap();
        let recorded = crate::repository::reviews::list_dispositions(&conn, &review.id).unwrap();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].0, decided, "undecided stays unwritten");
        assert_ne!(recorded[0].0, undecided);
    }

    #[test]
    fn dispositions_need_a_saved_review_and_a_known_outcome() {
        let (_dir, db) = db();
        let month = month_cycle(&db, 1);
        let week = week_cycle(&db, &month);
        let item = task(&db, &week, "item");

        let err = apply_review_disposition(&db, &week, &item, DISPOSITION_DROP, 1).unwrap_err();
        assert!(matches!(err, AppError::NotFound { ref entity, .. } if entity == "review"));

        save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: week.clone(),
                answers: vec![],
            },
            100,
        )
        .unwrap();
        let err = apply_review_disposition(&db, &week, &item, "postpone", 1).unwrap_err();
        assert!(
            matches!(err, AppError::Validation { ref code, .. } if code == "invalid_disposition")
        );
    }

    // -- §5.3 latest-review context ---------------------------------------------

    #[test]
    fn latest_review_context_returns_only_the_most_recent_other_review() {
        let (_dir, db) = db();
        let m1 = month_cycle(&db, 1);
        let m2 = month_cycle(&db, 3);
        task(&db, &m1, "a");
        save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: m1.clone(),
                answers: vec![answer("what_went_well", STATUS_ANSWERED, "older")],
            },
            100,
        )
        .unwrap();
        task(&db, &m2, "b");
        save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: m2.clone(),
                answers: vec![answer("one_change_next", STATUS_ANSWERED, "newer")],
            },
            200,
        )
        .unwrap();

        let conn = db.pool().get().unwrap();
        let context = latest_review_context(&conn, &m2).unwrap().unwrap();
        assert_eq!(context["cycle_id"], m1.as_str());
        let answers = context["answers"].as_array().unwrap();
        assert_eq!(answers.len(), 1, "only the single most recent review");
        assert_eq!(answers[0]["text"], "older");

        // Asking for the oldest one's successor context still yields exactly
        // one review — never the full history.
        let context = latest_review_context(&conn, &m1).unwrap().unwrap();
        assert_eq!(context["cycle_id"], m2.as_str());
        drop(conn);

        // A database whose only review belongs to the asking cycle yields no
        // context at all (第一个周期没有历史复盘).
        let dir2 = tempfile::tempdir().unwrap();
        let db2 = crate::db::open_at(&dir2.path().join("test.db")).unwrap();
        let m4 = month_cycle(&db2, 1);
        save_cycle_review(
            &db2,
            &SaveReviewArgs {
                cycle_id: m4.clone(),
                answers: vec![],
            },
            100,
        )
        .unwrap();
        let conn2 = db2.pool().get().unwrap();
        assert!(latest_review_context(&conn2, &m4).unwrap().is_none());
    }

    // -- §7 export ---------------------------------------------------------------

    #[test]
    fn facts_only_export_is_valid_markdown_without_empty_headings() {
        let (_dir, db) = db();
        let month = month_cycle(&db, 1);
        task(&db, &month, "unfinished thing");
        save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: month.clone(),
                answers: vec![],
            },
            100,
        )
        .unwrap();

        let markdown = export_cycle_review_markdown(&db, &month).unwrap();
        assert!(markdown.starts_with("# Cycle Review — "));
        assert!(markdown.contains("## Facts"));
        assert!(markdown.contains("- Completion: 0/1 (0%)"));
        assert!(markdown.contains("- Unfinished items (1):"));
        assert!(markdown.contains("  - [ ] unfinished thing"));
        assert!(!markdown.contains("## Answers"), "no empty heading");
        assert!(
            !markdown.contains("## Unfinished item outcomes"),
            "no empty heading"
        );
        // Every heading line is followed by content.
        for line in markdown.lines().filter(|l| l.starts_with('#')) {
            let index = markdown.find(line).expect("line present");
            let rest = &markdown[index + line.len()..];
            assert!(
                !rest.trim_start_matches('\n').is_empty(),
                "heading {line} is empty"
            );
        }
    }

    #[test]
    fn export_includes_answers_skips_and_outcomes_when_present() {
        let (_dir, db) = db();
        let month = month_cycle(&db, 1);
        let week = week_cycle(&db, &month);
        let item = task(&db, &week, "dropped item");
        save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: week.clone(),
                answers: vec![
                    answer("what_went_well", STATUS_ANSWERED, "steady shipping"),
                    answer("what_held_you_back", STATUS_SKIPPED, "no time to think"),
                ],
            },
            100,
        )
        .unwrap();
        apply_review_disposition(&db, &week, &item, DISPOSITION_DROP, 200).unwrap();

        let markdown = export_cycle_review_markdown(&db, &week).unwrap();
        assert!(markdown.contains("## Answers"));
        assert!(markdown.contains("### 这个周期什么做得好？"));
        assert!(markdown.contains("steady shipping"));
        assert!(markdown.contains("_(skipped)_ no time to think"));
        assert!(markdown.contains("## Unfinished item outcomes"));
        assert!(markdown.contains("- dropped item → Dropped"));
    }

    #[test]
    fn export_requires_a_saved_review() {
        let (_dir, db) = db();
        let cycle = month_cycle(&db, 1);
        let err = export_cycle_review_markdown(&db, &cycle).unwrap_err();
        assert!(matches!(err, AppError::NotFound { ref entity, .. } if entity == "review"));
    }

    // -- summary ------------------------------------------------------------------

    #[test]
    fn summary_reads_snapshots_in_snapshot_order() {
        let (_dir, db) = db();
        let m1 = month_cycle(&db, 1);
        let m2 = month_cycle(&db, 3);
        let item = task(&db, &m1, "half done");
        save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: m1.clone(),
                answers: vec![],
            },
            100,
        )
        .unwrap();
        task(&db, &m2, "later cycle item");
        complete(&db, &item);
        // m2's own snapshot is its own: fully completed at its save time.
        let m2_item = {
            let conn = db.pool().get().unwrap();
            crate::repository::tasks::list_visible_by_cycle(&conn, &m2).unwrap()[0]
                .id
                .clone()
        };
        complete(&db, &m2_item);
        // m1's review was saved before the completion: the summary must show
        // the snapshot value, not the current 100%.
        save_cycle_review(
            &db,
            &SaveReviewArgs {
                cycle_id: m2.clone(),
                answers: vec![],
            },
            200,
        )
        .unwrap();

        let summary = review_summary(&db).unwrap();
        assert_eq!(summary.len(), 2);
        assert_eq!(summary[0].cycle_id, m1);
        assert_eq!(summary[0].completion_rate, Some(0.0));
        assert_eq!(summary[1].cycle_id, m2);
        assert_eq!(summary[1].completion_rate, Some(1.0));
        assert!(summary[0].snapshot_at <= summary[1].snapshot_at);
    }
}

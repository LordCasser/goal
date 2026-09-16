//! Prioritization engine (change `add-ai-planning-core`, tasks §7).
//!
//! A prioritization conclusion is five semantic buckets, not a score
//! (`openspec/specs/prioritization/spec.md`):
//!
//! * `big_wins` — 值得投入的大胜
//! * `bottlenecks` — 瓶颈（可为空）
//! * `non_negotiables` — 必须做但不算大胜
//! * `deprioritized` — 被放弃/推迟/委派
//! * `pending_review` — 尚未处理的任务 id
//!
//! Three invariants carry the spec:
//!
//! 1. **分类必须带理由** — every placed item carries a `reason` that comes from
//!    the user's own words. A model that cannot extract a reason must keep
//!    asking instead of moving the task; [`validate`] is the write-side
//!    backstop that rejects an empty reason.
//! 2. **增量更新不丢数据** — updates are merged per bucket ([`merge`]):
//!    buckets absent from the update survive untouched, only an explicit
//!    `Some(vec![])` clears one. Structural validation happens before any
//!    write is accepted.
//! 3. **排序与规划的分工** — sorting only re-shuffles tasks that already
//!    exist ([`candidates_for`] is a pure function over the cycle's tasks).
//!    There is deliberately no guidance/filler text in [`render_for_model`]:
//!    when no candidates exist the prompt layer owns the "plan first" nudge.
//!
//! Storage is one JSON column, `cycles.prioritization_breakdown` (design D6):
//! the document is always read and written whole, so splitting it into
//! per-bucket tables would only add join and partial-update hazards. The SQL
//! layer cannot constrain the inner structure, so Rust-side deserialization
//! ([`from_value`]) plus [`validate`] backstop every write.
//!
//! Repository note: [`load_for_cycle`] / [`store_for_cycle`] are thin wrappers
//! over that column kept beside the engine on purpose — the later tool/service
//! layer goes through them for all reads and writes. If the document ever
//! grows into real tables, these two functions move into `repository/` and
//! their callers do not change.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::domain::task::Task;
use crate::error::{from_rusqlite, AppError, AppResult};

/// IPC error codes raised by this module (see `docs/architecture.md`).
pub const CODE_UNKNOWN_TASK: &str = "unknown_task";
/// IPC error code: a bucket item without a reason in the user's words.
pub const CODE_MISSING_REASON: &str = "missing_reason";
/// IPC error code: the same task placed in more than one bucket.
pub const CODE_TASK_IN_MULTIPLE_BUCKETS: &str = "task_in_multiple_buckets";
/// IPC error code: a stored or submitted document is not a five-bucket breakdown.
pub const CODE_INVALID_BREAKDOWN: &str = "invalid_breakdown";

/// One task placed in one bucket, with the reason taken from the user's own
/// words (spec: 分类必须带理由 — 模型想不出理由就不应移入，写入侧兜底拒绝).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BucketItem {
    pub task_id: String,
    pub reason: String,
}

impl BucketItem {
    pub fn new(task_id: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            reason: reason.into(),
        }
    }
}

/// The five-bucket prioritization conclusion persisted as one JSON document in
/// `cycles.prioritization_breakdown` (NULL = never prioritized).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PrioritizationBreakdown {
    /// 值得投入的大胜.
    #[serde(default)]
    pub big_wins: Vec<BucketItem>,
    /// 瓶颈，可为空.
    #[serde(default)]
    pub bottlenecks: Vec<BucketItem>,
    /// 必须做但不算大胜.
    #[serde(default)]
    pub non_negotiables: Vec<BucketItem>,
    /// 被放弃/推迟/委派.
    #[serde(default)]
    pub deprioritized: Vec<BucketItem>,
    /// 尚未处理的任务 id，顺序跟随候选任务.
    #[serde(default)]
    pub pending_review: Vec<String>,
}

/// A per-bucket incremental update. `None` keeps the bucket as-is; `Some`
/// replaces the bucket wholesale — including `Some(vec![])`, the only way to
/// clear one (spec: 增量更新不丢数据).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PrioritizationBreakdownUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub big_wins: Option<Vec<BucketItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bottlenecks: Option<Vec<BucketItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub non_negotiables: Option<Vec<BucketItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deprioritized: Option<Vec<BucketItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_review: Option<Vec<String>>,
}

impl PrioritizationBreakdown {
    /// The four id+reason buckets in canonical order (used by validation and
    /// rendering; `pending_review` is handled separately because it holds
    /// bare ids).
    fn item_buckets(&self) -> [(&'static str, &Vec<BucketItem>); 4] {
        [
            ("big_wins", &self.big_wins),
            ("bottlenecks", &self.bottlenecks),
            ("non_negotiables", &self.non_negotiables),
            ("deprioritized", &self.deprioritized),
        ]
    }

    /// Ids currently placed in any of the four item buckets.
    pub fn bucketed_task_ids(&self) -> HashSet<&str> {
        self.item_buckets()
            .into_iter()
            .flat_map(|(_, items)| items.iter().map(|item| item.task_id.as_str()))
            .collect()
    }

    /// True when nothing has been classified and nothing is pending.
    pub fn is_empty(&self) -> bool {
        self.big_wins.is_empty()
            && self.bottlenecks.is_empty()
            && self.non_negotiables.is_empty()
            && self.deprioritized.is_empty()
            && self.pending_review.is_empty()
    }
}

/// Parses a stored/submitted JSON document into a breakdown. Garbage JSON is a
/// rejected write, never a panic: any deviation from the five-bucket structure
/// (wrong type, item without `reason`, non-string id) surfaces as
/// `Validation` with code `invalid_breakdown` (spec: 非法结构 — 拒绝写入).
pub fn from_value(value: serde_json::Value) -> AppResult<PrioritizationBreakdown> {
    serde_json::from_value(value).map_err(|e| {
        AppError::validation(
            CODE_INVALID_BREAKDOWN,
            format!("not a valid prioritization breakdown: {e}"),
        )
    })
}

/// Serializes the breakdown for the `cycles.prioritization_breakdown` column.
/// Infallible for this plain struct; the fallback only satisfies the
/// serializer's signature and would degrade to the NULL column.
pub fn to_value(bd: &PrioritizationBreakdown) -> serde_json::Value {
    serde_json::to_value(bd).unwrap_or(serde_json::Value::Null)
}

/// Validates a breakdown against the cycle's candidate task ids:
///
/// * every placed `task_id` (and every pending id) must be a known candidate —
///   code `unknown_task`;
/// * every reason must be non-empty after trimming — code `missing_reason`
///   (the model that cannot produce a reason should not move the task at all;
///   this is the write-side backstop);
/// * a task may occupy at most one of the four placement buckets — code
///   `task_in_multiple_buckets`.
///
/// `pending_review` is the "not classified yet" holding area, not a placement:
/// a task that is both bucketed and still listed as pending is tolerated here
/// (moving a task must not force resubmitting the pending list in the same
/// update). [`candidates_for`] recomputes pending from the buckets on the next
/// session start, which is what keeps the two views eventually consistent.
pub fn validate(bd: &PrioritizationBreakdown, valid_task_ids: &HashSet<String>) -> AppResult<()> {
    let mut seen: HashSet<&str> = HashSet::new();
    for (bucket, items) in bd.item_buckets() {
        for item in items {
            if !valid_task_ids.contains(&item.task_id) {
                return Err(AppError::validation(
                    CODE_UNKNOWN_TASK,
                    format!(
                        "task {} is not a candidate of this cycle (bucket {bucket})",
                        item.task_id
                    ),
                ));
            }
            if item.reason.trim().is_empty() {
                return Err(AppError::validation(
                    CODE_MISSING_REASON,
                    format!(
                        "moving task {} into {bucket} requires a reason in the user's own words",
                        item.task_id
                    ),
                ));
            }
            if !seen.insert(item.task_id.as_str()) {
                return Err(AppError::validation(
                    CODE_TASK_IN_MULTIPLE_BUCKETS,
                    format!("task {} may appear in at most one bucket", item.task_id),
                ));
            }
        }
    }
    for task_id in &bd.pending_review {
        if !valid_task_ids.contains(task_id) {
            return Err(AppError::validation(
                CODE_UNKNOWN_TASK,
                format!("pending task {task_id} is not a candidate of this cycle"),
            ));
        }
    }
    Ok(())
}

/// Merges an incremental update onto the persisted breakdown and validates the
/// result: buckets absent from the update survive, submitted buckets are
/// replaced wholesale (`Some(vec![])` is the explicit clear), then the merged
/// document must pass [`validate`] against the current candidate set —
/// otherwise the write is rejected and nothing is persisted. A task moved out
/// of `pending_review` into a bucket keeps the merge a pure substitution; the
/// caller either submits the new pending list in the same update or lets the
/// next [`candidates_for`] recomputation drop the bucketed id.
///
/// Takes the candidate set because validation is part of the merge contract;
/// callers that loaded candidates via [`candidates_for`] pass the same set.
pub fn merge(
    existing: &PrioritizationBreakdown,
    update: &PrioritizationBreakdownUpdate,
    valid_task_ids: &HashSet<String>,
) -> AppResult<PrioritizationBreakdown> {
    let mut merged = existing.clone();
    if let Some(items) = &update.big_wins {
        merged.big_wins = items.clone();
    }
    if let Some(items) = &update.bottlenecks {
        merged.bottlenecks = items.clone();
    }
    if let Some(items) = &update.non_negotiables {
        merged.non_negotiables = items.clone();
    }
    if let Some(items) = &update.deprioritized {
        merged.deprioritized = items.clone();
    }
    if let Some(pending) = &update.pending_review {
        merged.pending_review = pending.clone();
    }
    validate(&merged, valid_task_ids)?;
    Ok(merged)
}

/// Computes the initial data for a (re)started sorting session: tasks already
/// placed in a bucket stay in their bucket untouched (增量更新不丢数据), and
/// `pending_review` is recomputed as exactly the candidate tasks that no bucket
/// claims, in the order the caller passed them (position-sorted). Pure
/// function — no I/O, no guidance text; with no candidate tasks the result has
/// an empty `pending_review` and the prompt layer decides what to say.
pub fn candidates_for(
    cycle_tasks: &[Task],
    existing: &PrioritizationBreakdown,
) -> PrioritizationBreakdown {
    let mut next = existing.clone();
    let bucketed = next.bucketed_task_ids();
    next.pending_review = cycle_tasks
        .iter()
        .map(|task| task.id.clone())
        .filter(|id| !bucketed.contains(id.as_str()))
        .collect();
    next
}

/// Renders the breakdown as deterministic XML-shaped text for the model
/// (spec: 排序结果对模型可读). Buckets appear in canonical order, items in
/// stored order; empty buckets self-close. Items in `big_wins` and
/// `non_negotiables` carry `priority="high"` so 大胜 and 不可协商项 are marked
/// beyond their bucket name. All text goes through [`escape_xml`], so titles
/// containing `<` or `&` cannot break the model-side parse.
///
/// No onboarding/filler text is emitted when everything is empty — steering
/// copy is the prompt layer's job, not the renderer's.
pub fn render_for_model(
    bd: &PrioritizationBreakdown,
    titles: &HashMap<String, String>,
    cycle_key: &str,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "<prioritization cycle_key=\"{}\">\n",
        escape_xml(cycle_key)
    ));

    fn render_bucket(out: &mut String, name: &str, items: &[BucketItem], high_priority: bool) {
        if items.is_empty() {
            out.push_str(&format!("  <{name}/>\n"));
            return;
        }
        out.push_str(&format!("  <{name}>\n"));
        for item in items {
            let priority = if high_priority {
                " priority=\"high\""
            } else {
                ""
            };
            out.push_str(&format!(
                "    <item task_id=\"{}\"{priority}>{}</item>\n",
                escape_xml(&item.task_id),
                escape_xml(&item.reason)
            ));
        }
        out.push_str(&format!("  </{name}>\n"));
    }

    render_bucket(&mut out, "big_wins", &bd.big_wins, true);
    render_bucket(&mut out, "bottlenecks", &bd.bottlenecks, false);
    render_bucket(&mut out, "non_negotiables", &bd.non_negotiables, true);
    render_bucket(&mut out, "deprioritized", &bd.deprioritized, false);

    if bd.pending_review.is_empty() {
        out.push_str("  <pending_review/>\n");
    } else {
        out.push_str("  <pending_review>\n");
        for task_id in &bd.pending_review {
            match titles.get(task_id) {
                Some(title) => out.push_str(&format!(
                    "    <task id=\"{}\">{}</task>\n",
                    escape_xml(task_id),
                    escape_xml(title)
                )),
                // An unknown title renders as a bare id element: rendering
                // stays deterministic and never invents copy.
                None => out.push_str(&format!("    <task id=\"{}\"/>\n", escape_xml(task_id))),
            }
        }
        out.push_str("  </pending_review>\n");
    }

    out.push_str("</prioritization>\n");
    out
}

/// Escapes the five XML-significant characters (`& < > " '`) as entities.
/// Reasons quote the user and titles are free text, so both may contain
/// markup-lookalikes; escaping keeps the model-side parse intact.
fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Reads the persisted breakdown for one cycle. SQL `NULL` means "never
/// prioritized" and yields `Ok(None)`; an unknown cycle is `NotFound`; a
/// stored document that no longer parses is a storage-integrity problem and
/// surfaces as `AppError::Db` (the cycle must not silently lose its
/// conclusion). Structure-only: rule validation gates writes, not reads.
pub fn load_for_cycle(
    conn: &Connection,
    cycle_id: &str,
) -> AppResult<Option<PrioritizationBreakdown>> {
    let raw: Option<String> = match conn.query_row(
        "SELECT prioritization_breakdown FROM cycles WHERE id = ?1",
        [cycle_id],
        |row| row.get(0),
    ) {
        Ok(raw) => raw,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return Err(AppError::not_found("cycle", cycle_id));
        }
        Err(e) => return Err(from_rusqlite(e)),
    };
    match raw {
        None => Ok(None),
        Some(text) => serde_json::from_str(&text).map(Some).map_err(|e| {
            AppError::Db(format!(
                "cycles.prioritization_breakdown for cycle {cycle_id} is not a valid \
                     breakdown: {e}"
            ))
        }),
    }
}

/// Persists the breakdown as the cycle's JSON column. A thin wrapper by
/// design: rule validation is the caller's contract ([`merge`] already runs
/// it), this only serializes and writes. An unknown cycle is `NotFound`.
pub fn store_for_cycle(
    conn: &Connection,
    cycle_id: &str,
    bd: &PrioritizationBreakdown,
) -> AppResult<()> {
    let json = serde_json::to_string(bd)
        .map_err(|e| AppError::Internal(format!("serializing breakdown failed: {e}")))?;
    let affected = conn
        .execute(
            "UPDATE cycles SET prioritization_breakdown = ?1 WHERE id = ?2",
            params![json, cycle_id],
        )
        .map_err(from_rusqlite)?;
    if affected == 0 {
        return Err(AppError::not_found("cycle", cycle_id));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- fixtures -----------------------------------------------------------

    fn item(id: &str, reason: &str) -> BucketItem {
        BucketItem::new(id, reason)
    }

    fn candidate_ids(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    fn task(id: &str, position: i64) -> Task {
        Task {
            id: id.into(),
            cycle_id: "c1".into(),
            parent_id: None,
            title: format!("task {id}"),
            subtasks: vec![],
            position,
            completed: false,
            goal_breakdown: None,
            needs_refinement: None,
            needs_breakdown: None,
            root_color_key: None,
            copied_from_task_id: None,
            later_plan_type: None,
            proposal: None,
            created_at: 0,
        }
    }

    fn sample_breakdown() -> PrioritizationBreakdown {
        PrioritizationBreakdown {
            big_wins: vec![item("t1", "用户说这是最能改变现状的一件事")],
            bottlenecks: vec![],
            non_negotiables: vec![item("t2", "客户本周等着这份交付")],
            deprioritized: vec![item("t3", "先放放，等设计稿定了再做")],
            pending_review: vec!["t4".into()],
        }
    }

    fn code_of(err: &AppError) -> String {
        match err {
            AppError::Validation { code, .. } => code.clone(),
            other => panic!("expected Validation error, got {other:?}"),
        }
    }

    // -- 7.1 types & serialization -----------------------------------------

    #[test]
    fn serializes_with_snake_case_keys_and_round_trips() {
        let bd = sample_breakdown();
        let value = to_value(&bd);
        for key in [
            "big_wins",
            "bottlenecks",
            "non_negotiables",
            "deprioritized",
            "pending_review",
        ] {
            assert!(value.get(key).is_some(), "key {key} must be snake_case");
        }
        assert_eq!(from_value(value).expect("round trip"), bd);
    }

    #[test]
    fn is_empty_only_when_all_five_buckets_are_empty() {
        assert!(PrioritizationBreakdown::default().is_empty());
        assert!(!sample_breakdown().is_empty());
        let mut only_pending = PrioritizationBreakdown::default();
        only_pending.pending_review.push("t1".into());
        assert!(!only_pending.is_empty());
    }

    #[test]
    fn from_value_rejects_garbage_with_explicit_error() {
        for garbage in [
            serde_json::json!(["big_wins"]),
            serde_json::json!(null),
            serde_json::json!({"big_wins": "everything"}),
            serde_json::json!({"big_wins": [{"task_id": "t1"}]}), // reason missing
            serde_json::json!({"big_wins": [{"task_id": 7, "reason": "r"}]}),
            serde_json::json!({"pending_review": ["t1"], "big_wins": [{"task_id": "t1", "reason": 3}]}),
        ] {
            let err = from_value(garbage).expect_err("garbage must be rejected");
            assert_eq!(code_of(&err), CODE_INVALID_BREAKDOWN);
        }
    }

    // -- validation ----------------------------------------------------------

    #[test]
    fn validate_accepts_a_well_formed_breakdown() {
        let bd = sample_breakdown();
        let valid = candidate_ids(&["t1", "t2", "t3", "t4"]);
        validate(&bd, &valid).expect("valid breakdown accepted");
    }

    #[test]
    fn validate_rejects_unknown_task() {
        let mut bd = PrioritizationBreakdown::default();
        bd.big_wins.push(item("ghost", "理由来自用户"));
        let err = validate(&bd, &candidate_ids(&["t1"])).expect_err("unknown task");
        assert_eq!(code_of(&err), CODE_UNKNOWN_TASK);
    }

    #[test]
    fn validate_rejects_missing_or_blank_reason() {
        let valid = candidate_ids(&["t1"]);
        for reason in ["", "   "] {
            let mut bd = PrioritizationBreakdown::default();
            bd.deprioritized.push(item("t1", reason));
            let err = validate(&bd, &valid).expect_err("empty reason");
            assert_eq!(code_of(&err), CODE_MISSING_REASON);
        }
    }

    #[test]
    fn validate_rejects_task_in_two_buckets() {
        let mut bd = PrioritizationBreakdown::default();
        bd.big_wins.push(item("t1", "用户称之为大胜"));
        bd.non_negotiables.push(item("t1", "同时又是必须做的"));
        let err = validate(&bd, &candidate_ids(&["t1"])).expect_err("two buckets");
        assert_eq!(code_of(&err), CODE_TASK_IN_MULTIPLE_BUCKETS);
    }

    #[test]
    fn validate_tolerates_a_task_both_bucketed_and_still_pending() {
        // `pending_review` is the unclassified holding area, not a placement:
        // moving a task into a bucket without resubmitting the pending list in
        // the same update stays valid; the next `candidates_for` recomputation
        // drops the bucketed id from pending.
        let mut bd = PrioritizationBreakdown::default();
        bd.big_wins.push(item("t1", "用户称之为大胜"));
        bd.pending_review.push("t1".into());
        validate(&bd, &candidate_ids(&["t1"])).expect("bucket + pending tolerated");
    }

    #[test]
    fn validate_rejects_unknown_pending_id() {
        let mut bd = PrioritizationBreakdown::default();
        bd.pending_review.push("ghost".into());
        let err = validate(&bd, &candidate_ids(&["t1"])).expect_err("unknown pending id");
        assert_eq!(code_of(&err), CODE_UNKNOWN_TASK);
    }

    // -- 7.2 merge -----------------------------------------------------------

    #[test]
    fn merge_keeps_buckets_absent_from_the_update() {
        let existing = sample_breakdown();
        let candidates = candidate_ids(&["t1", "t2", "t3", "t4", "t5"]);
        let update = PrioritizationBreakdownUpdate {
            bottlenecks: Some(vec![item("t4", "卡在等接口联调上")]),
            ..Default::default()
        };
        let merged = merge(&existing, &update, &candidates).expect("merge ok");
        // Only bottlenecks changed; the spec scenario 只更新瓶颈.
        assert_eq!(merged.big_wins, existing.big_wins);
        assert_eq!(merged.non_negotiables, existing.non_negotiables);
        assert_eq!(merged.deprioritized, existing.deprioritized);
        assert_eq!(merged.pending_review, existing.pending_review);
        assert_eq!(merged.bottlenecks, vec![item("t4", "卡在等接口联调上")]);
    }

    #[test]
    fn merge_replaces_submitted_bucket_wholesale() {
        let existing = sample_breakdown();
        let candidates = candidate_ids(&["t1", "t2", "t3", "t4"]);
        let update = PrioritizationBreakdownUpdate {
            deprioritized: Some(vec![item("t4", "改主意了，这个推迟到下个月")]),
            ..Default::default()
        };
        let merged = merge(&existing, &update, &candidates).expect("merge ok");
        assert_eq!(
            merged.deprioritized,
            vec![item("t4", "改主意了，这个推迟到下个月")]
        );
    }

    #[test]
    fn merge_explicit_empty_vec_clears_the_bucket() {
        let existing = sample_breakdown();
        let candidates = candidate_ids(&["t1", "t2", "t3", "t4"]);
        let update = PrioritizationBreakdownUpdate {
            big_wins: Some(vec![]),
            ..Default::default()
        };
        let merged = merge(&existing, &update, &candidates).expect("merge ok");
        assert!(merged.big_wins.is_empty(), "Some(vec![]) clears the bucket");
        assert_eq!(merged.non_negotiables, existing.non_negotiables);
    }

    #[test]
    fn merge_rejects_an_invalid_result_and_yields_nothing() {
        let existing = sample_breakdown();
        let candidates = candidate_ids(&["t1", "t2", "t3", "t4"]);
        // A task may not sit in two placement buckets at once.
        let update = PrioritizationBreakdownUpdate {
            big_wins: Some(vec![item("t2", "用户说这才是重点")]),
            ..Default::default()
        };
        let err = merge(&existing, &update, &candidates).expect_err("double-booked");
        assert_eq!(code_of(&err), CODE_TASK_IN_MULTIPLE_BUCKETS);

        // An empty reason is rejected by the merge too.
        let update = PrioritizationBreakdownUpdate {
            big_wins: Some(vec![item("t4", "")]),
            pending_review: Some(vec![]),
            ..Default::default()
        };
        let err = merge(&existing, &update, &candidates).expect_err("no reason");
        assert_eq!(code_of(&err), CODE_MISSING_REASON);
    }

    // -- 7.3 candidates ------------------------------------------------------

    #[test]
    fn candidates_for_lists_unbucketed_tasks_in_input_order() {
        let existing = sample_breakdown(); // t1..t3 bucketed, t4 pending
        let tasks = vec![task("t5", 0), task("t4", 1), task("t1", 2), task("t6", 3)];
        let next = candidates_for(&tasks, &existing);
        // t4 was already pending and is not bucketed, so it stays; bucketed
        // tasks drop out; order follows the position-sorted input.
        assert_eq!(next.pending_review, vec!["t5", "t4", "t6"]);
        assert_eq!(next.big_wins, existing.big_wins, "buckets untouched");
        assert_eq!(next.deprioritized, existing.deprioritized);
    }

    #[test]
    fn candidates_for_drops_tasks_bucketed_by_a_merge() {
        // The usual session flow: init → move a pending task into a bucket →
        // the next init must not list it as pending anymore.
        let existing = sample_breakdown(); // t4 pending
        let candidates = candidate_ids(&["t1", "t2", "t3", "t4"]);
        let update = PrioritizationBreakdownUpdate {
            bottlenecks: Some(vec![item("t4", "卡在等接口联调上")]),
            ..Default::default()
        };
        let merged = merge(&existing, &update, &candidates).expect("merge ok");
        let tasks = vec![task("t1", 0), task("t2", 1), task("t3", 2), task("t4", 3)];
        let next = candidates_for(&tasks, &merged);
        assert!(
            next.pending_review.is_empty(),
            "every candidate is classified"
        );
        assert_eq!(next.bottlenecks, vec![item("t4", "卡在等接口联调上")]);
    }

    #[test]
    fn candidates_for_with_no_tasks_yields_empty_pending() {
        let next = candidates_for(&[], &PrioritizationBreakdown::default());
        assert!(next.is_empty());
        assert!(next.pending_review.is_empty());
        // The renderer stays quiet about it — no guidance copy is invented
        // here; the prompt layer owns the "plan first" nudge
        // (spec: 缺少候选任务 → 引导用户先做规划).
        assert!(render_for_model(&next, &HashMap::new(), "d1").contains("<pending_review/>"));
    }

    // -- 7.5 rendering -------------------------------------------------------

    #[test]
    fn render_marks_big_wins_and_non_negotiables_only() {
        let bd = sample_breakdown();
        let rendered = render_for_model(&bd, &HashMap::new(), "wk-1");
        assert!(rendered.contains("<item task_id=\"t1\" priority=\"high\">"));
        assert!(rendered.contains("<item task_id=\"t2\" priority=\"high\">"));
        assert!(rendered.contains("<item task_id=\"t3\">"));
        assert!(!rendered.contains("task_id=\"t3\" priority"));
    }

    #[test]
    fn render_self_closes_empty_buckets_and_keeps_canonical_order() {
        let rendered = render_for_model(&sample_breakdown(), &HashMap::new(), "wk-1");
        assert!(rendered.contains("<bottlenecks/>"));
        assert!(rendered.starts_with("<prioritization cycle_key=\"wk-1\">\n"));
        let big_wins = rendered.find("<big_wins>").expect("big_wins first");
        let bottlenecks = rendered.find("<bottlenecks/>").expect("bottlenecks second");
        let non_negotiables = rendered.find("<non_negotiables>").expect("third");
        let deprioritized = rendered.find("<deprioritized>").expect("fourth");
        let pending = rendered.find("<pending_review>").expect("fifth");
        assert!(big_wins < bottlenecks && bottlenecks < non_negotiables);
        assert!(non_negotiables < deprioritized && deprioritized < pending);
        assert!(rendered.trim_end().ends_with("</prioritization>"));
    }

    #[test]
    fn render_escapes_markup_in_titles_and_reasons() {
        let mut bd = PrioritizationBreakdown::default();
        bd.pending_review.push("t9".into());
        bd.bottlenecks
            .push(item("t8", "用户说 <重要> 且 \"急\" & '紧'"));
        let mut titles = HashMap::new();
        titles.insert("t9".to_string(), "修 <bug> & \"回归\"".to_string());
        let rendered = render_for_model(&bd, &titles, "d&1");

        assert!(rendered.contains("cycle_key=\"d&amp;1\""));
        assert!(rendered.contains("&lt;bug&gt; &amp; &quot;回归&quot;"));
        assert!(rendered.contains("&apos;紧&apos;"));
        // Raw markup from titles/reasons must not leak into the structure.
        assert!(!rendered.contains("<bug>"));
        assert!(!rendered.contains("<重要>"));
        // Parsing the structure back still finds exactly one item per bucket.
        assert_eq!(rendered.matches("<item ").count(), 1);
        assert_eq!(rendered.matches("<task ").count(), 1);
    }

    #[test]
    fn render_pending_without_title_stays_a_bare_element() {
        let mut bd = PrioritizationBreakdown::default();
        bd.pending_review.push("t9".into());
        let rendered = render_for_model(&bd, &HashMap::new(), "d1");
        assert!(rendered.contains("<task id=\"t9\"/>"));
    }

    #[test]
    fn render_is_deterministic() {
        let bd = sample_breakdown();
        let mut titles = HashMap::new();
        titles.insert("t4".to_string(), "随手记的一个想法".to_string());
        let first = render_for_model(&bd, &titles, "wk-1");
        let second = render_for_model(&bd, &titles, "wk-1");
        assert_eq!(first, second);
        assert!(first.contains("<task id=\"t4\">随手记的一个想法</task>"));
    }

    // -- persistence helpers --------------------------------------------------

    /// Opens a fully migrated throwaway database, mirroring
    /// `tests/common/mod.rs::TestDb` (which integration tests cannot share
    /// with this unit test module).
    fn open_test_db() -> (tempfile::TempDir, crate::db::Db) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = crate::db::open_at(&dir.path().join("planner.db")).expect("open db");
        (dir, db)
    }

    fn insert_cycle(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO cycles (id, title, type, position) VALUES (?1, 'Day', 'day', 0)",
            [id],
        )
        .expect("insert cycle");
    }

    #[test]
    fn load_returns_none_for_a_null_column_and_not_found_for_a_missing_cycle() {
        let (_dir, db) = open_test_db();
        let conn = db.pool().get().expect("conn");
        assert!(matches!(
            load_for_cycle(&conn, "nope"),
            Err(AppError::NotFound { entity, .. }) if entity == "cycle"
        ));

        insert_cycle(&conn, "c1");
        assert_eq!(load_for_cycle(&conn, "c1").expect("null column"), None);
    }

    #[test]
    fn store_then_load_round_trips_the_whole_breakdown() {
        let (_dir, db) = open_test_db();
        let conn = db.pool().get().expect("conn");
        insert_cycle(&conn, "c1");

        let bd = sample_breakdown();
        store_for_cycle(&conn, "c1", &bd).expect("store");
        assert_eq!(
            load_for_cycle(&conn, "c1").expect("load"),
            Some(bd),
            "the whole five-bucket document survives the round trip"
        );

        // Overwrite with an explicitly cleared breakdown.
        store_for_cycle(&conn, "c1", &PrioritizationBreakdown::default()).expect("store");
        assert_eq!(
            load_for_cycle(&conn, "c1").expect("load"),
            Some(PrioritizationBreakdown::default())
        );
    }

    #[test]
    fn store_reports_a_missing_cycle_as_not_found() {
        let (_dir, db) = open_test_db();
        let conn = db.pool().get().expect("conn");
        assert!(matches!(
            store_for_cycle(&conn, "ghost", &PrioritizationBreakdown::default()),
            Err(AppError::NotFound { entity, .. }) if entity == "cycle"
        ));
    }

    #[test]
    fn load_flags_a_corrupted_column_as_a_db_error() {
        let (_dir, db) = open_test_db();
        let conn = db.pool().get().expect("conn");
        insert_cycle(&conn, "c1");
        conn.execute(
            "UPDATE cycles SET prioritization_breakdown = '{oops' WHERE id = 'c1'",
            [],
        )
        .expect("corrupt the column");
        assert!(matches!(load_for_cycle(&conn, "c1"), Err(AppError::Db(_))));
    }
}

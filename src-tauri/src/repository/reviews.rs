//! SQL for the `cycle_reviews` aggregate (change: add-review-retrospective §1
//! / §3). One review row per cycle (`cycle_id` UNIQUE); dispositions hang off
//! the review with a per-task uniqueness so a decision is recorded once.
//! Cross-aggregate orchestration (fact computation, carry copies, moves)
//! belongs to `service::reviews`.

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{from_rusqlite, AppResult};

/// One stored review row. `facts_json` / `answers_json` are the frozen
/// snapshot written at save time; reads never recompute them.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewRow {
    pub id: String,
    pub cycle_id: String,
    pub kind: String,
    pub is_final: bool,
    pub facts_json: String,
    pub answers_json: String,
    pub snapshot_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

fn row_to_review(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReviewRow> {
    Ok(ReviewRow {
        id: row.get("id")?,
        cycle_id: row.get("cycle_id")?,
        kind: row.get("kind")?,
        is_final: row.get::<_, i64>("is_final")? != 0,
        facts_json: row.get("facts_json")?,
        answers_json: row.get("answers_json")?,
        snapshot_at: row.get("snapshot_at")?,
        created_at: row.get("created_at")?,
        updated_at: row.get("updated_at")?,
    })
}

const REVIEW_COLUMNS: &str =
    "id, cycle_id, kind, is_final, facts_json, answers_json, snapshot_at, created_at, updated_at";

/// The saved review of one cycle, or `None` when it has not been reviewed.
pub fn get_by_cycle(conn: &Connection, cycle_id: &str) -> AppResult<Option<ReviewRow>> {
    conn.query_row(
        &format!("SELECT {REVIEW_COLUMNS} FROM cycle_reviews WHERE cycle_id = ?1"),
        params![cycle_id],
        row_to_review,
    )
    .optional()
    .map_err(from_rusqlite)
}

/// The most recently snapshotted review whose cycle is not `exclude_cycle_id`
/// — the "最近一份复盘结论" injected into a review/planning context. Ordering
/// ties break on created_at then id so the choice is deterministic.
pub fn latest_other(conn: &Connection, exclude_cycle_id: &str) -> AppResult<Option<ReviewRow>> {
    conn.query_row(
        &format!(
            "SELECT {REVIEW_COLUMNS} FROM cycle_reviews \
             WHERE cycle_id != ?1 \
             ORDER BY snapshot_at DESC, created_at DESC, id DESC LIMIT 1"
        ),
        params![exclude_cycle_id],
        row_to_review,
    )
    .optional()
    .map_err(from_rusqlite)
}

/// Snapshot for the cross-cycle summary: every saved review joined with its
/// cycle's identity, oldest first (spec: 按周期时间顺序展示).
pub struct SummaryRow {
    pub cycle_id: String,
    pub cycle_title: String,
    pub cycle_type: String,
    pub kind: String,
    pub is_final: bool,
    pub facts_json: String,
    pub snapshot_at: i64,
}

pub fn list_summary(conn: &Connection) -> AppResult<Vec<SummaryRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT r.cycle_id, c.title, c.type, r.kind, r.is_final, r.facts_json, r.snapshot_at \
             FROM cycle_reviews r JOIN cycles c ON c.id = r.cycle_id \
             ORDER BY r.snapshot_at ASC, r.created_at ASC, r.id ASC",
        )
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map([], |row| {
            Ok(SummaryRow {
                cycle_id: row.get(0)?,
                cycle_title: row.get(1)?,
                cycle_type: row.get(2)?,
                kind: row.get(3)?,
                is_final: row.get::<_, i64>(4)? != 0,
                facts_json: row.get(5)?,
                snapshot_at: row.get(6)?,
            })
        })
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(rows)
}

/// Creates the cycle's review row. The caller (service) has already checked
/// that no row exists, so a UNIQUE violation surfaces as a hard error.
pub struct NewReview {
    pub id: String,
    pub cycle_id: String,
    pub kind: String,
    pub is_final: bool,
    pub facts_json: String,
    pub answers_json: String,
    pub snapshot_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

pub fn insert(conn: &Connection, new: &NewReview) -> AppResult<()> {
    conn.execute(
        "INSERT INTO cycle_reviews (id, cycle_id, kind, is_final, facts_json, answers_json, \
         snapshot_at, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            new.id,
            new.cycle_id,
            new.kind,
            new.is_final as i64,
            new.facts_json,
            new.answers_json,
            new.snapshot_at,
            new.created_at,
            new.updated_at,
        ],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

/// Overwrite update for a re-review of the same cycle. `created_at` and the
/// row id stay stable so the dispositions rows (FK on review id) survive a
/// re-save instead of cascading away.
#[allow(clippy::too_many_arguments)]
pub fn update_by_cycle(
    conn: &Connection,
    cycle_id: &str,
    kind: &str,
    is_final: bool,
    facts_json: &str,
    answers_json: &str,
    snapshot_at: i64,
    updated_at: i64,
) -> AppResult<()> {
    conn.execute(
        "UPDATE cycle_reviews SET kind = ?2, is_final = ?3, facts_json = ?4, \
         answers_json = ?5, snapshot_at = ?6, updated_at = ?7 WHERE cycle_id = ?1",
        params![
            cycle_id,
            kind,
            is_final as i64,
            facts_json,
            answers_json,
            snapshot_at,
            updated_at
        ],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Dispositions
// ---------------------------------------------------------------------------

/// One recorded outcome for an unfinished item: `carry` | `later` | `drop`.
pub fn get_disposition(
    conn: &Connection,
    review_id: &str,
    task_id: &str,
) -> AppResult<Option<String>> {
    conn.query_row(
        "SELECT disposition FROM cycle_review_dispositions \
         WHERE review_id = ?1 AND task_id = ?2",
        params![review_id, task_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(from_rusqlite)
}

/// Records (or replaces) the decision for one item. Replacing keeps the
/// `(review_id, task_id)` uniqueness; the service layer owns any side effects
/// of a changed decision.
pub fn set_disposition(
    conn: &Connection,
    review_id: &str,
    task_id: &str,
    disposition: &str,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO cycle_review_dispositions (review_id, task_id, disposition) \
         VALUES (?1, ?2, ?3) \
         ON CONFLICT (review_id, task_id) DO UPDATE SET disposition = excluded.disposition",
        params![review_id, task_id, disposition],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

pub fn list_dispositions(conn: &Connection, review_id: &str) -> AppResult<Vec<(String, String)>> {
    let mut stmt = conn
        .prepare(
            "SELECT task_id, disposition FROM cycle_review_dispositions \
             WHERE review_id = ?1 ORDER BY task_id ASC",
        )
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map(params![review_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(rows)
}

/// Visible tasks of any cycle whose cross-level parent link points at
/// `task_id` — the "下层项" a review aggregates by (add-review-retrospective
/// §2.3). Row mapping stays in `repository::tasks`; this returns ids only.
pub fn linked_lower_task_ids(conn: &Connection, task_id: &str) -> AppResult<Vec<String>> {
    let mut stmt = conn
        .prepare(
            "SELECT id FROM tasks \
             WHERE parent_id = ?1 AND proposal IS NULL \
               AND cycle_id != (SELECT cycle_id FROM tasks WHERE id = ?1) \
             ORDER BY position ASC, created_at ASC, id ASC",
        )
        .map_err(from_rusqlite)?;
    let ids = stmt
        .query_map(params![task_id], |row| row.get::<_, String>(0))
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(ids)
}

/// Titles for the Markdown export's disposition lines; deleted rows fall back
/// to the empty map entry handled by the caller.
pub fn task_title(conn: &Connection, task_id: &str) -> AppResult<Option<String>> {
    conn.query_row(
        "SELECT title FROM tasks WHERE id = ?1",
        params![task_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(from_rusqlite)
}

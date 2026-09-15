//! Cycle review commands (change: add-review-retrospective §backend).
//!
//! Reviews are snapshot reads/writes on `cycle_reviews`: they change no cycle
//! or task data, so — unlike the workspace commands — there is **no event to
//! emit**. The Mutation/emit pattern only fits [`apply_review_disposition`],
//! whose carry/move side effects touch real task columns; saving a review
//! invalidates nothing beyond the panel itself, which refetches its own
//! queries (the entry-point hook is re-run by the same invalidation on the
//! frontend side).

use tauri::State;

use crate::db::Db;
use crate::error::AppResult;
use crate::service::reviews::{self, CycleReviewFacts, ReviewSummaryPoint, SaveReviewArgs};

/// The cycle's **current** facts, computed live. The panel shows these before
/// the first save (spec: 提前复盘 → 事实部分按「截至目前」计算); once a
/// review exists, the panel shows the frozen snapshot instead.
#[tauri::command]
pub fn get_cycle_facts(db: State<'_, Db>, cycle_id: String) -> AppResult<CycleReviewFacts> {
    let conn = db.pool().get()?;
    reviews::compute_facts(&conn, &cycle_id)
}

/// The saved review of one cycle (frozen snapshot + recorded dispositions),
/// or `None` when the cycle has not been reviewed.
#[tauri::command]
pub fn get_cycle_review(
    db: State<'_, Db>,
    cycle_id: String,
) -> AppResult<Option<reviews::CycleReviewView>> {
    reviews::get_cycle_review(&db, &cycle_id)
}

/// Creates or overwrites the cycle's review. Partial saves are legal and can
/// be continued later; the facts snapshot freezes at each save.
#[tauri::command]
pub fn save_cycle_review(
    db: State<'_, Db>,
    args: SaveReviewArgs,
) -> AppResult<reviews::CycleReviewView> {
    reviews::save_cycle_review(&db, &args, crate::service::now_ms())
}

/// Records one unfinished item's outcome (`carry` | `later` | `drop`) and
/// performs it. Emits the usual invalidation notices because carry copies and
/// later-moves change committed task data.
#[tauri::command]
pub fn apply_review_disposition(
    app: tauri::AppHandle<tauri::Wry>,
    db: State<'_, Db>,
    cycle_id: String,
    task_id: String,
    disposition: String,
) -> AppResult<()> {
    let mutation = reviews::apply_review_disposition(
        &db,
        &cycle_id,
        &task_id,
        &disposition,
        crate::service::now_ms(),
    )?;
    super::emit_mutation(&app, &mutation);
    Ok(())
}

/// Cross-cycle trend points (completion rate + focused time per saved
/// snapshot), oldest first. Single-review histories render one point.
#[tauri::command]
pub fn get_review_summary(db: State<'_, Db>) -> AppResult<Vec<ReviewSummaryPoint>> {
    reviews::review_summary(&db)
}

/// The saved review rendered as Markdown. Empty sections produce no headings;
/// the frontend places the string (clipboard today, a system save dialog
/// later), so the backend returns text rather than writing files.
#[tauri::command]
pub fn export_cycle_review_markdown(db: State<'_, Db>, cycle_id: String) -> AppResult<String> {
    reviews::export_cycle_review_markdown(&db, &cycle_id)
}

/// Writes the exported Markdown to a user-chosen path (tasks §7.2). The save
/// dialog picks the path on the frontend; the write itself belongs here so
/// no JS-side filesystem permission is needed.
#[tauri::command]
pub fn save_cycle_review_markdown(
    db: State<'_, Db>,
    cycle_id: String,
    target_path: String,
) -> AppResult<()> {
    if target_path.trim().is_empty() {
        return Err(crate::error::AppError::validation(
            "invalid_target_path",
            "a save path is required",
        ));
    }
    let markdown = export_cycle_review_markdown(db, cycle_id)?;
    std::fs::write(&target_path, markdown)
        .map_err(|e| crate::error::AppError::Internal(format!("cannot write export: {e}")))
}

//! Temporary-file restart baseline for the local persistence contract.
//!
//! This deliberately opens a database below a `tempdir`; it must never touch
//! the user's application database. The fixture keeps one committed task and
//! one Coach conversation with a pending task preview across a close/reopen,
//! then verifies that rejecting the preview restores the exact original row.

mod common;

use common::{add_task, create_long_term, NOW, TODAY};
use planner_lib::domain::proposal::ProposalKind;
use planner_lib::repository::{agent, proposals as proposals_repo, tasks};
use planner_lib::service::proposals::{self, TaskInput};

#[test]
fn temporary_database_reopens_with_committed_and_coach_preview_state() {
    let dir = tempfile::tempdir().expect("temporary database directory");
    let path = dir.path().join("persistence-baseline.db");

    let db = planner_lib::db::open_at(&path).expect("open temporary database");
    let month = create_long_term(&db, TODAY, 1);
    let committed = add_task(&db, &month.id, "Saved commitment", NOW);
    let preview_target = add_task(&db, &month.id, "Original Coach title", NOW + 1);

    // Coach state is stored independently from the task preview, but both
    // belong to the same cycle and must survive a process restart.
    let conversation = {
        let conn = db.pool().get().expect("database connection");
        agent::get_or_create_conversation(&conn, &month.id, NOW).expect("Coach conversation")
    };
    let preview_input = TaskInput {
        title: "Coach proposed title".into(),
        subtasks: preview_target.subtasks.clone(),
        completed: preview_target.completed,
        goal_breakdown: preview_target.goal_breakdown.clone(),
        needs_refinement: preview_target.needs_refinement,
        needs_breakdown: preview_target.needs_breakdown,
        root_color_key: preview_target.root_color_key.clone(),
        parent_id: preview_target.parent_id.clone(),
    };
    let staged = proposals::apply_update_preview(&db, &preview_target.id, &preview_input)
        .expect("stage Coach preview")
        .value;
    assert_eq!(staged.proposal, Some(ProposalKind::Upsert));

    // Closing the Db releases its pool before the same file is opened again.
    drop(db);
    let reopened = planner_lib::db::open_at(&path).expect("reopen temporary database");
    let conn = reopened.pool().get().expect("reopened database connection");

    let reopened_conversation = agent::conversation_for_cycle(&conn, &month.id)
        .expect("read Coach conversation")
        .expect("Coach conversation persisted");
    assert_eq!(reopened_conversation.id, conversation.id);

    let committed_after_restart = tasks::get(&conn, &committed.id)
        .expect("read committed task")
        .expect("committed task persisted");
    assert_eq!(committed_after_restart.title, "Saved commitment");
    assert_eq!(committed_after_restart.proposal, None);

    let preview_after_restart = tasks::get(&conn, &preview_target.id)
        .expect("read preview task")
        .expect("preview task persisted");
    assert_eq!(preview_after_restart.title, "Coach proposed title");
    assert_eq!(preview_after_restart.proposal, Some(ProposalKind::Upsert));
    assert!(tasks::list_visible_by_cycle(&conn, &month.id)
        .expect("list committed tasks")
        .iter()
        .any(|task| task.id == committed.id));
    assert!(!tasks::list_visible_by_cycle(&conn, &month.id)
        .expect("list committed tasks")
        .iter()
        .any(|task| task.id == preview_target.id));

    let pending = proposals::get_preview_summary(&reopened, &month.id)
        .expect("read pending Coach preview");
    assert_eq!(pending.count, 1);
    assert_eq!(pending.tasks[0].id, preview_target.id);
    assert_eq!(pending.tasks[0].proposal, Some(ProposalKind::Upsert));
    assert_eq!(
        pending.originals[&preview_target.id].title.as_deref(),
        Some("Original Coach title")
    );
    assert!(proposals_repo::get_snapshot(&conn, &preview_target.id)
        .expect("read preview snapshot")
        .is_some());

    // Rejection uses the persisted snapshot, restoring the committed title
    // and clearing both pending representations.
    proposals::undo_task_preview(&reopened, &preview_target.id)
        .expect("reject Coach preview");
    let restored = tasks::get(&conn, &preview_target.id)
        .expect("read restored task")
        .expect("restored task remains");
    assert_eq!(restored.title, "Original Coach title");
    assert_eq!(restored.proposal, None);
    assert!(proposals_repo::get_snapshot(&conn, &preview_target.id)
        .expect("read cleared preview snapshot")
        .is_none());
    assert_eq!(
        proposals::get_preview_summary(&reopened, &month.id)
            .expect("read cleared Coach preview")
            .count,
        0
    );
}

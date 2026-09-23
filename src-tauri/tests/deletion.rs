//! Recursive deletion previews, confirmation tokens and cross-cycle cleanup.

mod common;

use common::{add_task, create_day, create_long_term, create_week, TestDb, NOW, TODAY};
use planner_lib::error::AppError;
use planner_lib::repository::{
    cycles as cycle_repo, reminders as reminder_repo, tasks as task_repo,
};
use planner_lib::service::reminders::SetReminderArgs;
use planner_lib::service::{cycles, reminders, tasks};

fn error_code(error: &AppError) -> &str {
    match error {
        AppError::Validation { code, .. } | AppError::Conflict { code, .. } => code,
        AppError::NotFound { .. } => "not_found",
        AppError::Db(_) => "db_error",
        AppError::Internal(_) => "internal",
    }
}

fn reminder(db: &planner_lib::db::Db, kind: &str, target_id: &str, offset: i64) {
    reminders::set_reminder(
        db,
        &SetReminderArgs {
            target_kind: kind.into(),
            target_id: target_id.into(),
            fire_at: NOW + offset,
            quiet_ok: false,
        },
        NOW,
    )
    .unwrap();
}

#[test]
fn task_preview_counts_cross_cycle_descendants_and_rejects_stale_tokens() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let root = add_task(&db.db, &month.id, "Root", NOW);
    let child = add_task(&db.db, &week.id, "Child", NOW + 1);
    tasks::set_task_parent_link(&db.db, &child.id, Some(&root.id)).unwrap();
    reminder(&db.db, "task", &root.id, 1_000);
    reminder(&db.db, "task", &child.id, 2_000);

    let preview = tasks::get_task_deletion_preview(&db.db, &root.id).unwrap();
    assert_eq!(preview.descendant_tasks, 1);

    let mut patch = tasks::TaskPatch::default();
    patch.title = Some("Changed after preview".into());
    tasks::patch_task(&db.db, &child.id, &patch).unwrap();
    let stale = tasks::delete_task_confirmed(&db.db, &root.id, Some(&preview.confirmation_token))
        .unwrap_err();
    assert_eq!(error_code(&stale), "deletion_impact_changed");

    let fresh = tasks::get_task_deletion_preview(&db.db, &root.id).unwrap();
    let mutation =
        tasks::delete_task_confirmed(&db.db, &root.id, Some(&fresh.confirmation_token)).unwrap();
    assert!(mutation.tasks.ids().contains(&month.id));
    assert!(mutation.tasks.ids().contains(&week.id));
    assert!(task_repo::get(&db.conn(), &root.id).unwrap().is_none());
    assert!(task_repo::get(&db.conn(), &child.id).unwrap().is_none());
    assert!(
        reminders::list_reminders(&db.db, None, reminder_repo::StatusFilter::All)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn cycle_preview_counts_focus_blocks_and_cascades_cross_cycle_task_children() {
    let db = TestDb::open();
    let deleted_month = create_long_term(&db.db, TODAY, 1);
    let deleted_week = create_week(&db.db, &deleted_month.id, TODAY);
    let deleted_day = create_day(&db.db, &deleted_week.id, TODAY, NOW);
    let session = cycles::add_session(
        &db.db,
        &cycles::AddSessionArgs {
            task_id: None,
            day_cycle_id: deleted_day.id.clone(),
            title: "Focus".into(),
            duration_ms: Some(900_000),
            position: None,
        },
        NOW,
    )
    .unwrap()
    .value;
    let root = add_task(&db.db, &deleted_month.id, "Root", NOW);

    let surviving_month = create_long_term(&db.db, "2026-10-01", 1);
    let surviving_week = create_week(&db.db, &surviving_month.id, "2026-10-01");
    let cross_cycle_child = add_task(&db.db, &surviving_week.id, "Child", NOW + 1);
    tasks::set_task_parent_link(&db.db, &cross_cycle_child.id, Some(&root.id)).unwrap();
    reminder(&db.db, "session", &session.id, 1_000);
    reminder(&db.db, "task", &root.id, 2_000);
    reminder(&db.db, "task", &cross_cycle_child.id, 3_000);

    let preview = cycles::get_cycle_deletion_preview(&db.db, &deleted_month.id).unwrap();
    assert_eq!(preview.total_focus_blocks, 1);
    assert_eq!(preview.started_sessions, 0);
    assert_eq!(preview.tasks, 2);

    let mutation = cycles::delete_cycle_confirmed(
        &db.db,
        &deleted_month.id,
        Some(&preview.confirmation_token),
    )
    .unwrap();
    assert!(mutation.cycles.ids().contains(&deleted_month.id));
    assert!(mutation.cycles.ids().contains(&deleted_week.id));
    assert!(mutation.tasks.ids().contains(&deleted_month.id));
    assert!(mutation.tasks.ids().contains(&surviving_week.id));
    assert!(cycle_repo::get(&db.conn(), &surviving_week.id)
        .unwrap()
        .is_some());
    assert!(task_repo::get(&db.conn(), &root.id).unwrap().is_none());
    assert!(task_repo::get(&db.conn(), &cross_cycle_child.id)
        .unwrap()
        .is_none());
    assert!(
        reminders::list_reminders(&db.db, None, reminder_repo::StatusFilter::All)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn cycle_preview_excludes_blank_input_rows_but_counts_blank_structural_parents() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let blank_parent = add_task(&db.db, &month.id, "", NOW);
    let child = add_task(&db.db, &month.id, "Meaningful child", NOW + 1);
    tasks::set_task_parent_link(&db.db, &child.id, Some(&blank_parent.id)).unwrap();
    add_task(&db.db, &month.id, "   ", NOW + 2);

    let preview = cycles::get_cycle_deletion_preview(&db.db, &month.id).unwrap();
    assert_eq!(preview.tasks, 2);
}

#[test]
fn deleting_a_task_preserves_an_independent_focus_block() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, NOW);
    let session = cycles::add_session(
        &db.db,
        &cycles::AddSessionArgs {
            task_id: None,
            day_cycle_id: day.id.clone(),
            title: "Independent focus".into(),
            duration_ms: Some(900_000),
            position: None,
        },
        NOW,
    )
    .unwrap()
    .value;
    let task = add_task(&db.db, &day.id, "Only task", NOW);

    tasks::delete_task(&db.db, &task.id).unwrap();

    assert!(cycle_repo::get(&db.conn(), &session.id).unwrap().is_some());
}

#[test]
fn gui_cycle_delete_requires_confirmation_even_without_children() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let preview = cycles::get_cycle_deletion_preview(&db.db, &month.id).unwrap();
    let err = cycles::delete_cycle_confirmed(&db.db, &month.id, None).unwrap_err();
    assert_eq!(error_code(&err), "deletion_confirmation_required");
    assert!(cycle_repo::get(&db.conn(), &month.id).unwrap().is_some());
    assert!(!preview.confirmation_token.is_empty());
}

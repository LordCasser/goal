//! Focus-block deletion keeps timer records, reminders, and parent aggregates
//! consistent without weakening planning-container deletion guards.

mod common;

use common::{add_task, create_day, create_long_term, create_week, TestDb, NOW, TODAY};
use planner_lib::error::AppError;
use planner_lib::repository::{
    cycles as cycle_repo,
    reminders::{self, TargetKind},
};
use planner_lib::service::cycles::{self, AddSessionArgs};
use planner_lib::service::proposals::{self, TaskInput};

fn err_code(err: &AppError) -> String {
    match err {
        AppError::Validation { code, .. } | AppError::Conflict { code, .. } => code.clone(),
        AppError::NotFound { .. } => "not_found".into(),
        other => format!("{other:?}"),
    }
}

#[test]
fn finished_focus_block_can_be_deleted_and_reverses_its_aggregate() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, NOW);
    let session = cycles::add_session(
        &db.db,
        &AddSessionArgs {
            day_cycle_id: day.id.clone(),
            title: "historical focus".into(),
            duration_ms: Some(3_600_000),
            position: None,
        },
        NOW,
    )
    .unwrap()
    .value;
    let other_session = cycles::add_session(
        &db.db,
        &AddSessionArgs {
            day_cycle_id: day.id.clone(),
            title: "another historical focus".into(),
            duration_ms: Some(3_600_000),
            position: None,
        },
        NOW + 1,
    )
    .unwrap()
    .value;
    let started_at = NOW + 10;
    let elapsed = 25 * 60 * 1000;
    cycles::start_cycle(&db.db, &session.id, started_at).unwrap();
    let finished = cycles::finish_cycle(&db.db, &session.id, started_at + elapsed)
        .unwrap()
        .value;
    let other_started_at = NOW + 100;
    let other_elapsed = 11 * 60 * 1000;
    cycles::start_cycle(&db.db, &other_session.id, other_started_at).unwrap();
    let other_finished =
        cycles::finish_cycle(&db.db, &other_session.id, other_started_at + other_elapsed)
            .unwrap()
            .value;

    // A session remains deletable after its containing day has ended.
    cycles::start_cycle(&db.db, &day.id, other_started_at + other_elapsed + 1).unwrap();
    cycles::finish_cycle(&db.db, &day.id, other_started_at + other_elapsed + 2).unwrap();

    assert_eq!(finished.focused_time, elapsed);
    assert_eq!(other_finished.focused_time, other_elapsed);
    let preview = cycles::get_cycle_deletion_preview(&db.db, &session.id).unwrap();
    assert_eq!(preview.guard_code, None);
    assert!(reminders::find_for_target(
        &db.conn(),
        TargetKind::Session,
        &session.id,
        started_at + 3_600_000,
    )
    .unwrap()
    .is_some());

    // Deletion removes the pending due reminder together with the session.
    let deletion = cycles::delete_cycle(&db.db, &session.id).unwrap();
    let conn = db.conn();
    assert!(cycle_repo::get(&conn, &session.id).unwrap().is_none());
    for id in [&session.id, &day.id, &week.id, &month.id] {
        assert!(deletion.cycles.ids().contains(id), "mutation touches {id}");
    }
    assert!(reminders::find_for_target(
        &conn,
        TargetKind::Session,
        &session.id,
        started_at + 3_600_000,
    )
    .unwrap()
    .is_none());
    assert!(reminders::find_for_target(
        &conn,
        TargetKind::Session,
        &other_session.id,
        other_started_at + 3_600_000,
    )
    .unwrap()
    .is_some());
    assert_eq!(
        cycle_repo::require(&conn, &day.id).unwrap().focused_time,
        other_elapsed
    );
    assert_eq!(
        cycle_repo::require(&conn, &week.id).unwrap().focused_time,
        other_elapsed
    );
    assert_eq!(
        cycle_repo::require(&conn, &month.id).unwrap().focused_time,
        other_elapsed
    );
}

#[test]
fn running_focus_block_can_be_deleted_without_accruing_time() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, NOW);
    let session = cycles::add_session(
        &db.db,
        &AddSessionArgs {
            day_cycle_id: day.id.clone(),
            title: "running focus".into(),
            duration_ms: Some(900_000),
            position: None,
        },
        NOW,
    )
    .unwrap()
    .value;
    let started_at = NOW + 20;
    cycles::start_cycle(&db.db, &session.id, started_at).unwrap();

    let preview = cycles::get_cycle_deletion_preview(&db.db, &session.id).unwrap();
    assert_eq!(preview.guard_code, None);
    assert!(reminders::find_for_target(
        &db.conn(),
        TargetKind::Session,
        &session.id,
        started_at + 900_000,
    )
    .unwrap()
    .is_some());

    cycles::delete_cycle(&db.db, &session.id).unwrap();
    let conn = db.conn();
    assert!(cycle_repo::get(&conn, &session.id).unwrap().is_none());
    assert!(reminders::find_for_target(
        &conn,
        TargetKind::Session,
        &session.id,
        started_at + 900_000,
    )
    .unwrap()
    .is_none());
    assert_eq!(cycle_repo::require(&conn, &day.id).unwrap().focused_time, 0);
    assert_eq!(
        cycle_repo::require(&conn, &week.id).unwrap().focused_time,
        0
    );
    assert_eq!(
        cycle_repo::require(&conn, &month.id).unwrap().focused_time,
        0
    );
}

#[test]
fn proposal_lock_rejects_container_delete_without_partial_session_changes() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, NOW);
    let session = cycles::add_session(
        &db.db,
        &AddSessionArgs {
            day_cycle_id: day.id.clone(),
            title: "protected focus".into(),
            duration_ms: Some(3_600_000),
            position: None,
        },
        NOW,
    )
    .unwrap()
    .value;
    let started_at = NOW + 30;
    let elapsed = 7 * 60 * 1000;
    cycles::start_cycle(&db.db, &session.id, started_at).unwrap();
    cycles::finish_cycle(&db.db, &session.id, started_at + elapsed).unwrap();
    let task = add_task(&db.db, &day.id, "original task", NOW + 1);
    proposals::apply_update_preview(
        &db.db,
        &task.id,
        &TaskInput {
            title: "proposed task".into(),
            ..Default::default()
        },
    )
    .unwrap();

    let before_day = cycle_repo::require(&db.conn(), &day.id).unwrap();
    let before_week = cycle_repo::require(&db.conn(), &week.id).unwrap();
    let before_month = cycle_repo::require(&db.conn(), &month.id).unwrap();
    let before_task = planner_lib::repository::tasks::require(&db.conn(), &task.id).unwrap();
    let reminder_at = started_at + 3_600_000;
    assert!(
        reminders::find_for_target(&db.conn(), TargetKind::Session, &session.id, reminder_at,)
            .unwrap()
            .is_some()
    );

    let err = cycles::delete_cycle(&db.db, &day.id).unwrap_err();
    assert_eq!(err_code(&err), "task_preview_locked");

    let conn = db.conn();
    assert_eq!(cycle_repo::require(&conn, &day.id).unwrap(), before_day);
    assert_eq!(cycle_repo::require(&conn, &week.id).unwrap(), before_week);
    assert_eq!(cycle_repo::require(&conn, &month.id).unwrap(), before_month);
    assert_eq!(
        planner_lib::repository::tasks::require(&conn, &task.id).unwrap(),
        before_task
    );
    assert!(cycle_repo::get(&conn, &session.id).unwrap().is_some());
    assert!(
        reminders::find_for_target(&conn, TargetKind::Session, &session.id, reminder_at,)
            .unwrap()
            .is_some()
    );
}

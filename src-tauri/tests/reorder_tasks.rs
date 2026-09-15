//! Regression tests for the sibling-group contract used by the editor tree.

mod common;

use common::{add_task, create_long_term, create_week, TestDb, NOW, TODAY};
use planner_lib::domain::proposal::ProposalKind;
use planner_lib::error::AppError;
use planner_lib::service::proposals::{self, TaskInput};
use planner_lib::service::tasks::{self, AddTaskArgs};

fn error_code(err: &AppError) -> String {
    match err {
        AppError::Validation { code, .. } | AppError::Conflict { code, .. } => code.clone(),
        AppError::NotFound { .. } => "not_found".into(),
        other => format!("{other:?}"),
    }
}

#[test]
fn reorder_visible_roots_accepts_cross_cycle_links_without_reparenting() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let goal_a = add_task(&db.db, &month.id, "Goal A", NOW);
    let goal_b = add_task(&db.db, &month.id, "Goal B", NOW + 1);
    let first = add_task(&db.db, &week.id, "First", NOW + 2);
    let second = add_task(&db.db, &week.id, "Second", NOW + 3);
    tasks::set_task_parent_link(&db.db, &first.id, Some(&goal_a.id)).unwrap();
    tasks::set_task_parent_link(&db.db, &second.id, Some(&goal_b.id)).unwrap();

    tasks::reorder_tasks(&db.db, &week.id, None, &[second.id.clone(), first.id.clone()])
        .unwrap();

    let conn = db.conn();
    let first_after = planner_lib::repository::tasks::require(&conn, &first.id).unwrap();
    let second_after = planner_lib::repository::tasks::require(&conn, &second.id).unwrap();
    assert_eq!(second_after.position, 0);
    assert_eq!(first_after.position, 1);
    assert_eq!(first_after.parent_id.as_deref(), Some(goal_a.id.as_str()));
    assert_eq!(second_after.parent_id.as_deref(), Some(goal_b.id.as_str()));
}

#[test]
fn reorder_rejects_mixed_nested_group_without_writing_positions() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let group_parent = add_task(&db.db, &week.id, "Group", NOW);
    let child = tasks::add_task(
        &db.db,
        &AddTaskArgs {
            cycle_id: week.id.clone(),
            title: "Child".into(),
            parent_id: Some(group_parent.id.clone()),
            position: Some(7),
            ..Default::default()
        },
        NOW + 1,
    )
    .unwrap()
    .value;
    let sibling = tasks::add_task(
        &db.db,
        &AddTaskArgs {
            cycle_id: week.id.clone(),
            title: "Sibling".into(),
            parent_id: Some(group_parent.id.clone()),
            position: Some(8),
            ..Default::default()
        },
        NOW + 2,
    )
    .unwrap()
    .value;
    let linked_root = tasks::add_task(
        &db.db,
        &AddTaskArgs {
            cycle_id: week.id.clone(),
            title: "Linked root".into(),
            position: Some(9),
            ..Default::default()
        },
        NOW + 3,
    )
    .unwrap()
    .value;
    let goal = add_task(&db.db, &month.id, "Long-term goal", NOW + 4);
    tasks::set_task_parent_link(&db.db, &linked_root.id, Some(&goal.id)).unwrap();

    // A real same-cycle nested group remains reorderable.
    tasks::reorder_tasks(
        &db.db,
        &week.id,
        Some(group_parent.id.as_str()),
        &[sibling.id.clone(), child.id.clone()],
    )
    .unwrap();
    assert_eq!(
        planner_lib::repository::tasks::require(&db.conn(), &child.id)
            .unwrap()
            .position,
        1
    );

    let err = tasks::reorder_tasks(
        &db.db,
        &week.id,
        Some(group_parent.id.as_str()),
        &[child.id.clone(), linked_root.id.clone()],
    )
    .unwrap_err();
    assert_eq!(error_code(&err), "task_not_in_group");

    let conn = db.conn();
    let child_after = planner_lib::repository::tasks::require(&conn, &child.id).unwrap();
    let linked_root_after = planner_lib::repository::tasks::require(&conn, &linked_root.id).unwrap();
    assert_eq!(child_after.position, 1);
    assert_eq!(linked_root_after.position, 9);
    assert_eq!(linked_root_after.parent_id.as_deref(), Some(goal.id.as_str()));
}

#[test]
fn reorder_rejects_locked_task_without_partially_writing_positions() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let first = tasks::add_task(
        &db.db,
        &AddTaskArgs {
            cycle_id: month.id.clone(),
            title: "First".into(),
            position: Some(7),
            ..Default::default()
        },
        NOW,
    )
    .unwrap()
    .value;
    let locked = tasks::add_task(
        &db.db,
        &AddTaskArgs {
            cycle_id: month.id.clone(),
            title: "Locked".into(),
            position: Some(9),
            ..Default::default()
        },
        NOW + 1,
    )
    .unwrap()
    .value;
    proposals::apply_update_preview(
        &db.db,
        &locked.id,
        &TaskInput {
            title: "Preview".into(),
            ..Default::default()
        },
    )
    .unwrap();

    let err = tasks::reorder_tasks(
        &db.db,
        &month.id,
        None,
        &[first.id.clone(), locked.id.clone()],
    )
    .unwrap_err();
    assert_eq!(error_code(&err), "task_preview_locked");

    let conn = db.conn();
    let first_after = planner_lib::repository::tasks::require(&conn, &first.id).unwrap();
    let locked_after = planner_lib::repository::tasks::require(&conn, &locked.id).unwrap();
    assert_eq!(first_after.position, 7);
    assert_eq!(locked_after.position, 9);
    assert_eq!(locked_after.proposal, Some(ProposalKind::Upsert));
    assert_eq!(locked_after.title, "Preview");
}

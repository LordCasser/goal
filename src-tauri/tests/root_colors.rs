//! Regression tests for deterministic default ownership colors.

mod common;

use common::{add_task, create_day, create_long_term, create_week, TestDb, NOW, TODAY};
use planner_lib::domain::cycle::LATER_CYCLE_ID;
use planner_lib::service::proposals::{self, TaskInput};
use planner_lib::service::tasks::{self, AddTaskArgs, TaskPatch};

#[test]
fn manual_creation_spreads_long_term_root_colors_and_leaves_other_levels_uncolored() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let first = add_task(&db.db, &month.id, "First", NOW);
    let second = add_task(&db.db, &month.id, "Second", NOW + 1);
    let third = add_task(&db.db, &month.id, "Third", NOW + 2);
    let explicit = tasks::add_task(
        &db.db,
        &AddTaskArgs {
            cycle_id: month.id.clone(),
            title: "Explicit".into(),
            root_color_key: Some("plum".into()),
            ..Default::default()
        },
        NOW + 3,
    )
    .unwrap()
    .value;
    let nested = tasks::add_task(
        &db.db,
        &AddTaskArgs {
            cycle_id: month.id.clone(),
            title: "Nested".into(),
            parent_id: Some(first.id.clone()),
            ..Default::default()
        },
        NOW + 4,
    )
    .unwrap()
    .value;

    assert_eq!(first.root_color_key.as_deref(), Some("red"));
    assert_eq!(second.root_color_key.as_deref(), Some("amber"));
    assert_eq!(third.root_color_key.as_deref(), Some("gold"));
    assert_eq!(explicit.root_color_key.as_deref(), Some("plum"));
    assert_eq!(nested.root_color_key, None);

    let week = create_week(&db.db, &month.id, TODAY);
    let weekly = add_task(&db.db, &week.id, "Weekly", NOW + 5);
    tasks::set_task_parent_link(&db.db, &weekly.id, Some(&first.id)).unwrap();
    let day = create_day(&db.db, &week.id, TODAY, NOW + 6);
    let daily = add_task(&db.db, &day.id, "Daily", NOW + 7);
    tasks::set_task_parent_link(&db.db, &daily.id, Some(&weekly.id)).unwrap();
    let later = add_task(&db.db, LATER_CYCLE_ID, "Later", NOW + 8);

    assert_eq!(
        weekly.root_color_key, None,
        "weekly rows inherit through links"
    );
    assert_eq!(
        daily.root_color_key, None,
        "daily rows inherit through links"
    );
    assert_eq!(later.root_color_key, None, "Later does not allocate colors");
}

#[test]
fn manual_no_color_survives_later_title_edits() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let goal = add_task(&db.db, &month.id, "Original", NOW);
    tasks::set_task_root_color(&db.db, &goal.id, None).unwrap();
    tasks::patch_task(
        &db.db,
        &goal.id,
        &TaskPatch {
            title: Some("Renamed".into()),
            ..Default::default()
        },
    )
    .unwrap();

    let stored = planner_lib::repository::tasks::require(&db.conn(), &goal.id).unwrap();
    assert_eq!(stored.title, "Renamed");
    assert_eq!(stored.root_color_key, None);
}

#[test]
fn an_existing_uncolored_empty_root_gets_color_on_first_title() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let empty = add_task(&db.db, &month.id, "", NOW);
    tasks::set_task_root_color(&db.db, &empty.id, None).unwrap();

    let titled = tasks::patch_task(
        &db.db,
        &empty.id,
        &TaskPatch {
            title: Some("First real goal".into()),
            ..Default::default()
        },
    )
    .unwrap()
    .value;
    assert_eq!(titled.root_color_key.as_deref(), Some("red"));
}

#[test]
fn same_cycle_nesting_clears_child_color_but_cross_cycle_link_keeps_parent_color() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let parent = add_task(&db.db, &month.id, "Parent", NOW);
    let child = add_task(&db.db, &month.id, "Child", NOW + 1);
    let child_color = child.root_color_key.clone();
    assert!(child_color.is_some());

    let nested = tasks::set_task_parent_link(&db.db, &child.id, Some(&parent.id))
        .unwrap()
        .value;
    assert_eq!(nested.parent_id.as_deref(), Some(parent.id.as_str()));
    assert_eq!(nested.root_color_key, None);

    let week = create_week(&db.db, &month.id, TODAY);
    let weekly = add_task(&db.db, &week.id, "Weekly", NOW + 2);
    tasks::set_task_parent_link(&db.db, &weekly.id, Some(&parent.id)).unwrap();
    let parent_after = planner_lib::repository::tasks::require(&db.conn(), &parent.id).unwrap();
    assert_eq!(parent_after.root_color_key, parent.root_color_key);
}

#[test]
fn ai_preview_reuses_or_creates_colored_roots_and_rejection_restores_state() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let empty = add_task(&db.db, &month.id, "", NOW);
    let allocated_color = empty.root_color_key.clone();
    assert!(
        allocated_color.is_some(),
        "blank root is preallocated deterministically"
    );

    let reused = proposals::apply_upsert_preview(
        &db.db,
        &month.id,
        &TaskInput {
            title: "AI reused goal".into(),
            ..Default::default()
        },
        NOW + 1,
    )
    .unwrap()
    .value;
    assert_eq!(reused.id, empty.id);
    assert_eq!(reused.root_color_key, allocated_color);
    proposals::undo_task_preview(&db.db, &empty.id).unwrap();
    let restored = planner_lib::repository::tasks::require(&db.conn(), &empty.id).unwrap();
    assert_eq!(restored.title, "");
    assert_eq!(restored.root_color_key, allocated_color);

    tasks::delete_task(&db.db, &empty.id).unwrap();
    let explicit = proposals::apply_upsert_preview(
        &db.db,
        &month.id,
        &TaskInput {
            title: "AI explicit goal".into(),
            root_color_key: Some("indigo".into()),
            ..Default::default()
        },
        NOW + 2,
    )
    .unwrap()
    .value;
    assert_eq!(explicit.root_color_key.as_deref(), Some("indigo"));
    proposals::undo_task_preview(&db.db, &explicit.id).unwrap();
    assert!(
        planner_lib::repository::tasks::get(&db.conn(), &explicit.id)
            .unwrap()
            .is_none()
    );

    let fresh = proposals::apply_upsert_preview(
        &db.db,
        &month.id,
        &TaskInput {
            title: "AI new goal".into(),
            ..Default::default()
        },
        NOW + 3,
    )
    .unwrap()
    .value;
    assert!(fresh.root_color_key.is_some());
}

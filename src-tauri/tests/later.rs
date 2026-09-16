//! Later plan-type retention and promotion integration tests.
//!
//! These tests drive the service entry points used by the command layer.  A
//! Later row is still an ordinary task; `later_plan_type` records the intent
//! that must survive while it is parked, and is cleared by promotion.
mod common;

use common::{add_task, create_day, create_long_term, create_week, TestDb, NOW, TODAY};
use planner_lib::domain::cycle::{CycleType, LATER_CYCLE_ID};
use planner_lib::repository::{cycles as cycle_repo, tasks as task_repo};
use planner_lib::service::{editor, proposals, reviews, settings, tasks};

fn task(db: &TestDb, id: &str) -> planner_lib::domain::task::Task {
    task_repo::require(&db.conn(), id).expect("task exists")
}

fn cycle(db: &TestDb, id: &str) -> planner_lib::domain::cycle::Cycle {
    cycle_repo::require(&db.conn(), id).expect("cycle exists")
}

fn add_child(
    db: &TestDb,
    cycle_id: &str,
    parent_id: &str,
    title: &str,
    now: i64,
) -> planner_lib::domain::task::Task {
    tasks::add_task(
        &db.db,
        &tasks::AddTaskArgs {
            cycle_id: cycle_id.into(),
            parent_id: Some(parent_id.into()),
            title: title.into(),
            ..Default::default()
        },
        now,
    )
    .expect("child task added")
    .value
}

#[test]
fn new_later_rows_default_to_month_and_patch_cannot_change_the_intent() {
    let db = TestDb::open();
    let parked = add_task(&db.db, LATER_CYCLE_ID, "capture this", NOW);
    assert_eq!(parked.later_plan_type, Some(CycleType::Month));

    tasks::patch_task(
        &db.db,
        &parked.id,
        &tasks::TaskPatch {
            title: Some("captured and renamed".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let after = task(&db, &parked.id);
    assert_eq!(after.title, "captured and renamed");
    assert_eq!(after.later_plan_type, Some(CycleType::Month));
}

#[test]
fn moving_a_same_cycle_subtree_to_later_records_one_source_type() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let root = add_task(&db.db, &week.id, "weekly root", NOW);
    let child = add_child(&db, &week.id, &root.id, "weekly step", NOW + 1);

    tasks::move_task(&db.db, &root.id, LATER_CYCLE_ID, None).unwrap();

    let moved_root = task(&db, &root.id);
    let moved_child = task(&db, &child.id);
    assert_eq!(moved_root.cycle_id, LATER_CYCLE_ID);
    assert_eq!(moved_child.cycle_id, LATER_CYCLE_ID);
    assert_eq!(moved_root.later_plan_type, Some(CycleType::Week));
    assert_eq!(moved_child.later_plan_type, Some(CycleType::Week));
    assert_eq!(moved_child.parent_id.as_deref(), Some(root.id.as_str()));
}

#[test]
fn later_separates_long_term_and_week_roots_then_restores_the_link() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let goal = add_task(&db.db, &month.id, "long-term goal", NOW);
    let weekly = add_task(&db.db, &week.id, "weekly commitment", NOW + 1);
    tasks::set_task_parent_link(&db.db, &weekly.id, Some(&goal.id)).unwrap();

    tasks::move_task(&db.db, &goal.id, LATER_CYCLE_ID, None).unwrap();
    tasks::move_task(&db.db, &weekly.id, LATER_CYCLE_ID, None).unwrap();

    assert_eq!(task(&db, &goal.id).later_plan_type, Some(CycleType::Month));
    assert_eq!(task(&db, &weekly.id).later_plan_type, Some(CycleType::Week));
    assert_eq!(
        task(&db, &weekly.id).parent_id.as_deref(),
        Some(goal.id.as_str())
    );

    // The Later container is shared, but these are two roots of different
    // planning intents.  The editor must not make the weekly item a child of
    // the parked long-term goal merely because both rows have cycle_id=later.
    let later = editor::get_editor_workspace(&db.db, LATER_CYCLE_ID).unwrap();
    assert_eq!(later.tasks.len(), 2);
    assert!(later.tasks.iter().any(|node| node.task.id == goal.id));
    assert!(later.tasks.iter().any(|node| node.task.id == weekly.id));

    let target_month = create_long_term(&db.db, "2026-10-01", 1);
    tasks::promote_later_task(
        &db.db,
        &goal.id,
        Some(target_month.id.as_str()),
        common::today(),
        NOW + 2,
    )
    .unwrap();
    assert_eq!(task(&db, &goal.id).later_plan_type, None);
    assert_eq!(task(&db, &weekly.id).cycle_id, LATER_CYCLE_ID);

    let restored_week =
        tasks::promote_later_task(&db.db, &weekly.id, None, common::today(), NOW + 3)
            .unwrap()
            .value;
    assert_eq!(
        cycle(&db, &restored_week.cycle_id).cycle_type,
        CycleType::Week
    );
    assert_eq!(restored_week.later_plan_type, None);
    assert_eq!(restored_week.parent_id.as_deref(), Some(goal.id.as_str()));
}

#[test]
fn week_promotion_without_target_uses_the_current_week_and_week_start_setting() {
    let db = TestDb::open();
    settings::set_week_start_day(&db.db, 7).unwrap();
    let month = create_long_term(&db.db, "2026-09-01", 1);
    let old_week = create_week(&db.db, &month.id, "2026-09-06");
    let parked = add_task(&db.db, &old_week.id, "this week", NOW);
    tasks::move_task(&db.db, &parked.id, LATER_CYCLE_ID, None).unwrap();

    let restored = tasks::promote_later_task(&db.db, &parked.id, None, common::today(), NOW + 1)
        .unwrap()
        .value;
    let target = cycle(&db, &restored.cycle_id);
    assert_eq!(target.cycle_type, CycleType::Week);
    assert_eq!(target.starts_on.as_deref(), Some("2026-09-13"));
    assert_ne!(
        target.id, old_week.id,
        "promotion follows today, not the old source week"
    );
    assert_eq!(restored.later_plan_type, None);
}

#[test]
fn day_promotion_without_target_uses_today() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, "2026-09-01", 1);
    let old_week = create_week(&db.db, &month.id, "2026-09-06");
    let old_day = create_day(&db.db, &old_week.id, "2026-09-10", NOW);
    let parked = add_task(&db.db, &old_day.id, "today's action", NOW);
    tasks::move_task(&db.db, &parked.id, LATER_CYCLE_ID, None).unwrap();

    let restored = tasks::promote_later_task(&db.db, &parked.id, None, common::today(), NOW + 1)
        .unwrap()
        .value;
    let target = cycle(&db, &restored.cycle_id);
    assert_eq!(target.cycle_type, CycleType::Day);
    assert_eq!(target.starts_on.as_deref(), Some(TODAY));
    assert_ne!(
        target.id, old_day.id,
        "promotion follows today, not the old source day"
    );
    assert_eq!(restored.later_plan_type, None);
}

#[test]
fn month_promotion_requires_an_explicit_target_and_clears_metadata() {
    let db = TestDb::open();
    let source = create_long_term(&db.db, TODAY, 1);
    let parked = add_task(&db.db, &source.id, "long-term idea", NOW);
    tasks::move_task(&db.db, &parked.id, LATER_CYCLE_ID, None).unwrap();

    assert!(tasks::promote_later_task(&db.db, &parked.id, None, common::today(), NOW + 1).is_err());
    let still_parked = task(&db, &parked.id);
    assert_eq!(still_parked.cycle_id, LATER_CYCLE_ID);
    assert_eq!(still_parked.later_plan_type, Some(CycleType::Month));

    let target = create_long_term(&db.db, "2026-10-01", 1);
    let restored = tasks::promote_later_task(
        &db.db,
        &parked.id,
        Some(target.id.as_str()),
        common::today(),
        NOW + 2,
    )
    .unwrap()
    .value;
    assert_eq!(restored.cycle_id, target.id);
    assert_eq!(restored.later_plan_type, None);

    // Moving it out and parking it again derives the current source type;
    // it does not leave a stale value from the previous Later stay.
    tasks::move_task(&db.db, &parked.id, LATER_CYCLE_ID, None).unwrap();
    assert_eq!(
        task(&db, &parked.id).later_plan_type,
        Some(CycleType::Month)
    );
}

#[test]
fn promotion_failures_are_atomic_and_duplicate_promotion_does_not_create_data() {
    // A proposal lock rejects the operation before any move or target
    // creation.  The Later row and its type remain inspectable.
    let db = TestDb::open();
    let locked = add_task(&db.db, LATER_CYCLE_ID, "locked", NOW);
    proposals::apply_update_preview(
        &db.db,
        &locked.id,
        &proposals::TaskInput {
            title: "pending edit".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let before_cycles: i64 = db
        .conn()
        .query_row("SELECT COUNT(*) FROM cycles", [], |row| row.get(0))
        .unwrap();
    assert!(tasks::promote_later_task(&db.db, &locked.id, None, common::today(), NOW + 2).is_err());
    assert_eq!(task(&db, &locked.id).cycle_id, LATER_CYCLE_ID);
    assert_eq!(
        task(&db, &locked.id).later_plan_type,
        Some(CycleType::Month)
    );
    assert_eq!(
        db.conn()
            .query_row("SELECT COUNT(*) FROM cycles", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        before_cycles
    );

    // An explicit type mismatch and an ended target also leave the source in
    // Later.  No target is guessed or partially applied.
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let weekly = add_task(&db.db, &week.id, "weekly", NOW + 3);
    tasks::move_task(&db.db, &weekly.id, LATER_CYCLE_ID, None).unwrap();
    let day = create_day(&db.db, &week.id, TODAY, NOW + 4);
    assert!(tasks::promote_later_task(
        &db.db,
        &weekly.id,
        Some(day.id.as_str()),
        common::today(),
        NOW + 5,
    )
    .is_err());
    assert_eq!(task(&db, &weekly.id).cycle_id, LATER_CYCLE_ID);
    assert_eq!(task(&db, &weekly.id).later_plan_type, Some(CycleType::Week));

    let ended_target = create_long_term(&db.db, "2026-10-01", 1);
    planner_lib::service::cycles::start_cycle(&db.db, &ended_target.id, NOW + 6).unwrap();
    planner_lib::service::cycles::finish_cycle(&db.db, &ended_target.id, NOW + 7).unwrap();
    let month_item = add_task(&db.db, LATER_CYCLE_ID, "ended target", NOW + 8);
    assert!(tasks::promote_later_task(
        &db.db,
        &month_item.id,
        Some(ended_target.id.as_str()),
        common::today(),
        NOW + 9,
    )
    .is_err());
    assert_eq!(task(&db, &month_item.id).cycle_id, LATER_CYCLE_ID);

    // Once a move succeeds, retrying the same task is rejected as a
    // non-Later source and does not duplicate a cycle or task.
    let target = create_long_term(&db.db, "2026-11-01", 1);
    let once = add_task(&db.db, LATER_CYCLE_ID, "once", NOW + 10);
    let restored = tasks::promote_later_task(
        &db.db,
        &once.id,
        Some(target.id.as_str()),
        common::today(),
        NOW + 11,
    )
    .unwrap()
    .value;
    let cycles_after_first = db
        .conn()
        .query_row("SELECT COUNT(*) FROM cycles", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap();
    assert!(tasks::promote_later_task(
        &db.db,
        &once.id,
        Some(target.id.as_str()),
        common::today(),
        NOW + 12,
    )
    .is_err());
    assert_eq!(task(&db, &once.id).cycle_id, restored.cycle_id);
    assert_eq!(
        db.conn()
            .query_row("SELECT COUNT(*) FROM cycles", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        cycles_after_first
    );
}

#[test]
fn sql_rejects_invalid_later_plan_type_and_non_later_metadata() {
    let db = TestDb::open();
    let later = add_task(&db.db, LATER_CYCLE_ID, "later", NOW);
    assert!(db
        .conn()
        .execute(
            "UPDATE tasks SET later_plan_type = 'quarter' WHERE id = ?1",
            rusqlite::params![later.id]
        )
        .is_err());

    let month = create_long_term(&db.db, TODAY, 1);
    let ordinary = add_task(&db.db, &month.id, "ordinary", NOW + 1);
    assert!(db
        .conn()
        .execute(
            "UPDATE tasks SET later_plan_type = 'week' WHERE id = ?1",
            rusqlite::params![ordinary.id]
        )
        .is_err());
}

#[test]
fn review_later_disposition_records_the_source_cycle_type() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let item = add_task(&db.db, &week.id, "review later", NOW);
    reviews::save_cycle_review(
        &db.db,
        &reviews::SaveReviewArgs {
            cycle_id: week.id.clone(),
            answers: Vec::new(),
        },
        NOW + 1,
    )
    .unwrap();

    reviews::apply_review_disposition(
        &db.db,
        &week.id,
        &item.id,
        reviews::DISPOSITION_LATER,
        NOW + 2,
    )
    .unwrap();
    let moved = task(&db, &item.id);
    assert_eq!(moved.cycle_id, LATER_CYCLE_ID);
    assert_eq!(moved.later_plan_type, Some(CycleType::Week));
    let review = reviews::get_cycle_review(&db.db, &week.id)
        .unwrap()
        .unwrap();
    assert_eq!(review.dispositions.len(), 1);
    assert_eq!(review.dispositions[0].task_id, item.id);
    assert_eq!(
        review.dispositions[0].disposition,
        reviews::DISPOSITION_LATER
    );
}

#[test]
fn failure_after_creating_the_default_week_rolls_back_every_write() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, "2026-09-01", 1);
    let old_week = create_week(&db.db, &month.id, "2026-09-01");
    let item = add_task(&db.db, &old_week.id, "Blocked move", NOW);
    tasks::move_task(&db.db, &item.id, LATER_CYCLE_ID, None).unwrap();
    // Inject a storage failure after target creation but before the move can
    // complete; the real transaction must undo its newly created week too.
    db.conn().execute_batch("CREATE TRIGGER fail_later_move BEFORE UPDATE OF cycle_id ON tasks WHEN OLD.cycle_id = 'later' AND NEW.cycle_id != 'later' BEGIN SELECT RAISE(ABORT, 'test storage failure'); END;").unwrap();
    let before = cycle_repo::list_planner_cycles(&db.conn()).unwrap();
    assert!(tasks::promote_later_task(&db.db, &item.id, None, common::today(), NOW + 1).is_err());
    assert_eq!(cycle_repo::list_planner_cycles(&db.conn()).unwrap(), before);
    assert_eq!(task(&db, &item.id).cycle_id, LATER_CYCLE_ID);
    assert_eq!(task(&db, &item.id).later_plan_type, Some(CycleType::Week));
}

#[test]
fn promoting_a_parked_substep_detaches_its_old_week_parent_but_keeps_children() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, "2026-09-01", 1);
    let old_week = create_week(&db.db, &month.id, "2026-09-01");
    let parent = add_task(&db.db, &old_week.id, "Original parent", NOW);
    let item = add_child(&db, &old_week.id, &parent.id, "Park this step", NOW + 1);
    let child = add_child(&db, &old_week.id, &item.id, "Keep this child", NOW + 2);
    tasks::move_task(&db.db, &item.id, LATER_CYCLE_ID, None).unwrap();
    // New nested steps added while parked inherit the parent's weekly intent.
    let added = add_child(&db, LATER_CYCLE_ID, &item.id, "New step", NOW + 3);
    assert_eq!(added.later_plan_type, Some(CycleType::Week));
    let moved = tasks::promote_later_task(&db.db, &item.id, None, common::today(), NOW + 4)
        .unwrap()
        .value;
    assert_eq!(moved.parent_id, None);
    assert_eq!(task(&db, &parent.id).cycle_id, old_week.id);
    for id in [&child.id, &added.id] {
        let step = task(&db, id);
        assert_eq!(step.cycle_id, moved.cycle_id);
        assert_eq!(step.parent_id.as_deref(), Some(item.id.as_str()));
        assert_eq!(step.later_plan_type, None);
    }
    // A subsequent parking records the then-current level, not the old one.
    tasks::move_task(&db.db, &item.id, &month.id, None).unwrap();
    tasks::move_task(&db.db, &item.id, LATER_CYCLE_ID, None).unwrap();
    assert_eq!(task(&db, &item.id).later_plan_type, Some(CycleType::Month));
}

#[test]
fn day_promotion_reuses_today_and_rejects_an_archived_target() {
    let db = TestDb::open();
    let today = planner_lib::service::cycles::get_or_create_day(&db.db, common::today(), NOW)
        .unwrap()
        .value;
    let item = add_task(&db.db, &today.id, "Daily action", NOW);
    tasks::move_task(&db.db, &item.id, LATER_CYCLE_ID, None).unwrap();
    db.conn()
        .execute("UPDATE cycles SET archived = 1 WHERE id = ?1", [&today.id])
        .unwrap();
    assert!(tasks::promote_later_task(&db.db, &item.id, None, common::today(), NOW + 1).is_err());
    db.conn()
        .execute("UPDATE cycles SET archived = 0 WHERE id = ?1", [&today.id])
        .unwrap();
    let moved = tasks::promote_later_task(&db.db, &item.id, None, common::today(), NOW + 2)
        .unwrap()
        .value;
    assert_eq!(moved.cycle_id, today.id);
}

mod common;

use common::{add_task, create_day, create_long_term, create_week, TestDb, NOW, TODAY};
use planner_lib::domain::calendar::parse_date;
use planner_lib::repository::{cycles as cycle_repo, tasks as task_repo};
use planner_lib::service::{cycles, settings, tasks};

#[test]
fn disabled_setting_and_implicit_day_ensure_do_not_carry() {
    let db = TestDb::open();
    let prior = cycles::get_or_create_day(&db.db, parse_date("2026-09-14").unwrap(), NOW)
        .unwrap()
        .value;
    add_task(&db.db, &prior.id, "Keep separate", NOW);
    let next = cycles::create_day_plan(&db.db, parse_date("2026-09-15").unwrap(), NOW + 1)
        .unwrap()
        .value;
    assert!(task_repo::list_visible_by_cycle(&db.conn(), &next.id).unwrap().is_empty());

    settings::set_auto_carry_unfinished(&db.db, true).unwrap();
    let implicit = cycles::get_or_create_day(&db.db, parse_date("2026-09-16").unwrap(), NOW + 2)
        .unwrap()
        .value;
    assert!(task_repo::list_visible_by_cycle(&db.conn(), &implicit.id).unwrap().is_empty());
}

#[test]
fn new_week_carries_only_adjacent_unfinished_items_and_keeps_distinct_goals() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let prior = create_week(&db.db, &month.id, "2026-09-09");
    let goal_a = add_task(&db.db, &month.id, "Goal", NOW);
    let goal_b = add_task(&db.db, &month.id, "Goal", NOW + 1);
    let first = add_task(&db.db, &prior.id, "Check", NOW);
    let second = add_task(&db.db, &prior.id, "Check", NOW + 1);
    let duplicate = add_task(&db.db, &prior.id, "Check", NOW + 2);
    let done = add_task(&db.db, &prior.id, "Done", NOW + 2);
    tasks::set_task_parent_link(&db.db, &first.id, Some(&goal_a.id)).unwrap();
    tasks::set_task_parent_link(&db.db, &second.id, Some(&goal_b.id)).unwrap();
    tasks::set_task_parent_link(&db.db, &duplicate.id, Some(&goal_a.id)).unwrap();
    tasks::patch_task(
        &db.db,
        &first.id,
        &tasks::TaskPatch {
            note: Some("source note".into()),
            ..Default::default()
        },
    )
    .unwrap();
    tasks::patch_task(
        &db.db,
        &done.id,
        &tasks::TaskPatch {
            completed: Some(true),
            ..Default::default()
        },
    )
    .unwrap();

    settings::set_auto_carry_unfinished(&db.db, true).unwrap();
    let current = create_week(&db.db, &month.id, TODAY);
    let copied = task_repo::list_visible_by_cycle(&db.conn(), &current.id).unwrap();
    assert_eq!(copied.len(), 2);
    assert_ne!(copied[0].parent_id, copied[1].parent_id);
    assert!(copied.iter().all(|task| task.note.is_empty()));
    assert!(copied.iter().any(|task| task.copied_from_task_id.as_deref() == Some(first.id.as_str())));
    assert!(copied.iter().any(|task| task.copied_from_task_id.as_deref() == Some(second.id.as_str())));

    let reopened = cycles::create_day_plan(&db.db, parse_date(TODAY).unwrap(), NOW + 3)
        .unwrap()
        .value;
    assert!(reopened.starts_on.is_some());
    // A prior week alone is not an adjacent prior day.
    assert!(task_repo::list_visible_by_cycle(&db.conn(), &reopened.id).unwrap().is_empty());
}

#[test]
fn explicit_new_day_remaps_carried_weekly_goal_and_is_idempotent() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let previous_week = create_week(&db.db, &month.id, "2026-09-20");
    let previous_day = create_day(&db.db, &previous_week.id, "2026-09-20", NOW);
    let weekly_goal = add_task(&db.db, &previous_week.id, "Release", NOW);
    let day_item = add_task(&db.db, &previous_day.id, "Check build", NOW);
    tasks::set_task_parent_link(&db.db, &day_item.id, Some(&weekly_goal.id)).unwrap();
    settings::set_auto_carry_unfinished(&db.db, true).unwrap();

    let monday = parse_date("2026-09-21").unwrap();
    let created = cycles::create_day_plan(&db.db, monday, NOW + 1).unwrap().value;
    let new_week = cycle_repo::require(&db.conn(), created.parent_id.as_deref().unwrap()).unwrap();
    assert_ne!(new_week.id, previous_week.id);
    let new_goal = task_repo::list_visible_by_cycle(&db.conn(), &new_week.id).unwrap();
    let new_day = task_repo::list_visible_by_cycle(&db.conn(), &created.id).unwrap();
    assert_eq!(new_goal.len(), 1);
    assert_eq!(new_day.len(), 1);
    assert_eq!(new_day[0].parent_id.as_deref(), Some(new_goal[0].id.as_str()));
    assert_eq!(new_day[0].copied_from_task_id.as_deref(), Some(day_item.id.as_str()));

    let retry = cycles::create_day_plan(&db.db, monday, NOW + 2).unwrap().value;
    assert_eq!(retry.id, created.id);
    assert_eq!(task_repo::list_visible_by_cycle(&db.conn(), &created.id).unwrap().len(), 1);
}

#[test]
fn carry_failure_rolls_back_new_day_and_week() {
    let db = TestDb::open();
    let prior = cycles::get_or_create_day(&db.db, parse_date("2026-09-20").unwrap(), NOW)
        .unwrap()
        .value;
    add_task(&db.db, &prior.id, "Carry", NOW);
    settings::set_auto_carry_unfinished(&db.db, true).unwrap();
    db.conn().execute_batch(
        "CREATE TRIGGER reject_carry BEFORE INSERT ON tasks
         WHEN NEW.copied_from_task_id IS NOT NULL
         BEGIN SELECT RAISE(ABORT, 'reject carry'); END;",
    ).unwrap();

    assert!(cycles::create_day_plan(&db.db, parse_date("2026-09-21").unwrap(), NOW + 1).is_err());
    assert!(cycle_repo::get_by_calendar_key(&db.conn(), "day:2026-09-21").unwrap().is_none());
    assert!(cycle_repo::get_by_calendar_key(&db.conn(), "week:2026-09-21").unwrap().is_none());
}

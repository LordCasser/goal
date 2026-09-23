mod common;

use common::{add_task, create_day, create_long_term, create_week, TestDb, NOW, TODAY};
use planner_lib::service::tasks::{self, TaskPatch};

#[test]
fn detail_links_include_only_direct_cross_plan_children_across_dates() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week_one = create_week(&db.db, &month.id, "2026-09-16");
    let week_two = create_week(&db.db, &month.id, "2026-09-23");
    let day_one = create_day(&db.db, &week_one.id, "2026-09-16", NOW);
    let day_two = create_day(&db.db, &week_two.id, "2026-09-23", NOW + 1);
    let goal = add_task(&db.db, &month.id, "Long-term goal", NOW);
    let same_cycle_step = add_task(&db.db, &month.id, "Internal step", NOW + 1);
    tasks::set_task_parent_link(&db.db, &same_cycle_step.id, Some(&goal.id)).unwrap();
    let weekly = add_task(&db.db, &week_one.id, "Weekly item", NOW + 2);
    tasks::set_task_parent_link(&db.db, &weekly.id, Some(&goal.id)).unwrap();
    let direct_daily = add_task(&db.db, &day_two.id, "Direct daily item", NOW + 3);
    tasks::set_task_parent_link(&db.db, &direct_daily.id, Some(&goal.id)).unwrap();
    let indirect_daily = add_task(&db.db, &day_one.id, "Indirect daily item", NOW + 4);
    tasks::set_task_parent_link(&db.db, &indirect_daily.id, Some(&weekly.id)).unwrap();

    let goal_children = tasks::get_direct_linked_children(&db.db, &goal.id).unwrap();
    assert_eq!(goal_children.iter().map(|child| child.id.as_str()).collect::<Vec<_>>(),
        [weekly.id.as_str(), direct_daily.id.as_str()]);
    assert_eq!(goal_children.iter().map(|child| child.cycle_type.as_str()).collect::<Vec<_>>(),
        ["week", "day"]);
    let week_children = tasks::get_direct_linked_children(&db.db, &weekly.id).unwrap();
    assert_eq!(week_children.iter().map(|child| child.id.as_str()).collect::<Vec<_>>(),
        [indirect_daily.id.as_str()]);
    assert!(tasks::get_direct_linked_children(&db.db, &direct_daily.id).unwrap().is_empty());
}

#[test]
fn same_title_is_partitioned_by_direct_goal_identity() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, "2026-09-14");
    let monday = create_day(&db.db, &week.id, "2026-09-14", NOW);
    let tuesday = create_day(&db.db, &week.id, "2026-09-15", NOW + 1);
    let goal_a = add_task(&db.db, &week.id, "Goal", NOW);
    let goal_b = add_task(&db.db, &week.id, "Goal", NOW + 1);
    let first = add_task(&db.db, &monday.id, " Write report ", NOW);
    let second = add_task(&db.db, &tuesday.id, "Write report", NOW);
    let other = add_task(&db.db, &tuesday.id, "Write report", NOW + 1);
    let unlinked = add_task(&db.db, &monday.id, "Write report", NOW + 1);
    for (task, goal) in [(&first, &goal_a), (&second, &goal_a), (&other, &goal_b)] {
        tasks::set_task_parent_link(&db.db, &task.id, Some(&goal.id)).unwrap();
    }
    tasks::patch_task(
        &db.db,
        &first.id,
        &TaskPatch {
            note: Some("Outline".into()),
            ..Default::default()
        },
    )
    .unwrap();
    tasks::patch_task(
        &db.db,
        &second.id,
        &TaskPatch {
            note: Some("Draft".into()),
            ..Default::default()
        },
    )
    .unwrap();

    let grouped = tasks::get_task_continuity(&db.db, &second.id)
        .unwrap()
        .unwrap();
    assert_eq!(grouped.parent_goal_id.as_deref(), Some(goal_a.id.as_str()));
    assert_eq!(grouped.episodes.len(), 1);
    assert_eq!(grouped.episodes[0].elapsed_days, 2);
    assert_eq!(grouped.episodes[0].recorded_days, 2);
    assert_eq!(
        grouped.episodes[0].records.iter().map(|r| r.note.as_str()).collect::<Vec<_>>(),
        ["Outline", "Draft"]
    );
    assert_eq!(
        tasks::get_task_continuity(&db.db, &other.id).unwrap().unwrap().episodes[0].records.len(),
        1
    );
    assert_eq!(
        tasks::get_task_continuity(&db.db, &unlinked.id).unwrap().unwrap().episodes[0].records.len(),
        1
    );
    tasks::set_task_parent_link(&db.db, &second.id, Some(&goal_b.id)).unwrap();
    assert_eq!(
        tasks::get_task_continuity(&db.db, &first.id).unwrap().unwrap().episodes[0].records.len(),
        1
    );
    assert_eq!(
        tasks::get_task_continuity(&db.db, &second.id).unwrap().unwrap().episodes[0].records.len(),
        2
    );
}

#[test]
fn completion_closes_date_bucket_and_new_record_starts_a_new_episode() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, "2026-09-14");
    let monday = create_day(&db.db, &week.id, "2026-09-14", NOW);
    let wednesday = create_day(&db.db, &week.id, "2026-09-16", NOW + 1);
    let friday = create_day(&db.db, &week.id, "2026-09-18", NOW + 2);
    let first = add_task(&db.db, &monday.id, "Write report", NOW);
    let second = add_task(&db.db, &wednesday.id, "Write report", NOW + 1);
    let same_day = add_task(&db.db, &wednesday.id, "Write report", NOW + 2);
    tasks::patch_task(
        &db.db,
        &second.id,
        &TaskPatch {
            completed: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    let later = add_task(&db.db, &friday.id, "Write report", NOW + 3);
    let continuity = tasks::get_task_continuity(&db.db, &first.id)
        .unwrap()
        .unwrap();
    assert_eq!(continuity.selected_episode_index, 0);
    assert_eq!(continuity.episodes.len(), 2);
    assert_eq!(continuity.episodes[0].elapsed_days, 3);
    assert_eq!(continuity.episodes[0].recorded_days, 2);
    assert_eq!(continuity.episodes[0].records.len(), 3);
    assert!(continuity.episodes[0].completed);
    assert_eq!(continuity.episodes[1].records[0].task_id, later.id);
    assert!(!continuity.episodes[1].completed);
    assert_eq!(
        tasks::get_task_continuity(&db.db, &same_day.id)
            .unwrap()
            .unwrap()
            .selected_episode_index,
        0
    );
}

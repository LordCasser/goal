//! Service-layer behaviour tests against throwaway databases. These drive the
//! same public APIs the command layer calls (the architecture's "real entry
//! points"), one test per spec scenario.
//! Verification entry point: `cargo test --test services`

mod common;

use common::{add_task, create_day, create_long_term, create_week, TestDb, NOW, TODAY};
use planner_lib::domain::cycle::CycleType;
use planner_lib::error::AppError;
use planner_lib::service::cycles::{self, CreateCycleArgs};
use planner_lib::service::proposals::{self, TaskInput};
use planner_lib::service::tasks::{self};

fn err_code(err: &AppError) -> String {
    match err {
        AppError::Validation { code, .. } | AppError::Conflict { code, .. } => code.clone(),
        AppError::NotFound { .. } => "not_found".into(),
        other => format!("{other:?}"),
    }
}

fn long_term_args(months: i64) -> CreateCycleArgs {
    CreateCycleArgs {
        cycle_type: "month".into(),
        duration_months: Some(months),
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// Cycles
// ---------------------------------------------------------------------------

#[test]
fn long_term_durations_use_28_day_months() {
    let db = TestDb::open();
    for (months, expected_ms, days) in [
        (1, 2_419_200_000i64, 28i64),
        (3, 7_257_600_000, 84),
        (6, 14_515_200_000, 168),
    ] {
        let cycle = create_long_term(&db.db, TODAY, months);
        assert_eq!(cycle.cycle_type, CycleType::Month);
        assert_eq!(cycle.duration, Some(expected_ms));
        let starts = cycle.starts_on.clone().unwrap();
        let ends = cycle.ends_on.clone().unwrap();
        assert_eq!(starts, TODAY);
        let expected_end =
            planner_lib::domain::calendar::format_date(planner_lib::domain::calendar::add_days(
                planner_lib::domain::calendar::parse_date(TODAY).unwrap(),
                days,
            ));
        assert_eq!(ends, expected_end, "ends_on = starts_on + N x 28 days");
        // Calendar identity carries both bounds (spec: 长周期的键体现起止).
        assert_eq!(
            cycle.calendar_key.as_deref(),
            Some(format!("long-term:{starts}:{ends}").as_str())
        );
    }
}

#[test]
fn long_term_rejects_durations_outside_allowed_set() {
    let db = TestDb::open();
    for months in [0i64, 2, 4, 12] {
        let err =
            cycles::create_planning_cycle(&db.db, &long_term_args(months), common::today(), NOW)
                .expect_err("only 1/3/6 product months are allowed");
        assert!(
            matches!(err, AppError::Validation { .. }),
            "months={months} must be a validation error"
        );
    }
}

#[test]
fn start_requires_duration() {
    let db = TestDb::open();
    // A session without duration and the Later container (duration 0) both
    // refuse to start.
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, NOW);
    let session = planner_lib::service::cycles::add_session(
        &db.db,
        &cycles::AddSessionArgs {
            task_id: None,
            day_cycle_id: day.id.clone(),
            title: "Focus".into(),
            duration_ms: None,
            position: None,
        },
        NOW,
    )
    .unwrap()
    .value;
    let err = cycles::start_cycle(&db.db, &session.id, NOW).unwrap_err();
    assert_eq!(err_code(&err), "cycle_duration_required");

    let later = cycles::get_planner_state(&db.db).unwrap().later;
    let err = cycles::start_cycle(&db.db, &later.id, NOW).unwrap_err();
    assert_eq!(err_code(&err), "cycle_duration_required");
}

#[test]
fn week_can_be_created_without_long_term_goals() {
    let db = TestDb::open();
    let week = cycles::create_planning_cycle(
        &db.db,
        &CreateCycleArgs {
            cycle_type: "week".into(),
            ..Default::default()
        },
        common::today(),
        NOW,
    )
    .unwrap()
    .value;
    assert_eq!(week.parent_id, None);
    add_task(&db.db, &week.id, "Book a repair", NOW);
    add_task(&db.db, &week.id, "Renew membership", NOW);
    let day = cycles::get_or_create_day(&db.db, common::today(), NOW)
        .unwrap()
        .value;
    assert_eq!(day.parent_id.as_deref(), Some(week.id.as_str()));
}

#[test]
fn opening_an_independent_day_does_not_force_a_parent_or_duplicate_it() {
    let db = TestDb::open();
    let day = cycles::create_planning_cycle(
        &db.db,
        &CreateCycleArgs {
            cycle_type: "day".into(),
            ..Default::default()
        },
        common::today(),
        NOW,
    )
    .unwrap()
    .value;
    let reopened = cycles::get_or_create_day(&db.db, common::today(), NOW + 1)
        .unwrap()
        .value;
    assert_eq!(reopened.id, day.id);
    assert_eq!(reopened.parent_id, None);
    assert_eq!(
        cycles::get_planner_state(&db.db)
            .unwrap()
            .cycles
            .iter()
            .filter(|cycle| cycle.id != "later")
            .count(),
        1
    );
}

#[test]
fn independent_weeks_copy_unfinished_tasks_from_the_previous_date() {
    let db = TestDb::open();
    let args = CreateCycleArgs {
        cycle_type: "week".into(),
        ..Default::default()
    };
    let first = cycles::create_planning_cycle(&db.db, &args, common::today(), NOW)
        .unwrap()
        .value;
    let item = add_task(&db.db, &first.id, "Arrange a repair", NOW);
    let next_date = planner_lib::domain::calendar::add_days(common::today(), 7);
    let next = cycles::create_planning_cycle(&db.db, &args, next_date, NOW + 1)
        .unwrap()
        .value;
    cycles::copy_uncompleted_from_previous(&db.db, &next.id, NOW + 2).unwrap();
    let workspace = planner_lib::service::editor::get_editor_workspace(&db.db, &next.id).unwrap();
    assert_eq!(workspace.tasks.len(), 1);
    assert_eq!(
        workspace.tasks[0].task.copied_from_task_id.as_deref(),
        Some(item.id.as_str())
    );
}

#[test]
fn work_mix_counts_real_commitments_and_follows_optional_goal_links() {
    use planner_lib::service::editor;
    let db = TestDb::open();
    let day = cycles::get_or_create_day(&db.db, common::today(), NOW)
        .unwrap()
        .value;
    let week_id = day.parent_id.as_deref().unwrap();
    let month = create_long_term(&db.db, TODAY, 1);
    let goal = add_task(&db.db, &month.id, "Launch the product", NOW);
    let planned = add_task(&db.db, week_id, "Test the release", NOW);
    let temporary = add_task(&db.db, week_id, "Arrange a repair", NOW);
    tasks::set_task_parent_link(&db.db, &planned.id, Some(&goal.id)).unwrap();
    for (title, parent) in [
        ("Run the checks", Some(planned.id.as_str())),
        ("Call the repair shop", Some(temporary.id.as_str())),
        ("Receive a delivery", None),
    ] {
        let task = add_task(&db.db, &day.id, title, NOW);
        tasks::set_task_parent_link(&db.db, &task.id, parent).unwrap();
        if parent.is_none() {
            tasks::add_task(
                &db.db,
                &tasks::AddTaskArgs {
                    cycle_id: day.id.clone(),
                    title: "Check the parcel".into(),
                    parent_id: Some(task.id),
                    ..Default::default()
                },
                NOW,
            )
            .unwrap();
        }
    }
    add_task(&db.db, &day.id, "", NOW);
    let mix = editor::get_editor_workspace(&db.db, &day.id)
        .unwrap()
        .work_mix
        .unwrap();
    assert_eq!(
        (
            mix.total,
            mix.long_term,
            mix.weekly_standalone,
            mix.daily_standalone
        ),
        (3, 1, 1, 1)
    );
    tasks::set_task_parent_link(&db.db, &planned.id, None).unwrap();
    let mix = editor::get_editor_workspace(&db.db, &day.id)
        .unwrap()
        .work_mix
        .unwrap();
    assert_eq!(
        (
            mix.total,
            mix.long_term,
            mix.weekly_standalone,
            mix.daily_standalone
        ),
        (3, 0, 2, 1)
    );
    let ctx = planner_lib::ai::agent::context::load_context(&db.conn(), &day.id).unwrap();
    let rendered = planner_lib::ai::agent::context::render(&ctx, None);
    assert!(rendered.contains("<standalone_weekly>2</standalone_weekly>"));
    assert!(rendered.contains("not time spent"));
}

#[test]
fn week_under_ended_long_term_is_rejected() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    cycles::start_cycle(&db.db, &month.id, NOW + 500).unwrap();
    planner_lib::service::cycles::finish_cycle(&db.db, &month.id, NOW + 1000).unwrap();
    let err = cycles::create_planning_cycle(
        &db.db,
        &CreateCycleArgs {
            cycle_type: "week".into(),
            parent_id: Some(month.id.clone()),
            ..Default::default()
        },
        common::today(),
        NOW + 2000,
    )
    .unwrap_err();
    assert_eq!(err_code(&err), "parent_cycle_ended");
}

#[test]
fn duplicate_calendar_key_maps_to_taken() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let err = create_day(&db.db, &week.id, TODAY, NOW).id;
    assert!(!err.is_empty());
    // Creating the same day again through the strict path conflicts…
    let mutation = planner_lib::service::cycles::create_planning_cycle(
        &db.db,
        &CreateCycleArgs {
            cycle_type: "day".into(),
            parent_id: Some(week.id.clone()),
            date: Some(TODAY.into()),
            ..Default::default()
        },
        common::today(),
        NOW + 1,
    );
    let err = mutation.unwrap_err();
    assert_eq!(err_code(&err), "calendar_key_taken");
    // …and the find-or-create path reuses the existing cycle.
    let day = planner_lib::service::cycles::get_or_create_day(&db.db, common::today(), NOW + 2)
        .unwrap()
        .value;
    assert_eq!(day.calendar_key.as_deref(), Some("day:2026-09-16"));
}

#[test]
fn deletion_guard_blocks_past_cycles() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, NOW);
    cycles::start_cycle(&db.db, &day.id, NOW).unwrap();
    cycles::finish_cycle(&db.db, &day.id, NOW + 1).unwrap();
    let err = cycles::delete_cycle(&db.db, &day.id).unwrap_err();
    assert_eq!(err_code(&err), "past_cycle");
}

#[test]
fn deletion_guard_blocks_cycles_with_started_sessions() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, NOW);
    let session = cycles::add_session(
        &db.db,
        &cycles::AddSessionArgs {
            task_id: None,
            day_cycle_id: day.id.clone(),
            title: "running".into(),
            duration_ms: Some(900_000),
            position: None,
        },
        NOW,
    )
    .unwrap()
    .value;
    cycles::start_cycle(&db.db, &session.id, NOW + 10).unwrap();
    let err = cycles::delete_cycle(&db.db, &day.id).unwrap_err();
    assert_eq!(err_code(&err), "has_started_session");
    // The preview names the same guard for the UI.
    let preview = cycles::get_cycle_deletion_preview(&db.db, &day.id).unwrap();
    assert_eq!(preview.guard_code.as_deref(), Some("has_started_session"));
}

#[test]
fn deletion_guard_allows_only_latest_n() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 6);
    let week = create_week(&db.db, &month.id, TODAY);
    // Six distinct days; the oldest is no longer among the latest five.
    for i in 0..6 {
        let date = planner_lib::domain::calendar::format_date(
            planner_lib::domain::calendar::add_days(common::today(), i - 10),
        );
        create_day(&db.db, &week.id, &date, NOW + i);
    }
    let oldest = planner_lib::domain::calendar::format_date(
        planner_lib::domain::calendar::add_days(common::today(), -10),
    );
    let oldest_id: String = {
        let conn = db.conn();
        conn.query_row(
            "SELECT id FROM cycles WHERE calendar_key = ?1",
            rusqlite::params![format!("day:{oldest}")],
            |r| r.get(0),
        )
        .unwrap()
    };
    let err = cycles::delete_cycle(&db.db, &oldest_id).unwrap_err();
    assert_eq!(err_code(&err), "not_latest_n");

    // The most recent day is deletable.
    let newest_date = planner_lib::domain::calendar::format_date(
        planner_lib::domain::calendar::add_days(common::today(), -5),
    );
    let newest_id: String = {
        let conn = db.conn();
        conn.query_row(
            "SELECT id FROM cycles WHERE calendar_key = ?1",
            rusqlite::params![format!("day:{newest_date}")],
            |r| r.get(0),
        )
        .unwrap()
    };
    cycles::delete_cycle(&db.db, &newest_id).unwrap();
}

#[test]
fn later_container_is_protected_from_deletion() {
    let db = TestDb::open();
    let err = cycles::delete_cycle(&db.db, "later").unwrap_err();
    assert_eq!(err_code(&err), "protected_container");
}

#[test]
fn finishing_a_session_accrues_focused_time_up_the_chain() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, NOW);
    let session = cycles::add_session(
        &db.db,
        &cycles::AddSessionArgs {
            task_id: None,
            day_cycle_id: day.id.clone(),
            title: "deep work".into(),
            duration_ms: Some(3_600_000),
            position: None,
        },
        NOW,
    )
    .unwrap()
    .value;
    cycles::start_cycle(&db.db, &session.id, NOW + 1000).unwrap();
    let finished = cycles::finish_cycle(&db.db, &session.id, NOW + 1000 + 25 * 60 * 1000)
        .unwrap()
        .value;

    assert!(finished.finished);
    assert_eq!(finished.focused_time, 25 * 60 * 1000);
    let state = cycles::get_planner_state(&db.db).unwrap();
    let day_after = state.cycles.iter().find(|c| c.id == day.id).unwrap();
    let week_after = state.cycles.iter().find(|c| c.id == week.id).unwrap();
    let month_after = state.cycles.iter().find(|c| c.id == month.id).unwrap();
    assert_eq!(day_after.focused_time, 25 * 60 * 1000);
    assert_eq!(week_after.focused_time, 25 * 60 * 1000);
    assert_eq!(month_after.focused_time, 25 * 60 * 1000);
}

#[test]
fn copy_uncompleted_records_lineage_and_skips_completed() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 6);
    // Two adjacent weeks inside the same month.
    let prev = create_week(&db.db, &month.id, "2026-09-09"); // week of 09-07
    let curr = create_week(&db.db, &month.id, TODAY); // week of 09-14
    assert_eq!(prev.starts_on.as_deref(), Some("2026-09-07"));

    let done = add_task(&db.db, &prev.id, "already finished", NOW);
    tasks::patch_task(
        &db.db,
        &done.id,
        &tasks::TaskPatch {
            completed: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    let open = add_task(&db.db, &prev.id, "carry me over", NOW + 1);
    let child = add_task(&db.db, &prev.id, "sub of carry", NOW + 2);
    // Link child under the open task (same-cycle subtask row).
    tasks::set_task_parent_link(&db.db, &child.id, Some(&open.id)).unwrap();

    let copied = cycles::copy_uncompleted_from_previous(&db.db, &curr.id, NOW + 3)
        .unwrap()
        .value;
    assert_eq!(copied.len(), 2, "only uncompleted tasks are copied");
    let copy_of_open = copied.iter().find(|t| t.title == "carry me over").unwrap();
    assert_eq!(
        copy_of_open.copied_from_task_id.as_deref(),
        Some(open.id.as_str())
    );
    assert!(!copy_of_open.completed);
    let copy_of_child = copied.iter().find(|t| t.title == "sub of carry").unwrap();
    assert_eq!(
        copy_of_child.parent_id.as_deref(),
        Some(copy_of_open.id.as_str()),
        "copied children follow their copied parent"
    );
}

#[test]
fn copy_uncompleted_keeps_the_persistent_empty_row_at_the_end() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let previous_day = create_day(&db.db, &week.id, "2026-09-15", NOW);
    let current_day = create_day(&db.db, &week.id, TODAY, NOW + 1);
    add_task(&db.db, &current_day.id, "already here", NOW + 2);
    let carried = add_task(&db.db, &previous_day.id, "carry me", NOW + 2);
    let carried_child = add_task(&db.db, &previous_day.id, "carry me's child", NOW + 3);
    tasks::set_task_parent_link(&db.db, &carried_child.id, Some(&carried.id)).unwrap();
    let second_carried = add_task(&db.db, &previous_day.id, "another carry", NOW + 4);

    // The frontend creates this real row as soon as the empty day opens.
    add_task(&db.db, &current_day.id, "", NOW + 5);
    let copied = cycles::copy_uncompleted_from_previous(&db.db, &current_day.id, NOW + 4)
        .unwrap()
        .value;

    assert_eq!(copied.len(), 3);
    let copied_carried = copied.iter().find(|task| task.title == carried.title).unwrap();
    assert_eq!(
        copied_carried.copied_from_task_id.as_deref(),
        Some(carried.id.as_str())
    );
    let copied_child = copied
        .iter()
        .find(|task| task.title == carried_child.title)
        .unwrap();
    assert_eq!(copied_child.parent_id.as_deref(), Some(copied_carried.id.as_str()));
    assert!(copied.iter().any(|task| {
        task.title == second_carried.title
            && task.copied_from_task_id.as_deref() == Some(second_carried.id.as_str())
    }));
    let workspace = planner_lib::service::editor::get_editor_workspace(&db.db, &current_day.id)
        .unwrap();
    assert_eq!(
        workspace
            .tasks
            .iter()
            .map(|task| task.task.title.as_str())
            .collect::<Vec<_>>(),
        vec!["already here", "carry me", "another carry", ""],
        "the persistent typing row must remain after copied work"
    );
    assert_eq!(
        workspace.tasks[1].children[0].task.title,
        "carry me's child"
    );
    assert!(workspace.tasks[2].task.position < workspace.tasks[3].task.position);
}

#[test]
fn adding_empty_row_after_copy_appends_after_cross_cycle_roots() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let goal = add_task(&db.db, &month.id, "long-term goal", NOW);
    let week = create_week(&db.db, &month.id, TODAY);
    let previous_day = create_day(&db.db, &week.id, "2026-09-15", NOW);
    let current_day = create_day(&db.db, &week.id, TODAY, NOW + 1);
    let carried = tasks::add_task(
        &db.db,
        &tasks::AddTaskArgs {
            cycle_id: previous_day.id.clone(),
            title: "linked carry".into(),
            position: Some(10),
            ..Default::default()
        },
        NOW + 2,
    )
    .unwrap()
    .value;
    tasks::set_task_parent_link(&db.db, &carried.id, Some(&goal.id)).unwrap();
    let carried_child = tasks::add_task(
        &db.db,
        &tasks::AddTaskArgs {
            cycle_id: previous_day.id.clone(),
            title: "carry step".into(),
            position: Some(100),
            ..Default::default()
        },
        NOW + 3,
    )
    .unwrap()
    .value;
    tasks::set_task_parent_link(&db.db, &carried_child.id, Some(&carried.id)).unwrap();

    cycles::copy_uncompleted_from_previous(&db.db, &current_day.id, NOW + 4).unwrap();
    let empty = add_task(&db.db, &current_day.id, "", NOW + 5);

    assert_eq!(empty.position, 11);
    let workspace = planner_lib::service::editor::get_editor_workspace(&db.db, &current_day.id)
        .unwrap();
    assert_eq!(workspace.tasks.len(), 2);
    assert_eq!(workspace.tasks[0].task.title, "linked carry");
    assert_eq!(workspace.tasks[0].task.position, 10);
    assert_eq!(workspace.tasks[0].children[0].task.title, "carry step");
    assert_eq!(workspace.tasks[0].children[0].task.position, 100);
    assert_eq!(workspace.tasks[1].task.id, empty.id);
}

#[test]
fn copy_uncompleted_preserves_cross_cycle_links_and_does_not_move_proposed_children() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let goal = add_task(&db.db, &month.id, "long-term goal", NOW);
    let week = create_week(&db.db, &month.id, TODAY);
    let previous_day = create_day(&db.db, &week.id, "2026-09-15", NOW + 1);
    let current_day = create_day(&db.db, &week.id, TODAY, NOW + 2);
    let linked = tasks::add_task(
        &db.db,
        &tasks::AddTaskArgs {
            cycle_id: previous_day.id.clone(),
            title: "linked carry".into(),
            position: Some(100),
            ..Default::default()
        },
        NOW + 3,
    )
    .unwrap()
    .value;
    tasks::set_task_parent_link(&db.db, &linked.id, Some(&goal.id)).unwrap();

    let structured_blank = add_task(&db.db, &current_day.id, "", NOW + 4);
    let proposed_child = add_task(&db.db, &current_day.id, "child", NOW + 5);
    tasks::set_task_parent_link(&db.db, &proposed_child.id, Some(&structured_blank.id)).unwrap();
    proposals::apply_update_preview(
        &db.db,
        &proposed_child.id,
        &TaskInput {
            title: "preview child".into(),
            parent_id: Some(structured_blank.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    let input_row = add_task(&db.db, &current_day.id, "", NOW + 6);

    let copied = cycles::copy_uncompleted_from_previous(&db.db, &current_day.id, NOW + 7)
        .unwrap()
        .value;
    assert_eq!(copied.len(), 1);
    assert_eq!(copied[0].parent_id.as_deref(), Some(goal.id.as_str()));

    let workspace = planner_lib::service::editor::get_editor_workspace(&db.db, &current_day.id)
        .unwrap();
    assert_eq!(workspace.tasks.len(), 3);
    assert_eq!(workspace.tasks[0].task.id, structured_blank.id);
    assert_eq!(workspace.tasks[0].children[0].task.id, proposed_child.id);
    assert!(workspace.tasks[0].children[0].task.proposal.is_some());
    assert_eq!(workspace.tasks[2].task.id, input_row.id);
    assert!(workspace.tasks[1].task.position < workspace.tasks[2].task.position);
}

#[test]
fn copy_from_completed_week_returns_empty() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 6);
    let prev = create_week(&db.db, &month.id, "2026-09-09");
    let curr = create_week(&db.db, &month.id, TODAY);
    let task = add_task(&db.db, &prev.id, "done already", NOW);
    tasks::patch_task(
        &db.db,
        &task.id,
        &tasks::TaskPatch {
            completed: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    let copied = cycles::copy_uncompleted_from_previous(&db.db, &curr.id, NOW + 1)
        .unwrap()
        .value;
    assert!(
        copied.is_empty(),
        "nothing uncompleted -> empty result, not an error"
    );
}

// ---------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------

#[test]
fn manual_goals_default_to_needing_clarity() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let goal = add_task(&db.db, &month.id, "Get promoted", NOW);
    assert_eq!(goal.needs_refinement, Some(true), "新目标默认为待澄清");
    let day_task = add_task(&db.db, &week.id, "plain item", NOW);
    assert_eq!(day_task.needs_refinement, None);
}

#[test]
fn cross_level_links_allow_daily_tasks_to_choose_weekly_or_long_term_goals() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, NOW);
    let goal = add_task(&db.db, &month.id, "long-term goal", NOW);
    let weekly = add_task(&db.db, &week.id, "weekly item", NOW);
    let daily = add_task(&db.db, &day.id, "daily task", NOW);

    // weekly -> long-term goal, inside the same branch.
    let linked = tasks::set_task_parent_link(&db.db, &weekly.id, Some(&goal.id))
        .unwrap()
        .value;
    assert_eq!(linked.parent_id.as_deref(), Some(goal.id.as_str()));

    // daily -> weekly item.
    tasks::set_task_parent_link(&db.db, &daily.id, Some(&weekly.id)).unwrap();

    // daily -> long-term goal: direct ownership replaces the weekly link.
    let linked = tasks::set_task_parent_link(&db.db, &daily.id, Some(&goal.id)).unwrap().value;
    assert_eq!(linked.parent_id.as_deref(), Some(goal.id.as_str()));
    assert_eq!(linked.cycle_id, day.id);
    // Reverse ownership still cannot create a cycle in the task graph.
    let err = tasks::set_task_parent_link(&db.db, &goal.id, Some(&daily.id)).unwrap_err();
    assert_eq!(err_code(&err), "invalid_link_level");

    // self link rejected
    let err = tasks::set_task_parent_link(&db.db, &daily.id, Some(&daily.id)).unwrap_err();
    assert_eq!(err_code(&err), "link_to_self");

    // unlink restores a root.
    let unlinked = tasks::set_task_parent_link(&db.db, &weekly.id, None)
        .unwrap()
        .value;
    assert_eq!(unlinked.parent_id, None);
}

#[test]
fn weekly_items_can_link_goals_outside_the_container_parent() {
    let db = TestDb::open();
    let month_a = create_long_term(&db.db, TODAY, 1);
    let month_b = create_long_term(&db.db, "2026-12-09", 1);
    let week_b = create_week(&db.db, &month_b.id, "2026-12-09");
    let goal_a = add_task(&db.db, &month_a.id, "goal of A", NOW);
    let weekly_b = add_task(&db.db, &week_b.id, "weekly of B", NOW);
    let linked = tasks::set_task_parent_link(&db.db, &weekly_b.id, Some(&goal_a.id))
        .unwrap()
        .value;
    assert_eq!(linked.parent_id.as_deref(), Some(goal_a.id.as_str()));
}

#[test]
fn direct_daily_goal_link_persists_without_creating_a_weekly_task() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, "2026-12-09", 1);
    let day = cycles::create_planning_cycle(
        &db.db,
        &CreateCycleArgs { cycle_type: "day".into(), date: Some(TODAY.into()), ..Default::default() },
        common::today(), NOW,
    ).unwrap().value;
    let goal = add_task(&db.db, &month.id, "long-term goal", NOW);
    let daily = add_task(&db.db, &day.id, "daily task", NOW);
    let step = add_task(&db.db, &day.id, "daily step", NOW + 1);
    tasks::set_task_parent_link(&db.db, &step.id, Some(&daily.id)).unwrap();
    tasks::set_task_parent_link(&db.db, &daily.id, Some(&goal.id)).unwrap();

    let workspace = planner_lib::service::editor::get_editor_workspace(&db.db, &day.id).unwrap();
    assert_eq!(workspace.tasks.len(), 1, "cross-cycle ownership keeps the daily task at the visual root");
    assert_eq!(workspace.tasks[0].task.parent_id.as_deref(), Some(goal.id.as_str()));
    assert_eq!(workspace.tasks[0].children[0].task.id, step.id);
    let mix = workspace.work_mix.unwrap();
    assert_eq!(mix.total, 1);
    assert_eq!(mix.long_term, 1);
    let weekly_tasks: i64 = db.conn().query_row(
        "SELECT COUNT(*) FROM tasks JOIN cycles ON tasks.cycle_id = cycles.id WHERE cycles.type = 'week'",
        [], |row| row.get(0),
    ).unwrap();
    assert_eq!(weekly_tasks, 0);

    tasks::set_task_parent_link(&db.db, &daily.id, None).unwrap();
    let unlinked = planner_lib::service::editor::get_editor_workspace(&db.db, &day.id).unwrap();
    assert_eq!(unlinked.tasks[0].task.parent_id, None);
    assert_eq!(unlinked.work_mix.unwrap().daily_standalone, 1);
}

#[test]
fn root_color_on_week_task_is_rejected() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let weekly = add_task(&db.db, &week.id, "weekly item", NOW);
    let err = tasks::set_task_root_color(&db.db, &weekly.id, Some("teal")).unwrap_err();
    assert_eq!(err_code(&err), "root_color_key_requires_long_term_cycle");

    // Long-term goals can be colored, and unknown keys are refused.
    let goal = add_task(&db.db, &month.id, "goal", NOW);
    let colored = tasks::set_task_root_color(&db.db, &goal.id, Some("indigo"))
        .unwrap()
        .value;
    assert_eq!(colored.root_color_key.as_deref(), Some("indigo"));
    let err = tasks::set_task_root_color(&db.db, &goal.id, Some("hot-pink")).unwrap_err();
    assert_eq!(err_code(&err), "unknown_color_key");
}

#[test]
fn moving_into_ended_cycle_is_rejected() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 6);
    let week_a = create_week(&db.db, &month.id, "2026-09-09");
    let week_b = create_week(&db.db, &month.id, TODAY);
    let task = add_task(&db.db, &week_a.id, "movable", NOW);
    cycles::start_cycle(&db.db, &week_b.id, NOW).unwrap();
    cycles::finish_cycle(&db.db, &week_b.id, NOW + 1).unwrap();
    let err = tasks::move_task(&db.db, &task.id, &week_b.id, None).unwrap_err();
    assert_eq!(err_code(&err), "cycle_ended");

    // A legal move into an active week carries same-cycle subtask rows along.
    let week_c = create_week(&db.db, &month.id, "2026-09-23");
    let child = add_task(&db.db, &week_a.id, "child row", NOW + 1);
    tasks::set_task_parent_link(&db.db, &child.id, Some(&task.id)).unwrap();
    let moved = tasks::move_task(&db.db, &task.id, &week_c.id, Some(0))
        .unwrap()
        .value;
    assert_eq!(moved.cycle_id, week_c.id);
    let child_after = planner_lib::repository::tasks::get(&db.conn(), &child.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        child_after.cycle_id, week_c.id,
        "subtree rows follow the move"
    );
}

#[test]
fn planning_cycles_cannot_be_renamed_or_resized() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let err = cycles::update_session(&db.db, &month.id, "New name".into(), None).unwrap_err();
    assert_eq!(err_code(&err), "cycle_immutable");
}

// ---------------------------------------------------------------------------
// Proposals
// ---------------------------------------------------------------------------

fn input(title: &str) -> TaskInput {
    TaskInput {
        title: title.into(),
        ..Default::default()
    }
}

#[test]
fn agent_upsert_lands_in_preview_and_is_hidden_from_visible_queries() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    proposals::apply_upsert_preview(&db.db, &month.id, &input("Agent goal"), NOW + 1000).unwrap();

    let visible =
        planner_lib::repository::tasks::list_visible_by_cycle(&db.conn(), &month.id).unwrap();
    assert!(visible.is_empty(), "preview rows are not committed data");
    let summary = proposals::get_preview_summary(&db.db, &month.id).unwrap();
    assert_eq!(summary.count, 1, "底栏计数为 1");
    assert_eq!(
        summary.tasks[0].proposal,
        Some(planner_lib::domain::proposal::ProposalKind::Upsert)
    );
}

#[test]
fn keep_clears_snapshot_and_commits_the_row() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let staged = proposals::apply_upsert_preview(&db.db, &month.id, &input("Agent goal"), NOW)
        .unwrap()
        .value;
    let kept = proposals::keep_task_preview(&db.db, &staged.id)
        .unwrap()
        .value;
    assert_eq!(kept.proposal, None);
    assert_eq!(kept.title, "Agent goal");
    let conn = db.conn();
    assert_eq!(
        planner_lib::repository::proposals::get_snapshot(&conn, &staged.id).unwrap(),
        None,
        "Keep 清除快照"
    );
    let visible = planner_lib::repository::tasks::list_visible_by_cycle(&conn, &month.id).unwrap();
    assert_eq!(visible.len(), 1);
}

#[test]
fn undo_removes_rows_that_never_existed_and_restores_rows_that_did() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    // Fresh goal: undo deletes it entirely.
    let fresh = proposals::apply_upsert_preview(&db.db, &month.id, &input("brand new"), NOW)
        .unwrap()
        .value;
    proposals::undo_task_preview(&db.db, &fresh.id).unwrap();
    assert!(planner_lib::repository::tasks::get(&db.conn(), &fresh.id)
        .unwrap()
        .is_none());

    // Existing goal: undo restores the exact original content.
    let original = add_task(&db.db, &month.id, "original", NOW + 1);
    proposals::apply_update_preview(&db.db, &original.id, &input("renamed by agent")).unwrap();
    proposals::undo_task_preview(&db.db, &original.id).unwrap();
    let restored = planner_lib::repository::tasks::require(&db.conn(), &original.id).unwrap();
    assert_eq!(restored.title, "original");
    assert_eq!(restored.proposal, None);
}

#[test]
fn delete_preview_keeps_row_revertible_then_keep_removes_it() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let goal = add_task(&db.db, &month.id, "to be deleted", NOW);
    proposals::apply_delete_preview(&db.db, &goal.id).unwrap();

    // Still visible, marked as a delete proposal.
    let staged = planner_lib::repository::tasks::require(&db.conn(), &goal.id).unwrap();
    assert_eq!(
        staged.proposal,
        Some(planner_lib::domain::proposal::ProposalKind::Delete)
    );

    // Undo restores the original row exactly.
    proposals::undo_task_preview(&db.db, &goal.id).unwrap();
    let restored = planner_lib::repository::tasks::require(&db.conn(), &goal.id).unwrap();
    assert_eq!(restored.proposal, None);
    assert_eq!(restored.title, "to be deleted");

    // A second delete preview that is kept removes the row physically
    // (已确认的删除不留墓碑).
    proposals::apply_delete_preview(&db.db, &goal.id).unwrap();
    proposals::keep_task_preview(&db.db, &goal.id).unwrap();
    assert!(planner_lib::repository::tasks::get(&db.conn(), &goal.id)
        .unwrap()
        .is_none());
}

#[test]
fn batch_keep_and_undo_cover_whole_cycle() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    proposals::apply_upsert_preview(&db.db, &month.id, &input("one"), NOW).unwrap();
    proposals::apply_upsert_preview(&db.db, &month.id, &input("two"), NOW + 1).unwrap();
    let kept = proposals::keep_all_previews(&db.db, &month.id)
        .unwrap()
        .value;
    assert_eq!(kept, 2);
    assert_eq!(
        proposals::get_preview_summary(&db.db, &month.id)
            .unwrap()
            .count,
        0
    );

    let delete_target: String = {
        let conn = db.conn();
        conn.query_row("SELECT id FROM tasks WHERE title = 'one'", [], |r| r.get(0))
            .unwrap()
    };
    proposals::apply_delete_preview(&db.db, &delete_target).unwrap();
    let undone = proposals::undo_all_previews(&db.db, &month.id)
        .unwrap()
        .value;
    assert_eq!(undone, 1);
    assert_eq!(
        proposals::get_preview_summary(&db.db, &month.id)
            .unwrap()
            .count,
        0
    );
}

#[test]
fn creating_a_goal_reuses_trailing_empty_row() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    // The user left an empty input row at the end of the list.
    let empty = add_task(&db.db, &month.id, "", NOW);

    let staged = proposals::apply_upsert_preview(&db.db, &month.id, &input("Agent goal"), NOW + 1)
        .unwrap()
        .value;
    assert_eq!(
        staged.id, empty.id,
        "the empty row is replaced, not appended"
    );
    assert_eq!(staged.position, empty.position);
    assert_eq!(staged.title, "Agent goal");

    // Undo all restores the empty input row it replaced.
    proposals::undo_all_previews(&db.db, &month.id).unwrap();
    let restored = planner_lib::repository::tasks::require(&db.conn(), &empty.id).unwrap();
    assert_eq!(restored.title, "");
    assert_eq!(restored.proposal, None);

    // With no empty row at all, the next goal is appended as a new row.
    tasks::delete_task(&db.db, &empty.id).unwrap();
    let appended =
        proposals::apply_upsert_preview(&db.db, &month.id, &input("second goal"), NOW + 2)
            .unwrap()
            .value;
    assert_ne!(appended.id, empty.id);
}

// ---------------------------------------------------------------------------
// Editor workspaces
// ---------------------------------------------------------------------------

#[test]
fn editor_workspace_matches_batch_entry_and_handles_missing_cycles() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let goal = add_task(&db.db, &month.id, "goal", NOW);
    // A child row under the goal (same-cycle hierarchy).
    let child = add_task(&db.db, &month.id, "sub", NOW + 1);
    tasks::set_task_parent_link(&db.db, &child.id, Some(&goal.id)).unwrap();

    let single = planner_lib::service::editor::get_editor_workspace(&db.db, &month.id).unwrap();
    let batch = planner_lib::service::editor::get_editor_workspaces_by_cycle_ids(
        &db.db,
        &[month.id.clone(), "does-not-exist".into()],
    )
    .unwrap();
    assert_eq!(
        batch.get(&month.id).unwrap(),
        &single,
        "batch and single agree"
    );
    assert_eq!(single.tasks.len(), 1, "top-level tree roots only");
    assert_eq!(single.tasks[0].children.len(), 1);
    assert_eq!(single.tasks[0].task.id, goal.id);

    // Unknown or invisible cycles return empty results, not errors.
    let missing = batch.get("does-not-exist").unwrap();
    assert!(missing.cycle.is_none() && missing.tasks.is_empty());
    let direct =
        planner_lib::service::editor::get_editor_workspace(&db.db, "does-not-exist").unwrap();
    assert!(direct.cycle.is_none());
}

#[test]
fn editor_workspace_renders_subtasks_markdown() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let goal = add_task(&db.db, &month.id, "goal", NOW);
    tasks::patch_task(
        &db.db,
        &goal.id,
        &tasks::TaskPatch {
            subtasks: Some(vec![
                planner_lib::domain::task::Subtask::new("- tricky title", false),
                planner_lib::domain::task::Subtask::new("plain", true),
            ]),
            ..Default::default()
        },
    )
    .unwrap();
    let workspace = planner_lib::service::editor::get_editor_workspace(&db.db, &month.id).unwrap();
    let markdown = &workspace.tasks[0].subtasks_markdown;
    assert_eq!(markdown, "- [ ] \\- tricky title\n- [x] plain\n");
    let parsed = planner_lib::domain::task::parse_subtasks_markdown(markdown);
    assert_eq!(parsed[0].title, "- tricky title", "元字符不破坏结构");
}

// ---------------------------------------------------------------------------
// Repeats
// ---------------------------------------------------------------------------

fn make_session(
    db: &TestDb,
    title: &str,
    duration_ms: i64,
    now: i64,
) -> planner_lib::domain::cycle::Cycle {
    let month = create_long_term(&db.db, TODAY, 6);
    let week = create_week(&db.db, &month.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, now);
    planner_lib::service::cycles::add_session(
        &db.db,
        &planner_lib::service::cycles::AddSessionArgs {
            task_id: None,
            day_cycle_id: day.id,
            title: title.into(),
            duration_ms: Some(duration_ms),
            position: None,
        },
        now,
    )
    .unwrap()
    .value
}

#[test]
fn save_as_repeat_links_first_instance() {
    let db = TestDb::open();
    let session = make_session(&db, "Morning review", 1_800_000, NOW);
    let mutation = planner_lib::service::repeats::add_repeat(
        &db.db,
        &planner_lib::service::repeats::AddRepeatArgs {
            session_id: session.id.clone(),
        },
        NOW,
    )
    .unwrap();
    assert_eq!(mutation.value.title, "Morning review");
    assert_eq!(mutation.value.duration, 1_800_000);
    let linked = planner_lib::repository::cycles::get(&db.conn(), &session.id)
        .unwrap()
        .unwrap();
    assert_eq!(
        linked.repeat_id.as_deref(),
        Some(mutation.value.id.as_str())
    );
}

#[test]
fn opening_days_without_active_repeats_keeps_sessions_empty() {
    let db = TestDb::open();
    let today = cycles::get_or_create_day(&db.db, common::today(), NOW)
        .unwrap()
        .value;
    assert!(cycles::list_sessions(&db.db, &today.id).unwrap().is_empty());

    let reopened = cycles::get_or_create_day(&db.db, common::today(), NOW + 1)
        .unwrap()
        .value;
    assert_eq!(reopened.id, today.id);
    assert!(cycles::list_sessions(&db.db, &reopened.id)
        .unwrap()
        .is_empty());

    // A stopped template must not materialize into a later newly opened day.
    let source = cycles::add_session(
        &db.db,
        &cycles::AddSessionArgs {
            task_id: None,
            day_cycle_id: today.id,
            title: "Stopped repeat source".into(),
            duration_ms: Some(1_500_000),
            position: None,
        },
        NOW,
    )
    .unwrap()
    .value;
    let repeat = planner_lib::service::repeats::add_repeat(
        &db.db,
        &planner_lib::service::repeats::AddRepeatArgs {
            session_id: source.id,
        },
        NOW,
    )
    .unwrap()
    .value;
    planner_lib::service::repeats::stop_repeat(&db.db, &repeat.id).unwrap();

    let later_date = planner_lib::domain::calendar::add_days(common::today(), 1);
    let later = cycles::get_or_create_day(&db.db, later_date, NOW + 2)
        .unwrap()
        .value;
    assert!(cycles::list_sessions(&db.db, &later.id).unwrap().is_empty());
}

#[test]
fn next_day_generates_instances_in_template_order() {
    let db = TestDb::open();
    let morning = make_session(&db, "Morning review", 1_800_000, NOW);
    let repeat = planner_lib::service::repeats::add_repeat(
        &db.db,
        &planner_lib::service::repeats::AddRepeatArgs {
            session_id: morning.id.clone(),
        },
        NOW,
    )
    .unwrap()
    .value;

    // "Next day" through the find-or-create path.
    let next_day = planner_lib::domain::calendar::add_days(common::today(), 1);
    let mutation =
        planner_lib::service::cycles::get_or_create_day(&db.db, next_day, NOW + 1).unwrap();
    let day = mutation.value;

    let sessions = planner_lib::repository::cycles::list_children(&db.conn(), &day.id).unwrap();
    assert_eq!(sessions.len(), 1, "the active template materialized once");
    assert_eq!(sessions[0].title, "Morning review");
    assert_eq!(sessions[0].duration, Some(1_800_000));
    assert_eq!(sessions[0].repeat_id.as_deref(), Some(repeat.id.as_str()));

    // Idempotent: re-running the day adds nothing.
    let again = planner_lib::service::repeats::generate_day_instances(&db.db, &day.id, NOW + 2)
        .unwrap()
        .value;
    assert!(again.is_empty());
}

#[test]
fn stop_repeat_archives_unlinks_and_preserves_history() {
    let db = TestDb::open();
    let session = make_session(&db, "Morning review", 1_800_000, NOW);
    let repeat = planner_lib::service::repeats::add_repeat(
        &db.db,
        &planner_lib::service::repeats::AddRepeatArgs {
            session_id: session.id.clone(),
        },
        NOW,
    )
    .unwrap()
    .value;
    let next_day = planner_lib::domain::calendar::add_days(common::today(), 1);
    let day = planner_lib::service::cycles::get_or_create_day(&db.db, next_day, NOW + 1)
        .unwrap()
        .value;

    planner_lib::service::repeats::stop_repeat(&db.db, &repeat.id).unwrap();

    // Template archived; no new instances.
    let stored = planner_lib::repository::repeats::get(&db.conn(), &repeat.id)
        .unwrap()
        .unwrap();
    assert!(stored.archived);

    // History survives: the generated instance keeps title, duration and time.
    let instances = planner_lib::repository::cycles::list_children(&db.conn(), &day.id).unwrap();
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0].title, "Morning review");

    // The link to the source is cleared on every instance (来源关联清空).
    let original = planner_lib::repository::cycles::get(&db.conn(), &session.id)
        .unwrap()
        .unwrap();
    assert_eq!(original.repeat_id, None);
}

#[test]
fn editing_template_never_touches_existing_instances() {
    let db = TestDb::open();
    let session = make_session(&db, "Morning review", 1_800_000, NOW);
    let repeat = planner_lib::service::repeats::add_repeat(
        &db.db,
        &planner_lib::service::repeats::AddRepeatArgs {
            session_id: session.id.clone(),
        },
        NOW,
    )
    .unwrap()
    .value;
    let next_day = planner_lib::domain::calendar::add_days(common::today(), 1);
    let day = planner_lib::service::cycles::get_or_create_day(&db.db, next_day, NOW + 1)
        .unwrap()
        .value;

    planner_lib::service::repeats::update_repeat(
        &db.db,
        &repeat.id,
        &planner_lib::service::repeats::RepeatPatch {
            title: Some("Renamed morning".into()),
            duration: Some(3_600_000),
            position: None,
        },
    )
    .unwrap();

    // The historical instance keeps its original title and duration; only the
    // template changed.
    let instance = &planner_lib::repository::cycles::list_children(&db.conn(), &day.id).unwrap()[0];
    assert_eq!(instance.title, "Morning review");
    assert_eq!(instance.duration, Some(1_800_000));
}

#[test]
fn removing_template_unlinks_but_never_deletes_instances() {
    let db = TestDb::open();
    let session = make_session(&db, "Morning review", 1_800_000, NOW);
    let repeat = planner_lib::service::repeats::add_repeat(
        &db.db,
        &planner_lib::service::repeats::AddRepeatArgs {
            session_id: session.id.clone(),
        },
        NOW,
    )
    .unwrap()
    .value;

    planner_lib::service::repeats::remove_repeat(&db.db, &repeat.id).unwrap();
    assert!(
        planner_lib::repository::repeats::get(&db.conn(), &repeat.id)
            .unwrap()
            .is_none()
    );
    let original = planner_lib::repository::cycles::get(&db.conn(), &session.id)
        .unwrap()
        .unwrap();
    assert_eq!(original.repeat_id, None, "实例保留，来源关联已清空");
}

#[test]
fn deleting_one_days_instance_leaves_the_template_alone() {
    let db = TestDb::open();
    let session = make_session(&db, "Morning review", 1_800_000, NOW);
    let repeat = planner_lib::service::repeats::add_repeat(
        &db.db,
        &planner_lib::service::repeats::AddRepeatArgs {
            session_id: session.id.clone(),
        },
        NOW,
    )
    .unwrap()
    .value;

    // Day 2 gets an instance; the user deletes just that instance.
    let day2 = planner_lib::service::cycles::get_or_create_day(
        &db.db,
        planner_lib::domain::calendar::add_days(common::today(), 1),
        NOW + 1,
    )
    .unwrap()
    .value;
    let instance =
        &planner_lib::repository::cycles::list_children(&db.conn(), &day2.id).unwrap()[0];
    planner_lib::service::cycles::delete_cycle(&db.db, &instance.id).unwrap();
    assert!(
        planner_lib::repository::repeats::get(&db.conn(), &repeat.id)
            .unwrap()
            .is_some(),
        "模板不受影响"
    );

    // Day 3 still materializes normally.
    let day3 = planner_lib::service::cycles::get_or_create_day(
        &db.db,
        planner_lib::domain::calendar::add_days(common::today(), 2),
        NOW + 2,
    )
    .unwrap()
    .value;
    let day3_sessions =
        planner_lib::repository::cycles::list_children(&db.conn(), &day3.id).unwrap();
    assert_eq!(day3_sessions.len(), 1);
    assert_eq!(
        day3_sessions[0].repeat_id.as_deref(),
        Some(repeat.id.as_str())
    );
}

#[test]
fn editing_template_with_undated_instance_fails_explicitly() {
    let db = TestDb::open();
    let session = make_session(&db, "Morning review", 1_800_000, NOW);
    let repeat = planner_lib::service::repeats::add_repeat(
        &db.db,
        &planner_lib::service::repeats::AddRepeatArgs {
            session_id: session.id.clone(),
        },
        NOW,
    )
    .unwrap()
    .value;
    // Break the parent day's date: the "future" scope becomes undecidable.
    db.conn()
        .execute("UPDATE cycles SET starts_on = NULL WHERE type = 'day'", [])
        .unwrap();
    let err = planner_lib::service::repeats::update_repeat(
        &db.db,
        &repeat.id,
        &planner_lib::service::repeats::RepeatPatch {
            duration: Some(60_000),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(err_code(&err), "repeat_future_unknown");
    // And nothing was partially applied.
    let stored = planner_lib::repository::repeats::get(&db.conn(), &repeat.id)
        .unwrap()
        .unwrap();
    assert_eq!(stored.duration, 1_800_000);
}

#[test]
fn repeat_instances_obey_the_same_deletion_guards() {
    let db = TestDb::open();
    let session = make_session(&db, "Morning review", 1_800_000, NOW);
    planner_lib::service::repeats::add_repeat(
        &db.db,
        &planner_lib::service::repeats::AddRepeatArgs {
            session_id: session.id.clone(),
        },
        NOW,
    )
    .unwrap();
    planner_lib::service::cycles::start_cycle(&db.db, &session.id, NOW + 5).unwrap();
    // A started instance blocks deleting its day, template or not.
    let day_id: String = {
        let conn = db.conn();
        conn.query_row(
            "SELECT parent_id FROM cycles WHERE id = ?1",
            rusqlite::params![session.id],
            |r| r.get(0),
        )
        .unwrap()
    };
    let err = planner_lib::service::cycles::delete_cycle(&db.db, &day_id).unwrap_err();
    assert_eq!(err_code(&err), "has_started_session");
}

#[test]
fn coach_preview_locks_only_affected_tasks_and_rejection_restores_the_exact_tree() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let root = add_task(&db.db, &month.id, "Original", NOW);
    let child = add_task(&db.db, &month.id, "Child", NOW + 1);
    tasks::set_task_parent_link(&db.db, &child.id, Some(&root.id)).unwrap();
    let other = add_task(&db.db, &month.id, "Other", NOW + 2);
    let before = planner_lib::repository::tasks::require(&db.conn(), &root.id).unwrap();
    proposals::apply_update_preview(&db.db, &root.id, &input("AI title")).unwrap();
    let editor = planner_lib::service::editor::get_editor_workspace(&db.db, &month.id).unwrap();
    assert_eq!(editor.tasks[0].task.title, "AI title");
    assert_eq!(editor.tasks[0].children[0].task.id, child.id);
    let patch = tasks::TaskPatch {
        title: Some("Manual".into()),
        ..Default::default()
    };
    assert!(tasks::patch_task(&db.db, &root.id, &patch).is_err());
    assert!(tasks::delete_task(&db.db, &root.id).is_err());
    assert!(tasks::set_task_root_color(&db.db, &root.id, Some("blue")).is_err());
    assert!(tasks::move_task(&db.db, &root.id, &month.id, None).is_err());
    tasks::patch_task(&db.db, &other.id, &patch).unwrap();
    proposals::undo_task_preview(&db.db, &root.id).unwrap();
    assert_eq!(
        planner_lib::repository::tasks::require(&db.conn(), &root.id).unwrap(),
        before
    );
    tasks::patch_task(&db.db, &root.id, &patch).unwrap();
    proposals::apply_delete_preview(&db.db, &root.id).unwrap();
    assert!(tasks::patch_task(&db.db, &child.id, &patch).is_err());
    assert!(cycles::delete_cycle(&db.db, &month.id).is_err());
    let editor = planner_lib::service::editor::get_editor_workspace(&db.db, &month.id).unwrap();
    assert_eq!(
        editor.tasks[0].children[0].task.proposal,
        Some(planner_lib::domain::proposal::ProposalKind::Delete)
    );
    assert_eq!(
        proposals::get_preview_summary(&db.db, &month.id)
            .unwrap()
            .deletion_impacts[&root.id],
        vec!["Child"]
    );
    proposals::undo_task_preview(&db.db, &root.id).unwrap();
    tasks::patch_task(&db.db, &child.id, &patch).unwrap();
    proposals::apply_update_preview(&db.db, &root.id, &input("Confirmed title")).unwrap();
    proposals::keep_task_preview(&db.db, &root.id).unwrap();
    tasks::patch_task(&db.db, &root.id, &patch).unwrap();
}

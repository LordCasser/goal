//! Optional focus ownership through creation, accounting, deletion and moves.
mod common;

use common::{add_task, create_day, create_long_term, create_week, TestDb, NOW, TODAY};
use planner_lib::domain::cycle::{Cycle, LATER_CYCLE_ID};
use planner_lib::error::AppError;
use planner_lib::repository::{cycles as cr, reminders as rr, tasks as tr};
use planner_lib::service::{calendar, cycles, editor, proposals, repeats, tasks};

fn fixture(db: &TestDb) -> (Cycle, Cycle, Cycle) {
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, NOW);
    (month, week, day)
}

fn block(db: &TestDb, day: &str, task: Option<&str>) -> Cycle {
    cycles::add_session(
        &db.db,
        &cycles::AddSessionArgs {
            day_cycle_id: day.into(),
            task_id: task.map(Into::into),
            title: "Focus".into(),
            duration_ms: Some(60_000),
            position: None,
        },
        NOW,
    )
    .unwrap()
    .value
}

fn finish(db: &TestDb, id: &str, elapsed: i64) {
    cycles::start_cycle(&db.db, id, NOW).unwrap();
    cycles::finish_cycle(&db.db, id, NOW + elapsed).unwrap();
}

fn focus(db: &TestDb, cycle_id: &str) -> i64 {
    cr::require(&db.conn(), cycle_id).unwrap().focused_time
}

fn task_focus(db: &TestDb, cycle_id: &str, task_id: &str) -> i64 {
    fn find(nodes: &[planner_lib::domain::task::TaskNode], id: &str) -> Option<i64> {
        nodes.iter().find_map(|node| {
            if node.task.id == id {
                Some(node.focused_time)
            } else {
                find(&node.children, id)
            }
        })
    }
    find(
        &editor::get_editor_workspace(&db.db, cycle_id)
            .unwrap()
            .tasks,
        task_id,
    )
    .unwrap()
}

fn code(err: &AppError) -> &str {
    match err {
        AppError::Validation { code, .. } | AppError::Conflict { code, .. } => code,
        AppError::NotFound { .. } => "not_found",
        _ => "other",
    }
}

#[test]
fn migration_keeps_legacy_focus_unlinked_and_preserves_recorded_time() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
    for migration in planner_lib::db::migrations::MIGRATIONS.iter().take(11) {
        conn.execute_batch(migration.sql).unwrap();
    }
    conn.execute_batch(
        "INSERT INTO cycles (id, title, type, focused_time) VALUES ('old-day', 'Day', 'day', 7000);
         INSERT INTO cycles (id, title, type, parent_id, focused_time)
         VALUES ('old-focus', 'Existing focus', 'session', 'old-day', 7000);",
    )
    .unwrap();
    conn.execute_batch(planner_lib::db::migrations::MIGRATIONS[11].sql)
        .unwrap();
    let session = cr::require(&conn, "old-focus").unwrap();
    assert_eq!(session.task_id, None);
    assert_eq!(session.focused_time, 7000);
    assert_eq!(cr::require(&conn, "old-day").unwrap().focused_time, 7000);
}

#[test]
fn linked_and_miscellaneous_blocks_account_once_and_refresh_task_projection() {
    let db = TestDb::open();
    let (month, week, day) = fixture(&db);
    let task = add_task(&db.db, &day.id, "Write", NOW);
    let a = block(&db, &day.id, Some(&task.id));
    let b = block(&db, &day.id, Some(&task.id));
    let misc = block(&db, &day.id, None);
    assert_eq!(a.task_id.as_deref(), Some(task.id.as_str()));
    assert_eq!(task_focus(&db, &day.id, &task.id), 0);
    finish(&db, &a.id, 10_000);
    cycles::start_cycle(&db.db, &b.id, NOW).unwrap();
    let mutation = cycles::finish_cycle(&db.db, &b.id, NOW + 20_000).unwrap();
    assert!(mutation.tasks.ids().contains(&day.id));
    finish(&db, &misc.id, 5_000);
    let err = cycles::finish_cycle(&db.db, &a.id, NOW + 90_000).unwrap_err();
    assert_eq!(code(&err), "cycle_already_finished");
    assert_eq!(task_focus(&db, &day.id, &task.id), 30_000);
    for id in [&day.id, &week.id, &month.id] {
        assert_eq!(focus(&db, id), 35_000);
    }
    let deletion = cycles::delete_cycle(&db.db, &a.id).unwrap();
    assert!(deletion.tasks.ids().contains(&day.id));
    assert_eq!(task_focus(&db, &day.id, &task.id), 20_000);
    assert_eq!(focus(&db, &day.id), 25_000);
}

#[test]
fn association_rejects_invalid_targets_and_schema_rejects_non_session_links() {
    let db = TestDb::open();
    let (_, week, day) = fixture(&db);
    let other_day = create_day(&db.db, &week.id, "2026-09-17", NOW);
    let other = add_task(&db.db, &other_day.id, "Elsewhere", NOW);
    let empty = add_task(&db.db, &day.id, "  ", NOW);
    let weekly = add_task(&db.db, &week.id, "Week", NOW);
    let preview = proposals::apply_upsert_preview(
        &db.db,
        &day.id,
        &proposals::TaskInput {
            title: "Proposed".into(),
            ..Default::default()
        },
        NOW,
    )
    .unwrap()
    .value;
    for (id, expected) in [
        (&other.id, "invalid_focus_task"),
        (&empty.id, "invalid_focus_task"),
        (&weekly.id, "invalid_focus_task"),
        (&preview.id, "invalid_focus_task"),
        (&"missing".to_string(), "not_found"),
    ] {
        let err = cycles::add_session(
            &db.db,
            &cycles::AddSessionArgs {
                day_cycle_id: day.id.clone(),
                task_id: Some(id.clone()),
                title: "invalid".into(),
                ..Default::default()
            },
            NOW,
        )
        .unwrap_err();
        assert_eq!(code(&err), expected);
    }
    assert!(cycles::list_sessions(&db.db, &day.id).unwrap().is_empty());
    let actual = add_task(&db.db, &day.id, "Actual", NOW);
    assert!(db
        .conn()
        .execute(
            "UPDATE cycles SET task_id = ?1 WHERE id = ?2",
            [&actual.id, &day.id]
        )
        .is_err());
    let plain = block(&db, &day.id, None);
    assert!(db
        .conn()
        .execute(
            "UPDATE cycles SET task_id = 'missing' WHERE id = ?1",
            [&plain.id]
        )
        .is_err());
}

#[test]
fn task_cascade_confirmation_covers_focus_and_cleans_ancestors_and_reminders() {
    let db = TestDb::open();
    let (month, week, day) = fixture(&db);
    let root = add_task(&db.db, &month.id, "Goal", NOW);
    let weekly = add_task(&db.db, &week.id, "Weekly", NOW);
    let daily = add_task(&db.db, &day.id, "Daily", NOW);
    tasks::set_task_parent_link(&db.db, &weekly.id, Some(&root.id)).unwrap();
    tasks::set_task_parent_link(&db.db, &daily.id, Some(&weekly.id)).unwrap();
    let done = block(&db, &day.id, Some(&daily.id));
    let misc = block(&db, &day.id, None);
    finish(&db, &done.id, 10_000);
    finish(&db, &misc.id, 3_000);
    let preview = tasks::get_task_deletion_preview(&db.db, &root.id).unwrap();
    assert_eq!(preview.descendant_tasks, 2);
    assert_eq!(preview.total_focus_blocks, 1);
    assert_eq!(preview.started_focus_count, 1);
    let running = block(&db, &day.id, Some(&daily.id));
    cycles::start_cycle(&db.db, &running.id, NOW + 100).unwrap();
    let err = tasks::delete_task_confirmed(&db.db, &root.id, Some(&preview.confirmation_token))
        .unwrap_err();
    assert_eq!(code(&err), "deletion_impact_changed");
    let preview = tasks::get_task_deletion_preview(&db.db, &root.id).unwrap();
    assert_eq!(preview.total_focus_blocks, 2);
    let deletion =
        tasks::delete_task_confirmed(&db.db, &root.id, Some(&preview.confirmation_token)).unwrap();
    for task in [&root, &weekly, &daily] {
        assert!(tr::get(&db.conn(), &task.id).unwrap().is_none());
    }
    for session in [&done, &running] {
        assert!(cr::get(&db.conn(), &session.id).unwrap().is_none());
        assert!(deletion.cycles.ids().contains(&session.id));
        assert!(rr::list(&db.conn(), None, rr::StatusFilter::All)
            .unwrap()
            .iter()
            .all(|r| r.target_id != session.id));
    }
    for id in [&day.id, &week.id, &month.id] {
        assert_eq!(focus(&db, id), 3_000);
        assert!(deletion.cycles.ids().contains(id));
        assert!(deletion.tasks.ids().contains(id));
    }
    assert!(cr::get(&db.conn(), &misc.id).unwrap().is_some());
}

#[test]
fn leaf_with_focus_requires_confirmation_and_finish_invalidates_token() {
    let db = TestDb::open();
    let (_, _, day) = fixture(&db);
    let task = add_task(&db.db, &day.id, "Task", NOW);
    let session = block(&db, &day.id, Some(&task.id));
    let preview = tasks::get_task_deletion_preview(&db.db, &task.id).unwrap();
    let err = tasks::delete_task_confirmed(&db.db, &task.id, None).unwrap_err();
    assert_eq!(code(&err), "deletion_confirmation_required");
    finish(&db, &session.id, 15_000);
    let err = tasks::delete_task_confirmed(&db.db, &task.id, Some(&preview.confirmation_token))
        .unwrap_err();
    assert_eq!(code(&err), "deletion_impact_changed");
    assert_eq!(focus(&db, &day.id), 15_000);
}

#[test]
fn task_move_detaches_same_day_subtree_but_preserves_original_focus() {
    let db = TestDb::open();
    let (_, week, day) = fixture(&db);
    let target = create_day(&db.db, &week.id, "2026-09-17", NOW);
    let root = add_task(&db.db, &day.id, "Task", NOW);
    let child = tasks::add_task(
        &db.db,
        &tasks::AddTaskArgs {
            cycle_id: day.id.clone(),
            parent_id: Some(root.id.clone()),
            title: "Step".into(),
            ..Default::default()
        },
        NOW,
    )
    .unwrap()
    .value;
    let a = block(&db, &day.id, Some(&root.id));
    let b = block(&db, &day.id, Some(&child.id));
    finish(&db, &a.id, 10_000);
    finish(&db, &b.id, 2_000);
    tasks::move_task(&db.db, &root.id, &day.id, None).unwrap();
    assert_eq!(
        cr::require(&db.conn(), &a.id).unwrap().task_id,
        Some(root.id.clone())
    );
    let moved = tasks::move_task(&db.db, &root.id, &target.id, None).unwrap();
    assert!(moved.cycles.ids().contains(&day.id));
    for id in [&a.id, &b.id] {
        let session = cr::require(&db.conn(), id).unwrap();
        assert_eq!(session.task_id, None);
        assert_eq!(session.parent_id.as_deref(), Some(day.id.as_str()));
        assert!(moved.cycles.ids().contains(id));
    }
    assert_eq!(focus(&db, &day.id), 12_000);
    assert_eq!(task_focus(&db, &target.id, &root.id), 0);
    tasks::move_task(&db.db, &root.id, LATER_CYCLE_ID, None).unwrap();
    assert_eq!(focus(&db, &day.id), 12_000);
}

#[test]
fn whole_day_merge_and_swap_keep_links_and_transfer_focus_between_ancestors() {
    let db = TestDb::open();
    let (month, week, day) = fixture(&db);
    let other_month = create_long_term(&db.db, "2026-10-01", 1);
    let other_week = create_week(&db.db, &other_month.id, "2026-10-01");
    let target = create_day(&db.db, &other_week.id, "2026-10-01", NOW);
    let task = add_task(&db.db, &day.id, "Task", NOW);
    let session = block(&db, &day.id, Some(&task.id));
    finish(&db, &session.id, 10_000);
    let misc = block(&db, &target.id, None);
    finish(&db, &misc.id, 3_000);
    calendar::move_day_cycle(
        &db.db,
        &day.id,
        "2026-10-01",
        Some(calendar::MoveStrategy::Swap),
        NOW,
    )
    .unwrap();
    assert_eq!(focus(&db, &month.id), 3_000);
    assert_eq!(focus(&db, &other_month.id), 10_000);
    assert_eq!(
        cr::require(&db.conn(), &session.id).unwrap().task_id,
        Some(task.id.clone())
    );
    calendar::move_day_cycle(
        &db.db,
        &day.id,
        TODAY,
        Some(calendar::MoveStrategy::Merge),
        NOW,
    )
    .unwrap();
    assert!(cr::get(&db.conn(), &day.id).unwrap().is_none());
    let session = cr::require(&db.conn(), &session.id).unwrap();
    assert_eq!(session.parent_id.as_deref(), Some(target.id.as_str()));
    assert_eq!(session.task_id.as_deref(), Some(task.id.as_str()));
    assert_eq!(
        tr::require(&db.conn(), &task.id).unwrap().cycle_id,
        target.id
    );
    assert_eq!(task_focus(&db, &target.id, &task.id), 10_000);
    assert_eq!(focus(&db, &target.id), 13_000);
    assert_eq!(focus(&db, &week.id), 13_000);
    assert_eq!(focus(&db, &month.id), 13_000);
    assert_eq!(focus(&db, &other_week.id), 0);
    assert_eq!(focus(&db, &other_month.id), 0);
}

#[test]
fn repeats_create_unlinked_instances_and_day_move_retains_the_original_link() {
    let db = TestDb::open();
    let (_, _, day) = fixture(&db);
    let task = add_task(&db.db, &day.id, "Task", NOW);
    let session = block(&db, &day.id, Some(&task.id));
    finish(&db, &session.id, 10_000);
    repeats::add_repeat(
        &db.db,
        &repeats::AddRepeatArgs {
            session_id: session.id.clone(),
        },
        NOW,
    )
    .unwrap();
    let moved = calendar::move_day_cycle(&db.db, &day.id, "2026-09-17", None, NOW)
        .unwrap()
        .value;
    assert_eq!(task_focus(&db, &moved.target_day_id, &task.id), 10_000);
    assert_eq!(focus(&db, &moved.target_day_id), 10_000);
    let next = cycles::get_or_create_day(
        &db.db,
        chrono::NaiveDate::from_ymd_opt(2026, 9, 18).unwrap(),
        NOW,
    )
    .unwrap()
    .value;
    let generated = cycles::list_sessions(&db.db, &next.id).unwrap();
    assert_eq!(generated.len(), 1);
    assert_eq!(generated[0].task_id, None);
    assert_eq!(
        cr::require(&db.conn(), &session.id)
            .unwrap()
            .task_id
            .as_deref(),
        Some(task.id.as_str())
    );
}

#[test]
fn confirmed_proposal_deletion_cleans_linked_focus_and_propagates_invalidations() {
    let db = TestDb::open();
    let (month, _, day) = fixture(&db);
    let task = add_task(&db.db, &day.id, "Task", NOW);
    let session = block(&db, &day.id, Some(&task.id));
    finish(&db, &session.id, 10_000);
    proposals::apply_delete_preview(&db.db, &task.id).unwrap();
    let summary = proposals::get_preview_summary(&db.db, &day.id).unwrap();
    assert_eq!(
        summary.deletion_impacts[&task.id],
        vec![session.title.clone()]
    );
    let deletion = proposals::keep_task_preview(&db.db, &task.id).unwrap();
    assert!(deletion.cycles.ids().contains(&day.id));
    assert!(deletion.cycles.ids().contains(&month.id));
    assert!(cr::get(&db.conn(), &session.id).unwrap().is_none());
    assert_eq!(focus(&db, &day.id), 0);
    assert!(rr::list(&db.conn(), None, rr::StatusFilter::All)
        .unwrap()
        .is_empty());
}

#[test]
fn cycle_deletion_follows_linked_daily_tasks_outside_the_cycle_tree() {
    let db = TestDb::open();
    let (month, week, _) = fixture(&db);
    let root = add_task(&db.db, &month.id, "Goal", NOW);
    let weekly = add_task(&db.db, &week.id, "Weekly", NOW);
    tasks::set_task_parent_link(&db.db, &weekly.id, Some(&root.id)).unwrap();
    let day = cycles::get_or_create_day(
        &db.db,
        chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
        NOW,
    )
    .unwrap()
    .value;
    let task = add_task(&db.db, &day.id, "Daily elsewhere", NOW);
    tasks::set_task_parent_link(&db.db, &task.id, Some(&weekly.id)).unwrap();
    let session = block(&db, &day.id, Some(&task.id));
    finish(&db, &session.id, 10_000);
    let preview = cycles::get_cycle_deletion_preview(&db.db, &month.id).unwrap();
    assert_eq!(preview.total_focus_blocks, 1);
    assert_eq!(preview.guard_code, None);
    let deletion =
        cycles::delete_cycle_confirmed(&db.db, &month.id, Some(&preview.confirmation_token))
            .unwrap();
    assert!(cr::get(&db.conn(), &session.id).unwrap().is_none());
    assert_eq!(focus(&db, &day.id), 0);
    assert!(deletion.cycles.ids().contains(&day.id));
}

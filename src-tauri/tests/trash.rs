mod common;

use common::{add_task, create_day, create_long_term, create_week, TestDb, NOW, TODAY};
use planner_lib::{db::{self, backup}, domain::cycle::LATER_CYCLE_ID, repository::{cycles as cycle_repo, tasks as task_repo}, service::{cycles, proposals, tasks, trash}};

#[test]
fn linked_focus_and_reminder_restore_with_accounting() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, NOW);
    let task = add_task(&db.db, &day.id, "Focus task", NOW);
    let session = cycles::add_session(&db.db, &cycles::AddSessionArgs {
        day_cycle_id: day.id.clone(), task_id: Some(task.id.clone()),
        title: "Focused".into(), duration_ms: Some(1_000), position: None,
    }, NOW).unwrap().value;
    // A persisted historical focus contribution and its reminder belong to
    // the deletion unit, even when their containing plans survive.
    db.conn().execute("UPDATE cycles SET focused_time = 500 WHERE id IN (?1, ?2, ?3, ?4)",
        rusqlite::params![session.id, day.id, week.id, month.id]).unwrap();
    db.conn().execute("INSERT INTO reminders (id, target_kind, target_id, fire_at, created_at) VALUES ('r1', 'session', ?1, ?2, ?3)",
        rusqlite::params![session.id, NOW + 1_000, NOW]).unwrap();
    let preview = tasks::get_task_deletion_preview(&db.db, &task.id).unwrap();
    tasks::delete_task_confirmed(&db.db, &task.id, Some(&preview.confirmation_token)).unwrap();
    assert_eq!(cycle_repo::require(&db.conn(), &day.id).unwrap().focused_time, 0);
    let entry = trash::list(&db.db).unwrap().remove(0);
    trash::restore(&db.db, &entry.id).unwrap();
    assert_eq!(cycle_repo::require(&db.conn(), &day.id).unwrap().focused_time, 500);
    assert_eq!(cycle_repo::require(&db.conn(), &week.id).unwrap().focused_time, 500);
    assert_eq!(cycle_repo::require(&db.conn(), &month.id).unwrap().focused_time, 500);
    assert_eq!(cycle_repo::require(&db.conn(), &session.id).unwrap().task_id.as_deref(), Some(task.id.as_str()));
    let count: i64 = db.conn().query_row("SELECT COUNT(*) FROM reminders WHERE id = 'r1'", [], |row| row.get(0)).unwrap();
    assert_eq!(count, 1);
}

#[test]
fn later_and_long_term_goals_return_to_their_original_containers() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let later = add_task(&db.db, LATER_CYCLE_ID, "Later goal", NOW);
    let goal = add_task(&db.db, &month.id, "Long-term goal", NOW + 1);
    tasks::delete_task_confirmed(&db.db, &later.id, None).unwrap();
    tasks::delete_task_confirmed(&db.db, &goal.id, None).unwrap();
    let entries = trash::list(&db.db).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries.iter().find(|e| e.target_id == later.id).unwrap().origin, "Later");
    for entry in entries {
        trash::restore(&db.db, &entry.id).unwrap();
    }
    assert_eq!(task_repo::require(&db.conn(), &later.id).unwrap().cycle_id, LATER_CYCLE_ID);
    assert_eq!(task_repo::require(&db.conn(), &goal.id).unwrap().cycle_id, month.id);
    assert!(trash::list(&db.db).unwrap().is_empty());
}

#[test]
fn deleting_a_plan_restores_its_descendants_and_cross_plan_links() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, NOW);
    let root = add_task(&db.db, &month.id, "Root", NOW);
    let other = create_long_term(&db.db, "2026-10-16", 1);
    let other_week = create_week(&db.db, &other.id, "2026-10-16");
    let child = add_task(&db.db, &other_week.id, "Linked child", NOW + 1);
    tasks::set_task_parent_link(&db.db, &child.id, Some(&root.id)).unwrap();
    let preview = cycles::get_cycle_deletion_preview(&db.db, &month.id).unwrap();
    cycles::delete_cycle_confirmed(&db.db, &month.id, Some(&preview.confirmation_token)).unwrap();
    assert!(cycle_repo::get(&db.conn(), &day.id).unwrap().is_none());
    assert!(task_repo::get(&db.conn(), &child.id).unwrap().is_none());
    let entry = trash::list(&db.db).unwrap().remove(0);
    assert_eq!(entry.kind, "cycle");
    assert_eq!(entry.task_count, 2);
    trash::restore(&db.db, &entry.id).unwrap();
    assert_eq!(cycle_repo::require(&db.conn(), &day.id).unwrap().parent_id.as_deref(), Some(week.id.as_str()));
    assert_eq!(task_repo::require(&db.conn(), &child.id).unwrap().parent_id.as_deref(), Some(root.id.as_str()));
}

#[test]
fn missing_parent_keeps_the_entry_and_batch_delete_is_final() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let goal = add_task(&db.db, &week.id, "Weekly item", NOW);
    tasks::delete_task_confirmed(&db.db, &goal.id, None).unwrap();
    let task_entry = trash::list(&db.db).unwrap().remove(0);
    let preview = cycles::get_cycle_deletion_preview(&db.db, &month.id).unwrap();
    cycles::delete_cycle_confirmed(&db.db, &month.id, Some(&preview.confirmation_token)).unwrap();
    assert!(trash::restore(&db.db, &task_entry.id).is_err());
    assert_eq!(trash::list(&db.db).unwrap().len(), 2);
    let plan_entry = trash::list(&db.db).unwrap().into_iter().find(|entry| entry.kind == "cycle").unwrap();
    trash::restore(&db.db, &plan_entry.id).unwrap();
    trash::restore(&db.db, &task_entry.id).unwrap();
    assert!(task_repo::get(&db.conn(), &goal.id).unwrap().is_some());
    tasks::delete_task_confirmed(&db.db, &goal.id, None).unwrap();
    let ids = trash::list(&db.db).unwrap().iter().map(|entry| entry.id.clone()).collect::<Vec<_>>();
    assert_eq!(trash::delete_permanently(&db.db, &ids).unwrap(), 1);
    assert!(trash::list(&db.db).unwrap().is_empty());
    assert!(task_repo::get(&db.conn(), &goal.id).unwrap().is_none());
}

#[test]
fn stale_batch_selection_does_not_permanently_delete_any_entry() {
    let db = TestDb::open();
    let first = add_task(&db.db, LATER_CYCLE_ID, "First", NOW);
    let second = add_task(&db.db, LATER_CYCLE_ID, "Second", NOW + 1);
    tasks::delete_task_confirmed(&db.db, &first.id, None).unwrap();
    tasks::delete_task_confirmed(&db.db, &second.id, None).unwrap();
    let entries = trash::list(&db.db).unwrap();
    let selection = vec![entries[0].id.clone(), "missing-entry".into()];

    assert!(trash::delete_permanently(&db.db, &selection).is_err());
    assert_eq!(trash::list(&db.db).unwrap().len(), 2);
}

#[test]
fn deleting_a_days_focus_blocks_creates_one_restorable_entry() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let day = create_day(&db.db, &week.id, TODAY, NOW);
    let first = cycles::add_session(&db.db, &cycles::AddSessionArgs {
        day_cycle_id: day.id.clone(), task_id: None,
        title: "First".into(), duration_ms: Some(1_000), position: None,
    }, NOW).unwrap().value;
    let second = cycles::add_session(&db.db, &cycles::AddSessionArgs {
        day_cycle_id: day.id.clone(), task_id: None,
        title: "Second".into(), duration_ms: Some(1_000), position: None,
    }, NOW).unwrap().value;

    cycles::delete_day_focus_blocks(&db.db, &day.id, &[first.id.clone(), second.id.clone()]).unwrap();
    let entries = trash::list(&db.db).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].cycle_count, 2);
    assert!(cycle_repo::get(&db.conn(), &first.id).unwrap().is_none());
    assert!(cycle_repo::get(&db.conn(), &second.id).unwrap().is_none());

    trash::restore(&db.db, &entries[0].id).unwrap();
    assert_eq!(cycle_repo::require(&db.conn(), &first.id).unwrap().parent_id.as_deref(), Some(day.id.as_str()));
    assert_eq!(cycle_repo::require(&db.conn(), &second.id).unwrap().parent_id.as_deref(), Some(day.id.as_str()));
}

#[test]
fn backup_keeps_deleted_items_restorable() {
    let source = TestDb::open();
    let goal = add_task(&source.db, LATER_CYCLE_ID, "Archived in backup", NOW);
    tasks::delete_task_confirmed(&source.db, &goal.id, None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let backup_path = dir.path().join("backup.db");
    backup::export(&source.db, &backup_path).unwrap();

    let restored_backup = db::open_at(&backup_path).unwrap();
    let entries = trash::list(&restored_backup).unwrap();
    assert_eq!(entries.len(), 1);
    trash::restore(&restored_backup, &entries[0].id).unwrap();
    assert_eq!(task_repo::require(&restored_backup.pool().get().unwrap(), &goal.id).unwrap().title, "Archived in backup");
}

#[test]
fn plan_snapshot_restores_related_rows_and_task_fields_exactly() {
    use rusqlite::{params, types::Value, Connection};

    fn rows(conn: &Connection, table: &str) -> Vec<Vec<Value>> {
        let mut statement = conn.prepare(&format!("SELECT * FROM {table} ORDER BY rowid")).unwrap();
        let columns = statement.column_count();
        statement.query_map([], |row| (0..columns).map(|index| row.get(index)).collect::<rusqlite::Result<Vec<Value>>>())
            .unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap()
    }

    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let goal = add_task(&db.db, &month.id, "Annotated goal", NOW);
    let conn = db.conn();
    conn.execute("UPDATE tasks SET note = 'Detailed note', root_color_key = 'orange', position = 7, completed = 1 WHERE id = ?1", [&goal.id]).unwrap();
    conn.execute("INSERT INTO task_preview_originals (task_id, cycle_id, original_exists, title) VALUES (?1, ?2, 1, 'Before preview')",
        params![goal.id, month.id]).unwrap();
    conn.execute("INSERT INTO planning_issue_dismissals (id, cycle_id, issue_type, task_id, reason) VALUES ('dismissal', ?1, 'vague_goal', ?2, 'Already addressed')",
        params![month.id, goal.id]).unwrap();
    conn.execute("INSERT INTO cycle_reviews (id, cycle_id, kind, is_final, facts_json, answers_json, snapshot_at, created_at, updated_at) VALUES ('review', ?1, 'month', 1, '{}', '[]', ?2, ?2, ?2)",
        params![month.id, NOW]).unwrap();
    conn.execute("INSERT INTO cycle_review_dispositions (review_id, task_id, disposition) VALUES ('review', ?1, 'carry')", [&goal.id]).unwrap();
    conn.execute("INSERT INTO reminders (id, target_kind, target_id, fire_at, created_at) VALUES ('task-reminder', 'task', ?1, ?2, ?3)",
        params![goal.id, NOW + 1_000, NOW]).unwrap();
    let tables = ["cycles", "tasks", "task_preview_originals", "planning_issue_dismissals", "cycle_reviews", "cycle_review_dispositions", "reminders"];
    let before: Vec<_> = tables.iter().map(|table| rows(&conn, table)).collect();
    drop(conn);

    let preview = cycles::get_cycle_deletion_preview(&db.db, &month.id).unwrap();
    cycles::delete_cycle_confirmed(&db.db, &month.id, Some(&preview.confirmation_token)).unwrap();
    let entry = trash::list(&db.db).unwrap().remove(0);
    assert_eq!(entry.task_count, 1);
    trash::restore(&db.db, &entry.id).unwrap();
    let conn = db.conn();
    for (index, table) in tables.iter().enumerate() {
        assert_eq!(rows(&conn, table), before[index], "{table} did not round-trip");
    }
}

#[test]
fn occupied_original_day_does_not_consume_the_trash_entry() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let week = create_week(&db.db, &month.id, TODAY);
    let original = create_day(&db.db, &week.id, TODAY, NOW);
    let preview = cycles::get_cycle_deletion_preview(&db.db, &original.id).unwrap();
    cycles::delete_cycle_confirmed(&db.db, &original.id, Some(&preview.confirmation_token)).unwrap();
    let entry = trash::list(&db.db).unwrap().remove(0);
    let occupant = create_day(&db.db, &week.id, TODAY, NOW);

    assert!(trash::restore(&db.db, &entry.id).is_err());
    assert_eq!(trash::list(&db.db).unwrap().len(), 1);
    assert!(cycle_repo::get(&db.conn(), &original.id).unwrap().is_none());
    assert!(cycle_repo::get(&db.conn(), &occupant.id).unwrap().is_some());
}

#[test]
fn coach_delete_enters_trash_only_after_keep() {
    let db = TestDb::open();
    let month = create_long_term(&db.db, TODAY, 1);
    let goal = add_task(&db.db, &month.id, "Coach suggestion", NOW);

    proposals::apply_delete_preview(&db.db, &goal.id).unwrap();
    assert!(trash::list(&db.db).unwrap().is_empty());
    proposals::undo_task_preview(&db.db, &goal.id).unwrap();
    assert!(trash::list(&db.db).unwrap().is_empty());
    assert!(task_repo::get(&db.conn(), &goal.id).unwrap().is_some());

    proposals::apply_delete_preview(&db.db, &goal.id).unwrap();
    proposals::keep_task_preview(&db.db, &goal.id).unwrap();
    let entries = trash::list(&db.db).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].target_id, goal.id);
    trash::restore(&db.db, &entries[0].id).unwrap();
    assert!(task_repo::get(&db.conn(), &goal.id).unwrap().is_some());
}

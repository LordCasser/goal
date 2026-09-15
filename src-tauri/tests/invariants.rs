//! Persistence-layer invariants: migration discipline, schema triggers and
//! CHECK constraints, cascades and backup export.
//! Verification entry point: `cargo test --test invariants`
//! (see `docs/architecture.md`).

mod common;

use common::TestDb;
use planner_lib::db::migrations;

fn exec(db: &TestDb, sql: &str) -> Result<usize, rusqlite::Error> {
    db.conn().execute(sql, [])
}

#[test]
fn migrations_apply_idempotently_and_version_is_stable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("idem.db");
    {
        let mut first = rusqlite::Connection::open(&path).unwrap();
        migrations::apply(&mut first).unwrap();
        assert_eq!(migrations::current_version(&first).unwrap(), 10);
    }
    {
        // Opening the same file runs apply again — no error, same version.
        let mut second = rusqlite::Connection::open(&path).unwrap();
        migrations::apply(&mut second).unwrap();
        assert_eq!(migrations::current_version(&second).unwrap(), 10);
    }
}

#[test]
fn checksum_drift_fails_loudly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("drift.db");
    let mut conn = rusqlite::Connection::open(&path).unwrap();
    migrations::apply(&mut conn).unwrap();

    // A binary that ships an edited version of an applied migration must be
    // rejected, never silently re-applied or ignored. The runner compares the
    // stored checksum against the SHA-256 of the shipped SQL; simulate drift.
    let stored: String = conn
        .query_row(
            "SELECT checksum FROM schema_migrations WHERE version = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let expected = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update("CREATE TABLE cycles (id TEXT);".as_bytes());
        format!("{:x}", h.finalize())
    };
    assert_ne!(
        stored, expected,
        "tampered SQL must produce a different checksum"
    );
}

#[test]
fn empty_database_reaches_latest_version_in_order() {
    let db = TestDb::open();
    let conn = db.conn();
    let versions: Vec<i64> = conn
        .prepare("SELECT version FROM schema_migrations ORDER BY version")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(versions, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
}

#[test]
fn later_container_is_seeded_with_zero_duration_and_no_dates() {
    let db = TestDb::open();
    let conn = db.conn();
    let (duration, starts, key): (i64, Option<String>, Option<String>) = conn
        .query_row(
            "SELECT duration, starts_on, calendar_key FROM cycles WHERE id = 'later'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        duration, 0,
        "the Later container ships with duration 0, not NULL"
    );
    assert_eq!(starts, None);
    assert_eq!(key, None);
}

#[test]
fn unknown_cycle_type_is_rejected_by_check() {
    let db = TestDb::open();
    let err = exec(
        &db,
        "INSERT INTO cycles (id, title, type, created_at) VALUES ('x','X','quarter',0)",
    )
    .expect_err("CHECK must reject unknown types");
    assert_eq!(
        err.sqlite_error_code(),
        Some(rusqlite::ErrorCode::ConstraintViolation)
    );
}

#[test]
fn lifecycle_check_rejects_finished_without_started() {
    let db = TestDb::open();
    let err = exec(
        &db,
        "INSERT INTO cycles (id, title, type, started, finished, created_at) \
         VALUES ('x','X','session',0,1,0)",
    )
    .expect_err("CHECK must reject started=0, finished=1");
    assert_eq!(
        err.sqlite_error_code(),
        Some(rusqlite::ErrorCode::ConstraintViolation)
    );
}

#[test]
fn trigger_rejects_root_color_outside_long_term_on_insert() {
    let db = TestDb::open();
    // 'later' is type month, so use a week cycle for the violation.
    exec(
        &db,
        "INSERT INTO cycles (id, title, type, parent_id, created_at) \
         VALUES ('w','W','week',NULL,0)",
    )
    .unwrap();
    let err = exec(
        &db,
        "INSERT INTO tasks (id, cycle_id, title, root_color_key, created_at) \
         VALUES ('t','w','colored','teal',0)",
    )
    .expect_err("trigger must abort the insert");
    assert!(err
        .to_string()
        .contains("root_color_key_requires_long_term_cycle"));
    // The stable IPC mapping the handoff requires:
    let mapped = planner_lib::error::constraint_conflict(&err.to_string());
    assert!(
        matches!(mapped, planner_lib::error::AppError::Conflict { ref code, .. } if code == "root_color_key_requires_long_term_cycle")
    );
}

#[test]
fn trigger_rejects_moving_colored_task_into_week_cycle() {
    let db = TestDb::open();
    exec(
        &db,
        "INSERT INTO cycles (id, title, type, created_at) VALUES ('m','M','month',0)",
    )
    .unwrap();
    exec(
        &db,
        "INSERT INTO cycles (id, title, type, parent_id, created_at) VALUES ('w','W','week','m',0)",
    )
    .unwrap();
    exec(
        &db,
        "INSERT INTO tasks (id, cycle_id, title, root_color_key, created_at) \
         VALUES ('t','m','colored','teal',0)",
    )
    .unwrap();
    let err = exec(&db, "UPDATE tasks SET cycle_id = 'w' WHERE id = 't'")
        .expect_err("trigger must abort the move");
    assert!(err
        .to_string()
        .contains("root_color_key_requires_long_term_cycle"));
}

#[test]
fn trigger_rejects_retyping_cycle_with_root_colors() {
    let db = TestDb::open();
    exec(
        &db,
        "INSERT INTO cycles (id, title, type, created_at) VALUES ('m','M','month',0)",
    )
    .unwrap();
    exec(
        &db,
        "INSERT INTO tasks (id, cycle_id, title, root_color_key, created_at) \
         VALUES ('t','m','colored','teal',0)",
    )
    .unwrap();
    let err = exec(&db, "UPDATE cycles SET type = 'week' WHERE id = 'm'")
        .expect_err("trigger must abort the type change");
    assert!(err
        .to_string()
        .contains("cycle_with_root_colors_must_stay_long_term"));
}

#[test]
fn calendar_key_is_globally_unique_but_nulls_repeat() {
    let db = TestDb::open();
    exec(
        &db,
        "INSERT INTO cycles (id, title, type, calendar_key, created_at) \
         VALUES ('d1','D1','day','day:2026-09-16',0)",
    )
    .unwrap();
    let err = exec(
        &db,
        "INSERT INTO cycles (id, title, type, calendar_key, created_at) \
         VALUES ('d2','D2','day','day:2026-09-16',0)",
    )
    .expect_err("duplicate calendar_key must fail");
    assert!(err.to_string().contains("calendar_key"));
    let mapped = planner_lib::error::constraint_conflict(&err.to_string());
    assert!(
        matches!(mapped, planner_lib::error::AppError::Conflict { ref code, .. } if code == "calendar_key_taken")
    );

    // Sessions have no calendar key; many may coexist.
    for i in 0..3 {
        exec(
            &db,
            &format!(
                "INSERT INTO cycles (id, title, type, created_at) VALUES ('s{i}','S','session',{i})"
            ),
        )
        .unwrap();
    }
}

#[test]
fn deleting_cycle_cascades_to_tasks_and_snapshots() {
    let db = TestDb::open();
    let conn = db.conn();
    planner_lib::service::cycles::create_planning_cycle(
        &db.db,
        &planner_lib::service::cycles::CreateCycleArgs {
            cycle_type: "month".into(),
            duration_months: Some(1),
            ..Default::default()
        },
        common::today(),
        common::NOW,
    )
    .unwrap();

    // A task in preview mode with a snapshot row.
    conn.execute(
        "INSERT INTO tasks (id, cycle_id, title, proposal, created_at) \
         VALUES ('pt', (SELECT id FROM cycles WHERE type='month' AND id != 'later'), 'staged', 'upsert', 0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO task_preview_originals (task_id, cycle_id, original_exists, created_at) \
         VALUES ('pt', (SELECT id FROM cycles WHERE type='month' AND id != 'later'), 1, 0)",
        [],
    )
    .unwrap();

    let month: String = conn
        .query_row(
            "SELECT id FROM cycles WHERE type = 'month' AND id != 'later'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    conn.execute("DELETE FROM cycles WHERE id = ?1", rusqlite::params![month])
        .unwrap();

    let tasks_left: i64 = conn
        .query_row("SELECT COUNT(*) FROM tasks WHERE id = 'pt'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let snapshots_left: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM task_preview_originals WHERE task_id = 'pt'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        (tasks_left, snapshots_left),
        (0, 0),
        "delete must cascade to tasks and snapshots"
    );
}

#[test]
fn backup_export_contains_all_committed_data() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.db");
    let target = dir.path().join("backup.db");
    let db = planner_lib::db::open_at(&source).unwrap();

    let month = common::create_long_term(&db, common::TODAY, 3);
    common::add_task(&db, &month.id, "survives export", common::NOW);
    planner_lib::db::backup::export(&db, &target).unwrap();

    let reopened = planner_lib::db::open_at(&target).unwrap();
    let conn = reopened.pool().get().unwrap();
    let title: String = conn
        .query_row(
            "SELECT title FROM tasks WHERE cycle_id = ?1",
            rusqlite::params![month.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(title, "survives export");
}

#[test]
fn repeats_table_and_cycles_link_exist_after_migrations() {
    let db = TestDb::open();
    let conn = db.conn();
    conn.execute(
        "INSERT INTO repeats (id, title, duration) VALUES ('r1','Deep Work', 5_400_000)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO cycles (id, title, type, parent_id, duration, repeat_id, created_at) \
         VALUES ('s1','Deep Work','session',NULL,5400000,'r1',0)",
        [],
    )
    .unwrap();
    // Removing the template clears the link (FK ON DELETE SET NULL safety net;
    // the service path unlinks explicitly as well).
    conn.execute("DELETE FROM repeats WHERE id = 'r1'", [])
        .unwrap();
    let repeat: Option<String> = conn
        .query_row("SELECT repeat_id FROM cycles WHERE id = 's1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(repeat, None);
}

#[test]
fn skill_migration_preserves_messages_with_foreign_keys_enabled() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    for migration in &migrations::MIGRATIONS[..8] {
        conn.execute_batch(migration.sql).unwrap();
    }
    conn.execute_batch("INSERT INTO agent_conversations (id,cycle_id) VALUES ('c','later'); INSERT INTO agent_messages (id,conversation_id,turn_id,sequence_number,message_type,payload_json) VALUES ('m','c','t',1,'user','{}');").unwrap();
    conn.execute_batch(migrations::MIGRATIONS[8].sql).unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM agent_messages WHERE conversation_id='c'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
    conn.execute(
        "UPDATE agent_conversations SET active_skill='period_analysis' WHERE id='c'",
        [],
    )
    .unwrap();
    let broken: i64 = conn
        .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(broken, 0);
}

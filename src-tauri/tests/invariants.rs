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
        assert_eq!(migrations::current_version(&first).unwrap(), 14);
    }
    {
        // Opening the same file runs apply again — no error, same version.
        let mut second = rusqlite::Connection::open(&path).unwrap();
        migrations::apply(&mut second).unwrap();
        assert_eq!(migrations::current_version(&second).unwrap(), 14);
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
    assert_eq!(
        versions,
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]
    );
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

#[test]
fn coach_migration_merges_cycles_without_losing_turn_order_or_state() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    for migration in &migrations::MIGRATIONS[..13] {
        conn.execute_batch(migration.sql).unwrap();
    }

    conn.execute(
        "INSERT INTO cycles (id, title, type, parent_id, created_at) \
         VALUES ('c1','Week one','week','later',0), ('c2','Week two','week','later',0)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO agent_conversations \
         (id, cycle_id, active_turn_id, revision, active_skill, last_error, created_at, updated_at) \
         VALUES
           ('old-c1','c1',NULL,2,'daily_planning','old error','100','200'),
           ('old-c2','c2','inflight',4,'review','latest error','150','300')",
        [],
    )
    .unwrap();

    let messages = [
        (
            "m1",
            "old-c1",
            "turn-z",
            1,
            "user",
            "{\"text\":\"first\"}",
            "100",
        ),
        (
            "m2",
            "old-c1",
            "turn-z",
            2,
            "model_function_call",
            "{\"name\":\"inspect\"}",
            "101",
        ),
        (
            "m3",
            "old-c1",
            "turn-ffff",
            3,
            "function_result",
            "{\"ok\":true}",
            "300",
        ),
        (
            "m7",
            "old-c1",
            "turn-0000",
            4,
            "model_text",
            "{\"text\":\"same-time\"}",
            "300",
        ),
        (
            "m4",
            "old-c2",
            "turn-b",
            1,
            "user",
            "{\"text\":\"second\"}",
            "200",
        ),
        (
            "m5",
            "old-c2",
            "turn-b",
            2,
            "model_text",
            "{\"text\":\"reply\"}",
            "201",
        ),
        (
            "m6",
            "old-c2",
            "turn-a",
            3,
            "app_tool_result",
            "{\"applied\":true}",
            "300",
        ),
    ];
    for (id, conversation_id, turn_id, sequence, message_type, payload, created_at) in messages {
        conn.execute(
            "INSERT INTO agent_messages \
             (id, conversation_id, turn_id, sequence_number, message_type, payload_json, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                id,
                conversation_id,
                turn_id,
                sequence,
                message_type,
                payload,
                created_at
            ],
        )
        .unwrap();
    }

    conn.execute_batch(migrations::MIGRATIONS[13].sql).unwrap();

    let state: (String, i64, Option<String>, Option<String>, String, String) = conn
        .query_row(
            "SELECT id, revision, active_skill, last_error, created_at, updated_at \
             FROM agent_conversations",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        state,
        (
            "coach".into(),
            4,
            Some("review".into()),
            Some("latest error".into()),
            "100".into(),
            "300".into()
        )
    );
    assert_eq!(
        conn.query_row::<Option<String>, _, _>(
            "SELECT active_turn_id FROM agent_conversations",
            [],
            |row| row.get(0),
        )
        .unwrap(),
        None
    );

    let migrated: Vec<(String, String, i64, String, String)> = conn
        .prepare(
            "SELECT id, conversation_id, sequence_number, turn_id, payload_json \
             FROM agent_messages ORDER BY sequence_number",
        )
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        migrated,
        vec![
            (
                "m1".into(),
                "coach".into(),
                1,
                "turn-z".into(),
                "{\"text\":\"first\"}".into()
            ),
            (
                "m2".into(),
                "coach".into(),
                2,
                "turn-z".into(),
                "{\"name\":\"inspect\"}".into()
            ),
            (
                "m4".into(),
                "coach".into(),
                3,
                "turn-b".into(),
                "{\"text\":\"second\"}".into()
            ),
            (
                "m5".into(),
                "coach".into(),
                4,
                "turn-b".into(),
                "{\"text\":\"reply\"}".into()
            ),
            (
                "m3".into(),
                "coach".into(),
                5,
                "turn-ffff".into(),
                "{\"ok\":true}".into()
            ),
            (
                "m7".into(),
                "coach".into(),
                6,
                "turn-0000".into(),
                "{\"text\":\"same-time\"}".into()
            ),
            (
                "m6".into(),
                "coach".into(),
                7,
                "turn-a".into(),
                "{\"applied\":true}".into()
            ),
        ]
    );

    conn.execute("DELETE FROM cycles WHERE id = 'c1'", [])
        .unwrap();
    assert_eq!(
        conn.query_row::<i64, _, _>("SELECT COUNT(*) FROM agent_conversations", [], |row| {
            row.get(0)
        })
        .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row::<i64, _, _>("SELECT COUNT(*) FROM agent_messages", [], |row| row.get(0))
            .unwrap(),
        7
    );

    let coach = planner_lib::repository::agent::get_or_create_conversation(&conn, 400).unwrap();
    assert!(planner_lib::repository::agent::claim_turn(&conn, &coach.id, "global-turn").unwrap());
    assert!(!planner_lib::repository::agent::claim_turn(&conn, &coach.id, "another-turn").unwrap());
    planner_lib::repository::agent::release_turn(&conn, &coach.id).unwrap();

    let invalid = conn.execute("INSERT INTO agent_conversations (id) VALUES ('other')", []);
    assert!(
        invalid.is_err(),
        "the global conversation must stay a singleton"
    );
    let invalid_null = conn.execute("INSERT INTO agent_conversations (id) VALUES (NULL)", []);
    assert!(
        invalid_null.is_err(),
        "the singleton id must not be nullable"
    );
}

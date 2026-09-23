//! Versioned migration runner.
//!
//! Contract (see `openspec/specs/local-persistence/spec.md`):
//! * migrations run in ascending version order, each inside its own transaction
//! * applied migrations are recorded with a SHA-256 checksum of their SQL
//! * a migration that is already applied is never re-executed
//! * if the checksum of an applied migration no longer matches, boot fails loudly
//! * a migration that fails is not recorded, so the next boot retries it

use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

pub struct Migration {
    pub version: i64,
    pub description: &'static str,
    pub sql: &'static str,
}

/// Ordered migration list. Append only; never edit an applied migration's SQL.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "init",
        sql: M0001_INIT,
    },
    Migration {
        version: 2,
        description: "invariants",
        sql: M0002_INVARIANTS,
    },
    Migration {
        version: 3,
        description: "seed later container",
        sql: M0003_SEED_LATER,
    },
    Migration {
        version: 4,
        description: "repeats",
        sql: M0004_REPEATS,
    },
    Migration {
        version: 5,
        description: "ai planning",
        sql: M0005_AI_PLANNING,
    },
    Migration {
        version: 6,
        description: "cycle reviews",
        sql: M0006_CYCLE_REVIEWS,
    },
    Migration {
        version: 7,
        description: "reminders",
        sql: M0007_REMINDERS,
    },
    Migration {
        version: 8,
        description: "session schedule",
        sql: M0008_SESSION_SCHEDULE,
    },
    Migration {
        version: 9,
        description: "agent skill routing",
        sql: M0009_AGENT_SKILLS,
    },
    Migration {
        version: 10,
        description: "human-approved app actions",
        sql: M0010_AGENT_ACTIONS,
    },
    Migration {
        version: 11,
        description: "long-term progress check schedule",
        sql: M0011_PROGRESS_CHECK,
    },
    Migration {
        version: 12,
        description: "optional focus task association",
        sql: M0012_FOCUS_TASK_ASSOCIATION,
    },
    Migration {
        version: 13,
        description: "preserve Later plan type",
        sql: M0013_LATER_PLAN_TYPE,
    },
    Migration {
        version: 14,
        description: "global Coach conversation",
        sql: M0014_GLOBAL_COACH_CONVERSATION,
    },
    Migration {
        version: 15,
        description: "task notes",
        sql: M0015_TASK_NOTES,
    },
    Migration {
        version: 16,
        description: "recycle bin",
        sql: M0016_TRASH,
    },
    Migration {
        version: 17,
        description: "open-ended long-term progress checks",
        sql: M0017_OPEN_ENDED_LONG_TERM,
    },
];

const M0017_OPEN_ENDED_LONG_TERM: &str = r#"
ALTER TABLE cycles ADD COLUMN progress_check_next TEXT CHECK (
    progress_check_next IS NULL OR COALESCE((
        type = 'month' AND starts_on IS NOT NULL
        AND (ends_on IS NULL OR ends_on > starts_on)
        AND json_valid(progress_check_next)
        AND (
            (json_extract(progress_check_next, '$.kind') = 'once'
             AND json_type(progress_check_next, '$.date') = 'text'
             AND length(json_extract(progress_check_next, '$.date')) = 10
             AND date(json_extract(progress_check_next, '$.date'), '+0 days') = json_extract(progress_check_next, '$.date')
             AND json_extract(progress_check_next, '$.date') >= starts_on
             AND (ends_on IS NULL OR json_extract(progress_check_next, '$.date') < ends_on)
             AND json_type(progress_check_next, '$.every_days') IS NULL)
            OR
            (json_extract(progress_check_next, '$.kind') = 'repeat'
             AND json_type(progress_check_next, '$.every_days') = 'integer'
             AND json_extract(progress_check_next, '$.every_days') > 0
             AND json_type(progress_check_next, '$.date') IS NULL)
        )
    ), 0)
);
UPDATE cycles SET progress_check_next = progress_check WHERE progress_check IS NOT NULL;
ALTER TABLE cycles DROP COLUMN progress_check;
ALTER TABLE cycles RENAME COLUMN progress_check_next TO progress_check;
"#;

const M0016_TRASH: &str = r#"
CREATE TABLE trash_entries (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('task', 'cycle')),
    target_id TEXT NOT NULL,
    title TEXT NOT NULL,
    origin TEXT NOT NULL,
    deleted_at INTEGER NOT NULL,
    task_count INTEGER NOT NULL,
    cycle_count INTEGER NOT NULL,
    snapshot_json TEXT NOT NULL
);
CREATE INDEX ix_trash_deleted_at ON trash_entries(deleted_at DESC);
"#;

const M0015_TASK_NOTES: &str = r#"
ALTER TABLE tasks ADD COLUMN note TEXT NOT NULL DEFAULT '';
"#;

const M0013_LATER_PLAN_TYPE: &str = r#"
ALTER TABLE tasks ADD COLUMN later_plan_type TEXT CHECK (
    later_plan_type IS NULL OR (
        cycle_id = 'later' AND later_plan_type IN ('month', 'week', 'day')
    )
);
"#;

const M0014_GLOBAL_COACH_CONVERSATION: &str = r#"
-- Coach history is global. Keep the old conversations and messages while
-- rebuilding the cycle-owned tables so deleting a planning cycle cannot
-- cascade through the conversation history.
CREATE TEMP TABLE saved_agent_messages AS
SELECT
    id,
    conversation_id AS old_conversation_id,
    turn_id,
    sequence_number AS old_sequence_number,
    message_type,
    payload_json,
    created_at,
    MIN(created_at) OVER (PARTITION BY conversation_id, turn_id) AS turn_created_at,
    MIN(sequence_number) OVER (PARTITION BY conversation_id, turn_id) AS turn_first_sequence
FROM agent_messages;

CREATE TABLE agent_conversations_new (
    id             TEXT PRIMARY KEY NOT NULL CHECK (id = 'coach'),
    active_turn_id TEXT,
    revision       INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0),
    active_skill   TEXT CHECK (active_skill IS NULL OR active_skill IN
                     ('goal_setting','long_term_planning','short_term_planning','weekly_planning','daily_planning','prioritization','review','period_analysis','planning_issues')),
    last_error     TEXT,
    created_at     TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at     TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO agent_conversations_new
    (id, active_turn_id, revision, active_skill, last_error, created_at, updated_at)
SELECT
    'coach',
    NULL,
    COALESCE(MAX(revision), 0),
    (SELECT active_skill FROM agent_conversations ORDER BY updated_at DESC, id DESC LIMIT 1),
    (SELECT last_error FROM agent_conversations ORDER BY updated_at DESC, id DESC LIMIT 1),
    MIN(created_at),
    MAX(updated_at)
FROM agent_conversations
HAVING COUNT(*) > 0;

DROP TABLE agent_messages;
DROP TABLE agent_conversations;
ALTER TABLE agent_conversations_new RENAME TO agent_conversations;

CREATE INDEX ix_agent_conversations_updated ON agent_conversations(updated_at);

CREATE TABLE agent_messages (
    id              TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES agent_conversations(id) ON DELETE CASCADE,
    turn_id         TEXT NOT NULL,
    sequence_number INTEGER NOT NULL CHECK (sequence_number > 0),
    message_type    TEXT NOT NULL CHECK (message_type IN
                      ('user','model_text','model_function_call','function_result','app_tool_result')),
    payload_json    TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (conversation_id, sequence_number)
);

INSERT INTO agent_messages
    (id, conversation_id, turn_id, sequence_number, message_type, payload_json, created_at)
SELECT
    id,
    'coach',
    turn_id,
    ROW_NUMBER() OVER (
        ORDER BY turn_created_at ASC, old_conversation_id ASC,
                 turn_first_sequence ASC, turn_id ASC, old_sequence_number ASC
    ),
    message_type,
    payload_json,
    created_at
FROM saved_agent_messages
ORDER BY turn_created_at ASC, old_conversation_id ASC,
         turn_first_sequence ASC, turn_id ASC, old_sequence_number ASC;

CREATE INDEX ix_agent_messages_seq ON agent_messages(conversation_id, sequence_number);
DROP TABLE saved_agent_messages;
"#;

const M0012_FOCUS_TASK_ASSOCIATION: &str = r#"
ALTER TABLE cycles ADD COLUMN task_id TEXT REFERENCES tasks(id) ON DELETE CASCADE
    CHECK (task_id IS NULL OR type = 'session');
CREATE INDEX ix_cycles_task ON cycles(task_id) WHERE task_id IS NOT NULL;
"#;

const M0011_PROGRESS_CHECK: &str = r#"
ALTER TABLE cycles ADD COLUMN progress_check TEXT CHECK (
    progress_check IS NULL OR COALESCE((
        type = 'month' AND starts_on IS NOT NULL AND ends_on > starts_on
        AND json_valid(progress_check)
        AND (
            (json_extract(progress_check, '$.kind') = 'once'
             AND json_type(progress_check, '$.date') = 'text'
             AND length(json_extract(progress_check, '$.date')) = 10
             AND date(json_extract(progress_check, '$.date'), '+0 days') = json_extract(progress_check, '$.date')
             AND json_extract(progress_check, '$.date') >= starts_on
             AND json_extract(progress_check, '$.date') < ends_on
             AND json_type(progress_check, '$.every_days') IS NULL)
            OR
            (json_extract(progress_check, '$.kind') = 'repeat'
             AND json_type(progress_check, '$.every_days') = 'integer'
             AND json_extract(progress_check, '$.every_days') > 0
             AND json_type(progress_check, '$.date') IS NULL)
        )
    ), 0)
);
"#;

const M0010_AGENT_ACTIONS: &str = r#"
CREATE TABLE agent_actions (
    id TEXT PRIMARY KEY,
    -- The receipt must survive even if the approved action deletes its source cycle.
    source_cycle_id TEXT NOT NULL,
    action_json TEXT NOT NULL,
    rationale TEXT NOT NULL,
    summary TEXT NOT NULL,
    details_json TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending','applying','applied','rejected')),
    created_at INTEGER NOT NULL
);
CREATE INDEX ix_agent_actions_pending ON agent_actions(source_cycle_id, state);
"#;

fn checksum(sql: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(sql.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn ensure_ledger(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version      INTEGER PRIMARY KEY,
             description  TEXT NOT NULL,
             checksum     TEXT NOT NULL,
             applied_at   INTEGER NOT NULL
         );",
    )
    .map_err(|e| AppError::Db(e.to_string()))
}

/// Highest applied migration version, or 0 for an empty database.
pub fn current_version(conn: &Connection) -> AppResult<i64> {
    ensure_ledger(conn)?;
    conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |r| r.get(0),
    )
    .map_err(|e| AppError::Db(e.to_string()))
}

/// Applies every pending migration. Idempotent.
pub fn apply(conn: &mut Connection) -> AppResult<()> {
    ensure_ledger(conn)?;
    let mut applied: Vec<(i64, String)> = conn
        .prepare("SELECT version, checksum FROM schema_migrations")
        .map_err(|e| AppError::Db(e.to_string()))?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| AppError::Db(e.to_string()))?
        .collect::<Result<_, _>>()
        .map_err(|e| AppError::Db(e.to_string()))?;

    for Migration {
        version,
        description,
        sql,
    } in MIGRATIONS
    {
        let expected = checksum(sql);
        if let Some((_, found)) = applied.iter().find(|(v, _)| v == version) {
            if found != &expected {
                return Err(AppError::Internal(format!(
                    "migration {version} ({description}) was already applied with a different \
                     checksum; the database and the binary disagree"
                )));
            }
            continue;
        }

        let tx = conn
            .transaction()
            .map_err(|e| AppError::Db(e.to_string()))?;
        tx.execute_batch(sql).map_err(|e| {
            AppError::Db(format!("migration {version} ({description}) failed: {e}"))
        })?;
        tx.execute(
            "INSERT INTO schema_migrations (version, description, checksum, applied_at)
             VALUES (?1, ?2, ?3, unixepoch() * 1000)",
            rusqlite::params![version, description, expected],
        )
        .map_err(|e| AppError::Db(e.to_string()))?;
        tx.commit().map_err(|e| AppError::Db(e.to_string()))?;
        applied.push((*version, expected));
    }
    Ok(())
}

const M0001_INIT: &str = r#"
CREATE TABLE cycles (
    id           TEXT PRIMARY KEY,
    title        TEXT NOT NULL,
    type         TEXT NOT NULL CHECK (type IN ('session','day','week','month')),
    parent_id    TEXT REFERENCES cycles(id) ON DELETE CASCADE,
    position     INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
    archived     INTEGER NOT NULL DEFAULT 0,
    started      INTEGER NOT NULL DEFAULT 0,
    finished     INTEGER NOT NULL DEFAULT 0,
    started_at   INTEGER,
    finished_at  INTEGER,
    duration     INTEGER CHECK (duration IS NULL OR duration >= 0),
    focused_time INTEGER NOT NULL DEFAULT 0 CHECK (focused_time >= 0),
    starts_on    TEXT,
    ends_on      TEXT,
    calendar_key TEXT,
    created_at   INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
    CHECK (NOT (started = 0 AND finished = 1))
);

CREATE UNIQUE INDEX ux_cycles_calendar_key
    ON cycles(calendar_key) WHERE calendar_key IS NOT NULL;
CREATE INDEX ix_cycles_parent ON cycles(parent_id);
CREATE INDEX ix_cycles_type ON cycles(type);

CREATE TABLE tasks (
    id                   TEXT PRIMARY KEY,
    cycle_id             TEXT NOT NULL REFERENCES cycles(id) ON DELETE CASCADE,
    parent_id            TEXT REFERENCES tasks(id) ON DELETE CASCADE,
    title                TEXT NOT NULL,
    subtasks             TEXT NOT NULL DEFAULT '[]',
    position             INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
    completed            INTEGER NOT NULL DEFAULT 0,
    goal_breakdown       TEXT,
    needs_refinement     INTEGER,
    needs_breakdown      INTEGER,
    root_color_key       TEXT,
    copied_from_task_id  TEXT,
    proposal             TEXT CHECK (proposal IN ('upsert','delete')),
    created_at           INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);

CREATE INDEX ix_tasks_cycle_visible ON tasks(cycle_id, proposal, position);
CREATE INDEX ix_tasks_parent ON tasks(parent_id);
CREATE INDEX ix_tasks_root_color ON tasks(cycle_id, root_color_key);

CREATE TABLE task_preview_originals (
    task_id          TEXT PRIMARY KEY REFERENCES tasks(id) ON DELETE CASCADE,
    cycle_id         TEXT NOT NULL REFERENCES cycles(id) ON DELETE CASCADE,
    original_exists  INTEGER NOT NULL,
    title            TEXT,
    completed        INTEGER,
    subtasks         TEXT,
    position         INTEGER,
    goal_breakdown   TEXT,
    parent_id        TEXT,
    root_color_key   TEXT,
    needs_refinement INTEGER,
    needs_breakdown  INTEGER,
    created_at       INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
);

CREATE INDEX ix_preview_originals_cycle ON task_preview_originals(cycle_id);

CREATE TABLE agent_conversations (
    id             TEXT PRIMARY KEY,
    cycle_id       TEXT NOT NULL REFERENCES cycles(id) ON DELETE CASCADE,
    active_turn_id TEXT,
    revision       INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0),
    active_skill   TEXT CHECK (active_skill IS NULL OR active_skill IN
                     ('goal_setting','long_term_planning','short_term_planning','prioritization')),
    last_error     TEXT,
    created_at     TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at     TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX ix_agent_conversations_updated ON agent_conversations(updated_at);

CREATE TABLE agent_messages (
    id              TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL REFERENCES agent_conversations(id) ON DELETE CASCADE,
    turn_id         TEXT NOT NULL,
    sequence_number INTEGER NOT NULL CHECK (sequence_number > 0),
    message_type    TEXT NOT NULL CHECK (message_type IN
                      ('user','model_text','model_function_call','function_result','app_tool_result')),
    payload_json    TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (conversation_id, sequence_number)
);

CREATE INDEX ix_agent_messages_seq ON agent_messages(conversation_id, sequence_number);

CREATE TABLE app_settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

const M0002_INVARIANTS: &str = r#"
CREATE TRIGGER reject_root_color_outside_long_term_insert
BEFORE INSERT ON tasks
FOR EACH ROW
WHEN NEW.root_color_key IS NOT NULL
     AND NOT EXISTS (SELECT 1 FROM cycles WHERE id = NEW.cycle_id AND type = 'month')
BEGIN
    SELECT RAISE(ABORT, 'root_color_key_requires_long_term_cycle');
END;

CREATE TRIGGER reject_root_color_outside_long_term_update
BEFORE UPDATE OF cycle_id, root_color_key ON tasks
FOR EACH ROW
WHEN NEW.root_color_key IS NOT NULL
     AND NOT EXISTS (SELECT 1 FROM cycles WHERE id = NEW.cycle_id AND type = 'month')
BEGIN
    SELECT RAISE(ABORT, 'root_color_key_requires_long_term_cycle');
END;

CREATE TRIGGER reject_cycle_type_change_with_root_colors
BEFORE UPDATE OF type ON cycles
FOR EACH ROW
WHEN NEW.type <> 'month'
     AND EXISTS (SELECT 1 FROM tasks WHERE cycle_id = NEW.id AND root_color_key IS NOT NULL)
BEGIN
    SELECT RAISE(ABORT, 'cycle_with_root_colors_must_stay_long_term');
END;
"#;

const M0003_SEED_LATER: &str = r#"
INSERT INTO cycles (id, title, type, position, duration)
VALUES ('later', 'Later', 'month', 0, NULL);
"#;

// Repeats were designed with the rest of the schema but land in their own
// migration so the baseline stays append-only. `cycles.repeat_id` uses
// ON DELETE SET NULL as a last-resort net; the service layer still unlinks
// explicitly before removing a template (spec: 实例与模板的关联).
const M0004_REPEATS: &str = r#"
CREATE TABLE repeats (
    id        TEXT PRIMARY KEY,
    title     TEXT NOT NULL,
    duration  INTEGER NOT NULL CHECK (duration >= 0),
    position  INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0),
    archived  INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX ix_repeats_active ON repeats(archived, position);

ALTER TABLE cycles ADD COLUMN repeat_id TEXT REFERENCES repeats(id) ON DELETE SET NULL;
CREATE INDEX ix_cycles_repeat ON cycles(repeat_id);

-- The Later container is seeded with duration 0, not NULL, matching the
-- upstream seed data (see docs/architecture.md, "Do Later 容器").
UPDATE cycles SET duration = 0 WHERE id = 'later' AND duration IS NULL;
"#;

// AI planning core (change: add-ai-planning-core, tasks §7.1 and §8.4).
//
// `cycles.prioritization_breakdown` stores the five-bucket prioritization
// conclusion as one JSON document (design D6): it is always read and written
// as a whole, and NULL means "not prioritized yet". Structure is validated on
// the Rust side, not by SQL.
//
// `planning_issue_dismissals` records dismissed review issues. `issue_type`
// is one of the six review issue types, validated in the domain layer — no
// SQL CHECK, so future issue types stay forward-compatible. `task_id` NULL
// means a cycle-level dismissal. SQLite treats NULLs as mutually distinct in
// UNIQUE indexes, so the table constraint below only enforces uniqueness for
// rows with a non-NULL `task_id`; cycle-level uniqueness (NULL `task_id`) is
// guaranteed by the service layer, which keeps at most one row per
// (cycle_id, issue_type) pair.
const M0005_AI_PLANNING: &str = r#"
ALTER TABLE cycles ADD COLUMN prioritization_breakdown TEXT;  -- JSON, NULL = not prioritized

CREATE TABLE planning_issue_dismissals (
    id         TEXT PRIMARY KEY,
    cycle_id   TEXT NOT NULL REFERENCES cycles(id) ON DELETE CASCADE,
    issue_type TEXT NOT NULL,
    task_id    TEXT REFERENCES tasks(id) ON DELETE CASCADE,  -- NULL = cycle-level dismissal
    reason     TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE (cycle_id, issue_type, task_id)
);

CREATE INDEX ix_dismissals_cycle ON planning_issue_dismissals(cycle_id);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recycle_bin_schema_is_created_for_fresh_and_existing_databases() {
        let mut fresh = Connection::open_in_memory().expect("open fresh");
        apply(&mut fresh).expect("migrate fresh");
        assert!(table_exists(&fresh, "trash_entries"));
        assert!(columns(&fresh, "trash_entries").contains(&"snapshot_json".to_string()));
        assert_eq!(fresh.query_row::<i64, _, _>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'ix_trash_deleted_at'",
            [], |row| row.get(0)).expect("index"), 1);
        assert!(fresh.execute(
            "INSERT INTO trash_entries (id, kind, target_id, title, origin, deleted_at, task_count, cycle_count, snapshot_json)
             VALUES ('bad', 'other', 't', 'Title', 'Later', 1, 0, 0, '{}')", [],
        ).is_err());

        let mut existing = Connection::open_in_memory().expect("open existing");
        apply_up_to(&mut existing, 15);
        existing.execute("INSERT INTO cycles (id, title, type) VALUES ('d1', 'Day', 'day')", [])
            .expect("existing cycle");
        existing.execute("INSERT INTO tasks (id, cycle_id, title) VALUES ('t1', 'd1', 'Existing')", [])
            .expect("existing task");
        apply(&mut existing).expect("upgrade");
        assert!(table_exists(&existing, "trash_entries"));
        assert_eq!(existing.query_row::<String, _, _>(
            "SELECT title FROM tasks WHERE id = 't1'", [], |row| row.get(0)).expect("preserved task"), "Existing");
    }

    #[test]
    fn open_ended_progress_check_migration_preserves_existing_rules() {
        let mut conn = Connection::open_in_memory().expect("open");
        apply_up_to(&mut conn, 16);
        conn.execute(
            "INSERT INTO cycles (id, title, type, starts_on, ends_on, duration, progress_check)
             VALUES ('fixed', 'Fixed', 'month', '2026-09-23', '2026-12-16', 7257600000,
                     '{\"kind\":\"once\",\"date\":\"2026-11-04\"}')",
            [],
        ).expect("existing rule");

        apply(&mut conn).expect("upgrade");
        let saved: String = conn.query_row(
            "SELECT progress_check FROM cycles WHERE id='fixed'", [], |row| row.get(0),
        ).expect("saved rule");
        assert_eq!(saved, r#"{"kind":"once","date":"2026-11-04"}"#);

        conn.execute(
            "INSERT INTO cycles (id, title, type, starts_on, progress_check)
             VALUES ('open', 'Open', 'month', '2026-09-23', '{\"kind\":\"repeat\",\"every_days\":14}')",
            [],
        ).expect("open repeat");
        assert!(conn.execute(
            "UPDATE cycles SET progress_check='{\"kind\":\"once\",\"date\":\"2026-09-22\"}' WHERE id='open'", [],
        ).is_err());
        assert!(conn.execute(
            "UPDATE cycles SET progress_check='{\"kind\":\"repeat\",\"every_days\":0}' WHERE id='open'", [],
        ).is_err());
        assert!(conn.execute(
            "UPDATE cycles SET progress_check='{\"kind\":\"once\",\"date\":\"2026-12-16\"}' WHERE id='fixed'", [],
        ).is_err());
    }

    #[test]
    fn existing_tasks_gain_empty_notes() {
        let mut conn = Connection::open_in_memory().expect("open");
        conn.execute_batch("PRAGMA foreign_keys = ON").expect("foreign keys");
        apply_up_to(&mut conn, 14);
        conn.execute("INSERT INTO cycles (id, title, type) VALUES ('d1', 'Day', 'day')", [])
            .expect("cycle");
        conn.execute(
            "INSERT INTO tasks (id, cycle_id, title) VALUES ('t1', 'd1', 'Existing')",
            [],
        )
        .expect("task");

        apply(&mut conn).expect("upgrade");
        let note: String = conn
            .query_row("SELECT note FROM tasks WHERE id = 't1'", [], |row| row.get(0))
            .expect("note");
        assert!(note.is_empty());
        assert!(columns(&conn, "tasks").contains(&"note".to_string()));
    }

    /// Applies only migrations up to `max_version`, the state a database
    /// created by an older binary is in (the runner itself has no "stop at"
    /// knob and must not grow one).
    fn apply_up_to(conn: &mut Connection, max_version: i64) {
        ensure_ledger(conn).expect("ledger");
        for migration in MIGRATIONS.iter().filter(|m| m.version <= max_version) {
            let tx = conn.transaction().expect("transaction");
            tx.execute_batch(migration.sql)
                .unwrap_or_else(|e| panic!("migration {} failed: {e}", migration.version));
            tx.execute(
                "INSERT INTO schema_migrations (version, description, checksum, applied_at)
                 VALUES (?1, ?2, ?3, unixepoch() * 1000)",
                rusqlite::params![
                    migration.version,
                    migration.description,
                    checksum(migration.sql)
                ],
            )
            .expect("record migration");
            tx.commit().expect("commit");
        }
    }

    fn columns(conn: &Connection, table: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .expect("pragma");
        stmt.query_map([], |row| row.get::<_, String>(1))
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("rows")
    }

    fn table_exists(conn: &Connection, table: &str) -> bool {
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get::<_, i64>(0),
        )
        .expect("sqlite_master")
            == 1
    }

    #[test]
    fn fresh_database_carries_the_ai_planning_schema() {
        let mut conn = Connection::open_in_memory().expect("open");
        apply(&mut conn).expect("apply");
        assert_eq!(
            current_version(&conn).expect("version"),
            MIGRATIONS.last().expect("non-empty list").version
        );

        assert!(
            columns(&conn, "cycles").contains(&"prioritization_breakdown".to_string()),
            "the new cycles column exists"
        );
        assert!(
            table_exists(&conn, "planning_issue_dismissals"),
            "the dismissals table exists"
        );

        // The breakdown column is a nullable JSON document; NULL means the
        // cycle was never prioritized.
        conn.execute(
            "INSERT INTO cycles (id, title, type, position) VALUES ('c1', 'Day', 'day', 0)",
            [],
        )
        .expect("insert cycle");
        let stored: Option<String> = conn
            .query_row(
                "SELECT prioritization_breakdown FROM cycles WHERE id = 'c1'",
                [],
                |r| r.get(0),
            )
            .expect("select");
        assert_eq!(stored, None);

        conn.execute(
            "UPDATE cycles SET prioritization_breakdown = '{\"must\":[{\"task_id\":\"t1\"}]}' \
             WHERE id = 'c1'",
            [],
        )
        .expect("update");
        let raw: String = conn
            .query_row(
                "SELECT prioritization_breakdown FROM cycles WHERE id = 'c1'",
                [],
                |r| r.get(0),
            )
            .expect("select");
        let value: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON stored");
        assert_eq!(value["must"][0]["task_id"], "t1");
    }

    #[test]
    fn later_type_upgrade_preserves_unclassified_rows_and_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply_up_to(&mut conn, 12);
        conn.execute(
            "INSERT INTO tasks (id, cycle_id, title) VALUES ('parked', 'later', 'Existing idea')",
            [],
        )
        .unwrap();
        apply(&mut conn).unwrap();
        apply(&mut conn).unwrap();
        let task = crate::repository::tasks::require(&conn, "parked").unwrap();
        assert_eq!(task.title, "Existing idea");
        assert_eq!(task.later_plan_type, None, "Old source cannot be inferred");
    }

    #[test]
    fn database_at_0004_upgrades_to_0005_without_data_loss() {
        let mut conn = Connection::open_in_memory().expect("open");
        conn.execute_batch("PRAGMA foreign_keys = ON;").expect("fk");
        apply_up_to(&mut conn, 4);
        assert_eq!(current_version(&conn).unwrap(), 4);
        assert!(
            !columns(&conn, "cycles").contains(&"prioritization_breakdown".to_string()),
            "the column does not exist yet at 0004"
        );

        conn.execute(
            "INSERT INTO cycles (id, title, type, position) VALUES ('c1', 'Week', 'week', 0)",
            [],
        )
        .expect("insert cycle");
        conn.execute(
            "INSERT INTO tasks (id, cycle_id, title, position) VALUES ('t1', 'c1', 'keep me', 0)",
            [],
        )
        .expect("insert task");

        apply(&mut conn).expect("upgrade");
        assert_eq!(
            current_version(&conn).unwrap(),
            MIGRATIONS.last().unwrap().version
        );

        let title: String = conn
            .query_row("SELECT title FROM tasks WHERE id = 't1'", [], |r| r.get(0))
            .expect("task survived");
        assert_eq!(title, "keep me");

        // Task-level duplicates are rejected by the UNIQUE constraint.
        conn.execute(
            "INSERT INTO planning_issue_dismissals (id, cycle_id, issue_type, task_id, reason) \
             VALUES ('d1', 'c1', 'too_many_goals', 't1', 'resolved')",
            [],
        )
        .expect("insert dismissal");
        let duplicate = conn.execute(
            "INSERT INTO planning_issue_dismissals (id, cycle_id, issue_type, task_id) \
             VALUES ('d2', 'c1', 'too_many_goals', 't1')",
            [],
        );
        assert!(
            duplicate.is_err(),
            "UNIQUE (cycle_id, issue_type, task_id) rejects duplicates"
        );

        // Cycle-level dismissals carry a NULL task_id; SQLite treats NULLs as
        // distinct, so their uniqueness is the service layer's contract.
        conn.execute(
            "INSERT INTO planning_issue_dismissals (id, cycle_id, issue_type) \
             VALUES ('d3', 'c1', 'not_sure_what_to_do_next')",
            [],
        )
        .expect("cycle-level dismissal");

        conn.execute("DELETE FROM cycles WHERE id = 'c1'", [])
            .expect("delete cycle");
        let dismissals: i64 = conn
            .query_row("SELECT COUNT(*) FROM planning_issue_dismissals", [], |r| {
                r.get(0)
            })
            .expect("count");
        assert_eq!(dismissals, 0, "dismissals cascade with their cycle");
    }
}

// Extension changes (review / reminders / calendar) land as separate,
// append-only migrations so each capability owns exactly one version.

const M0006_CYCLE_REVIEWS: &str = r#"
-- Review-retrospective (change: add-review-retrospective §1). One review per
-- cycle; the snapshot columns freeze facts/answers at save time.
CREATE TABLE cycle_reviews (
    id           TEXT PRIMARY KEY,
    cycle_id     TEXT NOT NULL UNIQUE REFERENCES cycles(id) ON DELETE CASCADE,
    kind         TEXT NOT NULL,
    is_final     INTEGER NOT NULL DEFAULT 0,
    facts_json   TEXT NOT NULL,
    answers_json TEXT NOT NULL,
    snapshot_at  INTEGER NOT NULL,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

CREATE TABLE cycle_review_dispositions (
    review_id   TEXT NOT NULL REFERENCES cycle_reviews(id) ON DELETE CASCADE,
    task_id     TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    disposition TEXT NOT NULL CHECK (disposition IN ('carry','later','drop')),
    UNIQUE (review_id, task_id)
);

-- The review skill joins the persisted skill set: SQLite cannot alter a
-- CHECK, so the table is rebuilt and the rows carried over.
CREATE TABLE agent_conversations_new (
    id             TEXT PRIMARY KEY,
    cycle_id       TEXT NOT NULL REFERENCES cycles(id) ON DELETE CASCADE,
    active_turn_id TEXT,
    revision       INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0),
    active_skill   TEXT CHECK (active_skill IS NULL OR active_skill IN
                     ('goal_setting','long_term_planning','short_term_planning','prioritization','review')),
    last_error     TEXT,
    created_at     TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at     TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO agent_conversations_new
    SELECT id, cycle_id, active_turn_id, revision, active_skill, last_error, created_at, updated_at
    FROM agent_conversations;
DROP TABLE agent_conversations;
ALTER TABLE agent_conversations_new RENAME TO agent_conversations;
CREATE INDEX ix_agent_conversations_updated ON agent_conversations(updated_at);
"#;

const M0007_REMINDERS: &str = r#"
-- Reminders (change: add-reminders-notifications §1). Polymorphic targets
-- (task/session/day/cycle) — SQLite cannot foreign-key a union, so cleanup
-- on target deletion is a service-layer duty guarded by tests.
CREATE TABLE reminders (
    id           TEXT PRIMARY KEY,
    target_kind  TEXT NOT NULL CHECK (target_kind IN ('task','session','day','cycle')),
    target_id    TEXT NOT NULL,
    fire_at      INTEGER NOT NULL,
    quiet_ok     INTEGER NOT NULL DEFAULT 0,
    fired_at     INTEGER,
    dismissed_at INTEGER,
    created_at   INTEGER NOT NULL,
    UNIQUE (target_kind, target_id, fire_at)
);

CREATE INDEX ix_reminders_due ON reminders(fire_at) WHERE fired_at IS NULL;
"#;

const M0008_SESSION_SCHEDULE: &str = r#"
-- Calendar-time-view (change: add-calendar-time-view §3): a session's planned
-- wall-clock start. Lifecycle `started_at` keeps its meaning; this column is
-- the schedule the timeline renders and the budget aggregates.
ALTER TABLE cycles ADD COLUMN scheduled_start_at INTEGER;
"#;

const M0009_AGENT_SKILLS: &str = r#"
-- Preserve messages before the FK cascade triggered by rebuilding the CHECK.
CREATE TEMP TABLE saved_agent_messages AS SELECT * FROM agent_messages;
CREATE TABLE agent_conversations_new (
    id TEXT PRIMARY KEY,
    cycle_id TEXT NOT NULL REFERENCES cycles(id) ON DELETE CASCADE,
    active_turn_id TEXT,
    revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0),
    active_skill TEXT CHECK (active_skill IS NULL OR active_skill IN
      ('goal_setting','long_term_planning','short_term_planning','weekly_planning','daily_planning','prioritization','review','period_analysis','planning_issues')),
    last_error TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
INSERT INTO agent_conversations_new SELECT * FROM agent_conversations;
DROP TABLE agent_conversations;
ALTER TABLE agent_conversations_new RENAME TO agent_conversations;
CREATE INDEX ix_agent_conversations_updated ON agent_conversations(updated_at);
INSERT INTO agent_messages SELECT * FROM saved_agent_messages;
DROP TABLE saved_agent_messages;
"#;

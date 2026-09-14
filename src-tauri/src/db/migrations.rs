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
    Migration { version: 1, description: "init", sql: M0001_INIT },
    Migration { version: 2, description: "invariants", sql: M0002_INVARIANTS },
    Migration { version: 3, description: "seed later container", sql: M0003_SEED_LATER },
    Migration { version: 4, description: "repeats", sql: M0004_REPEATS },
];

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
    conn.query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |r| r.get(0))
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

    for Migration { version, description, sql } in MIGRATIONS {
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

        let tx = conn.transaction().map_err(|e| AppError::Db(e.to_string()))?;
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

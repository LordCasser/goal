CREATE TABLE _sqlx_migrations (
    version BIGINT PRIMARY KEY,
    description TEXT NOT NULL,
    installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    success BOOLEAN NOT NULL,
    checksum BLOB NOT NULL,
    execution_time BIGINT NOT NULL
);
CREATE TABLE sqlite_sequence(name,seq);
CREATE TABLE repeats_table (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    duration INTEGER NOT NULL CHECK (duration >= 0),
    position INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0 AND position <= 999),
    archived BOOLEAN NOT NULL DEFAULT 0
);
CREATE TABLE task_preview_originals (
    task_id TEXT PRIMARY KEY REFERENCES tasks_table(id) ON DELETE CASCADE,
    cycle_id TEXT NOT NULL REFERENCES "cycles_table"(id) ON DELETE CASCADE,
    original_exists BOOLEAN NOT NULL,
    title TEXT,
    completed BOOLEAN,
    subtasks TEXT,
    position INTEGER,
    goal_breakdown TEXT,
    parent_id TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000)
, root_color_key TEXT, needs_refinement BOOLEAN DEFAULT NULL, needs_breakdown BOOLEAN DEFAULT NULL);
CREATE INDEX idx_task_preview_originals_cycle
ON task_preview_originals(cycle_id);
CREATE TABLE agent_messages (
    id TEXT PRIMARY KEY,
    conversation_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    sequence_number INTEGER NOT NULL CHECK (sequence_number > 0),
    message_type TEXT NOT NULL CHECK (
        message_type IN (
            'user',
            'model_text',
            'model_function_call',
            'function_result',
            'app_tool_result'
        )
    ),
    payload_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (conversation_id) REFERENCES agent_conversations (id) ON DELETE CASCADE,
    UNIQUE (conversation_id, sequence_number)
);
CREATE INDEX idx_agent_messages_conversation_sequence
    ON agent_messages(conversation_id, sequence_number);
CREATE TABLE planning_issue_dismissals (
    id TEXT PRIMARY KEY,
    cycle_id TEXT NOT NULL,
    issue_type TEXT NOT NULL,
    task_id TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY (cycle_id) REFERENCES cycles_table(id) ON DELETE CASCADE,
    CHECK (TRIM(cycle_id) <> ''),
    CHECK (TRIM(issue_type) <> ''),
    CHECK (task_id IS NULL OR TRIM(task_id) <> ''),
    UNIQUE (cycle_id, issue_type, task_id)
);
CREATE UNIQUE INDEX idx_planning_issue_dismissals_plan_unique
    ON planning_issue_dismissals(cycle_id, issue_type)
    WHERE task_id IS NULL;
CREATE INDEX idx_planning_issue_dismissals_cycle
    ON planning_issue_dismissals(cycle_id);
CREATE TABLE IF NOT EXISTS "cycles_table" (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    type TEXT NOT NULL CHECK (type IN ('session', 'day', 'week', 'month')),
    started BOOLEAN NOT NULL DEFAULT 0,
    finished BOOLEAN NOT NULL DEFAULT 0,
    started_at INTEGER,
    finished_at INTEGER,
    created_at INTEGER NOT NULL DEFAULT (unixepoch() * 1000),
    parent_id TEXT REFERENCES "cycles_table"(id) ON DELETE CASCADE,
    archived BOOLEAN DEFAULT 0,
    position INTEGER NOT NULL DEFAULT 0 CHECK (position >= 0 AND position <= 999),
    repeat_id TEXT REFERENCES repeats_table(id),
    focused_time INTEGER,
    prioritization_breakdown TEXT,
    duration INTEGER DEFAULT 0 CHECK (duration IS NULL OR duration >= 0),
    starts_on TEXT NULL,
    ends_on TEXT NULL,
    calendar_key TEXT NULL,
    CHECK (NOT (started = 0 AND finished = 1))
);
CREATE INDEX idx_cycles_parent ON cycles_table(parent_id);
CREATE INDEX idx_cycles_type ON cycles_table(type);
CREATE INDEX idx_cycles_archived ON cycles_table(archived);
CREATE UNIQUE INDEX idx_unique_cycle_calendar_key
ON cycles_table(calendar_key)
WHERE calendar_key IS NOT NULL;
CREATE TABLE IF NOT EXISTS "tasks_table" (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    completed BOOLEAN NOT NULL DEFAULT 0,
    subtasks TEXT NOT NULL DEFAULT '[]',
    cycle_id TEXT NOT NULL REFERENCES cycles_table(id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    parent_id TEXT DEFAULT NULL,
    goal_breakdown TEXT DEFAULT NULL,
    agent_proposal TEXT DEFAULT NULL CHECK (
        agent_proposal IN ('upsert', 'delete')
    ),
    root_color_key TEXT DEFAULT NULL,
    copied_from_task_id TEXT DEFAULT NULL,
    needs_refinement BOOLEAN DEFAULT NULL,
    needs_breakdown BOOLEAN DEFAULT NULL
);
CREATE INDEX idx_tasks_cycle ON tasks_table(cycle_id);
CREATE INDEX idx_tasks_cycle_agent_proposal_visibility
ON tasks_table(cycle_id, agent_proposal, position);
CREATE INDEX idx_tasks_parent ON tasks_table(parent_id);
CREATE INDEX idx_tasks_root_color_key ON tasks_table(cycle_id, root_color_key);
CREATE INDEX idx_tasks_cycle_copied_from_task
ON tasks_table(cycle_id, copied_from_task_id);
CREATE TABLE IF NOT EXISTS "agent_conversations" (
    id TEXT PRIMARY KEY,
    cycle_id TEXT NOT NULL,
    active_turn_id TEXT,
    revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0),
    last_error TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now')),
    active_skill TEXT CHECK (
        active_skill IS NULL
        OR active_skill IN (
            'goal_setting',
            'long_term_planning',
            'short_term_planning',
            'prioritization'
        )
    ),
    FOREIGN KEY (cycle_id) REFERENCES cycles_table(id) ON DELETE CASCADE
);
CREATE INDEX idx_agent_conversations_updated_at
ON agent_conversations(updated_at);
CREATE TRIGGER reject_task_root_color_key_non_long_term_insert
BEFORE INSERT ON tasks_table
FOR EACH ROW
WHEN NEW.root_color_key IS NOT NULL
    AND NOT EXISTS (
        SELECT 1
        FROM cycles_table
        WHERE id = NEW.cycle_id
          AND type = 'month'
    )
BEGIN
    SELECT RAISE(ABORT, 'root_color_key requires a Long-term cycle');
END;
CREATE TRIGGER reject_task_root_color_key_non_long_term_update
BEFORE UPDATE OF cycle_id, root_color_key ON tasks_table
FOR EACH ROW
WHEN NEW.root_color_key IS NOT NULL
    AND NOT EXISTS (
        SELECT 1
        FROM cycles_table
        WHERE id = NEW.cycle_id
          AND type = 'month'
    )
BEGIN
    SELECT RAISE(ABORT, 'root_color_key requires a Long-term cycle');
END;
CREATE TRIGGER reject_cycle_type_change_with_root_colors
BEFORE UPDATE OF type ON cycles_table
FOR EACH ROW
WHEN NEW.type != 'month'
    AND EXISTS (
        SELECT 1
        FROM tasks_table
        WHERE cycle_id = NEW.id
          AND root_color_key IS NOT NULL
    )
BEGIN
    SELECT RAISE(ABORT, 'cycles with root colors must stay Long-term');
END;

//! SQL for the agent conversation aggregate: one conversation per planning
//! cycle, ordered messages, skill and error state
//! (add-ai-planning-core §2; schema in migration 0001).

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{from_rusqlite, AppError, AppResult};

/// One conversation row (mirrors `agent_conversations`).
#[derive(Debug, Clone, PartialEq)]
pub struct Conversation {
    pub id: String,
    pub cycle_id: String,
    /// Set while a turn is in flight; the concurrency guard reads it.
    pub active_turn_id: Option<String>,
    pub revision: i64,
    /// One of the four skill values or NULL (`AgentSkill::None` is not
    /// persisted).
    pub active_skill: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// One stored message (mirrors `agent_messages`).
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub id: String,
    pub conversation_id: String,
    pub turn_id: String,
    pub sequence_number: i64,
    /// user | model_text | model_function_call | function_result |
    /// app_tool_result — validated by a SQL CHECK, parsed here.
    pub message_type: String,
    pub payload_json: String,
    pub created_at: String,
}

const CONVERSATION_COLUMNS: &str =
    "id, cycle_id, active_turn_id, revision, active_skill, last_error, created_at, updated_at";

fn row_to_conversation(row: &rusqlite::Row<'_>) -> rusqlite::Result<Conversation> {
    Ok(Conversation {
        id: row.get(0)?,
        cycle_id: row.get(1)?,
        active_turn_id: row.get(2)?,
        revision: row.get(3)?,
        active_skill: row.get(4)?,
        last_error: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

const MESSAGE_COLUMNS: &str =
    "id, conversation_id, turn_id, sequence_number, message_type, payload_json, created_at";

fn row_to_message(row: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
    Ok(Message {
        id: row.get(0)?,
        conversation_id: row.get(1)?,
        turn_id: row.get(2)?,
        sequence_number: row.get(3)?,
        message_type: row.get(4)?,
        payload_json: row.get(5)?,
        created_at: row.get(6)?,
    })
}

/// The cycle's conversation, or `None` before the first use (spec: 每周期一条
/// 会话 — creation happens on demand).
pub fn conversation_for_cycle(
    conn: &Connection,
    cycle_id: &str,
) -> AppResult<Option<Conversation>> {
    conn.query_row(
        &format!("SELECT {CONVERSATION_COLUMNS} FROM agent_conversations WHERE cycle_id = ?1"),
        params![cycle_id],
        row_to_conversation,
    )
    .optional()
    .map_err(from_rusqlite)
}

#[cfg(test)]
pub const CONTEXT_IDLE_TTL_MS: i64 = 15 * 60 * 1000;
pub const CONTEXT_IDLE_SETTING: &str = "ai.context-idle-minutes";
pub fn context_idle_minutes(conn: &Connection) -> AppResult<i64> {
    Ok(crate::repository::settings::get(conn, CONTEXT_IDLE_SETTING)?
        .and_then(|value| value.parse::<i64>().ok()).filter(|n| (1..=1440).contains(n)).unwrap_or(15))
}

pub fn expires_at(conversation: &Conversation, minutes: i64) -> Option<i64> {
    if conversation.active_turn_id.is_some() { return None; }
    conversation.updated_at.parse::<i64>().ok().map(|t| t + minutes * 60_000)
}

/// Expiration is enforced before every context read/turn, including after app
/// suspension. In-flight replies are not cut off and browsing never extends TTL.
/// The UI schedules a refresh at this same deadline for visible automatic clearing.
pub fn expire_idle(conn: &Connection, now_ms: i64) -> AppResult<()> {
    if !conn.is_autocommit() { return expire_idle_in_transaction(conn,now_ms); }
    let tx=conn.unchecked_transaction().map_err(from_rusqlite)?;
    expire_idle_in_transaction(&tx,now_ms)?;
    tx.commit().map_err(from_rusqlite)
}

fn expire_idle_in_transaction(conn:&Connection,now_ms:i64)->AppResult<()> {
    let predicate = "active_turn_id IS NULL AND CAST(updated_at AS INTEGER) <= ?1 AND (active_skill IS NOT NULL OR last_error IS NOT NULL OR EXISTS (SELECT 1 FROM agent_messages m WHERE m.conversation_id = agent_conversations.id))";
    let cutoff=now_ms-context_idle_minutes(conn)?*60_000;
    let ids:Vec<String>=conn.prepare(&format!("UPDATE agent_conversations SET active_skill=NULL, last_error=NULL, revision=revision+1, updated_at=?2 WHERE {predicate} RETURNING id"))
        .map_err(from_rusqlite)?.query_map(params![cutoff,now_ms],|r|r.get(0)).map_err(from_rusqlite)?
        .collect::<rusqlite::Result<_>>().map_err(from_rusqlite)?;
    for id in ids {conn.execute("DELETE FROM agent_messages WHERE conversation_id=?1",[id]).map_err(from_rusqlite)?;}
    Ok(())
}

/// Insert-or-get for the cycle's single conversation.
pub fn get_or_create_conversation(
    conn: &Connection,
    cycle_id: &str,
    now_ms: i64,
) -> AppResult<Conversation> {
    expire_idle(conn, now_ms)?;
    if let Some(existing) = conversation_for_cycle(conn, cycle_id)? {
        return Ok(existing);
    }
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO agent_conversations (id, cycle_id, revision, created_at, updated_at) \
         VALUES (?1, ?2, 0, ?3, ?3)",
        params![id, cycle_id, now_ms],
    )
    .map_err(from_rusqlite)?;
    conversation_for_cycle(conn, cycle_id)?
        .ok_or_else(|| AppError::Internal("conversation vanished right after insert".to_string()))
}

pub fn conversation_by_id(
    conn: &Connection,
    conversation_id: &str,
) -> AppResult<Option<Conversation>> {
    conn.query_row(
        &format!("SELECT {CONVERSATION_COLUMNS} FROM agent_conversations WHERE id = ?1"),
        params![conversation_id],
        row_to_conversation,
    )
    .optional()
    .map_err(from_rusqlite)
}

/// Messages in conversation order; `since_sequence` 0 = everything.
pub fn list_messages(
    conn: &Connection,
    conversation_id: &str,
    since_sequence: i64,
) -> AppResult<Vec<Message>> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT {MESSAGE_COLUMNS} FROM agent_messages \
             WHERE conversation_id = ?1 AND sequence_number > ?2 \
             ORDER BY sequence_number ASC"
        ))
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map(params![conversation_id, since_sequence], row_to_message)
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(rows)
}

pub fn max_sequence(conn: &Connection, conversation_id: &str) -> AppResult<i64> {
    conn.query_row(
        "SELECT COALESCE(MAX(sequence_number), 0) FROM agent_messages WHERE conversation_id = ?1",
        params![conversation_id],
        |r| r.get(0),
    )
    .map_err(from_rusqlite)
}

/// One message insert; caller assigns the sequence (the turn executor writes
/// the final order in one pass, spec: 消息按最终顺序一次性写入).
pub fn insert_message(conn: &Connection, message: &Message) -> AppResult<()> {
    conn.execute(
        "INSERT INTO agent_messages \
         (id, conversation_id, turn_id, sequence_number, message_type, payload_json, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            message.id,
            message.conversation_id,
            message.turn_id,
            message.sequence_number,
            message.message_type,
            message.payload_json,
            message.created_at,
        ],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

/// Atomically claims the conversation for one turn: succeeds only when no
/// other turn is in flight (spec: 同一会话同一时刻最多一个进行中的回合).
/// Returns `false` when the conversation is already busy.
pub fn claim_turn(conn: &Connection, conversation_id: &str, turn_id: &str) -> AppResult<bool> {
    let claimed = conn
        .execute(
            "UPDATE agent_conversations SET active_turn_id = ?2 \
             WHERE id = ?1 AND active_turn_id IS NULL",
            params![conversation_id, turn_id],
        )
        .map_err(from_rusqlite)?;
    Ok(claimed == 1)
}

/// Clears the in-flight marker; idempotent.
pub fn release_turn(conn: &Connection, conversation_id: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE agent_conversations SET active_turn_id = NULL WHERE id = ?1",
        params![conversation_id],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

/// Persists the turn's outcome: new skill, cleared/kept error, bumped
/// revision. `skill` None means "keep the current value" (a text-only turn
/// does not deactivate the active skill).
pub fn finish_turn(
    conn: &Connection,
    conversation_id: &str,
    now_ms: i64,
    skill: Option<&str>,
    last_error: Option<&str>,
) -> AppResult<Conversation> {
    let skill_set = match skill {
        Some(_) => "active_skill = NULLIF(?3, 'none'),",
        None => "",
    };
    let sql = format!(
        "UPDATE agent_conversations SET {skill_set} last_error = ?4, revision = revision + 1, \
         updated_at = ?2, active_turn_id = NULL WHERE id = ?1 \
         RETURNING {CONVERSATION_COLUMNS}"
    );
    conn.query_row(
        &sql,
        rusqlite::params![
            conversation_id,
            now_ms,
            skill.unwrap_or_default(),
            last_error,
        ],
        row_to_conversation,
    )
    .map_err(from_rusqlite)
}

/// The latest conversation across cycles, for "previous conversation" reads.
pub fn latest_for_cycle(conn: &Connection, cycle_id: &str) -> AppResult<Option<Conversation>> {
    conversation_for_cycle(conn, cycle_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;

    fn db() -> (Db, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db = crate::db::open_at(&dir.path().join("test.db")).unwrap();
        (db, dir)
    }

    #[test]
    fn configured_timeout_is_validated_and_controls_expiration_immediately() {
        let (db, _dir) = db(); let conn = db.pool().get().unwrap();
        assert_eq!(context_idle_minutes(&conn).unwrap(), 15);
        for invalid in ["0", "1.5", "1441", "forever"] {
            assert!(crate::service::settings::set_app_flag(&db, CONTEXT_IDLE_SETTING.into(), invalid.into()).is_err());
        }
        crate::service::settings::set_app_flag(&db, CONTEXT_IDLE_SETTING.into(), "1".into()).unwrap();
        let c = get_or_create_conversation(&conn, "later", 1000).unwrap();
        conn.execute("UPDATE agent_conversations SET active_skill='daily_planning' WHERE id=?1", [&c.id]).unwrap();
        assert_eq!(expires_at(&c, context_idle_minutes(&conn).unwrap()), Some(61_000));
        expire_idle(&conn, 61_000).unwrap();
        assert!(conversation_for_cycle(&conn, "later").unwrap().unwrap().active_skill.is_none());
    }

    #[test]
    fn expiry_clears_only_idle_context_at_fifteen_minutes_and_never_extends_on_read() {
        let (db, _dir) = db(); let conn = db.pool().get().unwrap();
        let c = get_or_create_conversation(&conn, "later", 1000).unwrap();
        conn.execute("UPDATE agent_conversations SET active_skill='daily_planning' WHERE id=?1", [&c.id]).unwrap();
        conn.execute("INSERT INTO agent_messages (id,conversation_id,turn_id,sequence_number,message_type,payload_json) VALUES ('m',?1,'t',1,'user','{}')", [&c.id]).unwrap();
        get_or_create_conversation(&conn, "later", 1000 + CONTEXT_IDLE_TTL_MS - 1).unwrap();
        assert_eq!(list_messages(&conn, &c.id, 0).unwrap().len(), 1);
        assert_eq!(conversation_for_cycle(&conn, "later").unwrap().unwrap().updated_at, "1000");
        claim_turn(&conn, &c.id, "running").unwrap();
        expire_idle(&conn, 1000 + CONTEXT_IDLE_TTL_MS).unwrap();
        assert_eq!(list_messages(&conn, &c.id, 0).unwrap().len(), 1);
        release_turn(&conn, &c.id).unwrap();
        expire_idle(&conn, 1000 + CONTEXT_IDLE_TTL_MS).unwrap();
        assert!(list_messages(&conn, &c.id, 0).unwrap().is_empty());
        let cleared = conversation_for_cycle(&conn, "later").unwrap().unwrap();
        assert!(cleared.active_skill.is_none()); assert_eq!(cleared.revision, 1);
        assert!(crate::repository::cycles::get(&conn, "later").unwrap().is_some());
    }

    #[test]
    fn get_or_create_is_one_per_cycle() {
        let (db, _dir) = db();
        let conn = db.pool().get().unwrap();
        let first = get_or_create_conversation(&conn, "later", 1).unwrap();
        let second = get_or_create_conversation(&conn, "later", 2).unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(first.revision, 0);
        assert!(first.active_skill.is_none());
    }

    #[test]
    fn messages_are_sequential_and_isolated_per_conversation() {
        let (db, _dir) = db();
        let conn = db.pool().get().unwrap();
        // Conversations must reference real cycles (FK + invariant trigger).
        let month = crate::service::cycles::create_planning_cycle(
            &db,
            &crate::service::cycles::CreateCycleArgs {
                cycle_type: "month".into(),
                duration_months: Some(1),
                ..Default::default()
            },
            crate::domain::calendar::today_local(),
            1,
        )
        .unwrap()
        .value;
        let conversation = get_or_create_conversation(&conn, "later", 1).unwrap();
        let other = get_or_create_conversation(&conn, &month.id, 1).unwrap();
        for (index, conversation_id) in [&conversation.id, &other.id].into_iter().enumerate() {
            let message = Message {
                id: uuid::Uuid::new_v4().to_string(),
                conversation_id: conversation_id.to_string(),
                turn_id: "t1".into(),
                sequence_number: index as i64 + 1,
                message_type: "user".into(),
                payload_json: "{\"text\":\"hi\"}".into(),
                created_at: "now".into(),
            };
            insert_message(&conn, &message).unwrap();
        }
        assert_eq!(list_messages(&conn, &conversation.id, 0).unwrap().len(), 1);
        assert_eq!(max_sequence(&conn, &conversation.id).unwrap(), 1);
    }

    #[test]
    fn message_type_check_rejects_unknown_types() {
        let (db, _dir) = db();
        let conn = db.pool().get().unwrap();
        let conversation = get_or_create_conversation(&conn, "later", 1).unwrap();
        let bad = Message {
            id: uuid::Uuid::new_v4().to_string(),
            conversation_id: conversation.id.clone(),
            turn_id: "t1".into(),
            sequence_number: 1,
            message_type: "gossip".into(),
            payload_json: "{}".into(),
            created_at: "now".into(),
        };
        assert!(insert_message(&conn, &bad).is_err());
    }

    #[test]
    fn finish_turn_bumps_revision_and_persists_skill() {
        let (db, _dir) = db();
        let conn = db.pool().get().unwrap();
        let conversation = get_or_create_conversation(&conn, "later", 1).unwrap();
        assert!(claim_turn(&conn, &conversation.id, "t1").unwrap());
        assert!(
            !claim_turn(&conn, &conversation.id, "t2").unwrap(),
            "second claim must fail while busy"
        );
        let after = finish_turn(&conn, &conversation.id, 2, Some("goal_setting"), None).unwrap();
        assert_eq!(after.revision, 1);
        assert_eq!(after.active_skill.as_deref(), Some("goal_setting"));
        assert!(after.active_turn_id.is_none());
        // A text-only turn keeps the skill and keeps counting.
        let after2 = finish_turn(&conn, &conversation.id, 3, None, Some("boom")).unwrap();
        assert_eq!(after2.revision, 2);
        assert_eq!(after2.active_skill.as_deref(), Some("goal_setting"));
        assert_eq!(after2.last_error.as_deref(), Some("boom"));
    }

    #[test]
    fn cycle_delete_cascades_conversation_and_messages() {
        let (db, _dir) = db();
        let conn = db.pool().get().unwrap();
        // Create a real planning cycle to host the conversation.
        let month = crate::service::cycles::create_planning_cycle(
            &db,
            &crate::service::cycles::CreateCycleArgs {
                cycle_type: "month".into(),
                duration_months: Some(1),
                ..Default::default()
            },
            crate::domain::calendar::today_local(),
            1,
        )
        .unwrap()
        .value;
        let conversation = get_or_create_conversation(&conn, &month.id, 1).unwrap();
        insert_message(
            &conn,
            &Message {
                id: uuid::Uuid::new_v4().to_string(),
                conversation_id: conversation.id.clone(),
                turn_id: "t1".into(),
                sequence_number: 1,
                message_type: "user".into(),
                payload_json: "{}".into(),
                created_at: "now".into(),
            },
        )
        .unwrap();
        crate::service::cycles::delete_cycle(&db, &month.id).unwrap();
        assert!(conversation_for_cycle(&conn, &month.id).unwrap().is_none());
        assert!(list_messages(&conn, &conversation.id, 0)
            .unwrap()
            .is_empty());
    }
}

//! SQL for the `reminders` aggregate (change: add-reminders-notifications §1).
//!
//! Reminders are polymorphic (`target_kind` + `target_id`) by design (design
//! D2): this module never interprets what a target *means*, it only stores
//! and queries trigger points. Existence/completion checks and cross-aggregate
//! cleanup live in `service::reminders`.

use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::Serialize;

use crate::error::{from_rusqlite, AppResult};

/// What a reminder points at. Mirrors the migration's CHECK constraint
/// (`'task','session','day','cycle'`) — SQLite cannot alter a CHECK, so the
/// parse set here must stay in sync with `db::migrations::M0007_REMINDERS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetKind {
    Task,
    Session,
    Day,
    Cycle,
}

impl TargetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::Session => "session",
            Self::Day => "day",
            Self::Cycle => "cycle",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "task" => Some(Self::Task),
            "session" => Some(Self::Session),
            "day" => Some(Self::Day),
            "cycle" => Some(Self::Cycle),
            _ => None,
        }
    }
}

/// One row of `reminders`, as it travels across IPC.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reminder {
    pub id: String,
    pub target_kind: TargetKind,
    pub target_id: String,
    /// Milliseconds since the Unix epoch — the scheduled trigger point.
    pub fire_at: i64,
    /// Whether the reminder *may* be silenced when `fire_at` lands inside the
    /// user's quiet hours. `false` = a hard time point that always notifies
    /// (spec: 不可免打扰的提醒).
    pub quiet_ok: bool,
    pub fired_at: Option<i64>,
    pub dismissed_at: Option<i64>,
    pub created_at: i64,
}

impl Reminder {
    /// `true` while the trigger point has not been processed yet.
    pub fn is_pending(&self) -> bool {
        self.fired_at.is_none()
    }
}

fn row_to_reminder(row: &rusqlite::Row<'_>) -> rusqlite::Result<Reminder> {
    let kind_raw: String = row.get("target_kind")?;
    Ok(Reminder {
        id: row.get("id")?,
        target_kind: TargetKind::parse(&kind_raw).ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                format!("unknown target_kind '{kind_raw}'").into(),
            )
        })?,
        target_id: row.get("target_id")?,
        fire_at: row.get("fire_at")?,
        quiet_ok: row.get::<_, i64>("quiet_ok")? != 0,
        fired_at: row.get("fired_at")?,
        dismissed_at: row.get("dismissed_at")?,
        created_at: row.get("created_at")?,
    })
}

pub struct NewReminder {
    pub id: String,
    pub target_kind: TargetKind,
    pub target_id: String,
    pub fire_at: i64,
    pub quiet_ok: bool,
    pub created_at: i64,
}

/// Inserts a reminder. `INSERT OR IGNORE` implements the "reuse, don't
/// duplicate" rule for `(target_kind, target_id, fire_at)` at the storage
/// level; the service layer does the explicit find first so callers get the
/// existing row back (§1.5: 重复设定复用既有记录).
pub fn insert(conn: &Connection, new: &NewReminder) -> AppResult<usize> {
    conn.execute(
        "INSERT OR IGNORE INTO reminders \
         (id, target_kind, target_id, fire_at, quiet_ok, created_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            new.id,
            new.target_kind.as_str(),
            new.target_id,
            new.fire_at,
            new.quiet_ok as i64,
            new.created_at,
        ],
    )
    .map_err(from_rusqlite)
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Option<Reminder>> {
    conn.query_row(
        "SELECT id, target_kind, target_id, fire_at, quiet_ok, fired_at, dismissed_at, created_at \
         FROM reminders WHERE id = ?1",
        params![id],
        row_to_reminder,
    )
    .optional()
    .map_err(from_rusqlite)
}

pub fn require(conn: &Connection, id: &str) -> AppResult<Reminder> {
    get(conn, id)?.ok_or_else(|| crate::error::AppError::not_found("reminder", id))
}

/// The row the reuse rule resolves to: same target at the same instant.
pub fn find_for_target(
    conn: &Connection,
    target_kind: TargetKind,
    target_id: &str,
    fire_at: i64,
) -> AppResult<Option<Reminder>> {
    conn.query_row(
        "SELECT id, target_kind, target_id, fire_at, quiet_ok, fired_at, dismissed_at, created_at \
         FROM reminders WHERE target_kind = ?1 AND target_id = ?2 AND fire_at = ?3",
        params![target_kind.as_str(), target_id, fire_at],
        row_to_reminder,
    )
    .optional()
    .map_err(from_rusqlite)
}

/// Re-points an existing (pending) reminder. Used by `update_reminder` — the
/// old fire time stops existing, so it can never fire (§5.6: 改时间后旧时间失效).
pub fn update_trigger(conn: &Connection, id: &str, fire_at: i64, quiet_ok: bool) -> AppResult<()> {
    conn.execute(
        "UPDATE reminders SET fire_at = ?2, quiet_ok = ?3 WHERE id = ?1",
        params![id, fire_at, quiet_ok as i64],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

/// Re-arms a row that already fired (or was dismissed) so a repeated explicit
/// set of the same reminder fires again instead of silently doing nothing.
pub fn rearm(conn: &Connection, id: &str, quiet_ok: bool) -> AppResult<()> {
    conn.execute(
        "UPDATE reminders SET quiet_ok = ?2, fired_at = NULL, dismissed_at = NULL WHERE id = ?1",
        params![id, quiet_ok as i64],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

/// Updates only the suppressibility flag (quiet-hours policy of the row).
pub fn set_quiet_ok(conn: &Connection, id: &str, quiet_ok: bool) -> AppResult<()> {
    conn.execute(
        "UPDATE reminders SET quiet_ok = ?2 WHERE id = ?1",
        params![id, quiet_ok as i64],
    )
    .map_err(from_rusqlite)?;
    Ok(())
}

/// Claims a reminder for delivery: only the first caller wins. Returns
/// `true` when this call transitioned the row to fired. The guard makes
/// concurrent passes (timer thread + reconcile command + startup collect)
/// deliver each reminder exactly once (§3.5: 专注块通知不重复).
pub fn claim_fired(conn: &Connection, id: &str, at: i64) -> AppResult<bool> {
    let rows = conn
        .execute(
            "UPDATE reminders SET fired_at = ?2 WHERE id = ?1 AND fired_at IS NULL",
            params![id, at],
        )
        .map_err(from_rusqlite)?;
    Ok(rows > 0)
}

/// Marks reminders dismissed (handled in the UI). Returns how many rows
/// changed; already-dismissed rows are not double-counted.
pub fn mark_dismissed(conn: &Connection, ids: &[String], at: i64) -> AppResult<usize> {
    if ids.is_empty() {
        return Ok(0);
    }
    let placeholders = std::iter::repeat("?")
        .take(ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "UPDATE reminders SET dismissed_at = ?1 \
         WHERE id IN ({placeholders}) AND dismissed_at IS NULL"
    );
    let mut bound = vec![rusqlite::types::Value::Integer(at)];
    bound.extend(
        ids.iter()
            .map(|id| rusqlite::types::Value::Text(id.clone())),
    );
    let rows = conn
        .execute(&sql, params_from_iter(bound))
        .map_err(from_rusqlite)?;
    Ok(rows)
}

pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM reminders WHERE id = ?1", params![id])
        .map_err(from_rusqlite)?;
    Ok(())
}

/// Every due, not-yet-fired reminder in delivery order. The partial index
/// `ix_reminders_due` covers the `fired_at IS NULL` filter; explicit tiebreaks
/// keep the batch order deterministic (§2.3: 按 fire_at 升序).
pub fn due(conn: &Connection, now: i64) -> AppResult<Vec<Reminder>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, target_kind, target_id, fire_at, quiet_ok, fired_at, dismissed_at, created_at \
             FROM reminders WHERE fire_at <= ?1 AND fired_at IS NULL \
             ORDER BY fire_at ASC, created_at ASC, id ASC",
        )
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map(params![now], row_to_reminder)
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(rows)
}

/// Reminders that came due after `since` without ever being processed — the
/// startup-compensation window (§4.1). Same ordering as [`due`].
pub fn due_between(conn: &Connection, since: i64, now: i64) -> AppResult<Vec<Reminder>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, target_kind, target_id, fire_at, quiet_ok, fired_at, dismissed_at, created_at \
             FROM reminders WHERE fire_at > ?1 AND fire_at <= ?2 AND fired_at IS NULL \
             ORDER BY fire_at ASC, created_at ASC, id ASC",
        )
        .map_err(from_rusqlite)?;
    let rows = stmt
        .query_map(params![since, now], row_to_reminder)
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(rows)
}

/// The earliest pending trigger point, for the scheduler's next sleep.
pub fn next_pending_fire_at(conn: &Connection) -> AppResult<Option<i64>> {
    conn.query_row(
        "SELECT MIN(fire_at) FROM reminders WHERE fired_at IS NULL",
        [],
        |row| row.get::<_, Option<i64>>(0),
    )
    .map_err(from_rusqlite)
}

/// How the caller wants [`list`] scoped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StatusFilter {
    /// Not yet processed (the manageable list, spec: 查看待触发提醒).
    #[default]
    Pending,
    /// Fired but not yet dismissed — the in-app fallback surface (§3.2/§3.3).
    Fired,
    All,
}

/// Lists reminders, optionally scoped to one cycle's subtree: reminders whose
/// target is a cycle in the subtree (session/day/cycle) or a task inside one.
pub fn list(
    conn: &Connection,
    cycle_id: Option<&str>,
    status: StatusFilter,
) -> AppResult<Vec<Reminder>> {
    let status_sql = match status {
        StatusFilter::Pending => "r.fired_at IS NULL",
        StatusFilter::Fired => "r.fired_at IS NOT NULL AND r.dismissed_at IS NULL",
        StatusFilter::All => "1 = 1",
    };
    let (sql, bindings): (String, Vec<rusqlite::types::Value>) = match cycle_id {
        Some(cycle_id) => (
            format!(
                "WITH RECURSIVE sub(id) AS ( \
                     SELECT id FROM cycles WHERE id = ?1 \
                     UNION ALL \
                     SELECT c.id FROM cycles c JOIN sub s ON c.parent_id = s.id \
                 ) \
                 SELECT r.id, r.target_kind, r.target_id, r.fire_at, r.quiet_ok, \
                        r.fired_at, r.dismissed_at, r.created_at \
                 FROM reminders r \
                 WHERE {status_sql} AND ( \
                     (r.target_kind IN ('session','day','cycle') \
                      AND r.target_id IN (SELECT id FROM sub)) \
                     OR (r.target_kind = 'task' \
                         AND r.target_id IN (SELECT id FROM tasks WHERE cycle_id IN (SELECT id FROM sub))) \
                 ) \
                 ORDER BY r.fire_at ASC, r.created_at ASC, r.id ASC"
            ),
            vec![rusqlite::types::Value::Text(cycle_id.to_string())],
        ),
        None => (
            format!(
                "SELECT r.id, r.target_kind, r.target_id, r.fire_at, r.quiet_ok, \
                        r.fired_at, r.dismissed_at, r.created_at \
                 FROM reminders r WHERE {status_sql} \
                 ORDER BY r.fire_at ASC, r.created_at ASC, r.id ASC"
            ),
            Vec::new(),
        ),
    };
    let mut stmt = conn.prepare(&sql).map_err(from_rusqlite)?;
    let rows = stmt
        .query_map(params_from_iter(bindings), row_to_reminder)
        .map_err(from_rusqlite)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(from_rusqlite)?;
    Ok(rows)
}

/// Removes every reminder pointing at one target (§1.3). Called from the
/// service delete paths; returns how many rows went away.
pub fn purge_for_target(
    conn: &Connection,
    target_kind: TargetKind,
    target_id: &str,
) -> AppResult<usize> {
    conn.execute(
        "DELETE FROM reminders WHERE target_kind = ?1 AND target_id = ?2",
        params![target_kind.as_str(), target_id],
    )
    .map_err(from_rusqlite)
}

/// Removes every reminder attached to a deleted cycle subtree: targets that
/// are cycles of the subtree plus tasks living in those cycles. SQLite cannot
/// express a foreign key on a polymorphic union, so this is the storage half
/// of the service-layer cleanup duty (§1.3).
pub fn purge_for_cycle_subtree(conn: &Connection, cycle_ids: &[String]) -> AppResult<usize> {
    if cycle_ids.is_empty() {
        return Ok(0);
    }
    let placeholders = std::iter::repeat("?")
        .take(cycle_ids.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "DELETE FROM reminders WHERE \
         (target_kind IN ('session','day','cycle') AND target_id IN ({placeholders})) \
         OR (target_kind = 'task' AND target_id IN ( \
             SELECT id FROM tasks WHERE cycle_id IN ({placeholders})) )"
    );
    let mut bound: Vec<rusqlite::types::Value> = cycle_ids
        .iter()
        .map(|id| rusqlite::types::Value::Text(id.clone()))
        .collect();
    bound.extend(
        cycle_ids
            .iter()
            .map(|id| rusqlite::types::Value::Text(id.clone())),
    );
    let rows = conn
        .execute(&sql, params_from_iter(bound))
        .map_err(from_rusqlite)?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open() -> Connection {
        let mut conn = Connection::open_in_memory().expect("open");
        crate::db::migrations::apply(&mut conn).expect("migrations");
        conn
    }

    fn sample(id: &str, fire_at: i64) -> NewReminder {
        NewReminder {
            id: id.into(),
            target_kind: TargetKind::Task,
            target_id: "t1".into(),
            fire_at,
            quiet_ok: false,
            created_at: 1000,
        }
    }

    #[test]
    fn duplicate_target_and_time_reuses_the_row() {
        let conn = open();
        assert_eq!(insert(&conn, &sample("r1", 5000)).unwrap(), 1);
        // Same (target, fire_at), different id: ignored by the UNIQUE index.
        assert_eq!(insert(&conn, &sample("r2", 5000)).unwrap(), 0);
        let found = find_for_target(&conn, TargetKind::Task, "t1", 5000)
            .unwrap()
            .expect("row exists");
        assert_eq!(found.id, "r1");
    }

    #[test]
    fn due_returns_only_unfired_rows_ascending() {
        let conn = open();
        insert(&conn, &sample("r-late", 9000)).unwrap();
        insert(&conn, &sample("r-early", 1000)).unwrap();
        let mut fired = sample("r-fired", 500);
        fired.id = "r-fired".into();
        insert(&conn, &fired).unwrap();
        claim_fired(&conn, "r-fired", 600).unwrap();

        let due = due(&conn, 10_000).unwrap();
        let ids: Vec<&str> = due.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["r-early", "r-late"],
            "fired rows and future rows stay out, ascending order"
        );
        assert_eq!(next_pending_fire_at(&conn).unwrap(), Some(1000));
    }

    #[test]
    fn purge_for_cycle_subtree_covers_cycle_and_task_targets() {
        let conn = open();
        conn.execute(
            "INSERT INTO cycles (id, title, type) VALUES ('c1', 'Day', 'day')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cycles (id, title, type, parent_id) VALUES ('s1', 'Block', 'session', 'c1')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tasks (id, cycle_id, title) VALUES ('t1', 'c1', 'Task')",
            [],
        )
        .unwrap();

        for (kind, target) in [
            (TargetKind::Task, "t1"),
            (TargetKind::Session, "s1"),
            (TargetKind::Day, "c1"),
            (TargetKind::Cycle, "c1"),
        ] {
            let mut new = sample("x", 1);
            new.id = format!("r-{}-{target}", kind.as_str());
            new.target_kind = kind;
            new.target_id = target.into();
            insert(&conn, &new).unwrap();
        }
        let purged = purge_for_cycle_subtree(&conn, &["c1".to_string(), "s1".to_string()]).unwrap();
        assert_eq!(purged, 4, "every reminder under the deleted subtree goes");
        assert!(due(&conn, 10_000).unwrap().is_empty());
    }
}

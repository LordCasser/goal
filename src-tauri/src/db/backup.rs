//! Backup export: produce a self-contained copy of the live database.
//!
//! See `openspec/specs/local-persistence/spec.md` — the export must include
//! changes still sitting in the WAL, so a checkpoint runs before copying.

use std::path::Path;

use crate::db::Db;
use crate::error::{AppError, AppResult};

/// Flushes the WAL into the main database file, then copies it to `target`.
pub fn export(db: &Db, target: &Path) -> AppResult<()> {
    {
        let conn = db.pool().get()?;
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))
            .map_err(|e| AppError::Db(format!("checkpoint failed: {e}")))?;
        let ok: String = conn
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(|e| AppError::Db(e.to_string()))?;
        if ok != "ok" {
            return Err(AppError::Internal(format!("integrity check failed: {ok}")));
        }
    }
    let source = db
        .path()
        .ok_or_else(|| AppError::Internal("database path unknown".into()))?;
    std::fs::copy(&source, target)
        .map_err(|e| AppError::Internal(format!("cannot write backup: {e}")))?;
    Ok(())
}

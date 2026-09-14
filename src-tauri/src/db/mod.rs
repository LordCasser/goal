//! Database access: connection pool, pragmas, migrations and backup.
//!
//! Owns the only `SqliteConnectionManager` in the process. No other module may
//! open a second connection to the same file.

pub mod backup;
pub mod migrations;

use std::path::PathBuf;

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use tauri::{Manager, Runtime};

/// Process-wide database handle stored in Tauri managed state.
pub struct Db {
    pool: Pool<SqliteConnectionManager>,
    path: PathBuf,
}

impl Db {
    pub fn pool(&self) -> &Pool<SqliteConnectionManager> {
        &self.pool
    }

    /// Filesystem location of the database file.
    pub fn path(&self) -> Option<PathBuf> {
        Some(self.path.clone())
    }

    /// Current applied migration version.
    pub fn schema_version(&self) -> Result<i64, crate::error::AppError> {
        let conn = self.pool.get()?;
        migrations::current_version(&conn)
    }
}

/// Resolves the application data directory, creating it when missing.
pub fn data_dir<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<PathBuf, crate::error::AppError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| crate::error::AppError::Internal(format!("no app data dir: {e}")))?;
    std::fs::create_dir_all(&dir)
        .map_err(|e| crate::error::AppError::Internal(format!("cannot create data dir: {e}")))?;
    Ok(dir)
}

/// Opens (creating if needed) the database, applies pragmas and migrations.
pub fn init<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<Db, crate::error::AppError> {
    let path = data_dir(app)?.join("planner.db");
    open_at(&path)
}

/// Opens a database at an explicit path. Used by tests and by backup verification.
pub fn open_at(path: &std::path::Path) -> Result<Db, crate::error::AppError> {
    let manager = SqliteConnectionManager::file(path).with_init(|c| {
        c.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;
             PRAGMA synchronous = NORMAL;",
        )
    });
    let pool = Pool::builder()
        .max_size(8)
        .build(manager)
        .map_err(crate::error::AppError::from)?;

    {
        let mut conn = pool.get()?;
        migrations::apply(&mut conn)?;
    }

    Ok(Db {
        pool,
        path: path.to_path_buf(),
    })
}

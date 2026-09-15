//! Shared fixtures for integration tests: a throwaway database per test.

// Each test binary compiles this module independently; not every helper is
// used by both.
#![allow(dead_code)]

use std::path::PathBuf;

use planner_lib::db::Db;

pub struct TestDb {
    pub db: Db,
    _dir: tempfile::TempDir,
}

impl TestDb {
    /// Opens a fully migrated database in a fresh temp directory.
    pub fn open() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let path: PathBuf = dir.path().join("planner.db");
        let db = planner_lib::db::open_at(&path).expect("open db");
        Self { db, _dir: dir }
    }

    pub fn conn(&self) -> r2d2::PooledConnection<r2d2_sqlite::SqliteConnectionManager> {
        self.db.pool().get().expect("pool connection")
    }
}

/// A stable "today" for deterministic calendar math: Wednesday 2026-09-16.
pub const TODAY: &str = "2026-09-16";

pub fn today() -> chrono::NaiveDate {
    planner_lib::domain::calendar::parse_date(TODAY).unwrap()
}

/// Fixed wall-clock milliseconds so lifecycle math is exact.
pub const NOW: i64 = 1_700_000_000_000;

/// Creates a long-term cycle starting `start`, `months` product months long.
pub fn create_long_term(db: &Db, start: &str, months: i64) -> planner_lib::domain::cycle::Cycle {
    let today = planner_lib::domain::calendar::parse_date(start).unwrap();
    let mutation = planner_lib::service::cycles::create_planning_cycle(
        db,
        &planner_lib::service::cycles::CreateCycleArgs {
            cycle_type: "month".into(),
            duration_months: Some(months),
            ..Default::default()
        },
        today,
        NOW,
    )
    .expect("long-term cycle created");
    mutation.value
}

/// Creates a week cycle under `parent`, anchored on `anchor`.
pub fn create_week(db: &Db, parent_id: &str, anchor: &str) -> planner_lib::domain::cycle::Cycle {
    let today = planner_lib::domain::calendar::parse_date(anchor).unwrap();
    let mutation = planner_lib::service::cycles::create_planning_cycle(
        db,
        &planner_lib::service::cycles::CreateCycleArgs {
            cycle_type: "week".into(),
            parent_id: Some(parent_id.into()),
            ..Default::default()
        },
        today,
        NOW,
    )
    .expect("week cycle created");
    mutation.value
}

/// Creates a day cycle for `date` under `parent`.
pub fn create_day(
    db: &Db,
    parent_id: &str,
    date: &str,
    now: i64,
) -> planner_lib::domain::cycle::Cycle {
    let mutation = planner_lib::service::cycles::create_planning_cycle(
        db,
        &planner_lib::service::cycles::CreateCycleArgs {
            cycle_type: "day".into(),
            parent_id: Some(parent_id.into()),
            date: Some(date.into()),
            ..Default::default()
        },
        today(),
        now,
    )
    .expect("day cycle created");
    mutation.value
}

/// Adds a task through the service (the same entry the command layer uses).
pub fn add_task(db: &Db, cycle_id: &str, title: &str, now: i64) -> planner_lib::domain::task::Task {
    let mutation = planner_lib::service::tasks::add_task(
        db,
        &planner_lib::service::tasks::AddTaskArgs {
            cycle_id: cycle_id.into(),
            title: title.into(),
            ..Default::default()
        },
        now,
    )
    .expect("task added");
    mutation.value
}

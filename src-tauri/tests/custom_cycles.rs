//! Integration coverage for custom long-term bounds and progress schedules.

mod common;

use common::{TestDb, NOW, TODAY};
use planner_lib::domain::cycle::ProgressCheck;
use planner_lib::error::AppError;
use planner_lib::service::cycles::{self, CreateCycleArgs};

fn err_code(error: &AppError) -> &str {
    match error {
        AppError::Validation { code, .. } | AppError::Conflict { code, .. } => code,
        _ => "other",
    }
}

fn custom_args(start: &str, end: &str, progress_check: Option<ProgressCheck>) -> CreateCycleArgs {
    CreateCycleArgs {
        cycle_type: "month".into(),
        starts_on: Some(start.into()),
        ends_on: Some(end.into()),
        progress_check,
        ..Default::default()
    }
}

#[test]
fn custom_cycle_persists_arbitrary_range_and_progress_schedule_after_reload() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("custom.db");
    let cycle_id;
    {
        let db = planner_lib::db::open_at(&path).unwrap();
        let cycle = cycles::create_planning_cycle(
            &db,
            &custom_args(
                "2028-02-28",
                "2028-03-09",
                Some(ProgressCheck::Repeat { every_days: 2 }),
            ),
            common::today(),
            NOW,
        )
        .unwrap()
        .value;
        cycle_id = cycle.id.clone();
        assert_eq!(cycle.starts_on.as_deref(), Some("2028-02-28"));
        assert_eq!(cycle.ends_on.as_deref(), Some("2028-03-09"));
        assert_eq!(
            cycle.duration,
            Some(10 * planner_lib::domain::cycle::MS_PER_DAY)
        );
        assert_eq!(
            cycle.progress_check,
            Some(ProgressCheck::Repeat { every_days: 2 })
        );
        assert_eq!(
            cycle.calendar_key.as_deref(),
            Some("long-term:2028-02-28:2028-03-09")
        );
    }

    let reloaded = planner_lib::db::open_at(&path).unwrap();
    let cycle =
        planner_lib::repository::cycles::require(&reloaded.pool().get().unwrap(), &cycle_id)
            .unwrap();
    assert_eq!(cycle.starts_on.as_deref(), Some("2028-02-28"));
    assert_eq!(cycle.ends_on.as_deref(), Some("2028-03-09"));
    assert_eq!(
        cycle.progress_check,
        Some(ProgressCheck::Repeat { every_days: 2 })
    );
}

#[test]
fn preset_cycles_default_to_a_midpoint_once_check_including_leap_days() {
    let db = TestDb::open();
    let cycle = cycles::create_planning_cycle(
        &db.db,
        &CreateCycleArgs {
            cycle_type: "month".into(),
            duration_months: Some(1),
            ..Default::default()
        },
        planner_lib::domain::calendar::parse_date("2028-02-28").unwrap(),
        NOW,
    )
    .unwrap()
    .value;

    assert_eq!(cycle.ends_on.as_deref(), Some("2028-03-27"));
    assert_eq!(
        cycle.progress_check,
        Some(ProgressCheck::Once {
            date: "2028-03-13".into()
        })
    );
}

#[test]
fn custom_cycle_rejects_invalid_ranges_and_mutually_exclusive_presets() {
    let db = TestDb::open();
    let cases = [
        (
            CreateCycleArgs {
                cycle_type: "month".into(),
                starts_on: Some("2028-02-28".into()),
                ..Default::default()
            },
            "invalid_cycle_range",
        ),
        (
            custom_args("2028-02-30", "2028-03-09", None),
            "invalid_date",
        ),
        (
            custom_args("2028-03-09", "2028-02-28", None),
            "invalid_cycle_range",
        ),
        (
            CreateCycleArgs {
                cycle_type: "month".into(),
                duration_months: Some(1),
                starts_on: Some("2028-02-28".into()),
                ends_on: Some("2028-03-09".into()),
                ..Default::default()
            },
            "invalid_cycle_range",
        ),
        (
            custom_args(
                "2028-02-28",
                "2028-03-09",
                Some(ProgressCheck::Once {
                    date: "2028-03-09".into(),
                }),
            ),
            "invalid_progress_check",
        ),
        (
            custom_args(
                "2028-02-28",
                "2028-03-09",
                Some(ProgressCheck::Repeat { every_days: 0 }),
            ),
            "invalid_progress_check",
        ),
    ];

    for (args, expected) in cases {
        let error = cycles::create_planning_cycle(&db.db, &args, common::today(), NOW).unwrap_err();
        assert_eq!(err_code(&error), expected, "unexpected error for {args:?}");
    }
}

#[test]
fn invalid_progress_check_rolls_back_the_cycle_insert() {
    let db = TestDb::open();
    let before: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM cycles WHERE type = 'month'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    let error = cycles::create_planning_cycle(
        &db.db,
        &custom_args(
            "2028-02-28",
            "2028-03-09",
            Some(ProgressCheck::Once {
                date: "2028-03-09".into(),
            }),
        ),
        common::today(),
        NOW,
    )
    .unwrap_err();

    assert_eq!(err_code(&error), "invalid_progress_check");
    let after: i64 = db
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM cycles WHERE type = 'month'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        after, before,
        "the failed transaction must not leave a cycle behind"
    );
    assert!(planner_lib::repository::cycles::get_by_calendar_key(
        &db.conn(),
        "long-term:2028-02-28:2028-03-09"
    )
    .unwrap()
    .is_none());
}

#[test]
fn repeat_and_once_checks_are_independent_valid_configurations() {
    let db = TestDb::open();
    let once = cycles::create_planning_cycle(
        &db.db,
        &custom_args(
            TODAY,
            "2026-09-26",
            Some(ProgressCheck::Once {
                date: "2026-09-20".into(),
            }),
        ),
        common::today(),
        NOW,
    )
    .unwrap()
    .value;
    let repeat = cycles::create_planning_cycle(
        &db.db,
        &custom_args(
            "2026-10-01",
            "2026-10-11",
            Some(ProgressCheck::Repeat { every_days: 7 }),
        ),
        common::today(),
        NOW + 1,
    )
    .unwrap()
    .value;

    assert_eq!(
        once.progress_check,
        Some(ProgressCheck::Once {
            date: "2026-09-20".into()
        })
    );
    assert_eq!(
        repeat.progress_check,
        Some(ProgressCheck::Repeat { every_days: 7 })
    );
}

#[test]
fn database_rejects_malformed_or_out_of_range_progress_schedules() {
    let db = TestDb::open();
    let cycle = cycles::create_planning_cycle(
        &db.db,
        &custom_args("2028-02-28", "2028-03-09", None),
        common::today(),
        NOW,
    )
    .unwrap()
    .value;
    let conn = db.conn();
    for invalid in [
        "not json",
        "{}",
        "null",
        r#"{"kind":"repeat","every_days":0}"#,
        r#"{"kind":"repeat","every_days":1.5}"#,
        r#"{"kind":"once","date":"2028-02-30"}"#,
        r#"{"kind":"once","date":"2028-03-09"}"#,
        r#"{"kind":"once","date":"2028-02-29","every_days":2}"#,
    ] {
        assert!(
            conn.execute(
                "UPDATE cycles SET progress_check = ?1 WHERE id = ?2",
                rusqlite::params![invalid, cycle.id]
            )
            .is_err(),
            "accepted {invalid}"
        );
    }
    assert_eq!(
        planner_lib::repository::cycles::require(&conn, &cycle.id)
            .unwrap()
            .progress_check,
        cycle.progress_check
    );
}

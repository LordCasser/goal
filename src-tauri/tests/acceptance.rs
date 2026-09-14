//! Acceptance walkthroughs for rebuild-baseline §11: the manual verification
//! paths (创建周期链 → 专注块计时、备份恢复、日志定位) reproduced as
//! deterministic integration tests through the same service entry points the
//! command layer uses. UI wiring itself needs a human run; these tests pin
//! the behaviour the UI is a window onto.

mod common;

use common::TestDb;
use planner_lib::db;
use planner_lib::service::cycles;

/// §11.2 手工走查：长周期 → 目标 → 周计划 → 日任务 → 专注块启动/结束 →
/// `focused_time` 沿链累加。
#[test]
fn full_chain_walkthrough_accrues_focus_time() {
    let test = TestDb::open();
    let month = common::create_long_term(&test.db, common::TODAY, 3);
    let _goal = common::add_task(&test.db, &month.id, "Ship the planner rebuild", common::NOW);
    let week = common::create_week(&test.db, &month.id, common::TODAY);
    let _weekly = common::add_task(&test.db, &week.id, "Plan the week", common::NOW);
    let day = common::create_day(&test.db, &week.id, common::TODAY, common::NOW);
    let _daily = common::add_task(&test.db, &day.id, "Write acceptance tests", common::NOW);

    let session = cycles::add_session(
        &test.db,
        &cycles::AddSessionArgs {
            day_cycle_id: day.id.clone(),
            title: "deep work".into(),
            duration_ms: Some(50 * 60 * 1000),
            position: None,
        },
        common::NOW,
    )
    .unwrap()
    .value;
    cycles::start_cycle(&test.db, &session.id, common::NOW + 1).unwrap();
    let finished =
        cycles::finish_cycle(&test.db, &session.id, common::NOW + 1 + 32 * 60 * 1000)
            .unwrap()
            .value;

    assert!(finished.started && finished.finished);
    let expected = 32 * 60 * 1000;
    let state = cycles::get_planner_state(&test.db).unwrap();
    for (label, id) in [("day", &day.id), ("week", &week.id), ("month", &month.id)] {
        let cycle = state.cycles.iter().find(|c| c.id == *id).unwrap();
        assert_eq!(cycle.focused_time, expected, "focused_time on {label}");
    }
}

/// §11.7 备份恢复：导出后在新目录打开，数据与原库一致。
#[test]
fn backup_exports_and_restores_into_a_fresh_location() {
    let test = TestDb::open();
    let month = common::create_long_term(&test.db, common::TODAY, 1);
    common::add_task(&test.db, &month.id, "persisted through backup", common::NOW);

    let backup_dir = tempfile::tempdir().expect("backup dir");
    let backup_path = backup_dir.path().join("planner-backup.db");
    db::backup::export(&test.db, &backup_path).expect("export backup");
    assert!(backup_path.exists(), "backup file written");

    // Restore = open the exported file as the database of a fresh location.
    let restored = db::open_at(&backup_path).expect("restored db opens and migrates");
    let state = cycles::get_planner_state(&restored).unwrap();
    let restored_month = state
        .cycles
        .iter()
        .find(|c| c.id == month.id)
        .expect("cycle survives restore");
    let tasks_state = planner_lib::service::editor::get_editor_workspace(&restored, &month.id)
        .unwrap();
    assert!(
        tasks_state
            .tasks
            .iter()
            .any(|t| t.task.title == "persisted through backup"),
        "task content survives restore"
    );
    assert!(!restored_month.archived);
}

/// §11.6 日志定位与纯本地：日志写进数据目录下的 logs/，行带模块与级别，
/// 且渠道不涉及任何网络类型（编译层面即无从出网——这里钉住文件行为）。
#[test]
fn debug_log_lands_in_log_dir_with_module_and_level() {
    let dir = tempfile::tempdir().expect("tempdir");
    let logs = dir.path().join("logs");
    planner_lib::logging::init(logs.clone(), Some(planner_lib::logging::Level::Info));
    planner_lib::logging::info("app", "acceptance smoke line");
    let contents =
        std::fs::read_to_string(logs.join("planner.log")).expect("log file readable");
    assert!(contents.contains("[INFO] [app] acceptance smoke line"));
}

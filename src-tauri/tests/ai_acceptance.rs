//! Acceptance walkthroughs for add-ai-planning-core §10: the manual paths
//! (agent write → pending → revert restores; AI off → review silently skips)
//! reproduced as offline tests through the same entry points the commands
//! use. The real-provider clarification flow (§10.2) stays a manual check;
//! its mechanics are covered by the FakeProvider loop tests in
//! `ai::agent::turn`, and the semantic layer's provider-failure degradation
//! by the `ai::review` unit tests.

mod common;

use common::TestDb;
use planner_lib::ai::agent::turn::{NopExecutor, ToolExecutor, ToolOutcome};
use planner_lib::ai::llm::types::ToolCallRecord;
use planner_lib::ai::tools::ToolRegistry;

fn call(id: &str, name: &str, arguments: serde_json::Value) -> ToolCallRecord {
    ToolCallRecord { id: id.into(), name: name.into(), arguments }
}

/// §10.3: a tool write lands in the pending list and Revert restores the
/// exact prior data — the conversation records the proposal, the data
/// expresses the final truth (spec: 会话历史与最终数据不一致是预期状态).
#[test]
fn agent_write_is_revertible_and_revert_restores_the_data() {
    let test = TestDb::open();
    let month = common::create_long_term(&test.db, common::TODAY, 3);
    let executor = ToolRegistry;

    // The agent "creates" a goal — reported success to the model.
    let outcome = executor.execute(
        &test.db,
        &month.id,
        &call("c1", "create_goal", serde_json::json!({ "title": "Ship v1", "rationale": "user asked" })),
    );
    assert!(!outcome.is_error, "{:?}", outcome.result);
    let task_id = outcome.result["task_id"].as_str().unwrap().to_string();

    // It is only a proposal: hidden from the visible list, snapshotted.
    let preview =
        planner_lib::service::proposals::get_preview_summary(&test.db, &month.id).unwrap();
    assert_eq!(preview.count, 1);
    let conn = test.db.pool().get().unwrap();
    let visible = planner_lib::repository::tasks::list_visible_by_cycle(&conn, &month.id)
        .unwrap();
    assert!(!visible.iter().any(|t| t.id == task_id));

    // Revert: the row never existed before, so it disappears entirely.
    planner_lib::service::proposals::undo_task_preview(&test.db, &task_id).unwrap();
    assert!(planner_lib::repository::tasks::get(&conn, &task_id).unwrap().is_none());
    let preview_after =
        planner_lib::service::proposals::get_preview_summary(&test.db, &month.id).unwrap();
    assert_eq!(preview_after.count, 0);
}

/// §10.5: with no provider configured, the structural review still runs and
/// the whole path stays error-free — planning is never gated on AI. (The
/// command layer answers `no_active_provider` before any review; the
/// semantic layer's own provider-failure degradation is covered in
/// `ai::review` with a failing FakeProvider.)
#[test]
fn review_degrades_silently_without_a_provider() {
    let test = TestDb::open();
    let month = common::create_long_term(&test.db, common::TODAY, 1);
    for index in 0..6 {
        common::add_task(&test.db, &month.id, &format!("Goal {index}"), common::NOW);
    }
    // No AI state is even needed: the structural review is pure local data.
    let cache = planner_lib::ai::review::IssueCache::default();
    let issues = planner_lib::ai::review::review_cached(&test.db, &cache, &month.id);
    assert!(issues
        .iter()
        .any(|issue| issue.issue_type == planner_lib::ai::review::IssueType::TooManyGoals));

    // Resolving a provider without configuration fails with the stable code
    // the UI maps to the settings entry — and nothing else breaks.
    let settings =
        planner_lib::providers::service::AiSettingsState::load(&std::env::temp_dir().join("planner-nonexistent-test")).unwrap();
    let error = planner_lib::ai::llm::resolve(&settings).unwrap_err();
    assert_eq!(error.code(), "no_active_provider");
}

/// The disabled-executor contract: a turn whose registry is unavailable
/// still answers tool errors without touching data.
#[test]
fn nop_executor_answers_tool_errors_without_touching_data() {
    let test = TestDb::open();
    let month = common::create_long_term(&test.db, common::TODAY, 1);
    let outcome = NopExecutor.execute(
        &test.db,
        &month.id,
        &call("c1", "create_goal", serde_json::json!({ "title": "x" })),
    );
    assert!(outcome.is_error);
    let conn = test.db.pool().get().unwrap();
    let visible = planner_lib::repository::tasks::list_visible_by_cycle(&conn, &month.id).unwrap();
    assert!(visible.is_empty());
}

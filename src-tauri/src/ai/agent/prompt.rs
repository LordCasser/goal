//! Skill system prompts (add-ai-planning-core §3): one instruction block per
//! [`AgentSkill`], assembled by [`build_system_instruction`].
//!
//! Behaviour constraints come from the four capability specs
//! (`openspec/specs/{goal-clarification,prioritization,planning-issues}.md`
//! and agent-conversation's 技能激活 requirement). The prompts are the only
//! place that tells the model how to behave; deterministic rules (clarity
//! derivation, merge semantics) live in code and are never restated here as
//! suggestions — the model's job is extraction and conversation, the code's
//! job is computation.

use crate::ai::llm::types::AgentSkill;

/// The five instruction blocks. `None` explains the start tools and asks the
/// model to pick one before doing anything else (spec: 未激活技能时的引导).
pub fn build_system_instruction(skill: AgentSkill) -> String {
    // Every block carries the shared contract rules (preview semantics and
    // no-fabrication); the skill text adds its own workflow on top.
    let skill_text = match skill {
        AgentSkill::None => NONE_PROMPT,
        AgentSkill::GoalSetting => GOAL_SETTING_PROMPT,
        AgentSkill::LongTermPlanning => LONG_TERM_PLANNING_PROMPT,
        AgentSkill::ShortTermPlanning => SHORT_TERM_PLANNING_PROMPT,
        AgentSkill::Prioritization => PRIORITIZATION_PROMPT,
    };
    format!("{COMMON}\n{skill_text}")
}

/// Shared conventions every skill block carries: preview semantics (write
/// tools always report success — the confirm step belongs to the human
/// interface, never to the conversation), and the no-fabrication rule.
const COMMON: &str = "\
General rules:
- Write tools always return success once the change is recorded as a proposal. \
The user confirms or reverts proposals in the interface; do not ask whether a \
proposal was accepted, and never repeat a write because it is still pending.
- Never invent metrics, dates, stakeholders, products or scope. Every value you \
write must come from something the user said or from a tool result.
- Speak the user's language: goals, steps and questions stay in the language the \
user writes in.
- Ask at most one question per message.";

const NONE_PROMPT: &str = "\
You are a planning coach inside a local-first planner app.

No planning skill is active yet. Look at the context block and choose the right \
entry tool before doing anything else:
- `start_goal_setting` to clarify one vague goal (works on any planning cycle),
- `start_planning` to plan a long-term or week/day cycle,
- `start_prioritization` to sort existing tasks into priority buckets.

Do not create, change or analyse anything until a skill is activated.

";
const GOAL_SETTING_PROMPT: &str = "\
You are a goal-clarification coach. Your job is to turn one vague goal into a \
clear, executable one by filling its structured breakdown: context (why), \
output (what), outcome (how we know it worked), scope (how big).

Follow the stages in order; stay in the stage until its goal is met:
1. UNDERSTAND — read the goal and its breakdown. Say briefly why the goal is \
not yet clear, then ask exactly one targeted question about the missing piece. \
Start with context: why the goal matters decides everything after it.
2. COLLECT — gather output, outcome and verification. If the user answers \
several missing pieces at once, write them all with one `update_goal_breakdown` \
call. A field that truly does not apply may be skipped, but say so.
3. REFINE_TITLE — once output and outcome are known, propose a sharper title. \
Prefer a controllable action over a passive result (\"signed agreement\" becomes \
\"Sign agreement\"); when only the output exists, the title is the action that \
produces it; when neither exists, keep the current title.
4. BREAK_DOWN — propose concrete next steps. Steps are proposals: tell the user \
to confirm them in the interface. Only after the user confirms do the steps \
count as written back (the interface clears the needs-breakdown flag).
5. FINISH — summarise the clarified goal and stop coaching.

Never rate clarity with numbers or labels. Clarity is computed by the system \
from the breakdown; you just fill the breakdown and ask the next question.

";
const LONG_TERM_PLANNING_PROMPT: &str = "\
You are a long-term planning coach for a cycle that spans months. The context \
block lists the cycle's goals and their state.

Work top-down: goals first, then the concrete results each goal needs. Use \
`get_cycle_context` and `get_task_details` to read before you write; create or \
change goals only through the write tools, one coherent change per call, always \
with a short rationale that names what the user told you.

Typical moves:
- turn a wish into a goal with a checkable outcome,
- split an overloaded goal into separate goals,
- park a goal that lost its why (tell the user it can be reverted).

Keep the cycle small enough to stay honest: if the goals clearly exceed the \
cycle's remaining time, say so and let the user decide what to drop.

";
const SHORT_TERM_PLANNING_PROMPT: &str = "\
You are a short-term planning coach for a week or day cycle. The context block \
lists the parent goal(s), the cycle's tasks and the remaining time.

Work bottom-up: concrete tasks that visibly serve a parent goal, sized for the \
time that is actually left. Use `get_cycle_context` and `get_task_details` to \
read before you write; write only through the tools with a rationale that names \
the user's words.

If a task has no parent goal, planning may still proceed — link it when the \
user names one, and never invent a parent for it. If the plan clearly does not \
fit the remaining time, point at the largest item and ask what to move out.

";
const PRIORITIZATION_PROMPT: &str = "\
You are a prioritization coach. The context block contains the cycle's \
prioritization breakdown and the tasks still awaiting review.

Sort the pending tasks into the buckets big_wins, bottlenecks, \
non_negotiables and deprioritized with `update_prioritization_breakdown`. \
Every moved task needs a reason in the user's own words — if the user has not \
given one, ask; do not move the task on a reason you invented. Tasks stay in \
`pending_review` until they are placed.

This skill only sorts what exists. Never create goals or break work down here; \
if the cycle has nothing to sort, say that planning comes first.

Only send buckets that change in one update — buckets you leave out keep their \
current content.
";

/// `start_planning` dispatch (task 3.4): the cycle type decides the skill.
/// Sessions are rejected by the caller — agent mutations are not supported
/// for focus blocks (spec: 对专注块启动规划).
pub fn planning_skill_for_cycle(cycle_type: &str) -> Result<AgentSkill, String> {
    match cycle_type {
        "month" => Ok(AgentSkill::LongTermPlanning),
        "week" | "day" => Ok(AgentSkill::ShortTermPlanning),
        other => Err(format!(
            "Agent mutations are not supported for {other} cycles."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_skill_has_a_nonempty_instruction() {
        for skill in [
            AgentSkill::None,
            AgentSkill::GoalSetting,
            AgentSkill::LongTermPlanning,
            AgentSkill::ShortTermPlanning,
            AgentSkill::Prioritization,
        ] {
            let prompt = build_system_instruction(skill);
            assert!(!prompt.trim().is_empty(), "{skill:?} prompt is empty");
        }
    }

    #[test]
    fn prompts_carry_the_preview_and_no_fabrication_rules() {
        // The two contract-level rules must be present in every block so a
        // skill can never "forget" them.
        for skill in [
            AgentSkill::GoalSetting,
            AgentSkill::LongTermPlanning,
            AgentSkill::ShortTermPlanning,
            AgentSkill::Prioritization,
        ] {
            let prompt = build_system_instruction(skill);
            assert!(prompt.contains("always return success"));
            assert!(prompt.contains("Never invent metrics"));
        }
    }

    #[test]
    fn none_prompt_directs_to_start_tools_only() {
        let prompt = build_system_instruction(AgentSkill::None);
        assert!(prompt.contains("start_goal_setting"));
        assert!(prompt.contains("start_planning"));
        assert!(prompt.contains("start_prioritization"));
    }

    #[test]
    fn goal_setting_walks_the_five_stages() {
        let prompt = build_system_instruction(AgentSkill::GoalSetting);
        for stage in [
            "UNDERSTAND",
            "COLLECT",
            "REFINE_TITLE",
            "BREAK_DOWN",
            "FINISH",
        ] {
            assert!(prompt.contains(stage), "missing stage {stage}");
        }
    }

    #[test]
    fn planning_dispatch_follows_cycle_type() {
        assert_eq!(
            planning_skill_for_cycle("month").unwrap(),
            AgentSkill::LongTermPlanning
        );
        assert_eq!(
            planning_skill_for_cycle("week").unwrap(),
            AgentSkill::ShortTermPlanning
        );
        assert_eq!(
            planning_skill_for_cycle("day").unwrap(),
            AgentSkill::ShortTermPlanning
        );
        let error = planning_skill_for_cycle("session").unwrap_err();
        assert!(error.contains("not supported for session"), "{error}");
    }
}

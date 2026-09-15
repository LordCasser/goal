//! Request and response shapes of the LLM abstraction (task §1.1).
//!
//! These are the agent layer's neutral types: the conversation and tool
//! engines speak only these, [`crate::ai::llm::real`] adapts them onto
//! [`crate::sampling`], and [`crate::ai::llm::fake`] scripts them in tests.
//! Nothing here mentions a specific vendor.

use serde::{Deserialize, Serialize};

/// Workflow selected by the model or an explicit plan-area entry.
/// Names are persisted on the existing conversation; None is stored as NULL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSkill {
    None,
    GoalSetting,
    LongTermPlanning,
    ShortTermPlanning,
    WeeklyPlanning,
    DailyPlanning,
    PeriodAnalysis,
    PlanningIssues,
    Prioritization,
    Review,
}

impl AgentSkill {
    /// Stable lower label; the serde wire value and the persisted value.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::GoalSetting => "goal_setting",
            Self::LongTermPlanning => "long_term_planning",
            Self::ShortTermPlanning => "short_term_planning",
            Self::WeeklyPlanning => "weekly_planning",
            Self::DailyPlanning => "daily_planning",
            Self::PeriodAnalysis => "period_analysis",
            Self::PlanningIssues => "planning_issues",
            Self::Prioritization => "prioritization",
            Self::Review => "review",
        }
    }

    /// Inverse of [`AgentSkill::as_str`]; unknown values yield `None`.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "goal_setting" => Some(Self::GoalSetting),
            "long_term_planning" => Some(Self::LongTermPlanning),
            "short_term_planning" => Some(Self::ShortTermPlanning),
            "weekly_planning" => Some(Self::WeeklyPlanning),
            "daily_planning" => Some(Self::DailyPlanning),
            "period_analysis" => Some(Self::PeriodAnalysis),
            "planning_issues" => Some(Self::PlanningIssues),
            "prioritization" => Some(Self::Prioritization),
            "review" => Some(Self::Review),
            _ => None,
        }
    }
}

/// Author of one stored conversation message. `Tool` marks a tool result
/// message; how it reaches a provider that only knows user/assistant turns is
/// the adapter's problem, not the agent layer's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentRole {
    User,
    Assistant,
    Tool,
}

/// One tool call as the model issued it, or as it is replayed from history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCallRecord {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// One message of the conversation history, shaped so a stored turn can be
/// replayed into a later request: assistant tool calls and the tool results
/// answering them both live here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentMessage {
    pub role: AgentRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCallRecord>,
    /// Set on `Tool` messages: which call this result answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Token accounting of one generation, when the provider reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

/// One agent turn: system instruction, injected context, replayable history,
/// the user's message and the tools the model may call.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRequest {
    pub skill: AgentSkill,
    /// Product of `build_system_instruction(ActiveAgentSkill)` (task §3.1).
    pub system: String,
    /// The `<context>…</context>` injection block (spec: 上下文注入).
    pub context_block: String,
    /// Prior turns, oldest first, including tool-call pairs already executed.
    pub history: Vec<AgentMessage>,
    pub user_message: String,
    pub tools: Vec<ToolDef>,
    /// Resolution order: request value > model configuration > adapter
    /// default (the anthropic adapter is the only one that needs one).
    pub max_tokens: Option<u64>,
}

/// Structured-output request for tool-less scenarios such as clarity
/// evaluation (design D1).
#[derive(Debug, Clone, PartialEq)]
pub struct LlmRequest {
    pub system: String,
    pub prompt: String,
    /// Same resolution order as [`AgentRequest::max_tokens`].
    pub max_tokens: Option<u64>,
}

/// Everything one agent generation produced: text plus zero or more tool
/// calls, delivered whole (never half-assembled), and usage when reported.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCallRecord>,
    pub usage: Option<Usage>,
}

/// A tool definition in the agent layer's neutral shape; the JSON Schema
/// passes through to the provider untouched (task §1.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_wire_values_are_lowercase_and_round_trip() {
        let cases = [
            (AgentSkill::None, "none"),
            (AgentSkill::GoalSetting, "goal_setting"),
            (AgentSkill::LongTermPlanning, "long_term_planning"),
            (AgentSkill::ShortTermPlanning, "short_term_planning"),
            (AgentSkill::Prioritization, "prioritization"),
        ];
        for (skill, label) in cases {
            assert_eq!(skill.as_str(), label);
            assert_eq!(skill, AgentSkill::parse(label).expect("parses"));
            assert_eq!(
                serde_json::to_value(skill).unwrap(),
                serde_json::json!(label)
            );
            assert_eq!(
                skill,
                serde_json::from_value::<AgentSkill>(serde_json::json!(label)).unwrap()
            );
        }
        assert_eq!(AgentSkill::parse("bogus"), None);
        assert_eq!(AgentSkill::parse(""), None);
    }

    #[test]
    fn review_skill_wire_value_round_trips() {
        // add-review-retrospective: the fifth persisted skill.
        assert_eq!(AgentSkill::Review.as_str(), "review");
        assert_eq!(AgentSkill::parse("review"), Some(AgentSkill::Review));
        assert_eq!(
            serde_json::to_value(AgentSkill::Review).unwrap(),
            serde_json::json!("review")
        );
        assert_eq!(
            serde_json::from_value::<AgentSkill>(serde_json::json!("review")).unwrap(),
            AgentSkill::Review
        );
    }

    #[test]
    fn agent_message_serializes_with_lowercase_role_and_optional_tool_fields() {
        let message = AgentMessage {
            role: AgentRole::Assistant,
            content: "starting".into(),
            tool_calls: vec![ToolCallRecord {
                id: "call_1".into(),
                name: "start_planning".into(),
                arguments: serde_json::json!({ "cycle_key": "long-term:2026-09" }),
            }],
            tool_call_id: None,
        };
        let value = serde_json::to_value(&message).unwrap();
        assert_eq!(value["role"], "assistant");
        assert_eq!(value["tool_calls"][0]["name"], "start_planning");
        // `tool_call_id` is absent, not null.
        assert!(value.get("tool_call_id").is_none());

        let back: AgentMessage = serde_json::from_value(value).unwrap();
        assert_eq!(back, message);

        let plain = AgentMessage {
            role: AgentRole::User,
            content: "hi".into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        };
        let value = serde_json::to_value(&plain).unwrap();
        assert!(value.get("tool_calls").is_none());
        // Old payloads without the optional fields still parse.
        let legacy: AgentMessage =
            serde_json::from_value(serde_json::json!({ "role": "user", "content": "hi" })).unwrap();
        assert_eq!(legacy, plain);
    }

    #[test]
    fn response_round_trips_with_usage_and_calls() {
        let response = AgentResponse {
            text: "done".into(),
            tool_calls: vec![ToolCallRecord {
                id: "c".into(),
                name: "create_goal".into(),
                arguments: serde_json::json!({ "title": "x" }),
            }],
            usage: Some(Usage {
                input_tokens: Some(3),
                output_tokens: None,
            }),
        };
        let back: AgentResponse =
            serde_json::from_value(serde_json::to_value(&response).unwrap()).unwrap();
        assert_eq!(back, response);
    }
}

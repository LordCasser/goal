//! Tool definitions and the assembler that collects them (task §1.2).
//!
//! The assembler is the single collection point for the agent tool set: the
//! conversation engine asks it what a skill may call and never hand-builds a
//! tool list. The concrete tools (read, skill activation, preview-backed
//! writes) arrive with task §5; until then every skill resolves to an empty
//! set, which exercises the full request path without changing call sites.

use crate::sampling::ToolSpec;

use super::types::{AgentSkill, ToolDef};

impl ToolDef {
    /// Builds a definition from its three neutral parts; `input_schema` is a
    /// JSON Schema object written with `serde_json::json!`.
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }
}

/// Maps a definition onto the sampling layer's wire shape; the schema passes
/// through untouched.
pub fn to_spec(def: &ToolDef) -> ToolSpec {
    ToolSpec {
        name: def.name.clone(),
        description: def.description.clone(),
        input_schema: def.input_schema.clone(),
    }
}

/// Collects every tool definition available to one skill.
///
/// `tools_supported = false` (the active model was configured as tool-less)
/// degrades to conversation mode: no tools are offered and the agent layer
/// runs without them rather than sending a request the provider must reject
/// (task §1.3). Tool sets per skill land with task §5.
pub fn tool_definitions(skill: AgentSkill, tools_supported: bool) -> Vec<ToolDef> {
    let _ = skill; // consumed once the per-skill sets exist
    if !tools_supported {
        return Vec::new();
    }
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assembler_is_empty_for_every_skill_until_task_5_lands() {
        for skill in [
            AgentSkill::None,
            AgentSkill::GoalSetting,
            AgentSkill::LongTermPlanning,
            AgentSkill::ShortTermPlanning,
            AgentSkill::Prioritization,
        ] {
            assert!(
                tool_definitions(skill, true).is_empty(),
                "{skill:?} should expose no tools yet"
            );
            assert!(
                tool_definitions(skill, false).is_empty(),
                "tool-less models always get the empty set"
            );
        }
    }

    #[test]
    fn tool_def_maps_onto_the_sampling_spec_untouched() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "cycle_key": { "type": "string" }
            },
            "required": ["cycle_key"],
            "additionalProperties": false
        });
        let def = ToolDef::new(
            "get_cycle_context",
            "Read the cycle context",
            schema.clone(),
        );
        let spec = to_spec(&def);
        assert_eq!(spec.name, "get_cycle_context");
        assert_eq!(spec.description, "Read the cycle context");
        assert_eq!(spec.input_schema, schema, "the JSON Schema passes through");
    }
}

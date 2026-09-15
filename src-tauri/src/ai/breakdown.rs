//! GoalBreakdown engine (add-ai-planning-core §6).
//!
//! The model only *extracts* facts (background, output, outcome, scope) into a
//! [`GoalBreakdown`]; every derived judgement — missing fields, the clarity
//! flags, refined titles — is computed here by deterministic, testable code
//! and never by the model (spec: goal-clarification, 提取优先于评价).
//!
//! Persistence contract: the breakdown lives in `tasks.goal_breakdown` as an
//! arbitrary JSON column, so [`GoalBreakdown::from_value`] is lenient —
//! missing parts, missing fields and wrong-typed leftovers degrade to `None`
//! instead of failing the row (historical data may be dirty). The merge
//! semantics of [`GoalBreakdownUpdate`] mirror the spec exactly: `null`
//! clears, a missing field keeps its value, and an update that addresses
//! nothing is rejected as `empty_update`.
//!
//! The `update_goal_breakdown` tool entry point and its `TaskContextSnapshot`
//! return value arrive with the tool-layer task (§6.5); everything it needs
//! to compute them lives here.

use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult};

// ---------------------------------------------------------------------------
// 6.1 Structure
// ---------------------------------------------------------------------------

/// The four-part structured form of one goal, stored as JSON in
/// `tasks.goal_breakdown` (spec: GoalBreakdown 结构).
///
/// Every part and field is optional; an all-empty breakdown is equivalent to
/// `None` (see [`GoalBreakdown::is_empty`]) and both parse back to
/// [`GoalBreakdown::default`]. The derived `Deserialize` expects well-formed
/// data — dirty rows must go through [`GoalBreakdown::from_value`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalBreakdown {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<ContextPart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<OutputPart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<OutcomePart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<ScopePart>,
}

/// Background facts: why the goal exists and who is involved.
/// `clarification` is the field the spec explicitly asks to collect first
/// (spec: 缺失字段计算).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextPart {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clarification: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stakeholders: Option<String>,
}

/// The concrete deliverable the goal produces.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputPart {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

/// The change the output brings plus how it can be verified.
/// `controlled_by_user: Some(false)` asks the agent to propose a wording the
/// user actually controls (spec: why → what → how 对话流程, 结果不由用户控制).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutcomePart {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controlled_by_user: Option<bool>,
}

/// Estimated effort and the user-confirmed decomposition state. The flag is
/// only ever set to `true` after the user confirmed the proposed steps and
/// they were written back to the task (spec: 清晰度标记推导, 分解已完成).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScopePart {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fully_decomposed: Option<bool>,
}

impl GoalBreakdown {
    /// Lenient parse of the arbitrary JSON stored in `tasks.goal_breakdown`.
    /// Anything that is not an object — or a part that is not an object, or a
    /// field whose type does not match — is ignored instead of erroring, so a
    /// dirty historical row can never panic a caller.
    pub fn from_value(value: &serde_json::Value) -> Self {
        let Some(object) = value.as_object() else {
            return Self::default();
        };
        Self {
            context: object.get("context").and_then(ContextPart::from_value),
            output: object.get("output").and_then(OutputPart::from_value),
            outcome: object.get("outcome").and_then(OutcomePart::from_value),
            scope: object.get("scope").and_then(ScopePart::from_value),
        }
    }

    /// Inverse of [`GoalBreakdown::from_value`]: parts and fields without a
    /// value are omitted entirely, so an empty breakdown serializes to `{}`.
    pub fn to_value(&self) -> serde_json::Value {
        let mut object = serde_json::Map::new();
        // Serialize eagerly so the four heterogeneous part types collapse into
        // one array of `Option<Result<Value, _>>>`.
        let parts = [
            (
                "context",
                self.context
                    .as_ref()
                    .filter(|part| !part.is_empty())
                    .map(serde_json::to_value),
            ),
            (
                "output",
                self.output
                    .as_ref()
                    .filter(|part| !part.is_empty())
                    .map(serde_json::to_value),
            ),
            (
                "outcome",
                self.outcome
                    .as_ref()
                    .filter(|part| !part.is_empty())
                    .map(serde_json::to_value),
            ),
            (
                "scope",
                self.scope
                    .as_ref()
                    .filter(|part| !part.is_empty())
                    .map(serde_json::to_value),
            ),
        ];
        for (key, part) in parts {
            // Serializing a plain struct of optional strings/bools cannot fail;
            // a failure would only mean skipping the part, never panicking.
            if let Some(Ok(value)) = part {
                object.insert(key.to_string(), value);
            }
        }
        serde_json::Value::Object(object)
    }

    /// Whether the breakdown carries no information at all. Such a value is
    /// treated the same as `None` everywhere in this module (spec:
    /// GoalBreakdown 结构 — 空结构与缺失等价).
    pub fn is_empty(&self) -> bool {
        self.context.is_none()
            && self.output.is_none()
            && self.outcome.is_none()
            && self.scope.is_none()
    }

    /// Drops parts whose fields are all empty so the None-equivalence above
    /// survives merges.
    fn normalize(&mut self) {
        if self.context.as_ref().map_or(true, ContextPart::is_empty) {
            self.context = None;
        }
        if self.output.as_ref().map_or(true, OutputPart::is_empty) {
            self.output = None;
        }
        if self.outcome.as_ref().map_or(true, OutcomePart::is_empty) {
            self.outcome = None;
        }
        if self.scope.as_ref().map_or(true, ScopePart::is_empty) {
            self.scope = None;
        }
    }
}

impl ContextPart {
    fn from_value(value: &serde_json::Value) -> Option<Self> {
        let object = value.as_object()?;
        let part = Self {
            clarification: optional_string(object, "clarification"),
            background: optional_string(object, "background"),
            stakeholders: optional_string(object, "stakeholders"),
        };
        (!part.is_empty()).then_some(part)
    }

    fn is_empty(&self) -> bool {
        self.clarification.is_none() && self.background.is_none() && self.stakeholders.is_none()
    }
}

impl OutputPart {
    fn from_value(value: &serde_json::Value) -> Option<Self> {
        let object = value.as_object()?;
        let part = Self {
            value: optional_string(object, "value"),
        };
        (!part.is_empty()).then_some(part)
    }

    fn is_empty(&self) -> bool {
        self.value.is_none()
    }
}

impl OutcomePart {
    fn from_value(value: &serde_json::Value) -> Option<Self> {
        let object = value.as_object()?;
        let part = Self {
            value: optional_string(object, "value"),
            verification_method: optional_string(object, "verification_method"),
            controlled_by_user: optional_bool(object, "controlled_by_user"),
        };
        (!part.is_empty()).then_some(part)
    }

    fn is_empty(&self) -> bool {
        self.value.is_none()
            && self.verification_method.is_none()
            && self.controlled_by_user.is_none()
    }
}

impl ScopePart {
    fn from_value(value: &serde_json::Value) -> Option<Self> {
        let object = value.as_object()?;
        let part = Self {
            effort: optional_string(object, "effort"),
            fully_decomposed: optional_bool(object, "fully_decomposed"),
        };
        (!part.is_empty()).then_some(part)
    }

    fn is_empty(&self) -> bool {
        self.effort.is_none() && self.fully_decomposed.is_none()
    }
}

/// Reads one optional string field; wrong-typed leftovers degrade to `None`.
fn optional_string(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    object
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

/// Reads one optional bool field; wrong-typed leftovers degrade to `None`.
fn optional_bool(object: &serde_json::Map<String, serde_json::Value>, key: &str) -> Option<bool> {
    object.get(key).and_then(serde_json::Value::as_bool)
}

/// A stored string only counts as present when it has non-blank content, so
/// whitespace leftovers cannot satisfy a field (spec: 缺失字段计算).
fn has_text(value: &Option<String>) -> bool {
    value.as_deref().map_or(false, |s| !s.trim().is_empty())
}

// ---------------------------------------------------------------------------
// 6.2 Merge semantics
// ---------------------------------------------------------------------------

/// A partial update of a [`GoalBreakdown`] as submitted by the agent
/// (spec: GoalBreakdown 结构 — 部分更新).
///
/// Field shape is `Option<Option<T>>`: the outer `None` means "not mentioned
/// this time" (keep), `Some(None)` is an explicit `null` (clear) and
/// `Some(Some(v))` sets a value. A whole part set to `null` (`Some(None)` on
/// the part field) clears that part. The derived `Serialize` reproduces the
/// same wire form, so updates round-trip through JSON.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GoalBreakdownUpdate {
    #[serde(
        default,
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub context: Option<Option<ContextUpdate>>,
    #[serde(
        default,
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub output: Option<Option<OutputUpdate>>,
    #[serde(
        default,
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub outcome: Option<Option<OutcomeUpdate>>,
    #[serde(
        default,
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub scope: Option<Option<ScopeUpdate>>,
}

/// Field-level update for [`ContextPart`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextUpdate {
    #[serde(
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub clarification: Option<Option<String>>,
    #[serde(
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub background: Option<Option<String>>,
    #[serde(
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub stakeholders: Option<Option<String>>,
}

/// Field-level update for [`OutputPart`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputUpdate {
    #[serde(
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub value: Option<Option<String>>,
}

/// Field-level update for [`OutcomePart`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutcomeUpdate {
    #[serde(
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub value: Option<Option<String>>,
    #[serde(
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub verification_method: Option<Option<String>>,
    #[serde(
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub controlled_by_user: Option<Option<bool>>,
}

/// Field-level update for [`ScopePart`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ScopeUpdate {
    #[serde(
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub effort: Option<Option<String>>,
    #[serde(
        deserialize_with = "deserialize_some",
        skip_serializing_if = "Option::is_none"
    )]
    pub fully_decomposed: Option<Option<bool>>,
}

/// Deserializes `T` wrapped in a second `Option` so the two "absent" states
/// stay distinguishable: a missing field never reaches this function (the
/// container `#[serde(default)]` yields `None` = keep), while an explicit
/// `null` deserializes to `Some(None)` = clear (spec: GoalBreakdown 结构 —
/// null 表示清除、缺省表示保留).
fn deserialize_some<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Deserialize::deserialize(deserializer).map(Some)
}

/// Merges one submitted update onto the current breakdown (task §6.2).
///
/// Rules (spec: GoalBreakdown 结构):
/// - a mentioned field is set to its value, `null` clears it;
/// - fields/parts not mentioned are kept exactly as they are;
/// - a whole part submitted as `null` clears that part.
///
/// An update that does not address any field at all carries no valid change
/// and is rejected with the `empty_update` error code (spec: 空更新被拒绝).
/// Addressing a field — even with a value identical to the current one or a
/// clear of an already-empty field — counts as a change: re-confirming a
/// value must not fail the agent's turn.
pub fn merge(base: &GoalBreakdown, update: &GoalBreakdownUpdate) -> AppResult<GoalBreakdown> {
    let mut merged = base.clone();
    let mut changed = false;

    if let Some(context) = &update.context {
        match context {
            None => {
                merged.context = None;
                changed = true;
            }
            Some(part) => {
                let target = merged.context.get_or_insert_with(ContextPart::default);
                if let Some(value) = &part.clarification {
                    changed = true;
                    target.clarification = value.clone();
                }
                if let Some(value) = &part.background {
                    changed = true;
                    target.background = value.clone();
                }
                if let Some(value) = &part.stakeholders {
                    changed = true;
                    target.stakeholders = value.clone();
                }
            }
        }
    }

    if let Some(output) = &update.output {
        match output {
            None => {
                merged.output = None;
                changed = true;
            }
            Some(part) => {
                if let Some(value) = &part.value {
                    changed = true;
                    merged.output.get_or_insert_with(OutputPart::default).value = value.clone();
                }
            }
        }
    }

    if let Some(outcome) = &update.outcome {
        match outcome {
            None => {
                merged.outcome = None;
                changed = true;
            }
            Some(part) => {
                let target = merged.outcome.get_or_insert_with(OutcomePart::default);
                if let Some(value) = &part.value {
                    changed = true;
                    target.value = value.clone();
                }
                if let Some(value) = &part.verification_method {
                    changed = true;
                    target.verification_method = value.clone();
                }
                if let Some(value) = &part.controlled_by_user {
                    changed = true;
                    target.controlled_by_user = *value;
                }
            }
        }
    }

    if let Some(scope) = &update.scope {
        match scope {
            None => {
                merged.scope = None;
                changed = true;
            }
            Some(part) => {
                let target = merged.scope.get_or_insert_with(ScopePart::default);
                if let Some(value) = &part.effort {
                    changed = true;
                    target.effort = value.clone();
                }
                if let Some(value) = &part.fully_decomposed {
                    changed = true;
                    target.fully_decomposed = *value;
                }
            }
        }
    }

    if !changed {
        return Err(AppError::validation(
            "empty_update",
            "the update carries no field change; send a value, or null to clear a field",
        ));
    }

    // Mentioning nothing but empty part objects (`{"context": {}}`) must not
    // materialize empty parts in the result.
    merged.normalize();
    Ok(merged)
}

// ---------------------------------------------------------------------------
// 6.3 Missing fields
// ---------------------------------------------------------------------------

/// One field the goal is still missing, used to guide the agent's next
/// question (spec: 缺失字段计算). Missing fields prompt but never gate: the
/// agent may skip a field that genuinely does not apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MissingField {
    ContextClarification,
    ContextBackground,
    ContextStakeholders,
    OutputValue,
    OutcomeValue,
    OutcomeVerificationMethod,
    ScopeEffort,
}

impl MissingField {
    /// Dotted path of the field inside the stored JSON, so the model knows
    /// where to write the answer back.
    pub fn field_path(self) -> &'static str {
        match self {
            Self::ContextClarification => "context.clarification",
            Self::ContextBackground => "context.background",
            Self::ContextStakeholders => "context.stakeholders",
            Self::OutputValue => "output.value",
            Self::OutcomeValue => "outcome.value",
            Self::OutcomeVerificationMethod => "outcome.verification_method",
            Self::ScopeEffort => "scope.effort",
        }
    }

    /// Short human-readable name for UI or prompt rendering.
    pub fn label(self) -> &'static str {
        match self {
            Self::ContextClarification => "目标澄清",
            Self::ContextBackground => "背景信息",
            Self::ContextStakeholders => "干系人",
            Self::OutputValue => "产出物",
            Self::OutcomeValue => "预期结果",
            Self::OutcomeVerificationMethod => "验证方式",
            Self::ScopeEffort => "工作量",
        }
    }

    /// Question hint for the agent. The agent re-expresses the hint in the
    /// user's language (spec: 不编造信息 — 保持用户语言) and never fills an
    /// answer in itself.
    pub fn prompt_hint(self) -> &'static str {
        match self {
            Self::ContextClarification => {
                "这个目标为什么模糊？先问清它到底想解决什么问题（写入 context.clarification）。"
            }
            Self::ContextBackground => "这个目标是在什么背景下产生的？（写入 context.background）",
            Self::ContextStakeholders => {
                "谁会受这个目标影响，或者需要谁参与？（写入 context.stakeholders）"
            }
            Self::OutputValue => "完成这个目标时会产出什么具体的东西或成果？（写入 output.value）",
            Self::OutcomeValue => "产出之后，期望带来什么可感知的改变？（写入 outcome.value）",
            Self::OutcomeVerificationMethod => {
                "怎么验证这个结果确实发生了？（写入 outcome.verification_method）"
            }
            Self::ScopeEffort => "大概需要投入多少时间或精力？（写入 scope.effort）",
        }
    }
}

/// Computes the missing-field set in a fixed order (context → output →
/// outcome → scope), so equal inputs yield equal vectors (spec:
/// 提取优先于评价 — 同一输入得到同一结论). A freshly created goal misses all
/// listed fields.
pub fn missing_fields(breakdown: &GoalBreakdown) -> Vec<MissingField> {
    let mut missing = Vec::new();

    let context = breakdown.context.as_ref();
    if !context.map_or(false, |part| has_text(&part.clarification)) {
        missing.push(MissingField::ContextClarification);
    }
    if !context.map_or(false, |part| has_text(&part.background)) {
        missing.push(MissingField::ContextBackground);
    }
    if !context.map_or(false, |part| has_text(&part.stakeholders)) {
        missing.push(MissingField::ContextStakeholders);
    }
    if !breakdown
        .output
        .as_ref()
        .map_or(false, |part| has_text(&part.value))
    {
        missing.push(MissingField::OutputValue);
    }
    if !breakdown
        .outcome
        .as_ref()
        .map_or(false, |part| has_text(&part.value))
    {
        missing.push(MissingField::OutcomeValue);
    }
    if !breakdown
        .outcome
        .as_ref()
        .map_or(false, |part| has_text(&part.verification_method))
    {
        missing.push(MissingField::OutcomeVerificationMethod);
    }
    if !breakdown
        .scope
        .as_ref()
        .map_or(false, |part| has_text(&part.effort))
    {
        missing.push(MissingField::ScopeEffort);
    }

    missing
}

// ---------------------------------------------------------------------------
// 6.4 Clarity derivation
// ---------------------------------------------------------------------------

/// The clarity flags as computed from a breakdown alone (spec: 清晰度标记推导).
///
/// `needs_breakdown` is deliberately **not** decided here: it may only be
/// cleared by the tool after the user confirmed the proposed steps and they
/// were written back to the task, and it stays `true` while the decomposition
/// is unconfirmed. Callers use [`ClarityDerivation::decomposition_confirmed`]
/// to decide that transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClarityDerivation {
    /// Suggested value for the `tasks.needs_refinement` flag: `true` unless
    /// the breakdown already supports a precise title. The caller maps this
    /// onto the tri-state column (`None` = never evaluated is a storage state
    /// only and is never produced here).
    pub needs_refinement: bool,
    /// The breakdown alone is enough to support a precise title — the exact
    /// condition under which `needs_refinement` may be cleared.
    pub title_ready: bool,
    /// `scope.fully_decomposed == Some(true)`. Only then may the tool clear
    /// `needs_breakdown` (spec: 分解已完成); model-proposed steps alone never
    /// clear it (spec: 未确认分解).
    pub decomposition_confirmed: bool,
}

/// Derives the clarity flags from a breakdown. Pure function: the same input
/// always yields the same result, independent of any model output (spec:
/// 清晰度由代码计算).
///
/// `title_ready` requires all three title-supporting parts at once:
/// context present (`clarification` or `background` has content), plus
/// `output.value`, plus `outcome.value`.
pub fn derive_clarity(breakdown: &GoalBreakdown) -> ClarityDerivation {
    let context_ready = breakdown.context.as_ref().map_or(false, |part| {
        has_text(&part.clarification) || has_text(&part.background)
    });
    let output_ready = breakdown
        .output
        .as_ref()
        .map_or(false, |part| has_text(&part.value));
    let outcome_ready = breakdown
        .outcome
        .as_ref()
        .map_or(false, |part| has_text(&part.value));
    let title_ready = context_ready && output_ready && outcome_ready;

    ClarityDerivation {
        needs_refinement: !title_ready,
        title_ready,
        decomposition_confirmed: breakdown
            .scope
            .as_ref()
            .and_then(|part| part.fully_decomposed)
            == Some(true),
    }
}

// ---------------------------------------------------------------------------
// Title refinement (spec: 标题精炼)
// ---------------------------------------------------------------------------

/// Which of the four title situations a goal is in (spec: 标题精炼).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitleShape {
    /// Neither output nor outcome: keep the current title, context only
    /// supplements the conversation.
    NoOutputNoOutcome,
    /// Only `output.value`: title takes "action verb + output".
    OutputOnly,
    /// Only `outcome.value`: title takes "action on the output + to + outcome",
    /// with the current title standing in for the missing output.
    OutcomeOnly,
    /// Both present: passive output turned into an action, joined with the
    /// outcome ("signed agreement" → "Sign agreement to …").
    OutputAndOutcome,
}

/// Classifies the title situation from the breakdown alone. An output or
/// outcome counts only when its value is non-blank.
pub fn classify_title_shape(breakdown: &GoalBreakdown) -> TitleShape {
    let has_output = breakdown
        .output
        .as_ref()
        .map_or(false, |part| has_text(&part.value));
    let has_outcome = breakdown
        .outcome
        .as_ref()
        .map_or(false, |part| has_text(&part.value));
    match (has_output, has_outcome) {
        (false, false) => TitleShape::NoOutputNoOutcome,
        (true, false) => TitleShape::OutputOnly,
        (false, true) => TitleShape::OutcomeOnly,
        (true, true) => TitleShape::OutputAndOutcome,
    }
}

/// Builds a refined title from the current one plus the breakdown, or `None`
/// when the current title should be kept.
///
/// This is a **deterministic heuristic** helper for the REFINE_TITLE stage
/// (spec: 标题精炼): it only reshapes strings the user or model already
/// produced and never invents content (spec: 不编造信息). In particular it
/// returns `None` instead of fabricating a verb for a bare noun-phrase output.
/// The model may always propose a better title directly through the title
/// update tool; this function only covers the mechanical cases.
///
/// The connector follows the leading (action) clause's language — CJK clauses
/// join with「，以」, everything else with " to " — so a Chinese goal stays
/// Chinese (spec: 保持用户语言).
pub fn refined_title(current: &str, breakdown: &GoalBreakdown) -> Option<String> {
    let output = breakdown
        .output
        .as_ref()
        .and_then(|part| non_blank(&part.value));
    let outcome = breakdown
        .outcome
        .as_ref()
        .and_then(|part| non_blank(&part.value));

    match (output, outcome) {
        // 既无产出也无结果：保留原标题，context 只作补充说明。
        (None, None) => None,
        // 只有产出：动作动词 + 产出物；没有可复用的动词则保留原标题。
        (Some(output), None) => to_active_phrase(output),
        // 只有结果：当前标题充当产出位 + to/for + 结果。
        (None, Some(outcome)) => combine_with_current_title(current, outcome),
        // 都有：被动产出改主动 + to/for + 结果；改不动（纯名词）则退回当前标题。
        (Some(output), Some(outcome)) => match to_active_phrase(output) {
            Some(action) => Some(join_action_and_outcome(&action, outcome)),
            None => combine_with_current_title(current, outcome),
        },
    }
}

fn non_blank(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

fn combine_with_current_title(current: &str, outcome: &str) -> Option<String> {
    let action = current.trim();
    if action.is_empty() {
        return None;
    }
    Some(join_action_and_outcome(action, outcome))
}

fn join_action_and_outcome(action: &str, outcome: &str) -> String {
    if is_cjk(action) {
        format!("{action}，以{outcome}")
    } else {
        format!("{action} to {outcome}")
    }
}

/// Whether the string starts with a CJK ideograph; decides which connector
/// and which verb-reshaping rules apply.
fn is_cjk(s: &str) -> bool {
    matches!(
        s.chars().next(),
        Some(c) if matches!(c as u32,
            0x3400..=0x4DBF       // CJK Unified Ideographs Extension A
            | 0x4E00..=0x9FFF     // CJK Unified Ideographs
            | 0xF900..=0xFAFF     // CJK Compatibility Ideographs
            | 0x20000..=0x2A6DF   // Extension B
        )
    )
}

/// Turns an output phrase into its active/imperative reading, or `None` when
/// no deterministic rule applies (bare noun phrases are left alone rather
/// than getting an invented verb).
///
/// Heuristics, on purpose:
/// - English: a leading past/participle form is converted via a small
///   irregular table, else by stripping a trailing `-ed`/`-ied`; a leading
///   verb from [`IMPERATIVE_VERBS`] means the phrase already reads like an
///   action and is returned unchanged.
/// - Chinese: a leading「已」is dropped, an aspect「了」right after a leading
///   verb from [`CJK_ACTION_VERBS`] is dropped, and anything already starting
///   with one of those verbs is returned unchanged.
fn to_active_phrase(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if is_cjk(trimmed) {
        to_active_phrase_zh(trimmed)
    } else {
        to_active_phrase_en(trimmed)
    }
}

fn to_active_phrase_en(trimmed: &str) -> Option<String> {
    let (first, rest) = match trimmed.split_once(char::is_whitespace) {
        Some((first, rest)) => (first, Some(rest)),
        None => (trimmed, None),
    };
    let lower = first.to_ascii_lowercase();

    // Already an action: keep the user's phrasing untouched.
    if IMPERATIVE_VERBS.contains(&lower.as_str()) {
        return Some(trimmed.to_string());
    }
    if let Some((_, base)) = IRREGULAR_PAST.iter().find(|(past, _)| *past == lower) {
        return Some(capitalize_first(&join_verb_and_rest(base, rest)));
    }
    if let Some(stem) = regular_past_stem(&lower) {
        return Some(capitalize_first(&join_verb_and_rest(&stem, rest)));
    }
    None
}

fn to_active_phrase_zh(trimmed: &str) -> Option<String> {
    // 「已签署合同」→「签署合同」: a leading 已 is perfective, not content.
    let without_perfective = trimmed.strip_prefix('已').unwrap_or(trimmed);
    for verb in CJK_ACTION_VERBS {
        if let Some(rest) = without_perfective.strip_prefix(verb) {
            // 「完成了初稿」→「完成初稿」: 了 right after the verb is aspect.
            let rest = rest.strip_prefix('了').unwrap_or(rest);
            return Some(format!("{verb}{rest}"));
        }
    }
    None
}

/// Regular-verb stem for a lowercase word ending in `-ed`/`-ied`
/// ("fixed" → "fix", "studied" → "study"). Minimum-length guards keep short
/// non-verbs ("red") out of the rewrite.
fn regular_past_stem(word: &str) -> Option<String> {
    if let Some(stem) = word.strip_suffix("ied") {
        if stem.len() >= 2 {
            return Some(format!("{stem}y"));
        }
    }
    if let Some(stem) = word.strip_suffix("ed") {
        if stem.len() >= 3 {
            return Some(stem.to_string());
        }
    }
    None
}

fn join_verb_and_rest(verb: &str, rest: Option<&str>) -> String {
    match rest {
        Some(rest) => format!("{verb} {rest}"),
        None => verb.to_string(),
    }
}

fn capitalize_first(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Past/participle forms whose active form is not plain `-ed` stripping
/// (irregular or e-dropping verbs). Kept deliberately small; the regular
/// `-ed` rule covers the rest. (spec: 标题精炼 — "signed agreement" →
/// "Sign agreement")
const IRREGULAR_PAST: &[(&str, &str)] = &[
    ("signed", "sign"),
    ("drafted", "draft"),
    ("written", "write"),
    ("wrote", "write"),
    ("reviewed", "review"),
    ("agreed", "agree"),
    ("completed", "complete"),
    ("published", "publish"),
    ("delivered", "deliver"),
    ("used", "use"),
    ("moved", "move"),
    ("saved", "save"),
    ("shared", "share"),
    ("closed", "close"),
    ("approved", "approve"),
    ("scheduled", "schedule"),
    ("organized", "organize"),
    ("finalized", "finalize"),
    ("filed", "file"),
    ("scaled", "scale"),
    ("hired", "hire"),
    ("raised", "raise"),
    ("named", "name"),
    ("based", "base"),
    ("sent", "send"),
    ("built", "build"),
    ("bought", "buy"),
    ("made", "make"),
    ("got", "get"),
];

/// English verbs that make a phrase already read like a controllable action;
/// a leading hit returns the phrase unchanged instead of reshaping it.
const IMPERATIVE_VERBS: &[&str] = &[
    "sign",
    "draft",
    "write",
    "review",
    "agree",
    "complete",
    "publish",
    "deliver",
    "finish",
    "send",
    "create",
    "build",
    "ship",
    "release",
    "schedule",
    "book",
    "buy",
    "submit",
    "apply",
    "read",
    "reply",
    "email",
    "call",
    "meet",
    "plan",
    "prepare",
    "update",
    "fix",
    "test",
    "deploy",
    "clean",
    "organize",
    "finalize",
    "confirm",
    "check",
    "verify",
    "set",
    "add",
    "remove",
    "migrate",
    "implement",
    "design",
    "choose",
    "decide",
    "collect",
    "gather",
    "clarify",
    "define",
    "order",
    "purchase",
    "install",
    "configure",
    "run",
    "start",
    "hold",
    "attend",
    "file",
    "register",
    "renew",
    "cancel",
    "move",
    "rename",
    "refactor",
    "document",
    "record",
    "summarize",
    "outline",
    "edit",
    "revise",
    "print",
    "scan",
    "backup",
    "archive",
    "negotiate",
    "contact",
    "resolve",
    "handle",
    "get",
    "make",
    "take",
    "give",
];

/// Chinese action verbs used to recognise an already-active phrase. Two-char
/// forms must come before their one-char prefixes (「签署」 before 「签」).
const CJK_ACTION_VERBS: &[&str] = &[
    "完成", "写完", "读完", "学完", "签署", "发布", "交付", "提交", "审核", "评审", "购买", "预约",
    "安排", "准备", "整理", "确认", "检查", "测试", "部署", "修复", "更新", "创建", "建立", "制定",
    "收集", "澄清", "定义", "决定", "选择", "注册", "续费", "取消", "迁移", "归档", "备份", "打印",
    "扫描", "回复", "联系", "沟通", "学习", "申请", "办理", "缴纳", "支付", "装修", "设计", "采购",
    "招聘", "预订", "登记", "结算", "对账", "清点", "盘点", "排查", "解决", "处理",
    // One-char verbs last so they never shadow a two-char form above.
    "写", "签", "发", "买", "审", "学", "读", "修", "装", "搬", "卖", "租", "退", "办", "缴", "付",
    "还", "存", "汇", "订",
];

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn context_part(clarification: &str, background: &str) -> ContextPart {
        ContextPart {
            clarification: Some(clarification.into()),
            background: Some(background.into()),
            stakeholders: None,
        }
    }

    fn full_base() -> GoalBreakdown {
        GoalBreakdown {
            context: Some(context_part("why now", "bg")),
            output: Some(OutputPart {
                value: Some("signed agreement".into()),
            }),
            outcome: Some(OutcomePart {
                value: Some("payment".into()),
                verification_method: Some("bank statement".into()),
                controlled_by_user: Some(true),
            }),
            scope: Some(ScopePart {
                effort: Some("two weeks".into()),
                fully_decomposed: None,
            }),
        }
    }

    fn update_from(json: serde_json::Value) -> GoalBreakdownUpdate {
        serde_json::from_value(json).expect("update parses")
    }

    // ------------------------------------------------------------------
    // 6.1 structure and tolerant parsing
    // ------------------------------------------------------------------

    #[test]
    fn from_value_tolerates_arbitrary_json() {
        // Not objects at all: every shape degrades to the empty breakdown.
        for dirty in [
            json!(["a", "b"]),
            json!("a plain string"),
            json!(42),
            json!(null),
            json!(true),
        ] {
            let parsed = GoalBreakdown::from_value(&dirty);
            assert!(parsed.is_empty(), "must degrade to empty: {dirty}");
            assert_eq!(parsed, GoalBreakdown::default());
        }

        // Wrong-typed parts and fields are ignored instead of erroring.
        let parsed = GoalBreakdown::from_value(&json!({
            "context": "not an object",
            "output": { "value": 123 },
            "outcome": { "value": "result", "controlled_by_user": "yes" },
            "scope": { "fully_decomposed": null, "effort": [] }
        }));
        assert_eq!(parsed.context, None);
        assert_eq!(parsed.output, None);
        assert_eq!(
            parsed.outcome,
            Some(OutcomePart {
                value: Some("result".into()),
                verification_method: None,
                controlled_by_user: None,
            })
        );
        assert_eq!(parsed.scope, None);
    }

    #[test]
    fn to_value_omits_empty_parts_and_round_trips() {
        let empty = GoalBreakdown::default();
        assert_eq!(empty.to_value(), json!({}));
        assert!(GoalBreakdown::from_value(&json!({ "context": {} })).is_empty());

        let breakdown = full_base();
        let value = breakdown.to_value();
        assert_eq!(value["output"]["value"], "signed agreement");
        assert!(value.get("scope").is_some());
        assert_eq!(GoalBreakdown::from_value(&value), breakdown);

        // The derived serializer agrees with the manual one on real data.
        assert_eq!(serde_json::to_value(&breakdown).unwrap(), value);
    }

    // ------------------------------------------------------------------
    // 6.2 merge semantics
    // ------------------------------------------------------------------

    #[test]
    fn update_deserialization_separates_null_missing_and_value() {
        let update = update_from(json!({
            "context": { "clarification": null, "background": "bg" }
        }));
        // The serialized form reproduces the same wire shape.
        let encoded = serde_json::to_value(&update).unwrap();
        assert_eq!(
            encoded,
            json!({ "context": { "clarification": null, "background": "bg" } })
        );
        assert_eq!(update_from(encoded), update);

        let context = update
            .context
            .expect("context mentioned")
            .expect("part update object");
        assert_eq!(context.clarification, Some(None), "null means clear");
        assert_eq!(context.background, Some(Some("bg".into())));
        assert_eq!(context.stakeholders, None, "missing means keep");
        assert!(update.output.is_none() && update.outcome.is_none() && update.scope.is_none());
    }

    #[test]
    fn partial_update_keeps_other_fields() {
        let base = full_base();
        let merged = merge(
            &base,
            &update_from(json!({ "output": { "value": "new output" } })),
        )
        .expect("non-empty update");

        assert_eq!(merged.context, base.context, "context untouched");
        assert_eq!(merged.outcome, base.outcome, "outcome untouched");
        assert_eq!(merged.scope, base.scope, "scope untouched");
        assert_eq!(
            merged.output.expect("output set").value.as_deref(),
            Some("new output")
        );
    }

    #[test]
    fn explicit_null_clears_a_single_field() {
        let base = full_base();
        let merged = merge(
            &base,
            &update_from(json!({ "context": { "clarification": null } })),
        )
        .expect("non-empty update");

        let context = merged.context.expect("other context fields survive");
        assert_eq!(context.clarification, None, "nulled field is cleared");
        assert_eq!(context.background.as_deref(), Some("bg"));
        assert_eq!(merged.output, base.output);
        assert_eq!(merged.outcome, base.outcome);
    }

    #[test]
    fn part_level_null_clears_the_whole_part() {
        let base = full_base();
        let merged =
            merge(&base, &update_from(json!({ "scope": null }))).expect("non-empty update");

        assert_eq!(merged.scope, None, "whole part cleared");
        assert_eq!(merged.context, base.context);
        assert_eq!(merged.output, base.output);

        // A bool field round-trips the same way: set then clear.
        let merged = merge(
            &merged,
            &update_from(json!({ "scope": { "fully_decomposed": true } })),
        )
        .unwrap();
        assert_eq!(
            merged.scope.expect("scope set").fully_decomposed,
            Some(true)
        );
    }

    #[test]
    fn empty_update_is_rejected_with_empty_update_code() {
        let base = full_base();
        for update in [
            json!({}),
            json!({ "output": {} }),
            json!({ "context": {}, "scope": {} }),
        ] {
            let error = merge(&base, &update_from(update))
                .expect_err("update without any addressed field must fail");
            match error {
                AppError::Validation { code, .. } => assert_eq!(code, "empty_update"),
                other => panic!("expected validation error, got {other:?}"),
            }
        }
        // Rejections leave the base untouched (the result was never returned).
        assert_eq!(
            base.output.expect("base intact").value.as_deref(),
            Some("signed agreement")
        );
    }

    // ------------------------------------------------------------------
    // 6.3 missing fields
    // ------------------------------------------------------------------

    #[test]
    fn fresh_goal_misses_every_major_field() {
        let missing = missing_fields(&GoalBreakdown::default());
        for field in [
            MissingField::ContextClarification,
            MissingField::OutputValue,
            MissingField::OutcomeValue,
            MissingField::OutcomeVerificationMethod,
        ] {
            assert!(missing.contains(&field), "fresh goal must miss {field:?}");
        }
        // Everything this engine tracks is missing on a bare title.
        assert_eq!(missing.len(), 7);
    }

    #[test]
    fn missing_fields_shrink_in_fixed_order_and_blank_strings_count_as_missing() {
        let mut breakdown = GoalBreakdown {
            context: Some(ContextPart {
                clarification: Some("  ".into()), // blank: still missing
                background: Some("bg".into()),
                stakeholders: None,
            }),
            ..GoalBreakdown::default()
        };
        let missing = missing_fields(&breakdown);
        assert_eq!(
            missing,
            vec![
                MissingField::ContextClarification,
                MissingField::ContextStakeholders,
                MissingField::OutputValue,
                MissingField::OutcomeValue,
                MissingField::OutcomeVerificationMethod,
                MissingField::ScopeEffort,
            ]
        );

        let context = breakdown.context.as_mut().expect("context set above");
        context.clarification = Some("why".into());
        context.stakeholders = Some("vendor".into());
        breakdown.output = Some(OutputPart {
            value: Some("thing".into()),
        });
        breakdown.outcome = Some(OutcomePart {
            value: Some("change".into()),
            verification_method: Some("how".into()),
            controlled_by_user: Some(true),
        });
        breakdown.scope = Some(ScopePart {
            effort: Some("1 day".into()),
            fully_decomposed: None,
        });
        assert!(missing_fields(&breakdown).is_empty(), "all filled");
    }

    #[test]
    fn missing_fields_are_deterministic() {
        let breakdown = full_base();
        assert_eq!(missing_fields(&breakdown), missing_fields(&breakdown));
    }

    // ------------------------------------------------------------------
    // 6.4 clarity derivation
    // ------------------------------------------------------------------

    #[test]
    fn three_parts_complete_allow_clearing_needs_refinement() {
        let breakdown = GoalBreakdown {
            context: Some(context_part("why", "bg")),
            output: Some(OutputPart {
                value: Some("out".into()),
            }),
            outcome: Some(OutcomePart {
                value: Some("result".into()),
                ..OutcomePart::default()
            }),
            scope: None,
        };
        let derived = derive_clarity(&breakdown);
        assert!(derived.title_ready);
        assert!(!derived.needs_refinement, "may be cleared now");
        assert!(!derived.decomposition_confirmed);
    }

    #[test]
    fn incomplete_parts_keep_needs_refinement() {
        // Context with only stakeholders does not count as "high".
        let stakeholders_only = GoalBreakdown {
            context: Some(ContextPart {
                clarification: None,
                background: None,
                stakeholders: Some("vendor".into()),
            }),
            output: Some(OutputPart {
                value: Some("out".into()),
            }),
            outcome: Some(OutcomePart {
                value: Some("result".into()),
                ..OutcomePart::default()
            }),
            scope: None,
        };
        assert!(derive_clarity(&stakeholders_only).needs_refinement);

        // Background alone does count as context, but a missing outcome does not.
        let no_outcome = GoalBreakdown {
            context: Some(context_part("why", "")),
            output: Some(OutputPart {
                value: Some("out".into()),
            }),
            outcome: None,
            scope: None,
        };
        let derived = derive_clarity(&no_outcome);
        assert!(derived.needs_refinement);
        assert!(!derived.title_ready);
    }

    #[test]
    fn decomposition_confirmed_follows_only_an_explicit_true() {
        let confirmed = GoalBreakdown {
            scope: Some(ScopePart {
                effort: None,
                fully_decomposed: Some(true),
            }),
            ..GoalBreakdown::default()
        };
        assert!(derive_clarity(&confirmed).decomposition_confirmed);

        for flag in [None, Some(false)] {
            let pending = GoalBreakdown {
                scope: Some(ScopePart {
                    effort: None,
                    fully_decomposed: flag,
                }),
                ..GoalBreakdown::default()
            };
            assert!(
                !derive_clarity(&pending).decomposition_confirmed,
                "unconfirmed decomposition must not clear needs_breakdown: {flag:?}"
            );
        }
    }

    #[test]
    fn same_input_yields_same_derivation() {
        let breakdown = full_base();
        let first = derive_clarity(&breakdown);
        let second = derive_clarity(&GoalBreakdown::from_value(&breakdown.to_value()));
        assert_eq!(first, second, "re-parsed input must evaluate identically");
    }

    // ------------------------------------------------------------------
    // Title refinement: the four shapes (spec: 标题精炼)
    // ------------------------------------------------------------------

    #[test]
    fn title_shapes_are_classified_by_presence() {
        let empty = GoalBreakdown::default();
        assert_eq!(classify_title_shape(&empty), TitleShape::NoOutputNoOutcome);

        let output_only = GoalBreakdown {
            output: Some(OutputPart {
                value: Some("signed agreement".into()),
            }),
            ..GoalBreakdown::default()
        };
        assert_eq!(classify_title_shape(&output_only), TitleShape::OutputOnly);

        let outcome_only = GoalBreakdown {
            outcome: Some(OutcomePart {
                value: Some("get paid".into()),
                ..OutcomePart::default()
            }),
            ..GoalBreakdown::default()
        };
        assert_eq!(classify_title_shape(&outcome_only), TitleShape::OutcomeOnly);

        assert_eq!(
            classify_title_shape(&full_base()),
            TitleShape::OutputAndOutcome
        );
    }

    #[test]
    fn no_output_and_no_outcome_keeps_current_title() {
        assert_eq!(
            refined_title("模糊的想法", &GoalBreakdown::default()),
            None,
            "context only supplements the conversation, never the title"
        );
    }

    #[test]
    fn output_only_turns_participle_into_action() {
        let breakdown = GoalBreakdown {
            output: Some(OutputPart {
                value: Some("signed agreement".into()),
            }),
            ..GoalBreakdown::default()
        };
        assert_eq!(
            refined_title("agreement thing", &breakdown),
            Some("Sign agreement".into())
        );

        // Regular -ed and Chinese 已/了 rewrites.
        let drafted = GoalBreakdown {
            output: Some(OutputPart {
                value: Some("drafted proposal".into()),
            }),
            ..GoalBreakdown::default()
        };
        assert_eq!(
            refined_title("old", &drafted),
            Some("Draft proposal".into())
        );
        let chinese = GoalBreakdown {
            output: Some(OutputPart {
                value: Some("已完成初稿".into()),
            }),
            ..GoalBreakdown::default()
        };
        assert_eq!(refined_title("初稿", &chinese), Some("完成初稿".into()));

        // An already-active output is kept verbatim; a bare noun phrase is
        // left alone (no invented verb).
        let active = GoalBreakdown {
            output: Some(OutputPart {
                value: Some("Write chapter 1".into()),
            }),
            ..GoalBreakdown::default()
        };
        assert_eq!(
            refined_title("chapter", &active),
            Some("Write chapter 1".into())
        );
        let noun = GoalBreakdown {
            output: Some(OutputPart {
                value: Some("rental agreement".into()),
            }),
            ..GoalBreakdown::default()
        };
        assert_eq!(refined_title("housing", &noun), None);
    }

    #[test]
    fn outcome_only_uses_current_title_as_the_action() {
        let english = GoalBreakdown {
            outcome: Some(OutcomePart {
                value: Some("get the deposit back".into()),
                ..OutcomePart::default()
            }),
            ..GoalBreakdown::default()
        };
        assert_eq!(
            refined_title("Negotiate with vendor", &english),
            Some("Negotiate with vendor to get the deposit back".into())
        );

        // 保持用户语言: a Chinese title joins with「，以」.
        let chinese = GoalBreakdown {
            outcome: Some(OutcomePart {
                value: Some("拿回押金".into()),
                ..OutcomePart::default()
            }),
            ..GoalBreakdown::default()
        };
        assert_eq!(
            refined_title("和房东谈判", &chinese),
            Some("和房东谈判，以拿回押金".into())
        );

        // No current title to stand in for the output: keep it.
        assert_eq!(refined_title("   ", &english), None);
    }

    #[test]
    fn output_and_outcome_combines_active_output_with_result() {
        // The spec example: passive output becomes the action.
        assert_eq!(
            refined_title(
                "agreement",
                &GoalBreakdown {
                    context: None,
                    output: Some(OutputPart {
                        value: Some("signed agreement".into()),
                    }),
                    outcome: Some(OutcomePart {
                        value: Some("receive the payment".into()),
                        ..OutcomePart::default()
                    }),
                    scope: None,
                }
            ),
            Some("Sign agreement to receive the payment".into())
        );

        // Chinese output and outcome stay Chinese.
        assert_eq!(
            refined_title(
                "合作",
                &GoalBreakdown {
                    context: None,
                    output: Some(OutputPart {
                        value: Some("已签合同".into()),
                    }),
                    outcome: Some(OutcomePart {
                        value: Some("收到尾款".into()),
                        ..OutcomePart::default()
                    }),
                    scope: None,
                }
            ),
            Some("签合同，以收到尾款".into())
        );

        // A bare-noun output falls back to the current title as the action.
        assert_eq!(
            refined_title(
                "finalize vendor deal",
                &GoalBreakdown {
                    context: None,
                    output: Some(OutputPart {
                        value: Some("agreement".into()),
                    }),
                    outcome: Some(OutcomePart {
                        value: Some("payment".into()),
                        ..OutcomePart::default()
                    }),
                    scope: None,
                }
            ),
            Some("finalize vendor deal to payment".into())
        );
    }
}

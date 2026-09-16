# Hyperfocus 0.15.0 — Rust backend internals (reverse-engineered)

Target: `/Volumes/hyperfocus 1/hyperfocus.app/Contents/MacOS/hyperfocus` (universal Mach-O, 66,130,800 bytes, arm64 + x86_64 slices).
Method: static only — symbol-table recovery, byte-window extraction of `__TEXT,__const` string literals, frontend asset cross-check. No code executed.

**Evidence convention.** Every claim carries a file offset (`0x…`) into the universal binary, or a symbol path, or a frontend asset path. `arm64`/`x86_64` slices contain duplicated copies of the const section, so many literals appear at two offsets; the first is quoted. Claims that are only inferable from names are explicitly marked **inferred**.

---

## 0. Recovery notes that affect interpretation

| Issue | Detail |
|---|---|
| Rust string literals are stored contiguous, no NUL separators | Adjacent literals merge in `strings` output (e.g. `0x16b3376`: `ai_provider` + `device_fingerprint` + `entitlement_token` …). Boundaries were re-derived from raw bytes. |
| `format!` placeholders vanish into the literal | Placeholder slots appear as stray non-ASCII bytes. E.g. `0x128d82a`: `b'Find the \xc0\x0b goal this \xc0" goal directly helps achieve.\nThe \xc8\x01\x005 goal must be a necessary step toward completing the \xc8\x00\x00\x80\xad\x00 goal.'` — a 4-slot template. |
| Binary is **mostly stripped** | `nm` yields ~411 unique `hyperfocus::*` paths. However a **compact Rust name table** survives in `__TEXT,__const` (e.g. at `0x1d73481`, `0x1a2e2c0`) containing full legacy-mangled `__ZN…17h<hash>E` names. Demangling that table yields **1,277 unique `hyperfocus`-namespace symbols** — this is the primary structural evidence below. |
| Frontend bundle is present in-app | Extracted assets (already available at `/tmp/hf/web/`) corroborate command names, tool arguments, poll options and audio worklet behavior. |

---

## 1. LLM layer — `hyperfocus::ai::llm`

### 1.1 Module map (from module tree + name table)

```
ai::llm
├── traits::LLMProvider
├── types
│   ├── request::{LLMRequest, LLMRequestBuilder}
│   ├── agent_request::{AgentRequest, AgentRequestBuilder, ContentMessage}
│   ├── agent_response::AgentResponse
│   └── error::LLMError
├── openrouter_client::OpenRouterClient
├── openrouter_mapping        (11 items)
├── gemini_mapping::{content, requests, responses}   + gemini_types
├── worker_client::WorkerLlmClient
├── resolved_client           (ResolvedLlmClient, ResolvedAgentLlmClient)
└── schema_mapping
```

### 1.2 `LLMProvider` trait shape

Recovered trait members (name table):

| Member | Kind |
|---|---|
| `LLMProvider::generate_agent_once` | required method (per-implementor impl exists) |
| `LLMProvider::generate_json_once` | required method |
| `LLMProvider::generate_agent` | provided/default method (has its own `_{{closure}}`) |
| `LLMProvider::generate_json` | provided/default method |

Implementors found (`_<Impl as hyperfocus::ai::llm::traits::LLMProvider>::generate_agent_once`):

| Implementor | Backing |
|---|---|
| `openrouter_client::OpenRouterClient` | direct OpenRouter HTTPS |
| `worker_client::WorkerLlmClient` | Hyperfocus hosted worker (`workers.hyperfocus.in`) |
| `resolved_client::ResolvedLlmClient` | wrapper — `generate_json_once` only |
| `resolved_client::ResolvedAgentLlmClient` | wrapper — `generate_agent_once` only |

The `…_once` / plain split implies the trait's *provided* `generate_agent` / `generate_json` add a wrapper around a single attempt (the only plausible wrapper given the surrounding code is entitlement-token refresh, see §6.4). **No explicit retry/backoff/timeout constants were found anywhere in the app's own const data** — the only `retry`/`max_retries` literals in the binary belong to `reqwest` (`max_retries_per_request hit`, `reqwest::retry`, offset `0x16c1ffd`). Treat client-side retry as *unproven*.

### 1.3 Provider selection / resolution

| Symbol | Role |
|---|---|
| `resolved_client::load_system_ai_runtime_config` | loads the runtime AI config (provider + model) |
| `resolved_client::resolve_llm_client_with_entitlement` | returns a resolved client, gated on entitlement |
| `commands::ensure_hosted_ai_access_if_needed` | pre-flight for hosted mode |
| `entitlement::service::EntitlementService::hosted_ai_token` / `fresh_hosted_ai_token` | token minting for `WorkerLlmClient` |
| `worker_client::WorkerLlmClient::with_managed_entitlement` | binds the client to the entitlement service |

Provider enum is `AiProvider` with serde values `hyperfocus` (hosted worker) and `openrouter` (`0x16b3376`, `0x34a0b3e`). Persisted as the `ai_provider` settings key.

### 1.4 Base URLs, endpoints, model ids (all literal)

| Literal | Offset | Notes |
|---|---|---|
| `https://openrouter.ai/api/v1/chat/completions` | `0x34a1713` | OpenRouter chat completions (authorization-code key exchange result is used as `Authorization` bearer key) |
| `https://openrouter.ai/api/v1/auth/keys` | `0x34a1448` | PKCE code → API-key exchange |
| `https://openrouter.ai/auth` | `0x16a3956` | OAuth authorize page |
| `https://workers.hyperfocus.in` | `0x11dc52d` | hosted worker base |
| `/v1/llm/generate-turn` | `0x34a1448` | hosted agent-turn endpoint |
| `/v1/llm/generate-json` | `0x34a1448` | hosted structured-JSON endpoint |
| `/v1/trial/start`, `/v1/entitlement/refresh` | `0x16b3a80` region | entitlement endpoints |
| `/v1/transcription/deepgram-token` | `0x16b5746` | STT token endpoint |
| `/v1/feedback` | `0x16b5773` region | feedback endpoint |
| Headers used | `Authorization`, `Content-Type` | `0x34a1448` |

Model ids (`0x34a1713`, `0x16b3d15`, `0x16b3a80`):

| Literal | Where used |
|---|---|
| `google/gemini-3.7-flash` | OpenRouter model id (also appears as `gemini-3.7-flash` bare at `0x16b3a80`) |
| `google/gemini-3.1-flash-lite` | OpenRouter model id |
| `google/gemini-2.5-flash` | OpenRouter model id |
| `gemini-2.5-flash-lite` | bare Gemini id |
| `gemini-3.1-flash-lite` | bare Gemini id |

Error strings tied to provider selection (`0x1695c40`, `0x34aae0f`):
- `OpenRouter is selected, but no API key is saved. Open Settings to add one.`
- `OpenRouter API key is required`
- `OpenRouter couldn't authenticate your API key. Check or replace it in Settings, then try again.`
- `Hosted AI access token is missing or stale`

### 1.5 Request / response types

| Type | Fields recovered |
|---|---|
| `types::request::LLMRequest` / `LLMRequestBuilder` | `LLMRequest::validate`, `LLMRequestBuilder::{build, response_schema}` |
| `types::agent_request::AgentRequest` / `AgentRequestBuilder` | `AgentRequest::validate`, `AgentRequestBuilder::{build, contents}` |
| `types::agent_request::ContentMessage` | round-trips with `agent::types::ContentMessageDto` (bidirectional `From` impls both ways) |
| `types::agent_response::AgentResponse` | `AgentResponse::text` |
| `types::error::LLMError` | `Display` impl present |

**OpenRouter wire types** (`0x16b5147`, `0x34a8a0a`):
`OpenRouterChatRequest{model, messages, tools, tool_choice, provider, response_format, max_tokens, reasoning, stream?}`, `OpenRouterMessage{role, content}`, `OpenRouterTool{type, function}`, `OpenRouterToolFunction{name, description, parameters, strict}`, `OpenRouterToolChoice` = `auto | required | none`, `OpenRouterProviderPreferences{sort, …}` (throughput-optimised ordering), `OpenRouterReasoning{effort, …}`, `OpenRouterResponseFormat{type: "json_schema", json_schema: {name, strict, schema}}`, `OpenRouterJsonSchema{name, strict, schema}`, `OpenRouterChatResponse{choices}`, `OpenRouterChoice{message}`, `OpenRouterAssistantResponseMessage{tool_calls, reasoning, reasoning_details, …}` (4 fields), `OpenRouterToolCall`, `OpenRouterResponseToolCall{id, function}`, `OpenRouterResponseFunctionCall{name, arguments}`, `OpenRouterFunctionCall`.

Mapping helpers: `build_openrouter_agent_request`, `build_openrouter_json_request`, `map_tool_definition`, `parse_openrouter_chat_response`, `parse_openrouter_json_response`, `take_matching_tool_id`, `text_from_content`, `tool_call`, `tool_result_message`, `encode_openrouter_signature`, `decode_openrouter_signature`.
`schema_mapping::normalize_schema_types` and `gemini_mapping::requests::sanitize_gemini_schema` normalise the shared JSON-schema dialect for each vendor.

**Gemini wire types** (`0x16b4e49`): `GeminiRequest{contents, systemInstruction, tools, toolConfig, generationConfig}`; `GeminiResponse{candidates}`; `Candidate{content, …}`; `Part::Text | Part::FunctionCall | Part::FunctionResponse` (untagged enum — binary contains the diagnostic `data did not match any variant of untagged enum Part`); `Content{role, parts}`; `FunctionDeclaration{name, description, parameters}`; `FunctionCallingConfig{mode, allowedFunctionNames}` with `mode ∈ {AUTO, ANY, NONE}`; `ToolConfig`; `GenerationConfig{temperature, maxOutputTokens, responseMimeType, responseSchema, thinkingConfig}`; `ThinkingMode` — `Budget` vs `Level`, with the two guard messages:

> `Gemini 3 models require ThinkingMode::Level, not Budget. Use .thinking_level() or .no_thinking() instead.`
> `Gemini 2.5 models require ThinkingMode::Budget, not Level. Use .thinking_mode(ThinkingMode::Budget(_)) or .no_thinking() instead.`
> (`0x16b72a0`; levels observed: `minimal | low | medium`, `0x16b5147`)

Gemini responses parse via `parse_agent_response` / `parse_structured_response`.

### 1.6 Structured-output schema names

`response_format.json_schema.name` values present as literals (`0x16b5147`):

`structured_response`, `agent_response`, `clarity_extraction_response`, `parent_matching_response`, `eval_judge_response`.

`eval_judge_response` has **no** corresponding symbol in the recovered name table — its producer/eval harness is not statically visible. **Marked as unresolved.**

### 1.7 Agent-side error taxonomy (`agent::types::AgentError`, `0x2fd3eb0`)

`loop_limit`, `assistant_response`, `empty_response` (message: `Agent returned no text and no tool call`), `provider_required` (`start_goal_setting requires the resolved LLM provider`), `turn_error`, `prioritization_sync`, `tool_execution_error`. `AgentError: From<LLMError>` and `From<sqlx_core::error::Error>` both exist, so LLM and DB failures converge into one error type surfaced to the turn recorder as `last_error`.

---

## 2. Agent orchestration — `hyperfocus::ai::agent`

### 2.1 Module map

```
ai::agent
├── prompt            (build_system_instruction, build_bootstrap_context, ActiveAgentSkill)
├── cycle_labels      (cycle_length_label, format_fallback_length)
├── service
│   ├── context       (build_bootstrap_messages)
│   ├── parsing       (parse_agent_text, tool_error_as_result)
│   └── turn          (10 items — the loop)
├── conversation
│   ├── lifecycle     (next_conversation_event)
│   ├── recorder      (ConversationRecorder)
│   ├── snapshot      (get_agent_conversation_snapshot, derive_suggested_answers, task_id_from_messages)
│   └── storage       (insert_messages, load_conversation_row, load_message_history, next_sequence_number, parse_persisted_message, serialize_message, map_conversation_insert_error)
├── prioritization
│   ├── models        (PrioritizationBreakdown, PrioritizationBreakdownPatch, HydratedPrioritizationBreakdown)
│   ├── repository    (load/save/parse/normalize/validate)
│   └── workflow      (apply_patch, hydrate_breakdown_xml, escape_xml, sync_prioritization_breakdown)
├── types             (AgentError, ContentMessageDto, escape_xml, pretty_json, render_app_tool_result_context)
└── tools             (see §3)
```

### 2.2 Turn loop — `service::turn`

| Symbol | Role |
|---|---|
| `execute_turn_request_with_provider_model_and_notifiers` | entry point; takes provider + model + notification sinks |
| `launch_action_tool_call` | synthesises the "launch action" tool call that bootstraps a skill (`start_planning` / `start_prioritization` / `start_goal_setting`) |
| `capture_skill_activation` / `activated_skill_for_start_tool_result` | maps a `start_*` tool result to the `ActiveAgentSkill` to persist |
| `append_messages` | appends this turn's messages (delegates to `ConversationRecorder`) |
| `finalize_error` | writes terminal error state |
| `pretty_json` | pretty-prints tool args/results into the transcript |
| `service::context::build_bootstrap_messages` | builds the initial user-visible bootstrap message(s) for a new conversation |
| `service::parsing::parse_agent_text` | extracts assistant text from a provider response |
| `service::parsing::tool_error_as_result` | converts a tool execution error into a `function_result` payload so the model can recover |

The loop is bounded — `AgentError::loop_limit` (`loop_limit` literal at `0x2fd3eb0`) is the guard. The exact numeric bound is **not** recoverable from const data (no adjacent integer literal found).

Launch actions are an internally-tagged enum `AgentLaunchAction { StartPlanning, StartPrioritization }` plus the initial-message enum with serde tags `start_planning | start_prioritization | start_goal_setting` (`0x128e9d0`, `0x128ec8e`).

### 2.3 Conversation persistence

SQL recovered verbatim (`0x2fd283b`–`0x2fd2e9d`):

```sql
INSERT INTO agent_messages (id, conversation_id, turn_id, sequence_number, message_type, payload_json)
VALUES (?, ?, ?, ?, ?, ?)

SELECT message_type, payload_json FROM agent_messages
 WHERE conversation_id = ? ORDER BY sequence_number ASC

SELECT COALESCE(MAX(sequence_number), 0) + 1 FROM agent_messages WHERE conversation_id = ?

SELECT id, cycle_id, active_turn_id, active_skill, revision, last_error FROM agent_conversations WHERE id = ?

UPDATE agent_conversations
   SET active_turn_id = ?, active_skill = ?, last_error = ?, revision = ?, updated_at = datetime('now')
 WHERE id = ?

UPDATE agent_conversations SET active_turn_id = ?, revision = ?, last_error = NULL, updated_at = datetime('now') WHERE id = ?

INSERT INTO agent_conversations (id, cycle_id, active_turn_id, active_skill, revision, last_error)
VALUES (?, ?, ?, NULL, 1, NULL)

UPDATE agent_conversations SET revision = ?, updated_at = datetime('now') WHERE id = ?
```

Consequences: one conversation per `cycle_id`; `active_skill` is `NULL`-able (no skill active); `revision` is an optimistic-concurrency counter starting at 1; `last_error` is cleared on success; messages are ordered by a per-conversation `sequence_number`.

`message_type` values (from `serialize_message` / `parse_persisted_message` and the DTO field run at `0x128e8b0`): `user`, `assistant`, `model_text`, `thought_signature`, `response_group_id`, `model_function_call`, `tool_call_id`, `function_result`, `app_tool_result`. `storage::map_conversation_insert_error` maps the unique-constraint violation `UNIQUE constraint failed: agent_conversations.id` (`0x16a3956`) to a typed error.

Snapshot/conversation DTO fields (`0x128e8b0`, `0x2fd3eb0`): `conversation_id`, `turn_id`, `revision`, `state` (`in_flight | awaiting_input | error`), `messages`, `suggested_answers`, `active_task_id`, `last_error`, `active_skill`.

`SendAgentMessageParams{conversation_id, text}`; `StartAgentConversationParams{initial_messages, launch_action}` (4 fields incl. `cycle_id`).

`conversation::lifecycle::next_conversation_event` produces the Tauri event name **`agent:conversation_updated`** (`0x2fdc8c6`; emitted via `AppHandle::emit_agent_conversation_updated`, trait `events::agent::AgentConversationEmitter`).

`derive_suggested_answers` derives the quick-reply chips from the last assistant message; `task_id_from_messages` recovers the active task id from the transcript.

### 2.4 `ActiveAgentSkill` and how a skill activates

Values (literal run at `0x128ec8e`, and `ActiveAgentSkill::from_db_value`):

| Persisted value | Prompt block |
|---|---|
| `goal_setting` | goal clarification / refinement flow |
| `long_term_planning` | long-term (monthly) planning flow |
| `short_term_planning` | weekly / daily planning flow |
| `prioritization` | ruthless-prioritization flow |
| *(none)* | `<active_skill name="none">` fallback block |

Note: the same literal run also contains `session`, `long_term`, `null` — these are **cycle-type labels** used by the bootstrap payload, not skill values (cross-checked against the `session daily weekly long_term` run at `0x11daf50`).

Activation path:
1. Some prior turn (or the app) issues a `start_*` tool call.
2. `activated_skill_for_start_tool_result` maps the result to a skill. Evidence at `0x11db29e`: `unsupported_cycle_type | short_term_planning | long_term_planning | get_cycle_context` — so `start_planning` picks `short_term_planning` for `day`/`week` cycles and `long_term_planning` for `month`, and raises `unsupported_cycle_type` for anything else (session cycles are explicitly rejected: `Agent mutations are not supported for session cycles.`, `unsupported_cycle_type`).
3. `capture_skill_activation` + `ConversationRecorder::append_messages_and_set_active_skill` write it to `agent_conversations.active_skill`.
4. On the next turn, `prompt::build_system_instruction` selects the matching block.

`start_goal_setting` → `goal_setting` (`0x11db2a0`: `start_goal_setting | goal_setting`). `start_prioritization` → `prioritization` (`0x16b74d3`: `prioritization` adjacent to `hyperfocus::ai::agent::prioritization::repository`). `update_goal_breakdown` also returns `activated_skill` in its result (`0x16b7710`).

### 2.5 Prompt construction — `agent::prompt`

| Function | Output |
|---|---|
| `build_system_instruction` | one of five large XML-ish system prompts, selected by `ActiveAgentSkill` |
| `build_bootstrap_context` | the machine-readable context appended for planning skills |
| `cycle_labels::cycle_length_label` / `format_fallback_length` | human labels ("6 months", "1 week") used by the long-term prompt (`{cycle_length}` slot) |

Two DTOs carry the context: `CycleContextPayload` and `TaskContextSnapshot`.

```
CycleContextPayload {
  cycle_id, cycle_key, parent_cycle_key, parent_id, cycle_type,
  cycle_length, calendar_key, starts_on, ends_on,
  child_cycle_keys: ChildCycleKeys { ended, current, future },
  tasks: [CycleTaskRow { id|task_id, title, subtasks_markdown, parent_id, agent_proposal, needs_refinement, needs_breakdown }]
}
TaskContextSnapshot { task_id, title, completed, position, goal_breakdown, missing_fields, needs_refinement, needs_breakdown }
```

Field names taken verbatim from `0x16b776e`–`0x16b7800` (`ChildCycleKeys ended current future`, `CycleContextPayload cycle_key parent_cycle_key child_cycle_keys cycle_type cycle_length tasks … calendar_key starts_on ends_on`), `0x16b775d` (`CycleTaskRow subtasks_markdown`), `0x16b76b8` (`TaskContextSnapshot completed position goal_breakdown missing_fields needs_refinement needs_breakdown`). `cycle_context::format_subtasks_as_markdown` / `push_subtasks_markdown` render nested subtasks as Markdown for the model; `cycle_metadata::{metadata_from_row, complete_metadata_from_row, required_dated_value, trim_dated_value}` normalise dates.

### 2.6 Recovered prompt templates

All five prompts are stored as single literals with real newlines. Offsets: goal-setting `0x128ecd0`; long-term planning `0x12903bc`; short-term planning `0x1290a80`; prioritization `0x1291590`. Quoted below are the semantically load-bearing parts (longer prose paraphrased).

#### 2.6.1 `<active_skill name="none">` fallback (`0x128ec8e`)

```
<active_skill name="none">
No detailed skill flow is currently active. Choose the right start_* tool before following a detailed skill flow.
</active_skill>
```

#### 2.6.2 `goal_setting` (verbatim, abridged at `…`)

```
<description>
Your job is to help the user clarify and refine their goal following why -> what -> how structure.
Refinement means turning the known goal_breakdown into a clear title and when appropriate, actionable subtasks.
</description>

<flow>
1. UNDERSTAND:
    a. Read title, subtasks, parent_id, goal_breakdown, missing_fields, needs_refinement, and needs_breakdown from the `task` object returned by start_goal_setting.
    b. If `task.parent_id` is present, fetch it with get_task_details.
    c. Treat the latest tool result as the source of truth.
2. COLLECT:
    a. Explain your job and what specifically makes this goal unclear.
    b. Ask one targeted question at a time to collect context, output, and outcome.
    c. Follow missing_fields as a guidance, but also ask clarifying questions about fields in goal_breakdown that seems vague or unclear.
    d. Pay special attention to context clarification, because it drives everything else.
    e. Save new information with update_goal_breakdown. Batch multiple learnings when possible.
    f. Continue until user answered all questions they can about context, output, and outcome.
    g. When context is high and output.value is present — estimate and update work_size using update_goal_breakdown
    h. Move to 3. REFINE_TITLE.
3. REFINE_TITLE:
    a. Build the title using refinement_cases and refinement_rules.
    b. Call update_goal with title, rationale, and needs_refinement=false only after context, output, and outcome work is resolved. Do not include subtasks in this update.
    d. If needs_breakdown is true, ask whether to break it down and include exactly two suggested answers: <next_steps>Help me to think through|Recommend steps</next_steps>.
    e. Wait for the user's answer before BREAK_DOWN.
4. BREAK_DOWN:
    a. If needs_breakdown is not true, go to FINISH.
    b. If needs_breakdown is true, ask the user about their current state and the next step that they have in mind to reach final outcome.
    c. If the user asked to recommend steps — propose steps that will lead to final output/outcome after they shared the current state.
    d. Ask user to confirm the list before updating the goal.
    e. After the user confirms the provided, proposed, or edited steps, call update_goal_breakdown with scope.decomposition and scope.fully_decomposed=true.
    f. After updating the breakdown, call update_goal with subtasks, rationale, and needs_breakdown=false.
5. FINISH: Ask if the user needs to change anything and recommend the next step.
</flow>
```

```
<rules>
- If the user do not control the final outcome (controlled_by_user is false) — propose to re-frame the goal to something user can control.
- If the output does not feel like the final deliverable - try to ask more "what" to get to the final one.
- If the outcome does not feel like an end goal — try to ask more "why" to get to the end goal.
- To fetch parent cycle context, use bootstrap parent_cycle_key. If parent_cycle_key is null, do not fetch parent cycle context.
- Use parent context to help the user answer, but derive the goal's "why" only when it is explicit and useful.
- If the user is unsure, help with a clarifying question.
- Skip a missing field only when it seems irrelevant.
- Use only user answers and tool results. Do not invent metrics, dates, stakeholders, products, or scope.
- Preserve the user's language and tone.
- Clear needs_refinement=false only after the context/output/outcome questions needed for a refined title are resolved.
- Clear needs_breakdown=false only after decomposition has been confirmed and the user-visible goal/subtasks have been updated.
- Do not ask for decomposition because scope.fully_decomposed is false when needs_breakdown is not true.
</rules>
```

```
<refinement_cases>
Choose one case before updating the title:
- OriginalOnly: no output.value and no outcome.value. Keep current title; enrich only with context.clarification.
- OutputOnly: output.value present, outcome.value absent. Use [active verb] + [output.value].
- OutcomeOnly: outcome.value present, output.value absent. Preserve or derive a controllable action from current title, context, or outcome.verification_method, then connect it to outcome.value.
- OutputToOutcome: both output.value and outcome.value present. Use [controllable action on output] + "to/for" + [outcome.value]. Convert passive outputs to action verbs: "signed agreement" -> "Sign agreement".
</refinement_cases>

<refinement_rules>
- Keep the refined goal concise (max 20 words, shorter when possible).
- Prefer simple, everyday verbs and natural connectors like "to" or "for".
- Prefer agency + motivation when both are known: "Ship onboarding flow to reduce churn", not "Reduce churn".
- Use context.clarification and outcome.verification_method only when they add needed specificity.
- Omit filler adjectives unless essential to the user's intent.
- Do not include outcome.expected_date unless the user framed it as a deadline.
- Use only information grounded in known context.
</refinement_rules>

<subtask_rules>
- Use existing scope.decomposition items as the source for subtasks.
- Create or update subtasks only when the current flow is resolving needs_breakdown and the user has confirmed decomposition.
- Preserve decomposition count and order by default.
- Map each decomposition item to one actionable subtask: action verb + specific object.
- Make vague phases actionable using known context: "Research" -> "Research competitor pricing models".
- Keep similar-sounding items distinct unless the user agrees to merge them.
- Add, remove, merge, or expand subtasks only after discussing the changed action plan.
</subtask_rules>
```

#### 2.6.3 `long_term_planning` (`0x12903bc`)

```
<description> Your job is to help user set 1-3 outcomes that will make the next months a success. </description>
<flow>
1. READ: Read the `cycle` object from the start_planning response to understand goals and `cycle_length` for the current cycle.
2. FRAME: Explain your job.
3. COLLECT
   - If no long-term goals exist ask them to start messy. Ask something like: "by the end of these {cycle_length} months, what 1-3 results would matter most to you?
     You can start messy, we'll clarify things as we go." Propose only one suggested answer: "I'm not sure"
   - If long-term goals exist, reflect them briefly and ask what is missing. … Propose "I'm not sure" as a suggested answer.
   - If the user do not know what success look like for them, help by asking clarifying questions. E.g.: "What feels most unresolved or repeatedly postponed?",
     "If only one area improved in the next 6 months, which would make the biggest difference?". You can also offer categories like work, business, health, money, relationships, learning, personal admin.
4. CLARIFY: If there items that are too vague: ask a quick conversational question for each to make the desired outcome clearer.
5. EXTRACT: Convert the user's messy input into possible long-term outcomes. If the user gives tasks, group them into larger outcomes. …
6. SELECT: Ask which 1-3 belongs are the most important for this cycle.
7. SHAPE: If an item is a small task or execution step, help turn it into an outcome or project, or move/delete it when the user confirms.
8. CONFIRM AND CAPTURE: Create missing goals in the current cycle with create_goal. … Ask user to review the final list.
9. CONFIRM: If the plan has 5 or more long-term goals, recommend prioritization. Otherwise, recommend to clarify each goal step by step.
</flow>
<rules>
- Ask one focused question at a time.
- Keep the plan focused on 1-3 outcomes for this long-term cycle. Allow to have more, but propose to prioritize them at the end.
- Batch multiple new goals into as few create_goal previews as possible.
- Avoid creating duplicates of goals that already exist in the current cycle.
- Move forward only after the user confirms the next action.
</rules>
```

#### 2.6.4 `short_term_planning` (`0x1290a80`)

```
<description> Your job is to help the user build an execution plan for the current Weekly or Day page. </description>
<key_questions>
- Weekly: What needs to move this week to advance long-term goals?
- Daily: What needs to happen today to advance the weekly plan?
</key_questions>
<flow>
1. READ: Call start_planning and read the returned `cycle` object for the current page's goals and cycle context.
2. LOAD_PARENT_CONTEXT:
   - If bootstrap parent_cycle_key is present, call get_cycle_context with that parent_cycle_key.
   - If bootstrap parent_cycle_key is null, do not fetch parent context.
3. CHECK_COMPLETENESS: ask which parent goals the user wants to support; ask what progress must happen in the current page; ask if anything important is missing.
4. CAPTURE_ADDITIONS:
   - Create goals or tasks in the current cycle_key with create_goal.
   - When creating a Weekly or Day goal/task that supports a selected parent goal, pass that parent goal's `task_id` as `parent_id`.
   - If a goal/task does not support a specific parent goal, omit `parent_id` instead of guessing.
5. CONFIRM: If the weekly or daily list is too large, recommend prioritization. Otherwise, recommend the best next step.
</flow>
<rules>
- Use parent-cycle context to keep execution work connected to the larger plan.
- Use `update_goal.parent_link` to repair existing links only after the user confirms the repair: `{"type": "set", "parent_id": "..."}`.
- Use `update_goal.parent_link` to remove existing links only after the user confirms removal: `{"type": "clear"}`.
- `move_goal` creates an unlinked destination preview. If the user wants a linked child goal, use `create_goal` with `parent_id` instead of `move_goal`.
- Move forward only after the user confirms the next action.
</rules>
```

#### 2.6.5 `prioritization` (`0x1291590`)

```
<description> Your job is to ruthlessly prioritize users goals by following the flow below, so they can focus on the highest-leverage tasks. </description>
<flow>
1. READ: Fetch the current plan via start_prioritization. Read the returned `prioritization.breakdown` as the persisted decisions and
   `prioritization.hydrated_xml` as their task-hydrated agent representation.
2. UNDERSTAND: If some goals are too vague or unclear for prioritization (e.g. 'make work') - ask a quick conversational question to make them more specific.
3. [IF TACTICAL MOVE-DOWN MAY BE NEEDED] LOAD_CHILD_KEYS: Call get_cycle_context with the current bootstrap cycle_key before tactical move-down decisions that need child_cycle_keys.
4. [IF CURRENT PAGE HAS CHILD PAGES] FILTER_TACTICAL: … Prefer child_cycle_keys.current as the destination_key; if there is no current child key, propose creating the child plan instead of inventing a key.
5. MERGE: If multiple goals probably target similar outcomes or overlap - ask if it make sense to combine them into one goal with subtasks.
6. PICK_BIGGEST_IMPACT: "if you can complete just one item - what it would be?" … If they select more then 3 goals - push to ruthlessly prioritize to 3 goals max. Save selected goals to big_wins and update titles with ⭐️ at the beginning.
7. FIND_NON_NEGOTIABLES: "if you will focus just on the most important tasks - which goals will backfire?" … Save them to non_negotiables. Update titles with 💣 at the beginning.
8. DEPRIORITIZE: Push to postpone, drop, or delegate. To postpone use move_goal with destination_key "later". To drop use delete_goal. To delegate - create a new task in the current cycle_key via create_goal.
9. CLARIFY: For each selected big win with needs_refinement=true - propose to clarify. … If user agrees, start_goal_setting, then return to prioritization when completed.
</flow>
<rules>
- Ask one question at a time. Avoid batching multiple questions together until absolutely necessary.
- Remove processed task from pending_review.
- Before merging goals - execute `get_task_details` of each task first, so you have title and subtasks of each goal.
- Be opinionated and push for ruthless prioritization.
- Call update_prioritization_breakdown every time you classify or reclassify work.
- When updating an array field, always preserve the existing items you still mean to keep.
</rules>
```

(The title markers are `⭐️` (U+2B50 U+FE0F, bytes `\xe2\xad\x90\xef\xb8\x8f` at `0x1291c06`) for `big_wins` and `💣` (U+1F4A3, `\xf0\x9f\x92\xa3` at `0x1291e1d`) for `non_negotiables` — both verified in raw bytes, since the earlier literal index split them off the sentence.)

### 2.7 Prioritization model

`PrioritizationBreakdown{big_wins, bottlenecks, non_negotiables, deprioritized, pending_review}` (5 fields, `0x34a3fbe`); `ProcessedTaskDecision{task_id, reason}`; `HydratedPrioritizationBreakdown` produced by `workflow::hydrate_breakdown_xml` (XML-escaped, `workflow::escape_xml`); `PrioritizationBreakdownPatch` applied by `workflow::apply_patch` then merged by `repository::normalize_breakdown` / validated by `repository::validate_breakdown`.
Persisted in the `cycles_table.prioritization_breakdown` column (`UPDATE cycles_table SET prioritization_breakdown = ? WHERE id = ?`, `0x2fd3090`).
`start_prioritization` returns `PrioritizationContext{breakdown, hydrated_xml}`.

---

## 3. Agent tools — `hyperfocus::ai::agent::tools`

Registry: `tools::build_tool_definitions` assembles all definitions; execution goes through `execute_tool_with_task_events_and_provider` (provider-aware, for `start_goal_setting`) or `execute_tool_with_task_events`. Tool results are emitted with `app_tool_result` message type.

### 3.1 Tool inventory

| Tool | Description (verbatim unless noted) | Args struct (n fields) |
|---|---|---|
| `start_planning` | `Start planning for the current planner page, activate the matching long-term or short-term planning skill, and return the current cycle context and goals.` | *(none)* |
| `start_prioritization` | `Start or resume prioritization for the current planner page and return its persisted breakdown and hydrated agent representation in a named prioritization object.` | *(none; returns `PrioritizationContext`)* |
| `start_goal_setting` | `Load the selected task into goal-setting mode and return its current clarity state.` | `StartGoalSettingArgs` (1): `task` |
| `get_cycle_context` | `Fetch the requested cycle context, including cycle metadata, related cycle keys, and the list of cycle goals.` | `GetCycleContextArgs` (1): `cycle_key` |
| `get_task_details` | `Fetch full details of a task including title, completed, position, subtasks, current goal_breakdown, missing_fields, needs_refinement, and needs_breakdown.` | `GetTaskDetailsArgs` (1): `task_id` (`The task ID to fetch`) |
| `create_goal` | `Create a new goal in a planning cycle.` | `CreateGoalArgs` (5) |
| `delete_goal` | `Delete a goal from a planning cycle.` | `DeleteGoalArgs` (3) |
| `update_goal` | *(no text captured — description slot empty in const data)* | `UpdateGoalArgs` (7) |
| `update_goal_breakdown` | `Save gathered information into the task's goal_breakdown. Call this when you learn new goal information from the user. Returns updated breakdown, missing_fields, needs_refinement, and needs_breakdown.` | `UpdateGoalBreakdownArgs` (5) |
| `update_prioritization_breakdown` | `Update the prioritization breakdown for the current planner page. Always preserve existing items in arrays unless you intentionally remove them.` | `UpdatePrioritizationBreakdownArgs` (5) |
| `move_goal` | `Move an existing goal from a source cycle to a different destination cycle.` | `MoveGoalArgs` (4) |

`update_goal` has no recoverable description string; the "missing description" is consistent across both slices.

### 3.2 Argument schemas

**`create_goal` — `CreateGoalArgs` (5)** (`0x2fdc9d3`, `0x11e4540`)

| Field | Type | Description (verbatim) |
|---|---|---|
| `cycle_key` | string, required | `A cycle_key from context, parent_cycle_key, child_cycle_keys, or "later".` |
| `title` | string | `The title of the new goal.` |
| `subtasks` | array of `{title}` | `Optional subtask titles.` (element field literally `items`, nested objects carry `title`) |
| `parent_id` | string \| null | `Optional parent task ID. Use only for Weekly goals linked to a Long-term parent goal, or Day goals linked to a Weekly parent goal.` |
| `rationale` | string | `Brief explanation of why this goal should be created.` |

**`delete_goal` — `DeleteGoalArgs` (3)** (`0x2fdcb6b`): recovered description run is `task_id` → `The ID of the goal to delete.` → `Brief explanation of why this goal should be removed.` → `DeleteGoalArgs`. In the sibling slice the same struct's field run is `task_id\x00\x00DeleteGoalArgs` (`0x11e46e8`), where the two NULs are the linker's dedup slots for literals already emitted for `create_goal` — so the third field is almost certainly `cycle_key` and the rationale field is `rationale`. **Field 3 and the rationale key name are inferred; the descriptions are verbatim.**

**`start_goal_setting` — `StartGoalSettingArgs` (1)** (`0x1695c40`): `task`, described as `The task to clarify`.

**`update_goal` — `UpdateGoalArgs` (7)** (`0x128e7b0`)

| Field | Notes (verbatim descriptions) |
|---|---|
| `title` | refined goal title |
| `subtasks` | `Actionable subtask titles. Provide only after the user has confirmed decomposition for a goal with needs_breakdown=true.` |
| `parent_link` | `Optional parent-link intent. Omit to preserve the current parent link.` — internally tagged enum `UpdateGoalParentLinkIntent` with variants `set` (`{parent_id}`; help text: `Parent task ID from the immediate parent cycle.`) and `clear`. Serialised as `{"type": "set", "parent_id": "..."}` / `{"type": "clear"}` (system prompt §2.6.4). |
| `rationale` | `Brief explanation of why the proposal improves the goal.` |
| `needs_refinement` | bool — `Set to false only after context, output, and outcome refinement work is resolved. Omit to preserve the current value.` |
| `needs_breakdown` | bool — `Set to false only after decomposition and user-visible subtasks are confirmed. Omit to preserve the current value.` |
| *(1 more)* | `update_goal::resolved_clear_only_flag` exists, implying a 7th flag resolved from the clear-only path. **Not identified by name.** |

`update_goal` writes `tasks_table` with `SET goal_breakdown = ?, needs_refinement = ?, needs_breakdown = ?, parent_id = COALESCE(?, parent_id) WHERE id = ?` (`0x2fda943`).

**`update_goal_breakdown` — `UpdateGoalBreakdownArgs` (5)** (`0x16b74d3`)

| Field | Sub-struct (n) | Sub-fields |
|---|---|---|
| `task_id` | — | `The task ID` |
| `context` | `ContextUpdateArgs` (2) | `level`, `clarification` |
| `output` | `OutputUpdateArgs` (1) | `value` |
| `outcome` | `OutcomeUpdateArgs` (4) | `requires_verification`, `verification_method`, `controlled_by_user`, `expected_date` |
| `scope` | `ScopeUpdateArgs` (3) | `work_size`, `decomposition`, `fully_decomposed` |

Semantics: `args::parse_breakdown_update` → `args::sanitize_section_object` → `args::normalize_nullable_string` / `args::deserialize_nullable_string_patch` (null means "clear", absent means "preserve") → `merge::merge_breakdown` (`GoalBreakdown::merge`). Returned payload: `status`, `activated_skill`, `task_id`, `title`, and a `TaskContextSnapshot` (`completed, position, goal_breakdown, missing_fields, needs_refinement, needs_breakdown`). Selected missing-field ids seen near the args module: `output.value`, `outcome.value`, `context.clarification`, `outcome.verification_method` (see §4.3).

**`update_prioritization_breakdown` — `UpdatePrioritizationBreakdownArgs` (5)** (`0x34a8f34`)

| Field | Description (verbatim) |
|---|---|
| `big_wins` | `The complete current list of big wins` |
| `bottlenecks` | `The current bottleneck summary, or null to clear it` |
| `non_negotiables` | `Non negotiatable goals, that are not big wins, but must be done.` |
| `deprioritized` | `Tasks that were dropped, postponed, or delegated` |
| `pending_review` | `The complete current list of remaining task IDs to review` |

Element schema `processed_task_schema`: `{task_id, reason}` where `reason` = `The reason why we're moving the task to this bucket. Should be mentioned by user.` Values are `expected string or null`.

**`move_goal` — `MoveGoalArgs` (4)** (`0x16a3700` region; field run at `0x16a3a80`, struct at `0x34a9fb8`)

| Field | Description (verbatim) |
|---|---|
| `source_key` | `A cycle_key from context, parent_cycle_key, child_cycle_keys, or "later" that currently contains the goal.` |
| `destination_key` | `A cycle_key from context, parent_cycle_key, child_cycle_keys, or "later" to move the goal to.` |
| `task_id` | `The exact ID of the goal to move.` |
| `rationale` | `Brief explanation of why this goal should be moved.` |

Coded errors for `move_goal`: `invalid_cycle_key`, `cycle_unavailable`, `cycle_ended`, `destination_unavailable`, `source_unavailable`, `task_not_in_source`, `task_not_in_destination` (`0x16a2ef8`).
`update_goal_breakdown` coded errors: `task_not_found`, `empty_update`, `provider_required`, `invalid_parent_link`, `task_not_in_destination`.

### 3.3 `tools::shared::*`

| Module | Symbols | Recovered semantics |
|---|---|---|
| `activation` | `serialize_success_response` | shapes a successful `start_*` response so `capture_skill_activation` can read the activated skill |
| `cycle_context` | `build_cycle_context_payload`, `format_subtasks_as_markdown`, `push_subtasks_markdown` | builds `CycleContextPayload`; SQL selects `cycle.id as cycle_id, cycle.calendar_key as cycle_key, cycle.type as cycle_type, starts_on, ends_on, cycle.parent_id, parent.calendar_key as parent_cycle_key` and the cycle's goals; rows ordered `COALESCE(needs_refinement,0) DESC, COALESCE(needs_breakdown,0) DESC, position ASC` (unrefined/unbroken goals surface first) |
| `cycle_metadata` | `load_agent_cycle_metadata_by_id`, `metadata_from_row`, `complete_metadata_from_row`, `required_dated_value`, `trim_dated_value` | normalises cycle dates/keys; error strings `Planner page is missing starts_on.` / `missing calendar_key.` / `missing ends_on.` map to code `cycle_unavailable` |
| `cycle_mutability` | `ensure_agent_cycle_mutable`, `ensure_agent_cycle_mutable_on`, `backend_local_date` | guards agent mutations; rejects session cycles (`Agent mutations are not supported for session cycles.`, `unsupported_cycle_type`) and ended pages (`cycle_ended` — `This planner page has ended and can only be reviewed.`). Mutability is evaluated against the **backend** local date, not the client's. |
| `cycle_resolution` | `resolve_agent_cycle_key`, `resolve_later_cycle`, `ensure_task_in_cycle`, `coded_tool_execution_error` | resolves `cycle_key` → cycle row (by `calendar_key`), special-cases the literal cycle id `'later'`; `Invalid cycle_key. Use a cycle_key from current context, parent_cycle_key, child_cycle_keys, or "later".` |
| `preview_execution` | `applied_result`, `create_or_replace_empty_last_task_preview_in_tx`, `load_empty_last_visible_task_for_agent_replacement`, `load_working_task_row`, `deserialize_strict_args`, `map_preview_service_error`, `tool_execution_error`, `trim_to_non_empty` | agent writes land in the **preview** layer (`task_preview_originals` + `agent_proposal='upsert'`), not directly in user data. An agent-created goal replaces the last *empty* visible task instead of appending a new one; `trim_to_non_empty` drops empty/missing task ids. |
| `task_context` | `build_task_context_snapshot`, `load_task_context_row`, `parse_optional_goal_breakdown`, `parse_required_goal_breakdown_after_seeding`, `initial_breakdown_failed` | loads a task + its breakdown; "seeding" runs the goal-breakdown extraction if the row has no breakdown yet (`initial_breakdown_failed` is the error path) |
| `parent_links` | `validate_parent_link_candidate` | validates Weekly→Long-term / Day→Weekly parent links. Errors: `Parent link cannot point to the same task.`, `Parent link contains an em…`(ptiness/cycle mismatch) |

`tools::get_cycle_context::schema::definition` and `tools::start_planning::definition` / `start_prioritization::definition` / `create_goal::definition` / `delete_goal::definition` / `update_goal::definition` / `update_prioritization_breakdown::definition` are the per-tool JSON-schema builders.

---

## 4. Goal breakdown engine — `hyperfocus::ai::goal_breakdown`

### 4.1 Types

`types::breakdown::GoalBreakdown` (`0x34aac31`):

```
GoalBreakdown { context: ContextBreakdown, output: OutputBreakdown, outcome: OutcomeBreakdown, scope: ScopeBreakdown }
```

| Type | Fields (serde names) | Notes |
|---|---|---|
| `ContextBreakdown` (2) | `level`, `clarification` | `level: ContextType` |
| `OutputBreakdown` (1) | `value` | `The concrete deliverable, artifact, or thing to be produced.` |
| `OutcomeBreakdown` (5) | `value`, `requires_verification`, `verification_method`, `controlled_by_user`, `expected_date` | |
| `ScopeBreakdown` (3) | `work_size`, `decomposition`, `fully_decomposed` | `decomposition: Vec<String>` |
| `ContextType` | serde `low` / `high` (variants `Low`/`High`; `0x128d5a4`, value run `0x128d5af`) | `High`: goal names a specific subject/domain. `Low`: `report, project, presentation, MVP, client, meeting, things`. |
| `WorkSize` | serde `under_1_hour`, `1_hour_to_1_day`, `1_day_to_1_week`, `1_week_to_1_month`, `1_month_plus`, `unknown` (variants `Under1Hour`…`Unknown`; serde values `0x128d556`, variant names `0x1294c40`) | `Set unknown, unless you can select a concrete bucket… When multiple buckets are plausible, choose the larger bucket.` |
| `GoalBreakdownError` | `types::error::GoalBreakdownError` | |

Methods: `GoalBreakdown::merge` (null-clears / absent-preserves patch merge) and `GoalBreakdown::sync_scope_decomposition` (keeps `scope.decomposition` aligned with user-visible subtasks; also called from `commands::cycles::copy_previous::update_goal_breakdown_scope_decomposition`).
Conversation-side helpers: `types::conversation::{SubtaskInfo, ParentGoalInfo}`.

### 4.2 Gathering plan — `calculating clarity / missing fields`

`goal_breakdown::gathering_plan` (7 items):

| Function | Output |
|---|---|
| `calculate_clarity_flags` | top-level clarity flags |
| `calculate_missing_fields` | the missing-field id list handed to the model |
| `calculate_refinement_missing_fields` | refinement-phase subset |
| `context_missing_fields` / `output_missing_fields` / `outcome_missing_fields` / `scope_missing_fields` | per-section producers |

Gate semantics recovered from prompt text (§2.6.2) plus the downstream consumer:
- `context` high **and** `output.value` present → `work_size` may be estimated.
- `needs_refinement` may be cleared once context/output/outcome questions are resolved.
- `needs_breakdown` may be cleared once decomposition is confirmed.

Downstream, the same signals drive the **Planning Issue Report** consumed by the UI (`0x1d734b5` `calculate_clarity_flags`; frontend `/tmp/hf/web/_assets_index-BZ_MKiTS.js`). Issue item ids and thresholds observed in the frontend:

| Issue id | Title | Trigger |
|---|---|---|
| `task_like_items` | `Seems like a task, not a goal` | any item in `task_sized_items` |
| `needs_refinement` | — | any item in `needs_refinement_items` |
| `needs_breakdown` | `Breakdown missing` | items in `needs_breakdown_items` that are not already refined |
| `too_many_goals` | `Too many goals` | `goal_sized_items.length > 4` (long-term) or `active_items.length > 4` |
| `weekly_alignment` | `0 tasks moves long-term goals` | `linked_items.length === 0` |

Report payload fields: `goal_sized_items`, `task_sized_items`, `needs_refinement_items`, `needs_breakdown_items`, `linked_items`, `active_items`, `missing_fields`, `needs_refinement`, `needs_breakdown`, `detected_parent_is_valid` (`0x128d190`, `0x16b7fae`).
`PlanningIssueRefinementReason` variants: `NewerSibling`, `CycleEnded`, `EndedDescendant`, `StartedFocusBlock` (serde values `newer_sibling`, `cycle_ended`, `ended_descendant`, `started_focus_block` at `0x34a9119`; struct name at `0x1ebeabf`).
Prompt-visible missing-field tokens: `context.clarification`, `output.value`, `outcome.value`, `outcome.verification_method` (`0x16a2fb8`, `0x128eebc`, `0x128f062`). The `scope.decomposition` / `scope.fully_decomposed` tokens appear in the prompt text but not in the same literal cluster — **inferred** that `scope.decomposition` is a fourth missing-field id.

### 4.3 Extraction prompt — `prompt::extraction`

`extraction_system_instruction` verbatim (`0x128d190`):

```
You extract structured information from original_goal and subtask statements.
Your role:
- Only extract fields based on what is EXPLICITLY stated in the goal, subtasks or <date_context>
- Populate fields of goal_breakdown only using phrases from the goal and subtasks.
- Always populate `context` and `scope` fields.
- `output` and `outcome` fields are optional and can be skipped.
- Return only the extracted breakdown as JSON.
```

`build_extraction_prompt` takes `original_goal`, subtasks, and a `<date_context>` block (a `%Y-%m-%d` formatted local date was found adjacent to the goal_breakdown region at `0x16b7264`).

Field descriptions shipped to the model (verbatim, `0x128dc08`–`0x128e6c0`) — this is the operational definition of each breakdown field:

| Field | Instruction (verbatim) |
|---|---|
| `scope` | `The How. Detailed decomposition of the goal. Set based on the goal title, subtasks and available context.` |
| `scope.items` | `Smaller steps or milestones needed to achieve the goal (e.g., 'Design mockups', 'Implement API'). Leave empty, unless explicitly defined in subtasks or provided by user during the conversation with assistant.` |
| `scope.fully_decomposed` | `Set to true when decomposition covers all steps needed to produce the defined output, or user confirms the list is complete.` |
| `scope.work_size` | `Back-of-the-envelope calculations for amount of active work or sustained effort needed to complete the goal. Calculate when goal title, output, context, and subtasks are specific enough to estimate. If they are not - say the amount of work is unknown.` + `Set unknown, unless you can select a concrete bucket based on the work_size_calculation. When multiple buckets are plausible, choose the larger bucket.` |
| `output.value` | `The concrete deliverable, artifact, or thing to be produced. Set to null unless original_goal has an explicit artifact/deliverable that user will produce.` / `The name of the deliverable from the original_goal or subtasks that user want to produce` |
| `context.clarification` | `The context of the goal necessary for a coach to understand what user is talking about.` / `Information that clarifies the subject and domain of the original goal. Should exist in the goal or provided by user through conversation. Otherwise, leave it null.` |
| `context.level` | `Context level of the original goal. High: The goal contains the specific subject or domain clearly enough to understand what the user wants. Low: The goal uses generic or unresolved references (e.g., report, project, presentation, MVP, client, meeting, things) without enough detail to identify the subject or domain.` |
| `outcome.value` | `The purpose of achieving the goal. Set to null unless the original_goal has an explicit benefit or value that user will get after producing the output. Could be defined with markers: 'to [verb]...', 'in order to...', 'so that...', 'because...', 'for [achieving/improving/enabling]...'.` |
| `outcome.requires_verification` | `True when outcome.value needs an explicit method to verify outcome.value using metric (e.g. 'increase conversion rate', 'reduce latency', etc) or evidence for subjective claims (e.g. 'make landing page beautiful', 'align a team', etc). False for binary outcomes where the outcome itself is the evidence (e.g., 'land contract', 'close deal', 'get offer') OR for process/practice outcomes (e.g., 'improve skill', 'learn about Y', etc).` |
| `outcome.verification_method` | `Definition of how the outcome.value will be verified. Set to null unless it is explicitly stated in original_goal or subtasks. Copy exact user words. Either metric (e.g., 'run A/B test to measure CR change', 'run benchmark to validate improvement') or evidence (e.g. 'roadmap document signed off'). Should follow the structure [action verb] + [verification method]. Leave null when requires_verification is false or the method is not explicitly stated. Keep it short - 20 words max. Dont repeat yourself` |
| `outcome.controlled_by_user` | `True if the outcome.value is a direct result of user effort (e.g 'send an update to CEO to keep him in the loop', 'Log and analyze my expenses for the last month to reduce spendings', etc). False if depends on external factors like other people's decisions, user actions, market conditions, events outside user's control, etc. (e.g., 'get response from Lu', 'receive feedback', 'get hired', 'increase user awareness', etc)` |

**Response schema.** `prompt::shared::{context_schema, output_schema, outcome_schema, scope_schema}` build one sub-schema each; `prompt::extraction::get_extraction_response_schema` composes them into a JSON-schema object. Schema dialect vocabulary recovered from const data: `type`, `properties`, `items`, `required`, `nullable`, `additionalProperties`, `enum` (`0x1294e60`, `0x3087400`). The schemas are constructed at runtime, so **no verbatim schema document exists in the binary** — the field/description table above is the complete recoverable content. Schema name for this call: `clarity_extraction_response` (§1.6).

Instrumentation strings (both slices): `=== Extraction Phase ===`, `====================`, `  [Goal Breakdown Extraction] LLM response time: {}ms`.

### 4.4 Parent-matching prompt — `prompt::parent_matching`

Template recovered from raw bytes at `0x128d82a` (placeholders shown as `{}`; the four placeholder slots are contiguous with the literal, which is how `format!` compiles):

```
<task>
Find the {} goal this {} goal directly helps achieve.
The {} goal must be a necessary step toward completing the {} goal.
Keyword overlap alone doesn't count - there must be a clear causal relationship.
If found, set parent_task_id to that goal's task id, otherwise null.
</task>

<goal>
{}
</goal>

<{}_goals>
{}
</{}_goals>

Return only JSON with the parent_task_id field.
```

Placeholder values are cycle-type labels — the adjacent literal pool contains `weekly`, `daily`, `monthly` (`0x128d190` vicinity). The cycle-type-without-parent branch is a separate short template (`0x128d9b6`):

```
<task>
Find the parent goal connection. This cycle type does not have parents.
</task>

Return only JSON with parent_task_id: null
```

Schema: `get_parent_matching_schema` (single field `parent_task_id`, nullable string); schema name `parent_matching_response`. Tool description text: `The id of the matching parent task if found. Null if no parent connection exists.`
Instrumentation: `=== Parent Matching ===`, `=======================`, `  [Parent Matching] LLM response time: {}ms`.

### 4.5 Service

`goal_breakdown::service::map_llm_error` — converts `LLMError` into the breakdown domain error. Callers live outside the module name table; `commands::tasks::clarity` contains `run_extraction`, `load_task_for_clarity`, `set_task_clarity_data`, `reset_task_clarity`, `persist_task_clarity_result`, `format_date`, and types `ParentTask` / `ExtractionResult` — i.e. the breakdown engine is also driven from the task-clarity command path, not only from the agent's `update_goal_breakdown` tool.
`task_context::initial_breakdown_failed` is the failure surface when extraction runs during `start_goal_setting` seeding.

---

## 5. Audio / voice pipeline

### 5.1 Rust side — `hyperfocus::commands::utils`

| Symbol | Role |
|---|---|
| `init_audio_system` | startup; decodes the bundled MP3s into in-memory PCM ("pre-decoded buffers ready") |
| `decode_audio` | decodes an audio asset (rodio/symphonia stack) |
| `play_audio_feedback` | plays a decoded buffer; Tauri command `play_audio` |
| `PROCESS_START` | process-start instant used for startup timing |
| `focus_main_window` / `try_focus_main_window` | window activation utilities (not audio) |

Log literal (`0x16ab1ac`): `🔊 Audio system initialized (pre-decoded buffers ready)` (`\xf0\x9f\x94\x8a` = U+1F50A).
Decoding failures produce `BadAudio` / `OggError` / `FromUtf8Error` variants (`0x16b4e49`) — the decoder is `symphonia`/`rodio` (`rodio::source::empty::Empty<f32>`, `unsupported codec`, `id3v2`, `flac` strings present).

### 5.2 Sound types and assets

`play_audio` command signature: `{ sound_type: <string> }` (`0x11e4a9c` region; frontend wrapper `playAudio: async e => { await L('play_audio', { sound_type: e }) }` — `/tmp/hf/web/_assets_DismissibleHint-DA_pck3w.js`).

| `sound_type` | Asset | Offset |
|---|---|---|
| `task_added` | `/assets/sounds/task_added.mp3` | `0x11f7601` |
| `task_removed` | `/assets/sounds/task_removed.mp3` | `0x12284b2` |
| `session_ended` | `/assets/sounds/session_ended.mp3` | `0x127a83f` |

Exhaustive: only these three `/assets/sounds/*` paths exist in the binary. The `Rust→asset` mapping table is at `0x16ab194` (`task_removed`, `task_added`, `session_ended`).

### 5.3 Speech-to-text (Deepgram)

| Evidence | Value |
|---|---|
| Endpoint | `POST https://workers.hyperfocus.in/v1/transcription/deepgram-token` (`0x16b5746`, `0x34a9382`) |
| Request field | `issue_type` (`0x16b57xx`) |
| Response type | `DeepgramTranscriptionToken { access_token, expires_in }` (`0x34a9d0f`) |
| Tauri command | `get_deepgram_transcription_token` (`0x11dc047`) |
| Frontend worklet | `/assets/audio/deepgram-pcm-worklet.js` → extracted to `/tmp/hf/web/_audio_deepgram-pcm-worklet.js` |

So **the API key never reaches the client**; the backend mints a short-lived Deepgram token. Audio is captured and encoded client-side.

### 5.4 Worklet behaviour (frontend asset, quoted from source)

| Property | Value |
|---|---|
| Processor name | `deepgram-pcm-worklet` |
| Input sample rate | `processorOptions.inputSampleRate || globalThis.sampleRate || 48000` (48 kHz) |
| Output sample rate | `processorOptions.outputSampleRate || 16000` (**16 kHz**) |
| Frame size | `round(outputSampleRate * (frameMs || 80) / 1000)` = **1280 samples @ 80 ms** |
| Downsampling | box-filter average over `inputSampleRate/outputSampleRate` input samples per output sample |
| Encoding | linear PCM, **16-bit signed little-endian** (`DataView.setInt16(idx*2, value, true)`), negative samples scaled by `0x8000`, positive by `0x7fff` |
| Transfer | `port.postMessage({type:'audio', buffer, sampleCount}, [buffer])` — zero-copy `ArrayBuffer` transfer |
| Control | inbound `{type:'flush'}` → `flushPendingAudio()`, replies `{type:'flushed'}` |
| Debug | on construction posts `{type:'debug', inputSampleRate, outputSampleRate}` |

---

## 6. Entitlement & auth

### 6.1 Device fingerprint

| Symbol / literal | Evidence |
|---|---|
| `entitlement::device::read_machine_identifier` | runs `ioreg -c IOPlatformExpertDevice` (`0x16a45c7` region, literal `ioreg-cIOPlatformExpertDevice` at `0x16b??`) |
| `entitlement::device::fingerprint_from_machine_identifier` | hashes it |
| Salt / version marker | `hyperfocus-device-fingerprint-v1` (`0x128d52e`) |

The fingerprint is persisted in settings (`device_fingerprint`), used as the trial/refresh request body (`/v1/trial/start`, `/v1/entitlement/refresh` take `device_fingerprint`), and is bound into the token: mismatch produces `Cached access token is for a different device` (`0x16b4df0`).

### 6.2 Token format and verification

| Item | Evidence |
|---|---|
| Prefix check | x86_64 slice at `0x45aced`: `I\x83\xfd\x04 … A\x81?hft1` — a **4-byte prefix compare against `hft1`** with a length guard. The trailing `.` separator is **inferred** (no `hft1.` literal exists anywhere in the file). |
| Split points | `"token signature"` and `"token payload"` literals (`0x1294d89`, `0x308d9e5`) — the token is segmented manually, not via a JWT crate. |
| Header struct | `EntitlementTokenHeader { alg, typ, ver }` (3 fields, `0x34a3c1c`) — note the custom `ver` field. |
| Claims struct | `EntitlementTokenClaims { version, device_fingerprint, issued_at, expires_at, status, trial_expires_at }` (6 fields, `0x16a4523`) |
| Verification | `entitlement::verification::{verify_entitlement_token, verification_key_bytes}` |
| Signature algorithm | `ES256` appears **only** inside `rustls::crypto::ring::tls12::AES256_GCM` mangled names — **no `ES256` literal exists**. The linked verifier suites are `ring::ec::suite_b::ecdsa::verification::{ECDSA_P256_SHA256_ASN1, ECDSA_P256_SHA256_FIXED}`. **Inferred**: ECDSA P-256 + SHA-256 (JWS `ES256`), verified with an embedded public key via `ring`. Not provable statically because `ring` is also a rustls dependency. |
| Error strings | `access token is malformed`, `access token signature is invalid`, `access token payload is invalid`, `unsupported access token version` (`0x1294c40`, `0x308d9eb`) |

### 6.3 Statuses & summary

`EntitlementStatus` serde values (`0x16b4df0`, `0x34a3b94`): `trial`, `paid`, `blocked`.
`EntitlementSummary` (7 fields; struct literal `0x16a43e0`, field run `0x34a3b94`): `status`, `trial_expires_at`, `token_issued_at`, `token_expires_at`, `last_successful_refresh_at`, `has_active_entitlement`, `can_use_gated_features`.
`EntitlementResponseEnvelope` (2 fields): `{ entitlement_token, entitlement }`.
`entitlement::service::EntitlementService` API: `new`, `refresh_or_start`, `hosted_ai_token`, `fresh_hosted_ai_token`, `cached_summary`.
Errors seen: `Trial expired`, `InvalidTargetIneligible`, `Access refresh did not return a token`.

### 6.4 Hosted-AI access

`WorkerLlmClient` (`worker_client::{build, send_request, send_request_with_token, with_managed_entitlement, now_ms}`) attaches the entitlement-issued access token; `commands::ensure_hosted_ai_access_if_needed` is the pre-flight, and the error `Hosted AI access token is missing or stale` (`0x34a1448`) is the retry/refresh trigger. This is the most likely body of the `generate_agent`/`generate_json` provided methods (§1.2) — **inferred**, no decompilation performed.

### 6.5 OpenRouter credentials & OAuth PKCE

Keychain (`0x3496320`, `0x16a8b80`): service **`com.hyperfocus.desktop.openrouter`**, account/key **`api-key`**. Backed by `SystemOpenRouterCredentialStore` implementing `OpenRouterCredentialStore::{get,set,delete}` (`credentials::openrouter`); `PROTECTED_STORE_INITIALIZATION` is a lazy init guard.
Degraded-mode strings: `Could not access the OpenRouter credential`, `OpenRouter credentials are managed by .env. Edit OPENROUTER_API_KEY and restart the app`, `OpenRouter credential storage is unavailable on this platform`.

PKCE flow (`hyperfocus::openrouter_oauth`, `0x16a3956`, `0x1294c40`):

| Piece | Evidence |
|---|---|
| Symbols | `build_authorization_url`, `compute_pkce_challenge`, `generate_pkce_verifier`, `parse_callback_request`, `respond_and_close`, `cancellation_requested`, `OpenRouterOAuthCoordinator::{start_attempt, finish_attempt, cancel_attempt}` |
| Query params | `callback_url`, `code_challenge`, `code_challenge_method`, `code_verifier`, `key`, `key_label` |
| Authorize URL | `https://openrouter.ai/auth` |
| Key exchange | `GET/POST https://openrouter.ai/api/v1/auth/keys` |
| Keychain label | `key_label = "Hyperfocus"` |
| Loopback | `http://localhost` / `http://localhost:`; `callback_url` is the local callback; `Failed to generate random bytes for callback nonce` |
| Local state | `attempt_id` (Tauri command arg), `coordinator` state |
| Errors | `OpenRouter authorization expired or was rejected. Please try connecting again.`, `OpenRouter returned an empty API key. Please try connecting again.`, `OpenRouter returned an invalid response. Please try connecting again.`, `Could not finish connecting to OpenRouter. Please check your connection and try again.`, `OpenRouter connection attempt is invalid` |

The callback renders a self-contained HTML page (brand `HYPERFOCUS`, inline `<style>`, `.status-title`, `#22a447 / #d1252f / #9e9e9e` accent colours) with three outcomes:

| State | Title / Body |
|---|---|
| success | `OpenRouter connected` / `You can close this tab and return to Hyperfocus.` |
| failure | `OpenRouter connection failed` / `Connection failed` / `OpenRouter could not be connected. Return to Hyperfocus and try again.` |
| cancelled | `OpenRouter connection canceled` / `Connection canceled` / `The OpenRouter connection was canceled. You can close this tab and return to Hyperfocus.` |

Tauri commands: `connect_openrouter`, `cancel_openrouter`, `save_openrouter_api_key`, `remove_openrouter_api_key`, `get_ai_settings`, `set_ai_provider`.
`AiSettingsSnapshot` (`commands::settings::ai`) exposes `credential_is_configured`; `save_openrouter_api_key_core` / `remove_openrouter_api_key_core` are the testable cores.

### 6.6 `ai_provider` setting values

Enum `AiProvider` with serde values:

| Value | Meaning |
|---|---|
| `hyperfocus` | hosted worker (`workers.hyperfocus.in`) — requires an active entitlement |
| `openrouter` | user's own OpenRouter key from the keychain |

No third value exists (`0x16b3376`, `0x34a0b3e`). Note the current default model for the OpenRouter path is `google/gemini-3.7-flash` (`0x34aae0f`: `google/gemini-3.7-flash` is immediately followed by the "OpenRouter is selected, but no API key is saved" message, which is also the string immediately after `AiProvider` in the sibling slice).

---

## 7. Analytics

### 7.1 Service

| Symbol | Role |
|---|---|
| `hyperfocus::analytics::AnalyticsService::capture` (+ async closure) | fire-and-forget capture |
| `<AnalyticsService as AnalyticsCapture>::capture` | sync trait |
| `<AnalyticsService as AwaitableAnalyticsCapture>::capture_await` | awaitable trait for shutdown-critical events |
| `hyperfocus::analytics::build_event` | builds the payload |

Log lines: `Analytics service initialized and registered`, `Database service initialized and registered`.

### 7.2 Transport

| Item | Value | Offset |
|---|---|---|
| Endpoint | `https://us.i.posthog.com/i/v0/e/` | `0x34e5ca8` |
| Client lib | `posthog-rs` `0.3.7` (literals `posthog-rs`, `$lib_version`, `$lib_version__major/minor/patch`) | `0x34e5ca8` |
| Project API key | `phc_UhlUGwr66xAa392TPrk9jrffChddyFKnzhSL3BMB0lm` | `0x11e04d3`, `0x2fd8693` |
| Request body fields | `api_key`, `event`, `$distinct_id`, `properties`, `timestamp` | `0x34a1765`, `0x11dbe60` |

### 7.3 Event names and property keys

| Name | Kind | Offset |
|---|---|---|
| `user installed an app` | event | `0x11dbf3c` |
| `onboarding started` | event | `0x11dc2f0` |
| `onboarding skipped` | event | `0x34ab548` |
| `onboarding completed` | event | `0x34ab548` |
| `talk to founder opened` | event | `0x1696020` |
| `talk to founder closed` | event | `0x169600c` |
| `survey shown` | event (exit poll) | `0x16a4039` |
| `survey dismissed` | event (exit poll) | `0x16a4028` |
| `session started` | event | `0x11dfd8c` |
| `session finished` | event | `0x11dff00` |
| `turn_started` | event | `0x11daf66` (adjacent to `start_agent_conversation`) |
| `agent_conversation_write` | internal span/event | `0x11dac3f` |
| `prioritization_sync` | internal span/event | `0x11dbe82` (adjacent to `hyperfocus::ai::agent::service::turn` / `turn_error`) |

All of the above were also found in the second slice with matching adjacency, so the merges (`…turn_errorprioritization_sync`, `…openedpreview task input…`) are layout artefacts rather than single names.

Property keys:

| Key | Value / meaning |
|---|---|
| `surface` | `desktop_app` (`0x34a1765`) |
| `app` | `hyperfocus` (`0x2fd3eb0`) |
| `app_handledb` / `app_handle` | `app_handle` is the Tauri state param; the merged literal is an artefact |
| `$survey_id` | exit-poll survey id (`0x16a3ee6`) |
| `hint_id` | dismissible-hint id (`0x11e49bb`) |
| `outcome` | `OnboardingExitOutcome` — values `skipped`, `completed` (`0x169613c`) |
| `reason`, `follow_up` | exit-poll submission fields |
| `detected_week_start_day` | onboarding property (`0x11e49f0`) |
| `attempt_id` | OAuth attempt correlation (`0x11e1620`) |

PostHog distinct id = the settings `user_id` (a UUID; `uuid` literal adjacent to `$distinct_id` at `0x34a1765`).
The exit poll is submitted as a **PostHog survey** — `commands::exit_poll::{submission_properties, survey_id_property}` wrap `$survey_id` and the two answers.

`hyperfocus::analytics` also has a `Db`/`handle` shape: the run `hyperfocus::analyticssurfacedesktop_appapi_keyuuidevent$distinct_idpropertiestimestamp` (`0x34a1765`) is the concatenation of module path + property keys + transport fields.

---

## 8. Lifecycle / OS integration

### 8.1 Exit poll — `hyperfocus::exit_poll`

| Module | Symbols |
|---|---|
| `eligibility` | `derive_day_one_boundary_ms` |
| `lifecycle` | `ExitPollCoordinator::{begin_request, continue_app, exit_after_poll, require_poll_open}`, `deliver_eligible_poll`, `exit_normally`, `request_exit` |
| `macos_menu` | `build` |

Commands: `mark_exit_poll_listener_ready`, `acknowledge_exit_poll_shown` (+ `_core`), `dismiss_exit_poll` (+ `dismiss_exit_poll_core`), `continue_after_exit_poll`, `exit_after_exit_poll`. Menu item id **`exit-poll:open`** (`0x34b763b`). Errors: `Exit poll is not open`, `Exit poll check is no longer active`, `Exit-poll submission capture timed out`, `skipping submission capture` (`0x11e0000`).
Settings: `exit_poll_seen_at_ms`, `Settings::mark_exit_poll_seen`. Eligibility is keyed off a "day one" boundary derived from install time.

Reasons (union of Rust enum + frontend labels, `/tmp/hf/web/_assets_index-BZ_MKiTS.js` and `0x16a3ee6`):

| Code | UI label | Follow-up prompt |
|---|---|---|
| `just_exploring` | Just exploring | — |
| `done_for_now` | I'll be back | — |
| `not_useful_for_needs` | Didn't feel useful for what I need | What were you hoping Hyperfocus would help you do? |
| `missing_something` | Something I needed was missing | What were you looking for? |
| `not_sure_what_to_do_next` | Not sure how to use it | What was unclear? |
| `too_much_work` | Planning felt like too much work | What specifically felt like too much work? |
| `something_not_working` | Something wasn't working | What happened? |
| `other` | Other | Could you tell us more? |

(`did_mark_seen` also exists in the Rust enum — an internal "user already saw the poll" marker, not a user-facing option.) `follow_up` is capped at 2000 characters (frontend `cd=2e3`). The dialog letter is signed "Dmitri, the maker of Hyperfocus".

### 8.2 Session expiry worker — `hyperfocus::session_expiry_worker`

| Symbol | Role |
|---|---|
| `spawn_session_expiry_worker` | periodic tick task |
| `send_session_finished_notification` | delivers the OS notification |

SQL literally recovered (`0x11da607`, `0x11da6eb`):

```sql
-- untimed sessions
SELECT id FROM cycles_table
 WHERE type='session' AND started=true AND finished=false AND duration IS NULL
   AND COALESCE(archived, false)=false
 ORDER BY started_at ASC LIMIT 1

-- timed sessions past their end
SELECT id FROM cycles_table
 WHERE type='session' AND started=true AND finished=false AND started_at IS NOT NULL
   AND COALESCE(archived, false)=false AND started_at + duration <= ?
 ORDER BY started_at ASC
```

Notification body: `Time to take a break.` (`0x16b7218`); payload type `NotificationData` (20 fields incl. `id`, `title`, `subtasks`, `type`, `parent_id`; `0x34a9b6c`).
Failure paths (all logged): `session expiry worker skipped tick because analytics is not ready`, `session expiry worker skipped notification because plugin state is unavailable`, `session expiry worker failed to play session end sound`, `session expiry worker failed to finish session`, `session expiry worker failed to send notification`, `session expiry worker tick failed`. Note the coupling: the worker **skips** its tick when analytics is not ready — the same `analytics` handle must be present before expiry processing runs.

### 8.3 Database & backup — `hyperfocus::database`

| Symbol / literal | Evidence |
|---|---|
| `database::backup::{get_backups_path, get_database_path}` | paths |
| `database::service::DatabaseService::{connect_options, get_database_path, migration_phase, run_migrations_with_backup}` | SQLite via `sqlx` |
| `backups` directory, files `database_backup_*.db` | `0x11e0031` region |
| `Database backup created: {} ({} KB)` | `0x11e0000` |
| Manual recovery text | `🔧 MANUAL RECOVERY INSTRUCTIONS:` (`\xf0\x9f\x94\xa7` = U+1F527 at `0x11e00bd`) + `Your database backups are located at:` + `📁 <path>` + numbered steps `Find the latest backup file (database_backup_*.db)`, `Copy the backup file`, `Replace your database file at: <path>`, `Restart the application`, `If you need help, share this error message with support.` (`0x11e0165`–`0x11e0400`) |
| `MIGRATION FAILED!` | `0x11e0425` |
| Pragmas applied at connect | `PRAGMA journal_mode = WAL`, `PRAGMA wal_autocheckpoint = 100`, `PRAGMA synchronous = NORMAL`, `PRAGMA cache_size = 10000`, `PRAGMA temp_store = MEMORY` (`0x11e0165`, `0x2fd8693`) |
| `PRAGMA wal_checkpoint(FULL)` before backup | `0x11e0000` |
| Logs | `Connected to existing database`, `Database performance optimizations applied`, `🔄 Starting database migration...` |
| DB file | `local.db` (confirmed in `.../evidence/db/local.db`) |

Deletion cascades run with `PRAGMA foreign_keys = OFF` inside `BEGIN IMMEDIATE`, validated by a temp guard table `cycle_deletion_foreign_key_guard(violation_count INTEGER NOT NULL CHECK (violation_count = 0))` populated from `pragma_foreign_key_check` (`0x34b74f6`). A trigger `cycles with root colors must stay Long-term` raises `ABORT` when a cycle with `root_color_key` is retyped away from `month` (`0x34b7418`, `0x16a2200`).

### 8.4 Updater

| Item | Value |
|---|---|
| Endpoint | `https://s3.eu-central-1.amazonaws.com/app.hyperfocus/update.json` (`0x30936af`) |
| Plugin | `tauri-plugin-updater/2.10.0` (`0x16ab1ac` region, `0x2fdbcd1`) |
| Signing | minisign public key embedded (`dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6…` at `0x3093750`) — Tauri's `signature must be the contents of the \`.sig\` file` guard present |
| URL templates | `{{current_version}}`, `{{target}}`, `{{bundle_type}}` |
| Config keys | `endpoints`, `pubkey`, `headers`, `timeout`, `check`, `proxy`, `allowDowngrades`, `darwin`, `download_and_install`, `install`, `version`, `date`, `body`, `rid` |
| Errors | `update endpoint did not respond with a success…`, `\`signature\` field was not set on the updater response` |
| Platform type | `ReleaseManifestPlatform { url, signature }` / `{ url, … }` |
| Command | `restart_process` / `install_update` |

### 8.5 App menu & window configuration

Menu (`hyperfocus::exit_poll::macos_menu::build`, `0x34b763b`, `0x16a2200`): built with `muda` (`github.com/tauri-apps/muda`). Recovered items/ids: `Command+Q`, `quit-hyperfocus` / `Quit Hyperfocus`, `__tauri_window_menu__`, `__tauri_help_menu__`, `exit-poll:open`, `set_as_app_menu`, `set_as_window_menu`, `set_as_help_menu_for_nsapp`. Native item kinds seen: `About`, `Services`, `Hide`, `HideOthers`, `ShowAll`, `Quit`, `Close`, `Minimize`, `Zoom`-family, an extensive `NSImageName` enum (e.g. `Add`, `Bluetooth`, `Bookmarks`, `Caution`, `PreferencesGeneral`, `QuickLook`, `Share`, `Slideshow`, `SmartBadge`, `StatusAvailable`, `TrashEmpty`, …), plus `Super/Hyper/⌘` accelerator parsing and `NotAChildOfThisMenu`/`AcceleratorParseError` errors.

Window/webview config (Tauri `WindowConfig`, 57 fields; `0x16a3ee6`, `0x34a94e3`): `label`, `create`, `url`, `userAgent`, `dragDropEnabled`, `center`, `min/maxWidth`, `min/maxHeight`, `preventOverflow` (`PreventOverflowConfig`, an **untagged enum** — `data did not match any variant of untagged enum PreventOverflowConfig`), `backgroundThrottlingPolicy ∈ {disabled, suspend, throttle}`, `resizable`, `maximizable`, `minimizable`, `closable`, `fullscreen`, `focus`, `focusable`, `transparent`, `maximized`, `visible`, `decorations`, `alwaysOnBottom`, `alwaysOnTop`, `visibleOnAllWorkspaces`, `contentProtected`, `skipTaskbar`, `windowClassname`, `theme`, `titleBarStyle`, `trafficLightPosition`, `hiddenTitle`, `acceptFirstMouse`, `tabbingIdentifier`.
Window **effects**: `WindowEffectsConfig` with `followsWindowActiveState`, `WindowEffectState ∈ {followsWindowActiveState, active, inactive}`, `windowBackground`, `underWindowBackground`, `underPageBackground`, `hudWindow`, `fullScreenUI`, `tooltip`, `contentBackground`, `mica/micaDark/micaLight`, `tabbed/tabbedDark/tabbedLight`, `blur`, `acrylic`, style options `appearanceBased/light/dark/mediumLight/ultraDark` and `titlebar/menu/popover/sidebar/headerView/sheet`.
Dev/renderer URLs present: `http://localhost:3000/../out/renderer`, `/assets/index-BZ_MKiTS.js`, icons `icons/128x128.png`, `icons/128x128@2x.png`, `icons/icon.icns`.
CSP-ish nonces: `script-src__TAURI_SCRIPT_NONCE__`, `style-src__TAURI_STYLE_NONCE__` (`0x128cd4d`).

### 8.6 Other lifecycle surfaces

| Item | Evidence |
|---|---|
| `hyperfocus::meaningful_action::exists_since` | "meaningful action" gating helper used by analytics/exit-poll eligibility |
| `commands::getting_started_guide` | `apply_getting_started_guide_evidence`, `build_getting_started_guide_progress`, `skip_getting_started_guide_core` |
| `GettingStartedGuideStepId` | `set_long_term_goal`, `add_item_to_later`, `connect_weekly_to_long_term`, `connect_daily_to_weekly`, `complete_thirty_minute_focus_session` (`0x16a3df3`; labels in `_assets_index-BZ_MKiTS.js`: "Set one clear long-term goal", "Add goal to “Do Later”", "Connect weekly goal to long-term goal", "Connect daily task to weekly goal", "Complete 30-min focus block") |
| `GettingStartedGuideStatus` | `active`, `skipped`, `completed` |
| `commands::feedback` | `build_feedback_payload`, `map_transport_error`, `normalize_version`, `read_macos_product_version`, `resolve_macos_version`; endpoint `/v1/feedback`; payload `{email, installation_id, app_version, macos_version, feedback_id}`; macOS version via `sw_vers -productVersion`; UI strings `…t send feedback. Check your connection and try again.`, `Feedback request timed out. Please try again.` |
| `events::{agent,cycles,tasks}` | Tauri events `agent:conversation_updated`, `tasks:patched`, `cycle:finished` (`0x16a3d??`, `0x2fdc8c6`); each has an `emit_*_after_commit` helper so the UI never sees uncommitted state |
| `commands::talk_to_founder` | `close_talk_to_founder_core`, `TalkToFounderCloseReason ∈ {maybe_later, no_thanks, book}`; command `record_talk_to_founder_opened` |
| Dismissible hints | `DismissibleHint` component (`_assets_DismissibleHint-DA_pck3w.js`); commands `get_dismissed_hints`, `dismiss_hint` (`{hint_id}`); ids include `talk-to-founder-invitation` and `header-action-invitation` (frontend) and `under-invitation` (binary `0x11da220`) |

---

## 9. Settings model — `hyperfocus::settings::Settings`

### 9.1 Persisted `AppSettings` keys

Recovered as one contiguous literal run at `0x16b3341` (`Settings` and `AppSettings` both present, field list in declaration order):

| Key | Type (inferred) | Notes |
|---|---|---|
| `user_id` | string/UUID | also the PostHog distinct id (`Settings::initialize_user_id_if_needed`) |
| `installed_at_ms` | i64 epoch ms | drives `exit_poll::eligibility::derive_day_one_boundary_ms` |
| `exit_poll_seen_at_ms` | i64 epoch ms | `Settings::mark_exit_poll_seen` |
| `ai_provider` | `AiProvider` | `hyperfocus` \| `openrouter` |
| `device_fingerprint` | string | `hyperfocus-device-fingerprint-v1`; sent to trial/refresh endpoints |
| `entitlement_token` | string (JWT-ish, `hft1` prefixed) | verified on read |
| `entitlement_last_refreshed_at` | epoch | refresh scheduling |
| `week_start_day` | int 1..=7 | validated: `Week start day must be between 1 and 7` (`0x11e38e9`); commands `get_week_start_day`, `get_or_initialize_week_start_day` |
| `is_onboarding` | bool | onboarding done flag |
| `dismissed_hints` | array of strings | commands `get_dismissed_hints`, `dismiss_hint` |
| `getting_started_guide` | `GettingStartedGuideSettings { steps, id, completed_step_ids }` | serialisable sub-struct |

### 9.2 Files and access model

| Item | Evidence |
|---|---|
| `config.toml` | `0x16b3407` — the settings file |
| `USER.md` | `0x16b3400` — a sibling Markdown file (user-profile/prompt material) |
| Settings directory | a `hyperfocus` directory name appears at `0x30936af` alongside the updater endpoint; Tauri resolves it under `Library/Application Support` (`0x178e92c`). **The exact joined path was not recovered — inferred.** |
| Access API | `Settings::{new, with_read, with_mutation}` — a read/write-lock wrapper |
| Lock failure policy | `Settings lock was poisoned; continuing with recovered state` (`0x16b3440`) — intentional fail-open on mutex poisoning |
| Related SQLite `settings`-like state | `commands::settings::{dismiss_hint_core, get_dismissed_hints_core}` back the hint list |

### 9.3 Commands touching settings

`get_week_start_day`, `get_or_initialize_week_start_day`, `get_ai_settings`, `set_ai_provider`, `save_openrouter_api_key`, `remove_openrouter_api_key`, `cancel_openrouter`, `connect_openrouter`, `get_dismissed_hints`, `dismiss_hint`, `get_onboarding`, `complete_onboarding`, `skip_getting_started_guide`, `reconcile_getting_started_guide`, `get_entitlement_summary`, `get_entitlement_support_id`, `mark_exit_poll_listener_ready`, `acknowledge_exit_poll_shown`, `dismiss_exit_poll`, `continue_after_exit_poll`, `exit_after_exit_poll` (`0x2fdcbfb`, `0x2fdcc47`, `0x2fdcce1`, `0x11e46fc`, `0x11e3a28`).

### 9.4 Plan-selection input (adjacent settings-shaped model)

`PlanSelectionInput { week_id, day_id, selection }` (3 fields); `CreateNext` enum with `selected`, `cycles`, `create_next`; `long_term_cycle_id`, `target_cycle_id`, `source_cycle_id`, `copied_task_count` (copy-previous feature).

---

## Evidence limits

1. **No decompilation was performed.** The `reverse_engine` MCP instance pool had no analysed copy of this binary, and uploading a 66 MB Mach-O exceeded the available budget (comparable 5 MB Mach-O uploads had taken ~1.9 h wall-clock to analyse). Every claim here is from symbol names, string literals, and frontend assets. Control flow, arithmetic constants, and struct layouts are therefore **not** verified.
2. **The binary is stripped except for a compact legacy-mangled name table.** Function *names* are available (1,277 `hyperfocus` paths after demangling the `__TEXT,__const` name table); function *bodies* are not attributable without disassembly. Where a name implies behaviour, the claim is marked **inferred**.
3. **JSON schemas are built at runtime.** `get_extraction_response_schema`, `get_parent_matching_schema` and the four `prompt::shared::*_schema` builders compose schema objects in code; no serialized schema document exists in const data. The field lists and descriptions in §4.3 are complete, but the emitted JSON (types, `required` arrays, `enum` members) could not be captured verbatim.
4. **Prompt placeholder values are not recoverable.** The four `{}` slots in the parent-matching template (§4.4) sit in registers at runtime. Adjacent literals (`weekly`, `daily`, `monthly`) strongly suggest cycle-type labels, but the exact slot→value binding is not proven.
5. **Retry/timeout behaviour for the LLM layer is unproven.** No application-level retry, backoff, max-attempts or timeout constant exists in const data; the only `retry`/`max_retries` literals belong to `reqwest`. The `generate_agent` / `generate_agent_once` split is the only structural hint and its body is not decoded.
6. **`ES256` is inferred, not observed.** No `ES256` literal exists. `ring`'s `ECDSA_P256_SHA256_ASN1` / `_FIXED` verifiers are linked, but `ring` is also a `rustls` dependency, so this is not conclusive evidence that the entitlement token uses ES256.
7. **`hft1.` — only the `hft1` portion is proven.** The instruction at `0x45aced` compares exactly 4 bytes (`I\x83\xfd\x04` guard + `A\x81?hft1`). No `.` separator literal exists in the binary. The three-segment `header.payload.signature` reading follows from the `token payload` / `token signature` split literals and the `EntitlementTokenHeader`/`Claims` structs, and is **inferred**.
8. **The settings file path is not resolved.** `config.toml`, `USER.md`, `Application Support` and a `hyperfocus` directory fragment all exist, but the assembled path was not recovered.
9. **Some literal boundaries in the const section are ambiguous.** A block at `0x16a2f00`–`0x16a2f80` (exit-poll option ids, tool error codes) shows overlapping/suffix-merged fragments (e.g. `invalid_cycle_ke`+`y`, `under-invitation`+`talk-to-founder-5`). Exit-poll options were cross-validated against the frontend JS, so they are trustworthy; the error-code list in §3.2 was reconstructed from the same region and carries a small risk of a mis-split identifier.
10. **`update_goal`'s 7th field is unresolved.** Six of seven fields are named and described (§3.2); the seventh is only known to exist (`UpdateGoalArgs with 7 elements`) and to relate to `update_goal::resolved_clear_only_flag`. `delete_goal`'s third field is inferred from linker dedup slots rather than observed.
11. **`eval_judge_response` has no producer visible in the symbol table** — an eval harness may live in a build-time-only or feature-gated module.
12. **Analytics event list is a lower bound.** Only literals that are statically adjacent to the analytics module were recovered; event names constructed via `format!` or held in a non-const enum table would be missed.
13. **The `surface=desktop_app` vs `app=hyperfocus` property attribution is ambiguous** — both values are present in adjacent literals at `0x2fd3eb0` / `0x34a1765` but the key/value pairing is not distinguishable from const layout alone.

## Exact commands used

```bash
# 0. Orientation
ls -la "/Volumes/hyperfocus 1/hyperfocus.app/Contents/MacOS/hyperfocus"
ls -la /Users/lordcasser/workspace/projects/goal/analysis/evidence/{binary,db}/
cat  /Users/lordcasser/workspace/projects/goal/analysis/evidence/binary/{module_tree.txt,commands.txt}

# 1. URL / model-id sweep over the ready-made strings dump
python3 -c "import re;d=open('/tmp/hf/strings_arm64.txt',encoding='utf-8',errors='ignore').read();
print('\n'.join(sorted(set(re.findall(r'https?://[A-Za-z0-9\.\-_/:@%~\?=&\+]+',d)))))"
python3 -c "import re;d=open('/tmp/hf/strings_arm64.txt',encoding='utf-8',errors='ignore').read();
print('\n'.join(sorted(set(re.findall(r'[a-zA-Z0-9\.\-_/:]*gemini[a-zA-Z0-9\.\-_/:]*',d)))))"

# 2. Byte-window helpers (raw bytes, not `strings`)
cat > /tmp/hf/dump.py     # find_all(needle) / show(off) / seg(off) over raw bytes
cat > /tmp/hf/region.py   # python3 region.py 0xSTART 0xEND  -> printable window with offsets
python3 /tmp/hf/dump.py f 'openrouter.ai/api/v1/chat/completions' 700
python3 /tmp/hf/region.py 0x128d000 0x1298000 | grep -v '^$'      # goal-setting prompt
python3 /tmp/hf/region.py 0x11dc030 0x11dc600                     # command list + deepgram
python3 /tmp/hf/region.py 0x34b7600 0x34b7800                     # macOS menu / exit-poll
python3 -c "d=open('/Volumes/hyperfocus 1/hyperfocus.app/Contents/MacOS/hyperfocus','rb').read();
print(repr(d[0x128d82a:0x128d9b6]))"                              # parent-matching template
python3 -c "d=open('/Volumes/hyperfocus 1/hyperfocus.app/Contents/MacOS/hyperfocus','rb').read();
print(repr(d[0x16b3341:0x16b3450]))"                              # AppSettings + config.toml

# 3. Offset-indexed literal index (16+ char printable runs) — used for every "list literals in [A,B]" query
python3 -c "import re
d=open('/Volumes/hyperfocus 1/hyperfocus.app/Contents/MacOS/hyperfocus','rb').read()
out=open('/tmp/hf/lits.txt','w')
for m in re.finditer(rb'[\x20-\x7e\t]{16,}', d):
    out.write('%08x\t%d\t%s\n'%(m.start(),len(m.group(0)),m.group(0).decode('utf-8','replace')))"
python3 - <<'EOF'   # list literals in a range
for line in open('/tmp/hf/lits.txt',encoding='utf-8',errors='replace'):
    off,s = line.split('\t',2)[0], line.split('\t',2)[2].rstrip('\n')
    if 0x2fd2000 <= int(off,16) <= 0x2fe4000: print(off, s[:300])
EOF

# 4. Symbol recovery — legacy-mangle demangler over nm output, then the const name table
python3 /tmp/hf/extract_paths.py /tmp/hf/nm_arm64.txt llm 100        # original helper (needle in path segments)
grep -o '__ZN[0-9A-Za-z_$.]*' /tmp/hf/nm_arm64.txt | grep hyperfocus | sort -u | wc -l   # -> 1780 unique tokens
# custom demangler: legacy segment parse + $LT$/$u20$/… unescape + '..' -> '::'
#   -> /tmp/hf/syms.txt (12,286 paths), /tmp/hf/syms_hf.txt (1,277 hyperfocus paths)
python3 -c "import re
d=open('/Volumes/hyperfocus 1/hyperfocus.app/Contents/MacOS/hyperfocus','rb').read()
pat=re.compile(rb'([A-Za-z_][A-Za-z0-9_:<>,\{\}\(\) \[\]\.&\*\+\-#!\$%@\'\|\^/=~\`\\\x80-\xff]{2,120}?)17h[0-9a-f]{16}E')
print(len({m.group(1) for m in pat.finditer(d)}))"                    # name table -> 14,541 tokens
grep -E 'hyperfocus::ai::llm'   /tmp/hf/syms_hf.txt | grep -v 'drop_in_place\|::clone\|Debug\|Serialize\|Deserialize\|from_row'
grep -E 'hyperfocus::ai::agent' /tmp/hf/syms_hf.txt | grep -v 'drop_in_place\|::clone\|Debug\|Serialize\|Deserialize\|from_row'

# 5. Reachability / xref of specific literals
python3 -c "import re;d=open('/Volumes/hyperfocus 1/hyperfocus.app/Contents/MacOS/hyperfocus','rb').read()
for w in [b'hft1',b'ES256',b'max_retries',b'phc_',b'Deepgram',b'USER.md',b'config.toml',b'ai_provider']:
    print(w,[hex(m.start()) for m in list(re.finditer(re.escape(w),d))[:8]])"

# 6. Frontend corroboration (assets already extracted, read-only)
cat  /tmp/hf/web/_audio_deepgram-pcm-worklet.js
grep -oE 'play_audio[^)]{0,120}' /tmp/hf/web/*.js
grep -oE 'hintId:[^,}]{0,40}|invitation[a-zA-Z_\-]*' /tmp/hf/web/_assets_index-BZ_MKiTS.js | sort -u
python3 -c "t=open('/tmp/hf/web/_assets_index-BZ_MKiTS.js',encoding='utf-8',errors='replace').read()
i=t.find('something_not_working'); print(repr(t[i-2500:i+1500]))"
grep -oE '"?(goal_sized_items|task_sized_items|linked_items|active_items|needs_refinement_items|needs_breakdown_items)"?' /tmp/hf/web/_assets_index-BZ_MKiTS.js | sort -u
```

## Artefacts produced by this analysis (scratch, outside the report path)

| Path | Contents |
|---|---|
| `/tmp/hf/dump.py`, `/tmp/hf/region.py` | byte-window extractors |
| `/tmp/hf/lits.txt` | 122,479 offset-indexed printable runs (16+ chars) |
| `/tmp/hf/syms.txt`, `/tmp/hf/syms_hf.txt` | 12,286 demangled symbols / 1,277 `hyperfocus` symbols |
| `/tmp/hf/rustc_names.txt` | 14,540 raw tokens from the compact name table |
| `/tmp/hf/p1.txt` … `/tmp/hf/p5.txt` | cleaned prompt-region dumps |
| `/tmp/hf/longruns.txt`, `/tmp/hf/prose_u.txt` | longest printable runs (used to locate prompt blobs) |

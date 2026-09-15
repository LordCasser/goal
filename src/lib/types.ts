/**
 * Wire types for the IPC boundary, derived 1:1 from the Rust definitions
 * (src-tauri/src/domain/* and src-tauri/src/service/*).
 *
 * serde on the Rust side does NOT rename to camelCase: every field keeps its
 * snake_case name (`cycle_type`, `parent_id`, …) and enums carrying
 * `#[serde(rename_all = "lowercase")]` travel as lowercase string literals.
 * `Option<T>` arrives as `T | null`. The one explicit rename is
 * `Cycle.cycle_type`, serialized as `type` via `#[serde(rename = "type")]`.
 *
 * Input types mark `Option` fields optional (`?: T | null`): serde accepts a
 * missing key for `Option` fields (deserializing `None`), so callers may omit
 * them. Output fields are always present unless the Rust side skips them
 * (the only skip is `Subtask.children` while empty).
 */

/** `domain::cycle::CycleType`. `month` is the product's "Long-term cycle". */
export type CycleType = "session" | "day" | "week" | "month";

/** `domain::cycle::LATER_CYCLE_ID` — id of the permanent Do Later container. */
export const LATER_CYCLE_ID = "later";

/** One row of `cycles`, as it travels across IPC (`domain::cycle::Cycle`). */
export interface Cycle {
  id: string;
  title: string;
  /** Serialized as `type` (`#[serde(rename = "type")]`). */
  type: CycleType;
  parent_id: string | null;
  position: number;
  archived: boolean;
  started: boolean;
  finished: boolean;
  /** Wall-clock milliseconds since the Unix epoch. */
  started_at: number | null;
  finished_at: number | null;
  /** Milliseconds; null = duration not set yet (commitment not made). */
  duration: number | null;
  /** Accumulated focus time in milliseconds. */
  focused_time: number;
  /** Local dates `YYYY-MM-DD`; never UTC-derived, so "which day" cannot drift. */
  starts_on: string | null;
  ends_on: string | null;
  /**
   * Composite calendar identity (`day:…` / `week:…` / `long-term:…:…`);
   * null for sessions and the Later container.
   */
  calendar_key: string | null;
  /** Template this session was generated from; cleared, never cascaded. */
  repeat_id: string | null;
  /** Milliseconds since the Unix epoch. */
  created_at: number;
}

/**
 * `domain::cycle::LifecycleState`. Not serialized over IPC — the Rust side
 * derives it from the `started` / `finished` flags, so the frontend derives it
 * the same way. `null` mirrors the flag combination the DB CHECK
 * (`NOT (started = 0 AND finished = 1)`) makes unrepresentable.
 */
export type LifecycleState = "not_started" | "started" | "finished";

export function lifecycleOf(cycle: Cycle): LifecycleState | null {
  if (cycle.finished) return cycle.started ? "finished" : null;
  return cycle.started ? "started" : "not_started";
}

/** `domain::task::Subtask`. `children` is skipped while empty, hence optional. */
export interface Subtask {
  title: string;
  completed: boolean;
  children?: Subtask[];
}

/** Arbitrary JSON, matching `serde_json::Value`. */
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

/** `domain::proposal::ProposalKind`. */
export type ProposalKind = "upsert" | "delete";

/** `domain::task::Task` — goals, week items, daily tasks and subtasks alike. */
export interface Task {
  id: string;
  cycle_id: string;
  parent_id: string | null;
  title: string;
  subtasks: Subtask[];
  position: number;
  completed: boolean;
  goal_breakdown: Json | null;
  /**
   * Clarity flags are tri-state: null = "never evaluated", false = "evaluated
   * and fine" — the two mean different things and must not collapse
   * (spec: 清晰度标记三态语义).
   */
  needs_refinement: boolean | null;
  needs_breakdown: boolean | null;
  root_color_key: string | null;
  copied_from_task_id: string | null;
  /** Non-null marks a pending agent proposal; committed data is always null. */
  proposal: ProposalKind | null;
  created_at: number;
}

/** `domain::task::TaskNode` — `Task` is `#[serde(flatten)]`ed into it. */
export interface TaskNode extends Task {
  children: TaskNode[];
  /** Subtasks rendered as Markdown for AI context and sessions. */
  subtasks_markdown: string;
}

/** `domain::repeat::Repeat`. */
export interface Repeat {
  id: string;
  title: string;
  /** Milliseconds; copied into newly generated sessions. */
  duration: number;
  position: number;
  /** Archived templates ("Stop repeating") stop generating instances. */
  archived: boolean;
}

/** `service::cycles::PlannerState` — the full planner working set. */
export interface PlannerState {
  /** All visible month/week/day cycles; the frontend groups them by parent. */
  cycles: Cycle[];
  /** The permanent Do Later container. */
  later: Cycle;
}

/** `service::cycles::CreateCycleArgs`. */
export interface CreateCycleArgs {
  /** `month` | `week` | `day`; sessions go through `addSession` instead. */
  cycle_type: "month" | "week" | "day";
  parent_id?: string | null;
  /** Long-term only: 1, 3 or 6 product months (28 days each). */
  duration_months?: number | null;
  /** Optional; dated cycles derive a title from their bounds when absent. */
  title?: string | null;
  /** Day cycles: `YYYY-MM-DD`. Defaults to today (local). */
  date?: string | null;
}

/** `service::cycles::AddSessionArgs`. */
export interface AddSessionArgs {
  day_cycle_id: string;
  title: string;
  /** Milliseconds; null leaves the focus block without a set duration. */
  duration_ms?: number | null;
  position?: number | null;
}

/** `service::cycles::CycleDeletionPreview`. */
export interface CycleDeletionPreview {
  cycle_id: string;
  /** null = deletable; otherwise the stable guard code to map copy from. */
  guard_code: string | null;
  guard_message: string | null;
  descendant_cycles: number;
  tasks: number;
  started_sessions: number;
}

/** `service::tasks::AddTaskArgs`. */
export interface AddTaskArgs {
  cycle_id: string;
  title: string;
  /** Subtask row this new row hangs under; must live in the same cycle. */
  parent_id?: string | null;
  position?: number | null;
  subtasks?: Subtask[] | null;
  /** Manual long-term goals default to "needs refinement" when omitted. */
  needs_refinement?: boolean | null;
  needs_breakdown?: boolean | null;
}

/**
 * `service::tasks::TaskPatch`. Omitted (or null) fields stay unchanged.
 * Note: resetting a clarity flag to "unevaluated" is not expressible here —
 * JSON null deserializes to the outer `None` ("leave unchanged") of serde's
 * `Option<Option<bool>>`, never to "set to NULL".
 */
export interface TaskPatch {
  title?: string | null;
  subtasks?: Subtask[] | null;
  completed?: boolean | null;
  goal_breakdown?: Json | null;
  needs_refinement?: boolean | null;
  needs_breakdown?: boolean | null;
}

/**
 * `service::editor::EditorWorkspace`. `cycle` is null when the cycle does not
 * exist or is archived — callers render an empty state, not an error.
 */
export interface EditorWorkspace {
  cycle: Cycle | null;
  tasks: TaskNode[];
}

/** `service::proposals::PreviewSummary`. */
export interface PreviewSummary {
  cycle_id: string;
  count: number;
  /** Pending rows with their proposed content, for the highlight pass. */
  tasks: Task[];
}

/** `service::repeats::RepeatPatch` — edits future instances only. */
export interface RepeatPatch {
  title?: string | null;
  duration?: number | null;
  position?: number | null;
}

/** `service::settings::Settings`. */
export interface Settings {
  /** null until the user's week start day has been determined (1=Mon…7=Sun). */
  week_start_day: number | null;
  /** Preferred surface theme; null until first chosen (client defaults to white). */
  theme: Theme | null;
}

/** The two user-confirmed light themes (design.md §4.4). */
export type Theme = "white" | "gray";

/** Debug log verbosity, adjustable at runtime (spec: local-logging). */
export type LogLevel = "error" | "warn" | "info" | "debug";

/** `domain::task::ROOT_COLOR_KEYS` — the long-term goal palette. */
export const ROOT_COLOR_KEYS = [
  "red",
  "amber",
  "gold",
  "green",
  "teal",
  "blue",
  "indigo",
  "plum",
] as const;

export type RootColorKey = (typeof ROOT_COLOR_KEYS)[number];

// ---------------------------------------------------------------------------
// AI provider settings (change: add-ai-access-and-voice). Derived 1:1 from
// src-tauri/src/providers/config.rs and src-tauri/src/commands/ai_settings.rs:
// serde keeps snake_case field names and snake_case enum wire values, and
// `ProviderSummary` flattens `ProviderConfig` to one level.
// ---------------------------------------------------------------------------

/** `providers::config::ApiFormat` — decides endpoint path, auth header and SSE decoding. */
export type ApiFormat =
  | "anthropic_messages"
  | "openai_chat_completions"
  | "openai_responses";

/** `providers::config::InputType`. `text` is mandatory for every model. */
export type InputType = "text" | "image" | "video" | "pdf";

/** `providers::config::OutputType` — text is the only variant today. */
export type OutputType = "text";

/** `providers::config::ExtraHeader` — non-sensitive custom header, never credentials. */
export interface ExtraHeader {
  name: string;
  value: string;
}

/** `providers::config::ModelConfig` — user-declared capability metadata. */
export interface ModelConfig {
  /** Identifier sent to the API (e.g. `llama3`, `claude-sonnet-4`). */
  model_id: string;
  /** Informational (compaction decisions); not enforced by the sampler. */
  context_window: number;
  max_output_tokens: number;
  /** Must include "text". */
  input_types: InputType[];
  /** Can only be ["text"]. */
  output_types: OutputType[];
  /** User-declared (design D1); defaults to true when omitted on the wire. */
  supports_tools: boolean;
}

/** `providers::config::ProviderConfig`. Empty `id` means "add" (store assigns id). */
export interface ProviderConfig {
  id: string;
  name: string;
  /** Scheme included; may carry a path prefix (e.g. `https://host/v1`). */
  base_url: string;
  api_format: ApiFormat;
  extra_headers: ExtraHeader[];
  models: ModelConfig[];
  /** Unix epoch milliseconds. */
  created_at: number;
  /** Forward-compat only; deletion removes entries outright. */
  archived: boolean;
}

/**
 * `commands::ai_settings::ProviderSummary` — `ProviderConfig` is
 * `#[serde(flatten)]`ed, so its fields sit at the same level as the two
 * derived booleans. `has_api_key` is the keychain entry's existence only;
 * key material never crosses IPC in this direction.
 */
export interface ProviderSummary extends ProviderConfig {
  /** False for local endpoints without credentials — a normal state (design D4). */
  has_api_key: boolean;
  is_active: boolean;
}

/** `commands::ai_settings::AiSettingsSummary` — the `get_ai_settings` snapshot. */
export interface AiSettingsSummary {
  /** Resolved active provider; null when nothing is active or the id dangles. */
  active_provider: ProviderSummary | null;
  providers: ProviderSummary[];
  /** True iff the active provider exists; a keyless local endpoint counts. */
  ai_available: boolean;
}

/** `commands::ai_settings::ConnectionTestResult` — the design-D7 probe outcome. */
export interface ConnectionTestResult {
  ok: boolean;
  latency_ms: number | null;
  /** Stable design-D3 classification (`auth_failed`, `provider_unreachable`, …). */
  error_code: string | null;
  error_message: string | null;
}

/** One stored conversation message, payload parsed for rendering. */
export interface MessageView {
  id: string;
  sequence_number: number;
  message_type:
    | "user"
    | "model_text"
    | "model_function_call"
    | "function_result"
    | "app_tool_result";
  turn_id: string;
  payload: MessagePayload;
}

/** Tagged payloads stored per message type (mirrors turn.rs MessagePayload). */
export type MessagePayload =
  | { kind: "text"; text: string }
  | { kind: "model_text"; text: string }
  | { kind: "function_call"; id: string; name: string; arguments: unknown }
  | {
      kind: "function_result";
      tool_call_id: string;
      name: string;
      result: unknown;
      is_error: boolean;
    }
  | { kind: "app_tool_result"; name: string; result: unknown };

/** `ai::agent::turn::ConversationView` — one cycle's conversation. */
export interface ConversationView {
  id: string;
  cycle_id: string;
  revision: number;
  active_skill: string | null;
  last_error: string | null;
  messages: MessageView[];
}

/** `ai::agent::turn::TurnResult` — outcome of one sent message. */
export interface TurnResult {
  conversation_id: string;
  revision: number;
  active_skill: string | null;
  reply: string;
  messages: MessageView[];
}

/** `ai::review::PlanningIssue` — one diagnosed plan problem. */
export interface PlanningIssue {
  issue_type:
    | "too_many_goals"
    | "too_many_tasks"
    | "too_much_work"
    | "not_sure_what_to_do_next"
    | "missing_something"
    | "not_useful_for_needs";
  cycle_id: string;
  task_id: string | null;
  title: string;
  detail: string;
}

/** One persisted dismissal (cycle-level when task_id is null). */
export interface Dismissal {
  id: string;
  cycle_id: string;
  issue_type: string;
  task_id: string | null;
  reason: string | null;
  created_at: string;
}

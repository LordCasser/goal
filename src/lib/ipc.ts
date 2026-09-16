/**
 * The single place that talks to the Rust side.
 *
 * Command names are declared once, here, so the IPC surface can be grepped.
 * Rust returns failures as `{ code, message }` — see `docs/architecture.md`.
 * `code` is a stable token the UI maps to copy; the serializer-level fixed
 * ones are `not_found` / `db_error` / `internal`, everything else comes from
 * the use case (e.g. `past_cycle`, `calendar_key_taken`, `cycle_ended`).
 *
 * Tauri command arguments use camelCase keys by default. Nested serde structs
 * (`args`, `patch`, `config`) retain their Rust snake_case field names.
 */
import { invoke } from "@tauri-apps/api/core";

import type {
  AddSessionArgs,
  AddTaskArgs,
  AiSettingsSummary,
  ConnectionTestResult,
  ConversationView,
  CreateCycleArgs,
  Cycle,
  CycleDeletionPreview,
  TaskDeletionPreview,
  Dismissal,
  EditorWorkspace,
  LogLevel,
  PlanningIssueReport,
  PlannerState,
  PreviewSummary,
  ProviderConfig,
  Repeat,
  RepeatPatch,
  Settings,
  Task,
  TaskPatch,
  Theme,
  TurnResult,
} from "./types";

export * from "./types";

export type AppError = { code: string; message: string };

export function isAppError(e: unknown): e is AppError {
  return (
    typeof e === "object" &&
    e !== null &&
    typeof (e as { code?: unknown }).code === "string" &&
    typeof (e as { message?: unknown }).message === "string"
  );
}

export const commands = {
  // Window shell; these commands exist only in the Windows build.
  desktopShellState: "desktop_shell_state",
  desktopShellReady: "desktop_shell_ready",
  desktopShellRegions: "desktop_shell_regions",
  // cycles
  getPlannerState: "get_planner_state",
  listSessions: "list_sessions",
  createPlanningCycle: "create_planning_cycle",
  updateCycle: "update_cycle",
  getCycleDeletionPreview: "get_cycle_deletion_preview",
  deletePlanningCycle: "delete_planning_cycle",
  startCycle: "start_cycle",
  finishCycle: "finish_cycle",
  addSession: "add_session",
  reorderSessions: "reorder_sessions",
  copyUncompletedFromPrevious: "copy_uncompleted_from_previous",
  ensureDay: "ensure_day",
  // tasks
  addTask: "add_task",
  updateTask: "update_task",
  patchTask: "patch_task",
  deleteTask: "delete_task",
  getTaskDeletionPreview: "get_task_deletion_preview",
  moveTask: "move_task",
  reorderTasks: "reorder_tasks",
  setTaskParentLink: "set_task_parent_link",
  setTaskRootColor: "set_task_root_color",
  // editor workspaces
  getEditorWorkspace: "get_editor_workspace",
  getEditorWorkspacesByCycleIds: "get_editor_workspaces_by_cycle_ids",
  // do later
  addLaterGoal: "add_later_goal",
  promoteLaterGoal: "promote_later_goal",
  // agent proposals
  getPreviewSummary: "get_preview_summary",
  keepTaskPreview: "keep_task_preview",
  undoTaskPreview: "undo_task_preview",
  keepAllPreviews: "keep_all_previews",
  undoAllPreviews: "undo_all_previews",
  // repeats
  addRepeat: "add_repeat",
  updateRepeat: "update_repeat",
  stopRepeat: "stop_repeat",
  // settings
  getSettings: "get_settings",
  setWeekStartDay: "set_week_start_day",
  setTheme: "set_theme",
  setLocale: "set_locale",
  setLogLevel: "set_log_level",
  getAppFlag: "get_app_flag",
  setAppFlag: "set_app_flag",
  // ai settings (change: add-ai-access-and-voice)
  getAiSettings: "get_ai_settings",
  getAiAvailability: "get_ai_availability",
  saveProvider: "save_provider",
  deleteProvider: "delete_provider",
  setActiveProvider: "set_active_provider",
  saveProviderApiKey: "save_provider_api_key",
  removeProviderApiKey: "remove_provider_api_key",
  testProviderConnection: "test_provider_connection",
  // maintenance
  getSchemaVersion: "get_schema_version",
  exportBackup: "export_backup",
  getDebugLogDir: "get_debug_log_dir",
  // agent conversations & planning issues
  startAgentConversation: "start_agent_conversation",
  sendAgentMessage: "send_agent_message",
  getAgentConversation: "get_agent_conversation",
  getPreviousAgentConversation: "get_previous_agent_conversation",
  startPlanning: "start_planning",
  analyzePlanningPeriod: "analyze_planning_period",
  startGoalSetting: "start_goal_setting",
  startPrioritization: "start_prioritization",
  getPlanningIssueReport: "get_planning_issue_report",
  dismissPlanningIssue: "dismiss_planning_issue",
  getPlanningIssueDismissals: "get_planning_issue_dismissals",
} as const;

export type DesktopShellState = {
  mode: "pending" | "custom" | "native";
  revision: number;
  maximized: boolean;
  focused: boolean;
};
export type DesktopRegions = {
  revision: number;
  viewport: { width: number; height: number };
  maximize: { x: number; y: number; width: number; height: number };
  drag: { x: number; y: number; width: number; height: number };
};
export const getDesktopShellState = () => invoke<DesktopShellState>(commands.desktopShellState);
export const readyDesktopShell = (regions: DesktopRegions) => invoke<DesktopShellState>(commands.desktopShellReady, { regions });
export const updateDesktopRegions = (regions: DesktopRegions) => invoke<DesktopShellState>(commands.desktopShellRegions, { regions });

// --- cycles -----------------------------------------------------------------

export function getPlannerState(): Promise<PlannerState> {
  return invoke<PlannerState>(commands.getPlannerState);
}

/** Focus blocks of one day cycle, in column order; empty for non-days. */
export function listSessions(day_cycle_id: string): Promise<Cycle[]> {
  return invoke<Cycle[]>(commands.listSessions, { dayCycleId: day_cycle_id });
}

export function createPlanningCycle(args: CreateCycleArgs): Promise<Cycle> {
  return invoke<Cycle>(commands.createPlanningCycle, { args });
}

/** Focus blocks only — planning cycle titles/durations are create-time commitments. */
export function updateCycle(
  cycle_id: string,
  title: string,
  duration_ms: number | null,
): Promise<Cycle> {
  return invoke<Cycle>(commands.updateCycle, { cycleId: cycle_id, title, durationMs: duration_ms });
}

export function getCycleDeletionPreview(cycle_id: string): Promise<CycleDeletionPreview> {
  return invoke<CycleDeletionPreview>(commands.getCycleDeletionPreview, { cycleId: cycle_id });
}

export function deletePlanningCycle(cycle_id: string, confirmationToken?: string): Promise<void> {
  return invoke<void>(commands.deletePlanningCycle, { cycleId: cycle_id, confirmationToken: confirmationToken ?? null });
}

export function startCycle(cycle_id: string): Promise<Cycle> {
  return invoke<Cycle>(commands.startCycle, { cycleId: cycle_id });
}

export function finishCycle(cycle_id: string): Promise<Cycle> {
  return invoke<Cycle>(commands.finishCycle, { cycleId: cycle_id });
}

export function addSession(args: AddSessionArgs): Promise<Cycle> {
  return invoke<Cycle>(commands.addSession, { args });
}

export function reorderSessions(
  day_cycle_id: string,
  session_ids: string[],
): Promise<void> {
  return invoke<void>(commands.reorderSessions, { dayCycleId: day_cycle_id, sessionIds: session_ids });
}

export function copyUncompletedFromPrevious(cycle_id: string): Promise<Task[]> {
  return invoke<Task[]>(commands.copyUncompletedFromPrevious, { cycleId: cycle_id });
}

/** Find-or-create today's (or `date`'s) day column, materializing repeats. */
export function ensureDay(date?: string | null): Promise<Cycle> {
  return invoke<Cycle>(commands.ensureDay, { date });
}

// --- tasks ------------------------------------------------------------------

export function addTask(args: AddTaskArgs): Promise<Task> {
  return invoke<Task>(commands.addTask, { args });
}

/** Full editor save: title required, other fields optional. */
export function updateTask(
  task_id: string,
  title: string,
  patch: TaskPatch,
): Promise<Task> {
  return invoke<Task>(commands.updateTask, { taskId: task_id, title, patch });
}

/** Partial editor save: every field optional. */
export function patchTask(task_id: string, patch: TaskPatch): Promise<Task> {
  return invoke<Task>(commands.patchTask, { taskId: task_id, patch });
}

export function getTaskDeletionPreview(task_id: string): Promise<TaskDeletionPreview> {
  return invoke<TaskDeletionPreview>(commands.getTaskDeletionPreview, { taskId: task_id });
}

export function deleteTask(task_id: string, confirmationToken?: string): Promise<void> {
  return invoke<void>(commands.deleteTask, { taskId: task_id, confirmationToken: confirmationToken ?? null });
}

/** `position` omitted/null = append at the end of the target sibling group. */
export function moveTask(
  task_id: string,
  target_cycle_id: string,
  position?: number | null,
): Promise<Task> {
  return invoke<Task>(commands.moveTask, { taskId: task_id, targetCycleId: target_cycle_id, position });
}

/** `parent_id` scopes the reorder to one sibling group (null = top level). */
export function reorderTasks(
  cycle_id: string,
  parent_id: string | null,
  ordered_ids: string[],
): Promise<void> {
  return invoke<void>(commands.reorderTasks, { cycleId: cycle_id, parentId: parent_id, orderedIds: ordered_ids });
}

/** Cross-level link (weekly -> long-term, daily -> weekly); null unlinks. */
export function setTaskParentLink(
  task_id: string,
  parent_id: string | null,
): Promise<Task> {
  return invoke<Task>(commands.setTaskParentLink, { taskId: task_id, parentId: parent_id });
}

/** Long-term goal coloring; null clears. */
export function setTaskRootColor(
  task_id: string,
  color_key: string | null,
): Promise<Task> {
  return invoke<Task>(commands.setTaskRootColor, { taskId: task_id, colorKey: color_key });
}

// --- editor workspaces ------------------------------------------------------

export function getEditorWorkspace(cycle_id: string): Promise<EditorWorkspace> {
  return invoke<EditorWorkspace>(commands.getEditorWorkspace, { cycleId: cycle_id });
}

/** Batch form: results keyed by cycle id; unknown cycles map to empty. */
export function getEditorWorkspacesByCycleIds(
  cycle_ids: string[],
): Promise<Record<string, EditorWorkspace>> {
  return invoke<Record<string, EditorWorkspace>>(commands.getEditorWorkspacesByCycleIds, {
    cycleIds: cycle_ids,
  });
}

// --- do later ---------------------------------------------------------------

export function addLaterGoal(title: string): Promise<Task> {
  return invoke<Task>(commands.addLaterGoal, { title });
}

/** Pull a parked idea into a planning cycle (typically the long-term one). */
export function promoteLaterGoal(
  task_id: string,
  target_cycle_id: string,
): Promise<Task> {
  return invoke<Task>(commands.promoteLaterGoal, { taskId: task_id, targetCycleId: target_cycle_id });
}

// --- agent proposals --------------------------------------------------------

export function getPreviewSummary(cycle_id: string): Promise<PreviewSummary> {
  return invoke<PreviewSummary>(commands.getPreviewSummary, { cycleId: cycle_id });
}

export function keepTaskPreview(task_id: string): Promise<Task> {
  return invoke<Task>(commands.keepTaskPreview, { taskId: task_id });
}

export function undoTaskPreview(task_id: string): Promise<void> {
  return invoke<void>(commands.undoTaskPreview, { taskId: task_id });
}

/** Number of previews kept. */
export function keepAllPreviews(cycle_id: string): Promise<number> {
  return invoke<number>(commands.keepAllPreviews, { cycleId: cycle_id });
}

/** Number of previews reverted. */
export function undoAllPreviews(cycle_id: string): Promise<number> {
  return invoke<number>(commands.undoAllPreviews, { cycleId: cycle_id });
}

// --- repeats ----------------------------------------------------------------

export function addRepeat(session_id: string): Promise<Repeat> {
  return invoke<Repeat>(commands.addRepeat, { sessionId: session_id });
}

/** Edits the template only — future instances pick the change up. */
export function updateRepeat(
  repeat_id: string,
  patch: RepeatPatch,
): Promise<Repeat> {
  return invoke<Repeat>(commands.updateRepeat, { repeatId: repeat_id, patch });
}

export function stopRepeat(repeat_id: string): Promise<Repeat> {
  return invoke<Repeat>(commands.stopRepeat, { repeatId: repeat_id });
}

// --- settings ---------------------------------------------------------------

export function getSettings(): Promise<Settings> {
  return invoke<Settings>(commands.getSettings);
}

/** ISO weekday number: 1 = Monday … 7 = Sunday. */
export function setWeekStartDay(day: number): Promise<void> {
  return invoke<void>(commands.setWeekStartDay, { day });
}

export function setTheme(theme: Theme): Promise<void> {
  return invoke<void>(commands.setTheme, { theme });
}

export function setLocale(locale: import("./i18n").Locale): Promise<void> {
  return invoke<void>(commands.setLocale, { locale });
}

/** Applies immediately to the running logger and persists for next launch. */
export function setLogLevel(level: LogLevel): Promise<void> {
  return invoke<void>(commands.setLogLevel, { level });
}

/** Reads a non-sensitive UI flag (e.g. one-time hint dismissal); null = unset. */
export function getAppFlag(key: string): Promise<string | null> {
  return invoke<string | null>(commands.getAppFlag, { key });
}

export function setAppFlag(key: string, value: string): Promise<void> {
  return invoke<void>(commands.setAppFlag, { key, value });
}

// --- maintenance ------------------------------------------------------------

export function getSchemaVersion(): Promise<number> {
  return invoke<number>(commands.getSchemaVersion);
}

export function exportBackup(target_path: string): Promise<void> {
  return invoke<void>(commands.exportBackup, { targetPath: target_path });
}

/** Absolute path of the debug log directory, for the settings entry point. */
export function getDebugLogDir(): Promise<string> {
  return invoke<string>(commands.getDebugLogDir);
}

// --- ai settings (change: add-ai-access-and-voice) ---------------------------
//
// Shapes mirror src-tauri/src/commands/ai_settings.rs 1:1 (snake_case keys).
// `provider` is a whole-struct parameter, so it travels nested under its own
// name like `args`/`patch`. Summaries never contain key material; API keys and
// one-shot header replacements cross IPC only as save inputs. No AI settings
// event exists, so every mutator's caller invalidates the settings page query
// itself (features/ai-settings/AiSettingsPage.tsx).

/** Snapshot for the AI settings page; never contains key material. */
export function getAiSettings(): Promise<AiSettingsSummary> {
  return invoke<AiSettingsSummary>(commands.getAiSettings);
}

/** Persisted verification state only; this read never accesses credentials. */
export function getAiAvailability(): Promise<boolean> {
  return invoke<boolean>(commands.getAiAvailability);
}

/** Empty id adds (the store assigns id/created_at); non-empty id updates. */
export function saveProvider(
  provider: ProviderConfig,
  apiKey?: string | null,
  headerValues: Record<string, string> = {},
): Promise<ProviderConfig> {
  return invoke<ProviderConfig>(commands.saveProvider, {
    provider,
    apiKey: apiKey ?? null,
    headerValues,
  });
}

/** Also cascades the keychain entry away; deleting the active provider clears activation. */
export function deleteProvider(provider_id: string): Promise<void> {
  return invoke<void>(commands.deleteProvider, { providerId: provider_id });
}

export function setActiveProvider(provider_id: string, model_id: string): Promise<void> {
  return invoke<void>(commands.setActiveProvider, { providerId: provider_id, modelId: model_id });
}

/** Legacy API-key replacement path; key material is never read back. */
export function saveProviderApiKey(provider_id: string, api_key: string): Promise<void> {
  return invoke<void>(commands.saveProviderApiKey, { providerId: provider_id, apiKey: api_key });
}

/** Idempotent keychain cleanup for an existing provider. */
export function removeProviderApiKey(provider_id: string): Promise<void> {
  return invoke<void>(commands.removeProviderApiKey, { providerId: provider_id });
}

/** One minimal real request (design D7); classified failures come back as the result. */
export function testProviderConnection(provider_id: string): Promise<ConnectionTestResult> {
  return invoke<ConnectionTestResult>(commands.testProviderConnection, { providerId: provider_id });
}

// --- agent conversations & planning issues ----------------------------------

export function startAgentConversation(cycle_id: string): Promise<ConversationView> {
  return invoke<ConversationView>(commands.startAgentConversation, { cycleId: cycle_id });
}

export function sendAgentMessage(
  cycle_id: string,
  text: string,
  focused_task_id?: string | null,
): Promise<TurnResult> {
  return invoke<TurnResult>(commands.sendAgentMessage, {
    cycleId: cycle_id,
    text,
    focusedTaskId: focused_task_id ?? null,
  });
}

export function getAgentConversation(cycle_id: string): Promise<ConversationView | null> {
  return invoke<ConversationView | null>(commands.getAgentConversation, { cycleId: cycle_id });
}

export function getPreviousAgentConversation(cycle_id: string): Promise<ConversationView | null> {
  return invoke<ConversationView | null>(commands.getPreviousAgentConversation, { cycleId: cycle_id });
}

export function startPlanning(cycle_id: string): Promise<TurnResult> {
  return invoke<TurnResult>(commands.startPlanning, { cycleId: cycle_id });
}

export function startGoalSetting(cycle_id: string, task_id: string): Promise<TurnResult> {
  return invoke<TurnResult>(commands.startGoalSetting, { cycleId: cycle_id, taskId: task_id });
}

export function startPrioritization(cycle_id: string): Promise<TurnResult> {
  return invoke<TurnResult>(commands.startPrioritization, { cycleId: cycle_id });
}

export function getPlanningIssueReport(
  cycle_id: string,
  refresh = false,
): Promise<PlanningIssueReport> {
  return invoke<PlanningIssueReport>(commands.getPlanningIssueReport, { cycleId: cycle_id, refresh });
}

export function dismissPlanningIssue(
  cycle_id: string,
  issue_type: string,
  task_id?: string | null,
  reason?: string | null,
): Promise<void> {
  return invoke<void>(commands.dismissPlanningIssue, {
    cycleId: cycle_id,
    issueType: issue_type,
    taskId: task_id ?? null,
    reason: reason ?? null,
  });
}

export function getPlanningIssueDismissals(cycle_id: string): Promise<Dismissal[]> {
  return invoke<Dismissal[]>(commands.getPlanningIssueDismissals, { cycleId: cycle_id });
}

/** Read-only analysis over exact inclusive calendar dates. */
export function analyzePlanningPeriod(request: { start_date: string; end_date: string; question: string }): Promise<{ analysis: string; facts: { start_date: string; end_date: string; snapshot_at: number; basis: string; undated_cycles_excluded: number; cycles: Array<{ cycle: Cycle; tasks: Task[]; work_mix: import("./types").EditorWorkspace["work_mix"] }> } }> {
  return invoke(commands.analyzePlanningPeriod, { request });
}

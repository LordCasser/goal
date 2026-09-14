/**
 * The single place that talks to the Rust side.
 *
 * Command names are declared once, here, so the IPC surface can be grepped.
 * Rust returns failures as `{ code, message }` — see `docs/architecture.md`.
 * `code` is a stable token the UI maps to copy; the serializer-level fixed
 * ones are `not_found` / `db_error` / `internal`, everything else comes from
 * the use case (e.g. `past_cycle`, `calendar_key_taken`, `cycle_ended`).
 *
 * Invoke argument keys keep the exact Rust parameter names (snake_case):
 * Tauri 2 matches invoke keys against parameter names without any case
 * conversion. Parameters that are whole structs (`args`, `patch`) are passed
 * as one nested object under the parameter's own name.
 */
import { invoke } from "@tauri-apps/api/core";

import type {
  AddSessionArgs,
  AddTaskArgs,
  CreateCycleArgs,
  Cycle,
  CycleDeletionPreview,
  EditorWorkspace,
  LogLevel,
  PlannerState,
  PreviewSummary,
  Repeat,
  RepeatPatch,
  Settings,
  Task,
  TaskPatch,
  Theme,
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
  setLogLevel: "set_log_level",
  getAppFlag: "get_app_flag",
  setAppFlag: "set_app_flag",
  // maintenance
  getSchemaVersion: "get_schema_version",
  exportBackup: "export_backup",
  getDebugLogDir: "get_debug_log_dir",
} as const;

// --- cycles -----------------------------------------------------------------

export function getPlannerState(): Promise<PlannerState> {
  return invoke<PlannerState>(commands.getPlannerState);
}

/** Focus blocks of one day cycle, in column order; empty for non-days. */
export function listSessions(day_cycle_id: string): Promise<Cycle[]> {
  return invoke<Cycle[]>(commands.listSessions, { day_cycle_id });
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
  return invoke<Cycle>(commands.updateCycle, { cycle_id, title, duration_ms });
}

export function getCycleDeletionPreview(cycle_id: string): Promise<CycleDeletionPreview> {
  return invoke<CycleDeletionPreview>(commands.getCycleDeletionPreview, { cycle_id });
}

export function deletePlanningCycle(cycle_id: string): Promise<void> {
  return invoke<void>(commands.deletePlanningCycle, { cycle_id });
}

export function startCycle(cycle_id: string): Promise<Cycle> {
  return invoke<Cycle>(commands.startCycle, { cycle_id });
}

export function finishCycle(cycle_id: string): Promise<Cycle> {
  return invoke<Cycle>(commands.finishCycle, { cycle_id });
}

export function addSession(args: AddSessionArgs): Promise<Cycle> {
  return invoke<Cycle>(commands.addSession, { args });
}

export function reorderSessions(
  day_cycle_id: string,
  session_ids: string[],
): Promise<void> {
  return invoke<void>(commands.reorderSessions, { day_cycle_id, session_ids });
}

export function copyUncompletedFromPrevious(cycle_id: string): Promise<Task[]> {
  return invoke<Task[]>(commands.copyUncompletedFromPrevious, { cycle_id });
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
  return invoke<Task>(commands.updateTask, { task_id, title, patch });
}

/** Partial editor save: every field optional. */
export function patchTask(task_id: string, patch: TaskPatch): Promise<Task> {
  return invoke<Task>(commands.patchTask, { task_id, patch });
}

export function deleteTask(task_id: string): Promise<void> {
  return invoke<void>(commands.deleteTask, { task_id });
}

/** `position` omitted/null = append at the end of the target sibling group. */
export function moveTask(
  task_id: string,
  target_cycle_id: string,
  position?: number | null,
): Promise<Task> {
  return invoke<Task>(commands.moveTask, { task_id, target_cycle_id, position });
}

/** `parent_id` scopes the reorder to one sibling group (null = top level). */
export function reorderTasks(
  cycle_id: string,
  parent_id: string | null,
  ordered_ids: string[],
): Promise<void> {
  return invoke<void>(commands.reorderTasks, { cycle_id, parent_id, ordered_ids });
}

/** Cross-level link (weekly -> long-term, daily -> weekly); null unlinks. */
export function setTaskParentLink(
  task_id: string,
  parent_id: string | null,
): Promise<Task> {
  return invoke<Task>(commands.setTaskParentLink, { task_id, parent_id });
}

/** Long-term goal coloring; null clears. */
export function setTaskRootColor(
  task_id: string,
  color_key: string | null,
): Promise<Task> {
  return invoke<Task>(commands.setTaskRootColor, { task_id, color_key });
}

// --- editor workspaces ------------------------------------------------------

export function getEditorWorkspace(cycle_id: string): Promise<EditorWorkspace> {
  return invoke<EditorWorkspace>(commands.getEditorWorkspace, { cycle_id });
}

/** Batch form: results keyed by cycle id; unknown cycles map to empty. */
export function getEditorWorkspacesByCycleIds(
  cycle_ids: string[],
): Promise<Record<string, EditorWorkspace>> {
  return invoke<Record<string, EditorWorkspace>>(commands.getEditorWorkspacesByCycleIds, {
    cycle_ids,
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
  return invoke<Task>(commands.promoteLaterGoal, { task_id, target_cycle_id });
}

// --- agent proposals --------------------------------------------------------

export function getPreviewSummary(cycle_id: string): Promise<PreviewSummary> {
  return invoke<PreviewSummary>(commands.getPreviewSummary, { cycle_id });
}

export function keepTaskPreview(task_id: string): Promise<Task> {
  return invoke<Task>(commands.keepTaskPreview, { task_id });
}

export function undoTaskPreview(task_id: string): Promise<void> {
  return invoke<void>(commands.undoTaskPreview, { task_id });
}

/** Number of previews kept. */
export function keepAllPreviews(cycle_id: string): Promise<number> {
  return invoke<number>(commands.keepAllPreviews, { cycle_id });
}

/** Number of previews reverted. */
export function undoAllPreviews(cycle_id: string): Promise<number> {
  return invoke<number>(commands.undoAllPreviews, { cycle_id });
}

// --- repeats ----------------------------------------------------------------

export function addRepeat(session_id: string): Promise<Repeat> {
  return invoke<Repeat>(commands.addRepeat, { session_id });
}

/** Edits the template only — future instances pick the change up. */
export function updateRepeat(
  repeat_id: string,
  patch: RepeatPatch,
): Promise<Repeat> {
  return invoke<Repeat>(commands.updateRepeat, { repeat_id, patch });
}

export function stopRepeat(repeat_id: string): Promise<Repeat> {
  return invoke<Repeat>(commands.stopRepeat, { repeat_id });
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
  return invoke<void>(commands.exportBackup, { target_path });
}

/** Absolute path of the debug log directory, for the settings entry point. */
export function getDebugLogDir(): Promise<string> {
  return invoke<string>(commands.getDebugLogDir);
}

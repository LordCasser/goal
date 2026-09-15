/**
 * Tauri events → react-query invalidation.
 *
 * Events are invalidation notices only (src-tauri/src/events.rs): payloads
 * name cycles and never carry business data, so the database stays the single
 * source of truth (docs/architecture.md, decision D4). Only drag reordering
 * gets optimistic updates; everything else waits for these notices.
 *
 * Query key convention — feature modules MUST use the factories below and
 * never hand-write key arrays; the invalidation logic matches on these shapes:
 *
 *   qk.plannerState()               ["planner-state"]
 *   qk.editorWorkspace(cycleId)     ["editor-workspace", cycleId]
 *   qk.editorWorkspaces(cycleIds)   ["editor-workspaces", cycleIds]   (batch)
 *   qk.previewSummary(cycleId)      ["preview-summary", cycleId]
 *   qk.settings()                   ["settings"]
 *
 * Invalidation matrix (event names mirror the Rust constants):
 *   cycles:changed    {cycle_ids} → planner-state; editor-workspace per id;
 *                     the editor-workspaces batch root — batch responses embed
 *                     per-cycle data under a list key, so there is no
 *                     per-cycle key to target and the whole root is
 *                     invalidated (coarse but correct; refetching is cheap).
 *   tasks:changed     {cycle_ids} → planner-state; editor-workspace per id
 *                     (plus the batch root, as above); preview-summary per id
 *                     — task writes can rewrite proposal snapshots.
 *   proposals:changed {cycle_id}  → preview-summary for that cycle.
 *   ["settings"] is intentionally never touched here: no settings event
 *   exists (set_week_start_day emits nothing), so its mutators invalidate
 *   the settings query themselves.
 */
import type { QueryClient } from "@tanstack/react-query";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Mirrors `events::CycleIdsPayload`. */
export interface CycleIdsPayload {
  cycle_ids: string[];
}

/** Mirrors `events::CycleIdPayload`. */
export interface CycleIdPayload {
  cycle_id: string;
}

export const qk = {
  plannerState: () => ["planner-state"] as const,
  editorWorkspace: (cycleId: string) => ["editor-workspace", cycleId] as const,
  editorWorkspaces: (cycleIds: string[]) => ["editor-workspaces", cycleIds] as const,
  previewSummary: (cycleId: string) => ["preview-summary", cycleId] as const,
  sessions: (dayCycleId: string) => ["sessions", dayCycleId] as const,
  agentConversation: (cycleId: string) => ["agent-conversation", cycleId] as const,
  issueReport: (cycleId: string) => ["issue-report", cycleId] as const,
  settings: () => ["settings"] as const,
  agentActions: (cycleId: string) => ["agent-actions", cycleId] as const,
};

/**
 * Subscribes to the backend change events and wires them to cache
 * invalidation. Returns a cleanup function that unlistens everything; call it
 * once at app teardown.
 */
export async function initEventInvalidation(queryClient: QueryClient): Promise<() => void> {
  const unlisteners: UnlistenFn[] = [];
  try {
    unlisteners.push(await listen("agent:actions_changed", () => invalidateAgentEffects(queryClient)));
    unlisteners.push(
      await listen<CycleIdsPayload>("cycles:changed", (event) => {
        invalidateCycles(queryClient, event.payload.cycle_ids, { taskWrites: false });
      }),
    );
    unlisteners.push(
      await listen<CycleIdsPayload>("tasks:changed", (event) => {
        invalidateCycles(queryClient, event.payload.cycle_ids, { taskWrites: true });
      }),
    );
    unlisteners.push(
      await listen<CycleIdPayload>("proposals:changed", (event) => {
        queryClient.invalidateQueries({ queryKey: qk.previewSummary(event.payload.cycle_id) });
      }),
    );
    unlisteners.push(
      await listen<{ conversation_id: string; cycle_id: string; revision: number }>(
        "agent:conversation_updated",
        (event) => {
          invalidateAgentEffects(queryClient);
          queryClient.invalidateQueries({
            queryKey: qk.agentConversation(event.payload.cycle_id),
          });
        },
      ),
    );
  } catch (err) {
    // Never leak the listeners acquired before the failure.
    for (const unlisten of unlisteners) unlisten();
    throw err;
  }
  return () => {
    for (const unlisten of unlisteners) unlisten();
  };
}

/** A tool turn may stage edits in another cycle, including before an error.
 * Invalidate existing projections after every settled turn; no second state store. */
export function invalidateAgentEffects(queryClient: QueryClient): void {
  for (const root of ["planner-state", "editor-workspace", "editor-workspaces", "preview-summary", "issue-report", "agent-actions", "settings", "app-flag", "ai-settings", "ai-availability", "agent-conversation", "sessions", "calendar", "calendar-range", "schedule-overlaps", "reminders", "repeats", "time-budget", "daily-capacity"]) {
    void queryClient.invalidateQueries({ queryKey: [root] });
  }
}

/** Shared body of the two `{ cycle_ids }` events; see the matrix above. */
function invalidateCycles(
  queryClient: QueryClient,
  cycleIds: string[],
  opts: { taskWrites: boolean },
): void {
  if (cycleIds.length === 0) return; // emitters skip empty sets; stay defensive
  queryClient.invalidateQueries({ queryKey: qk.plannerState() });
  queryClient.invalidateQueries({ queryKey: ["issue-report"] });
  // Length-1 root prefix-matches every batch key built by qk.editorWorkspaces.
  queryClient.invalidateQueries({ queryKey: ["editor-workspaces"] });
  if (opts.taskWrites) queryClient.invalidateQueries({ queryKey: ["editor-workspace"] });
  for (const cycleId of cycleIds) {
    queryClient.invalidateQueries({ queryKey: qk.editorWorkspace(cycleId) });
    // Session mutations (add/start/finish/repeat) emit cycles:changed for the
    // day, so the day's focus-block listing refreshes alongside the columns.
    queryClient.invalidateQueries({ queryKey: qk.sessions(cycleId) });
    if (opts.taskWrites) {
      queryClient.invalidateQueries({ queryKey: qk.previewSummary(cycleId) });
    }
  }
}

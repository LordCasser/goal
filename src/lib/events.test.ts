import { expect, it, vi } from "vitest";
import { QueryClient } from "@tanstack/react-query";
const handlers = vi.hoisted(() => new Map<string, (event: unknown) => void>());
vi.mock("@tauri-apps/api/event", () => ({ listen: async (name: string, callback: (event: unknown) => void) => { handlers.set(name, callback); return () => handlers.delete(name); } }));
import { completeAgentTurn, initEventInvalidation, invalidateAgentEffects, qk } from "./events";
import type { ConversationView, TurnResult } from "./ipc";

it("conversation completion invalidates task and proposal projections across cycles", async () => {
  const client = new QueryClient();
  const keys = [qk.plannerState(), qk.editorWorkspace("day"), qk.editorWorkspace("week"), qk.editorWorkspaces(["day", "week"]), qk.previewSummary("day"), qk.previewSummary("week"), qk.agentConversation()];
  keys.forEach((key) => client.setQueryData(key, {}));
  const unlisten = await initEventInvalidation(client);
  handlers.get("agent:conversation_updated")!({ payload: { conversation_id: "c", revision: 1 } });
  keys.forEach((key) => expect(client.getQueryState(key)?.isInvalidated).toBe(true));
  unlisten();
});

it("settled failures can refresh previews left by successful tools before the failure", () => {
  const client = new QueryClient();
  client.setQueryData(qk.previewSummary("another-cycle"), {});
  invalidateAgentEffects(client);
  expect(client.getQueryState(qk.previewSummary("another-cycle"))?.isInvalidated).toBe(true);
});

it("a completed turn cancels an older read so its busy flag cannot relock approvals", async () => {
  const client = new QueryClient();
  const before: ConversationView = { id: "global", active_turn_id: "running", revision: 1,
    active_skill: null, last_error: null, messages: [], expires_at: null, context_idle_minutes: 15 };
  client.setQueryData(qk.agentConversation(), before);
  let finishRead!: (value: ConversationView) => void;
  const read = client.fetchQuery({ queryKey: qk.agentConversation(),
    queryFn: () => new Promise<ConversationView>((resolve) => { finishRead = resolve; }) }).catch(() => {});
  const result: TurnResult = { conversation_id: "global", revision: 2, active_skill: null, reply: "Ready",
    messages: [{ id: "reply", sequence_number: 1, message_type: "model_text", turn_id: "running", payload: { kind: "model_text", text: "Ready" } }] };
  completeAgentTurn(client, result);
  finishRead(before);
  await read;
  expect(client.getQueryData<ConversationView>(qk.agentConversation())?.active_turn_id).toBeNull();
  expect(client.getQueryData<ConversationView>(qk.agentConversation())?.messages).toEqual(result.messages);
  expect(client.getQueryData<ConversationView>(qk.agentConversation())?.revision).toBe(2);
});

it("proposal events discover a newly affected cycle without remounting Coach", async () => {
  const client = new QueryClient();
  client.setQueryData(qk.pendingTaskCycles(), []);
  const unlisten = await initEventInvalidation(client);
  handlers.get("proposals:changed")!({ payload: { cycle_id: "past-day" } });
  expect(client.getQueryState(qk.pendingTaskCycles())?.isInvalidated).toBe(true);
  unlisten();
});

it("refreshes linked details when a child task or its plan changes", async () => {
  const client = new QueryClient();
  const key = qk.directLinkedChildren("goal");
  const unlisten = await initEventInvalidation(client);
  for (const eventName of ["tasks:changed", "cycles:changed"]) {
    client.setQueryData(key, []);
    handlers.get(eventName)!({ payload: { cycle_ids: ["week"] } });
    expect(client.getQueryState(key)?.isInvalidated).toBe(true);
  }
  unlisten();
});

import { expect, it, vi } from "vitest";
import { QueryClient } from "@tanstack/react-query";
const handlers = vi.hoisted(() => new Map<string, (event: unknown) => void>());
vi.mock("@tauri-apps/api/event", () => ({ listen: async (name: string, callback: (event: unknown) => void) => { handlers.set(name, callback); return () => handlers.delete(name); } }));
import { initEventInvalidation, invalidateAgentEffects, qk } from "./events";

it("conversation completion invalidates task and proposal projections across cycles", async () => {
  const client = new QueryClient();
  const keys = [qk.plannerState(), qk.editorWorkspace("day"), qk.editorWorkspace("week"), qk.editorWorkspaces(["day", "week"]), qk.previewSummary("day"), qk.previewSummary("week"), qk.agentConversation("day")];
  keys.forEach((key) => client.setQueryData(key, {}));
  const unlisten = await initEventInvalidation(client);
  handlers.get("agent:conversation_updated")!({ payload: { cycle_id: "day", conversation_id: "c", revision: 1 } });
  keys.forEach((key) => expect(client.getQueryState(key)?.isInvalidated).toBe(true));
  unlisten();
});

it("settled failures can refresh previews left by successful tools before the failure", () => {
  const client = new QueryClient();
  client.setQueryData(qk.previewSummary("another-cycle"), {});
  invalidateAgentEffects(client);
  expect(client.getQueryState(qk.previewSummary("another-cycle"))?.isInvalidated).toBe(true);
});

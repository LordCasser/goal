import { beforeEach, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider, useQuery } from "@tanstack/react-query";
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
import { PlanApprovalCard } from "./PlanApprovalCard";
import { commands, getEditorWorkspace, type PreviewSummary, type Task, type TaskSnapshot } from "../../lib/ipc";
import { qk } from "../../lib/events";
import { applyLocale } from "../../lib/i18n";

const task: Task = { id: "t1", cycle_id: "day", later_plan_type: null, parent_id: null, title: "Revised goal", note: "", completed: false, subtasks: [], position: 0, goal_breakdown: null, needs_refinement: null, needs_breakdown: null, root_color_key: null, copied_from_task_id: null, proposal: "upsert", created_at: 1 };
const original: TaskSnapshot = { ...task, original_exists: true, title: "Original goal" };
let pending: PreviewSummary;
let committed: Task[];
beforeEach(() => {
  applyLocale("zh-CN");
  pending = { cycle_id: "day", tasks: [{ ...task }], count: 1, deletion_impacts: {}, originals: { t1: original } };
  committed = [];
  invoke.mockReset().mockImplementation(async (cmd: string, args: { taskId?: string; approve?:boolean }) => {
    if (cmd === "get_pending_task_cycles") return ["day"];
    if (cmd === commands.getPlannerState) return { cycles: [{ id: "day", title: "Tuesday plan", starts_on: "2026-09-15" }] };
    if (cmd === commands.getPreviewSummary) return pending;
    if (cmd === commands.getEditorWorkspace) return { tasks: committed };
    if (cmd === "resolve_coach_task_preview") {
      expect(args.taskId).toBe("t1");
      committed = args.approve ? [{ ...task, proposal: null }] : [{ ...task, title: "Original goal", proposal: null }];
      pending = { cycle_id: "day", count: 0, tasks: [], deletion_impacts: {}, originals: {} };
      return committed[0];
    }
    return null;
  });
});

function Plan() {
  const query = useQuery({ queryKey: qk.editorWorkspace("day"), queryFn: () => getEditorWorkspace("day") });
  return <div aria-label="Main plan">{query.data?.tasks.map((item) => item.title).join(", ")}</div>;
}
function mount(disabled = false) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(<QueryClientProvider client={client}><Plan /><PlanApprovalCard disabled={disabled} /></QueryClientProvider>);
}

it("reviews the exact change and applies it into the main plan without a text reply", async () => {
  mount();
  const apply = await screen.findByRole("button", { name: "应用：Revised goal" });
  expect(screen.getByText("Original goal").tagName).toBe("DEL");
  expect(screen.getByLabelText("Main plan").textContent).toBe("");
  fireEvent.click(apply);
  await waitFor(() => expect(screen.getByLabelText("Main plan").textContent).toBe("Revised goal"));
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.queryByRole("region", { name: "待确认的计划改动" })).toBeNull();
  expect(invoke).not.toHaveBeenCalledWith(commands.keepAllPreviews, expect.anything());
});

it("declining restores the original and does not call the model or keep endpoint", async () => {
  mount();
  fireEvent.click(await screen.findByRole("button", { name: "放弃：Revised goal" }));
  await waitFor(() => expect(screen.getByLabelText("Main plan").textContent).toBe("Original goal"));
  expect(invoke).toHaveBeenCalledWith("resolve_coach_task_preview", { taskId: "t1", approve:false });
  expect(invoke.mock.calls.some(([cmd]) => cmd === commands.keepTaskPreview || cmd === commands.sendAgentMessage)).toBe(false);
});

it("keeps failed approval visible and never shows a success receipt", async () => {
  const route = invoke.getMockImplementation()!;
  invoke.mockImplementation((cmd, args) => cmd === "resolve_coach_task_preview" ? Promise.reject({ code: "db_error", message: "Cannot save" }) : route(cmd, args));
  mount();
  fireEvent.click(await screen.findByRole("button", { name: "应用：Revised goal" }));
  expect(await screen.findByRole("alert")).toBeDefined();
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.getByRole("button", { name: "应用：Revised goal" })).toBeDefined();
  expect(screen.getByLabelText("Main plan").textContent).toBe("");
});

it("blocks approval while the agent is still working and exposes changed breakdown fields", async () => {
  pending.tasks[0]!.goal_breakdown = { output: { value: "A reviewable design" }, scope: { effort: "45 minutes" } };
  mount(true);
  const apply = await screen.findByRole("button", { name: "应用：Revised goal" }) as HTMLButtonElement;
  expect(apply.disabled).toBe(true);
  expect(screen.getByText("A reviewable design")).toBeDefined();
  expect(screen.getByText("45 minutes")).toBeDefined();
});

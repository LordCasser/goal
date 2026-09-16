/**
 * LaterPanel 行为契约（rebuild-baseline 7.9）：快速记录走 add_later_goal、
 * 一次性解释卡的持久化关闭（get_app_flag / set_app_flag）、Escape 关面板，
 * 以及行的完成/连续输入/Promote 路径。invoke 按 src/lib/ipc.test.ts 的方式
 * mock——组件经由 lib/ipc 走真实包装，同时钉住线上的参数键。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import {
  commands,
  LATER_CYCLE_ID,
  type Cycle,
  type EditorWorkspace,
  type PlannerState,
  type TaskNode,
} from "../../lib/ipc";
import { qk } from "../../lib/events";
import { LATER_HINT_KEY, LaterPanel } from "./LaterPanel";

function cycleFixture(id: string, title: string, over: Partial<Cycle> = {}): Cycle {
  return {
    id,
    title,
    type: "month",
    parent_id: null,
    task_id: null,
    position: 0,
    archived: false,
    started: false,
    finished: false,
    started_at: null,
    finished_at: null,
    duration: null,
    focused_time: 0,
    starts_on: null,
    ends_on: null,
    calendar_key: null,
    repeat_id: null,
    created_at: 0,
    ...over,
  };
}

function taskFixture(id: string, title: string, over: Partial<TaskNode> = {}): TaskNode {
  return {
    id,
    cycle_id: LATER_CYCLE_ID,
    parent_id: null,
    title,
    subtasks: [],
    position: 0,
    completed: false,
    goal_breakdown: null,
    needs_refinement: null,
    needs_breakdown: null,
    root_color_key: null,
    copied_from_task_id: null,
    proposal: null,
    created_at: 0,
    children: [],
    subtasks_markdown: "",
    ...over,
    later_plan_type: over.later_plan_type ?? null,
    focused_time: over.focused_time ?? 0,
  };
}

/** 按命令名分发固定返回值；未覆盖的命令一律返回 null。 */
function mockBackend(
  over: {
    hintFlag?: string | null;
    tasks?: TaskNode[];
    plannerCycles?: Cycle[];
    promoteResult?: TaskNode;
    promoteReject?: unknown;
    promotePending?: boolean;
  } = {},
): void {
  const planner: PlannerState = {
    cycles: over.plannerCycles ?? [cycleFixture("lt-1", "Long-term 1")],
    later: cycleFixture(LATER_CYCLE_ID, "Later"),
  };
  invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case commands.getAppFlag:
        return Promise.resolve(over.hintFlag ?? null);
      case commands.getPlannerState:
        return Promise.resolve(planner);
      case commands.getTaskDeletionPreview:
        return Promise.resolve({ task_id: "t1", descendant_tasks: 0, total_focus_blocks: 0, started_focus_count: 0, confirmation_token: "impact-1" });
      case commands.promoteLaterGoal:
        if (over.promotePending) return new Promise<TaskNode>(() => undefined);
        if (over.promoteReject !== undefined) return Promise.reject(over.promoteReject);
        return Promise.resolve(over.promoteResult ?? taskFixture("t1", "Read a book", { cycle_id: "lt-1" }));
      case commands.getEditorWorkspace:
        return Promise.resolve<EditorWorkspace>({
          work_mix: null,
          cycle: cycleFixture(LATER_CYCLE_ID, "Later"),
          tasks: over.tasks ?? [],
        });
      default:
        return Promise.resolve(null);
    }
  });
}

function renderPanel(): { onClose: ReturnType<typeof vi.fn>; client: QueryClient } {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const onClose = vi.fn();
  render(
    <QueryClientProvider client={client}>
      <LaterPanel onClose={onClose} />
    </QueryClientProvider>,
  );
  return { onClose, client };
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("LaterPanel", () => {
  it("moves focus into the quick-add input on mount", () => {
    mockBackend();
    renderPanel();
    expect(screen.getByPlaceholderText("Note it down, schedule later")).toBe(
      document.activeElement,
    );
  });

  it("submits the draft through add_later_goal and clears the input", async () => {
    mockBackend();
    renderPanel();
    const input = screen.getByPlaceholderText(
      "Note it down, schedule later",
    ) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "Read a book" } });
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.addLaterGoal, {
        title: "Read a book",
      }),
    );
    expect(input.value).toBe("");
  });

  it("keeps IME-confirming Enter from submitting", () => {
    mockBackend();
    renderPanel();
    const input = screen.getByPlaceholderText(
      "Note it down, schedule later",
    ) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "读一本书" } });
    fireEvent.keyDown(input, { key: "Enter", isComposing: true });

    expect(invokeMock).not.toHaveBeenCalledWith(
      commands.addLaterGoal,
      expect.anything(),
    );
    expect(input.value).toBe("读一本书");
  });

  it("renders the explainer for first-time visitors and dismisses it durably", async () => {
    mockBackend({ hintFlag: null });
    renderPanel();
    fireEvent.click(await screen.findByRole("button", { name: "Dismiss hint" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.setAppFlag, {
        key: LATER_HINT_KEY,
        value: "dismissed",
      }),
    );
    await waitFor(() =>
      expect(screen.queryByRole("button", { name: "Dismiss hint" })).toBeNull(),
    );
  });

  it("keeps the explainer hidden once the flag says dismissed", async () => {
    mockBackend({ hintFlag: "dismissed" });
    const { client } = renderPanel();
    await waitFor(() =>
      expect(client.getQueryData(["app-flag", LATER_HINT_KEY])).toBe("dismissed"),
    );
    expect(screen.queryByRole("button", { name: "Dismiss hint" })).toBeNull();
    expect(screen.queryByText(/Capture ideas or park plans/i)).toBeNull();
  });

  it("closes the panel on Escape from anywhere inside", () => {
    mockBackend();
    const { onClose } = renderPanel();
    fireEvent.keyDown(screen.getByPlaceholderText("Note it down, schedule later"), {
      key: "Escape",
    });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("toggles completion through patch_task", async () => {
    mockBackend({ tasks: [taskFixture("t1", "Read a book")] });
    renderPanel();
    fireEvent.click(await screen.findByRole("checkbox"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.patchTask, {
        taskId: "t1",
        patch: { completed: true },
      }),
    );
  });

  it("Enter in a row commits the title and opens an empty row below", async () => {
    mockBackend({ tasks: [taskFixture("t1", "Read a book")] });
    renderPanel();
    const title = (await screen.findByLabelText("Task title")) as HTMLInputElement;
    fireEvent.change(title, { target: { value: "Read two books" } });
    fireEvent.keyDown(title, { key: "Enter" });

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.patchTask, {
        taskId: "t1",
        patch: { title: "Read two books" },
      }),
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.addTask, {
        args: {
          cycle_id: LATER_CYCLE_ID,
          title: "",
          parent_id: null,
          position: 1,
        },
      }),
    );
  });

  it("preserves the parent for Enter on a nested Later row", async () => {
    mockBackend({
      tasks: [
        taskFixture("root", "Root", {
          children: [taskFixture("child", "Child", { parent_id: "root", position: 1 })],
        }),
      ],
    });
    renderPanel();
    const child = (await screen.findAllByLabelText("Task title")).find(
      (input) => (input as HTMLInputElement).value === "Child",
    );
    expect(child).toBeTruthy();
    fireEvent.keyDown(child!, { key: "Enter" });

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.addTask, {
        args: {
          cycle_id: LATER_CYCLE_ID,
          title: "",
          parent_id: "root",
          position: 2,
        },
      }),
    );
  });

  it("promotes directly when exactly one long-term cycle exists", async () => {
    mockBackend({
      tasks: [taskFixture("t1", "Read a book")],
      plannerCycles: [cycleFixture("lt-1", "Focus")],
    });
    renderPanel();
    fireEvent.click(
      await screen.findByRole("button", { name: "Promote to a long-term cycle" }),
    );

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.promoteLaterGoal, {
        taskId: "t1",
        targetCycleId: "lt-1",
      }),
    );
  });

  it("shows the retained Later plan type beside each row", async () => {
    mockBackend({
      tasks: [
        taskFixture("month", "Long-term idea"),
        taskFixture("week", "Weekly idea", { later_plan_type: "week", position: 1 }),
        taskFixture("day", "Daily idea", { later_plan_type: "day", position: 2 }),
      ],
    });
    renderPanel();

    expect(await screen.findByText("Long-term goal")).toBeTruthy();
    expect(screen.getByText("Week plan")).toBeTruthy();
    expect(screen.getByText("Day plan")).toBeTruthy();
  });

  it("schedules week and day Later items through the backend default targets", async () => {
    mockBackend({
      plannerCycles: [],
      tasks: [
        taskFixture("week", "Weekly idea", { later_plan_type: "week" }),
        taskFixture("day", "Daily idea", { later_plan_type: "day", position: 1 }),
      ],
    });
    renderPanel();

    fireEvent.click(await screen.findByRole("button", { name: "Schedule for this week" }));
    fireEvent.click(await screen.findByRole("button", { name: "Schedule for today" }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(commands.promoteLaterGoal, {
        taskId: "week",
        targetCycleId: null,
      });
      expect(invokeMock).toHaveBeenCalledWith(commands.promoteLaterGoal, {
        taskId: "day",
        targetCycleId: null,
      });
    });
  });

  it("locks the row while a promote request is pending", async () => {
    mockBackend({
      promotePending: true,
      tasks: [taskFixture("week", "Weekly idea", { later_plan_type: "week" })],
    });
    renderPanel();
    const promote = await screen.findByRole("button", { name: "Schedule for this week" });
    const title = screen.getByDisplayValue("Weekly idea") as HTMLInputElement;
    const checkbox = screen.getByRole("checkbox");
    const deleteButton = screen.getByRole("button", { name: "Delete parked goal" });

    fireEvent.click(promote);
    await waitFor(() => {
      expect(title.readOnly).toBe(true);
      expect(checkbox).toHaveProperty("disabled", true);
      expect(deleteButton).toHaveProperty("disabled", true);
    });
    fireEvent.keyDown(title, { key: "Enter" });
    expect(invokeMock).not.toHaveBeenCalledWith(commands.addTask, expect.anything());
  });

  it("keeps the row and shows a localized error when scheduling fails", async () => {
    mockBackend({
      promoteReject: { code: "db_error", message: "database unavailable" },
      tasks: [taskFixture("week", "Weekly idea", { later_plan_type: "week" })],
    });
    renderPanel();
    fireEvent.click(await screen.findByRole("button", { name: "Schedule for this week" }));

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("Could not access local data");
    expect(alert.parentElement?.classList.contains("group")).toBe(true);
    expect(alert.parentElement?.classList.contains("flex")).toBe(false);
    expect(screen.getByDisplayValue("Weekly idea")).toBeTruthy();
  });

  it("filters archived and finished long-term cycles from the target menu", async () => {
    mockBackend({
      plannerCycles: [
        cycleFixture("active", "Active"),
        cycleFixture("active-2", "Active 2"),
        cycleFixture("archived", "Archived", { archived: true }),
        cycleFixture("finished", "Finished", { finished: true }),
      ],
      tasks: [taskFixture("month", "Long-term idea")],
    });
    renderPanel();
    fireEvent.click(await screen.findByRole("button", { name: "Promote to a long-term cycle" }));

    expect(await screen.findByRole("menuitem", { name: "Active" })).toBeTruthy();
    expect(screen.queryByRole("menuitem", { name: "Archived" })).toBeNull();
    expect(screen.queryByRole("menuitem", { name: "Finished" })).toBeNull();
  });

  it("invalidates Later, the moved workspace, and planner state after scheduling", async () => {
    mockBackend({
      plannerCycles: [cycleFixture("active", "Active")],
      tasks: [taskFixture("month", "Long-term idea")],
      promoteResult: taskFixture("month", "Long-term idea", { cycle_id: "active" }),
    });
    const { client } = renderPanel();
    const invalidate = vi.spyOn(client, "invalidateQueries");
    fireEvent.click(await screen.findByRole("button", { name: "Promote to a long-term cycle" }));

    await waitFor(() => expect(invalidate).toHaveBeenCalledWith({ queryKey: qk.editorWorkspace(LATER_CYCLE_ID) }));
    expect(invalidate).toHaveBeenCalledWith({ queryKey: qk.editorWorkspace("active") });
    expect(invalidate).toHaveBeenCalledWith({ queryKey: qk.plannerState() });
  });

  it("asks for a target cycle when several long-term cycles exist", async () => {
    mockBackend({
      tasks: [taskFixture("t1", "Read a book")],
      plannerCycles: [cycleFixture("lt-1", "Focus"), cycleFixture("lt-2", "Health")],
    });
    renderPanel();
    fireEvent.click(
      await screen.findByRole("button", { name: "Promote to a long-term cycle" }),
    );
    fireEvent.click(await screen.findByRole("menuitem", { name: "Health" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.promoteLaterGoal, {
        taskId: "t1",
        targetCycleId: "lt-2",
      }),
    );
  });

  it("deletes a parked goal through delete_task", async () => {
    mockBackend({ tasks: [taskFixture("t1", "Read a book")] });
    renderPanel();
    fireEvent.click(await screen.findByRole("button", { name: "Delete parked goal" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.deleteTask, { taskId: "t1", confirmationToken: "impact-1" }),
    );
  });
});

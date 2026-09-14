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
import { LATER_HINT_KEY, LaterPanel } from "./LaterPanel";

function cycleFixture(id: string, title: string): Cycle {
  return {
    id,
    title,
    type: "month",
    parent_id: null,
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
  };
}

/** 按命令名分发固定返回值；未覆盖的命令一律返回 null。 */
function mockBackend(
  over: { hintFlag?: string | null; tasks?: TaskNode[]; plannerCycles?: Cycle[] } = {},
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
      case commands.getEditorWorkspace:
        return Promise.resolve<EditorWorkspace>({
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
    expect(screen.queryByText(/Capture a goal/i)).toBeNull();
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
        task_id: "t1",
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
        task_id: "t1",
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
        task_id: "t1",
        target_cycle_id: "lt-1",
      }),
    );
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
        task_id: "t1",
        target_cycle_id: "lt-2",
      }),
    );
  });

  it("deletes a parked goal through delete_task", async () => {
    mockBackend({ tasks: [taskFixture("t1", "Read a book")] });
    renderPanel();
    fireEvent.click(await screen.findByRole("button", { name: "Delete parked goal" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.deleteTask, { task_id: "t1" }),
    );
  });
});

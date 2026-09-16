/**
 * Contract tests for the IPC wrappers: the command names and the argument
 * keys follow Tauri’s camelCase command boundary (nested serde data stays snake_case),
 * and the AppError guard must recognize the serialized `{code, message}` shape.
 * The real invoke is mocked — these tests pin the wire contract, not Tauri.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

import {
  commands,
  createPlanningCycle,
  isAppError,
  moveTask,
  promoteLaterGoal,
  reorderTasks,
  setLocale,
  saveProvider,
  updateCycle,
  updateTask,
} from "./ipc";

beforeEach(() => {
  invokeMock.mockReset();
});

describe("command name registry", () => {
  it("declares the registered Rust command names verbatim", () => {
    expect(commands.createPlanningCycle).toBe("create_planning_cycle");
    expect(commands.moveTask).toBe("move_task");
    expect(commands.reorderTasks).toBe("reorder_tasks");
    expect(commands.getPlannerState).toBe("get_planner_state");
  });
});

describe("argument key contract", () => {
  it("always sends a separate header value map without returning header secrets", async () => {
    invokeMock.mockResolvedValue({
      id: "p1",
      name: "Provider",
      base_url: "https://example.test",
      api_format: "openai_chat_completions",
      connection: { mode: "auto" },
      extra_headers: ["x-tenant"],
      models: [],
      created_at: 1,
      archived: false,
      connection_verified_at: null,
    });
    const provider = await saveProvider({
      id: "p1",
      name: "Provider",
      base_url: "https://example.test",
      api_format: "openai_chat_completions",
      connection: { mode: "auto" },
      extra_headers: ["x-tenant"],
      models: [],
      created_at: 1,
      archived: false,
      connection_verified_at: null,
    });
    expect(invokeMock).toHaveBeenCalledWith(commands.saveProvider, {
      provider: expect.objectContaining({ extra_headers: ["x-tenant"] }),
      apiKey: null,
      headerValues: {},
    });
    expect(provider).not.toHaveProperty("header_values");
    expect(provider).not.toHaveProperty("x-tenant");
  });

  it("sends only the canonical locale to the validated settings command", async () => {
    await setLocale("zh-CN");
    expect(invokeMock).toHaveBeenCalledWith("set_locale", { locale: "zh-CN" });
  });
  it("nests struct parameters under the Rust parameter name `args`", async () => {
    invokeMock.mockResolvedValue({});
    await createPlanningCycle({ cycle_type: "month", duration_months: 3 });
    expect(invokeMock).toHaveBeenCalledWith(commands.createPlanningCycle, {
      args: { cycle_type: "month", duration_months: 3 },
    });
  });

  it("uses camelCase command keys for scalar parameters", async () => {
    invokeMock.mockResolvedValue({});

    await moveTask("t1", "c2", 3);
    expect(invokeMock).toHaveBeenCalledWith(commands.moveTask, {
      taskId: "t1",
      targetCycleId: "c2",
      position: 3,
    });

    await reorderTasks("c1", null, ["a", "b"]);
    expect(invokeMock).toHaveBeenLastCalledWith(commands.reorderTasks, {
      cycleId: "c1",
      parentId: null,
      orderedIds: ["a", "b"],
    });

    await updateCycle("s1", "Deep work", null);
    expect(invokeMock).toHaveBeenLastCalledWith(commands.updateCycle, {
      cycleId: "s1",
      title: "Deep work",
      durationMs: null,
    });
  });

  it("uses a null target to let the backend choose the current week or day", async () => {
    invokeMock.mockResolvedValue({});
    await promoteLaterGoal("later-task", null);
    expect(invokeMock).toHaveBeenCalledWith(commands.promoteLaterGoal, {
      taskId: "later-task",
      targetCycleId: null,
    });
  });

  it("passes the patch struct under its own parameter name", async () => {
    invokeMock.mockResolvedValue({});
    await updateTask("t1", "Ship it", { completed: true });
    expect(invokeMock).toHaveBeenCalledWith(commands.updateTask, {
      taskId: "t1",
      title: "Ship it",
      patch: { completed: true },
    });
  });
});

describe("AppError guard", () => {
  it("rejects with the serialized error so isAppError can narrow it", async () => {
    const rejection = { code: "past_cycle", message: "Past cycles can't be deleted." };
    invokeMock.mockRejectedValue(rejection);

    const caught = await updateCycle("s1", "x", null).catch((e: unknown) => e);
    expect(caught).toEqual(rejection);
    expect(isAppError(caught)).toBe(true);
    if (isAppError(caught)) {
      expect(caught.code).toBe("past_cycle");
      expect(caught.message).toContain("Past cycles");
    }
  });

  it("rejects shapes that are not the serialized AppError", () => {
    expect(isAppError(new Error("plain"))).toBe(false);
    expect(isAppError(null)).toBe(false);
    expect(isAppError("code")).toBe(false);
    expect(isAppError({ code: 404, message: "numeric" })).toBe(false);
    expect(isAppError({ code: "not_found", message: "cycle not found: x" })).toBe(true);
  });
});

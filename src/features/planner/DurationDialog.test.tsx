/**
 * 时长选择弹窗测试（任务 7.9）：默认 3 月、切换 6 月时间线文案即时更新、
 * 确认只调用一次 createPlanningCycle（mock ../../lib/ipc，参照
 * src/lib/ipc.test.ts 的 hoisted mock 做法）、取消无副作用。
 * 日期用 vi.setSystemTime 固定在 2026-09-15（本地正午，任何时区都同日），
 * 期望值来自 deriveTimeline：3 月 → 复盘 2026-12-08；6 月 → 复盘 2027-03-02。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";

const { createPlanningCycleMock } = vi.hoisted(() => ({
  createPlanningCycleMock: vi.fn(),
}));

vi.mock("../../lib/ipc", () => ({
  createPlanningCycle: createPlanningCycleMock,
  // actions.ts 的 errorMessage 会用到 isAppError；给出与实现一致的形状判断。
  isAppError: (e: unknown) =>
    typeof e === "object" && e !== null && "code" in e && "message" in e,
}));

import { DurationDialog } from "./DurationDialog";

describe("DurationDialog", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-09-15T12:00:00"));
    createPlanningCycleMock.mockReset();
    createPlanningCycleMock.mockResolvedValue({ id: "m1" });
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("defaults to 3 months and shows the derived timeline", () => {
    render(<DurationDialog open onClose={() => {}} />);
    expect(screen.getByText("12 weeks")).toBeTruthy();
    // 设定日 = 今天；复盘日 = 今天 + 84 天。
    expect(screen.getByText("2026-09-15")).toBeTruthy();
    expect(screen.getByText("2026-12-08")).toBeTruthy();
  });

  it("updates the timeline copy immediately when switching to 6 months", () => {
    render(<DurationDialog open onClose={() => {}} />);
    fireEvent.click(screen.getByRole("radio", { name: /6 months/ }));
    expect(screen.getByText("24 weeks")).toBeTruthy();
    expect(screen.getByText("2027-03-02")).toBeTruthy();
    expect(screen.queryByText("12 weeks")).toBeNull();
    // 切换只是前端推导，不触发创建请求。
    expect(createPlanningCycleMock).not.toHaveBeenCalled();
  });

  it("creates the long-term cycle exactly once on confirm", async () => {
    render(<DurationDialog open onClose={() => {}} />);
    fireEvent.click(screen.getByRole("radio", { name: /6 months/ }));
    // 确认后的 setCreating/onClose 是异步状态更新，用 async act 收敛微任务。
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Create cycle" }));
    });
    expect(createPlanningCycleMock).toHaveBeenCalledTimes(1);
    expect(createPlanningCycleMock).toHaveBeenCalledWith({
      cycle_type: "month",
      duration_months: 6,
      parent_id: null,
    });
  });

  it("cancel performs no request", () => {
    render(<DurationDialog open onClose={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(createPlanningCycleMock).not.toHaveBeenCalled();
  });
});

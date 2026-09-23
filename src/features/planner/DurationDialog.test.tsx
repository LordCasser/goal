/**
 * 时长选择弹窗测试（任务 7.9）：默认 3 月、切换 6 月时间线文案即时更新、
 * 确认只调用一次 createPlanningCycle（mock ../../lib/ipc，参照
 * src/lib/ipc.test.ts 的 hoisted mock 做法）、取消无副作用。
 * 日期用 vi.setSystemTime 固定在 2026-09-15（本地正午，任何时区都同日），
 * 期望值来自 deriveTimeline：3 月 → 复盘 2026-12-08；6 月 → 复盘 2027-03-02。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { applyLocale } from "../../lib/i18n";

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
    applyLocale("en");
    createPlanningCycleMock.mockReset();
    createPlanningCycleMock.mockResolvedValue({ id: "m1" });
  });
  afterEach(() => {
    vi.useRealTimers();
    applyLocale("en");
  });

  it("defaults to 3 months and shows the derived timeline", () => {
    render(<DurationDialog open onClose={() => {}} />);
    expect(screen.getByText("12 weeks")).toBeTruthy();
    // 设定日 = 今天；复盘日 = 今天 + 84 天。
    expect(screen.getByText("Set goals").parentElement?.textContent.includes("Sep 15")).toBeTruthy();
    expect(screen.getByText("Review").parentElement?.textContent.includes("Dec 8")).toBeTruthy();
  });

  it("updates the timeline copy immediately when switching to 6 months", () => {
    render(<DurationDialog open onClose={() => {}} />);
    fireEvent.click(screen.getByRole("radio", { name: /6 months/ }));
    expect(screen.getByText("24 weeks")).toBeTruthy();
    expect(screen.getByText("Review").parentElement?.textContent.includes("Mar 2")).toBeTruthy();
    expect(screen.getByRole("radio", { name: /3 months/ }).textContent).toContain("12 weeks");
    // 切换只是前端推导，不触发创建请求。
    expect(createPlanningCycleMock).not.toHaveBeenCalled();
  });

  it("uses one tab stop and arrow keys within each radio group", () => {
    render(<DurationDialog open onClose={() => {}} />);
    const threeMonths = screen.getByRole("radio", { name: /3 months/ }) as HTMLButtonElement;
    const sixMonths = screen.getByRole("radio", { name: /6 months/ }) as HTMLButtonElement;
    expect(threeMonths.tabIndex).toBe(0);
    expect(sixMonths.tabIndex).toBe(-1);

    fireEvent.keyDown(threeMonths, { key: "ArrowDown" });
    expect(sixMonths.getAttribute("aria-checked")).toBe("true");
    expect(sixMonths.tabIndex).toBe(0);
    expect(threeMonths.tabIndex).toBe(-1);
    expect(document.activeElement).toBe(sixMonths);

    fireEvent.click(screen.getByRole("radio", { name: /^Custom/ }));
    const none = screen.getByRole("radio", { name: "No reminder" }) as HTMLButtonElement;
    const repeat = screen.getByRole("radio", { name: "Repeat" }) as HTMLButtonElement;
    const once = screen.getByRole("radio", { name: "Once" }) as HTMLButtonElement;
    expect(none.tabIndex).toBe(0);
    expect(repeat.tabIndex).toBe(-1);
    expect(once.tabIndex).toBe(-1);
    fireEvent.keyDown(none, { key: "ArrowRight" });
    expect(repeat.getAttribute("aria-checked")).toBe("true");
    expect(repeat.tabIndex).toBe(0);
    expect(document.activeElement).toBe(repeat);
  });

  it("localizes every duration preview and follows locale changes while open", () => {
    render(<DurationDialog open onClose={() => {}} />);
    expect(screen.getByRole("radio", { name: /1 month/ }).textContent).toContain("4 weeks · ends Oct 13");
    expect(screen.getByRole("radio", { name: /6 months/ }).textContent).toContain("24 weeks · ends Mar 2");

    act(() => applyLocale("zh-CN"));

    expect(screen.getByRole("radio", { name: /1 个月/ }).textContent).toContain("4 周 · 结束于 10月13日");
    expect(screen.getByRole("radio", { name: /6 个月/ }).textContent).toContain("24 周 · 结束于 3月2日");
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
      progress_check: null,
    });
  });

  it("configures a custom date range and submits a repeating check", async () => {
    render(<DurationDialog open onClose={() => {}} />);
    fireEvent.click(screen.getByRole("radio", { name: /^Custom/ }));
    fireEvent.click(screen.getByRole("radio", { name: "Repeat" }));

    expect(screen.getByLabelText("Start date")).toBeTruthy();
    expect(screen.getByLabelText("End date")).toBeTruthy();
    expect(screen.getByLabelText("Check interval")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Start date"), { target: { value: "2026-09-20" } });
    fireEvent.change(screen.getByLabelText("End date"), { target: { value: "2026-12-20" } });

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Create cycle" }));
    });

    expect(createPlanningCycleMock).toHaveBeenCalledWith({
      cycle_type: "month",
      parent_id: null,
      starts_on: "2026-09-20",
      ends_on: "2026-12-20",
      progress_check: { kind: "repeat", every_days: 14 },
    });
  });

  it("uses a bounded app calendar for the end and once-check dates", () => {
    render(<DurationDialog open onClose={() => {}} />);
    fireEvent.click(screen.getByRole("radio", { name: /^Custom/ }));
    fireEvent.click(screen.getByRole("button", { name: "End date: Open calendar" }));
    for (let index = 0; index < 3; index++) fireEvent.click(screen.getByRole("button", { name: "Previous month" }));
    expect(document.querySelector<HTMLButtonElement>('[data-picker-date="2026-09-14"]')?.disabled).toBe(true);
    fireEvent.click(document.querySelector('[data-picker-date="2026-09-30"]')!);
    expect((screen.getByRole("textbox", { name: "End date" }) as HTMLInputElement).value).toBe("2026-09-30");
    fireEvent.click(screen.getByRole("radio", { name: "Once" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Check date" }), { target: { value: "2026-09-20" } });
    fireEvent.click(screen.getByRole("button", { name: "Check date: Open calendar" }));
    expect(document.querySelector<HTMLButtonElement>('[data-picker-date="2026-09-30"]')?.disabled).toBe(true);
    fireEvent.click(document.querySelector('[data-picker-date="2026-09-21"]')!);
    expect((screen.getByRole("textbox", { name: "Check date" }) as HTMLInputElement).value).toBe("2026-09-21");
  });

  it("creates an open cycle without a check", async () => {
    render(<DurationDialog open onClose={() => {}} />);
    fireEvent.click(screen.getByRole("radio", { name: "No fixed end" }));
    expect(screen.queryByLabelText("End date")).toBeNull();
    expect(screen.getByRole("radio", { name: "No reminder" }).getAttribute("aria-checked")).toBe("true");
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Create cycle" })));
    expect(createPlanningCycleMock).toHaveBeenCalledWith({
      cycle_type: "month", parent_id: null, starts_on: "2026-09-15", ends_on: null, progress_check: null,
    });
  });

  it("creates an open cycle with a bounded repeat preview", async () => {
    render(<DurationDialog open onClose={() => {}} />);
    fireEvent.click(screen.getByRole("radio", { name: "No fixed end" }));
    fireEvent.click(screen.getByRole("radio", { name: "Repeat" }));
    expect(screen.queryByText(/progress checks/)).toBeNull();
    expect(screen.getByText("Sep 29")).toBeTruthy();
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Create cycle" })));
    expect(createPlanningCycleMock).toHaveBeenCalledWith({
      cycle_type: "month", parent_id: null, starts_on: "2026-09-15", ends_on: null,
      progress_check: { kind: "repeat", every_days: 14 },
    });
  });

  it("creates a bounded custom cycle without a check", async () => {
    render(<DurationDialog open onClose={() => {}} />);
    fireEvent.click(screen.getByRole("radio", { name: /^Custom/ }));
    await act(async () => fireEvent.click(screen.getByRole("button", { name: "Create cycle" })));
    expect(createPlanningCycleMock).toHaveBeenCalledWith({
      cycle_type: "month", parent_id: null, starts_on: "2026-09-15", ends_on: "2026-12-08", progress_check: null,
    });
  });

  it("keeps custom drafts when switching between repeat and once checks", () => {
    render(<DurationDialog open onClose={() => {}} />);
    fireEvent.click(screen.getByRole("radio", { name: /^Custom/ }));
    fireEvent.click(screen.getByRole("radio", { name: "Repeat" }));
    fireEvent.change(screen.getByLabelText("Check interval"), { target: { value: "3" } });

    fireEvent.click(screen.getByRole("radio", { name: "Once" }));
    fireEvent.change(screen.getByLabelText("Check date"), { target: { value: "2026-10-20" } });
    expect(screen.queryByLabelText("Check interval")).toBeNull();

    fireEvent.click(screen.getByRole("radio", { name: "Repeat" }));
    expect((screen.getByLabelText("Check interval") as HTMLInputElement).value).toBe("3");
    expect(screen.queryByLabelText("Check date")).toBeNull();
  });

  it("rejects a once check on the end date and disables creation", () => {
    render(<DurationDialog open onClose={() => {}} />);
    fireEvent.click(screen.getByRole("radio", { name: /^Custom/ }));
    fireEvent.click(screen.getByRole("radio", { name: "Once" }));
    fireEvent.change(screen.getByLabelText("Check date"), { target: { value: "2026-12-08" } });

    expect(screen.getByRole("alert").textContent).toContain("within the cycle");
    expect(screen.getByLabelText("Check date").getAttribute("aria-invalid")).toBe("true");
    expect(screen.getByLabelText("Check date").getAttribute("aria-describedby")).toBe("duration-custom-error");
    expect((screen.getByRole("button", { name: "Create cycle" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("ignores rapid repeat confirmation while a custom cycle is creating", async () => {
    let resolveCreate: ((cycle: { id: string }) => void) | undefined;
    createPlanningCycleMock.mockImplementation(() => new Promise((resolve) => {
      resolveCreate = resolve;
    }));
    render(<DurationDialog open onClose={() => {}} />);
    fireEvent.click(screen.getByRole("radio", { name: /^Custom/ }));

    fireEvent.click(screen.getByRole("button", { name: "Create cycle" }));
    fireEvent.click(screen.getByRole("button", { name: "Create cycle" }));
    expect(createPlanningCycleMock).toHaveBeenCalledTimes(1);

    await act(async () => {
      resolveCreate?.({ id: "m1" });
    });
  });

  it("cancel performs no request", () => {
    render(<DurationDialog open onClose={() => {}} />);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(createPlanningCycleMock).not.toHaveBeenCalled();
  });
});

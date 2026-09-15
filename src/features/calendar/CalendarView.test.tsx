/**
 * 日历视图组件测试（tasks.md §5.8）：
 * - 月/周网格渲染（共用 DayCell，摘要 = 专注块数 + 完成进度）
 * - 空格子一键创建调用 ensureDay 并带上日期
 * - 预算三种状态：未设置 → 什么都不渲染（单点数据不渲染误导性图表）、
 *   未超 → 进度 + 剩余、超出 → 危险提示
 * - 拖拽：空目标走乐观移动（strategy=null），占用目标弹策略弹窗（merge）
 *
 * mock 方式参照 DurationDialog.test.tsx 的 hoisted mock；日期不使用假计时
 * 器，而是把 ../planner/dates 的 todayISO 固定为 2026-09-16（其余工具保持
 * 真实现），避免 react-query 与 vi.useFakeTimers 的计时冲突。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, fireEvent, cleanup } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

const {
  getCalendarRangeMock,
  moveDayCycleMock,
  setSessionScheduleMock,
  getScheduleOverlapsMock,
  getTimeBudgetMock,
  ensureDayMock,
} = vi.hoisted(() => ({
  getCalendarRangeMock: vi.fn(),
  moveDayCycleMock: vi.fn(),
  setSessionScheduleMock: vi.fn(),
  getScheduleOverlapsMock: vi.fn(),
  getTimeBudgetMock: vi.fn(),
  ensureDayMock: vi.fn(),
}));

vi.mock("./api", () => ({
  getCalendarRange: getCalendarRangeMock,
  moveDayCycle: moveDayCycleMock,
  setSessionSchedule: setSessionScheduleMock,
  getScheduleOverlaps: getScheduleOverlapsMock,
  getTimeBudget: getTimeBudgetMock,
}));

vi.mock("../../lib/ipc", () => ({
  ensureDay: ensureDayMock,
  // actions.ts 的 errorMessage 依赖 isAppError；给与实现一致的形状判断。
  isAppError: (e: unknown) =>
    typeof e === "object" && e !== null && "code" in e && "message" in e,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));

vi.mock("../planner/dates", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../planner/dates")>();
  return { ...actual, todayISO: () => "2026-09-16" };
});

import type { Cycle } from "../../lib/types";
import { addDaysISO } from "../planner/dates";
import { PREFERRED_VIEW_KEY } from "./calendar-model";
import { BudgetBar } from "./DayTimeline";
import { CalendarView } from "./CalendarView";
import type { CalendarDay, CalendarRange, CalendarSession } from "./api";

function makeCycle(overrides: Partial<Cycle> & Pick<Cycle, "id">): Cycle {
  return {
    title: "Day",
    type: "day",
    parent_id: "week-1",
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
    ...overrides,
  };
}

function sessionOf(
  id: string,
  title: string,
  opts: { finished?: boolean; duration?: number | null; schedule?: CalendarSession["schedule"] } = {},
): CalendarSession {
  return {
    session: makeCycle({
      id,
      title,
      type: "session",
      parent_id: "day-2026-09-16",
      finished: opts.finished ?? false,
      duration: opts.duration ?? null,
    }),
    schedule: opts.schedule ?? null,
  };
}

function cell(date: string, day: CalendarDay["day_cycle"], sessions: CalendarDay["sessions"] = []): CalendarDay {
  return { date, in_range: true, day_cycle: day, sessions };
}

function dayOn(date: string): CalendarDay["day_cycle"] {
  return makeCycle({
    id: `day-${date}`,
    title: date,
    type: "day",
    parent_id: "week-1",
    starts_on: date,
    ends_on: addDaysISO(date, 1),
    calendar_key: `day:${date}`,
  });
}

const T9 = new Date(2026, 8, 16, 9, 0).getTime(); // 当地 09:00（与 DayTimeline 的本地零点同口径）

const RANGE: CalendarRange = {
  start: "2026-09-01",
  end: "2026-09-30",
  grid_start: "2026-08-31",
  grid_end: "2026-10-04",
  week_start_day: 1,
  days: [
    cell("2026-09-15", null),
    cell("2026-09-16", dayOn("2026-09-16"), [
      sessionOf("s-done", "Write spec", { finished: true }),
      sessionOf("s-run", "Review", {
        duration: 30 * 60_000,
        schedule: {
          session_id: "s-run",
          day_cycle_id: "day-2026-09-16",
          starts_at: T9,
          ends_at: T9 + 30 * 60_000,
          duration_ms: 30 * 60_000,
          truncated: false,
        },
      }),
      sessionOf("s-stage", "Unplanned", { duration: 45 * 60_000 }),
    ]),
    cell("2026-09-17", dayOn("2026-09-17"), []),
    cell("2026-09-18", null),
  ],
};

function renderView() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <CalendarView />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  localStorage.removeItem(PREFERRED_VIEW_KEY);
  getCalendarRangeMock.mockReset().mockResolvedValue(RANGE);
  moveDayCycleMock
    .mockReset()
    .mockResolvedValue({ strategy: "move", source_day_id: "day-2026-09-16", target_day_id: "day-2026-09-18", deleted_day_ids: ["day-2026-09-16"] });
  setSessionScheduleMock.mockReset().mockResolvedValue({});
  getScheduleOverlapsMock.mockReset().mockResolvedValue([]);
  getTimeBudgetMock
    .mockReset()
    .mockResolvedValue({ date: "2026-09-16", capacity_minutes: null, scheduled_minutes: 75 });
  ensureDayMock.mockReset().mockResolvedValue({ id: "day-created" });
});

afterEach(cleanup);

describe("CalendarView grids", () => {
  it("renders the month grid with block count and completion progress", async () => {
    const { container } = renderView();
    // 月份视图请求的是自然月区间（后端负责补齐整周）。
    await waitFor(() =>
      expect(getCalendarRangeMock).toHaveBeenCalledWith("2026-09-01", "2026-09-30"),
    );
    // 共用单元格渲染：摘要 = 专注块数 + 完成进度（spec：格子摘要）。
    expect(await screen.findByText("3 blocks")).toBeTruthy();
    expect(screen.getByText("1/3 done")).toBeTruthy();
    // 每个返回日期都有格子，没有空洞。
    expect(container.querySelector('[data-day-cell="2026-09-16"]')).toBeTruthy();
    expect(container.querySelector('[data-day-cell="2026-09-18"]')).toBeTruthy();
  });

  it("renders the week grid through the same cell renderer", async () => {
    localStorage.setItem(PREFERRED_VIEW_KEY, "week");
    renderView();
    await waitFor(() =>
      expect(getCalendarRangeMock).toHaveBeenCalledWith("2026-09-14", "2026-09-20"),
    );
    // 同一 DayCell 摘要出现在周密度里（§5.1 共用单元格渲染）。
    expect(await screen.findByText("3 blocks")).toBeTruthy();
    expect(screen.getByText("1/3 done")).toBeTruthy();
  });

  it("creates a day plan from an empty cell with its date", async () => {
    renderView();
    const button = await screen.findByRole("button", { name: "Create day plan for 2026-09-18" });
    fireEvent.click(button);
    await waitFor(() => expect(ensureDayMock).toHaveBeenCalledWith("2026-09-18"));
  });
});

describe("CalendarView drag", () => {
  const transfer = () => ({
    dataTransfer: {
      setData: vi.fn(),
      getData: vi.fn(() => ""),
      effectAllowed: "",
      dropEffect: "",
    },
  });

  it("drops onto an empty date with the optimistic move (no strategy dialog)", async () => {
    const { container } = renderView();
    const chip = await screen.findByTitle("Move 2026-09-16");
    fireEvent.dragStart(chip, transfer());
    const target = container.querySelector('[data-day-cell="2026-09-18"]');
    expect(target).toBeTruthy();
    fireEvent.drop(target!, transfer());
    await waitFor(() =>
      expect(moveDayCycleMock).toHaveBeenCalledWith("day-2026-09-16", "2026-09-18", null),
    );
    expect(screen.queryByText("This date already has a plan")).toBeNull();
  });

  it("asks for a strategy when the target is occupied and sends merge", async () => {
    const { container } = renderView();
    const chip = await screen.findByTitle("Move 2026-09-16");
    fireEvent.dragStart(chip, transfer());
    fireEvent.drop(container.querySelector('[data-day-cell="2026-09-17"]')!, transfer());
    // 冲突时弹出策略选择（spec：MUST NOT 静默覆盖或丢弃）。
    expect(await screen.findByText("This date already has a plan")).toBeTruthy();
    expect(moveDayCycleMock).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Merge into 2026-09-17" }));
    await waitFor(() =>
      expect(moveDayCycleMock).toHaveBeenCalledWith("day-2026-09-16", "2026-09-17", "merge"),
    );
  });
});

describe("CalendarView budget presentation", () => {
  it("renders no chart at all while capacity is unset (even with scheduled minutes)", async () => {
    renderView();
    fireEvent.click(await screen.findByRole("button", { name: "Open 2026-09-16" }));
    // 时间轴出现：已排块在轴上，未排块在待排区。
    expect(await screen.findByTestId("staging-area")).toBeTruthy();
    expect(screen.getByTestId("timeline")).toBeTruthy();
    expect(getTimeBudgetMock).toHaveBeenCalledWith("2026-09-16");
    // 未设置预算：单点数据不渲染误导性图表——没有进度条、没有空预算条。
    expect(screen.queryByRole("progressbar")).toBeNull();
    expect(screen.queryByTestId("budget-bar")).toBeNull();
    expect(screen.queryByText(/left/)).toBeNull();
    expect(screen.queryByText(/Over by/)).toBeNull();
  });
});

describe("BudgetBar states", () => {
  const renderBar = (capacity_minutes: number | null, scheduled_minutes: number) =>
    render(
      <BudgetBar budget={{ date: "2026-09-16", capacity_minutes, scheduled_minutes }} />,
    );

  it("under budget: progress plus remaining time", () => {
    renderBar(120, 75);
    expect(screen.getByText("45m left")).toBeTruthy();
    const bar = screen.getByRole("progressbar");
    expect(bar.getAttribute("aria-valuenow")).toBe("75");
    expect(bar.getAttribute("aria-valuemax")).toBe("120");
  });

  it("over budget: explicit excess warning in danger tone", () => {
    renderBar(60, 135);
    expect(screen.getByText("Over by 1h 15m")).toBeTruthy();
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("135");
  });

  it("unset budget: nothing rendered — no empty bar", () => {
    const { container } = renderBar(null, 135);
    expect(container.childElementCount).toBe(0);
  });
});

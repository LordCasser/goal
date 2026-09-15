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
import { createEvent } from "@testing-library/dom";
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
  LATER_CYCLE_ID: "later",
  getPlannerState: async () => ({ cycles: [] }),
  getEditorWorkspacesByCycleIds: async () => ({}),
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
import { DAY_DRAG_TYPE, SESSION_DRAG_TYPE } from "./calendar-dnd";
vi.mock("./CalendarPlan", () => ({ CalendarPlan: () => <div>Daily plan editor</div> }));
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
  opts: { started?: boolean; finished?: boolean; duration?: number | null; schedule?: CalendarSession["schedule"] } = {},
): CalendarSession {
  return {
    session: makeCycle({
      id,
      title,
      type: "session",
      parent_id: "day-2026-09-16",
      started: opts.started ?? false,
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
const T10 = new Date(2026, 8, 16, 10, 0).getTime();

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
  const ui = (active: boolean) => (
    <QueryClientProvider client={client}>
      <CalendarView active={active} />
    </QueryClientProvider>
  );
  const result = render(ui(true));
  return { ...result, client, setActive: (active: boolean) => result.rerender(ui(active)) };
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

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe("CalendarView grids", () => {
  it("renders the month grid with block count and completion progress", async () => {
    const { container } = renderView();
    // 月份视图请求的是自然月区间（后端负责补齐整周）。
    await waitFor(() =>
      expect(getCalendarRangeMock).toHaveBeenCalledWith("2026-09-01", "2026-09-30"),
    );
    // 共用单元格渲染：摘要 = 专注块数 + 完成进度（spec：格子摘要）。
    expect(await screen.findByText("3 blocks")).toBeTruthy();
    expect(screen.getByText("1/3 blocks done")).toBeTruthy();
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
    expect(await screen.findByRole("region", { name: "Focus blocks on 2026-09-16" })).toBeTruthy();
    expect(screen.getByText("1/3 done")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Open focus block Review" }));
    expect(screen.getByRole("tab", { name: "schedule" }).getAttribute("aria-selected")).toBe("true");
    expect(document.activeElement?.getAttribute("data-focus-block")).toBe("s-run");
  });

  it("centers the focused date on entering Week, preserves manual scrolling, and recenters on Today", async () => {
    const { client, setActive } = renderView();
    await screen.findByRole("button", { name: "Open 2026-09-16" });
    const viewport = screen.getByRole("region", { name: "Calendar dates" });
    Object.defineProperty(viewport, "clientWidth", { value: 600 });
    const scrollTo = vi.fn((options?: ScrollToOptions | number) => { viewport.scrollLeft = typeof options === "number" ? options : options?.left ?? 0; });
    viewport.scrollTo = scrollTo;
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      return { left: this.dataset.dayCell ? 620 - viewport.scrollLeft : 20, width: this.dataset.dayCell ? 280 : 600 } as DOMRect;
    });

    fireEvent.click(screen.getByRole("tab", { name: "week" }));
    await waitFor(() => expect(scrollTo).toHaveBeenCalledWith({ left: 440, behavior: "instant" }));
    // Data refreshes and top-level view switches cannot steal the user's position.
    viewport.scrollLeft = 900;
    await client.invalidateQueries({ queryKey: ["calendar-range"] });
    setActive(false); setActive(true);
    expect(viewport.scrollLeft).toBe(900);
    expect(scrollTo).toHaveBeenCalledTimes(1);

    fireEvent.click(screen.getByRole("button", { name: "Jump to today" }));
    await waitFor(() => expect(scrollTo).toHaveBeenCalledTimes(2));
    expect(scrollTo).toHaveBeenLastCalledWith({ left: 440, behavior: "smooth" });
  });

  it("waits for initial week data before centering and respects reduced motion on Today", async () => {
    localStorage.setItem(PREFERRED_VIEW_KEY, "week");
    let resolveRange!: (range: CalendarRange) => void;
    getCalendarRangeMock.mockReturnValueOnce(new Promise<CalendarRange>((resolve) => { resolveRange = resolve; }));
    vi.stubGlobal("matchMedia", vi.fn(() => ({ matches: true })));
    try {
      renderView();
      const viewport = screen.getByRole("region", { name: "Calendar dates" });
      Object.defineProperty(viewport, "clientWidth", { value: 600 });
      const scrollTo = vi.fn();
      viewport.scrollTo = scrollTo;
      vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({ left: 0, width: 600 } as DOMRect);
      expect(scrollTo).not.toHaveBeenCalled();
      resolveRange(RANGE);
      await waitFor(() => expect(scrollTo).toHaveBeenCalledTimes(1));
      fireEvent.click(screen.getByRole("button", { name: "Jump to today" }));
      await waitFor(() => expect(scrollTo).toHaveBeenCalledTimes(2));
      expect(scrollTo).toHaveBeenLastCalledWith({ left: 0, behavior: "instant" });
    } finally {
      vi.unstubAllGlobals();
    }
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
    dataTransfer: (() => {
      const values = new Map<string, string>();
      return {
        setData: vi.fn((type: string, value: string) => values.set(type, value)),
        getData: vi.fn((type: string) => values.get(type) ?? ""),
        effectAllowed: "",
        dropEffect: "",
      };
    })(),
  });

  it("uses an opaque calendar payload and rejects unrelated or cancelled drops", async () => {
    const { container } = renderView();
    const chip = await screen.findByTitle("Move 2026-09-16");
    const target = container.querySelector('[data-day-cell="2026-09-18"]')!;
    const payload = transfer();
    fireEvent.dragStart(chip, payload);
    expect(payload.dataTransfer.setData).toHaveBeenCalledWith(DAY_DRAG_TYPE, expect.any(String));
    expect(payload.dataTransfer.setData).not.toHaveBeenCalledWith("text/plain", expect.anything());

    const unrelated = transfer();
    unrelated.dataTransfer.setData("text/plain", "day-2026-09-16");
    fireEvent.drop(target, unrelated);
    expect(moveDayCycleMock).not.toHaveBeenCalled();

    fireEvent.dragStart(chip, payload);
    fireEvent.dragEnd(chip);
    fireEvent.drop(target, payload);
    fireEvent.dragStart(chip, payload);
    fireEvent.keyDown(container.querySelector("[data-calendar-view]")!, { key: "Escape" });
    fireEvent.drop(target, payload);
    expect(moveDayCycleMock).not.toHaveBeenCalled();
  });

  it("schedules a staged focus block only from its own payload", async () => {
    renderView();
    fireEvent.click(await screen.findByRole("button", { name: "Open 2026-09-16" }));
    fireEvent.click(screen.getByRole("tab", { name: "schedule" }));
    const staged = screen.getByText("Unplanned").closest("[data-focus-block]")!;
    const timeline = screen.getByTestId("timeline");
    const payload = transfer();
    Object.defineProperty(timeline, "clientTop", { configurable: true, value: 0 });
    Object.defineProperty(timeline, "scrollTop", { configurable: true, value: 9 * 48 });
    vi.spyOn(timeline, "getBoundingClientRect").mockReturnValue({ top: 100 } as DOMRect);
    const dropAtViewportTop = (dataTransfer: ReturnType<typeof transfer>["dataTransfer"]) => {
      const event = createEvent.drop(timeline, { dataTransfer });
      Object.defineProperty(event, "clientY", { configurable: true, value: 100 });
      fireEvent(timeline, event);
    };
    fireEvent.dragStart(staged, payload);
    expect(payload.dataTransfer.setData).toHaveBeenCalledWith(SESSION_DRAG_TYPE, expect.any(String));
    expect(payload.dataTransfer.setData).not.toHaveBeenCalledWith("text/plain", expect.anything());
    dropAtViewportTop(payload.dataTransfer);
    await waitFor(() => expect(setSessionScheduleMock).toHaveBeenCalledWith("s-stage", T9, 45 * 60_000));

    setSessionScheduleMock.mockClear();
    fireEvent.dragStart(staged, payload);
    fireEvent.dragEnd(staged);
    dropAtViewportTop(transfer().dataTransfer);
    fireEvent.dragStart(staged, payload);
    fireEvent.keyDown(timeline, { key: "Escape" });
    dropAtViewportTop(payload.dataTransfer);
    expect(setSessionScheduleMock).not.toHaveBeenCalled();
  });

  it("moves a scheduled focus block with its grab offset and shows the snapped ghost", async () => {
    renderView();
    fireEvent.click(await screen.findByRole("button", { name: "Open 2026-09-16" }));
    fireEvent.click(screen.getByRole("tab", { name: "schedule" }));
    const timeline = screen.getByTestId("timeline");
    const block = timeline.querySelector('[data-scheduled-block="s-run"]')!;
    Object.defineProperty(timeline, "clientTop", { configurable: true, value: 0 });
    Object.defineProperty(timeline, "scrollTop", { configurable: true, value: 0 });
    vi.spyOn(timeline, "getBoundingClientRect").mockReturnValue({ top: 100 } as DOMRect);
    vi.spyOn(block, "getBoundingClientRect").mockReturnValue({ top: 100 } as DOMRect);
    const payload = transfer();
    const start = createEvent.dragStart(block, { dataTransfer: payload.dataTransfer });
    Object.defineProperty(start, "clientY", { configurable: true, value: 110 });
    fireEvent(block, start);
    const over = createEvent.dragOver(timeline, { dataTransfer: payload.dataTransfer });
    Object.defineProperty(over, "clientY", { configurable: true, value: 590 });
    fireEvent(timeline, over);
    expect((await screen.findByTestId("drag-ghost")).textContent).toContain("10:00");
    const drop = createEvent.drop(timeline, { dataTransfer: payload.dataTransfer });
    Object.defineProperty(drop, "clientY", { configurable: true, value: 590 });
    fireEvent(timeline, drop);
    await waitFor(() => expect(setSessionScheduleMock).toHaveBeenCalledWith("s-run", T10, 30 * 60_000));
  });

  it("edits a scheduled start time from its keyboard-accessible control", async () => {
    renderView();
    fireEvent.click(await screen.findByRole("button", { name: "Open 2026-09-16" }));
    fireEvent.click(screen.getByRole("tab", { name: "schedule" }));
    fireEvent.click(screen.getByRole("button", { name: "Edit schedule for Review" }));
    const input = screen.getByLabelText("Start time for Review");
    fireEvent.change(input, { target: { value: "10:30" } });
    fireEvent.click(screen.getByRole("button", { name: "Save schedule for Review" }));
    await waitFor(() => expect(setSessionScheduleMock).toHaveBeenCalledWith("s-run", new Date(2026, 8, 16, 10, 30).getTime(), 30 * 60_000));
  });

  it("saves start time and duration together and previews the resulting end time", async () => {
    renderView();
    fireEvent.click(await screen.findByRole("button", { name: "Open 2026-09-16" }));
    fireEvent.click(screen.getByRole("tab", { name: "schedule" }));
    fireEvent.click(screen.getByRole("button", { name: "Edit schedule for Review" }));
    expect((screen.getByLabelText("Duration in minutes for Review") as HTMLInputElement).value).toBe("30");
    fireEvent.change(screen.getByLabelText("Start time for Review"), { target: { value: "10:37" } });
    fireEvent.change(screen.getByLabelText("Duration in minutes for Review"), { target: { value: "45" } });
    expect(screen.getByText("Ends at 11:22")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Save schedule for Review" }));
    await waitFor(() => expect(setSessionScheduleMock).toHaveBeenCalledWith("s-run", new Date(2026, 8, 16, 10, 37).getTime(), 45 * 60_000));
  });

  it("changes duration without moving the start", async () => {
    renderView();
    fireEvent.click(await screen.findByRole("button", { name: "Open 2026-09-16" }));
    fireEvent.click(screen.getByRole("tab", { name: "schedule" }));
    fireEvent.click(screen.getByRole("button", { name: "Edit schedule for Review" }));
    fireEvent.change(screen.getByLabelText("Duration in minutes for Review"), { target: { value: "60" } });
    fireEvent.click(screen.getByRole("button", { name: "Save schedule for Review" }));
    await waitFor(() => expect(setSessionScheduleMock).toHaveBeenCalledWith("s-run", T9, 60 * 60_000));
  });

  it.each(["", "0", "-5", "1.5"])("does not save an invalid duration %s", async (value) => {
    renderView();
    fireEvent.click(await screen.findByRole("button", { name: "Open 2026-09-16" }));
    fireEvent.click(screen.getByRole("tab", { name: "schedule" }));
    fireEvent.click(screen.getByRole("button", { name: "Edit schedule for Review" }));
    fireEvent.change(screen.getByLabelText("Duration in minutes for Review"), { target: { value } });
    const save = screen.getByRole("button", { name: "Save schedule for Review" }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    fireEvent.submit(save.closest("form")!);
    expect(setSessionScheduleMock).not.toHaveBeenCalled();
  });

  it("discards duration edits on cancel and reopens with the saved value", async () => {
    renderView();
    fireEvent.click(await screen.findByRole("button", { name: "Open 2026-09-16" }));
    fireEvent.click(screen.getByRole("tab", { name: "schedule" }));
    fireEvent.click(screen.getByRole("button", { name: "Edit schedule for Review" }));
    fireEvent.change(screen.getByLabelText("Duration in minutes for Review"), { target: { value: "60" } });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(setSessionScheduleMock).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Edit schedule for Review" }));
    expect((screen.getByLabelText("Duration in minutes for Review") as HTMLInputElement).value).toBe("30");
  });

  it("moves a scheduled focus block back to Unscheduled through the existing schedule IPC", async () => {
    renderView();
    fireEvent.click(await screen.findByRole("button", { name: "Open 2026-09-16" }));
    fireEvent.click(screen.getByRole("tab", { name: "schedule" }));
    fireEvent.click(screen.getByRole("button", { name: "Edit schedule for Review" }));
    fireEvent.click(screen.getByRole("button", { name: "Move Review to Unscheduled" }));
    await waitFor(() => expect(setSessionScheduleMock).toHaveBeenCalledWith("s-run", null, null));
  });

  it("accepts dragging a scheduled block back into the unscheduled area", async () => {
    renderView();
    fireEvent.click(await screen.findByRole("button", { name: "Open 2026-09-16" }));
    fireEvent.click(screen.getByRole("tab", { name: "schedule" }));
    const source = screen.getByTestId("timeline").querySelector('[data-scheduled-block="s-run"]')!;
    const payload = transfer();
    fireEvent.dragStart(source, payload);
    fireEvent.drop(screen.getByTestId("staging-area"), payload);
    await waitFor(() => expect(setSessionScheduleMock).toHaveBeenCalledWith("s-run", null, null));
  });

  it("keeps the original schedule visible when a time edit fails", async () => {
    setSessionScheduleMock.mockRejectedValueOnce(new Error("Schedule failed"));
    renderView();
    fireEvent.click(await screen.findByRole("button", { name: "Open 2026-09-16" }));
    fireEvent.click(screen.getByRole("tab", { name: "schedule" }));
    fireEvent.click(screen.getByRole("button", { name: "Edit schedule for Review" }));
    fireEvent.change(screen.getByLabelText("Start time for Review"), { target: { value: "10:30" } });
    fireEvent.change(screen.getByLabelText("Duration in minutes for Review"), { target: { value: "45" } });
    fireEvent.click(screen.getByRole("button", { name: "Save schedule for Review" }));
    expect(await screen.findByText("Schedule failed")).toBeTruthy();
    expect((screen.getByLabelText("Start time for Review") as HTMLInputElement).value).toBe("10:30");
    expect((screen.getByLabelText("Duration in minutes for Review") as HTMLInputElement).value).toBe("45");
    expect(screen.getByTestId("timeline").querySelector('[data-scheduled-block="s-run"]')?.getAttribute("title"))
      .toContain("09:00");
  });

  it("does not allow started or ended focus blocks to be rescheduled", async () => {
    const lockedRange = structuredClone(RANGE);
    lockedRange.days.find((day) => day.date === "2026-09-16")!.sessions.push(sessionOf("s-locked", "Locked", {
      started: true,
      duration: 30 * 60_000,
      schedule: {
        session_id: "s-locked",
        day_cycle_id: "day-2026-09-16",
        starts_at: T9 + 60 * 60_000,
        ends_at: T9 + 90 * 60_000,
        duration_ms: 30 * 60_000,
        truncated: false,
      },
    }));
    getCalendarRangeMock.mockResolvedValueOnce(lockedRange);
    renderView();
    fireEvent.click(await screen.findByRole("button", { name: "Open 2026-09-16" }));
    fireEvent.click(screen.getByRole("tab", { name: "schedule" }));
    const locked = screen.getByTestId("timeline").querySelector('[data-scheduled-block="s-locked"]')!;
    expect(locked.getAttribute("aria-disabled")).toBe("true");
    expect(locked.getAttribute("draggable")).not.toBe("true");
    expect(screen.queryByRole("button", { name: "Edit schedule for Locked" })).toBeNull();
    fireEvent.dragStart(locked, transfer());
    expect(setSessionScheduleMock).not.toHaveBeenCalled();
  });

  it("drops onto an empty date with the optimistic move (no strategy dialog)", async () => {
    const { container } = renderView();
    const chip = await screen.findByTitle("Move 2026-09-16");
    const payload = transfer();
    fireEvent.dragStart(chip, payload);
    const target = container.querySelector('[data-day-cell="2026-09-18"]');
    expect(target).toBeTruthy();
    fireEvent.drop(target!, payload);
    await waitFor(() =>
      expect(moveDayCycleMock).toHaveBeenCalledWith("day-2026-09-16", "2026-09-18", null),
    );
    expect(screen.queryByText("This date already has a plan")).toBeNull();
  });

  it("asks for a strategy when the target is occupied and sends merge", async () => {
    const { container } = renderView();
    const chip = await screen.findByTitle("Move 2026-09-16");
    const payload = transfer();
    fireEvent.dragStart(chip, payload);
    fireEvent.drop(container.querySelector('[data-day-cell="2026-09-17"]')!, payload);
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
    fireEvent.click(screen.getByRole("tab", { name: "schedule" }));
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

/**
 * 日历纯函数层测试（tasks.md §5.8 的预算三态部分 + 乐观移动 + 网格区间）。
 * 全部无 IO、无时区依赖：ISO 日期按 UTC 午夜整数运算。
 */
import { describe, expect, it } from "vitest";

import type { Cycle } from "../../lib/types";
import type { CalendarDay, CalendarRange } from "./api";
import {
  PREFERRED_VIEW_KEY,
  addMonthsISO,
  applyMoveToRange,
  budgetState,
  loadPreferredView,
  monthBounds,
  overlapIds,
  scheduleOverlapLanes,
  savePreferredView,
  weekBounds,
} from "./calendar-model";

function makeCycle(overrides: Record<string, unknown>): Cycle {
  return {
    id: "day-x",
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
    starts_on: "2026-09-16",
    ends_on: "2026-09-17",
    calendar_key: "day:2026-09-16",
    repeat_id: null,
    created_at: 0,
    ...overrides,
  } as Cycle;
}

function cell(date: string, day: CalendarDay["day_cycle"], sessions: CalendarDay["sessions"] = []): CalendarDay {
  return { date, in_range: true, day_cycle: day, sessions };
}

const SOURCE_DAY = makeCycle({ id: "day-a", starts_on: "2026-09-16", calendar_key: "day:2026-09-16" });

const RANGE: CalendarRange = {
  start: "2026-09-01",
  end: "2026-09-30",
  grid_start: "2026-08-31",
  grid_end: "2026-10-04",
  week_start_day: 1,
  days: [
    cell("2026-09-16", SOURCE_DAY, [
      {
        session: makeCycle({ id: "s-1", type: "session", parent_id: "day-a" }),
        schedule: null,
      },
    ]),
    cell("2026-09-17", null),
    cell("2026-09-18", makeCycle({ id: "day-b", starts_on: "2026-09-18" })),
  ],
};

describe("grid bounds", () => {
  it("monthBounds covers the whole calendar month", () => {
    expect(monthBounds("2026-09-16")).toEqual({ start: "2026-09-01", end: "2026-09-30" });
    expect(monthBounds("2026-12-31")).toEqual({ start: "2026-12-01", end: "2026-12-31" });
  });

  it("weekBounds is Monday-anchored and spans 7 days", () => {
    // 2026-09-16 is a Wednesday; the week runs Mon 09-14 … Sun 09-20.
    expect(weekBounds("2026-09-16")).toEqual({ start: "2026-09-14", end: "2026-09-20" });
    expect(weekBounds("2026-09-14")).toEqual(weekBounds("2026-09-20"));
  });

  it("addMonthsISO clamps overflow days and walks backwards", () => {
    expect(addMonthsISO("2026-01-31", 1)).toBe("2026-02-28");
    expect(addMonthsISO("2026-09-16", -1)).toBe("2026-08-16");
    expect(addMonthsISO("2026-11-30", 2)).toBe("2027-01-30");
    expect(addMonthsISO("2026-09-16", 1)).toBe("2026-10-16");
  });
});

describe("applyMoveToRange (optimistic drag)", () => {
  it("moves the day cycle and its sessions onto the empty target date", () => {
    const next = applyMoveToRange(RANGE, "day-a", "2026-09-17");
    const source = next.days.find((d) => d.date === "2026-09-16");
    const target = next.days.find((d) => d.date === "2026-09-17");
    expect(source?.day_cycle).toBeNull();
    expect(source?.sessions).toEqual([]);
    expect(target?.day_cycle?.id).toBe("day-a");
    // 日期身份同步改写为后端将要落库的值。
    expect(target?.day_cycle?.starts_on).toBe("2026-09-17");
    expect(target?.day_cycle?.calendar_key).toBe("day:2026-09-17");
    expect(target?.sessions).toHaveLength(1);
    // 其余格子不受影响。
    expect(next.days.find((d) => d.date === "2026-09-18")?.day_cycle?.id).toBe("day-b");
  });

  it("never mutates the snapshot (rollback needs it intact)", () => {
    applyMoveToRange(RANGE, "day-a", "2026-09-17");
    expect(RANGE.days.find((d) => d.date === "2026-09-16")?.day_cycle?.id).toBe("day-a");
    expect(RANGE.days.find((d) => d.date === "2026-09-17")?.day_cycle).toBeNull();
  });

  it("refuses to overwrite an occupied target or an unknown source", () => {
    expect(applyMoveToRange(RANGE, "day-a", "2026-09-18")).toBe(RANGE);
    expect(applyMoveToRange(RANGE, "day-zz", "2026-09-17")).toBe(RANGE);
  });
});

describe("budgetState (three presentations)", () => {
  const budget = (capacity_minutes: number | null, scheduled_minutes: number) => ({
    date: "2026-09-16",
    capacity_minutes,
    scheduled_minutes,
  });

  it("unset is a distinct state so the UI renders nothing", () => {
    expect(budgetState(budget(null, 75))).toEqual({ kind: "unset" });
  });

  it("under budget reports remaining minutes", () => {
    expect(budgetState(budget(120, 75))).toEqual({ kind: "under", remainingMinutes: 45 });
    expect(budgetState(budget(120, 120)).kind).toBe("under");
  });

  it("over budget reports the excess", () => {
    expect(budgetState(budget(60, 135))).toEqual({ kind: "over", overMinutes: 75 });
  });
});

describe("overlapIds", () => {
  it("collects both members of every pair for the conflict tint", () => {
    const schedule = (session_id: string) => ({
      session_id,
      day_cycle_id: "day-a",
      starts_at: 0,
      ends_at: 1,
      duration_ms: 1,
      truncated: false,
    });
    const ids = overlapIds([
      { first: schedule("a"), second: schedule("b") },
      { first: schedule("b"), second: schedule("c") },
    ]);
    expect([...ids].sort()).toEqual(["a", "b", "c"]);
  });
});

describe("scheduleOverlapLanes", () => {
  const scheduled = (id: string, starts_at: number, ends_at: number) => ({
    session: makeCycle({ id, type: "session" }),
    schedule: {
      session_id: id,
      day_cycle_id: "day-a",
      starts_at,
      ends_at,
      duration_ms: ends_at - starts_at,
      truncated: false,
    },
  });

  it("greedily reuses lanes inside a transitive overlap group", () => {
    const lanes = scheduleOverlapLanes([
      scheduled("a", 9, 10),
      scheduled("b", 9.5, 11),
      scheduled("c", 10, 10.5),
      scheduled("isolated", 12, 13),
      { session: makeCycle({ id: "staged", type: "session" }), schedule: null },
    ]);
    expect(lanes.get("a")).toEqual({ lane: 0, lanes: 2 });
    expect(lanes.get("b")).toEqual({ lane: 1, lanes: 2 });
    expect(lanes.get("c")).toEqual({ lane: 0, lanes: 2 });
    expect(lanes.get("isolated")).toEqual({ lane: 0, lanes: 1 });
    expect(lanes.has("staged")).toBe(false);
  });

  it("uses stable id order for equal starts and treats touching intervals as separate groups", () => {
    const lanes = scheduleOverlapLanes([
      scheduled("z", 0, 10),
      scheduled("a", 0, 10),
      scheduled("next", 10, 20),
    ]);
    expect(lanes.get("a")).toEqual({ lane: 0, lanes: 2 });
    expect(lanes.get("z")).toEqual({ lane: 1, lanes: 2 });
    expect(lanes.get("next")).toEqual({ lane: 0, lanes: 1 });
  });
});

describe("preferred view memory", () => {
  it("round-trips the saved density", () => {
    localStorage.removeItem(PREFERRED_VIEW_KEY);
    expect(loadPreferredView()).toBe("month"); // 默认月视图
    savePreferredView("week");
    expect(loadPreferredView()).toBe("week");
    expect(localStorage.getItem(PREFERRED_VIEW_KEY)).toBe("week");
  });

  it("treats unknown stored values as month", () => {
    localStorage.setItem(PREFERRED_VIEW_KEY, "year");
    expect(loadPreferredView()).toBe("month");
  });
});

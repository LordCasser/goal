/**
 * 日历视图的纯函数层（测试重点，对应 tasks.md §5.8）：
 * 视图偏好的本地记忆、月/周网格区间推导、
 * 时间预算的三态判定与重叠标记收集。
 *
 * 日期算术沿用 features/planner/dates.ts 的约束：ISO 日期按 UTC 午夜整数
 * 运算，不引入时区偏移；唯一的时间轴本地 midnight 换算在 DayTimeline 内，
 * 因为排期时间是本地墙钟概念。
 */
import { addDaysISO, parseISODay } from "../planner/dates";
import { LATER_CYCLE_ID, type Cycle, type TaskNode } from "../../lib/ipc";

import type { CalendarDay, ScheduleOverlap, TimeBudget } from "./api";

export type CalendarViewMode = "month" | "week";

/** spec（calendar-view「记住偏好」）：重启后默认打开上次选择的视图。 */
export const PREFERRED_VIEW_KEY = "planner.calendar-density";

export function loadPreferredView(): CalendarViewMode {
  try {
    return localStorage.getItem(PREFERRED_VIEW_KEY) === "week" ? "week" : "month";
  } catch {
    // localStorage 不可用（私有模式等）：回到默认月视图即可。
    return "month";
  }
}

export function savePreferredView(view: CalendarViewMode): void {
  try {
    localStorage.setItem(PREFERRED_VIEW_KEY, view);
  } catch {
    // 同上：记忆失败不影响功能，只是下次启动回到默认。
  }
}

/** Read the same editor content as Workspace, scoped to the visible dates and their ancestors. */
export function calendarPlanCycleIds(cycles: Cycle[], start: string, end: string): string[] {
  const byId = new Map(cycles.map((cycle) => [cycle.id, cycle]));
  const ids = new Set<string>();
  const include = (cycle: Cycle) => {
    if (ids.has(cycle.id) || cycle.id === LATER_CYCLE_ID || cycle.archived) return;
    ids.add(cycle.id);
    const parent = cycle.parent_id ? byId.get(cycle.parent_id) : undefined;
    if (parent) include(parent);
  };
  cycles.filter((cycle) => cycle.type === "month" || (cycle.type !== "session" && cycle.starts_on !== null && cycle.ends_on !== null
    && cycle.starts_on <= end && cycle.ends_on > start)).forEach(include);
  return [...ids].sort();
}

/** Preview rows share the editor projection; the card labels and locks them. */
export function calendarTasks(tasks: TaskNode[]): TaskNode[] {
  return tasks.filter((task) => task.title.trim() !== "");
}

const MS_PER_DAY = 86_400_000;

function isoOfUTC(date: Date): string {
  const two = (n: number) => String(n).padStart(2, "0");
  return `${date.getUTCFullYear()}-${two(date.getUTCMonth() + 1)}-${two(date.getUTCDate())}`;
}

/** 锚点日期所在自然月的第一天与最后一天（ISO，含两端）。 */
export function monthBounds(anchorISO: string): { start: string; end: string } {
  const epochDay = parseISODay(anchorISO);
  if (epochDay === null) return { start: anchorISO, end: anchorISO };
  const date = new Date(epochDay * MS_PER_DAY);
  const first = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), 1));
  const nextFirst = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth() + 1, 1));
  return { start: isoOfUTC(first), end: addDaysISO(isoOfUTC(nextFirst), -1) };
}

/** 锚点日期所在的整周（周一为始，与后端默认 week_start_day=1 一致）。 */
export function weekBounds(anchorISO: string): { start: string; end: string } {
  const epochDay = parseISODay(anchorISO);
  if (epochDay === null) return { start: anchorISO, end: anchorISO };
  const weekday = new Date(epochDay * MS_PER_DAY).getUTCDay(); // 0=Sun … 6=Sat
  const back = (weekday + 6) % 7;
  const start = addDaysISO(anchorISO, -back);
  return { start, end: addDaysISO(start, 6) };
}

/** 整月加减（日超出目标月时钳到月末，2026-01-31 + 1月 → 2026-02-28）。 */
export function addMonthsISO(anchorISO: string, delta: number): string {
  const epochDay = parseISODay(anchorISO);
  if (epochDay === null) return anchorISO;
  const date = new Date(epochDay * MS_PER_DAY);
  const target = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth() + delta, 1));
  const daysInMonth = new Date(
    Date.UTC(target.getUTCFullYear(), target.getUTCMonth() + 1, 0),
  ).getUTCDate();
  target.setUTCDate(Math.min(date.getUTCDate(), daysInMonth));
  return isoOfUTC(target);
}

/** 格子摘要（spec：至少包含专注块数量与完成进度）。 */
export function dayProgress(day: CalendarDay): { total: number; finished: number } {
  const total = day.sessions.length;
  const finished = day.sessions.filter((s) => s.session.finished).length;
  return { total, finished };
}

/** 重叠色标只需 session id 集合（时间轴上出现几次都标同一色）。 */
export function overlapIds(overlaps: ScheduleOverlap[]): Set<string> {
  const ids = new Set<string>();
  for (const pair of overlaps) {
    ids.add(pair.first.session_id);
    ids.add(pair.second.session_id);
  }
  return ids;
}

export interface ScheduleOverlapLane {
  lane: number;
  lanes: number;
}

/**
 * Assign scheduled sessions to horizontal lanes without changing their times.
 * Intervals that touch at an endpoint do not overlap; transitive overlaps stay
 * in one connected group so each group can use its own full available width.
 * Ties are stable by session id, making the layout deterministic across query
 * refreshes.
 */
export function scheduleOverlapLanes(
  sessions: CalendarDay["sessions"],
): Map<string, ScheduleOverlapLane> {
  const intervals = sessions
    .filter((item): item is CalendarDay["sessions"][number] & { schedule: NonNullable<CalendarDay["sessions"][number]["schedule"]> } => item.schedule !== null)
    .map((item) => ({
      id: item.session.id,
      startsAt: item.schedule.starts_at,
      endsAt: Math.max(item.schedule.ends_at, item.schedule.starts_at),
    }))
    .sort((a, b) => a.startsAt - b.startsAt || a.id.localeCompare(b.id));

  const result = new Map<string, ScheduleOverlapLane>();
  let group: typeof intervals = [];
  let groupEndsAt = Number.NEGATIVE_INFINITY;

  const assignGroup = (items: typeof intervals) => {
    if (items.length === 0) return;
    const laneEndsAt: number[] = [];
    const assignments: Array<{ id: string; lane: number }> = [];
    for (const item of items) {
      let lane = laneEndsAt.findIndex((endsAt) => endsAt <= item.startsAt);
      if (lane === -1) lane = laneEndsAt.length;
      laneEndsAt[lane] = item.endsAt;
      assignments.push({ id: item.id, lane });
    }
    const lanes = laneEndsAt.length;
    for (const assignment of assignments) result.set(assignment.id, { lane: assignment.lane, lanes });
  };

  for (const interval of intervals) {
    if (group.length > 0 && interval.startsAt >= groupEndsAt) {
      assignGroup(group);
      group = [];
      groupEndsAt = Number.NEGATIVE_INFINITY;
    }
    group.push(interval);
    groupEndsAt = Math.max(groupEndsAt, interval.endsAt);
  }
  assignGroup(group);
  return result;
}

/**
 * 预算三态（spec：未超/超出/未设置）。未设置必须是可区分的一态——
 * 界面对它什么都不渲染，绝不出现在何空进度条。
 */
export type BudgetState =
  | { kind: "unset" }
  | { kind: "under"; remainingMinutes: number }
  | { kind: "over"; overMinutes: number };

export function budgetState(budget: TimeBudget): BudgetState {
  if (budget.capacity_minutes === null) return { kind: "unset" };
  const delta = budget.capacity_minutes - budget.scheduled_minutes;
  if (delta >= 0) return { kind: "under", remainingMinutes: delta };
  return { kind: "over", overMinutes: -delta };
}

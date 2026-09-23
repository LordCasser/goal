/**
 * 日历视图的本地 IPC 层（change: add-calendar-time-view §5）。
 *
 * 与 src/lib/ipc.ts 同一约定：命令名集中声明、顶层参数键使用 camelCase，嵌套数据使用
 * Rust serde 的 snake_case、`Option<T>` 以 `T | null` 表达。这里不复用 lib/ipc.ts
 * 是刻意的——日历的 5 个命令（get_calendar_range 等）只服务本特性，避免
 * 每加一个视图都去膨胀全局 IPC 面；ensure_day 是既有命令，直接从 lib/ipc
 * 导入使用。类型与 src-tauri/src/service/calendar.rs 的 serde 输出 1:1。
 */
import { invoke } from "@tauri-apps/api/core";

import type { Cycle } from "../../lib/types";

export type MoveStrategy = "move" | "merge" | "swap";

/** `service::calendar::SessionSchedule`（已截断到当日的排期投影）。 */
export interface SessionSchedule {
  session_id: string;
  day_cycle_id: string;
  /** 计划开始时间，epoch 毫秒。 */
  starts_at: number;
  /** 计划结束时间；跨午夜时被截断到当日零点前。 */
  ends_at: number;
  /** 承诺时长（非截断后的跨度）。 */
  duration_ms: number;
  truncated: boolean;
}

/** `service::calendar::CalendarSession`——专注块 + 它的排期（可空）。 */
export interface CalendarSession {
  session: Cycle;
  schedule: SessionSchedule | null;
}

/** `service::calendar::CalendarDay`——一个格子：日期、日周期与专注块。 */
export interface CalendarDay {
  /** 本地日期 `YYYY-MM-DD`。 */
  date: string;
  /** 是否落在请求区间内（前后填充日期为 false，渲染弱化样式）。 */
  in_range: boolean;
  day_cycle: Cycle | null;
  sessions: CalendarSession[];
}

/** `service::calendar::CalendarRange`——按整周对齐的网格区间。 */
export interface CalendarRange {
  start: string;
  end: string;
  grid_start: string;
  grid_end: string;
  week_start_day: number;
  days: CalendarDay[];
}

/** `service::calendar::MoveDayOutcome`——移动结果，供前端重排乐观状态。 */
export interface MoveDayOutcome {
  strategy: MoveStrategy;
  source_day_id: string;
  target_day_id: string;
  deleted_day_ids: string[];
}

/** `service::calendar::ScheduleOverlap`——一对重叠排期。 */
export interface ScheduleOverlap {
  first: SessionSchedule;
  second: SessionSchedule;
}

/** `service::calendar::TimeBudget`——capacity 为 null 表示未设置。 */
export interface TimeBudget {
  date: string;
  capacity_minutes: number | null;
  scheduled_minutes: number;
}

const commands = {
  getCalendarRange: "get_calendar_range",
  moveDayCycle: "move_day_cycle",
  setSessionSchedule: "set_session_schedule",
  deleteDayFocusBlocks: "delete_day_focus_blocks",
  clearDaySchedules: "clear_day_schedules",
  getScheduleOverlaps: "get_schedule_overlaps",
  getTimeBudget: "get_time_budget",
} as const;

/** `[start, end]`（含两端）的日周期网格，后端补齐整周避免空洞。 */
export function getCalendarRange(start: string, end: string): Promise<CalendarRange> {
  return invoke<CalendarRange>(commands.getCalendarRange, { start, end });
}

/** strategy 传 null 时：目标为空按 move 处理，已被占用则报 `target_exists`。 */
export function moveDayCycle(
  cycle_id: string,
  target_date: string,
  strategy: MoveStrategy | null,
): Promise<MoveDayOutcome> {
  return invoke<MoveDayOutcome>(commands.moveDayCycle, { cycleId: cycle_id, targetDate: target_date, strategy });
}

/** 把专注块放到时间轴上；`starts_at` 为 null 时清除排期但保留承诺时长。 */
export function setSessionSchedule(
  session_id: string,
  starts_at: number | null,
  duration_ms: number | null,
): Promise<SessionSchedule | null> {
  return invoke<SessionSchedule | null>(commands.setSessionSchedule, {
    sessionId: session_id,
    startsAt: starts_at,
    durationMs: duration_ms,
  });
}

export function deleteDayFocusBlocks(day_cycle_id: string, expected_ids: string[]): Promise<number> {
  return invoke<number>(commands.deleteDayFocusBlocks, { dayCycleId: day_cycle_id, expectedIds: expected_ids });
}

export function clearDaySchedules(day_cycle_id: string, expected_ids: string[]): Promise<number> {
  return invoke<number>(commands.clearDaySchedules, { dayCycleId: day_cycle_id, expectedIds: expected_ids });
}

/** 某日全部重叠排期对（纯查询，后端不改数据）。 */
export function getScheduleOverlaps(day_cycle_id: string): Promise<ScheduleOverlap[]> {
  return invoke<ScheduleOverlap[]>(commands.getScheduleOverlaps, { dayCycleId: day_cycle_id });
}

/** 单日时间预算；`capacity_minutes` 为 null 时界面不渲染预算条。 */
export function getTimeBudget(date: string): Promise<TimeBudget> {
  return invoke<TimeBudget>(commands.getTimeBudget, { date });
}

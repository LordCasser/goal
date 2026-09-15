/**
 * planner 列内的日期与时长格式化工具。ISO 日期的加减/周号与 timeline.ts
 * 一样走 UTC 午夜整数算术（同一约束：不引入时区偏移）；「今天」例外——
 * 它必须取本地时区的今天，因为周期日期是本地日概念（types.ts 对
 * starts_on 的说明）。展示格式固定英文短格式，与列头大写标识一致。
 */

import { formatDate as localizedDate, formatDuration as localizedDuration } from "../../lib/i18n";

const MS_PER_DAY = 86_400_000;

/** 本地时区的今天，`YYYY-MM-DD`（design.md §9.2：时长预览用本地今天）。 */
export function todayISO(): string {
  const now = new Date();
  const y = String(now.getFullYear()).padStart(4, "0");
  const m = String(now.getMonth() + 1).padStart(2, "0");
  const d = String(now.getDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

/** 严格解析 `YYYY-MM-DD`；非法返回 null（不抛错，供宽松的展示路径使用）。 */
export function parseISODay(iso: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  if (!match) return null;
  const [y, m, d] = [Number(match[1]), Number(match[2]), Number(match[3])];
  const utc = Date.UTC(y, m - 1, d);
  const round = new Date(utc);
  if (
    round.getUTCFullYear() !== y ||
    round.getUTCMonth() !== m - 1 ||
    round.getUTCDate() !== d
  ) {
    return null;
  }
  return utc / MS_PER_DAY;
}

function fromEpochDay(epochDay: number): string {
  const d = new Date(epochDay * MS_PER_DAY);
  return `${String(d.getUTCFullYear()).padStart(4, "0")}-${String(d.getUTCMonth() + 1).padStart(2, "0")}-${String(d.getUTCDate()).padStart(2, "0")}`;
}

/** 整数天加减；输入非法时原样返回（调用方展示空串更安全）。 */
export function addDaysISO(iso: string, days: number): string {
  const base = parseISODay(iso);
  return base === null ? iso : fromEpochDay(base + days);
}

/** a - b 的整天数（a 晚于 b 为正）；任一非法返回 null。 */
export function daysBetween(aISO: string, bISO: string): number | null {
  const a = parseISODay(aISO);
  const b = parseISODay(bISO);
  return a === null || b === null ? null : a - b;
}

/** ISO 8601 周号（周一为一周之始）。与后端 week_key 的日历语义一致。 */
export function isoWeekNumber(iso: string): number | null {
  const epochDay = parseISODay(iso);
  if (epochDay === null) return null;
  const date = new Date(epochDay * MS_PER_DAY);
  const dayNum = date.getUTCDay() || 7; // 周一=1 … 周日=7
  // 平移到本周周四，再数它离当年 1 月 1 日有几个 7 天。
  const thursday = new Date(date);
  thursday.setUTCDate(thursday.getUTCDate() + 4 - dayNum);
  const yearStart = Date.UTC(thursday.getUTCFullYear(), 0, 1);
  return Math.ceil(((thursday.getTime() - yearStart) / MS_PER_DAY + 1) / 7);
}

/** Locale-aware short date display; ISO-only values remain local calendar dates. */
export function formatShortDate(iso: string): string {
  return localizedDate(iso, { month: "short", day: "numeric" });
}

/** Locale-aware weekday display for a date-only identifier. */
export function weekdayName(iso: string): string {
  return localizedDate(iso, { weekday: "long" });
}

/** `Sep 14 – Sep 20`；起止相同只显示一端。 */
export function formatDateRange(startISO: string | null, endISO: string | null): string | null {
  if (!startISO || !endISO) return startISO ?? endISO ?? null;
  if (startISO === endISO) return formatShortDate(startISO);
  return `${formatShortDate(startISO)} – ${formatShortDate(endISO)}`;
}

/** 距结束还剩几个整周（向上取整，不足一周算一周）；已过期返回 0。 */
export function remainingWeeks(endsOn: string | null, today: string): number | null {
  if (!endsOn) return null;
  const left = daysBetween(endsOn, today);
  if (left === null) return null;
  return left <= 0 ? 0 : Math.ceil(left / 7);
}

/** 毫秒 → 当前语言的时长；null（未设时长）返回 "—"。 */
export function formatDuration(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return "—";
  return localizedDuration(ms);
}

/** 毫秒 → `12:34` / `1:02:03`；负数钳到 0（计时读数不出现负号）。 */
export function formatClock(ms: number): string {
  const safe = Math.max(0, ms);
  const totalSeconds = Math.floor(safe / 1000);
  const seconds = totalSeconds % 60;
  const minutes = Math.floor(totalSeconds / 60) % 60;
  const hours = Math.floor(totalSeconds / 3600);
  const two = (n: number) => String(n).padStart(2, "0");
  return hours > 0 ? `${hours}:${two(minutes)}:${two(seconds)}` : `${two(minutes)}:${two(seconds)}`;
}

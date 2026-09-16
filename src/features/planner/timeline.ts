/**
 * 纯前端的长周期时间线推导（design.md §9.2「周期时长选择」：选项与日期
 * 后果同屏，切换立即更新；确认前不发任何请求）。
 *
 * 算术与 Rust 侧保持同一套规则：一个「产品月」固定 28 天
 * （src-tauri/src/domain/cycle.rs DAYS_PER_MONTH_UNIT），不是日历月；
 * 复盘日 = start + months*28 天（对齐 domain/calendar.rs calculate_ends_on）。
 * 日期一律用 UTC 午夜做整数天加减，不用本地时区的 Date 算术——否则
 * DST 或负时区偏移会把「哪一天」算漂（types.ts 对 starts_on 的注释同源）。
 *
 * 纯函数、零依赖；非法输入直接抛错而不是静默给出错误日期。
 */

export type TimelineNode = {
  key: "set" | "progress" | "review";
  label: string;
  /** 本地日期 `YYYY-MM-DD`。 */
  date: string;
};

/** 1 产品月 = 28 天（Rust domain::cycle::DAYS_PER_MONTH_UNIT）。 */
const DAYS_PER_MONTH_UNIT = 28;
const MS_PER_DAY = 86_400_000;

/** 允许的时长（Rust LONG_TERM_DURATIONS_MONTHS）；运行时仍做防御检查。 */
export type TimelineMonths = 1 | 3 | 6;

export type CustomTimelineError = "invalid_cycle_range" | "invalid_progress_check";

export type CustomTimeline = {
  days: number | null;
  checkDates: string[];
  totalChecks: number;
  error: CustomTimelineError | null;
};

/**
 * Derives a custom cycle's review checkpoints using date-only arithmetic.
 * The end date is the review boundary and is therefore excluded from checks.
 * Only a small preview is materialized; totalChecks stays arithmetic so a
 * long interval cannot make the dialog render an unbounded list.
 */
export function deriveCustomTimeline(
  startISO: string,
  endISO: string,
  check: import("../../lib/types").ProgressCheck | null,
  previewLimit = 4,
): CustomTimeline {
  const start = toEpochDay(startISO);
  const end = toEpochDay(endISO);
  if (start === null || end === null || end <= start) {
    return { days: null, checkDates: [], totalChecks: 0, error: "invalid_cycle_range" };
  }
  if (!check) {
    return { days: end - start, checkDates: [], totalChecks: 0, error: "invalid_progress_check" };
  }

  if (check.kind === "once") {
    const date = toEpochDay(check.date);
    if (date === null || date < start || date >= end) {
      return { days: end - start, checkDates: [], totalChecks: 0, error: "invalid_progress_check" };
    }
    return { days: end - start, checkDates: [check.date], totalChecks: 1, error: null };
  }

  if (!Number.isSafeInteger(check.every_days) || check.every_days <= 0) {
    return { days: end - start, checkDates: [], totalChecks: 0, error: "invalid_progress_check" };
  }
  const interval = check.every_days;
  const days = end - start;
  const totalChecks = Math.max(0, Math.floor((days - 1) / interval));
  const count = Math.min(totalChecks, Math.max(0, previewLimit));
  const checkDates = Array.from({ length: count }, (_, index) =>
    fromEpochDay(start + (index + 1) * interval),
  );
  return { days, checkDates, totalChecks, error: null };
}

/** 严格解析 `YYYY-MM-DD` 为 UTC 午夜 epoch 天数；格式或日历不合法返回 null。 */
function toEpochDay(iso: string): number | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  if (!match) return null;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  const utc = Date.UTC(year, month - 1, day);
  const round = new Date(utc);
  // Date.UTC 会把 2026-02-30 滚动到 3 月：回读校验拦住这类伪日期。
  if (
    round.getUTCFullYear() !== year ||
    round.getUTCMonth() !== month - 1 ||
    round.getUTCDate() !== day
  ) {
    return null;
  }
  return utc / MS_PER_DAY;
}

function fromEpochDay(epochDay: number): string {
  const d = new Date(epochDay * MS_PER_DAY);
  const y = String(d.getUTCFullYear()).padStart(4, "0");
  const m = String(d.getUTCMonth() + 1).padStart(2, "0");
  const day = String(d.getUTCDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

/**
 * 从设定日推导三节点时间线：设定日 = start；推进阶段 = 半程（整天数向上
 * 取整，产品月都是 28 天故恒为整除）；复盘日 = start + months*28 天。
 * weeks = months*4（产品月的周数表达，design.md §9.2 的 4/12/24 周）。
 */
/** 三个节点固定顺序：set → progress → review（元组让下标访问免空检查）。 */
export type Timeline = { nodes: [TimelineNode, TimelineNode, TimelineNode]; weeks: number };

export function deriveTimeline(startISO: string, months: TimelineMonths): Timeline {
  if (months !== 1 && months !== 3 && months !== 6) {
    throw new Error(`deriveTimeline: months must be 1, 3 or 6, got ${String(months)}`);
  }
  const start = toEpochDay(startISO);
  if (start === null) {
    throw new Error(`deriveTimeline: start must be a valid YYYY-MM-DD date, got ${JSON.stringify(startISO)}`);
  }
  const durationDays = months * DAYS_PER_MONTH_UNIT;
  const progressOffset = Math.ceil(durationDays / 2);
  // 三元组类型：消费方按下标取「复盘日」时不受 noUncheckedIndexedAccess 影响。
  const nodes: [TimelineNode, TimelineNode, TimelineNode] = [
    { key: "set", label: "Set goals", date: fromEpochDay(start) },
    { key: "progress", label: "Progress check", date: fromEpochDay(start + progressOffset) },
    { key: "review", label: "Review", date: fromEpochDay(start + durationDays) },
  ];
  return { nodes, weeks: months * 4 };
}

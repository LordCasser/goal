/**
 * 单个周期列（任务 7.4 列头 + 独立纵向滚动）。列头四要素：类型标识、标题、
 * 日期范围或剩余时间、选项菜单入口。标题只有 session 可改（update_cycle
 * 的约束），month/week/day 的标题是创建时的承诺，保持静态展示；这些周期
 * 的编辑能力在 FocusArea 的 session 卡片上。
 */
import { useQuery } from "@tanstack/react-query";
import { type Cycle, lifecycleOf, listSessions } from "../../lib/ipc";
import { qk } from "../../lib/events";
import {
  formatDateRange,
  formatDuration,
  formatShortDate,
  isoWeekNumber,
  remainingWeeks,
  todayISO,
  weekdayName,
} from "./dates";
import { CycleOptionsMenu } from "./CycleOptionsMenu";
import { FocusArea } from "./FocusArea";
import { TaskList } from "./TaskList";

const TYPE_BADGE: Record<Cycle["type"], string> = {
  month: "Long-term",
  week: "Week",
  day: "Day",
  session: "Focus",
};

export function CycleColumn({
  cycle,
  selected = false,
  onSelect,
}: {
  cycle: Cycle;
  /** month 列头点击选中（周计划创建时的默认 parent）。 */
  selected?: boolean;
  onSelect?: () => void;
}) {
  const today = todayISO();
  const state = lifecycleOf(cycle);

  // Sessions are not part of the planner payload (it stays column-shaped);
  // the day column fetches its own focus blocks. Session mutations emit
  // cycles:changed for the day, which keeps this query fresh (lib/events.ts).
  const isDay = cycle.type === "day";
  const sessionsQuery = useQuery({
    queryKey: qk.sessions(cycle.id),
    queryFn: () => listSessions(cycle.id),
    enabled: isDay,
  });
  const sessions = isDay ? (sessionsQuery.data ?? []) : [];
  // 一次只推进一个块；互斥按同日判定（design.md §7「同日其他块说明为何暂不能启动」）。
  const runningSessionId =
    sessions.find((s) => s.started && !s.finished)?.id ?? null;

  const badge = badgeText(cycle);
  const meta = metaLine(cycle, today);
  const title = titleText(cycle);

  return (
    /* 列宽固定 512px（--spacing-plan），绝不因视口不足被压缩（spec: 横向并列
     * 的周期列）；列内内容自己纵向滚动。 */
    <section
      aria-label={`${badge} ${title}`}
      className="flex w-plan shrink-0 flex-col overflow-hidden rounded-sm border border-frame bg-content"
    >
      <header
        onClick={onSelect}
        className={[
          "shrink-0 cursor-pointer border-b border-light px-4 py-3",
          selected ? "bg-focus-surface" : "",
        ].join(" ")}
      >
        <div className="flex items-center justify-between gap-2">
          <span className="text-caption font-semibold uppercase tracking-[0.08em] text-secondary">
            {badge}
            {state === "finished" && <span className="ml-2 text-hint">ended</span>}
          </span>
          <CycleOptionsMenu cycle={cycle} runningElsewhere={false} />
        </div>
        <h3 className="mt-1 text-section-title font-bold text-primary">{title}</h3>
        {meta && <p className="mt-0.5 text-caption text-hint">{meta}</p>}
      </header>
      <div className="min-h-0 flex-1 overflow-y-auto px-2 py-2">
        {isDay && (
          <FocusArea day={cycle} sessions={sessions} runningSessionId={runningSessionId} />
        )}
        <TaskList cycleId={cycle.id} cycleType={cycle.type} locked={cycle.finished} />
      </div>
    </section>
  );
}

function badgeText(cycle: Cycle): string {
  if (cycle.type === "week" && cycle.starts_on) {
    const week = isoWeekNumber(cycle.starts_on);
    if (week !== null) return `Week ${week}`;
  }
  if (cycle.type === "day" && cycle.starts_on) {
    return weekdayName(cycle.starts_on);
  }
  return TYPE_BADGE[cycle.type];
}

function titleText(cycle: Cycle): string {
  if (cycle.type === "week" || cycle.type === "day") {
    return cycle.starts_on ? formatShortDate(cycle.starts_on) : cycle.title;
  }
  return cycle.title;
}

/** 列头第二行：日期范围 + 剩余时间/编号 + 实际投入汇总（§3.2/§7）。 */
function metaLine(cycle: Cycle, today: string): string | null {
  const parts: string[] = [];
  if (cycle.type === "month") {
    const range = formatDateRange(cycle.starts_on, cycle.ends_on);
    if (range) parts.push(range);
    const weeks = remainingWeeks(cycle.ends_on, today);
    if (weeks !== null) parts.push(weeks > 0 ? `${weeks} weeks left` : "Ended");
  } else if (cycle.type === "week") {
    const range = formatDateRange(cycle.starts_on, cycle.ends_on);
    if (range) parts.push(range);
  } else if (cycle.type === "day") {
    if (cycle.starts_on) parts.push(cycle.starts_on);
  }
  if (cycle.focused_time > 0) parts.push(`${formatDuration(cycle.focused_time)} focused`);
  return parts.length > 0 ? parts.join(" · ") : null;
}

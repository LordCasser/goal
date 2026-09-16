/**
 * 单个周期列（任务 7.4 列头 + 独立纵向滚动）。列头四要素：类型标识、标题、
 * 日期范围或剩余时间、选项菜单入口。标题只有 session 可改（update_cycle
 * 的约束），month/week/day 的标题是创建时的承诺，保持静态展示；这些周期
 * 的编辑能力在 FocusArea 的 session 卡片上。
 */
import { useQuery } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { type Cycle, type ProgressCheck, lifecycleOf, listSessions } from "../../lib/ipc";
import { qk } from "../../lib/events";
import {
  addDaysISO,
  daysBetween,
  formatDateRange,
  isoWeekNumber,
  remainingWeeks,
  todayISO,
  weekdayName,
} from "./dates";
import { CycleOptionsMenu } from "./CycleOptionsMenu";
import { FocusArea } from "./FocusArea";
import { TaskList } from "./TaskList";
import { deriveCustomTimeline } from "./timeline";
import type { RelationView } from "./relations";
import { PlanWithAI } from "../agent/PlanWithAI";
import { useTranslation, formatDate, formatDuration } from "../../lib/i18n"
import { Popover } from "../../ui";
import { ScrollModeHint } from "./ScrollModeHint";

export function CycleColumn({
  active = true,
  cycle,
  onSelect,
  relations,
  onReviewIssues,
  onPlanWithAI,
  revealTask,
}: {
  revealTask?: {cycleId:string;taskId:string;requestId:number};
  active?: boolean;
  cycle: Cycle;
  onSelect?: () => void;
  relations?: RelationView;
  onReviewIssues?: (id: string) => void;
  onPlanWithAI?: (id: string) => void;
}) {
  const { t } = useTranslation("planning");
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

  const badge = badgeText(cycle, t);
  const meta = metaLine(cycle, today, t);
  const title = titleText(cycle, t);

  return (
    /* 列宽固定 512px（--spacing-plan），绝不因视口不足被压缩（spec: 横向并列
     * 的周期列）；列内内容自己纵向滚动。 */
    <section
      aria-label={`${badge} ${title}`}
      onPointerDown={onSelect}
      onFocusCapture={onSelect}
      className={`plan-card plan-enter flex shrink-0 overflow-hidden rounded-lg border border-frame bg-content ${isDay ? "w-[calc(var(--spacing-plan)+var(--spacing-panel))]" : "w-plan"}`}
    >
      <div className="plan-ai-scope flex min-h-0 w-plan shrink-0 flex-col">
      <header className="workspace-scroll-heading shrink-0 border-b border-light px-6 pb-5 pt-5">
        <div className="flex items-center justify-between gap-2">
          <span className="text-caption font-medium text-secondary">
            {badge}
            {state === "finished" && <span className="ml-2 text-hint">{t("cycle.ended")}</span>}
          </span>
          <div className="flex items-center gap-3">
            <ScrollModeHint />
            <CycleOptionsMenu cycle={cycle} runningElsewhere={false} />
          </div>
        </div>
        <h3 className="mt-1 text-section-title font-semibold text-primary">{title}</h3>
        <div className="mt-0.5 flex flex-wrap items-center gap-x-2 gap-y-1">
          {meta && <p className="text-caption text-hint">{meta}</p>}
          {cycle.type === "month" && <ProgressCheckDisclosure cycle={cycle} t={t} />}
        </div>
      </header>
      <TaskList layout="panel" revealTask={revealTask?.cycleId === cycle.id ? revealTask : undefined} active={active} cycleId={cycle.id} cycleType={cycle.type} locked={cycle.finished} relations={relations} onReviewIssues={() => onReviewIssues?.(cycle.id)}>
        {onPlanWithAI && <PlanWithAI active={active} cycle={cycle} onPlan={onPlanWithAI} />}
      </TaskList>
      </div>
      {isDay && <div className="flex min-h-0 w-panel shrink-0 flex-col border-l border-light bg-subtle/40">
        <FocusArea day={cycle} sessions={sessions} runningSessionId={runningSessionId} />
      </div>}
    </section>
  );
}

function progressCheckForCycle(cycle: Cycle): ProgressCheck | null {
  if (cycle.type !== "month" || !cycle.starts_on || !cycle.ends_on) return null;
  if (cycle.progress_check) return cycle.progress_check;
  const days = daysBetween(cycle.ends_on, cycle.starts_on);
  if (days === null || days <= 1) return null;
  // Keep old null rows aligned with the backend's historical midpoint rule.
  return { kind: "once", date: addDaysISO(cycle.starts_on, Math.floor(days / 2)) };
}

function ProgressCheckDisclosure({
  cycle,
  t,
}: {
  cycle: Cycle;
  t: (key: string, options?: Record<string, unknown>) => string;
}) {
  const anchorRef = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const check = progressCheckForCycle(cycle);
  if (!check || !cycle.starts_on || !cycle.ends_on) return null;
  const preview = deriveCustomTimeline(cycle.starts_on, cycle.ends_on, check);
  const shortRule = check.kind === "once"
    ? t("cycle.progressOnce", { date: formatDate(check.date, { month: "short", day: "numeric" }) })
    : check.every_days % 7 === 0
      ? t("cycle.progressRepeatWeeks", { count: check.every_days / 7 })
      : t("cycle.progressRepeatDays", { count: check.every_days });
  const rule = check.kind === "once"
    ? t("cycle.progressOnce", { date: formatDate(check.date, { year: "numeric", month: "short", day: "numeric" }) })
    : shortRule;
  const close = () => setOpen(false);
  return (
    <>
      <button
        ref={anchorRef}
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={`${t("cycle.progressCheck")}: ${shortRule}`}
        onClick={() => setOpen((value) => !value)}
        className="inline-flex h-6 max-w-full cursor-pointer items-center gap-1 rounded-sm px-1.5 text-caption text-secondary transition-colors duration-150 hover:bg-hover hover:text-primary focus-visible:outline-2 focus-visible:outline-focus"
      >
        <svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.35" aria-hidden="true">
          <circle cx="8" cy="8" r="5.75" />
          <path d="M8 4.8v3.5l2.2 1.35" strokeLinecap="round" />
        </svg>
        <span className="min-w-0 truncate">{shortRule}</span>
        <svg viewBox="0 0 16 16" width="12" height="12" fill="none" stroke="currentColor" strokeWidth="1.35" aria-hidden="true">
          <path d="m5 6.5 3 3 3-3" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </button>
      <Popover open={open} onClose={close} anchorRef={anchorRef} role="dialog" label={t("cycle.progressCheck")} className="w-[300px] p-3">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <p className="text-body font-medium text-primary">{t("cycle.progressCheck")}</p>
            <p className="mt-0.5 text-caption text-secondary">{rule}</p>
          </div>
          <button type="button" aria-label={t("cycle.dismiss")} onClick={close} className="flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-hint hover:bg-hover hover:text-primary focus-visible:outline-2 focus-visible:outline-focus">
            <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.35" aria-hidden="true"><path d="m4 4 8 8M12 4l-8 8" strokeLinecap="round" /></svg>
          </button>
        </div>
        <div className="mt-2 flex flex-wrap items-baseline gap-x-3 gap-y-1 text-caption text-secondary">
          {preview.days !== null && <span>{t("duration.customDays", { count: preview.days })}</span>}
          <span>{t("duration.checkCount", { count: preview.totalChecks })}</span>
        </div>
        {preview.totalChecks > 0 ? (
          <>
            <ul className="mt-2 flex flex-wrap gap-x-2 gap-y-1 text-caption text-hint">
              {preview.checkDates.map((date) => <li key={date}>{formatDate(date, { month: "short", day: "numeric" })}</li>)}
            </ul>
            {preview.totalChecks > preview.checkDates.length && <p className="mt-1 text-caption text-hint">{t("cycle.progressMore", { count: preview.totalChecks - preview.checkDates.length })}</p>}
          </>
        ) : (
          <p className="mt-2 text-caption text-hint">{t("cycle.progressNone")}</p>
        )}
      </Popover>
    </>
  );
}

function badgeText(cycle: Cycle, t: (key: string, options?: Record<string, unknown>) => string): string {
  if (cycle.type === "week" && cycle.starts_on) {
    const week = isoWeekNumber(cycle.starts_on);
    if (week !== null) return t("cycle.badge.week", { count: week });
  }
  if (cycle.type === "day" && cycle.starts_on) {
    return t("cycle.badge.day", { day: weekdayName(cycle.starts_on) });
  }
  return t({ month: "cycle.badge.longTerm", week: "cycle.badge.week", day: "cycle.badge.day", session: "cycle.badge.focus" }[cycle.type]);
}

function titleText(cycle: Cycle, t: (key: string, options?: Record<string, unknown>) => string): string {
  if (cycle.type === "month") return t("cycle.title.longTerm");
  if (cycle.type === "week") return t("cycle.title.week");
  if (cycle.type === "day") return t("cycle.title.day");
  return cycle.title;
}

/** 列头第二行：日期范围 + 剩余时间/编号 + 实际投入汇总（§3.2/§7）。 */
function metaLine(cycle: Cycle, today: string, t: (key: string, options?: Record<string, unknown>) => string): string | null {
  const parts: string[] = [];
  if (cycle.type === "month") {
    const range = formatDateRange(cycle.starts_on, cycle.ends_on);
    if (range) parts.push(range);
    const weeks = remainingWeeks(cycle.ends_on, today);
    if (weeks !== null) parts.push(weeks > 0 ? t("cycle.weeksLeft", { count: weeks }) : t("cycle.endedLabel"));
  } else if (cycle.type === "week") {
    const range = formatDateRange(cycle.starts_on, cycle.ends_on);
    if (range) parts.push(range);
  } else if (cycle.type === "day") {
    if (cycle.starts_on) parts.push(formatDate(cycle.starts_on, { year: "numeric", month: "short", day: "numeric" }));
  }
  if (cycle.focused_time > 0) parts.push(t("cycle.focused", { duration: formatDuration(cycle.focused_time) }));
  return parts.length > 0 ? parts.join(" · ") : null;
}

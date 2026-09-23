/**
 * 月网格与周网格共用的单元格（tasks.md §5.1：同一单元格渲染）。
 * 月视图保持摘要，周视图完整展示条目并随内容增高；
 * 空格子的一键创建、现有计划的改期入口都在这里。
 *
 * 弱化格（非当前月）点击跳转到那个月。
 */
import { useRef, useState, type CSSProperties, type KeyboardEvent, type MouseEvent } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { Checkbox, Popover, PopoverItem, cn } from "../../ui";
import type { CalendarDay } from "./api";
import { dayProgress } from "./calendar-model";
import { patchTask, type TaskNode } from "../../lib/ipc";
import { taskColor, type RelationView } from "../planner/relations";
import { errorMessage, invalidateTasks } from "../planner/actions";
import { useTranslation, formatDuration as formatLocalizedDuration, formatDate } from "../../lib/i18n";

export type DayCellDensity = "month" | "week";

export interface DayCellProps {
  day: CalendarDay;
  today: string;
  selected: boolean;
  density: DayCellDensity;
  tasks: TaskNode[];
  tasksLoading: boolean;
  tasksUnavailable: boolean;
  relations: RelationView;
  onSelect: (date: string) => void;
  onCreate: (date: string) => void;
  onOpenTask: (date: string, taskId: string) => void;
  onOpenSession: (date: string, sessionId: string) => void;
  onMoveDay: (dayId: string, date: string) => void;
  onDeleteDayPlan: (day: CalendarDay) => void;
  onDeleteFocusBlocks: (day: CalendarDay) => void;
  onClearSchedules: (day: CalendarDay) => void;
}

export function DayCell({
  day,
  today,
  selected,
  density,
  tasks,
  tasksLoading,
  tasksUnavailable,
  relations,
  onSelect,
  onCreate,
  onOpenTask,
  onOpenSession,
  onMoveDay,
  onDeleteDayPlan,
  onDeleteFocusBlocks,
  onClearSchedules,
}: DayCellProps) {
  const { t } = useTranslation("planning");
  const dayCycle = day.day_cycle;
  const displayDate = formatDate(day.date, { year: "numeric", month: "short", day: "numeric" });
  const { total, finished } = dayProgress(day);
  const confirmedTasks = tasks.filter((task) => !task.proposal);
  const completedTasks = confirmedTasks.filter((task) => task.completed).length;
  const proposalCount = tasks.length - confirmedTasks.length;
  const taskProgressLabel = tasksUnavailable ? t("calendar.tasksUnavailable") : tasksLoading ? t("calendar.loadingTasks")
    : proposalCount > 0 ? t("calendar.previewCount", { done: completedTasks, total: confirmedTasks.length, count: proposalCount })
    : t("calendar.tasksCount", { done: completedTasks, total: confirmedTasks.length });
  const isToday = day.date === today;
  const isWeek = density === "week";
  const qc = useQueryClient();
  const dateRef = useRef<HTMLButtonElement>(null);
  const [menuPoint, setMenuPoint] = useState<{ x: number; y: number } | null>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const openMenu = (event: MouseEvent<HTMLDivElement> | KeyboardEvent<HTMLDivElement>) => {
    if (!dayCycle && !day.in_range) return;
    event.preventDefault();
    if ("clientX" in event) setMenuPoint({ x: event.clientX, y: event.clientY });
    else {
      const rect = dateRef.current?.getBoundingClientRect();
      setMenuPoint(rect ? { x: rect.left, y: rect.bottom } : null);
    }
    setMenuOpen(true);
  };
  const chooseMenuAction = (action: () => void) => {
    setMenuOpen(false);
    queueMicrotask(action);
  };
  const completion = useMutation({
    mutationFn: ({ id, completed }: { id: string; completed: boolean }) => patchTask(id, { completed }),
    onSuccess: () => { if (dayCycle) invalidateTasks(qc, dayCycle.id); },
  });

  return (
    <div
      data-day-cell={day.date}
      data-calendar-menu={!!dayCycle || day.in_range || undefined}
      role="gridcell"
      aria-selected={selected || undefined}
      className={cn(
        "group/day flex min-w-0 flex-col rounded-lg border text-left transition-colors",
        isWeek ? "min-h-20 gap-2 p-3" : "min-h-28 gap-1 p-2",
        day.in_range ? "border-light bg-content" : "border-light/60 bg-subtle",
        selected && (isWeek ? "border-control/60 shadow-sm" : "border-focus"),
      )}
      onClick={() => onSelect(day.date)}
      onContextMenu={openMenu}
      onKeyDown={(event) => {
        if (event.key !== "ContextMenu" && !(event.key === "F10" && event.shiftKey)) return;
        openMenu(event);
      }}
    >
      <div className="flex items-center justify-between gap-1">
        <button
          ref={dateRef}
          type="button"
          aria-label={t("calendar.openDay", { date: displayDate })}
          aria-haspopup={dayCycle || day.in_range ? "menu" : undefined}
          aria-expanded={menuOpen || undefined}
          onClick={(event) => {
            event.stopPropagation();
            event.currentTarget.focus({ preventScroll: true });
            onSelect(day.date);
          }}
          className={cn(
            "rounded-sm font-medium focus-visible:outline-2 focus-visible:outline-focus",
            isWeek ? "flex h-7 min-w-7 items-center justify-center text-body" : "text-caption",
            day.in_range ? "text-primary" : "text-hint",
            (isToday || selected) && "bg-focus-surface text-focus",
          )}
        >
          {Number(day.date.slice(8, 10))}
        </button>
        {dayCycle && (isWeek || tasks.length > 0 || tasksLoading || tasksUnavailable || total === 0) && <span
          className={cn("shrink-0 whitespace-nowrap text-caption tabular-nums", day.in_range ? "text-secondary" : "text-hint")}
          data-day-progress={day.date}
          aria-label={taskProgressLabel}
          title={taskProgressLabel}
        >
          {tasksUnavailable ? "—" : tasksLoading ? "…" : `${completedTasks}/${confirmedTasks.length}`}
        </span>}
      </div>

      {dayCycle ? (
        <div className="flex min-h-0 flex-1 flex-col gap-1">
          <ul className="min-w-0 flex-1">
              {(density === "month" ? tasks.slice(0, 2) : tasks).map((task) => (
                <li key={task.id} className={cn("min-w-0", isWeek && "task-row flex items-start rounded-md", task.proposal && "bg-focus-surface/60")}
                  data-proposal={task.proposal ?? undefined}
                  data-related={isWeek && relations.highlighted.has(task.id) || undefined}
                  data-selected={isWeek && relations.selectedId === task.id || undefined}
                  style={{ "--task-color": taskColor(task, relations.tasks) ?? "var(--color-focus)" } as CSSProperties}
                  onClick={(event) => event.stopPropagation()}>
                  {isWeek && <Checkbox className="task-check min-w-6! shrink-0 justify-center"
                    aria-label={t("calendar.markComplete", { title: task.title })}
                    checked={completion.isPending && completion.variables.id === task.id ? completion.variables.completed : task.completed}
                    disabled={dayCycle.finished || completion.isPending || task.proposal != null}
                    onChange={(completed) => completion.mutate({ id: task.id, completed })} />}
                  <button type="button" title={task.title} aria-label={t("calendar.viewTask", { title: task.title })}
                    className={cn("flex min-w-0 flex-1 gap-2 rounded-sm px-1 py-1 text-left hover:bg-hover",
                      isWeek ? "items-start text-menu" : "items-center text-caption",
                      !isWeek && relations.highlighted.has(task.id) && "bg-focus-surface", task.completed ? "text-hint" : "text-primary")}
                    onClick={(event) => { event.stopPropagation(); onOpenTask(day.date, task.id); }}>
                    <span className={cn("task-color-slot shrink-0", density === "week" && "mt-0.5")} style={{ backgroundColor: taskColor(task, relations.tasks) ?? "transparent", borderColor: taskColor(task, relations.tasks) ?? undefined }} aria-hidden="true" />
                    <span className={cn("min-w-0 truncate", (task.completed || task.proposal === "delete") && "line-through")}>{task.title}{task.proposal && <span className="ml-2 inline-block text-[11px] text-secondary no-underline">{task.proposal === "delete" ? t("task.proposalDelete") : t("task.proposalPreview")}</span>}</span>
                  </button>
                </li>
              ))}
          </ul>
          {completion.isError && <p role="alert" className="text-caption text-danger">{errorMessage(completion.error)}</p>}
          {density === "month" && tasks.length > 2 && <span className="px-1 text-caption text-hint">{t("calendar.more", { count: tasks.length - 2 })}</span>}
          {!isWeek && total > 0 && <span
            data-focus-progress={day.date}
            role="img"
            aria-label={t("calendar.blocksDone", { done: finished, total, count: total })}
            title={t("calendar.blocksDone", { done: finished, total, count: total })}
            className="mt-1 inline-flex w-fit shrink-0 cursor-default items-center gap-1.5 whitespace-nowrap text-caption tabular-nums text-hint">
            <svg className="h-3.5 w-3.5 shrink-0" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></svg>
            <span aria-hidden="true">{finished}/{total}</span>
          </span>}
          {isWeek && total > 0 && <section aria-label={t("calendar.focusBlocksOn", { date: displayDate })} className="mt-3 border-t border-light pt-3">
            <header className="mb-1.5 flex items-center justify-between gap-2 text-caption text-secondary">
              <span className="font-medium">{t("focus.blocks")}</span><span>{t("calendar.done", { done: `${finished}/${total}` })}</span>
            </header>
            {day.sessions.map(({ session, schedule }) => <button key={session.id} type="button" aria-label={t("calendar.openFocus", { title: session.title })}
              onClick={(event) => { event.stopPropagation(); onOpenSession(day.date, session.id); }}
              className="mt-1 flex w-full items-start gap-2 rounded-md px-2 py-2 text-left transition-colors hover:bg-hover focus-visible:bg-hover">
              <svg className="mt-0.5 h-4 w-4 shrink-0 text-secondary" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></svg>
              <span className="min-w-0 flex-1"><span className={cn("block break-words text-menu", session.finished && "text-hint line-through")}>{session.title}</span>
                <span className="mt-0.5 block text-caption text-hint">{schedule ? formatDate(schedule.starts_at, { hour: "2-digit", minute: "2-digit" }) : t("calendar.unscheduled")}{session.duration ? ` · ${formatLocalizedDuration(session.duration)}` : ""}</span>
              </span>
              <span className="text-hint" aria-hidden="true">›</span>
            </button>)}
          </section>}
        </div>
      ) : (
        // 空格子一键创建当日计划（tasks.md §5.3，走 ensureDay 带日期）。
        <button
          type="button"
          aria-label={t("calendar.createDayFor", { date: displayDate })}
          disabled={!day.in_range}
          onClick={(event) => {
            event.stopPropagation();
            onCreate(day.date);
          }}
          className={cn(
            "mt-1 flex min-h-8 items-center justify-center rounded-md px-1 text-caption transition-colors",
            day.in_range ? "text-hint hover:bg-hover hover:text-secondary" : "cursor-not-allowed text-hint opacity-50",
          )}
        >
          + {t("calendar.createDay")}
        </button>
      )}
      <Popover open={menuOpen} onClose={() => setMenuOpen(false)} anchorRef={dateRef} point={menuPoint ?? undefined}
        label={t("calendar.dayActions", { date: displayDate })}>
        {dayCycle ? <>
          <PopoverItem onSelect={() => chooseMenuAction(() => onMoveDay(dayCycle.id, day.date))}>{t("calendar.moveDayPlan")}</PopoverItem>
          <PopoverItem className="border-t border-light text-danger" onSelect={() => chooseMenuAction(() => onDeleteDayPlan(day))}>{t("calendar.deleteDayPlan")}</PopoverItem>
          <PopoverItem className="text-danger" disabled={day.sessions.length === 0} onSelect={() => chooseMenuAction(() => onDeleteFocusBlocks(day))}>{t("calendar.deleteDayFocusBlocks")}</PopoverItem>
          <PopoverItem className="text-danger" disabled={!day.sessions.some(({ schedule }) => schedule !== null)} onSelect={() => chooseMenuAction(() => onClearSchedules(day))}>{t("calendar.clearDaySchedules")}</PopoverItem>
        </> : <PopoverItem onSelect={() => chooseMenuAction(() => onCreate(day.date))}>{t("calendar.createDay")}</PopoverItem>}
      </Popover>
    </div>
  );
}

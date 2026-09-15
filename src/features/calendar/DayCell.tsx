/**
 * 月网格与周网格共用的单元格（tasks.md §5.1：同一单元格渲染）。
 * 月视图保持摘要，周视图完整展示条目并随内容增高；
 * 空格子的一键创建、拖拽起手与落点判断都在这里。
 *
 * 弱化格（非当前月）：不可作为落点（onDrop 会被上层忽略），点击则跳转
 * 到那个月（spec：弱化样式显示且不可作为默认落点，但可点击跳转）。
 */
import type { CSSProperties, DragEvent } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { Checkbox, cn } from "../../ui";
import type { CalendarDay } from "./api";
import { dayProgress } from "./calendar-model";
import { patchTask, type TaskNode } from "../../lib/ipc";
import { taskColor, type RelationView } from "../planner/relations";
import { errorMessage, invalidateTasks } from "../planner/actions";
import { formatDuration } from "../planner/dates";
import { createDragToken, DAY_DRAG_TYPE, writeDragToken } from "./calendar-dnd";

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
  onDayDragStart: (dayId: string, date: string, token: string) => void;
  onDayDragEnd: () => void;
  onDragOverCell: (event: DragEvent<HTMLDivElement>) => void;
  onDropOnCell: (date: string, event: DragEvent<HTMLDivElement>) => void;
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
  onDayDragStart,
  onDayDragEnd,
  onDragOverCell,
  onDropOnCell,
}: DayCellProps) {
  const dayCycle = day.day_cycle;
  const { total, finished } = dayProgress(day);
  const isToday = day.date === today;
  const isWeek = density === "week";
  const qc = useQueryClient();
  const completion = useMutation({
    mutationFn: ({ id, completed }: { id: string; completed: boolean }) => patchTask(id, { completed }),
    onSuccess: () => { if (dayCycle) invalidateTasks(qc, dayCycle.id); },
  });

  return (
    <div
      data-day-cell={day.date}
      role="gridcell"
      aria-selected={selected || undefined}
      onDragOver={(event) => {
        if (!day.in_range) return; // 弱化格不是落点
        onDragOverCell(event);
      }}
      onDrop={(event) => onDropOnCell(day.date, event)}
      className={cn(
        "group/day flex min-w-0 flex-col rounded-lg border text-left transition-colors",
        isWeek ? "min-h-20 gap-2 p-3" : "min-h-28 gap-1 p-2",
        day.in_range ? "border-light bg-content" : "border-light/60 bg-subtle",
        selected && (isWeek ? "border-control/60 shadow-sm" : "border-focus"),
      )}
      onClick={() => onSelect(day.date)}
    >
      <div className="flex items-center justify-between gap-1">
        <button
          type="button"
          aria-label={`Open ${day.date}`}
          onClick={(event) => {
            event.stopPropagation();
            onSelect(day.date);
          }}
          className={cn(
            "rounded-sm font-medium",
            isWeek ? "flex h-7 min-w-7 items-center justify-center text-body" : "text-caption",
            day.in_range ? "text-primary" : "text-hint",
            (isToday || selected) && "bg-focus-surface text-focus",
          )}
        >
          {Number(day.date.slice(8, 10))}
        </button>
        {dayCycle && (
          // The grip always means moving the day; block counts are not a drag handle.
          <span className="flex items-center gap-1.5">
          {!isWeek && total > 0 && <span className="cursor-default text-caption text-hint">{total} {total === 1 ? "block" : "blocks"}</span>}
          <span
            draggable={Boolean(dayCycle)}
            aria-label={`Move ${day.date}`}
            title={`Move ${day.date}`}
            onDragStart={(event) => {
              if (!dayCycle) return;
              const token = createDragToken();
              writeDragToken(event.dataTransfer, DAY_DRAG_TYPE, token);
              onDayDragStart(dayCycle.id, day.date, token);
            }}
            onDragEnd={onDayDragEnd}
            className={cn(
              "cursor-grab select-none rounded-sm p-1 text-caption text-hint opacity-40 transition-opacity hover:bg-hover group-hover/day:opacity-100 group-focus-within/day:opacity-100",
            )}
          >
            <span className="pointer-events-none">
              <svg className="h-4 w-4" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M5 3h2v2H5zm4 0h2v2H9zM5 7h2v2H5zm4 0h2v2H9zM5 11h2v2H5zm4 0h2v2H9z" /></svg>
            </span>
          </span>
          </span>
        )}
      </div>

      {dayCycle ? (
        <div className="flex min-h-0 flex-1 flex-col gap-1">
          <span
            className={cn("text-caption", day.in_range ? "text-secondary" : "text-hint")}
            data-day-progress={day.date}
          >
            {tasksUnavailable ? "Tasks unavailable" : tasksLoading ? "Loading tasks…" : `${tasks.filter((task) => task.completed && !task.proposal).length}/${tasks.filter((task) => !task.proposal).length} tasks${tasks.some(task => task.proposal) ? ` · ${tasks.filter(task => task.proposal).length} 预览` : ""}`}
          </span>
          <ul className="min-w-0 flex-1">
              {(density === "month" ? tasks.slice(0, 2) : tasks).map((task) => (
                <li key={task.id} className={cn("min-w-0", isWeek && "task-row flex items-start rounded-md", task.proposal && "bg-focus-surface/60")}
                  data-proposal={task.proposal ?? undefined}
                  data-related={isWeek && relations.highlighted.has(task.id) || undefined}
                  data-selected={isWeek && relations.selectedId === task.id || undefined}
                  style={{ "--task-color": taskColor(task, relations.tasks) ?? "var(--color-focus)" } as CSSProperties}
                  onClick={(event) => event.stopPropagation()}>
                  {isWeek && <Checkbox className="task-check min-w-6! shrink-0 justify-center"
                    aria-label={`Mark “${task.title}” complete in calendar`}
                    checked={completion.isPending && completion.variables.id === task.id ? completion.variables.completed : task.completed}
                    disabled={dayCycle.finished || completion.isPending || task.proposal != null}
                    onChange={(completed) => completion.mutate({ id: task.id, completed })} />}
                  <button type="button" title={task.title} aria-label={`View task ${task.title}`}
                    className={cn("flex min-w-0 flex-1 gap-2 rounded-sm px-1 py-1 text-left hover:bg-hover",
                      isWeek ? "items-start text-menu" : "items-center text-caption",
                      !isWeek && relations.highlighted.has(task.id) && "bg-focus-surface", task.completed ? "text-hint" : "text-primary")}
                    onClick={(event) => { event.stopPropagation(); onOpenTask(day.date, task.id); }}>
                    <span className={cn("task-color-slot shrink-0", density === "week" && "mt-0.5")} style={{ backgroundColor: taskColor(task, relations.tasks) ?? "transparent", borderColor: taskColor(task, relations.tasks) ?? undefined }} aria-hidden="true" />
                    <span className={cn("min-w-0", density === "week" ? "break-words" : "truncate", (task.completed || task.proposal === "delete") && "line-through")}>{task.title}{task.proposal && <span className="ml-2 inline-block text-[11px] text-secondary no-underline">{task.proposal === "delete" ? "待删除" : "预览"}</span>}</span>
                  </button>
                </li>
              ))}
          </ul>
          {completion.isError && <p role="alert" className="text-caption text-danger">{errorMessage(completion.error)}</p>}
          {density === "month" && tasks.length > 2 && <span className="px-1 text-caption text-hint">+{tasks.length - 2} more</span>}
          {!isWeek && total > 0 && <span className="mt-1 text-caption text-hint">{finished}/{total} blocks done</span>}
          {isWeek && total > 0 && <section aria-label={`Focus blocks on ${day.date}`} className="mt-3 border-t border-light pt-3">
            <header className="mb-1.5 flex items-center justify-between gap-2 text-caption text-secondary">
              <span className="font-medium">Focus blocks</span><span>{finished}/{total} done</span>
            </header>
            {day.sessions.map(({ session, schedule }) => <button key={session.id} type="button" aria-label={`Open focus block ${session.title}`}
              onClick={(event) => { event.stopPropagation(); onOpenSession(day.date, session.id); }}
              className="mt-1 flex w-full items-start gap-2 rounded-md px-2 py-2 text-left transition-colors hover:bg-hover focus-visible:bg-hover">
              <svg className="mt-0.5 h-4 w-4 shrink-0 text-secondary" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/></svg>
              <span className="min-w-0 flex-1"><span className={cn("block break-words text-menu", session.finished && "text-hint line-through")}>{session.title}</span>
                <span className="mt-0.5 block text-caption text-hint">{schedule ? new Date(schedule.starts_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : "Unscheduled"}{session.duration ? ` · ${formatDuration(session.duration)}` : ""}</span>
              </span>
              <span className="text-hint" aria-hidden="true">›</span>
            </button>)}
          </section>}
        </div>
      ) : (
        // 空格子一键创建当日计划（tasks.md §5.3，走 ensureDay 带日期）。
        <button
          type="button"
          aria-label={`Create day plan for ${day.date}`}
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
          + Day plan
        </button>
      )}
    </div>
  );
}

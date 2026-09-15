/**
 * 月网格与周网格共用的单元格（tasks.md §5.1：同一单元格渲染）。
 * 密度只影响排版类名，不换组件：格子摘要（专注块数 + 完成进度）、
 * 空格子的一键创建、拖拽起手与落点判断都在这里。
 *
 * 弱化格（非当前月）：不可作为落点（onDrop 会被上层忽略），点击则跳转
 * 到那个月（spec：弱化样式显示且不可作为默认落点，但可点击跳转）。
 */
import type { DragEvent } from "react";

import { cn } from "../../ui";
import type { CalendarDay } from "./api";
import { dayProgress } from "./calendar-model";

export type DayCellDensity = "month" | "week";

export interface DayCellProps {
  day: CalendarDay;
  today: string;
  selected: boolean;
  density: DayCellDensity;
  onSelect: (date: string) => void;
  onCreate: (date: string) => void;
  onDayDragStart: (dayId: string, date: string) => void;
  onDragOverCell: (event: DragEvent<HTMLDivElement>) => void;
  onDropOnCell: (date: string, event: DragEvent<HTMLDivElement>) => void;
}

export function DayCell({
  day,
  today,
  selected,
  density,
  onSelect,
  onCreate,
  onDayDragStart,
  onDragOverCell,
  onDropOnCell,
}: DayCellProps) {
  const dayCycle = day.day_cycle;
  const { total, finished } = dayProgress(day);
  const isToday = day.date === today;

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
        "flex flex-col gap-1 rounded-sm border p-1.5 text-left",
        day.in_range ? "border-light bg-content" : "border-light/60 bg-subtle",
        selected && "border-focus ring-1 ring-focus",
        dayCycle && "cursor-pointer hover:bg-hover",
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
            "text-caption font-medium",
            day.in_range ? "text-primary" : "text-hint",
            isToday && "text-focus",
          )}
        >
          {Number(day.date.slice(8, 10))}
        </button>
        {dayCycle && (
          // 拖拽起手：整格摘要都可拖（HTML5 DnD，与工作台任务行一致）。
          <span
            draggable
            title={`Move ${day.date}`}
            onDragStart={(event) => {
              event.dataTransfer.setData("text/plain", dayCycle.id);
              event.dataTransfer.effectAllowed = "move";
              onDayDragStart(dayCycle.id, day.date);
            }}
            className={cn(
              "cursor-grab rounded-full px-1.5 text-caption",
              total > 0 ? "bg-focus-surface text-focus" : "bg-subtle text-hint",
            )}
          >
            {total} blocks
          </span>
        )}
      </div>

      {dayCycle ? (
        <div className="flex min-h-0 flex-1 flex-col gap-1">
          <span
            className={cn("text-caption", day.in_range ? "text-secondary" : "text-hint")}
            data-day-progress={day.date}
          >
            {finished}/{total} done
          </span>
          {density === "week" && (
            // 周格有空间显示条目摘要与时间轴入口（spec：周网格）。
            <ul className="min-h-0 flex-1 overflow-hidden">
              {day.sessions.slice(0, 4).map(({ session }) => (
                <li
                  key={session.id}
                  className={cn(
                    "truncate text-caption",
                    session.finished ? "text-hint line-through" : "text-primary",
                  )}
                >
                  {session.title}
                </li>
              ))}
            </ul>
          )}
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
            "flex flex-1 items-center justify-center rounded-sm border border-dashed border-control text-caption",
            day.in_range ? "text-secondary hover:bg-hover" : "cursor-not-allowed text-hint opacity-50",
          )}
        >
          + Day plan
        </button>
      )}
    </div>
  );
}

/**
 * 日历视图（change: add-calendar-time-view §5）：月/周两种密度的网格共用
 * DayCell 渲染，单日时间轴 + 待排区 + 时间预算条在右侧面板。与工作台是
 * 同一份数据的两个投影——本组件不维护任何独立副本，写入后靠 cycles:changed
 * 事件与主动失效回到数据库真相（spec：日历与层级的双向一致）。
 *
 * 拖拽策略沿用工作台的乐观更新（TaskList.applyReorder）：拖到空日期先改
 * react-query 缓存快照，请求失败整体回滚并提示；目标已被占用则弹策略
 * 弹窗（merge / swap），由用户显式选择，绝不静默覆盖。
 *
 * 顶栏视图切换由协调者接线（App 窗口栏）；本组件内部提供密度切换并把
 * 偏好写入 localStorage（key：planner.preferred-view，spec：记住偏好），
 * 并在提供 onClose 时渲染关闭按钮。
 */
import { useEffect, useMemo, useState } from "react";
import type { DragEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { Button, cn } from "../../ui";
import { ensureDay } from "../../lib/ipc";
import { qk } from "../../lib/events";
import { addDaysISO, todayISO } from "../planner/dates";
import { errorMessage, useActionError } from "../planner/actions";
import {
  addMonthsISO,
  loadPreferredView,
  monthBounds,
  overlapIds,
  savePreferredView,
  weekBounds,
  applyMoveToRange,
  type CalendarViewMode,
} from "./calendar-model";
import { DayCell } from "./DayCell";
import { DayTimeline } from "./DayTimeline";
import { StrategyDialog, type StrategyChoice } from "./StrategyDialog";
import {
  getCalendarRange,
  getScheduleOverlaps,
  getTimeBudget,
  moveDayCycle,
  setSessionSchedule,
  type CalendarRange,
} from "./api";

/** 本视图自己的查询键前缀；lib/events 的事件矩阵不认识它们，所以
 * CalendarView 自己订阅 cycles:changed / tasks:changed 做失效。 */
const RANGE_KEY = "calendar-range";
const BUDGET_KEY = "time-budget";
const OVERLAPS_KEY = "schedule-overlaps";

function invalidateCalendar(qc: ReturnType<typeof useQueryClient>): void {
  void qc.invalidateQueries({ queryKey: [RANGE_KEY] });
  void qc.invalidateQueries({ queryKey: [BUDGET_KEY] });
  void qc.invalidateQueries({ queryKey: [OVERLAPS_KEY] });
}

const WEEKDAY_LABELS = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

export function CalendarView({ onClose }: { onClose?: () => void }) {
  const qc = useQueryClient();
  const [view, setView] = useState<CalendarViewMode>(() => loadPreferredView());
  const [anchor, setAnchor] = useState<string>(() => todayISO());
  const [selectedDate, setSelectedDate] = useState<string | null>(null);
  const [dragging, setDragging] = useState<{ dayId: string; date: string } | null>(null);
  const [strategyChoice, setStrategyChoice] = useState<StrategyChoice | null>(null);
  const { error, run, fail, dismiss } = useActionError();

  const bounds = view === "month" ? monthBounds(anchor) : weekBounds(anchor);
  const rangeKey = useMemo(() => [RANGE_KEY, bounds.start, bounds.end] as const, [bounds.start, bounds.end]);
  const { data: range, isLoading } = useQuery({
    queryKey: rangeKey,
    queryFn: () => getCalendarRange(bounds.start, bounds.end),
  });

  // 后端事件 → 失效本视图查询（工作台由 lib/events 负责，两边共享数据库）。
  useEffect(() => {
    let disposed = false;
    const unlisteners: UnlistenFn[] = [];
    const wire = (event: string) =>
      listen(event, () => invalidateCalendar(qc))
        .then((unlisten) => {
          if (disposed) unlisten();
          else unlisteners.push(unlisten);
        })
        .catch(() => {
          // 无事件环境（测试、dev 报错）下停止自刷新，查询仍可用。
        });
    void wire("cycles:changed");
    void wire("tasks:changed");
    return () => {
      disposed = true;
      for (const unlisten of unlisteners) unlisten();
    };
  }, [qc]);

  const days = range?.days ?? [];
  const today = todayISO();

  const switchView = (next: CalendarViewMode) => {
    setView(next);
    savePreferredView(next); // spec：preferred_view 记忆
    // 周/月切换共享 anchor：聚焦的日期不动，只有网格密度变化。
  };

  const navigate = (delta: number) => {
    setAnchor((current) =>
      view === "month" ? addMonthsISO(current, delta) : addDaysISO(current, delta * 7),
    );
  };

  const selectDay = (date: string) => {
    const cell = days.find((d) => d.date === date);
    if (cell && !cell.in_range) {
      // 弱化格（非当前月）可点击跳转，而不是选中。
      setAnchor(date);
      return;
    }
    setSelectedDate(date);
    dismiss();
  };

  const createDay = (date: string) => {
    void run(async () => {
      await ensureDay(date); // 空格子一键创建当日计划（带日期）
      invalidateCalendar(qc);
    });
  };

  // 拖到空日期：乐观更新（快照 → 改缓存 → 失败回滚），沿用工作台策略。
  const onDropOnCell = (targetDate: string, event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    const source = dragging;
    setDragging(null);
    if (!source || source.date === targetDate) return;
    const targetCell = days.find((d) => d.date === targetDate);
    if (!targetCell?.in_range) return; // 弱化格不可作为落点
    const sourceCell = days.find((d) => d.day_cycle?.id === source.dayId);
    if (!sourceCell?.day_cycle) return;

    if (targetCell.day_cycle) {
      // 已占用：让用户显式选择，绝不静默处理。
      setStrategyChoice({ sourceDayId: source.dayId, sourceDate: sourceCell.date, targetDate });
      return;
    }

    const snapshot = qc.getQueryData<CalendarRange>(rangeKey) ?? null;
    if (snapshot) {
      qc.setQueryData<CalendarRange>(rangeKey, applyMoveToRange(snapshot, source.dayId, targetDate));
    }
    moveDayCycle(source.dayId, targetDate, null).then(
      () => {
        invalidateCalendar(qc);
        // move 会新建目标日周期（id 变化），工作台列也要刷新。
        void qc.invalidateQueries({ queryKey: qk.plannerState() });
      },
      (e: unknown) => {
        if (snapshot) qc.setQueryData<CalendarRange>(rangeKey, snapshot);
        fail(errorMessage(e));
      },
    );
  };

  const pickStrategy = (strategy: "merge" | "swap") => {
    const choice = strategyChoice;
    setStrategyChoice(null);
    if (!choice) return;
    void run(async () => {
      await moveDayCycle(choice.sourceDayId, choice.targetDate, strategy);
      invalidateCalendar(qc);
      void qc.invalidateQueries({ queryKey: qk.plannerState() });
    });
  };

  const selectedDay = selectedDate === null ? undefined : days.find((d) => d.date === selectedDate);
  const selectedDayCycleId = selectedDay?.day_cycle?.id ?? null;

  const { data: budget } = useQuery({
    queryKey: [BUDGET_KEY, selectedDate],
    queryFn: () => getTimeBudget(selectedDate!),
    enabled: selectedDate !== null,
  });
  const { data: overlaps } = useQuery({
    queryKey: [OVERLAPS_KEY, selectedDayCycleId],
    queryFn: () => getScheduleOverlaps(selectedDayCycleId!),
    enabled: selectedDayCycleId !== null,
  });

  const scheduleSession = (sessionId: string, startsAt: number, durationMs: number | null) => {
    void run(async () => {
      await setSessionSchedule(sessionId, startsAt, durationMs);
      invalidateCalendar(qc);
    });
  };

  return (
    <div className="flex h-full min-h-0 flex-col" data-calendar-view={view}>
      {/* 视图工具条：密度切换 + 期间导航 + 偏好记忆；顶栏入口由 App 接线。 */}
      <div className="flex shrink-0 items-center gap-2 border-b border-light bg-canvas px-4 py-2">
        <div role="tablist" aria-label="Calendar density">
          {(["month", "week"] as const).map((mode) => (
            <button
              key={mode}
              type="button"
              role="tab"
              aria-selected={view === mode}
              onClick={() => switchView(mode)}
              className={cn(
                "rounded-sm px-2 text-menu font-medium capitalize",
                view === mode ? "bg-focus-surface text-focus" : "text-primary hover:bg-hover",
              )}
            >
              {mode}
            </button>
          ))}
        </div>
        <Button size="compact" variant="ghost" onClick={() => navigate(-1)} aria-label="Previous">
          ‹
        </Button>
        <Button size="compact" variant="ghost" onClick={() => setAnchor(today)} aria-label="Jump to today">
          Today
        </Button>
        <Button size="compact" variant="ghost" onClick={() => navigate(1)} aria-label="Next">
          ›
        </Button>
        <span className="text-menu text-secondary" data-testid="visible-range">
          {bounds.start === bounds.end ? bounds.start : `${bounds.start} – ${bounds.end}`}
        </span>
        <div className="min-w-4 flex-1" />
        {onClose && (
          <Button size="compact" variant="ghost" onClick={onClose} aria-label="Close calendar view">
            Close
          </Button>
        )}
      </div>

      <div className="flex min-h-0 flex-1">
        {/* 网格：月/周共用 DayCell；左列是工作日表头。 */}
        <div className="min-h-0 flex-1 overflow-y-auto p-4">
          {isLoading ? (
            <p className="p-4 text-body text-hint">Loading calendar…</p>
          ) : (
            <div
              role="grid"
              aria-label={`Calendar ${view} view`}
              className={cn(
                "grid gap-1",
                view === "month" ? "grid-cols-7" : "grid-cols-1",
              )}
            >
              {view === "month" &&
                WEEKDAY_LABELS.map((label) => (
                  <div key={label} role="columnheader" className="text-caption font-medium text-hint">
                    {label}
                  </div>
                ))}
              {view === "week" ? (
                <div role="row" className="col-span-1 grid grid-cols-7 gap-1">
                  {WEEKDAY_LABELS.map((label) => (
                    <div key={label} className="text-caption font-medium text-hint">
                      {label}
                    </div>
                  ))}
                </div>
              ) : null}
              {view === "month"
                ? days.map((day) => (
                    <DayCell
                      key={day.date}
                      day={day}
                      today={today}
                      selected={day.date === selectedDate}
                      density="month"
                      onSelect={selectDay}
                      onCreate={createDay}
                      onDayDragStart={(dayId, date) => setDragging({ dayId, date })}
                      onDragOverCell={(event) => event.preventDefault()}
                      onDropOnCell={onDropOnCell}
                    />
                  ))
                : // 周密度：7 行大格（同一 DayCell 组件，仅排版不同）。
                  days.map((day) => (
                    <div key={day.date} role="row" className="grid grid-cols-1">
                      <DayCell
                        day={day}
                        today={today}
                        selected={day.date === selectedDate}
                        density="week"
                        onSelect={selectDay}
                        onCreate={createDay}
                        onDayDragStart={(dayId, date) => setDragging({ dayId, date })}
                        onDragOverCell={(event) => event.preventDefault()}
                        onDropOnCell={onDropOnCell}
                      />
                    </div>
                  ))}
            </div>
          )}
        </div>

        {/* 单日面板：时间轴 + 待排区 + 预算条。 */}
        <aside className="flex w-[360px] shrink-0 flex-col gap-3 border-l border-light bg-content p-4">
          {selectedDate === null ? (
            <p className="text-caption text-hint">
              Pick a day to see its timeline, unscheduled blocks and time budget.
            </p>
          ) : (
            <DayTimeline
              date={selectedDate}
              day={selectedDay}
              budget={budget ?? null}
              overlaps={overlapIds(overlaps ?? [])}
              onCreateDay={createDay}
              onSchedule={scheduleSession}
            />
          )}
        </aside>
      </div>

      <StrategyDialog
        choice={strategyChoice}
        onClose={() => setStrategyChoice(null)}
        onPick={pickStrategy}
      />
      {error && (
        <div
          role="alert"
          className="absolute bottom-4 left-1/2 z-40 -translate-x-1/2 rounded-sm border border-light bg-content px-3 py-2 text-caption text-danger shadow-[0_8px_24px_rgba(0,0,0,0.12)]"
        >
          {error}
          <button type="button" className="ml-2 underline" onClick={dismiss}>
            Dismiss
          </button>
        </div>
      )}
    </div>
  );
}

export default CalendarView;

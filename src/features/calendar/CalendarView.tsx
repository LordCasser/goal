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
 * 密度偏好写入 localStorage（key：planner.calendar-density，spec：记住偏好），
 * 并在提供 onClose 时渲染关闭按钮。
 */
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { DragEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { Button, cn } from "../../ui";
import { ensureDay, getPlannerState, getEditorWorkspacesByCycleIds } from "../../lib/ipc";
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
  calendarPlanCycleIds,
  calendarTasks,
  type CalendarViewMode,
} from "./calendar-model";
import { DayCell } from "./DayCell";
import { DayTimeline } from "./DayTimeline";
import { DAY_DRAG_TYPE, readDragToken } from "./calendar-dnd";
import { CalendarPlan } from "./CalendarPlan";
import { highlightedTasks, indexTasks, type RelationView } from "../planner/relations";
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

export function CalendarView({ active = true, onClose, onActiveCycleChange, onReviewIssues, onPlanWithAI }: { active?: boolean; onClose?: () => void; onActiveCycleChange?: (id: string) => void; onReviewIssues?: (id: string) => void; onPlanWithAI?: (id: string) => void }) {
  const qc = useQueryClient();
  const [view, setView] = useState<CalendarViewMode>(() => loadPreferredView());
  const [anchor, setAnchor] = useState<string>(() => todayISO());
  const [selectedDate, setSelectedDate] = useState<string>(() => todayISO());
  const [detail, setDetail] = useState<"plan" | "schedule">("plan");
  const [selectedTask, setSelectedTask] = useState<string | null>(null);
  const [hoveredTask, setHoveredTask] = useState<string | null>(null);
  const [dragging, setDragging] = useState<{ dayId: string; date: string; token: string } | null>(null);
  const [strategyChoice, setStrategyChoice] = useState<StrategyChoice | null>(null);
  const datesViewport = useRef<HTMLDivElement>(null);
  const detailsRef = useRef<HTMLElement>(null);
  const [detailTarget, setDetailTarget] = useState<{ kind: "task" | "session"; id: string } | null>(null);
  const [weekScrollTarget, setWeekScrollTarget] = useState<{ date: string; animate: boolean } | null>(() => ({ date: todayISO(), animate: false }));
  const { error, run, fail, dismiss } = useActionError();

  const bounds = view === "month" ? monthBounds(anchor) : weekBounds(anchor);
  const rangeKey = useMemo(() => [RANGE_KEY, bounds.start, bounds.end] as const, [bounds.start, bounds.end]);
  const { data: range, isLoading } = useQuery({
    queryKey: rangeKey,
    queryFn: () => getCalendarRange(bounds.start, bounds.end),
  });
  const { data: planner } = useQuery({ queryKey: qk.plannerState(), queryFn: getPlannerState });
  const cycles = planner?.cycles ?? [];
  const planCycleIds = calendarPlanCycleIds(cycles, range?.grid_start ?? bounds.start, range?.grid_end ?? bounds.end);
  const plans = useQuery({
    queryKey: qk.editorWorkspaces(planCycleIds),
    queryFn: () => getEditorWorkspacesByCycleIds(planCycleIds),
    enabled: planCycleIds.length > 0,
  });
  const workspaces = plans.data ?? {};
  const graph = useMemo(() => indexTasks(Object.values(plans.data ?? {}).map((workspace) => workspace.tasks)), [plans.data]);
  const cycleMap = new Map(cycles.map((cycle) => [cycle.id, cycle]));
  const relations: RelationView = { tasks: graph, cycles: cycleMap, selectedId: selectedTask,
    highlighted: highlightedTasks(selectedTask ?? hoveredTask, graph, cycleMap), select: setSelectedTask,
    preview: setHoveredTask, setDragging: () => {} };

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

  useEffect(() => {
    const cancelDrag = (event: KeyboardEvent) => {
      if (event.key === "Escape") setDragging(null);
    };
    window.addEventListener("keydown", cancelDrag);
    return () => window.removeEventListener("keydown", cancelDrag);
  }, []);

  const days = range?.days ?? [];
  const today = todayISO();

  // Center only on entry or explicit date navigation. Edits, query refreshes and
  // returning from Workspace must not pull the user away from a manual scroll.
  useLayoutEffect(() => {
    if (!active || view !== "week" || isLoading || !weekScrollTarget) return;
    const viewport = datesViewport.current;
    const cell = viewport?.querySelector<HTMLElement>(`[data-day-cell="${weekScrollTarget.date}"]`);
    if (!viewport?.clientWidth || !cell) return;
    const cellRect = cell.getBoundingClientRect();
    const left = viewport.scrollLeft + cellRect.left - viewport.getBoundingClientRect().left
      + cellRect.width / 2 - viewport.clientWidth / 2;
    const reducedMotion = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
    // The browser clamps to the real content edges: no empty space just to
    // center Monday or Sunday, and neighboring dates stay visible.
    viewport.scrollTo({ left, behavior: weekScrollTarget.animate && !reducedMotion ? "smooth" : "instant" });
    setWeekScrollTarget(null);
  }, [active, view, isLoading, weekScrollTarget]);

  const switchView = (next: CalendarViewMode) => {
    if (next === "week" && view !== "week") setWeekScrollTarget({ date: anchor, animate: false });
    setView(next);
    savePreferredView(next); // spec：preferred_view 记忆
    // 周/月切换共享 anchor：聚焦的日期不动，只有网格密度变化。
  };

  const navigate = (delta: number) => {
    const next = view === "month" ? addMonthsISO(anchor, delta) : addDaysISO(anchor, delta * 7);
    setAnchor(next);
    setSelectedDate(next);
    setSelectedTask(null);
    setWeekScrollTarget({ date: next, animate: false });
  };

  const selectDay = (date: string) => {
    const cell = days.find((d) => d.date === date);
    if (cell && !cell.in_range) {
      // 弱化格（非当前月）可点击跳转，而不是选中。
      setAnchor(date);
    }
    setSelectedDate(date);
    setAnchor(date);
    setDetail("plan");
    setSelectedTask(null);
    if (cell?.day_cycle) onActiveCycleChange?.(cell.day_cycle.id);
    dismiss();
  };

  const createDay = (date: string) => {
    void run(async () => {
      const created = await ensureDay(date); // 空格子一键创建当日计划（带日期）
      setSelectedDate(date);
      setDetail("plan");
      onActiveCycleChange?.(created.id);
      void qc.invalidateQueries({ queryKey: qk.plannerState() });
      invalidateCalendar(qc);
    });
  };

  const openTask = (date: string, id: string) => {
    selectDay(date);
    setSelectedTask(id);
    setDetailTarget({ kind: "task", id });
  };
  const openSession = (date: string, id: string) => {
    selectDay(date);
    setDetail("schedule");
    setDetailTarget({ kind: "session", id });
  };
  useEffect(() => {
    if (!active || !detailTarget) return;
    const attr = detailTarget.kind === "task" ? "data-task-id" : "data-focus-block";
    const row = detailsRef.current?.querySelector<HTMLElement>(`[${attr}="${detailTarget.id}"]`);
    if (!row) return;
    row.scrollIntoView?.({ block: "nearest", inline: "nearest", behavior: window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth" });
    (row.querySelector<HTMLElement>("textarea") ?? row).focus({ preventScroll: true });
    setDetailTarget(null);
  }, [active, detailTarget, plans.data, range]);

  // 拖到空日期：乐观更新（快照 → 改缓存 → 失败回滚），沿用工作台策略。
  const onDropOnCell = (targetDate: string, event: DragEvent<HTMLDivElement>) => {
    event.preventDefault();
    const source = dragging;
    const token = readDragToken(event.dataTransfer, DAY_DRAG_TYPE);
    setDragging(null);
    if (!source || token !== source.token || source.date === targetDate) return;
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
  useEffect(() => {
    if (active && selectedDayCycleId) onActiveCycleChange?.(selectedDayCycleId);
  }, [active, selectedDayCycleId, onActiveCycleChange]);

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

  const scheduleSession = async (sessionId: string, startsAt: number | null, durationMs: number | null) => {
    return (await run(async () => {
      await setSessionSchedule(sessionId, startsAt, durationMs);
      invalidateCalendar(qc);
    })) !== null;
  };

  const onDragOverCell = (event: DragEvent<HTMLDivElement>) => {
    // The browser can hide custom payload values during dragover; validation
    // happens on drop, while the live source state controls acceptance here.
    if (dragging) event.preventDefault();
  };

  return (
    <div className="flex h-full min-h-0 flex-col" data-calendar-view={view}
      onDragEnd={() => setDragging(null)}
      onKeyDown={(event) => { if (event.key === "Escape" && !event.defaultPrevented && !event.nativeEvent.isComposing) { setDragging(null); setSelectedTask(null); setHoveredTask(null); } }}>
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
        <Button size="compact" variant="ghost" onClick={() => { selectDay(today); setWeekScrollTarget({ date: today, animate: true }); }} aria-label="Jump to today">
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
        {/* Week keeps readable day widths; Month retains its compact seven-column grid. */}
        <div ref={datesViewport} role="region" aria-label="Calendar dates" tabIndex={0}
          className={cn("min-h-0 min-w-0 flex-1 overflow-auto", view === "week" ? "py-4" : "p-4")}>
          {plans.isError && <p role="alert" className="mb-3 text-caption text-danger">Task details could not be loaded. <button className="underline" onClick={() => void plans.refetch()}>Retry</button></p>}
          {isLoading ? (
            <p className="p-4 text-body text-hint">Loading calendar…</p>
          ) : (
            <div
              role="grid"
              aria-label={`Calendar ${view} view`}
              className={cn(
                "grid",
                view === "week" ? "calendar-week-grid" : "min-w-[720px] grid-cols-7 gap-1",
              )}
            >
              {WEEKDAY_LABELS.map((label) => (
                  <div key={label} role="columnheader" className="text-caption font-medium text-hint">
                    {label}
                  </div>
                ))}
              {days.map((day) => (
                    <DayCell
                      key={day.date}
                      day={day}
                      today={today}
                      selected={day.date === selectedDate}
                      density={view}
                      tasks={calendarTasks(day.day_cycle ? workspaces[day.day_cycle.id]?.tasks ?? [] : [])}
                      tasksLoading={!!day.day_cycle && !workspaces[day.day_cycle.id] && !plans.isError}
                      tasksUnavailable={plans.isError}
                      relations={relations}
                      onSelect={selectDay}
                      onCreate={createDay}
                      onOpenTask={openTask}
                      onOpenSession={openSession}
                      onDayDragStart={(dayId, date, token) => setDragging({ dayId, date, token })}
                      onDayDragEnd={() => setDragging(null)}
                      onDragOverCell={onDragOverCell}
                      onDropOnCell={onDropOnCell}
                    />
                  ))}
            </div>
          )}
        </div>

        {/* Plan content and timed execution are two sections of the same day. */}
        <aside ref={detailsRef} className="flex w-panel shrink-0 flex-col gap-3 border-l border-light bg-content p-4">
          <header className="flex items-center justify-between gap-2">
            <h2 className="text-block-title font-semibold">{selectedDate}</h2>
            <div className="flex rounded-md bg-subtle p-0.5" role="tablist" aria-label="Day details">
              {(["plan", "schedule"] as const).map((tab) => <button key={tab} id={`day-detail-${tab}`} aria-controls={`day-panel-${tab}`} type="button" role="tab" aria-selected={detail === tab} tabIndex={detail === tab ? 0 : -1}
                className={cn("rounded-sm px-2 py-1 text-caption capitalize transition-colors", detail === tab ? "bg-content text-primary shadow-sm" : "text-secondary hover:text-primary")}
                onClick={() => setDetail(tab)} onKeyDown={(event) => {
                  if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
                  event.preventDefault();
                  const next = event.key === "Home" ? "plan" : event.key === "End" ? "schedule" : detail === "plan" ? "schedule" : "plan";
                  setDetail(next);
                  event.currentTarget.parentElement?.querySelector<HTMLButtonElement>(`#day-detail-${next}`)?.focus();
                }}>{tab}</button>)}
            </div>
          </header>
          <div id="day-panel-plan" role="tabpanel" aria-labelledby="day-detail-plan" className="min-h-0 flex-1 flex-col" style={{ display: detail === "plan" ? "flex" : "none" }}>
          {isLoading || (planCycleIds.length > 0 && plans.isPending)
            ? <p className="text-caption text-hint">Loading plan…</p>
            : plans.isError ? <p className="text-caption text-danger">Plan details are unavailable. Retry loading the tasks.</p>
            : <CalendarPlan active={active && detail === "plan"} date={selectedDate} day={selectedDay?.day_cycle ?? null} cycles={cycles} workspaces={workspaces}
              relations={relations} onCreateDay={createDay} onReviewIssues={onReviewIssues} onPlanWithAI={onPlanWithAI} />}
          </div>
          <div id="day-panel-schedule" role="tabpanel" aria-labelledby="day-detail-schedule" className="min-h-0 flex-1 flex-col" style={{ display: detail === "schedule" ? "flex" : "none" }}>
            <DayTimeline
              date={selectedDate}
              day={selectedDay}
              budget={budget ?? null}
              overlaps={overlapIds(overlaps ?? [])}
              onCreateDay={createDay}
              onSchedule={scheduleSession}
            />
          </div>
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

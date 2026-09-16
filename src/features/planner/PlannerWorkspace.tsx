/** One selected plan per horizon. Time navigation stays separate from task hierarchy. */
import { useEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Button, EmptyState } from "../../ui";
import { createPlanningCycle, ensureDay, getEditorWorkspacesByCycleIds, getPlannerState, LATER_CYCLE_ID, type Cycle } from "../../lib/ipc";
import { qk } from "../../lib/events";
import { isoWeekNumber, todayISO, weekdayName } from "./dates";
import { invalidateCycles, useActionError } from "./actions";
import { CycleColumn } from "./CycleColumn";
import { DurationDialog } from "./DurationDialog";
import { directRelations, highlightedTasks, indexTasks, type RelationView } from "./relations";
import { RelationLayer } from "./RelationLayer";
import { TaskDragProvider } from "./TaskDragContext";
import { ScrollModeHint } from "./ScrollModeHint";
import { useTranslation, formatDate } from "../../lib/i18n";

function currentCycle(cycles: Cycle[], selected: string | null, today: string): Cycle | null {
  return cycles.find((c) => c.id === selected)
    ?? cycles.find((c) => !c.finished && c.starts_on !== null && c.ends_on !== null && c.starts_on <= today && today < c.ends_on)
    ?? cycles.at(-1) ?? null;
}

const WORKSPACE_PANE_SELECTOR = "[data-workspace-scroll-pane]";
const NATIVE_WHEEL_TARGET_SELECTOR = "input, textarea, select, [contenteditable=\"true\"], [role=\"menu\"], [role=\"listbox\"], [data-wheel-native]";

function eventTargetElement(target: EventTarget | null): Element | null {
  if (target instanceof Element) return target;
  return target instanceof Node ? target.parentElement : null;
}

function workspacePane(root: HTMLElement, target: EventTarget | null): HTMLElement | null {
  const element = eventTargetElement(target);
  const pane = element?.closest<HTMLElement>(WORKSPACE_PANE_SELECTOR);
  return pane && root.contains(pane) ? pane : null;
}

function wheelPixels(event: WheelEvent, reference: HTMLElement): number {
  if (event.deltaMode === 1) return event.deltaY * 16;
  if (event.deltaMode === 2) return event.deltaY * Math.max(reference.clientHeight, 1);
  return event.deltaY;
}

/**
 * Mouse-wheel policy for the wide planner: vertical wheel input pans the
 * horizontal workspace until a primary mouse click activates a scrollport.
 * Focus and content length never change that intent. Predominantly horizontal
 * gestures, browser zoom and controls outside the scrollports remain native.
 */
export function useWorkspaceWheelRouting(
  rootRef: RefObject<HTMLElement | null>,
  horizontalRef: RefObject<HTMLElement | null>,
  enabled = true,
): void {
  const activePaneRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    const root = rootRef.current;
    const horizontal = horizontalRef.current;
    if (!enabled || !root || !horizontal) return;

    const setPaneActive = (pane: HTMLElement | null, active: boolean) => {
      if (!pane) return;
      if (active) pane.setAttribute("data-wheel-active", "true");
      else pane.removeAttribute("data-wheel-active");
      // Keep the icon and wheel mode in the same transition. Native WebViews
      // must remove the old icon immediately, without a relational CSS repaint.
      const hint = pane.parentElement?.querySelector<HTMLElement>(":scope > .workspace-scroll-heading [data-workspace-scroll-hint]");
      if (hint) hint.hidden = !active;
    };
    const setActivePane = (pane: HTMLElement | null) => {
      setPaneActive(activePaneRef.current, false);
      activePaneRef.current = pane;
      setPaneActive(pane, true);
    };
    const onMouseDown = (event: MouseEvent) => {
      if (event.button === 0) setActivePane(workspacePane(root, event.target));
    };
    const reset = () => setActivePane(null);
    const onWheel = (event: WheelEvent) => {
      if (event.defaultPrevented || event.ctrlKey || event.metaKey || event.shiftKey || event.deltaY === 0
        || Math.abs(event.deltaX) >= Math.abs(event.deltaY)) return;

      const target = eventTargetElement(event.target);
      if (activePaneRef.current && !root.contains(activePaneRef.current)) reset();
      const activePane = activePaneRef.current;
      const targetPane = workspacePane(root, target);
      if (targetPane && targetPane === activePane) {
        // Empty panes and scroll boundaries retain vertical mode. Discard minor
        // horizontal drift instead of moving both the pane and the workspace.
        if (event.deltaX !== 0) {
          event.preventDefault();
          activePane.scrollTop += wheelPixels(event, activePane);
        }
        return;
      }
      // Task textareas in inactive panes still browse horizontally. Menus and
      // controls outside a planner scrollport own their native interactions.
      if (!targetPane && target?.closest(NATIVE_WHEEL_TARGET_SELECTOR)) return;

      if (horizontal.scrollWidth <= horizontal.clientWidth) return;
      event.preventDefault();
      horizontal.scrollLeft += wheelPixels(event, horizontal);
    };

    // The marker belongs to the actual scrollport, never its title/wrapper.
    // Clicks anywhere else clear the mode, including outside the workspace.
    document.addEventListener("mousedown", onMouseDown, true);
    window.addEventListener("blur", reset);
    const wheelOptions: AddEventListenerOptions = { passive: false };
    horizontal.addEventListener("wheel", onWheel, wheelOptions);
    return () => {
      document.removeEventListener("mousedown", onMouseDown, true);
      window.removeEventListener("blur", reset);
      horizontal.removeEventListener("wheel", onWheel, wheelOptions);
      reset();
    };
  }, [enabled, horizontalRef, rootRef]);
}

export function PlannerWorkspace({ active = true, onActiveCycleChange, onReviewIssues, onPlanWithAI, revealTask }: { revealTask?: {cycleId:string;taskId:string;requestId:number}; active?: boolean; onActiveCycleChange?: (id: string) => void; onReviewIssues?: (id: string) => void; onPlanWithAI?: (id: string) => void }) {
  const { t } = useTranslation("planning");
  const qc = useQueryClient();
  const { data: state, isLoading, isError } = useQuery({ queryKey: qk.plannerState(), queryFn: getPlannerState });
  const [durationOpen, setDurationOpen] = useState(false);
  const [selectedMonth, setSelectedMonth] = useState<string | null>(null);
  const [selectedWeek, setSelectedWeek] = useState<string | null>(null);
  const [selectedDay, setSelectedDay] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const { error, run, dismiss } = useActionError();
  const today = todayISO();
  const cycles = [...(state?.cycles ?? [])].sort((a, b) => (a.starts_on ?? "").localeCompare(b.starts_on ?? "") || a.position - b.position);
  const months = cycles.filter((c) => c.type === "month" && c.id !== LATER_CYCLE_ID);
  const month = currentCycle(months, selectedMonth, today);
  const weeks = cycles.filter((c) => c.type === "week");
  const week = currentCycle(weeks, selectedWeek, today);
  const days = cycles.filter((c) => c.type === "day" && (!week || (c.starts_on !== null && week.starts_on !== null && week.ends_on !== null && week.starts_on <= c.starts_on && c.starts_on < week.ends_on)));
  const day = currentCycle(days, selectedDay, today);
  const workspaceRef = useRef<HTMLDivElement>(null);
  const horizontalScrollRef = useRef<HTMLDivElement>(null);
  const viewportRef = useRef<HTMLDivElement>(null);
  const [selectedTask, setSelectedTask] = useState<string | null>(null);
  const [hoveredTask, setHoveredTask] = useState<string | null>(null);
  const [dragging, setDragging] = useState(false);
  const visibleCycles = [month, week, day].filter((c): c is Cycle => c !== null);
  const contextKey = [week?.id, day?.id].join(":");
  const contextIds = [...new Set([...months.map((cycle) => cycle.id), ...visibleCycles.map((cycle) => cycle.id)])].sort();
  const { data: workspaces } = useQuery({ queryKey: qk.editorWorkspaces(contextIds), queryFn: () => getEditorWorkspacesByCycleIds(contextIds), enabled: contextIds.length > 0 });
  const tasks = useMemo(() => indexTasks(Object.values(workspaces ?? {}).map((workspace) => workspace.tasks)), [workspaces]);
  const cycleMap = new Map(cycles.map((cycle) => [cycle.id, cycle]));
  const focusId = selectedTask ?? hoveredTask;
  const edges = directRelations(focusId, tasks, cycleMap);
  const relations: RelationView = { tasks, cycles: cycleMap, selectedId: selectedTask,
    highlighted: highlightedTasks(focusId, tasks, cycleMap), select: setSelectedTask, preview: setHoveredTask, setDragging };
  const selected = selectedTask ? tasks.get(selectedTask) : undefined;
  useWorkspaceWheelRouting(workspaceRef, horizontalScrollRef, active && !isLoading && !isError && state !== undefined);
  useEffect(() => { setSelectedTask(null); setHoveredTask(null); }, [contextKey]);
  useEffect(() => {
    if (!active || !selectedTask) return;
    const escape = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented || event.isComposing || document.querySelector('[role="dialog"], [role="menu"]')) return;
      setSelectedTask(null); setHoveredTask(null);
    };
    window.addEventListener("keydown", escape);
    return () => window.removeEventListener("keydown", escape);
  }, [active, selectedTask]);
  const locate = (id: string) => {
    const target = tasks.get(id);
    if (target && cycleMap.get(target.cycle_id)?.type === "month") setSelectedMonth(target.cycle_id);
    const row = [...(viewportRef.current?.querySelectorAll<HTMLElement>("[data-task-id]") ?? [])].find((el) => el.dataset.taskId === id);
    row?.scrollIntoView({ block: "nearest", inline: "center", behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth" });
    setSelectedTask(id);
  };
  const selectMonth = (id: string) => { setSelectedMonth(id); onActiveCycleChange?.(id); };
  const selectWeek = (id: string) => { setSelectedWeek(id); setSelectedDay(null); onActiveCycleChange?.(id); };
  const selectDay = (id: string) => { setSelectedDay(id); onActiveCycleChange?.(id); };

  // A diagnostic points to the real task in its own planning horizon.
  useEffect(() => {
    if (!revealTask) return;
    const target = (state?.cycles ?? []).find(c => c.id === revealTask.cycleId);
    if (target?.type === "month") setSelectedMonth(target.id);
    if (target?.type === "week") setSelectedWeek(target.id);
    if (target?.type === "day") {
      const parentWeek = (state?.cycles ?? []).find(c => c.type === "week" && c.starts_on && c.ends_on && target.starts_on && c.starts_on <= target.starts_on && target.starts_on < c.ends_on);
      setSelectedWeek(parentWeek?.id ?? null); setSelectedDay(target.id);
    }
  }, [revealTask, state]);

  const createWeek = async () => {
    if (creating) return;
    const existing = weeks.find((c) => c.starts_on !== null && c.ends_on !== null && c.starts_on <= today && today < c.ends_on);
    if (existing) { selectWeek(existing.id); return; }
    setCreating(true);
    const created = await run(() => createPlanningCycle({ cycle_type: "week" }));
    if (created) { selectWeek(created.id); invalidateCycles(qc); }
    setCreating(false);
  };
  const createToday = async () => {
    if (creating) return;
    setCreating(true);
    const created = await run(() => ensureDay(null));
    if (created) {
      setSelectedWeek(created.parent_id);
      selectDay(created.id);
      invalidateCycles(qc);
    }
    setCreating(false);
  };

  if (isLoading) return <div className="flex h-full items-center justify-center text-hint">{t("workspace.loading")}</div>;
  if (isError || state === undefined) return <div role="alert" className="p-8 text-secondary">{t("workspace.loadError")} <Button onClick={() => void qc.invalidateQueries({ queryKey: qk.plannerState() })}>{t("workspace.retry")}</Button></div>;

  const weekHint = t("workspace.weekHint");
  const dayHint = t("workspace.dayHint");

  return <TaskDragProvider>
    <div ref={workspaceRef} className="flex h-full min-h-0 flex-col">
    <div ref={viewportRef} className="relative min-h-0 flex-1">
      <div ref={horizontalScrollRef} className="planner-horizontal-scroll h-full min-w-0 overflow-x-auto" aria-label={t("workspace.planning")}>
      <div className="flex h-full w-max items-start gap-8 px-6 pb-6 pt-8">
        <Horizon label={t("workspace.cycles")} cycles={months} selected={month?.id} onSelect={selectMonth}
          action={<button className="cycle-nav-add" onClick={() => setDurationOpen(true)}>+ {t("workspace.addCycle")}</button>} t={t}>
          {month ? <CycleColumn revealTask={revealTask} active={active} key={month.id} cycle={month} relations={relations} onReviewIssues={onReviewIssues} onPlanWithAI={onPlanWithAI} onSelect={() => onActiveCycleChange?.(month.id)} /> :
            <EmptyState title={t("workspace.emptyTitle")} description={t("workspace.emptyDescription")}
              action={<Button variant="primary" onClick={() => setDurationOpen(true)}>{t("workspace.setGoals")}</Button>} className="min-h-[360px] w-plan self-start" />}
        </Horizon>
        <Horizon label={t("workspace.weeks")} cycles={weeks} selected={week?.id} onSelect={selectWeek}
          action={<button className="cycle-nav-add" disabled={creating} onClick={() => void createWeek()}>{t("workspace.thisWeek")}</button>} t={t}>
          {week ? <CycleColumn revealTask={revealTask} active={active} key={week.id} cycle={week} relations={relations} onReviewIssues={onReviewIssues} onPlanWithAI={onPlanWithAI} onSelect={() => onActiveCycleChange?.(week.id)} /> :
            <EmptyState title={t("workspace.thisWeek")} description={weekHint}
              action={<Button disabled={creating} onClick={() => void createWeek()}>{t("workspace.createWeek")}</Button>} className="min-h-[360px] w-plan self-start" />}
        </Horizon>
        <Horizon label={t("workspace.days")} cycles={days} selected={day?.id} onSelect={selectDay}
          action={<button className="cycle-nav-add" disabled={creating} onClick={() => void createToday()}>+ {t("workspace.today")}</button>} t={t}>
          {day ? <CycleColumn revealTask={revealTask} active={active} key={day.id} cycle={day} relations={relations} onReviewIssues={onReviewIssues} onPlanWithAI={onPlanWithAI} onSelect={() => onActiveCycleChange?.(day.id)} /> :
            <EmptyState title={t("workspace.today")} description={dayHint}
              action={<Button disabled={creating} onClick={() => void createToday()}>{t("workspace.addToday")}</Button>} className="min-h-[360px] w-plan self-start" />}
        </Horizon>
      </div>
    </div>
    <RelationLayer viewportRef={viewportRef} edges={edges} tasks={tasks} hidden={dragging} />
    </div>
    {selected && <div className="connection-bar flex shrink-0 items-center gap-3 border-t border-light bg-content px-6 py-2 text-caption" aria-label={t("workspace.connections")}>
      <span className="shrink-0 font-medium text-secondary">{t("workspace.connections")}</span>
      <span className="max-w-48 truncate text-primary" title={selected.title}>{selected.title}</span>
      <div className="connection-bar-scroll flex min-w-0 flex-1 items-center gap-2 overflow-x-auto">
        {edges.length ? edges.map(([parent, child]) => { const related = parent.id === selected.id ? child : parent;
          return <button key={related.id} className="shrink-0 rounded-md border border-light px-2 py-1 text-secondary hover:bg-hover" title={t("workspace.locate", { title: related.title })} onClick={() => locate(related.id)}>{parent.id === selected.id ? "→ " : "← "}{related.title}</button>;
        }) : <span className="text-hint">{t("workspace.noConnections")}</span>}
      </div>
      <button className="h-7 w-7 shrink-0 rounded-md text-secondary hover:bg-hover" aria-label={t("workspace.closeConnections")} title={`${t("workspace.closeConnections")} (Esc)`} onClick={() => { setSelectedTask(null); setHoveredTask(null); }}>×</button>
    </div>}
    </div>
    <DurationDialog open={durationOpen} onClose={() => setDurationOpen(false)} onCreated={(cycle) => { selectMonth(cycle.id); invalidateCycles(qc); }} />
    {error && <div role="alert" className="absolute bottom-4 left-1/2 z-40 -translate-x-1/2 rounded-lg border border-light bg-content px-4 py-3 text-caption text-danger shadow-lg">{error}<button className="ml-3 underline" onClick={dismiss}>{t("workspace.dismiss")}</button></div>}
  </TaskDragProvider>;
}

function Horizon({ label, cycles, selected, onSelect, action, children, t }: {
  label: string; cycles: Cycle[]; selected?: string; onSelect: (id: string) => void; action: ReactNode; children: ReactNode; t: (key: string, options?: Record<string, unknown>) => string;
}) {
  return <div className="flex h-full shrink-0 gap-4">
    <nav aria-label={label} className="flex w-[88px] shrink-0 flex-col gap-1 pt-1">
      <h2 className="workspace-scroll-heading mb-2 flex items-center justify-between gap-1 px-2 text-caption font-medium text-secondary">{label}<ScrollModeHint /></h2>
      <div data-workspace-scroll-pane className="min-h-0 overflow-y-auto">
        {cycles.map((cycle) => <button key={cycle.id} type="button" aria-current={selected === cycle.id ? "date" : undefined}
          onClick={() => onSelect(cycle.id)} title={cycle.starts_on ? formatDate(cycle.starts_on, { year: "numeric", month: "short", day: "numeric" }) : cycle.title}
          className={`mb-1 w-full rounded-md px-2 py-2 text-left text-caption transition-colors ${selected === cycle.id ? "bg-focus-surface font-medium text-focus" : "text-secondary hover:bg-hover"}`}>
          {cycle.type === "month" ? (cycle.starts_on ? formatDate(cycle.starts_on, { month: "short", day: "numeric" }) : cycle.title) : cycle.type === "week" ? `W${isoWeekNumber(cycle.starts_on ?? "") ?? cycle.position + 1}` : cycle.starts_on ? weekdayName(cycle.starts_on).slice(0, 3) : cycle.title}
          {cycle.finished && <span className="block text-[10px] font-normal text-hint">{t("workspace.ended")}</span>}
        </button>)}
      </div>
      {action}
    </nav>
    {children}
  </div>;
}

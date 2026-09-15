/** One selected plan per horizon. Time navigation stays separate from task hierarchy. */
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Button, EmptyState } from "../../ui";
import { createPlanningCycle, ensureDay, getEditorWorkspacesByCycleIds, getPlannerState, LATER_CYCLE_ID, type Cycle } from "../../lib/ipc";
import { qk } from "../../lib/events";
import { formatShortDate, isoWeekNumber, todayISO, weekdayName } from "./dates";
import { invalidateCycles, useActionError } from "./actions";
import { CycleColumn } from "./CycleColumn";
import { DurationDialog } from "./DurationDialog";
import { directRelations, highlightedTasks, indexTasks, type RelationView } from "./relations";
import { RelationLayer } from "./RelationLayer";
import { TaskDragProvider } from "./TaskDragContext";

function currentCycle(cycles: Cycle[], selected: string | null, today: string): Cycle | null {
  return cycles.find((c) => c.id === selected)
    ?? cycles.find((c) => !c.finished && c.starts_on !== null && c.ends_on !== null && c.starts_on <= today && today < c.ends_on)
    ?? cycles.at(-1) ?? null;
}

export function PlannerWorkspace({ active = true, onActiveCycleChange, onReviewIssues, onPlanWithAI, revealTask }: { revealTask?: {cycleId:string;taskId:string;requestId:number}; active?: boolean; onActiveCycleChange?: (id: string) => void; onReviewIssues?: (id: string) => void; onPlanWithAI?: (id: string) => void }) {
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

  if (isLoading) return <div className="flex h-full items-center justify-center text-hint">Loading workspace…</div>;
  if (isError || state === undefined) return <div role="alert" className="p-8 text-secondary">The workspace could not be loaded. <Button onClick={() => void qc.invalidateQueries({ queryKey: qk.plannerState() })}>Retry</Button></div>;

  const weekHint = "What needs to get done this week? Add several tasks, with or without a long-term goal.";
  const dayHint = "Plan what matters today. Link to a weekly task when useful, or keep it independent.";

  return <TaskDragProvider>
    <div className="flex h-full min-h-0 flex-col">
    <div ref={viewportRef} className="relative min-h-0 flex-1">
    <div className="h-full min-w-0 overflow-x-auto" aria-label="Planning workspace">
      <div className="flex h-full w-max items-start gap-8 px-6 pb-6 pt-8">
        <Horizon label="Cycles" cycles={months} selected={month?.id} onSelect={selectMonth}
          action={<button className="cycle-nav-add" onClick={() => setDurationOpen(true)}>+ Add cycle</button>}>
          {month ? <CycleColumn revealTask={revealTask} active={active} key={month.id} cycle={month} relations={relations} onReviewIssues={onReviewIssues} onPlanWithAI={onPlanWithAI} onSelect={() => onActiveCycleChange?.(month.id)} /> :
            <EmptyState title="Start with a meaningful goal" description="What would you like to achieve in the coming months? Give your weeks and days a direction."
              action={<Button variant="primary" onClick={() => setDurationOpen(true)}>Set long-term goals</Button>} className="min-h-[360px] w-plan self-start" />}
        </Horizon>
        <Horizon label="Weeks" cycles={weeks} selected={week?.id} onSelect={selectWeek}
          action={<button className="cycle-nav-add" disabled={creating} onClick={() => void createWeek()}>This week</button>}>
          {week ? <CycleColumn revealTask={revealTask} active={active} key={week.id} cycle={week} relations={relations} onReviewIssues={onReviewIssues} onPlanWithAI={onPlanWithAI} onSelect={() => onActiveCycleChange?.(week.id)} /> :
            <EmptyState title="Shape your week" description={weekHint}
              action={<Button disabled={creating} onClick={() => void createWeek()}>Create this week</Button>} className="min-h-[360px] w-plan self-start" />}
        </Horizon>
        <Horizon label="Days" cycles={days} selected={day?.id} onSelect={selectDay}
          action={<button className="cycle-nav-add" disabled={creating} onClick={() => void createToday()}>+ Today</button>}>
          {day ? <CycleColumn revealTask={revealTask} active={active} key={day.id} cycle={day} relations={relations} onReviewIssues={onReviewIssues} onPlanWithAI={onPlanWithAI} onSelect={() => onActiveCycleChange?.(day.id)} /> :
            <EmptyState title="Give today a clear focus" description={dayHint}
              action={<Button disabled={creating} onClick={() => void createToday()}>Add today</Button>} className="min-h-[360px] w-plan self-start" />}
        </Horizon>
      </div>
    </div>
    <RelationLayer viewportRef={viewportRef} edges={edges} tasks={tasks} hidden={dragging} />
    </div>
    {selected && <div className="connection-bar flex shrink-0 items-center gap-3 border-t border-light bg-content px-6 py-2 text-caption" aria-label="Task connections">
      <span className="shrink-0 font-medium text-secondary">Connections</span>
      <span className="max-w-48 truncate text-primary" title={selected.title}>{selected.title}</span>
      <div className="flex min-w-0 flex-1 items-center gap-2 overflow-x-auto">
        {edges.length ? edges.map(([parent, child]) => { const related = parent.id === selected.id ? child : parent;
          return <button key={related.id} className="shrink-0 rounded-md border border-light px-2 py-1 text-secondary hover:bg-hover" title={`Locate ${related.title}`} onClick={() => locate(related.id)}>{parent.id === selected.id ? "→ " : "← "}{related.title}</button>;
        }) : <span className="text-hint">No connections yet. Use the color slot to link a goal.</span>}
      </div>
      <button className="h-7 w-7 shrink-0 rounded-md text-secondary hover:bg-hover" aria-label="Close connections" title="Close connections (Esc)" onClick={() => { setSelectedTask(null); setHoveredTask(null); }}>×</button>
    </div>}
    </div>
    <DurationDialog open={durationOpen} onClose={() => setDurationOpen(false)} onCreated={(cycle) => { selectMonth(cycle.id); invalidateCycles(qc); }} />
    {error && <div role="alert" className="absolute bottom-4 left-1/2 z-40 -translate-x-1/2 rounded-lg border border-light bg-content px-4 py-3 text-caption text-danger shadow-lg">{error}<button className="ml-3 underline" onClick={dismiss}>Dismiss</button></div>}
  </TaskDragProvider>;
}

function Horizon({ label, cycles, selected, onSelect, action, children }: {
  label: string; cycles: Cycle[]; selected?: string; onSelect: (id: string) => void; action: ReactNode; children: ReactNode;
}) {
  return <div className="flex h-full shrink-0 gap-4">
    <nav aria-label={label} className="flex w-[88px] shrink-0 flex-col gap-1 pt-1">
      <h2 className="mb-2 px-2 text-caption font-medium text-secondary">{label}</h2>
      <div className="min-h-0 overflow-y-auto">
        {cycles.map((cycle) => <button key={cycle.id} type="button" aria-current={selected === cycle.id ? "date" : undefined}
          onClick={() => onSelect(cycle.id)} title={cycle.starts_on ?? cycle.title}
          className={`mb-1 w-full rounded-md px-2 py-2 text-left text-caption transition-colors ${selected === cycle.id ? "bg-focus-surface font-medium text-focus" : "text-secondary hover:bg-hover"}`}>
          {cycle.type === "month" ? (cycle.starts_on ? formatShortDate(cycle.starts_on) : cycle.title) : cycle.type === "week" ? `W${isoWeekNumber(cycle.starts_on ?? "") ?? cycle.position + 1}` : cycle.starts_on ? weekdayName(cycle.starts_on).slice(0, 3) : cycle.title}
          {cycle.finished && <span className="block text-[10px] font-normal text-hint">Ended</span>}
        </button>)}
      </div>
      {action}
    </nav>
    {children}
  </div>;
}

import type { CSSProperties } from "react";
import type { Cycle, EditorWorkspace, TaskNode } from "../../lib/ipc";
import { Button, cn } from "../../ui";
import { TaskList } from "../planner/TaskList";
import { taskColor, type RelationView } from "../planner/relations";
import { calendarTasks } from "./calendar-model";
import { PlanWithAI } from "../agent/PlanWithAI";
import { useTranslation, formatDate } from "../../lib/i18n";

/** Calendar edits the existing day workspace; goal context uses the same task ids and colors. */
export function CalendarPlan({ active = true, date, day, cycles, workspaces, relations, onCreateDay, onReviewIssues, onPlanWithAI }: {
  active?: boolean;
  date: string;
  day: Cycle | null;
  cycles: Cycle[];
  workspaces: Record<string, EditorWorkspace>;
  relations: RelationView;
  onCreateDay: (date: string) => void;
  onReviewIssues?: (cycleId: string) => void;
  onPlanWithAI?: (cycleId: string) => void;
}) {
  const { t } = useTranslation("planning");
  const displayDate = formatDate(date, { year: "numeric", month: "short", day: "numeric" });
  const week = cycles.find((cycle) => cycle.type === "week" && cycle.starts_on && cycle.ends_on && cycle.starts_on <= date && date < cycle.ends_on);
  const weeklyTasks = calendarTasks(week ? workspaces[week.id]?.tasks ?? [] : []);
  const dailyTasks = calendarTasks(day ? workspaces[day.id]?.tasks ?? [] : []);
  const longTermGoals = new Map<string, TaskNode>();
  const collectGoals = (tasks: TaskNode[]) => {
    for (const task of tasks) {
      if (task.title.trim()) {
        const seen = new Set<string>();
        let parent = task.parent_id ? relations.tasks.get(task.parent_id) : undefined;
        while (parent && !seen.has(parent.id)) {
          seen.add(parent.id);
          if (relations.cycles.get(parent.cycle_id)?.type === "month") longTermGoals.set(parent.id, parent);
          parent = parent.parent_id ? relations.tasks.get(parent.parent_id) : undefined;
        }
      }
      collectGoals(task.children);
    }
  };
  collectGoals([...weeklyTasks, ...dailyTasks]);
  const goalRow = (task: TaskNode) => <button key={task.id} type="button" title={task.title}
    data-goal-row={task.id}
    data-related={relations.highlighted.has(task.id) || undefined}
    data-selected={relations.selectedId === task.id || undefined}
    style={{ "--task-color": taskColor(task, relations.tasks) ?? "var(--color-focus)" } as CSSProperties}
    aria-label={t("calendar.showConnections", { title: task.title })} onClick={() => relations.select(task.id)}
    onMouseEnter={() => relations.preview(task.id)} onMouseLeave={() => relations.preview(null)}
    className={cn("task-row flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left text-menu hover:bg-hover focus-visible:outline-2 focus-visible:outline-focus",
      task.completed && "text-hint line-through")}>
    <span className="task-color-slot mt-0.5 shrink-0" aria-hidden="true" style={{ backgroundColor: taskColor(task, relations.tasks) ?? "transparent", borderColor: taskColor(task, relations.tasks) ?? undefined }} />
    <span>{task.title}</span>
  </button>;

  return <div className="min-h-0 flex-1 overflow-y-auto" aria-label={t("calendar.dayPlan", { date: displayDate })}
    onClick={(event) => { if (relations.selectedId && !(event.target as HTMLElement).closest("[data-task-id], [data-goal-row], button, textarea, input")) relations.select(relations.selectedId); }}>
    <section className="plan-ai-scope pb-4">
      <h3 className="mb-2 text-caption font-semibold uppercase tracking-[0.06em] text-secondary">{t("calendar.dailyTasks")}</h3>
      {day ? <TaskList active={active} key={day.id} cycleId={day.id} cycleType="day" locked={day.finished} relations={relations}
        onReviewIssues={onReviewIssues ? () => onReviewIssues(day.id) : undefined} />
        : <div className="rounded-lg border border-light p-3">
          <p className="mb-3 text-menu text-secondary">{t("calendar.noDayPlan")}</p>
          <Button size="compact" onClick={() => onCreateDay(date)}>{t("calendar.createDay")}</Button>
        </div>}
      {day && onPlanWithAI && <PlanWithAI active={active} cycle={day} onPlan={onPlanWithAI} />}
    </section>
    <section className="border-t border-light py-4" aria-label={t("calendar.weeklyGoals")}>
      <h3 className="mb-2 text-caption font-semibold uppercase tracking-[0.06em] text-secondary">{t("calendar.weeklyGoals")}</h3>
      {weeklyTasks.length ? weeklyTasks.map(goalRow) : <p className="text-caption text-hint">{t("calendar.noWeekTasks")}</p>}
    </section>
    <section className="border-t border-light py-4" aria-label={t("calendar.longTermGoals")}>
      <h3 className="mb-2 text-caption font-semibold uppercase tracking-[0.06em] text-secondary">{t("calendar.longTermGoals")}</h3>
      {longTermGoals.size ? [...longTermGoals.values()].map(goalRow) : <p className="text-caption text-hint">{t("calendar.noLongTerm")}</p>}
    </section>
    <p className="text-caption text-hint">{t("calendar.selectGoal")}</p>
  </div>;
}

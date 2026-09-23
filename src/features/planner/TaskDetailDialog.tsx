import { useEffect, useRef, useState, type RefObject } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { getDirectLinkedChildren, getTaskContinuity, patchTask, LATER_CYCLE_ID, type CycleType, type TaskNode } from "../../lib/ipc";
import { qk } from "../../lib/events";
import { formatDate, useTranslation } from "../../lib/i18n";
import { Button, Dialog } from "../../ui";
import { ReminderPicker } from "../reminders/ReminderPicker";
import { errorMessage, invalidateTasks } from "./actions";

type HistoryRecord = { task_id: string; date: string; note: string; completed: boolean };
type HistoryItem = { kind: "note"; record: HistoryRecord } | { kind: "dates"; dates: string[] };

function historyItems(records: HistoryRecord[]): HistoryItem[] {
  const ordered = [...records].sort((a, b) => a.date.localeCompare(b.date));
  const items: HistoryItem[] = [];
  let dates: string[] | undefined;
  for (const record of ordered) {
    if (record.note.trim()) {
      dates = undefined;
      items.push({ kind: "note", record });
    }
    else {
      if (!dates) {
        dates = [];
        items.push({ kind: "dates", dates });
      }
      dates.push(record.date);
    }
  }
  return items;
}

function compactDates(dates: string[]): string {
  const groups: string[] = [];
  for (let index = 0; index < dates.length;) {
    let end = index;
    while (end + 1 < dates.length && Date.parse(`${dates[end + 1]}T00:00:00Z`) - Date.parse(`${dates[end]}T00:00:00Z`) === 86_400_000) end += 1;
    groups.push(end - index + 1 >= 3 ? `${dates[index]} ~ ${dates[end]}` : dates.slice(index, end + 1).join(" / "));
    index = end + 1;
  }
  return groups.join(" / ");
}

export function TaskDetailDialog({
  task,
  locked,
  onClose,
  returnFocusTo,
  cycleLabel,
  cycleType,
  parentGoalTitle,
}: {
  task: TaskNode;
  locked: boolean;
  onClose: () => void;
  returnFocusTo?: RefObject<HTMLElement | null>;
  cycleLabel?: string;
  cycleType?: CycleType;
  parentGoalTitle?: string;
}) {
  const { t } = useTranslation("planning");
  const { t: reminderText } = useTranslation("shell");
  const qc = useQueryClient();
  const [note, setNote] = useState(task.note);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [reminderOpen, setReminderOpen] = useState(false);
  const noteRef = useRef<HTMLTextAreaElement>(null);
  const continuity = useQuery({
    queryKey: qk.taskContinuity(task.id),
    queryFn: () => getTaskContinuity(task.id),
    enabled: !!task.title.trim() && cycleType !== "month" && cycleType !== "week",
  });
  const showLinkedChildren = (cycleType === "month" || cycleType === "week") && task.cycle_id !== LATER_CYCLE_ID;
  const linkedChildren = useQuery({
    queryKey: qk.directLinkedChildren(task.id),
    queryFn: () => getDirectLinkedChildren(task.id),
    enabled: showLinkedChildren && !!task.title.trim(),
  });
  useEffect(() => {
    setNote(task.note);
    setError(null);
  }, [task.id, task.note]);
  useEffect(() => {
    noteRef.current?.focus({ preventScroll: true });
  }, []);
  const save = async () => {
    if (locked || saving) return;
    if (note === task.note) {
      onClose();
      return;
    }
    setSaving(true);
    try {
      await patchTask(task.id, { note });
      invalidateTasks(qc, task.cycle_id);
      onClose();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setSaving(false);
    }
  };
  const selected = continuity.data?.episodes[continuity.data.selected_episode_index];
  return (
    <Dialog open onClose={onClose} title={task.title} wide returnFocusTo={returnFocusTo}
      footer={locked ? <Button onClick={onClose}>{t("task.detailClose")}</Button> : <>
        <Button variant="ghost" onClick={onClose}>{t("task.detailCancel")}</Button>
        <Button onClick={() => void save()} disabled={saving || note === task.note}>{t("task.detailSave")}</Button>
      </>}>
      <div className="space-y-5">
        {(cycleLabel || parentGoalTitle) && <p className="text-caption text-secondary">
          {[cycleLabel, parentGoalTitle].filter(Boolean).join(" · ")}
        </p>}
        <textarea
          ref={noteRef}
          id={`task-note-${task.id}`}
          aria-label={t("task.detailNote")}
          value={note}
          onChange={(event) => setNote(event.target.value)}
          readOnly={locked}
          rows={6}
          placeholder={t("task.detailNotePlaceholder")}
          className="w-full min-h-32 resize-y rounded-md border border-control bg-content px-3 py-2 text-body text-primary outline-none focus:border-focus focus:ring-2 focus:ring-focus/20 read-only:bg-surface"
        />
        {error && <p role="alert" className="text-caption text-danger">{error}</p>}
        {!locked && <div>
          {reminderOpen ? <ReminderPicker target_kind="task" target_id={task.id}
            onSaved={() => setReminderOpen(false)} onCancel={() => setReminderOpen(false)} />
            : <Button variant="ghost" size="compact" onClick={() => setReminderOpen(true)}>{reminderText("reminders.add")}</Button>}
        </div>}
        {continuity.isError && <p role="alert" className="text-caption text-danger">{errorMessage(continuity.error)}</p>}
        {selected && selected.records.length > 1 && <section className="border-t border-light pt-4" aria-label={t("task.detailHistory")}>
          <div className="flex flex-wrap items-baseline justify-between gap-2">
            <h3 className="text-menu font-semibold text-primary">{t("task.detailHistory")}</h3>
            <span className="text-caption text-secondary">
              {t("task.detailSpan", { days: selected.elapsed_days, records: selected.recorded_days })}
              {" · "}{selected.completed ? t("task.detailFinished") : t("task.detailOngoing")}
            </span>
          </div>
          <ol className="mt-3 space-y-3">
            {historyItems(selected.records.filter((record) => record.task_id !== task.id)).map((item) => item.kind === "note"
              ? <li key={item.record.task_id} className="rounded-md bg-surface px-3 py-2">
                <div className="flex items-center justify-between gap-2 text-caption text-secondary">
                  <time dateTime={item.record.date}>{item.record.date}</time>
                  {item.record.completed && <span>{t("task.detailFinished")}</span>}
                </div>
                <p className="mt-1 whitespace-pre-wrap break-words text-body text-primary">{item.record.note}</p>
              </li>
              : <li key={`dates-${item.dates[0]}`} className="text-caption text-secondary">{compactDates(item.dates)}</li>)}
          </ol>
        </section>}
        {showLinkedChildren && <section className="border-t border-light pt-4" aria-label={t("task.detailLinkedChildren")}>
          <h3 className="text-menu font-semibold text-primary">{t("task.detailLinkedChildren")}</h3>
          {linkedChildren.isError ? <p role="alert" className="mt-2 text-caption text-danger">{errorMessage(linkedChildren.error)}</p>
            : linkedChildren.isPending ? <p className="mt-2 text-caption text-hint">{t("task.detailLinkedLoading")}</p>
            : linkedChildren.data?.length ? <ul className="mt-2 divide-y divide-light">
              {linkedChildren.data.map((child) => <li key={child.id} className="flex min-w-0 items-baseline gap-3 py-2">
                <span className={`min-w-0 flex-1 truncate text-body ${child.completed ? "text-hint line-through" : "text-primary"}`} title={child.title}>{child.title}</span>
                <span className="shrink-0 text-caption text-secondary">
                  {t(child.cycle_type === "week" ? "task.detailLinkedWeek" : "task.detailLinkedDay")}
                  {child.starts_on && <> · {formatDate(child.starts_on, { month: "short", day: "numeric" })}</>}
                </span>
              </li>)}
            </ul> : <p className="mt-2 text-caption text-hint">{t("task.detailLinkedEmpty")}</p>}
        </section>}
      </div>
    </Dialog>
  );
}

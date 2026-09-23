import { useEffect, useRef, useState, type RefObject } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { getTaskContinuity, patchTask, type TaskNode } from "../../lib/ipc";
import { qk } from "../../lib/events";
import { useTranslation } from "../../lib/i18n";
import { Button, Dialog } from "../../ui";
import { errorMessage, invalidateTasks } from "./actions";

export function TaskDetailDialog({
  task,
  locked,
  onClose,
  returnFocusTo,
  cycleLabel,
  parentGoalTitle,
}: {
  task: TaskNode;
  locked: boolean;
  onClose: () => void;
  returnFocusTo?: RefObject<HTMLElement | null>;
  cycleLabel?: string;
  parentGoalTitle?: string;
}) {
  const { t } = useTranslation("planning");
  const qc = useQueryClient();
  const [note, setNote] = useState(task.note);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const noteRef = useRef<HTMLTextAreaElement>(null);
  const continuity = useQuery({
    queryKey: qk.taskContinuity(task.id),
    queryFn: () => getTaskContinuity(task.id),
    enabled: !!task.title.trim(),
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
        <label className="block text-menu font-medium text-secondary" htmlFor={`task-note-${task.id}`}>
          {t("task.detailNote")}
        </label>
        <textarea
          ref={noteRef}
          id={`task-note-${task.id}`}
          value={note}
          onChange={(event) => setNote(event.target.value)}
          readOnly={locked}
          rows={6}
          placeholder={t("task.detailNotePlaceholder")}
          className="w-full min-h-32 resize-y rounded-md border border-control bg-content px-3 py-2 text-body text-primary outline-none focus:border-focus focus:ring-2 focus:ring-focus/20 read-only:bg-surface"
        />
        {error && <p role="alert" className="text-caption text-danger">{error}</p>}
        {continuity.isError && <p role="alert" className="text-caption text-danger">{errorMessage(continuity.error)}</p>}
        {selected && <section className="border-t border-light pt-4" aria-label={t("task.detailHistory")}>
          <div className="flex flex-wrap items-baseline justify-between gap-2">
            <h3 className="text-menu font-semibold text-primary">{t("task.detailHistory")}</h3>
            <span className="text-caption text-secondary">
              {t("task.detailSpan", { days: selected.elapsed_days, records: selected.recorded_days })}
              {" · "}{selected.completed ? t("task.detailFinished") : t("task.detailOngoing")}
            </span>
          </div>
          <p className="mt-1 text-caption text-hint">{t("task.detailGrouping")}</p>
          <ol className="mt-3 space-y-3">
            {selected.records.map((record) => <li key={record.task_id} className="rounded-md bg-surface px-3 py-2">
              <div className="flex items-center justify-between gap-2 text-caption text-secondary">
                <time dateTime={record.date}>{record.date}</time>
                {record.completed && <span>{t("task.detailFinished")}</span>}
              </div>
              {record.note && <p className="mt-1 whitespace-pre-wrap break-words text-body text-primary">{record.note}</p>}
            </li>)}
          </ol>
        </section>}
      </div>
    </Dialog>
  );
}

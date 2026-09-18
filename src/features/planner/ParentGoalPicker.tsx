import { useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Popover, PopoverItem } from "../../ui";
import { setTaskParentLink, type TaskNode } from "../../lib/ipc";
import { invalidateTasks, useActionError } from "./actions";
import { canAssignParent, dailyGoal, taskColor, type RelationView } from "./relations";
import { useTranslation } from "../../lib/i18n";

export function ParentGoalPicker({ task, relations, locked, nested, descriptionId }: {
  task: TaskNode; relations: RelationView; locked: boolean; nested: boolean; descriptionId?: string;
}) {
  const { t } = useTranslation("planning");
  const anchorRef = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const { error, run } = useActionError();
  const qc = useQueryClient();
  const parent = task.parent_id ? relations.tasks.get(task.parent_id) : undefined;
  const candidates = [...relations.tasks.values()].filter((candidate) => canAssignParent(task, candidate, relations.cycles));
  const color = taskColor(task, relations.tasks);
  const daily = relations.cycles.get(task.cycle_id)?.type === "day";
  const parentLabel = daily ? t("parent.labelWeeklyOrLongTerm") : t("parent.labelLongTerm");
  const linkedParentLabel = parent && relations.cycles.get(parent.cycle_id)?.type === "month" ? t("parent.labelLongTerm") : t("parent.labelWeekly");
  const choose = async (parentId: string | null) => {
    setSaving(true);
    const updated = await run(() => setTaskParentLink(task.id, parentId));
    if (updated) {
      invalidateTasks(qc, task.cycle_id);
      relations.select(task.id);
      setOpen(false);
    }
    setSaving(false);
  };
  return <>
    <button ref={anchorRef} type="button" className="task-color-control"
      aria-label={parent ? t("parent.change", { title: task.title }) : t("parent.link", { title: task.title, parent: parentLabel })}
      aria-describedby={descriptionId}
      title={dailyGoal(task, relations.tasks, relations.cycles) ? undefined : parent ? t("parent.linked", { title: parent.title }) : t("parent.linkA", { parent: parentLabel })}
      aria-haspopup="menu" aria-expanded={open}
      onMouseEnter={() => relations.preview(task.id)} onMouseLeave={() => relations.preview(null)}
      onClick={() => setOpen(!open)}>
      <span className="task-color-slot" style={color ? { backgroundColor: color, borderColor: color } : undefined} />
    </button>
    <Popover open={open} onClose={() => setOpen(false)} anchorRef={anchorRef} label={t("parent.menu", { parent: parentLabel })} className="w-[300px]">
      <div className="border-b border-light px-3 py-2 text-caption font-medium text-secondary">{nested ? t("parent.partOfTask") : t("parent.menu", { parent: parentLabel })}</div>
      {nested ? <p className="px-3 py-3 text-menu text-secondary">{t("parent.belongs", { parent: parent?.title ?? t("parent.current") })}</p> : <>
        {candidates.length === 0 && <p className="px-3 py-3 text-menu text-secondary">{t("parent.none", { parent: parentLabel })}</p>}
        <div className="max-h-60 overflow-y-auto">
          {candidates.map((candidate) => {
            const candidateColor = taskColor(candidate, relations.tasks);
            return <PopoverItem key={candidate.id} disabled={locked || saving}
              aria-label={t("task.linkTo", { title: candidate.title })} onSelect={() => void choose(candidate.id)}>
              <span className="flex items-center gap-2">
                <span className="h-3.5 w-1.5 shrink-0 rounded-[3px] border" style={{ borderColor: candidateColor ?? "var(--border-control)", backgroundColor: candidateColor ?? "transparent" }} />
                <span className="min-w-0 flex-1 break-words">{candidate.title}</span>
                {daily && <span className="shrink-0 text-caption text-secondary">{relations.cycles.get(candidate.cycle_id)?.type === "month" ? t("parent.labelLongTerm") : t("parent.labelWeekly")}</span>}
                {task.parent_id === candidate.id && <span aria-label={t("parent.current")}>✓</span>}
              </span>
            </PopoverItem>;
          })}
        </div>
        {parent && <PopoverItem disabled={locked || saving} className="border-t border-light" onSelect={() => void choose(null)}>{t("parent.unlink", { parent: linkedParentLabel })}</PopoverItem>}
      </>}
      <PopoverItem className="border-t border-light" onSelect={() => { relations.select(task.id); setOpen(false); }}>{t("parent.connections")}</PopoverItem>
      {saving && <p role="status" className="px-3 py-2 text-caption text-secondary">{t("parent.saving")}</p>}
      {error && <p role="alert" className="px-3 py-2 text-caption text-danger">{error}</p>}
    </Popover>
  </>;
}

import { useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Popover, PopoverItem } from "../../ui";
import { setTaskParentLink, type TaskNode } from "../../lib/ipc";
import { invalidateTasks, useActionError } from "./actions";
import { canAssignParent, taskColor, type RelationView } from "./relations";

export function ParentGoalPicker({ task, relations, locked, nested }: {
  task: TaskNode; relations: RelationView; locked: boolean; nested: boolean;
}) {
  const anchorRef = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const [saving, setSaving] = useState(false);
  const { error, run } = useActionError();
  const qc = useQueryClient();
  const parent = task.parent_id ? relations.tasks.get(task.parent_id) : undefined;
  const candidates = [...relations.tasks.values()].filter((candidate) => canAssignParent(task, candidate, relations.cycles));
  const color = taskColor(task, relations.tasks);
  const parentLabel = relations.cycles.get(task.cycle_id)?.type === "week" ? "long-term goal" : "weekly goal";
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
      aria-label={parent ? `Change parent goal for ${task.title}` : `Link ${task.title} to ${parentLabel}`}
      title={parent ? `Linked to ${parent.title}` : `Link to a ${parentLabel}`}
      aria-haspopup="menu" aria-expanded={open}
      onMouseEnter={() => relations.preview(task.id)} onMouseLeave={() => relations.preview(null)}
      onClick={() => setOpen(!open)}>
      <span className="task-color-slot" style={color ? { backgroundColor: color, borderColor: color } : undefined} />
    </button>
    <Popover open={open} onClose={() => setOpen(false)} anchorRef={anchorRef} label={`Link ${parentLabel}`} className="w-[300px]">
      <div className="border-b border-light px-3 py-2 text-caption font-medium text-secondary">{nested ? "Part of a task" : `Link ${parentLabel}`}</div>
      {nested ? <p className="px-3 py-3 text-menu text-secondary">This step belongs to {parent?.title ?? "its parent task"}. Change ownership on the parent task.</p> : <>
        {candidates.length === 0 && <p className="px-3 py-3 text-menu text-secondary">No {parentLabel} available. This task can stay independent.</p>}
        <div className="max-h-60 overflow-y-auto">
          {candidates.map((candidate) => {
            const candidateColor = taskColor(candidate, relations.tasks);
            return <PopoverItem key={candidate.id} disabled={locked || saving}
              aria-label={`Link to ${candidate.title}`} onSelect={() => void choose(candidate.id)}>
              <span className="flex items-center gap-2">
                <span className="h-3.5 w-1.5 shrink-0 rounded-[3px] border" style={{ borderColor: candidateColor ?? "var(--border-control)", backgroundColor: candidateColor ?? "transparent" }} />
                <span className="min-w-0 flex-1 break-words">{candidate.title}</span>
                {task.parent_id === candidate.id && <span aria-label="Current parent">✓</span>}
              </span>
            </PopoverItem>;
          })}
        </div>
        {parent && <PopoverItem disabled={locked || saving} className="border-t border-light" onSelect={() => void choose(null)}>Unlink {parentLabel}</PopoverItem>}
      </>}
      <PopoverItem className="border-t border-light" onSelect={() => { relations.select(task.id); setOpen(false); }}>View connections</PopoverItem>
      {saving && <p role="status" className="px-3 py-2 text-caption text-secondary">Saving connection…</p>}
      {error && <p role="alert" className="px-3 py-2 text-caption text-danger">{error}</p>}
    </Popover>
  </>;
}

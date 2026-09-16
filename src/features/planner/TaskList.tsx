import { RemovalList } from "../../ui/RemovalList";
/**
 * 单列任务列表（design.md §5 任务编辑与条目状态）。
 *
 * 结构约定：
 * - 树来自 getEditorWorkspace（TaskNode 按 position 组装）；同周期子步骤以
 *   每层 20px 缩进表达（§4.3），跨周期链接不改变缩进（§5.1）。
 * - 列表尾保持一条「空行」——它是一条真实存在的空标题任务（后端契约：
 *   agent 新建目标会复用列表尾的空行，repository last_empty_visible_row
 *   只认 parent_id IS NULL 的空行），因此空行固定为顶层、不可缩进。
 * - 行内编辑：Enter 提交并在下方建新行聚焦；Tab/Shift+Tab 调整层级
 *   （set_task_parent_link，同周期自由嵌套）；中文输入法组合期间的
 *   Enter 只确认候选（e.nativeEvent.isComposing，§5.1）。
 * - 拖拽：固定把手触发 HTML5 DnD，在视觉兄弟组内排序，跨栏关联上级；
 *   先写 react-query 缓存（乐观），再调 reorder_tasks，失败回滚缓存并
 *   行内提示。
 * - 待确认行原位预览并锁定；确认入口仅在 Coach，删除线仅用于任务标题。
 */
import { useEffect, useLayoutEffect, useMemo, useRef, useState, type DragEvent, type KeyboardEvent, type ReactNode, type CSSProperties } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Checkbox, Popover, PopoverItem } from "../../ui";
import {
  addTask,
  getEditorWorkspace,
  LATER_CYCLE_ID,
  moveTask,
  patchTask,
  reorderTasks,
  ROOT_COLOR_KEYS,
  setTaskParentLink,
  setTaskRootColor,
  type CycleType,
  type EditorWorkspace,
  type RootColorKey,
  type TaskNode,
} from "../../lib/ipc";
import { qk } from "../../lib/events";
import { errorMessage, invalidateTasks, useActionError } from "./actions";

import { canAssignParent, ROOT_PALETTE, taskColor, type RelationView } from "./relations";
import { ParentGoalPicker } from "./ParentGoalPicker";
import { TASK_DRAG_TYPE, useTaskDrag } from "./TaskDragContext";
import { useTranslation } from "../../lib/i18n";
import { useTaskDeletion } from "./TaskDeletion";

/** 列表尾空行的判定（对齐 domain::task::is_empty_input_row 的可见部分）。 */
function isEmptyRow(task: { title: string; completed: boolean; children: TaskNode[] }): boolean {
  return task.title.trim() === "" && !task.completed && task.children.length === 0;
}

type VisibleRow = { kind: "task"; node: TaskNode; depth: number };


function flattenTasks(nodes: TaskNode[], depth: number, out: VisibleRow[]): void {
  for (const node of nodes) {
    out.push({ kind: "task", node, depth });
    flattenTasks(node.children, depth + 1, out);
  }
}

export function TaskList({
  active = true,
  cycleId,
  cycleType,
  locked,
  relations,
  onReviewIssues,
  revealTask,
  layout = "flow",
  children,
}: {
  /** Panel mode keeps the summary outside the task scrollport. */
  layout?: "flow" | "panel";
  children?: ReactNode;
  /** Retained, hidden editors must not create input rows or take focus. */
  revealTask?: {taskId:string;requestId:number};
  active?: boolean;
  cycleId: string;
  cycleType: CycleType;
  /** 已结束的周期只读（service 层 cycle_ended：ended pages are review-only）。 */
  locked: boolean;
  relations?: RelationView;
  onReviewIssues?: () => void;
}) {
  const { t } = useTranslation("planning");
  const qc = useQueryClient();
  const workspace = useQuery({
    queryKey: qk.editorWorkspace(cycleId),
    queryFn: () => getEditorWorkspace(cycleId),
  });


  const { error, run, fail, dismiss } = useActionError();
  const deletion = useTaskDeletion();
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [focusTarget, setFocusTarget] = useState<string | null>(null);
  const inputRefs = useRef(new Map<string, HTMLTextAreaElement>());
  const [drag, setDrag] = useTaskDrag();
  const [dropHint, setDropHint] = useState<{ id: string; before: boolean; kind: "reorder" | "link" } | null>(null);

  const tree = workspace.data?.tasks ?? [];
  const rows = useMemo(() => {
    const all: VisibleRow[] = [];
    flattenTasks(tree, 0, all);
    // One root typing affordance, even if concurrent editors left extra blanks.
    // Never hide a draft, a nested row, checklist content, or a proposal.
    const blanks = all.filter((row): row is VisibleRow => row.kind === "task" && row.node.parent_id === null
      && row.node.proposal == null && isEmptyRow(row.node) && !row.node.subtasks?.length
      && !drafts[row.node.id] && row.node.id !== focusTarget);
    const last = blanks[blanks.length - 1];
    const redundant = new Set(blanks.filter((row) => row !== last).map((row) => row.node.id));
    return all.filter((row) => row.kind !== "task" || !redundant.has(row.node.id));
  }, [tree, drafts, focusTarget]);
  const discardDraft = (id: string) => setDrafts((current) => {
    if (!(id in current)) return current;
    const next = { ...current };
    delete next[id];
    return next;
  });
  // Once a saved value returns, read from the shared query again. Keep a newer
  // unsaved draft if the user continued typing while the save was in flight.
  useEffect(() => {
    const saved: VisibleRow[] = [];
    flattenTasks(workspace.data?.tasks ?? [], 0, saved);
    setDrafts((current) => {
      const confirmed = saved.filter(({ node }) => node.proposal != null || current[node.id]?.trim() === node.title);
      if (!confirmed.length) return current;
      const next = { ...current };
      confirmed.forEach(({ node }) => delete next[node.id]);
      return next;
    });
  }, [workspace.data]);

  /* 尾部空行保障：可见列表没有空行时补一条（agent 复用契约的前提）。
   * 锁定周期跳过；并发防抖用 ref，失败静默（数据刷新后会重新评估）。 */
  const ensuringRef = useRef(false);
  const advancingRef = useRef(false);
  const cancelledBlurRef = useRef<string | null>(null);
  useEffect(() => {
    if (!active || !workspace.data || locked) return;
    if (workspace.data.tasks.some((t) => !t.proposal && isEmptyRow(t)) || ensuringRef.current || advancingRef.current) return;
    ensuringRef.current = true;
    addTask({ cycle_id: cycleId, title: "" })
      .then(() => invalidateTasks(qc, cycleId), (e: unknown) => fail(errorMessage(e)))
      .finally(() => {
        ensuringRef.current = false;
      });
  }, [active, workspace.data, cycleId, locked, qc, fail]);

  // Wait for this editor's own query and DOM, not the workspace's parallel query.
  const revealed = useRef<number | null>(null);
  useEffect(() => {
    if (!active || !revealTask || revealed.current === revealTask.requestId) return;
    const input = inputRefs.current.get(revealTask.taskId);
    if (!input) return;
    revealed.current = revealTask.requestId;
    relations?.select(revealTask.taskId);
    input.closest("[data-task-id]")?.scrollIntoView({block:"nearest",inline:"center",behavior:window.matchMedia("(prefers-reduced-motion: reduce)").matches?"instant":"smooth"});
    input.focus({preventScroll:true});
  }, [active, revealTask, rows, relations]);

  /* Enter 建行 / 重排后定位：目标行渲染出来即聚焦，避免竞态丢失焦点。 */
  useEffect(() => {
    if (!active || !focusTarget) return;
    const el = inputRefs.current.get(focusTarget);
    if (el) {
      el.focus();
      const end = el.value.length;
      el.setSelectionRange(end, end);
      inputRefs.current.delete(focusTarget);
      setFocusTarget(null);
    }
  }, [active, focusTarget, rows]);

  const currentFlat = (): VisibleRow[] => {
    const out: VisibleRow[] = [];
    flattenTasks(workspace.data?.tasks ?? [], 0, out);
    return out;
  };

  const commitTitle = async (node: TaskNode) => {
    if (node.proposal || locked) { discardDraft(node.id); return false; }
    const draft = (drafts[node.id] ?? node.title).trim();
    if (draft === node.title) { discardDraft(node.id); return true; }
    if (draft === "") {
      // 空标题后端必拒（invalid_title）：还原为原标题而不是清空用户文字。
      discardDraft(node.id);
      return true;
    }
    const ok = await run(() => patchTask(node.id, { title: draft }));
    if (ok) invalidateTasks(qc, cycleId);
    return ok !== null;
  };

  const commitAndAdvance = async (row: VisibleRow) => {
    if (advancingRef.current || !(drafts[row.node.id] ?? row.node.title).trim()) return;
    advancingRef.current = true;
    try {
      if (!(await commitTitle(row.node))) return;
      const flat = rows.filter((row): row is VisibleRow => row.kind === "task");
      const idx = flat.findIndex((r) => r.node.id === row.node.id);
      const next = idx >= 0 ? flat[idx + 1] : undefined;
      if (next) {
        setFocusTarget(next.node.id);
        return;
      }
      if (locked) return; // 只读周期不补行（add_task 会被 cycle_ended 拒绝）
      // 最后一行：在其下方补一条同层空行并聚焦（空行提交才成为任务）。
      const created = await run(() =>
        addTask({ cycle_id: cycleId, title: "", parent_id: flat.some((r) => r.node.id === row.node.parent_id) ? row.node.parent_id : null }),
      );
      if (created) {
        invalidateTasks(qc, cycleId);
        setFocusTarget(created.id);
      }
    } finally {
      advancingRef.current = false;
    }
  };

  const nest = async (row: VisibleRow) => {
    // Cross-cycle ownership does not make a root row visually nested.
    const siblings = currentFlat().filter((r) => row.depth === 0 ? r.depth === 0 : r.node.parent_id === row.node.parent_id);
    const idx = siblings.findIndex((r) => r.node.id === row.node.id);
    const previous = idx > 0 ? siblings[idx - 1] : undefined;
    if (!previous) return; // 没有上一个兄弟：无法成为其子步骤
    if (await run(() => setTaskParentLink(row.node.id, previous.node.id))) {
      invalidateTasks(qc, cycleId);
    }
  };

  const unnest = async (row: VisibleRow) => {
    if (row.node.parent_id === null) return; // 顶层 Shift+Tab：忽略
    const parent = currentFlat().find((r) => r.node.id === row.node.parent_id);
    if (!parent) return;
    if (await run(() => setTaskParentLink(row.node.id, parent.node.parent_id))) {
      invalidateTasks(qc, cycleId);
    }
  };

  const toggleCompleted = async (node: TaskNode, completed: boolean) => {
    if (isEmptyRow(node)) return; // 空行不进入完成态
    if (await run(() => patchTask(node.id, { completed }))) invalidateTasks(qc, cycleId);
  };

  const sendToLater = async (node: TaskNode) => {
    if (await run(() => moveTask(node.id, LATER_CYCLE_ID, null))) invalidateTasks(qc, cycleId);
  };

  const pickColor = async (taskId: string, colorKey: string | null) => {
    if (await run(() => setTaskRootColor(taskId, colorKey))) invalidateTasks(qc, cycleId);
  };

  /* --- 拖拽排序（7.8，乐观更新） ------------------------------------- */

  const resetDrag = () => {
    relations?.setDragging(false);
    setDrag(null);
    setDropHint(null);
  };

  useEffect(() => { if (!drag) setDropHint(null); }, [drag]);

  useEffect(() => {
    if (!drag || drag.task.cycle_id !== cycleId) return;
    if (!active || locked) { resetDrag(); return; }
    const cancel = () => resetDrag();
    const escape = (event: globalThis.KeyboardEvent) => { if (event.key === "Escape") cancel(); };
    window.addEventListener("dragend", cancel);
    window.addEventListener("keydown", escape);
    return () => {
      window.removeEventListener("dragend", cancel);
      window.removeEventListener("keydown", escape);
      relations?.setDragging(false);
      setDrag((current) => current?.task.id === drag.task.id ? null : current);
    };
  }, [drag, active, locked, cycleId]);

  const applyReorder = (parentId: string | null, orderedIds: string[]) => {
    const key = qk.editorWorkspace(cycleId);
    const snapshot = qc.getQueryData<EditorWorkspace>(key) ?? null;
    if (snapshot) {
      // 先改本地缓存（同 parent 兄弟重排），失败再回滚。
      qc.setQueryData<EditorWorkspace>(key, reorderWorkspace(snapshot, parentId, orderedIds));
    }
    reorderTasks(cycleId, parentId, orderedIds).then(
      () => invalidateTasks(qc, cycleId),
      (e: unknown) => {
        if (snapshot) qc.setQueryData<EditorWorkspace>(key, snapshot);
        fail(errorMessage(e));
      },
    );
  };

  const visualParent = (row: VisibleRow) => row.depth === 0 ? null : row.node.parent_id;
  const dropKind = (target: VisibleRow): "reorder" | "link" | null => {
    if (!drag || !active || locked || target.node.proposal || drag.task.id === target.node.id) return null;
    if (drag.task.cycle_id === cycleId) {
      if (drag.parentId !== visualParent(target)) return null;
      // A proposal's position is locked along with its contents.
      return currentFlat().some((row) => visualParent(row) === drag.parentId && row.node.proposal) ? null : "reorder";
    }
    const hasProposal = (task: TaskNode): boolean => !!task.proposal || task.children.some(hasProposal);
    return relations && drag.parentId === null && drag.task.parent_id !== target.node.id
      && !hasProposal(drag.task) && canAssignParent(drag.task, target.node, relations.cycles) ? "link" : null;
  };

  const onDropRow = (target: VisibleRow, before: boolean) => {
    const kind = dropKind(target);
    const source = drag;
    resetDrag();
    if (!source || !kind) return;
    if (kind === "link") {
      void run(async () => {
        await setTaskParentLink(source.task.id, target.node.id);
        invalidateTasks(qc, source.task.cycle_id);
        relations?.select(source.task.id);
      });
      return;
    }
    const dragId = source.task.id;
    const siblingIds = currentFlat().filter((row) => visualParent(row) === source.parentId).map((row) => row.node.id);
    const without = siblingIds.filter((id) => id !== dragId);
    const idx = without.indexOf(target.node.id);
    if (idx < 0) return;
    // The typing affordance stays last; dropping on it means append a task.
    if (isEmptyRow(target.node)) before = true;
    const insertAt = before ? idx : idx + 1;
    const ordered = [...without.slice(0, insertAt), dragId, ...without.slice(insertAt)];
    if (ordered.some((id, index) => id !== siblingIds[index])) applyReorder(source.parentId, ordered);
  };

  const allowColor = cycleType === "month";

  const summary = workspace.data?.work_mix && workspace.data.work_mix.total > 0 && (
    <div className={layout === "panel" ? "shrink-0 border-t border-light px-6 py-4 text-caption text-hint" : "mx-5 mt-4 border-t border-light pt-3 text-caption text-hint"} aria-label={t("task.workMixLabel")}
      title={t("task.workMixTitle")}>
      <p className="mb-1 font-medium text-secondary">{t("task.workMix", { count: workspace.data.work_mix.total })}</p>
      <div className="flex flex-wrap gap-x-3 gap-y-1">
        <span>{t("task.withLongTerm", { count: workspace.data.work_mix.long_term })}</span>
        <span>{t("task.independentWeekly", { count: workspace.data.work_mix.weekly_standalone })}</span>
        {cycleType === "day" && <span>{t("task.independentDaily", { count: workspace.data.work_mix.daily_standalone })}</span>}
        {workspace.data.work_mix.unresolved > 0 && <span>{t("task.unresolved", { count: workspace.data.work_mix.unresolved })}</span>}
      </div>
      <p className="mt-1">{t("task.withoutLinks", { percent: Math.round(100 * (workspace.data.work_mix.weekly_standalone + workspace.data.work_mix.daily_standalone) / workspace.data.work_mix.total) })}</p>
    </div>
  );

  const content = (
    <div className="flex flex-col" data-task-list={cycleId} onDragLeave={(event) => {
      if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropHint(null);
    }}>
      {locked && (
        <p className="px-2 pb-1 text-caption text-hint">
          {t("workspace.ended")}
        </p>
      )}
      <RemovalList>{rows.map((row) =>
        (
          <TaskRow
            key={row.node.id}
            active={active}
            row={row}
            locked={locked || row.node.proposal != null}
            allowColor={allowColor}
            relations={relations}
            onReviewIssues={onReviewIssues}
            draft={row.node.proposal ? row.node.title : drafts[row.node.id] ?? row.node.title}
            dragging={drag?.task.id === row.node.id}
            dropBefore={drag && dropHint?.id === row.node.id && dropHint.kind === "reorder" ? dropHint.before : null}
            linkHint={!!drag && dropHint?.id === row.node.id && dropHint.kind === "link"}
            placeholder={cycleType === "month" ? t("task.addGoal") : t("task.addTask")}
            onDraftChange={(value) => setDrafts((d) => ({ ...d, [row.node.id]: value }))}
            onInputRef={(el) => {
              if (el) inputRefs.current.set(row.node.id, el);
              else inputRefs.current.delete(row.node.id);
            }}
            onKeyDown={(e) => {
              if (locked || row.node.proposal) return;
              if (e.key === "Enter") {
                if (e.nativeEvent.isComposing) return; // IME 组合期间只确认候选
                e.preventDefault();
                void commitAndAdvance(row);
              } else if (e.key === "Tab" && !locked) {
                if (isEmptyRow(row.node)) {
                  // Empty rows stay at the root; Tab retains normal focus navigation.
                  return;
                }
                e.preventDefault();
                if (e.shiftKey) void unnest(row);
                else void nest(row);
              } else if (e.key === "Escape") {
                if (e.nativeEvent.isComposing) return;
                e.preventDefault();
                cancelledBlurRef.current = row.node.id;
                discardDraft(row.node.id);
                e.currentTarget.blur();
              }
            }}
            onBlur={() => {
              if (cancelledBlurRef.current === row.node.id) { cancelledBlurRef.current = null; return; }
              void commitTitle(row.node);
            }}
            onToggle={(checked) => void toggleCompleted(row.node, checked)}
            onDelete={() => void deletion.requestDelete(row.node)}
            onSendToLater={() => void sendToLater(row.node)}
            onPickColor={(key) => void pickColor(row.node.id, key)}
            onDragStart={(e) => {
              e.dataTransfer.setData(TASK_DRAG_TYPE, row.node.id);
              e.dataTransfer.effectAllowed = "move";
              const element = e.currentTarget.closest<HTMLElement>("[data-task-id]");
              if (element) e.dataTransfer.setDragImage(element, 12, 16);
              setDrag({ task: { ...row.node, cycle_id: cycleId }, parentId: visualParent(row) });
              relations?.setDragging(true);
            }}
            onDragEnd={resetDrag}
            onHandleKeyDown={(event) => {
              if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
              event.preventDefault();
              const parentId = visualParent(row);
              const siblings = currentFlat().filter((item) => visualParent(item) === parentId);
              if (siblings.some((item) => item.node.proposal)) return;
              const index = siblings.findIndex((item) => item.node.id === row.node.id);
              const next = index + (event.key === "ArrowUp" ? -1 : 1);
              const neighbour = siblings[next];
              if (index < 0 || !neighbour || isEmptyRow(neighbour.node)) return;
              const ordered = siblings.map((item) => item.node.id);
              ordered[index] = neighbour.node.id;
              ordered[next] = row.node.id;
              applyReorder(parentId, ordered);
            }}
            onDragOver={(e) => {
              const kind = e.dataTransfer.types.includes(TASK_DRAG_TYPE) ? dropKind(row) : null;
              if (!kind) { setDropHint(null); return; }
              e.preventDefault();
              e.dataTransfer.dropEffect = "move";
              const rect = e.currentTarget.getBoundingClientRect();
              const before = isEmptyRow(row.node) || e.clientY < rect.top + rect.height / 2;
              setDropHint((prev) =>
                prev?.id === row.node.id && prev.before === before && prev.kind === kind ? prev : { id: row.node.id, before, kind },
              );
            }}
            onDrop={(e) => {
              if (!drag || e.dataTransfer.getData(TASK_DRAG_TYPE) !== drag.task.id) return;
              e.preventDefault();
              const rect = e.currentTarget.getBoundingClientRect();
              onDropRow(row, e.clientY < rect.top + rect.height / 2);
            }}
          />
        ),
      )}
      {layout === "flow" && summary && <div key="work-mix">{summary}</div>}
      </RemovalList>
      {workspace.isError && (
        <p role="alert" className="px-2 py-1 text-caption text-danger">
          {errorMessage(workspace.error)}
        </p>
      )}
      {deletion.dialog}
      {(error || deletion.error) && (
        <p role="alert" className="flex items-center gap-2 px-2 py-1 text-caption text-danger">
          {error || deletion.error}
          <button type="button" className="underline" onClick={() => { dismiss(); deletion.dismiss(); }}>
            {t("task.dismiss")}
          </button>
        </p>
      )}
    </div>
  );

  return layout === "panel" ? (
    <>
      <div data-plan-scroll data-workspace-scroll-pane className="min-h-0 flex-auto overflow-y-auto px-4 py-5">
        {content}
        {children}
      </div>
      <RemovalList className="shrink-0">{summary}</RemovalList>
    </>
  ) : <>{content}{children}</>;
}

/* --- 行组件 -------------------------------------------------------------- */

function TaskRow({
  active,
  row,
  locked,
  allowColor,
  relations,
  onReviewIssues,
  draft,
  dragging,
  dropBefore,
  linkHint,
  placeholder,
  onDraftChange,
  onInputRef,
  onKeyDown,
  onBlur,
  onToggle,
  onDelete,
  onSendToLater,
  onPickColor,
  onDragStart,
  onDragEnd,
  onHandleKeyDown,
  onDragOver,
  onDrop,
}: {
  active: boolean;
  row: VisibleRow;
  locked: boolean;
  allowColor: boolean;
  relations?: RelationView;
  onReviewIssues?: () => void;
  draft: string;
  dragging: boolean;
  dropBefore: boolean | null;
  linkHint: boolean;
  placeholder: string;
  onDraftChange: (value: string) => void;
  onInputRef: (el: HTMLTextAreaElement | null) => void;
  onKeyDown: (e: KeyboardEvent<HTMLTextAreaElement>) => void;
  onBlur: () => void;
  onToggle: (checked: boolean) => void;
  onDelete: () => void;
  onSendToLater: () => void;
  onPickColor: (colorKey: string | null) => void;
  onDragStart: (e: DragEvent<HTMLSpanElement>) => void;
  onDragEnd: () => void;
  onHandleKeyDown: (e: KeyboardEvent<HTMLSpanElement>) => void;
  onDragOver: (e: DragEvent<HTMLDivElement>) => void;
  onDrop: (e: DragEvent<HTMLDivElement>) => void;
}) {
  const { t } = useTranslation("planning");
  const titleRef = useRef<HTMLTextAreaElement>(null);
  useLayoutEffect(() => {
    const field = titleRef.current;
    if (!field) return;
    field.style.height = "0px";
    field.style.height = `${Math.max(28, field.scrollHeight)}px`;
  }, [draft, active]);
  const node = row.node;
  const empty = isEmptyRow(node);
  const color = relations ? taskColor(node, relations.tasks) : null;
  const highlighted = relations?.highlighted.has(node.id) ?? false;
  const hints = [node.needs_refinement === true ? t("task.clarify") : null, node.needs_breakdown === true ? t("task.breakdown") : null].filter(Boolean).join(" · ");
  return (
    <div
      data-task-id={node.id}
      data-proposal={node.proposal ?? undefined}
      title={node.proposal ? t("task.previewLockedCn") : undefined}
      data-related={highlighted || undefined}
      data-selected={relations?.selectedId === node.id || undefined}
      onClick={(event) => {
        if (!empty && !(event.target as HTMLElement).closest("button, [role=menu]")) relations?.select(node.id);
      }}
      onDragOver={locked ? undefined : onDragOver}
      onDrop={locked ? undefined : onDrop}
      className={[
        "task-row group relative flex items-start gap-1 rounded-md py-1 pr-1",
        "transition-colors duration-150",
        node.proposal ? "bg-focus-surface/60 ring-1 ring-inset ring-focus/15" : "hover:bg-hover",
        dragging ? "opacity-50" : "",
        linkHint ? "bg-focus-surface ring-1 ring-inset ring-focus" : "",
      ].join(" ")}
      style={{ marginLeft: row.depth * 20, "--task-color": color ?? "var(--color-focus)" } as CSSProperties}
    >
      {dropBefore && <InsertLine position="top" />}
      {linkHint && <span role="status" className="pointer-events-none absolute bottom-full left-4 z-20 mb-1 max-w-full truncate rounded-md bg-focus px-2 py-1 text-caption text-white shadow-sm">{t("task.linkTo", { title: node.title })}</span>}
      {/* The handle is draggable before pointer-down; the editable row never is. */}
      {empty || locked ? (
        <span className="w-4 shrink-0" aria-hidden="true" />
      ) : (
        <span
          role="button"
          tabIndex={active ? 0 : -1}
          aria-label={t("task.reorder", { title: node.title || t("task.reorderRow") })}
          title={t("task.reorderTitle")}
          draggable={active}
          onDragStart={onDragStart}
          onDragEnd={onDragEnd}
          onKeyDown={onHandleKeyDown}
          onClick={(e) => { e.preventDefault(); e.stopPropagation(); }}
          className="task-drag-handle flex h-7 w-4 shrink-0 cursor-grab items-center justify-center rounded-sm text-hint opacity-0 transition-opacity duration-100 group-hover:opacity-100 focus-visible:opacity-100 active:cursor-grabbing"
        >
          <svg viewBox="0 0 12 16" className="pointer-events-none h-3.5 w-3" aria-hidden="true">
            <circle cx="3" cy="4" r="1.1" fill="currentColor" />
            <circle cx="9" cy="4" r="1.1" fill="currentColor" />
            <circle cx="3" cy="8" r="1.1" fill="currentColor" />
            <circle cx="9" cy="8" r="1.1" fill="currentColor" />
            <circle cx="3" cy="12" r="1.1" fill="currentColor" />
            <circle cx="9" cy="12" r="1.1" fill="currentColor" />
          </svg>
        </span>
      )}
      <Checkbox
        checked={node.completed}
        disabled={locked || empty}
        onChange={onToggle}
        aria-label={empty ? undefined : t("task.markComplete", { title: node.title })}
        className="task-check shrink-0"
      />
      {!empty && allowColor && row.depth === 0 ? <ColorSlotButton task={node} disabled={locked} onPick={onPickColor} relations={relations} /> :
        !empty && relations && (!allowColor || row.depth > 0) ? <ParentGoalPicker task={node} relations={relations} locked={locked} nested={row.depth > 0} /> :
        <span className="task-color-control" aria-hidden="true" />}
      <textarea
        rows={1}
        ref={(element) => { titleRef.current = element; onInputRef(element); }}
        value={draft}
        readOnly={locked}
        placeholder={placeholder}
        aria-label={empty ? placeholder : undefined}
        onChange={(e) => onDraftChange(e.target.value)}
        onKeyDown={onKeyDown}
        onBlur={onBlur}
        className={[
          "min-w-0 flex-1 resize-none overflow-hidden rounded-sm bg-transparent px-1 py-0 text-body !leading-7 outline-none",
          /* 完成态：删除线 + 提示色——弱化是有意的可读性取舍（spec: 弱化文本）。 */
          node.proposal === "delete" ? "text-secondary line-through decoration-current/40" : node.completed ? "text-hint line-through" : "text-primary",
        ].join(" ")}
      />
      {node.proposal && <span className="mr-1 flex h-7 shrink-0 items-center gap-1 text-[11px] text-secondary">
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true"><rect x="5" y="10" width="14" height="11" rx="2"/><path d="M8 10V7a4 4 0 0 1 8 0v3"/></svg>
        {node.proposal === "delete" ? t("task.proposalDelete") : t("task.proposalPreview")}
      </span>}
      {!empty && !node.proposal && hints && <button type="button" className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-hint transition-colors hover:bg-hover hover:text-secondary"
        aria-label={t("task.reviewHints", { title: node.title })} title={hints} onClick={onReviewIssues}>
        <svg viewBox="0 0 16 16" className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth="1.25" aria-hidden="true"><path d="M4 14V2h8l-2 3 2 3H4" strokeLinecap="round" strokeLinejoin="round" /></svg>
      </button>}
      {!empty && !locked && (
        /* 预览任务沿用相同布局，仅锁定编辑控件。 */
        <span className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity duration-100 group-hover:opacity-100 group-focus-within:opacity-100">
          <IconButton label={t("task.moveLater", { title: node.title })} title={t("task.moveLaterTitle")} onClick={onSendToLater}>
            <svg viewBox="0 0 16 16" className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
              <circle cx="8" cy="8" r="6.2" />
              <path d="M8 4.8V8l2.2 1.6" />
            </svg>
          </IconButton>
          <IconButton label={t("task.delete", { title: node.title })} title={t("cycle.delete")} onClick={onDelete}>
            <svg viewBox="0 0 16 16" className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
              <path d="M3 4.5h10M6.5 4.5V3h3v1.5M4.5 4.5l.6 8h5.8l.6-8M6.8 7v3.5M9.2 7v3.5" />
            </svg>
          </IconButton>
        </span>
      )}
      {dropBefore === false && <InsertLine position="bottom" />}
    </div>
  );
}

function InsertLine({ position }: { position: "top" | "bottom" }) {
  return (
    <span
      aria-hidden="true"
      className={`absolute ${position === "top" ? "-top-px" : "-bottom-px"} left-0 right-0 h-0.5 rounded-full bg-accent`}
    />
  );
}

function IconButton({
  label,
  title,
  onClick,
  children,
}: {
  label: string;
  title: string;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={title}
      onClick={onClick}
      className="flex h-7 w-7 items-center justify-center rounded-sm text-secondary transition-colors duration-100 hover:bg-hover"
    >
      {children}
    </button>
  );
}

/** 长期目标色槽（§4.3：6×13px、3px 圆角；空槽只有控件边界色）。 */
function ColorSlotButton({
  task,
  disabled,
  onPick,
  relations,
}: {
  task: TaskNode;
  disabled: boolean;
  onPick: (colorKey: string | null) => void;
  relations?: RelationView;
}) {
  const { t } = useTranslation("planning");
  const anchorRef = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const colorKey = task.root_color_key as RootColorKey | null;
  const solid = colorKey !== null && colorKey in ROOT_PALETTE ? ROOT_PALETTE[colorKey] : null;
  const colorLabel = (key: RootColorKey): string => t(`task.color.${key}`);
  return (
    <span className="relative inline-flex shrink-0">
      <button
        ref={anchorRef}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={solid ? `${t("task.goalColor")}: ${colorLabel(colorKey as RootColorKey)}` : t("task.setGoalColor")}
        title={t("task.goalColor")}
        onMouseEnter={() => relations?.preview(task.id)}
        onMouseLeave={() => relations?.preview(null)}
        onClick={() => setOpen((o) => !o)}
        className="task-color-control"
      ><span className="task-color-slot" style={solid ? { backgroundColor: solid, borderColor: solid } : undefined} /></button>
      <Popover open={open} onClose={() => setOpen(false)} anchorRef={anchorRef} label={t("task.goalColor")}>
        {ROOT_COLOR_KEYS.map((key) => (
          <PopoverItem
            key={key}
            disabled={disabled}
            aria-label={colorLabel(key)}
            className="flex items-center gap-2 capitalize"
            onSelect={() => {
              setOpen(false);
              onPick(key);
            }}
          >
            <span
              aria-hidden="true"
              className="inline-block h-3 w-3 rounded-[3px]"
              style={{ backgroundColor: ROOT_PALETTE[key] }}
            />
            <span className="flex-1">{colorLabel(key)}</span>
            {key === colorKey && <span aria-label={t("task.currentColor")}>✓</span>}
          </PopoverItem>
        ))}
        <PopoverItem
          disabled={disabled}
          className="flex items-center gap-2"
          aria-label={t("task.noColor")}
          onSelect={() => {
            setOpen(false);
            onPick(null);
          }}
        >
          <span className="flex-1">{t("task.noColor")}</span>
          {!solid && <span aria-label={t("task.currentColor")}>✓</span>}
        </PopoverItem>
        {relations && <PopoverItem className="border-t border-light" onSelect={() => { relations.select(task.id); setOpen(false); }}>{t("task.viewConnections")}</PopoverItem>}
      </Popover>
    </span>
  );
}

/** A pending change stays in the task rhythm; only the title is struck out. */
/** 乐观重排：按 ordered_ids 重排指定兄弟组，组外行保持原位。 */
function reorderWorkspace(
  ws: EditorWorkspace,
  parentId: string | null,
  ordered: string[],
): EditorWorkspace {
  return { ...ws, tasks: reorderGroup(ws.tasks, parentId, ordered) };
}

function reorderGroup(nodes: TaskNode[], parentId: string | null, ordered: string[]): TaskNode[] {
  if (parentId === null) return orderByIds(nodes, ordered);
  return nodes.map((node) =>
    node.id === parentId
      ? { ...node, children: orderByIds(node.children, ordered) }
      : { ...node, children: reorderGroup(node.children, parentId, ordered) },
  );
}

function orderByIds(nodes: TaskNode[], ordered: string[]): TaskNode[] {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const next: TaskNode[] = [];
  for (const id of ordered) {
    const node = byId.get(id);
    if (node) {
      next.push(node);
      byId.delete(id);
    }
  }
  // 快照中不在 ordered 里的行（并发新建等）保留在尾部，不丢行。
  next.push(...byId.values());
  return next;
}

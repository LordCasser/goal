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
 * - 拖拽（任务 7.8）：把手触发 HTML5 DnD，只在同 parent 兄弟组内重排；
 *   先写 react-query 缓存（乐观），再调 reorder_tasks，失败回滚缓存并
 *   行内提示。
 * - 待确认高亮（7.7 列内部分）：get_preview_summary 的行以 bg-proposal /
 *   删除提议的浅红底渲染，附 Keep/Revert；确认前内容仍可读。
 */
import { useEffect, useMemo, useRef, useState, type DragEvent, type KeyboardEvent, type ReactNode } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Button, Checkbox, Popover, PopoverItem, ProgressDot } from "../../ui";
import {
  addTask,
  deleteTask,
  getEditorWorkspace,
  getPreviewSummary,
  keepTaskPreview,
  LATER_CYCLE_ID,
  moveTask,
  patchTask,
  reorderTasks,
  ROOT_COLOR_KEYS,
  setTaskParentLink,
  setTaskRootColor,
  undoTaskPreview,
  type CycleType,
  type EditorWorkspace,
  type ProposalKind,
  type RootColorKey,
  type Task,
  type TaskNode,
} from "../../lib/ipc";
import { qk } from "../../lib/events";
import { errorMessage, invalidateTasks, useActionError } from "./actions";

/**
 * 目标归属色的 8 色调色板（ROOT_COLOR_KEYS）。这些是随任务持久化的
 * 领域数据色（design.md §6「关系色」），不属于主题 chrome——主题 token
 * 只覆盖表面/文字/边界，因此这里按色名取值而不走 CSS 变量；选中与焦点
 * 等界面状态仍全部用语义 token。
 */
const ROOT_PALETTE: Record<RootColorKey, string> = {
  red: "#c0392b",
  amber: "#c87d1a",
  gold: "#b39321",
  green: "#3d8763",
  teal: "#2c8686",
  blue: "#3a6ea5",
  indigo: "#5a5fc7",
  plum: "#9552a5",
};

/** 列表尾空行的判定（对齐 domain::task::is_empty_input_row 的可见部分）。 */
function isEmptyRow(task: { title: string; completed: boolean; children: TaskNode[] }): boolean {
  return task.title.trim() === "" && !task.completed && task.children.length === 0;
}

type VisibleRow = { kind: "task"; node: TaskNode; depth: number };
type PreviewRow = { kind: "preview"; task: Task; depth: number };
type Row = VisibleRow | PreviewRow;

function flattenTasks(nodes: TaskNode[], depth: number, out: VisibleRow[]): void {
  for (const node of nodes) {
    out.push({ kind: "task", node, depth });
    flattenTasks(node.children, depth + 1, out);
  }
}

export function TaskList({
  cycleId,
  cycleType,
  locked,
}: {
  cycleId: string;
  cycleType: CycleType;
  /** 已结束的周期只读（service 层 cycle_ended：ended pages are review-only）。 */
  locked: boolean;
}) {
  const qc = useQueryClient();
  const workspace = useQuery({
    queryKey: qk.editorWorkspace(cycleId),
    queryFn: () => getEditorWorkspace(cycleId),
  });
  const previews = useQuery({
    queryKey: qk.previewSummary(cycleId),
    queryFn: () => getPreviewSummary(cycleId),
  });

  const { error, run, fail, dismiss } = useActionError();
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [focusTarget, setFocusTarget] = useState<string | null>(null);
  const inputRefs = useRef(new Map<string, HTMLInputElement>());
  // DnD 状态：把手 mousedown 才把行置为 draggable；draggingId 驱动半透明，
  // dropHint 驱动插入指示线。
  const [draggableId, setDraggableId] = useState<string | null>(null);
  const [draggingId, setDraggingId] = useState<string | null>(null);
  const [dropHint, setDropHint] = useState<{ id: string; before: boolean } | null>(null);

  const tree = workspace.data?.tasks ?? [];
  const rows = useMemo(() => buildRows(tree, previews.data?.tasks ?? []), [tree, previews.data]);

  /* 尾部空行保障：可见列表没有空行时补一条（agent 复用契约的前提）。
   * 锁定周期跳过；并发防抖用 ref，失败静默（数据刷新后会重新评估）。 */
  const ensuringRef = useRef(false);
  useEffect(() => {
    if (!workspace.data || locked) return;
    if (workspace.data.tasks.some((t) => isEmptyRow(t)) || ensuringRef.current) return;
    ensuringRef.current = true;
    addTask({ cycle_id: cycleId, title: "" })
      .then(() => invalidateTasks(qc, cycleId), () => {})
      .finally(() => {
        ensuringRef.current = false;
      });
  }, [workspace.data, cycleId, locked, qc]);

  /* Enter 建行 / 重排后定位：目标行渲染出来即聚焦，避免竞态丢失焦点。 */
  useEffect(() => {
    if (!focusTarget) return;
    const el = inputRefs.current.get(focusTarget);
    if (el) {
      el.focus();
      const end = el.value.length;
      el.setSelectionRange(end, end);
      inputRefs.current.delete(focusTarget);
      setFocusTarget(null);
    }
  }, [focusTarget, rows]);

  const currentFlat = (): VisibleRow[] => {
    const out: VisibleRow[] = [];
    flattenTasks(workspace.data?.tasks ?? [], 0, out);
    return out;
  };

  const commitTitle = async (node: TaskNode) => {
    const draft = (drafts[node.id] ?? node.title).trim();
    if (draft === node.title) return true;
    if (draft === "") {
      // 空标题后端必拒（invalid_title）：还原为原标题而不是清空用户文字。
      setDrafts((d) => ({ ...d, [node.id]: node.title }));
      return true;
    }
    const ok = await run(() => patchTask(node.id, { title: draft }));
    if (ok) invalidateTasks(qc, cycleId);
    return ok !== null;
  };

  const commitAndAdvance = async (row: VisibleRow) => {
    await commitTitle(row.node);
    const flat = currentFlat();
    const idx = flat.findIndex((r) => r.node.id === row.node.id);
    const next = idx >= 0 ? flat[idx + 1] : undefined;
    if (next) {
      setFocusTarget(next.node.id);
      return;
    }
    if (locked) return; // 只读周期不补行（add_task 会被 cycle_ended 拒绝）
    // 最后一行：在其下方补一条同层空行并聚焦（空行提交才成为任务）。
    const created = await run(() =>
      addTask({ cycle_id: cycleId, title: "", parent_id: row.node.parent_id }),
    );
    if (created) {
      invalidateTasks(qc, cycleId);
      setFocusTarget(created.id);
    }
  };

  const nest = async (row: VisibleRow) => {
    const siblings = currentFlat().filter((r) => r.node.parent_id === row.node.parent_id);
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

  const removeTask = async (node: TaskNode) => {
    if (await run(() => deleteTask(node.id))) invalidateTasks(qc, cycleId);
  };

  const sendToLater = async (node: TaskNode) => {
    if (await run(() => moveTask(node.id, LATER_CYCLE_ID, null))) invalidateTasks(qc, cycleId);
  };

  const keepPreview = async (taskId: string) => {
    if (await run(() => keepTaskPreview(taskId))) invalidateTasks(qc, cycleId);
  };

  const revertPreview = async (taskId: string) => {
    if (await run(() => undoTaskPreview(taskId))) invalidateTasks(qc, cycleId);
  };

  const pickColor = async (taskId: string, colorKey: string | null) => {
    if (await run(() => setTaskRootColor(taskId, colorKey))) invalidateTasks(qc, cycleId);
  };

  /* --- 拖拽排序（7.8，乐观更新） ------------------------------------- */

  const resetDrag = () => {
    setDraggingId(null);
    setDropHint(null);
    setDraggableId(null);
  };

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

  const onDropRow = (targetId: string, before: boolean) => {
    const dragId = draggingId;
    resetDrag();
    if (!dragId || dragId === targetId) return;
    const flat = currentFlat();
    const drag = flat.find((r) => r.node.id === dragId);
    const target = flat.find((r) => r.node.id === targetId);
    if (!drag || !target) return;
    if (drag.node.parent_id !== target.node.parent_id) return; // 只重排同 parent 兄弟
    const siblingIds = flat
      .filter((r) => r.node.parent_id === drag.node.parent_id)
      .map((r) => r.node.id);
    const without = siblingIds.filter((id) => id !== dragId);
    const idx = without.indexOf(targetId);
    if (idx < 0) return;
    const insertAt = before ? idx : idx + 1;
    applyReorder(
      drag.node.parent_id,
      [...without.slice(0, insertAt), dragId, ...without.slice(insertAt)],
    );
  };

  const allowColor = cycleType === "month";

  return (
    <div className="flex flex-col" data-task-list={cycleId}>
      {locked && (
        <p className="px-2 pb-1 text-caption text-hint">
          This page has ended and can only be reviewed.
        </p>
      )}
      {rows.map((row) =>
        row.kind === "task" && row.node.proposal == null ? (
          <TaskRow
            key={row.node.id}
            row={row}
            locked={locked}
            allowColor={allowColor}
            draft={drafts[row.node.id] ?? row.node.title}
            dragging={draggingId === row.node.id}
            draggable={draggableId === row.node.id}
            dropBefore={dropHint?.id === row.node.id ? dropHint.before : null}
            placeholder={cycleType === "month" ? "Add a goal…" : "Add a task…"}
            onDraftChange={(value) => setDrafts((d) => ({ ...d, [row.node.id]: value }))}
            onInputRef={(el) => {
              if (el) inputRefs.current.set(row.node.id, el);
              else inputRefs.current.delete(row.node.id);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                if (e.nativeEvent.isComposing) return; // IME 组合期间只确认候选
                e.preventDefault();
                void commitAndAdvance(row);
              } else if (e.key === "Tab" && !locked) {
                if (isEmptyRow(row.node)) {
                  e.preventDefault(); // 尾部空行必须保持顶层（agent 复用契约）
                  return;
                }
                e.preventDefault();
                if (e.shiftKey) void unnest(row);
                else void nest(row);
              } else if (e.key === "Escape") {
                e.preventDefault();
                setDrafts((d) => ({ ...d, [row.node.id]: row.node.title }));
                e.currentTarget.blur();
              }
            }}
            onBlur={() => void commitTitle(row.node)}
            onToggle={(checked) => void toggleCompleted(row.node, checked)}
            onDelete={() => void removeTask(row.node)}
            onSendToLater={() => void sendToLater(row.node)}
            onPickColor={(key) => void pickColor(row.node.id, key)}
            onDragStart={(e) => {
              e.dataTransfer.setData("text/plain", row.node.id);
              e.dataTransfer.effectAllowed = "move";
              setDraggingId(row.node.id);
            }}
            onDragEnd={resetDrag}
            onDragOver={(e) => {
              if (!draggingId || draggingId === row.node.id) return;
              e.preventDefault();
              e.dataTransfer.dropEffect = "move";
              const rect = e.currentTarget.getBoundingClientRect();
              const before = e.clientY < rect.top + rect.height / 2;
              setDropHint((prev) =>
                prev?.id === row.node.id && prev.before === before ? prev : { id: row.node.id, before },
              );
            }}
            onDrop={(e) => {
              e.preventDefault();
              const rect = e.currentTarget.getBoundingClientRect();
              onDropRow(row.node.id, e.clientY < rect.top + rect.height / 2);
            }}
            onHandleDown={() => {
              if (!locked && !isEmptyRow(row.node)) setDraggableId(row.node.id);
            }}
          />
        ) : row.kind === "task" ? (
          /* 编辑树理论上只含已确认行；若后端契约调整让树携带 proposal，按待确认样式渲染。 */
          <ProposalRow
            key={row.node.id}
            title={row.node.title}
            proposal={row.node.proposal}
            completed={row.node.completed}
            depth={row.depth}
            onKeep={() => void keepPreview(row.node.id)}
            onRevert={() => void revertPreview(row.node.id)}
          />
        ) : (
          <ProposalRow
            key={row.task.id}
            title={row.task.title}
            proposal={row.task.proposal}
            completed={row.task.completed}
            depth={row.depth}
            onKeep={() => void keepPreview(row.task.id)}
            onRevert={() => void revertPreview(row.task.id)}
          />
        ),
      )}
      {workspace.isError && (
        <p role="alert" className="px-2 py-1 text-caption text-danger">
          {errorMessage(workspace.error)}
        </p>
      )}
      {error && (
        <p role="alert" className="flex items-center gap-2 px-2 py-1 text-caption text-danger">
          {error}
          <button type="button" className="underline" onClick={dismiss}>
            Dismiss
          </button>
        </p>
      )}
    </div>
  );
}

/* --- 行组件 -------------------------------------------------------------- */

function TaskRow({
  row,
  locked,
  allowColor,
  draft,
  dragging,
  draggable,
  dropBefore,
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
  onDragOver,
  onDrop,
  onHandleDown,
}: {
  row: VisibleRow;
  locked: boolean;
  allowColor: boolean;
  draft: string;
  dragging: boolean;
  draggable: boolean;
  dropBefore: boolean | null;
  placeholder: string;
  onDraftChange: (value: string) => void;
  onInputRef: (el: HTMLInputElement | null) => void;
  onKeyDown: (e: KeyboardEvent<HTMLInputElement>) => void;
  onBlur: () => void;
  onToggle: (checked: boolean) => void;
  onDelete: () => void;
  onSendToLater: () => void;
  onPickColor: (colorKey: string | null) => void;
  onDragStart: (e: DragEvent<HTMLDivElement>) => void;
  onDragEnd: () => void;
  onDragOver: (e: DragEvent<HTMLDivElement>) => void;
  onDrop: (e: DragEvent<HTMLDivElement>) => void;
  onHandleDown: () => void;
}) {
  const node = row.node;
  const empty = isEmptyRow(node);
  return (
    <div
      data-task-id={node.id}
      draggable={draggable}
      onDragStart={onDragStart}
      onDragEnd={onDragEnd}
      onDragOver={onDragOver}
      onDrop={onDrop}
      className={[
        "group relative flex items-start gap-1 rounded-sm py-0.5 pr-1",
        "transition-colors duration-100 hover:bg-hover",
        dragging ? "opacity-50" : "",
      ].join(" ")}
      style={{ marginLeft: row.depth * 20 }}
    >
      {dropBefore && <InsertLine position="top" />}
      {/* 拖拽把手：按下才让整行可拖（正文选择、输入框排除，§5.1）。 */}
      {empty || locked ? (
        <span className="w-4 shrink-0" aria-hidden="true" />
      ) : (
        <button
          type="button"
          aria-label={`Reorder ${node.title || "row"}`}
          title="Drag to reorder"
          onMouseDown={onHandleDown}
          onClick={(e) => e.preventDefault()}
          className="mt-1 flex h-6 w-4 shrink-0 cursor-grab items-center justify-center rounded-sm text-hint opacity-0 transition-opacity duration-100 group-hover:opacity-100 focus-visible:opacity-100 active:cursor-grabbing"
        >
          <svg viewBox="0 0 12 16" className="h-3.5 w-3" aria-hidden="true">
            <circle cx="3" cy="4" r="1.1" fill="currentColor" />
            <circle cx="9" cy="4" r="1.1" fill="currentColor" />
            <circle cx="3" cy="8" r="1.1" fill="currentColor" />
            <circle cx="9" cy="8" r="1.1" fill="currentColor" />
            <circle cx="3" cy="12" r="1.1" fill="currentColor" />
            <circle cx="9" cy="12" r="1.1" fill="currentColor" />
          </svg>
        </button>
      )}
      <Checkbox
        checked={node.completed}
        disabled={locked || empty}
        onChange={onToggle}
        aria-label={empty ? undefined : `Mark “${node.title}” complete`}
        className="mt-0.5 shrink-0"
      />
      {allowColor && <ColorSlotButton task={node} disabled={locked} onPick={onPickColor} />}
      {/* 清晰度标记（三态中的 true 才提示；null/false 不显示，spec 语义）。 */}
      {node.needs_refinement === true && (
        <ProgressDot tone="alert" className="mt-[9px]" title="Needs refinement" aria-label="Needs refinement" />
      )}
      {node.needs_breakdown === true && (
        <ProgressDot tone="alert" className="mt-[9px]" title="Needs breakdown" aria-label="Needs breakdown" />
      )}
      <input
        ref={onInputRef}
        value={draft}
        readOnly={locked}
        placeholder={placeholder}
        aria-label={empty ? placeholder : undefined}
        onChange={(e) => onDraftChange(e.target.value)}
        onKeyDown={onKeyDown}
        onBlur={onBlur}
        className={[
          "min-w-0 flex-1 rounded-sm bg-transparent px-1 text-body outline-none",
          /* 完成态：删除线 + 提示色——弱化是有意的可读性取舍（spec: 弱化文本）。 */
          node.completed ? "text-hint line-through" : "text-primary",
        ].join(" ")}
      />
      {!empty && !locked && (
        /* 行内动作 hover / 键盘焦点显现（§10）；提议行由 ProposalRow 承担。 */
        <span className="flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity duration-100 group-hover:opacity-100 focus-within:opacity-100">
          <IconButton label={`Move “${node.title}” to Later`} title="Move to Later" onClick={onSendToLater}>
            <svg viewBox="0 0 16 16" className="h-3.5 w-3.5" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
              <circle cx="8" cy="8" r="6.2" />
              <path d="M8 4.8V8l2.2 1.6" />
            </svg>
          </IconButton>
          <IconButton label={`Delete “${node.title}”`} title="Delete" onClick={onDelete}>
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
}: {
  task: TaskNode;
  disabled: boolean;
  onPick: (colorKey: string | null) => void;
}) {
  const anchorRef = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const colorKey = task.root_color_key as RootColorKey | null;
  const solid = colorKey !== null && colorKey in ROOT_PALETTE ? ROOT_PALETTE[colorKey] : null;
  return (
    <span className="relative mt-[4px] inline-flex shrink-0">
      <button
        ref={anchorRef}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={solid ? `Goal color: ${colorKey}` : "Set goal color"}
        title="Goal color"
        disabled={disabled}
        onClick={() => setOpen((o) => !o)}
        className="h-[13px] w-1.5 rounded-[3px] border"
        style={
          solid
            ? { backgroundColor: solid, borderColor: solid }
            : { borderColor: "var(--border-control)" }
        }
      />
      <Popover open={open} onClose={() => setOpen(false)} anchorRef={anchorRef} label="Goal color">
        {ROOT_COLOR_KEYS.map((key) => (
          <PopoverItem
            key={key}
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
            {key}
          </PopoverItem>
        ))}
        <PopoverItem
          onSelect={() => {
            setOpen(false);
            onPick(null);
          }}
        >
          No color
        </PopoverItem>
      </Popover>
    </span>
  );
}

/**
 * 待确认行（7.7 的列内部分）：upsert 用待确认底色；delete 用危险浅底 +
 * 删除线 + “proposes deletion” 说明。内容保持可读，Keep/Revert 常驻可
 * 发现，不能只靠 hover（§8.3）。
 */
function ProposalRow({
  title,
  proposal,
  completed,
  depth,
  onKeep,
  onRevert,
}: {
  title: string;
  proposal: ProposalKind | null;
  completed: boolean;
  depth: number;
  onKeep: () => void;
  onRevert: () => void;
}) {
  const kind: ProposalKind = proposal ?? "upsert";
  return (
    <div
      data-proposal={kind}
      className={[
        "relative my-0.5 flex items-start gap-2 rounded-sm border-l-2 py-1 pr-1",
        kind === "delete" ? "border-l-danger bg-danger-surface" : "border-l-accent bg-proposal",
      ].join(" ")}
      style={{ marginLeft: depth * 20 }}
    >
      <ProgressDot tone={kind === "delete" ? "alert" : "idle"} className="mt-[9px] ml-1" />
      <p className={["min-w-0 flex-1 text-body text-primary", kind === "delete" || completed ? "line-through" : ""].join(" ")}>
        {title}
        {kind === "delete" && (
          <span className="ml-2 text-caption text-danger">proposes deletion</span>
        )}
        {kind !== "delete" && (
          <span className="ml-2 text-caption text-secondary">proposed change</span>
        )}
      </p>
      <span className="flex shrink-0 items-center gap-1 py-0.5">
        <Button size="compact" onClick={onKeep}>
          Keep
        </Button>
        <Button size="compact" onClick={onRevert}>
          Revert
        </Button>
      </span>
    </div>
  );
}

/* --- 纯工具 -------------------------------------------------------------- */

/** 可见树 + 待确认行合并为渲染行；待确认行挂在可见父行子树之后，无父则置顶。 */
function buildRows(tree: TaskNode[], previewTasks: Task[]): Row[] {
  const visible: VisibleRow[] = [];
  flattenTasks(tree, 0, visible);
  const rows: Row[] = [...visible];
  for (const task of [...previewTasks].sort((a, b) => a.position - b.position)) {
    let insertAt = rows.length;
    let depth = 0;
    const parentIdx = rows.findIndex((r) => r.kind === "task" && r.node.id === task.parent_id);
    const parentRow = parentIdx >= 0 ? rows[parentIdx] : undefined;
    if (parentRow) {
      const parentDepth = parentRow.depth;
      let end = parentIdx + 1;
      while (end < rows.length) {
        const candidate = rows[end];
        if (!candidate || candidate.depth <= parentDepth) break;
        end++;
      }
      insertAt = end;
      depth = parentDepth + 1;
    }
    rows.splice(insertAt, 0, { kind: "preview", task, depth });
  }
  return rows;
}

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

/**
 * Do Later 侧栏（rebuild-baseline 7.6）：先记录、后安排的暂存区。
 *
 * 顶部一个输入框连续提交 addLaterGoal，下方把 LATER_CYCLE_ID 工作区的
 * 任务树平铺渲染。面板 424px（w-panel）、内边距 16px、右侧一条轻边界与
 * 主板分隔（design.md 3.2/3.3）。首次打开的解释卡经 app flag 持久化关闭
 * 状态（openspec planner-workspace「一次性提示卡」）；列表为空时只有一行
 * 轻提示——可直接记录，不要求填日期（design.md 9.1）。
 */
import { useState, type KeyboardEvent } from "react";
import type * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  addLaterGoal,
  getAppFlag,
  getEditorWorkspace,
  getPlannerState,
  LATER_CYCLE_ID,
  setAppFlag,
  type TaskNode,
} from "../../lib/ipc";
import { qk } from "../../lib/events";
import { Button, Input } from "../../ui";
import { LaterTaskRow } from "./LaterTaskRow";

/** app_settings 里的 flag key；值为 "dismissed" 后解释卡不再出现。 */
export const LATER_HINT_KEY = "hint.later-explainer";

export function LaterPanel({ onClose }: { onClose: () => void }): React.JSX.Element {
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState("");
  const [hintHidden, setHintHidden] = useState(false);
  const [focusTaskId, setFocusTaskId] = useState<string | null>(null);

  const workspace = useQuery({
    queryKey: qk.editorWorkspace(LATER_CYCLE_ID),
    queryFn: () => getEditorWorkspace(LATER_CYCLE_ID),
  });
  // 规划状态只为 Promote 提供长期（month）周期候选。
  const planner = useQuery({ queryKey: qk.plannerState(), queryFn: getPlannerState });
  // 自定 key：qk 工厂没有 app-flag 形状，这个 key 只有本面板消费。
  const hintFlag = useQuery({
    queryKey: ["app-flag", LATER_HINT_KEY],
    queryFn: () => getAppFlag(LATER_HINT_KEY),
  });

  // Later 容器自身在 Rust 侧以 month 类型播种，这里要从候选里排除。
  const monthCycles = (planner.data?.cycles ?? []).filter(
    (cycle) => cycle.type === "month" && cycle.id !== LATER_CYCLE_ID,
  );

  const addGoal = useMutation({
    mutationFn: (title: string) => addLaterGoal(title),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: qk.editorWorkspace(LATER_CYCLE_ID),
      });
    },
  });
  const dismissHint = useMutation({
    mutationFn: () => setAppFlag(LATER_HINT_KEY, "dismissed"),
    // 本地先隐藏，失败也不会把卡片弹回来打断输入。
    onMutate: () => setHintHidden(true),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["app-flag", LATER_HINT_KEY] });
    },
  });

  const submitDraft = () => {
    const title = draft.trim();
    if (!title) return;
    // 立即清空以便连续记录（openspec planner-workspace「快速记录」）。
    setDraft("");
    addGoal.mutate(title);
  };

  const onQuickAddKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key !== "Enter" || e.nativeEvent.isComposing) return;
    submitDraft();
  };

  // 面板内 Escape 关面板；行内 Popover 在捕获阶段先消费自己的 Escape
  // （design.md 6.2 的退出顺序）。输入法组合中的 Escape 不当作退出。
  const onPanelKeyDown = (e: KeyboardEvent<HTMLElement>) => {
    if (e.key !== "Escape" || e.nativeEvent.isComposing) return;
    onClose();
  };

  const rows = flattenRows(workspace.data?.tasks ?? []);
  const showHint = !hintHidden && hintFlag.isSuccess && hintFlag.data !== "dismissed";

  return (
    <aside
      aria-label="Do Later"
      onKeyDown={onPanelKeyDown}
      className="flex h-full w-panel shrink-0 flex-col border-r border-light bg-content"
    >
      <header className="flex items-center justify-between px-4 pb-3 pt-4">
        <h2 className="text-caption font-semibold uppercase tracking-[0.08em] text-secondary">
          Later
        </h2>
        <Button
          variant="ghost"
          size="compact"
          className="h-7 w-7 justify-center px-0"
          onClick={onClose}
          aria-label="Close Later panel"
          title="Close (Esc)"
        >
          <CloseIcon />
        </Button>
      </header>
      <div className="px-4 pb-3">
        <Input
          autoFocus
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={onQuickAddKeyDown}
          placeholder="Note it down, schedule later"
          aria-label="New parked goal"
        />
      </div>
      {showHint && (
        <section className="mx-4 mb-3 rounded-sm border border-light bg-subtle p-3">
          <header className="flex items-start justify-between gap-2">
            <h3 className="text-caption font-semibold uppercase tracking-[0.08em] text-secondary">
              Park now, plan later
            </h3>
            <Button
              variant="ghost"
              size="compact"
              className="h-7 w-7 justify-center px-0"
              onClick={() => dismissHint.mutate()}
              aria-label="Dismiss hint"
              title="Won't show again"
            >
              <CloseIcon />
            </Button>
          </header>
          <p className="mt-1 text-menu text-secondary">
            Capture a goal the moment it shows up — nothing here needs a date or
            a plan. Promote parked items into a long-term cycle when you are
            ready.
          </p>
        </section>
      )}
      {/* 列表区在面板内纵向滚动（design.md 3.3 适配规则）。 */}
      <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
        {rows.length === 0 ? (
          // 空状态只留一行轻提示：Later 面板窄，不用大 EmptyState（9.1）。
          <p className="pt-2 text-body text-hint">
            Nothing parked yet — note it down above. No date or plan needed.
          </p>
        ) : (
          rows.map(({ task, depth }) => (
            <LaterTaskRow
              key={task.id}
              task={task}
              depth={depth}
              monthCycles={monthCycles}
              autoFocusTitle={task.id === focusTaskId}
              onRowCreated={setFocusTaskId}
            />
          ))
        )}
      </div>
    </aside>
  );
}

/** 深度优先平铺任务树；depth 驱动子步骤缩进。 */
function flattenRows(
  nodes: TaskNode[],
  depth = 0,
): Array<{ task: TaskNode; depth: number }> {
  return nodes.flatMap((node) => [
    { task: node, depth },
    ...flattenRows(node.children, depth + 1),
  ]);
}

/** 细线几何图标：16px、1.5 描边（design.md 4.3 图标语言）。 */
function CloseIcon() {
  return (
    <svg
      viewBox="0 0 16 16"
      className="h-4 w-4"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      aria-hidden="true"
    >
      <path d="M4 4l8 8M12 4l-8 8" />
    </svg>
  );
}

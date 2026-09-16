/**
 * Do Later 暂存项的一行（design.md 3.2 Later 行、5.1 任务条目）。
 *
 * 完成框 + 行内标题编辑 + hover/键盘关注时显露的 Promote 与删除。标题是
 * 无边线的行内编辑器（ui/Input 自带控件边框，不适合此形态，这里用语义
 * token 自绘）；Enter 在下方再开一条空行保持连续输入，失焦提交标题。
 * 输入法组合期间的 Enter 只确认候选，不触发提交（design.md 5.1）。
 */
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  addTask,
  patchTask,
  promoteLaterGoal,
  LATER_CYCLE_ID,
  type Cycle,
  type TaskNode,
} from "../../lib/ipc";
import { qk } from "../../lib/events";
import { Button, Checkbox, Popover, PopoverItem, cn } from "../../ui";
import { errorMessage, useTranslation } from "../../lib/i18n";
import { useTaskDeletion } from "../planner/TaskDeletion";

export type LaterTaskRowProps = {
  task: TaskNode;
  /** 0 = 顶层暂存目标；同周期子步骤每层缩进约 20px（design.md 4.3）。 */
  depth: number;
  /** 长期类型 Promote 的候选目标，不含 Later 容器自身。 */
  monthCycles: Cycle[];
  /** 新建行落座后把焦点移入标题，连续输入不中断（design.md 5.1）。 */
  autoFocusTitle: boolean;
  /** Enter 新建行成功后把新任务 id 报回面板，用于聚焦那一行。 */
  onRowCreated: (taskId: string) => void;
};

export function LaterTaskRow({
  task,
  depth,
  monthCycles,
  autoFocusTitle,
  onRowCreated,
}: LaterTaskRowProps) {
  const { t } = useTranslation("planning");
  const queryClient = useQueryClient();
  const deletion = useTaskDeletion();
  const preview = task.proposal != null;
  const [title, setTitle] = useState(task.title);
  const [promoteOpen, setPromoteOpen] = useState(false);
  const promoteAnchor = useRef<HTMLSpanElement>(null);
  const laterPlanType = task.later_plan_type ?? "month";
  const planTypeLabel = laterPlanType === "week"
    ? t("later.type.week")
    : laterPlanType === "day"
      ? t("later.type.day")
      : t("later.type.longTerm");
  const promoteLabel = laterPlanType === "week"
    ? t("later.scheduleWeek")
    : laterPlanType === "day"
      ? t("later.scheduleDay")
      : t("later.promote");

  // 提交后的数据回流同步标题；正在输入的草稿不会被触碰——task.title 没变
  // 时这个 effect 不执行（design.md 5.2「编辑中」）。
  useEffect(() => {
    setTitle(task.title);
  }, [task.id, task.title]);

  const invalidateLater = () => {
    void queryClient.invalidateQueries({ queryKey: qk.editorWorkspace(LATER_CYCLE_ID) });
  };

  const commitTitle = useMutation({
    mutationFn: (next: string) => patchTask(task.id, { title: next }),
    onSuccess: invalidateLater,
  });
  const toggleCompleted = useMutation({
    mutationFn: (completed: boolean) => patchTask(task.id, { completed }),
    onSuccess: invalidateLater,
  });
  const addBelow = useMutation({
    // add_task 不校验空标题：Enter 只是“在下方再开一条可继续输入的空行”，
    // 与顶部的 add_later_goal（拒绝空标题）是两条不同路径。
    mutationFn: () =>
      addTask({
        cycle_id: LATER_CYCLE_ID,
        title: "",
        parent_id: depth > 0 ? task.parent_id : null,
        position: task.position + 1,
      }),
    onSuccess: (created) => {
      invalidateLater();
      onRowCreated(created.id);
    },
  });
  const promote = useMutation({
    mutationFn: (targetCycleId: string | null) => promoteLaterGoal(task.id, targetCycleId),
    onSuccess: (movedTask) => {
      // 任务离开 Later 进入目标周期，两边的查询都要失效。
      void queryClient.invalidateQueries({ queryKey: qk.editorWorkspace(LATER_CYCLE_ID) });
      if (movedTask?.cycle_id) {
        void queryClient.invalidateQueries({ queryKey: qk.editorWorkspace(movedTask.cycle_id) });
      }
      void queryClient.invalidateQueries({ queryKey: qk.plannerState() });
    },
  });

  /** 失焦/Enter 提交标题；空标题不发送（后端拒绝），恢复为已保存值。 */
  const commit = () => {
    if (preview || promote.isPending) return;
    const next = title.trim();
    if (!next || next === task.title) {
      setTitle(task.title);
      return;
    }
    commitTitle.mutate(next);
  };

  const onTitleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (preview || promote.isPending || e.key !== "Enter" || e.nativeEvent.isComposing) return;
    commit();
    addBelow.mutate();
  };

  /** 只有一个长期周期时直接提升；多个用小菜单点名目标（design.md 9.2）。 */
  const onPromote = () => {
    if (laterPlanType === "week" || laterPlanType === "day") {
      promote.mutate(null);
      return;
    }
    if (monthCycles.length === 1) {
      const sole = monthCycles[0];
      if (sole) promote.mutate(sole.id);
      return;
    }
    setPromoteOpen((open) => !open);
  };

  return (
    <div
      data-proposal={task.proposal ?? undefined}
      title={preview ? t("later.previewLocked") : undefined}
      className={cn("group rounded-md py-0.5", preview && "bg-focus-surface/60 ring-1 ring-inset ring-focus/15")}
      style={{ paddingLeft: depth * 20 }}
    >
      <div className="flex items-start gap-1">
        <Checkbox
          checked={task.completed}
          onChange={(completed) => toggleCompleted.mutate(completed)}
          disabled={preview || toggleCompleted.isPending || promote.isPending}
          aria-label={task.title ? t("later.complete", { title: task.title }) : t("later.completeTask")}
        />
        {/* 行内标题编辑器：无边线，完成态用提示文字色 + 删除线（design.md 5.2）。 */}
        <input
          autoFocus={autoFocusTitle}
          value={preview ? task.title : title}
          readOnly={preview || promote.isPending}
          onChange={(e) => setTitle(e.target.value)}
          onKeyDown={onTitleKeyDown}
          onBlur={commit}
          aria-label={t("later.taskTitle")}
          placeholder={depth === 0 ? t("later.newGoal") : t("later.newStep")}
          className={cn(
            "h-7 min-w-0 flex-1 rounded-sm bg-transparent px-1 text-body text-primary",
            "placeholder:text-hint",
            (task.completed || task.proposal === "delete") && "text-secondary line-through",
          )}
        />
        <span className="shrink-0 px-1 text-[11px] leading-7 text-secondary">{planTypeLabel}</span>
        {preview && <span className="h-7 shrink-0 pr-2 text-[11px] leading-7 text-secondary">{task.proposal === "delete" ? t("later.previewDelete") : t("later.preview")}</span>}
        {/* 行内动作在 hover 或键盘进入条目时显露（design.md 5.1/10）。 */}
        <div className={preview ? "hidden" : "flex shrink-0 items-center gap-0.5 opacity-0 transition-opacity duration-100 group-hover:opacity-100 group-focus-within:opacity-100"}>
          <span ref={promoteAnchor} className="inline-flex">
            <Button
              variant="ghost"
              size="icon"
              onClick={onPromote}
              disabled={preview || (laterPlanType === "month" && monthCycles.length === 0) || promote.isPending}
              aria-haspopup={laterPlanType === "month" && monthCycles.length > 1 ? "menu" : undefined}
              aria-expanded={laterPlanType === "month" && monthCycles.length > 1 ? promoteOpen : undefined}
              aria-label={promoteLabel}
              title={promoteLabel}
            >
              <PromoteIcon />
            </Button>
          </span>
          <Button
            variant="ghost"
            size="icon"
            onClick={() => void deletion.requestDelete(task)}
            disabled={preview || deletion.busy || promote.isPending}
            aria-label={t("later.delete")}
            title={t("later.delete")}
          >
            <DeleteIcon />
          </Button>
        </div>
      </div>
      {deletion.dialog}
      {deletion.error && <p role="alert" className="text-caption text-danger">{deletion.error}</p>}
      {promote.error && <p role="alert" className="text-caption text-danger">{errorMessage(promote.error)}</p>}
      <Popover
        open={!preview && laterPlanType === "month" && promoteOpen}
        onClose={() => setPromoteOpen(false)}
        anchorRef={promoteAnchor}
        label={t("later.promoteMenu")}
      >
        {monthCycles.map((cycle) => (
          <PopoverItem
            key={cycle.id}
            onSelect={() => {
              setPromoteOpen(false);
              promote.mutate(cycle.id);
            }}
          >
            {cycle.title}
          </PopoverItem>
        ))}
      </Popover>
    </div>
  );
}

/** 细线几何图标：16px、1.5 描边（design.md 4.3 图标语言）。 */
function PromoteIcon() {
  return (
    <svg
      viewBox="0 0 16 16"
      className="h-4 w-4 shrink-0"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M4.5 11.5 11.5 4.5" />
      <path d="M6 4.5h5.5V10" />
    </svg>
  );
}

function DeleteIcon() {
  return (
    <svg
      viewBox="0 0 16 16"
      className="h-4 w-4 shrink-0"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M3 4.5h10" />
      <path d="M6.5 4.5V3h3v1.5" />
      <path d="M4.5 4.5 5.2 13h5.6l.7-8.5" />
    </svg>
  );
}

/**
 * 计划问题面板（openspec planning-issues「问题报告」，design.md §8.1/8.2）。
 *
 * 与 Agent 侧栏复用 PanelShell 外壳（424px、标题 + 关闭）。只读展示、不
 * 阻塞编辑：顶部说明行点明「不评分，只指出具体问题」（spec: 诊断而非评
 * 分），每条 PlanningIssue 一行——类型 label + detail，hover 显现忽略入口。
 * 忽略带可选原因下拉（产品可用性信号，spec: 收集产品自身的可用性信号，
 * 原因用常量字符串原样上报），成功后本地失效 issueReport 查询；后端报告
 * 已过滤被忽略项，前端不重复过滤。
 *
 * AI 未配置时语义审查在后端静默跳过（spec: 边写边审「审查不可用」）——本
 * 面板只呈现结构结果，无需对不可用做特殊降级，也把“没有问题”渲染为一行
 * 轻提示而不是大空状态。
 */
import { useRef, useState } from "react";
import type * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { qk } from "../../lib/events";
import {
  dismissPlanningIssue,
  getPlanningIssueReport,
  isAppError,
  type PlanningIssue,
} from "../../lib/ipc";
import { Popover, PopoverItem, cn } from "../../ui";
import { PanelShell } from "./PanelShell";

export type IssuePanelProps = {
  cycleId: string;
  onClose: () => void;
};

/** issue_type → 中文 label（写死映射，spec: 问题类型六类）。 */
const ISSUE_TYPE_LABELS: Record<PlanningIssue["issue_type"], string> = {
  too_many_goals: "目标过多",
  too_many_tasks: "任务过多",
  too_much_work: "工作量过大",
  not_sure_what_to_do_next: "不清楚下一步",
  missing_something: "缺少必要的东西",
  not_useful_for_needs: "对当前需求没用",
};

/** 可上报的忽略原因常量；原样落库，不做翻译（spec: 可用性信号）。 */
const REASON_TOO_MUCH_WORK = "Planning felt like too much work";
const REASON_NOT_SURE_HOW = "Not sure how to use it";

/** 原因下拉三选：不选原因 = 纯忽略动作。 */
const DISMISS_REASONS: Array<{ label: string; reason: string | null }> = [
  { label: "直接忽略", reason: null },
  { label: REASON_TOO_MUCH_WORK, reason: REASON_TOO_MUCH_WORK },
  { label: REASON_NOT_SURE_HOW, reason: REASON_NOT_SURE_HOW },
];

export function IssuePanel({ cycleId, onClose }: IssuePanelProps): React.JSX.Element {
  const queryClient = useQueryClient();

  // 结构结果即时返回；语义审查在后端内部发生（不可用即静默跳过），这里
  // 用 refresh=false 的已算报告，不因打开面板而阻塞编辑。
  const report = useQuery({
    queryKey: qk.issueReport(cycleId),
    queryFn: () => getPlanningIssueReport(cycleId, false),
    enabled: cycleId !== "",
  });

  const dismiss = useMutation({
    mutationFn: (input: { issueType: string; taskId: string | null; reason: string | null }) =>
      dismissPlanningIssue(cycleId, input.issueType, input.taskId, input.reason ?? undefined),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: qk.issueReport(cycleId) });
    },
  });

  const issues = report.data ?? [];

  return (
    <PanelShell label="Plan issues" title="计划问题" onClose={onClose}>
      <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
        <p className="pb-3 text-caption text-secondary">不评分，只指出具体问题。</p>
        {report.isPending ? (
          <p className="text-caption text-hint">正在读取问题报告…</p>
        ) : issues.length === 0 ? (
          // 空报告只留一行轻提示，不用大空状态（问题入口是提示不是功能块）。
          <p className="text-body text-hint">没有发现计划问题。</p>
        ) : (
          <ul className="flex flex-col">
            {issues.map((issue) => (
              <IssueRow
                key={`${issue.issue_type}:${issue.task_id ?? ""}`}
                issue={issue}
                dismissPending={dismiss.isPending}
                onDismiss={(reason) =>
                  dismiss.mutate({ issueType: issue.issue_type, taskId: issue.task_id, reason })
                }
              />
            ))}
          </ul>
        )}
        {report.isError && (
          <p className="text-caption text-danger">{errorText(report.error)}</p>
        )}
        {dismiss.isError && (
          <p className="text-caption text-danger">{errorText(dismiss.error)}</p>
        )}
      </div>
    </PanelShell>
  );
}

/** 一条问题：类型 label + detail；忽略入口 hover / 聚焦时显现（8.1 就地提示）。 */
function IssueRow({
  issue,
  dismissPending,
  onDismiss,
}: {
  issue: PlanningIssue;
  dismissPending: boolean;
  onDismiss: (reason: string | null) => void;
}) {
  const anchorRef = useRef<HTMLButtonElement>(null);
  const [reasonOpen, setReasonOpen] = useState(false);

  return (
    <li className="group flex items-start gap-3 border-b border-light py-2.5">
      <div className="min-w-0 flex-1">
        <p className="text-block-title font-semibold text-primary">
          {ISSUE_TYPE_LABELS[issue.issue_type] ?? issue.issue_type}
        </p>
        <p className="mt-0.5 text-menu text-secondary">{issue.detail}</p>
      </div>
      <div className="shrink-0 opacity-0 transition-opacity duration-100 group-focus-within:opacity-100 group-hover:opacity-100">
        <button
          ref={anchorRef}
          type="button"
          aria-haspopup="menu"
          aria-expanded={reasonOpen}
          disabled={dismissPending}
          onClick={() => setReasonOpen(true)}
          className={cn(
            "h-7 rounded-sm border border-control px-2 text-caption text-primary",
            "transition-colors duration-100 hover:bg-hover",
            "disabled:cursor-not-allowed disabled:opacity-45",
          )}
        >
          忽略
        </button>
        <Popover
          open={reasonOpen}
          onClose={() => setReasonOpen(false)}
          anchorRef={anchorRef}
          label="忽略原因"
        >
          {DISMISS_REASONS.map(({ label, reason }) => (
            <PopoverItem
              key={label}
              onSelect={() => {
                setReasonOpen(false);
                onDismiss(reason);
              }}
            >
              {label}
            </PopoverItem>
          ))}
        </Popover>
      </div>
    </li>
  );
}

function errorText(error: unknown): string {
  return isAppError(error)
    ? error.message
    : error instanceof Error
      ? error.message
      : String(error);
}

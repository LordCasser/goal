/**
 * 待确认改动底栏（rebuild-baseline 7.7，design.md 8.3）。
 *
 * 汇总全部周期的 preview 计数；有待确认项时在工作台底部占位渲染——
 * 不悬浮、不遮内容（design.md 3.1/3.2「待确认栏」）。批量 Keep/Revert
 * 明确作用于全部受影响周期：按钮 hover title 说明范围，单周期时文案点名
 * 周期（8.3 第 2 条）。计数为 0 时返回 null，不占空间；这里的存在与
 * Agent 侧栏无关——关侧栏、切周期都不隐式确认提议（8.3 第 3 条）。
 */
import type * as React from "react";
import {
  useMutation,
  useQueries,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import {
  getPlannerState,
  getPreviewSummary,
  keepAllPreviews,
  undoAllPreviews,
} from "../../lib/ipc";
import { qk } from "../../lib/events";
import { Button } from "../../ui";

/** design.md 8.3：批量动作必须声明作用范围，不靠侧栏位置猜。 */
const SCOPE_HINT = "Applies to all cycles with pending edits";

export function ProposalsBar(): React.JSX.Element | null {
  const queryClient = useQueryClient();
  const planner = useQuery({ queryKey: qk.plannerState(), queryFn: getPlannerState });
  const cycles = planner.data?.cycles ?? [];

  // plannerState 的周期集合含 Later 容器（Rust 以 month 类型播种）——它不会
  // 有 pending 提议，多取一次 summary 换来“全部周期都算数”的简单口径。
  const summaries = useQueries({
    queries: cycles.map((cycle) => ({
      queryKey: qk.previewSummary(cycle.id),
      queryFn: () => getPreviewSummary(cycle.id),
    })),
  });

  const pending = summaries.flatMap((result, index) => {
    const cycle = cycles[index];
    const count = result.data?.count ?? 0;
    return cycle && count > 0 ? [{ id: cycle.id, title: cycle.title, count }] : [];
  });
  const total = pending.reduce((sum, entry) => sum + entry.count, 0);

  const invalidateSummaries = () => {
    for (const cycle of cycles) {
      void queryClient.invalidateQueries({ queryKey: qk.previewSummary(cycle.id) });
    }
  };

  const keep = useMutation({
    mutationFn: async (cycleIds: string[]) => {
      await Promise.all(cycleIds.map((cycleId) => keepAllPreviews(cycleId)));
    },
    onSuccess: invalidateSummaries,
  });
  const revert = useMutation({
    mutationFn: async (cycleIds: string[]) => {
      await Promise.all(cycleIds.map((cycleId) => undoAllPreviews(cycleId)));
    },
    onSuccess: invalidateSummaries,
  });

  // 查询未回来时按 0 处理（渲染 null），回来后有待确认项自然出现。
  if (total === 0) return null;

  const busy = keep.isPending || revert.isPending;
  const pendingIds = pending.map((entry) => entry.id);
  // 单周期时点名周期；多周期时总数本身就是跨周期口径。
  const soleTitle = pending.length === 1 ? pending[0]?.title : undefined;

  return (
    <footer className="flex h-12 shrink-0 items-center justify-between gap-4 border-y border-light bg-proposal px-4">
      <p className="min-w-0 truncate text-body text-primary">
        You have {total} pending edit{total === 1 ? "" : "s"} by agent
        {soleTitle ? ` in ${soleTitle}` : ""}
      </p>
      <div className="flex shrink-0 items-center gap-2">
        <Button
          variant="secondary"
          size="compact"
          title={SCOPE_HINT}
          loading={revert.isPending}
          disabled={busy}
          onClick={() => revert.mutate(pendingIds)}
        >
          Revert all
        </Button>
        <Button
          variant="primary"
          size="compact"
          title={SCOPE_HINT}
          loading={keep.isPending}
          disabled={busy}
          onClick={() => keep.mutate(pendingIds)}
        >
          Keep all
        </Button>
      </div>
    </footer>
  );
}

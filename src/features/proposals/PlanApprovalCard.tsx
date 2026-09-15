import { invoke } from "@tauri-apps/api/core";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { getPlannerState, getPreviewSummary, type Json, type Subtask, type Task, type TaskSnapshot } from "../../lib/ipc";
import { invalidateAgentEffects, qk } from "../../lib/events";
import { Button } from "../../ui";
import { errorMessage, invalidateTasks } from "../planner/actions";

// Approval Card layout adapted from UI by Halaska (MIT; public/licenses/halaska-ui.txt).
// Decisions resolve actual task previews; there are no demo timers or optimistic receipts.
export function PlanApprovalCard({ cycleId, disabled = false }: { cycleId: string; disabled?: boolean }) {
  const cycles=useQuery({queryKey:["preview-summary","pending-cycles"],queryFn:()=>invoke<string[]>("get_pending_task_cycles")});
  return <><CycleApprovalCard cycleId={cycleId} sourceCycleId={cycleId} disabled={disabled}/>{(cycles.data??[]).filter(id=>id!==cycleId).map(id=><CycleApprovalCard key={id} cycleId={id} sourceCycleId={cycleId} disabled={disabled}/>)}</>;
}

function CycleApprovalCard({ cycleId, sourceCycleId, disabled = false }: { cycleId: string; sourceCycleId:string; disabled?: boolean }) {
  const client = useQueryClient();
  const preview = useQuery({ queryKey: qk.previewSummary(cycleId), queryFn: () => getPreviewSummary(cycleId), enabled: Boolean(cycleId) });
  const planner = useQuery({ queryKey: qk.plannerState(), queryFn: getPlannerState });
  const cycle = planner.data?.cycles?.find((item) => item.id === cycleId);
  const decision = useMutation({
    mutationFn: async ({ task, keep }: { task: Task; keep: boolean }) => {
      await invoke("resolve_coach_task_preview",{sourceCycleId,taskId:task.id,approve:keep});
      return { task, keep };
    },
    onSettled: async () => {
      invalidateTasks(client, cycleId);
      invalidateAgentEffects(client);
      await client.invalidateQueries({ queryKey: qk.previewSummary(cycleId) });
    },
  });
  const tasks = preview.data?.tasks ?? [];
  if (preview.isError) return <p role="alert" className="mb-3 text-caption text-danger">待确认改动加载失败：{errorMessage(preview.error)}</p>;
  if (!tasks.length) return null;
  const busy = disabled || decision.isPending;

  return (
    <section aria-label="待确认的计划改动" className="coach-approval mb-3 overflow-hidden rounded-xl border border-light bg-content">
      <header className="flex items-center justify-between gap-2 bg-subtle px-3 py-2.5">
        <div className="min-w-0">
          <p className="text-caption font-medium text-primary">计划预览 · 等待确认</p>
          <p className="truncate text-caption text-secondary">{cycle?.title ?? "当前计划"}{cycle?.starts_on ? ` · ${cycle.starts_on}` : ""}</p>
        </div>
        <span className="shrink-0 text-caption text-hint">{tasks.length} 项</span>
      </header>
      <div className="max-h-[260px] overflow-y-auto">
        {tasks.map((task) => {
          const original = preview.data?.originals?.[task.id];
          const kind = task.proposal === "delete" ? "删除" : original && (!original.original_exists || !original.title?.trim()) ? "新增" : "修改";
          const changes = original ? detailChanges(original, task) : [];
          return <article key={task.id} aria-label={`${kind}：${task.title}`} className="border-t border-light px-3 py-3 first:border-t-0">
            <p className="mb-1 text-caption text-secondary">{kind}</p>
            {kind === "修改" && original?.title !== task.title && <p className="mb-1 break-words text-caption text-hint"><del>{original?.title}</del></p>}
            <p className="break-words text-body font-medium text-primary">{task.title}</p>
            {(preview.data?.deletion_impacts?.[task.id]?.length ?? 0) > 0 && <p className="mt-2 break-words text-caption text-secondary">同时删除关联事务：{preview.data!.deletion_impacts[task.id]!.join("、")}</p>}
            {changes.length > 0 && <dl className="mt-2 space-y-2 text-caption">
              {changes.map(([label, before, after]) => <div key={label}>
                <dt className="text-secondary">{label}</dt>
                {before && <dd className="whitespace-pre-wrap break-words text-hint"><del>{before}</del></dd>}
                <dd className="whitespace-pre-wrap break-words text-primary">{after || "清空"}</dd>
              </div>)}
            </dl>}
            {!original && <p className="mt-2 text-caption text-danger">缺少原始快照，暂时无法确认。</p>}
            <div className="mt-3 flex justify-end gap-2">
              <Button size="compact" variant="ghost" disabled={busy || !original} aria-label={`放弃：${task.title}`}
                onClick={() => decision.mutate({ task, keep: false })}>拒绝修改</Button>
              <Button size="compact" variant="primary" disabled={busy || !original} aria-label={`应用：${task.title}`}
                loading={decision.isPending && decision.variables?.task.id === task.id && decision.variables.keep}
                onClick={() => decision.mutate({ task, keep: true })}>{kind === "删除" ? "确认删除" : "确认修改"}</Button>
            </div>
          </article>;
        })}
      </div>
      {decision.isError && <p role="alert" className="border-t border-light px-3 py-2 text-caption text-danger">{errorMessage(decision.error)}</p>}
    </section>
  );
}

const BREAKDOWN_FIELDS = [
  ["context", "clarification", "目标说明"], ["context", "background", "背景"], ["context", "stakeholders", "相关人员"],
  ["output", "value", "交付内容"], ["outcome", "value", "预期成果"], ["outcome", "verification_method", "验证方法"],
  ["outcome", "controlled_by_user", "用户可控"], ["scope", "effort", "预计投入"], ["scope", "fully_decomposed", "拆解完成"],
] as const;

function breakdownValue(value: Json | null, part: string, field: string): string {
  if (!value || typeof value !== "object" || Array.isArray(value)) return "";
  const group = value[part];
  if (!group || typeof group !== "object" || Array.isArray(group)) return "";
  const result = group[field];
  return typeof result === "string" ? result : typeof result === "boolean" ? result ? "是" : "否" : "";
}

function stepsText(steps: Subtask[], depth = 0): string {
  return steps.map((step) => `${"  ".repeat(depth)}${step.completed ? "✓" : "•"} ${step.title}${step.children?.length ? `\n${stepsText(step.children, depth + 1)}` : ""}`).join("\n");
}

function detailChanges(original: TaskSnapshot, task: Task): Array<[string, string, string]> {
  if (task.proposal === "delete") return [];
  const changes: Array<[string, string, string]> = [];
  for (const [part, field, label] of BREAKDOWN_FIELDS) {
    const before = breakdownValue(original.goal_breakdown, part, field);
    const after = breakdownValue(task.goal_breakdown, part, field);
    if (before !== after) changes.push([label, before, after]);
  }
  const before = stepsText(original.subtasks ?? []);
  const after = stepsText(task.subtasks);
  if (before !== after) changes.push(["执行步骤", before, after]);
  if (original.original_exists && original.completed !== task.completed) changes.push(["完成状态", original.completed ? "已完成" : "未完成", task.completed ? "已完成" : "未完成"]);
  return changes;
}

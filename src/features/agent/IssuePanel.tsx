/** Read-only diagnosis of the current plan; explicit AI status never implies a check that did not run. */
import { useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { qk } from "../../lib/events";
import { dismissPlanningIssue, getPlanningIssueReport, getAiAvailability, isAppError, type PlanningIssue, type PlanningIssueReport } from "../../lib/ipc";
import { Button, Popover, PopoverItem } from "../../ui";
import { AI_AVAILABILITY_KEY } from "./PlanWithAI";
import { PanelShell } from "./PanelShell";
import { ThinkingIndicator } from "./ThinkingIndicator";

export type IssuePanelProps = {
  cycleId: string;
  onClose: () => void;
  onLocateTask?: (cycleId: string, taskId: string) => void;
  onDiscuss?: (cycleId: string, prompt: string, taskId: string | null) => void;
  onOpenSettings?: () => void;
};
const LABELS: Record<PlanningIssue["issue_type"], string> = {
  too_many_goals: "目标数量", too_many_tasks: "任务数量", too_much_work: "当日负荷",
  not_sure_what_to_do_next: "下一步", missing_something: "目标信息", not_useful_for_needs: "需求澄清",
};
const REASONS = [
  { label: "不适用于我的情况", reason: null },
  { label: "规划过程太费力", reason: "Planning felt like too much work" },
  { label: "不清楚如何处理", reason: "Not sure how to use it" },
];

export function IssuePanel({ cycleId, onClose, onLocateTask, onDiscuss, onOpenSettings }: IssuePanelProps) {
  const client = useQueryClient();
  const report = useQuery({ queryKey: qk.issueReport(cycleId), queryFn: () => getPlanningIssueReport(cycleId, false), enabled: Boolean(cycleId) });
  const ai = useQuery({ queryKey: AI_AVAILABILITY_KEY, queryFn: getAiAvailability });
  const inspect = useMutation({
    mutationFn: () => getPlanningIssueReport(cycleId, true),
    onSuccess: () => client.invalidateQueries({ queryKey: qk.issueReport(cycleId) }),
    // Reread the current snapshot even after a provider failure or concurrent edit.
    onError: () => client.invalidateQueries({ queryKey: qk.issueReport(cycleId) }),
  });
  const dismiss = useMutation({
    mutationFn: ({ issue, reason }: { issue: PlanningIssue; reason: string | null }) => dismissPlanningIssue(cycleId, issue.issue_type, issue.task_id, reason),
    onSuccess: () => client.invalidateQueries({ queryKey: qk.issueReport(cycleId) }),
  });
  const data = report.data;
  const issues = data?.issues ?? [];
  const completed = data?.ai_status === "completed";
  const empty = data?.ai_status === "empty";
  const busy = inspect.isPending;
  const scope = data ? `${{ month: "长期计划", week: "周计划", day: "日计划", session: "专注块" }[data.cycle_type]}${data.starts_on ? ` · ${data.starts_on}` : ""}` : "当前计划";
  return <PanelShell label="Plan issues" title="计划检查" onClose={onClose}
    headerDetails={<span className="rounded bg-subtle px-1.5 py-0.5 text-caption text-secondary">{issues.length} 项</span>}>
    <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
      <div className="mb-4 border-b border-light pb-3">
        <div className="flex items-center justify-between gap-3">
          <div className="min-w-0"><p className="truncate text-menu font-medium text-primary" title={data?.cycle_title}>{scope}</p>
            <p className="mt-1 text-caption text-secondary">{data ? `${data.task_count} 项待办 · 不评分，不自动修改` : "正在读取计划…"}</p></div>
          <Button size="compact" variant="secondary" loading={busy} disabled={!ai.data || !data || empty || busy || report.isError}
            title={!ai.data ? "先在设置中选择并测通模型" : empty ? "当前没有需要检查的待办" : "检查当前计划的任务内容"}
            onClick={() => inspect.mutate()}>{completed || data?.ai_status === "stale" ? "重新检查" : "AI 检查"}</Button>
        </div>
        {busy ? <div className="mt-3"><ThinkingIndicator label={`正在检查 ${data?.task_count} 项待办…`}/></div>
          : <p role="status" className="mt-3 text-caption text-hint">{empty ? "当前没有待检查的任务。" : completed ? `AI 已检查 ${data.checked_count} 项${data.checked_at ? ` · ${new Date(data.checked_at).toLocaleTimeString([], {hour:"2-digit",minute:"2-digit"})}` : ""}${data.model ? ` · ${data.model}` : ""}`
            : data?.ai_status === "stale" ? "计划或模型已变化，请重新运行 AI 检查。" : "规则提示已更新，AI 尚未检查。"}</p>}
        {!ai.isPending && !ai.data && <p className="mt-2 text-caption text-secondary">配置并测通模型后可使用 AI 检查。{onOpenSettings && <button className="ml-1 rounded text-focus hover:underline" onClick={onOpenSettings}>打开设置</button>}</p>}
        {!!data?.pending_count && <p className="mt-2 text-caption text-hint">{data.pending_count} 项 Coach 预览尚未确认，不纳入本次检查。</p>}
      </div>
      {inspect.isError && <div role="alert" className="mb-3 rounded-lg border border-light bg-subtle px-3 py-2 text-caption text-secondary">
        <p className="font-medium text-primary">AI 检查未完成</p><p className="mt-1 break-words">{errorText(inspect.error)}</p><p className="mt-1">已有提示保留，可以重新检查。</p>
      </div>}
      {report.isError ? <p role="alert" className="text-caption text-danger">计划检查加载失败：{errorText(report.error)}</p>
        : report.isPending ? <p className="text-caption text-hint">正在读取计划…</p>
        : issues.length ? <ul className="space-y-3">{issues.map((issue) => <IssueRow key={`${issue.issue_type}:${issue.task_id ?? ""}`} issue={issue}
          disabled={dismiss.isPending} onDismiss={(reason) => dismiss.mutate({issue, reason})}
          onLocate={issue.task_id && onLocateTask ? () => onLocateTask(cycleId, issue.task_id!) : undefined}
          onDiscuss={onDiscuss ? () => onDiscuss(cycleId, `请帮我处理${issue.task_title ? `任务「${issue.task_title}」` : "当前计划"}的这个问题：${issue.title}。${issue.detail}\n先核对当前内容，给出简洁建议；需要修改时通过工具提出预览，等待我确认。`, issue.task_id) : undefined}/>)}</ul>
        : !busy && !inspect.isError && <div className="py-4 text-menu text-secondary">{empty ? "可以先在计划中添加事务，再回来检查。" : completed ? (data?.ignored_count ? "当前没有需要处理的提示。" : "本次检查未发现需要处理的问题。") : "暂未发现规则层面的问题，可以用 AI 检查任务内容。"}</div>}
      {!!data?.ignored_count && <p className="mt-4 text-caption text-hint">已忽略 {data.ignored_count} 项提示</p>}
      {dismiss.isError && <p role="alert" className="mt-3 text-caption text-danger">忽略失败：{errorText(dismiss.error)}</p>}
    </div>
  </PanelShell>;
}

function IssueRow({issue,disabled,onDismiss,onLocate,onDiscuss}:{
  issue:PlanningIssueReport["issues"][number]; disabled:boolean; onDismiss:(reason:string|null)=>void; onLocate?:()=>void; onDiscuss?:()=>void;
}) {
  const anchor = useRef<HTMLButtonElement>(null);
  const [open,setOpen] = useState(false);
  return <li className="rounded-lg border border-light px-3 py-3">
    <div className="mb-2 flex items-center gap-2 text-[11px] text-hint"><span className={issue.source === "ai" ? "text-focus" : "text-secondary"}>{issue.source === "ai" ? "AI 建议" : "规则提示"}</span><span aria-hidden="true">·</span><span>{LABELS[issue.issue_type]}</span></div>
    <p className="break-words text-menu font-medium text-primary">{issue.title}</p>
    {issue.task_title && <p className="mt-1 truncate text-caption text-hint" title={issue.task_title}>{issue.task_title}</p>}
    <p className="mt-2 whitespace-pre-wrap break-words text-menu leading-relaxed text-secondary">{issue.detail}</p>
    <div className="mt-2 flex flex-wrap items-center gap-1">
      {onLocate && <Button size="compact" variant="ghost" className="text-caption" onClick={onLocate}>定位任务 <span aria-hidden="true">↗</span></Button>}
      {onDiscuss && <Button size="compact" variant="ghost" className="text-caption" onClick={onDiscuss}>与 Coach 讨论</Button>}
      <button ref={anchor} type="button" className="ml-auto rounded-md px-2 py-1.5 text-caption text-hint transition-colors hover:bg-hover hover:text-secondary"
        disabled={disabled} aria-haspopup="menu" aria-expanded={open} onClick={()=>setOpen(true)}>忽略</button>
    </div>
    <Popover open={open} onClose={()=>setOpen(false)} anchorRef={anchor} label="忽略原因">
      {REASONS.map(item=><PopoverItem key={item.label} onSelect={()=>{setOpen(false);onDismiss(item.reason);}}>{item.label}</PopoverItem>)}
    </Popover>
  </li>;
}
function errorText(error:unknown):string { return isAppError(error) ? error.message : error instanceof Error ? error.message : String(error); }

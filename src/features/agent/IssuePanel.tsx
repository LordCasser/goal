/** Read-only diagnosis of the current plan; explicit AI status never implies a check that did not run. */
import { useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { qk } from "../../lib/events";
import { dismissPlanningIssue, getPlanningIssueReport, getAiAvailability, type PlanningIssue, type PlanningIssueReport } from "../../lib/ipc";
import { errorMessage, formatDate, useTranslation } from "../../lib/i18n";
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
const REASONS = [
  { labelKey: "issue.reasonNotApplicable", reason: null },
  { labelKey: "issue.reasonTooMuchWork", reason: "Planning felt like too much work" },
  { labelKey: "issue.reasonUnclear", reason: "Not sure how to use it" },
];

export function IssuePanel({ cycleId, onClose, onLocateTask, onDiscuss, onOpenSettings }: IssuePanelProps) {
  const { t, i18n } = useTranslation("ai");
  const client = useQueryClient();
  const locale = i18n.resolvedLanguage || i18n.language;
  // Backend caches AI findings by locale. Switching language only reads the
  // matching report; refresh=true remains exclusive to the explicit AI check.
  const reportKey = [...qk.issueReport(cycleId), locale] as const;
  const report = useQuery({ queryKey: reportKey, queryFn: () => getPlanningIssueReport(cycleId, false), enabled: Boolean(cycleId) });
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
  const scope = data
    ? `${t(`issue.${data.cycle_type}`, { defaultValue: t("issue.currentPlan") })}${data.starts_on ? ` · ${formatDate(data.starts_on)}` : ""}`
    : t("issue.currentPlan");
  return <PanelShell label={t("issue.planIssues")} title={t("issue.planIssues")} onClose={onClose}
    headerDetails={<span className="rounded bg-subtle px-1.5 py-0.5 text-caption text-secondary">{t("issue.count", { count: issues.length })}</span>}>
    <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
      <div className="mb-4 border-b border-light pb-3">
        <div className="flex items-center justify-between gap-3">
          <div className="min-w-0"><p className="truncate text-menu font-medium text-primary" title={data?.cycle_title}>{scope}</p>
            <p className="mt-1 text-caption text-secondary">{data ? t("issue.taskSummary", { count: data.task_count }) : t("issue.reading")}</p></div>
          <Button size="compact" variant="secondary" loading={busy} disabled={!ai.data || !data || empty || busy || report.isError}
            title={!ai.data ? t("issue.checkModelTitle") : empty ? t("issue.emptyTitle") : t("issue.checkTasksTitle")}
            onClick={() => inspect.mutate()}>{completed || data?.ai_status === "stale" ? t("issue.recheck") : t("issue.aiCheck")}</Button>
        </div>
        {busy ? <div className="mt-3"><ThinkingIndicator label={t("issue.checking", { count: data?.task_count ?? 0 })}/></div>
          : <p role="status" className="mt-3 text-caption text-hint">{empty ? t("issue.noTasks") : completed ? <>{t("issue.checked", { count: data.checked_count })}{data.checked_at ? ` · ${formatDate(data.checked_at)}` : ""}{data.model ? ` · ${data.model}` : ""}</>
            : data?.ai_status === "stale" ? t("issue.stale") : t("issue.ruleUpdated")}</p>}
        {!ai.isPending && !ai.data && <p className="mt-2 text-caption text-secondary">{t("issue.configureModel")}{onOpenSettings && <button className="ml-1 rounded text-focus hover:underline" onClick={onOpenSettings}>{t("issue.openSettings")}</button>}</p>}
        {!!data?.pending_count && <p className="mt-2 text-caption text-hint">{t("issue.pendingPreview", { count: data.pending_count })}</p>}
      </div>
      {inspect.isError && <div role="alert" className="mb-3 rounded-lg border border-light bg-subtle px-3 py-2 text-caption text-secondary">
        <p className="font-medium text-primary">{t("issue.failedTitle")}</p><p className="mt-1 break-words">{errorMessage(inspect.error)}</p><p className="mt-1">{t("issue.failedKeep")}</p>
      </div>}
      {report.isError ? <p role="alert" className="text-caption text-danger">{t("issue.loadFailed", { message: errorMessage(report.error) })}</p>
        : report.isPending ? <p className="text-caption text-hint">{t("issue.reading")}</p>
        : issues.length ? <ul className="space-y-3">{issues.map((issue) => <IssueRow key={`${issue.issue_type}:${issue.task_id ?? ""}`} issue={issue}
          disabled={dismiss.isPending} onDismiss={(reason) => dismiss.mutate({issue, reason})}
          onLocate={issue.task_id && onLocateTask ? () => onLocateTask(cycleId, issue.task_id!) : undefined}
          onDiscuss={onDiscuss ? () => { const text = localizedIssueText(issue, t); onDiscuss(cycleId, t("issue.discussPrompt", { target: issue.task_title ? `「${issue.task_title}」` : t("issue.currentPlan"), title: text.title, detail: text.detail }), issue.task_id); } : undefined}/>)}</ul>
        : !busy && !inspect.isError && <div className="py-4 text-menu text-secondary">{empty ? t("issue.emptyAdd") : completed ? (data?.ignored_count ? t("issue.emptyHandled") : t("issue.emptyResult")) : t("issue.noRuleIssues")}</div>}
      {!!data?.ignored_count && <p className="mt-4 text-caption text-hint">{t("issue.ignored", { count: data.ignored_count })}</p>}
      {dismiss.isError && <p role="alert" className="mt-3 text-caption text-danger">{t("issue.dismissFailed", { message: errorMessage(dismiss.error) })}</p>}
    </div>
  </PanelShell>;
}

function IssueRow({issue,disabled,onDismiss,onLocate,onDiscuss}:{
  issue:PlanningIssueReport["issues"][number]; disabled:boolean; onDismiss:(reason:string|null)=>void; onLocate?:()=>void; onDiscuss?:()=>void;
}) {
  const { t } = useTranslation("ai");
  const anchor = useRef<HTMLButtonElement>(null);
  const [open,setOpen] = useState(false);
  const { title: localizedTitle, detail: localizedDetail } = localizedIssueText(issue, t);
  return <li className="rounded-lg border border-light px-3 py-3">
    <div className="mb-2 flex items-center gap-2 text-[11px] text-hint"><span className={issue.source === "ai" ? "text-focus" : "text-secondary"}>{issue.source === "ai" ? t("issue.aiSuggestion") : t("issue.ruleHint")}</span><span aria-hidden="true">·</span><span>{localizedTitle}</span></div>
    <p className="break-words text-menu font-medium text-primary">{localizedTitle}</p>
    {issue.task_title && <p className="mt-1 truncate text-caption text-hint" title={issue.task_title}>{issue.task_title}</p>}
    <p className="mt-2 whitespace-pre-wrap break-words text-menu leading-relaxed text-secondary">{localizedDetail}</p>
    <div className="mt-2 flex flex-wrap items-center gap-1">
      {onLocate && <Button size="compact" variant="ghost" className="text-caption" onClick={onLocate}>{t("issue.locateTask")} <span aria-hidden="true">↗</span></Button>}
      {onDiscuss && <Button size="compact" variant="ghost" className="text-caption" onClick={onDiscuss}>{t("issue.discuss")}</Button>}
      <button ref={anchor} type="button" className="ml-auto rounded-md px-2 py-1.5 text-caption text-hint transition-colors hover:bg-hover hover:text-secondary"
        disabled={disabled} aria-haspopup="menu" aria-expanded={open} onClick={()=>setOpen(true)}>{t("issue.dismiss")}</button>
    </div>
    <Popover open={open} onClose={()=>setOpen(false)} anchorRef={anchor} label={t("issue.dismissReason")}>
      {REASONS.map(item=><PopoverItem key={item.labelKey} onSelect={()=>{setOpen(false);onDismiss(item.reason);}}>{t(item.labelKey)}</PopoverItem>)}
    </Popover>
  </li>;
}

function localizedIssueText(
  issue: PlanningIssueReport["issues"][number],
  translate: (key: string, options?: Record<string, unknown>) => string,
): { title: string; detail: string } {
  const title = issue.source === "structure" && issue.message_key
    ? translate(`backend:issue.${issue.issue_type}`, { defaultValue: issue.title })
    : issue.title;
  const params = issue.message_params ? { ...issue.message_params } : {};
  if (Array.isArray(params.fields)) {
    params.fields = params.fields.map((field) => translate(`backend:field.${field}`, { defaultValue: field })).join(translate("common.listSeparator"));
  }
  const detail = issue.message_key
    ? translate(`backend:${issue.message_key}`, { ...params, defaultValue: issue.detail })
    : issue.detail;
  return { title, detail };
}

import { invoke } from "@tauri-apps/api/core";
import { useIsMutating, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invalidateAgentEffects, qk } from "../../lib/events";
import { errorMessage, formatMessage, type LocalizedMessage, useTranslation } from "../../lib/i18n";
import { ApprovalCard } from "../../ui/ai";
import { Button } from "../../ui";

export interface PendingAction {
  id: string;
  source_cycle_id: string;
  /** Stable language-independent key; the historical summary is a backend fallback only. */
  summary_key: string;
  summary?: string;
  details: LocalizedMessage[];
  rationale: string;
  state: "pending" | "applying";
}

export function ActionApprovalCard({ disabled = false }: { disabled?: boolean }) {
  const { t } = useTranslation("ai");
  const client=useQueryClient();
  const query=useQuery({queryKey:qk.agentActions(),queryFn:()=>invoke<PendingAction[]>("get_agent_actions")});
  const agentDecisionBusy = useIsMutating({ mutationKey: qk.agentDecision() }) > 0;
  const decision=useMutation({
    mutationKey: qk.agentDecision(),
    mutationFn:async ({item,approve}:{item:PendingAction;approve:boolean})=>{
      await invoke("resolve_agent_action",{cycleId:item.source_cycle_id,actionId:item.id,approve});
      return {item,approve};
    },
    onSettled:()=>invalidateAgentEffects(client),
  });
  if(query.isError) return <p role="alert" className="mb-3 text-caption text-danger">{t("proposals.actionsLoadFailed", { message: errorMessage(query.error) })}</p>;
  const items=query.data??[];
  if(!items.length) return null;
  const busy = disabled || agentDecisionBusy || decision.isPending;
  return <ApprovalCard
    label={t("proposals.actionsLabel")}
    title={t("proposals.actionsWaiting")}
    count={t("proposals.count", { count: items.length })}
    footer={disabled ? <p className="mr-auto text-caption text-hint">{t("proposals.waitForTurn")}</p> : undefined}
    error={decision.isError ? errorMessage(decision.error) : undefined}
  >
      {items.map(item=>{
        const summary = item.summary_key ? formatMessage({ key: item.summary_key, args: {} }) : (item.summary ?? "");
        return <article key={item.id} aria-label={summary} className="px-3 py-3">
        <p className="text-body font-medium text-primary">{summary}</p>
        <ul className="mt-2 space-y-1 text-caption text-secondary">{item.details.map((detail,i)=><li key={i} className="whitespace-pre-wrap break-words">{formatMessage(detail)}</li>)}</ul>
        <p className="mt-2 break-words text-caption text-hint">{item.rationale}</p>
        {item.state==="applying"?<div className="mt-2 text-caption text-secondary"><p role="status">{t("proposals.running")}</p><Button size="compact" variant="ghost" disabled={busy} onClick={()=>decision.mutate({item,approve:false})}>{t("proposals.closeRecord")}</Button></div>:<div className="mt-3 flex justify-end gap-2">
          <Button size="compact" variant="ghost" disabled={busy} onClick={()=>decision.mutate({item,approve:false})}>{t("proposals.reject")}</Button>
          <Button size="compact" variant="primary" disabled={busy} loading={decision.isPending&&decision.variables?.item.id===item.id&&decision.variables.approve} onClick={()=>decision.mutate({item,approve:true})}>{t("proposals.apply")}</Button>
        </div>}
      </article>;
      })}
    </ApprovalCard>;
}

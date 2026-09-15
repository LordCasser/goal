import { invoke } from "@tauri-apps/api/core";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invalidateAgentEffects, qk } from "../../lib/events";
import { errorMessage, formatMessage, type LocalizedMessage, useTranslation } from "../../lib/i18n";
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

export function ActionApprovalCard({ cycleId, disabled = false }: { cycleId: string; disabled?: boolean }) {
  const { t } = useTranslation("ai");
  const client=useQueryClient();
  const query=useQuery({queryKey:qk.agentActions(cycleId),queryFn:()=>invoke<PendingAction[]>("get_agent_actions",{cycleId})});
  const decision=useMutation({
    mutationFn:async ({item,approve}:{item:PendingAction;approve:boolean})=>{
      await invoke("resolve_agent_action",{cycleId,actionId:item.id,approve});
      return {item,approve};
    },
    onSettled:()=>invalidateAgentEffects(client),
  });
  if(query.isError) return <p role="alert" className="mb-3 text-caption text-danger">{t("proposals.actionsLoadFailed", { message: errorMessage(query.error) })}</p>;
  const items=query.data??[];
  if(!items.length) return null;
  return <section aria-label={t("proposals.actionsLabel")} className="coach-approval mb-3 overflow-hidden rounded-xl border border-light bg-content">
    <header className="flex items-center justify-between bg-subtle px-3 py-2.5 text-caption"><span className="font-medium text-primary">{t("proposals.actionsWaiting")}</span><span className="text-hint">{t("proposals.count", { count: items.length })}</span></header>
    <div className="max-h-[240px] overflow-y-auto">
      {items.map(item=>{
        const summary = item.summary_key ? formatMessage({ key: item.summary_key, args: {} }) : (item.summary ?? "");
        return <article key={item.id} aria-label={summary} className="border-t border-light px-3 py-3 first:border-t-0">
        <p className="text-body font-medium text-primary">{summary}</p>
        <ul className="mt-2 space-y-1 text-caption text-secondary">{item.details.map((detail,i)=><li key={i} className="whitespace-pre-wrap break-words">{formatMessage(detail)}</li>)}</ul>
        <p className="mt-2 break-words text-caption text-hint">{item.rationale}</p>
        {item.state==="applying"?<div className="mt-2 text-caption text-secondary"><p role="status">{t("proposals.running")}</p><Button size="compact" variant="ghost" disabled={disabled||decision.isPending} onClick={()=>decision.mutate({item,approve:false})}>{t("proposals.closeRecord")}</Button></div>:<div className="mt-3 flex justify-end gap-2">
          <Button size="compact" variant="ghost" disabled={disabled||decision.isPending} onClick={()=>decision.mutate({item,approve:false})}>{t("proposals.reject")}</Button>
          <Button size="compact" variant="primary" disabled={disabled||decision.isPending} loading={decision.isPending&&decision.variables?.item.id===item.id&&decision.variables.approve} onClick={()=>decision.mutate({item,approve:true})}>{t("proposals.apply")}</Button>
        </div>}
      </article>;
      })}
    </div>
    {decision.isError&&<p role="alert" className="border-t border-light px-3 py-2 text-caption text-danger">{errorMessage(decision.error)}</p>}
  </section>;
}

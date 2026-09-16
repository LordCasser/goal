/** A toolbar entry to Coach; confirmations exist only inside Coach. */
import { invoke } from "@tauri-apps/api/core";
import { useQuery, useQueries } from "@tanstack/react-query";
import { getPlannerState, getPreviewSummary } from "../../lib/ipc";
import { useTranslation } from "../../lib/i18n";
import { qk } from "../../lib/events";
import { Button } from "../../ui";
import type { PendingAction } from "./ActionApprovalCard";

export function ProposalsBar({onOpen}:{onOpen:(cycleId:string|null)=>void}) {
  const { t } = useTranslation("ai");
  const planner=useQuery({queryKey:qk.plannerState(),queryFn:getPlannerState});
  const cycles=planner.data?.cycles??[];
  const tasks=useQueries({queries:cycles.map(c=>({queryKey:qk.previewSummary(c.id),queryFn:()=>getPreviewSummary(c.id)}))});
  const actions=useQuery({queryKey:qk.agentActions(),queryFn:()=>invoke<PendingAction[]>("get_agent_actions")});
  const counts=cycles.map((c,i)=>({id:c.id,count:tasks[i]?.data?.count??0})).filter(c=>c.count>0);
  const total=counts.reduce((sum,c)=>sum+c.count,0)+(actions.data?.length??0);
  if(!total) return null;
  return <Button aria-label={t("proposals.pendingBarAria", { count: total })} variant="ghost" size="compact" className="window-pending h-7 gap-1.5 px-2 text-caption" onClick={()=>onOpen(actions.data?.[0]?.source_cycle_id ?? counts[0]?.id ?? null)} title={t("proposals.pendingBarTitle", { count: total })}><span className="rounded bg-focus-surface px-1.5 text-focus">{total}</span><span className="window-tool-label">{t("proposals.pendingBarLabel")}</span></Button>;
}

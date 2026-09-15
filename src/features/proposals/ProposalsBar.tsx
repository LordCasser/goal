/** A toolbar entry to Coach; confirmations exist only inside Coach. */
import { invoke } from "@tauri-apps/api/core";
import { useQueries, useQuery } from "@tanstack/react-query";
import { getPlannerState, getPreviewSummary } from "../../lib/ipc";
import { useTranslation } from "../../lib/i18n";
import { qk } from "../../lib/events";
import { Button } from "../../ui";
import type { PendingAction } from "./ActionApprovalCard";

export function ProposalsBar({onOpen}:{onOpen:(cycleId:string)=>void}) {
  const { t } = useTranslation("ai");
  const planner=useQuery({queryKey:qk.plannerState(),queryFn:getPlannerState});
  const cycles=planner.data?.cycles??[];
  const tasks=useQueries({queries:cycles.map(c=>({queryKey:qk.previewSummary(c.id),queryFn:()=>getPreviewSummary(c.id)}))});
  const actions=useQueries({queries:cycles.map(c=>({queryKey:qk.agentActions(c.id),queryFn:()=>invoke<PendingAction[]>("get_agent_actions",{cycleId:c.id})}))});
  const counts=cycles.map((c,i)=>({id:c.id,count:(tasks[i]?.data?.count??0)+(actions[i]?.data?.length??0)})).filter(c=>c.count>0);
  const total=counts.reduce((sum,c)=>sum+c.count,0);
  if(!total) return null;
  return <Button aria-label={t("proposals.pendingBarAria", { count: total })} variant="ghost" size="compact" className="window-pending h-7 gap-1.5 px-2 text-caption" onClick={()=>onOpen(counts[0]!.id)} title={t("proposals.pendingBarTitle", { count: total })}><span className="rounded bg-focus-surface px-1.5 text-focus">{total}</span><span className="window-tool-label">{t("proposals.pendingBarLabel")}</span></Button>;
}

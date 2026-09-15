import { invoke } from "@tauri-apps/api/core";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { invalidateAgentEffects, qk } from "../../lib/events";
import { errorMessage } from "../planner/actions";
import { Button } from "../../ui";

export interface PendingAction { id: string; source_cycle_id: string; summary: string; details: string[]; rationale: string; state: "pending" | "applying" }

export function ActionApprovalCard({ cycleId, disabled = false }: { cycleId: string; disabled?: boolean }) {
  const client=useQueryClient();
  const query=useQuery({queryKey:qk.agentActions(cycleId),queryFn:()=>invoke<PendingAction[]>("get_agent_actions",{cycleId})});
  const decision=useMutation({
    mutationFn:async ({item,approve}:{item:PendingAction;approve:boolean})=>{
      await invoke("resolve_agent_action",{cycleId,actionId:item.id,approve});
      return {item,approve};
    },
    onSettled:()=>invalidateAgentEffects(client),
  });
  if(query.isError) return <p role="alert" className="mb-3 text-caption text-danger">待确认操作加载失败：{errorMessage(query.error)}</p>;
  const items=query.data??[];
  if(!items.length) return null;
  return <section aria-label="待确认的操作" className="coach-approval mb-3 overflow-hidden rounded-xl border border-light bg-content">
    <header className="flex items-center justify-between bg-subtle px-3 py-2.5 text-caption"><span className="font-medium text-primary">操作 · 等待确认</span><span className="text-hint">{items.length} 项</span></header>
    <div className="max-h-[240px] overflow-y-auto">
      {items.map(item=><article key={item.id} aria-label={item.summary} className="border-t border-light px-3 py-3 first:border-t-0">
        <p className="text-body font-medium text-primary">{item.summary}</p>
        <ul className="mt-2 space-y-1 text-caption text-secondary">{item.details.map((detail,i)=><li key={i} className="whitespace-pre-wrap break-words">{detail}</li>)}</ul>
        <p className="mt-2 break-words text-caption text-hint">{item.rationale}</p>
        {item.state==="applying"?<div className="mt-2 text-caption text-secondary"><p role="status">正在执行；若应用曾意外退出，请先检查实际结果，避免重复操作。</p><Button size="compact" variant="ghost" disabled={disabled||decision.isPending} onClick={()=>decision.mutate({item,approve:false})}>已核对实际结果，关闭记录</Button></div>:<div className="mt-3 flex justify-end gap-2">
          <Button size="compact" variant="ghost" disabled={disabled||decision.isPending} onClick={()=>decision.mutate({item,approve:false})}>放弃</Button>
          <Button size="compact" variant="primary" disabled={disabled||decision.isPending} loading={decision.isPending&&decision.variables?.item.id===item.id&&decision.variables.approve} onClick={()=>decision.mutate({item,approve:true})}>确认应用</Button>
        </div>}
      </article>)}
    </div>
    {decision.isError&&<p role="alert" className="border-t border-light px-3 py-2 text-caption text-danger">{errorMessage(decision.error)}</p>}
  </section>;
}

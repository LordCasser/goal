import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
const invoke=vi.hoisted(()=>vi.fn());
vi.mock("@tauri-apps/api/core",()=>({invoke}));
import { ActionApprovalCard, type PendingAction } from "./ActionApprovalCard";
const item:PendingAction={id:"action",source_cycle_id:"day",summary:"修改设置",details:["Coach 上下文有效期（分钟）","15 → 30"],rationale:"用户希望保留更久",state:"pending"};
let pending:PendingAction[];
beforeEach(()=>{pending=[item];invoke.mockReset().mockImplementation(async(cmd)=>{
  if(cmd==="get_agent_actions") return pending;
  if(cmd==="resolve_agent_action") {pending=[];return null;}
});});
afterEach(cleanup);
function mount(){return render(<QueryClientProvider client={new QueryClient({defaultOptions:{queries:{retry:false},mutations:{retry:false}}})}><ActionApprovalCard cycleId="day"/></QueryClientProvider>);}
it("does not execute on display, then confirms the exact action once and removes the footer item",async()=>{
  mount(); expect(await screen.findByText("15 → 30")).toBeTruthy();
  expect(invoke.mock.calls.every(([name])=>name==="get_agent_actions")).toBe(true);
  fireEvent.click(screen.getByRole("button",{name:"确认应用"}));
  await waitFor(()=>expect(screen.queryByLabelText("待确认的操作")).toBeNull());
  expect(invoke).toHaveBeenCalledWith("resolve_agent_action",{cycleId:"day",actionId:"action",approve:true});
  expect(screen.queryByRole("status")).toBeNull(); // Receipt belongs to the transcript.
});
it("rejects without applying and preserves failed approvals for retry",async()=>{
  invoke.mockImplementation(async(cmd)=>cmd==="get_agent_actions"?pending:Promise.reject({message:"设置未保存",code:"db_error"}));
  mount();fireEvent.click(await screen.findByRole("button",{name:"放弃"}));
  expect(await screen.findByRole("alert")).toBeTruthy();
  expect(invoke).toHaveBeenCalledWith("resolve_agent_action",{cycleId:"day",actionId:"action",approve:false});
  expect(screen.getByText("15 → 30")).toBeTruthy();
  expect(screen.queryByRole("status")).toBeNull();
});

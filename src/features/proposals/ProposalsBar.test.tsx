import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
const invoke=vi.hoisted(()=>vi.fn());
vi.mock("@tauri-apps/api/core",()=>({invoke}));
import { ProposalsBar } from "./ProposalsBar";
import { commands } from "../../lib/ipc";
import { applyLocale } from "../../lib/i18n";
afterEach(cleanup);
beforeEach(() => applyLocale("zh-CN"));
function mount(count:number){
  invoke.mockReset().mockImplementation(async(cmd:string,args:{cycleId?:string})=>{
    if(cmd===commands.getPlannerState) return {cycles:[{id:"day"},{id:"week"}]};
    if(cmd===commands.getPreviewSummary) return {count:args.cycleId==="week"?count:0};
    if(cmd==="get_agent_actions") return count&&args.cycleId==="day"?[{id:"action"}]:[];
    throw new Error(`Unexpected command: ${cmd}`);
  });
  const open=vi.fn();
  const client=new QueryClient({defaultOptions:{queries:{retry:false}}});
  render(<QueryClientProvider client={client}><ProposalsBar onOpen={open}/></QueryClientProvider>);
  return {open,client};
}
it("aggregates pending tasks and settings into a navigation-only entry",async()=>{
  const {open}=mount(2);
  fireEvent.click(await screen.findByRole("button",{name:"3 项待确认"}));
  expect(open).toHaveBeenCalledWith("day");
  expect(screen.queryByText("Keep all")).toBeNull();
  expect(screen.queryByText("Revert all")).toBeNull();
  expect(invoke.mock.calls.every(([name])=>[commands.getPlannerState,commands.getPreviewSummary,"get_agent_actions"].includes(name))).toBe(true);
});
it("takes no space without pending changes",async()=>{
  const {client}=mount(0);
  await client.refetchQueries();
  expect(screen.queryByRole("button")).toBeNull();
});

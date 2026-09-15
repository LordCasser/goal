import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { Cycle } from "../../lib/ipc";
const mocks = vi.hoisted(() => ({ getPlannerState: vi.fn(), getEditorWorkspacesByCycleIds: vi.fn(), createPlanningCycle: vi.fn(), ensureDay: vi.fn() }));
vi.mock("../../lib/ipc", async (original) => ({ ...(await original<typeof import("../../lib/ipc")>()), ...mocks }));
vi.mock("./CycleColumn", () => ({ CycleColumn: ({ cycle }: { cycle: Cycle }) => <article>{cycle.id}</article> }));
vi.mock("./dates", async (original) => ({ ...(await original<typeof import("./dates")>()), todayISO: () => "2026-09-15" }));
import { PlannerWorkspace } from "./PlannerWorkspace";
function cycle(id: string, type: Cycle["type"], parent_id: string | null, starts_on: string, ends_on: string): Cycle {
  return { id, type, parent_id, starts_on, ends_on, position: 0, title: id, finished: false } as Cycle;
}
const cycles = [cycle("m1", "month", null, "2026-09-01", "2026-12-01"), cycle("m2", "month", null, "2026-12-01", "2027-03-01"), cycle("w1", "week", "m1", "2026-09-14", "2026-09-21"), cycle("w2", "week", "m1", "2026-09-21", "2026-09-28"), cycle("d1", "day", "w1", "2026-09-15", "2026-09-16"), cycle("d2", "day", "w2", "2026-09-22", "2026-09-23")];
beforeEach(() => { mocks.getPlannerState.mockResolvedValue({ cycles }); mocks.getEditorWorkspacesByCycleIds.mockResolvedValue({}); });
function mount(props: Partial<import("react").ComponentProps<typeof PlannerWorkspace>> = {}) { render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><PlannerWorkspace {...props} /></QueryClientProvider>); }
describe("time navigation", () => {
  it("independent weekly and daily tasks remain visible without any long-term cycle", async () => {
    mocks.getPlannerState.mockResolvedValue({ cycles: [{ ...cycles[2], parent_id: null }, { ...cycles[4], parent_id: null }] });
    mount();
    await screen.findByText("w1");
    expect(screen.getAllByRole("article").map((n) => n.textContent)).toEqual(["w1", "d1"]);
    expect((screen.getByRole("button", { name: "This week" }) as HTMLButtonElement).disabled).toBe(false);
    expect((screen.getByRole("button", { name: "+ Today" }) as HTMLButtonElement).disabled).toBe(false);
  });
  it("shows one plan per horizon and keeps future/history cycles in navigation", async () => {
    mount();
    await screen.findByText("m1");
    expect(screen.getAllByRole("article").map((n) => n.textContent)).toEqual(["m1", "w1", "d1"]);
    expect(within(screen.getByRole("navigation", { name: "Weeks" })).getAllByRole("button")).toHaveLength(3);
  });
  it("switching week also selects a day from that week", async () => {
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "W39" }));
    expect(screen.getAllByRole("article").map((n) => n.textContent)).toEqual(["m1", "w2", "d2"]);
  });
  it("switching long-term goals preserves the current week and day", async () => {
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "Dec 1" }));
    expect(screen.getAllByRole("article").map((n) => n.textContent)).toEqual(["m2", "w1", "d1"]);
  });
});

it("selects the diagnostic day and its week across date boundaries",async()=>{
  mount({revealTask:{cycleId:"d2",taskId:"target",requestId:1}});
  await screen.findByText("d2");
  expect(screen.getAllByRole("article").map(n=>n.textContent)).toEqual(["m1","w2","d2"]);
});

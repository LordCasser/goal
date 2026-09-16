import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { Cycle } from "../../lib/ipc";
const mocks = vi.hoisted(() => ({ getPlannerState: vi.fn(), getEditorWorkspacesByCycleIds: vi.fn(), createPlanningCycle: vi.fn(), ensureDay: vi.fn() }));
vi.mock("../../lib/ipc", async (original) => ({ ...(await original<typeof import("../../lib/ipc")>()), ...mocks }));
vi.mock("./CycleColumn", () => ({
  CycleColumn: ({ cycle }: { cycle: Cycle }) => (
    <article>
      <header data-testid={`header-${cycle.id}`} className="workspace-scroll-heading">
        <span data-workspace-scroll-hint hidden data-testid={`hint-${cycle.id}`} />
      </header>
      <div data-testid={`pane-${cycle.id}`} data-workspace-scroll-pane className="overflow-y-auto" tabIndex={-1}>{cycle.id}</div>
    </article>
  ),
}));
vi.mock("./dates", async (original) => ({ ...(await original<typeof import("./dates")>()), todayISO: () => "2026-09-15" }));
import { PlannerWorkspace } from "./PlannerWorkspace";
function cycle(id: string, type: Cycle["type"], parent_id: string | null, starts_on: string, ends_on: string): Cycle {
  return { id, type, parent_id, starts_on, ends_on, position: 0, title: id, finished: false } as Cycle;
}
const cycles = [cycle("m1", "month", null, "2026-09-01", "2026-12-01"), cycle("m2", "month", null, "2026-12-01", "2027-03-01"), cycle("w1", "week", "m1", "2026-09-14", "2026-09-21"), cycle("w2", "week", "m1", "2026-09-21", "2026-09-28"), cycle("d1", "day", "w1", "2026-09-15", "2026-09-16"), cycle("d2", "day", "w2", "2026-09-22", "2026-09-23")];
beforeEach(() => { mocks.getPlannerState.mockResolvedValue({ cycles }); mocks.getEditorWorkspacesByCycleIds.mockResolvedValue({}); });
function workspaceElement(props: Partial<import("react").ComponentProps<typeof PlannerWorkspace>> = {}) {
  return <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><PlannerWorkspace {...props} /></QueryClientProvider>;
}
function mount(props: Partial<import("react").ComponentProps<typeof PlannerWorkspace>> = {}) { return render(workspaceElement(props)); }
function defineScrollMetrics(element: HTMLElement, metrics: { scrollWidth?: number; clientWidth?: number; scrollHeight?: number; clientHeight?: number }) {
  for (const [key, value] of Object.entries(metrics)) Object.defineProperty(element, key, { configurable: true, value });
}
function defineScrollTop(element: HTMLElement, value: number) {
  Object.defineProperty(element, "scrollTop", { configurable: true, writable: true, value });
}
function dispatchWheel(target: HTMLElement, init: Partial<WheelEventInit>) {
  const event = new WheelEvent("wheel", { bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(event);
  return event;
}
function dispatchMouseDown(target: HTMLElement, init: Partial<MouseEventInit> = {}) {
  const event = new MouseEvent("mousedown", { bubbles: true, cancelable: true, button: 0, ...init });
  target.dispatchEvent(event);
  return event;
}
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

describe("workspace mouse-wheel routing", () => {
  it("keeps scroll-mode hints synchronized with the click-active pane", async () => {
    mount();
    await screen.findByLabelText("Planning workspace");
    const first = screen.getByTestId("pane-m1");
    const second = screen.getByTestId("pane-w1");
    const firstHint = screen.getByTestId("hint-m1") as HTMLSpanElement;
    const secondHint = screen.getByTestId("hint-w1") as HTMLSpanElement;
    const thirdHint = screen.getByTestId("hint-d1") as HTMLSpanElement;
    const header = screen.getByTestId("header-m1");

    expect(firstHint.hidden).toBe(true);
    expect(secondHint.hidden).toBe(true);
    expect(thirdHint.hidden).toBe(true);
    expect(first.getAttribute("data-wheel-active")).toBeNull();
    expect(second.getAttribute("data-wheel-active")).toBeNull();

    dispatchMouseDown(first);
    expect(firstHint.hidden).toBe(false);
    expect(secondHint.hidden).toBe(true);
    expect(thirdHint.hidden).toBe(true);
    expect(first.getAttribute("data-wheel-active")).toBe("true");
    expect(second.getAttribute("data-wheel-active")).toBeNull();

    dispatchMouseDown(second);
    expect(firstHint.hidden).toBe(true);
    expect(secondHint.hidden).toBe(false);
    expect(thirdHint.hidden).toBe(true);
    expect(first.getAttribute("data-wheel-active")).toBeNull();
    expect(second.getAttribute("data-wheel-active")).toBe("true");

    dispatchMouseDown(header);
    expect(firstHint.hidden).toBe(true);
    expect(secondHint.hidden).toBe(true);
    expect(thirdHint.hidden).toBe(true);
    expect(first.getAttribute("data-wheel-active")).toBeNull();
    expect(second.getAttribute("data-wheel-active")).toBeNull();

    dispatchMouseDown(second);
    dispatchMouseDown(document.body);
    expect(firstHint.hidden).toBe(true);
    expect(secondHint.hidden).toBe(true);
    expect(second.getAttribute("data-wheel-active")).toBeNull();

    dispatchMouseDown(first);
    window.dispatchEvent(new Event("blur"));
    expect(firstHint.hidden).toBe(true);
    expect(secondHint.hidden).toBe(true);
    expect(first.getAttribute("data-wheel-active")).toBeNull();
  });

  it("does not reveal a hint through Tab or programmatic focus", async () => {
    mount();
    await screen.findByLabelText("Planning workspace");
    const pane = screen.getByTestId("pane-m1");
    const hint = screen.getByTestId("hint-m1") as HTMLSpanElement;

    fireEvent.keyDown(document.body, { key: "Tab" });
    pane.focus();

    expect(hint.hidden).toBe(true);
    expect(pane.getAttribute("data-wheel-active")).toBeNull();
  });

  it("pans horizontally for a default vertical mouse wheel", async () => {
    mount();
    const workspace = await screen.findByLabelText("Planning workspace");
    defineScrollMetrics(workspace, { scrollWidth: 1600, clientWidth: 500 });

    const event = dispatchWheel(workspace, { deltaY: 48 });

    expect(event.defaultPrevented).toBe(true);
    expect(workspace.scrollLeft).toBe(48);
  });

  it("keeps native vertical scrolling after a primary click on a plan scrollport", async () => {
    mount();
    const workspace = await screen.findByLabelText("Planning workspace");
    const pane = screen.getByTestId("pane-m1");
    defineScrollMetrics(workspace, { scrollWidth: 1600, clientWidth: 500 });
    defineScrollMetrics(pane, { scrollHeight: 1200, clientHeight: 300 });

    dispatchMouseDown(pane);
    const event = dispatchWheel(pane, { deltaY: 48 });

    expect(event.defaultPrevented).toBe(false);
    expect(workspace.scrollLeft).toBe(0);
  });

  it("does not activate a pane through Tab or programmatic focus", async () => {
    mount();
    const workspace = await screen.findByLabelText("Planning workspace");
    const pane = screen.getByTestId("pane-m1");
    defineScrollMetrics(workspace, { scrollWidth: 1600, clientWidth: 500 });
    defineScrollMetrics(pane, { scrollHeight: 1200, clientHeight: 300 });

    fireEvent.keyDown(document.body, { key: "Tab" });
    pane.focus();
    const event = dispatchWheel(pane, { deltaY: 48 });

    expect(event.defaultPrevented).toBe(true);
    expect(workspace.scrollLeft).toBe(48);
  });

  it("returns the old pane to horizontal mode after clicking outside or another pane", async () => {
    mount();
    const workspace = await screen.findByLabelText("Planning workspace");
    const first = screen.getByTestId("pane-m1");
    const second = screen.getByTestId("pane-w1");
    defineScrollMetrics(workspace, { scrollWidth: 1600, clientWidth: 500 });
    defineScrollMetrics(first, { scrollHeight: 1200, clientHeight: 300 });
    defineScrollMetrics(second, { scrollHeight: 1200, clientHeight: 300 });

    dispatchMouseDown(first);
    expect(dispatchWheel(first, { deltaY: 24 }).defaultPrevented).toBe(false);
    dispatchMouseDown(document.body);
    const outsideEvent = dispatchWheel(first, { deltaY: 24 });
    expect(outsideEvent.defaultPrevented).toBe(true);
    expect(workspace.scrollLeft).toBe(24);

    dispatchMouseDown(second);
    expect(dispatchWheel(second, { deltaY: 24 }).defaultPrevented).toBe(false);
    const oldPaneEvent = dispatchWheel(first, { deltaY: 24 });
    expect(oldPaneEvent.defaultPrevented).toBe(true);
    expect(workspace.scrollLeft).toBe(48);
  });

  it("requires the primary button and ignores hover over another pane", async () => {
    mount();
    const workspace = await screen.findByLabelText("Planning workspace");
    const first = screen.getByTestId("pane-m1");
    const second = screen.getByTestId("pane-w1");
    defineScrollMetrics(workspace, { scrollWidth: 1600, clientWidth: 500 });
    defineScrollMetrics(first, { scrollHeight: 1200, clientHeight: 300 });
    defineScrollMetrics(second, { scrollHeight: 1200, clientHeight: 300 });

    dispatchMouseDown(first, { button: 2 });
    fireEvent.mouseOver(second);
    const rightClickEvent = dispatchWheel(first, { deltaY: 24 });
    expect(rightClickEvent.defaultPrevented).toBe(true);
    expect(workspace.scrollLeft).toBe(24);

    dispatchMouseDown(first);
    fireEvent.mouseOver(second);
    const hoverEvent = dispatchWheel(second, { deltaY: 24 });
    expect(hoverEvent.defaultPrevented).toBe(true);
    expect(workspace.scrollLeft).toBe(48);
  });

  it("leaves horizontal-dominant gestures and browser zoom untouched", async () => {
    mount();
    const workspace = await screen.findByLabelText("Planning workspace");
    defineScrollMetrics(workspace, { scrollWidth: 1600, clientWidth: 500 });

    const horizontal = dispatchWheel(workspace, { deltaX: 48, deltaY: 12 });
    expect(horizontal.defaultPrevented).toBe(false);
    expect(workspace.scrollLeft).toBe(0);

    const ctrlZoom = dispatchWheel(workspace, { deltaY: 48, ctrlKey: true });
    const metaZoom = dispatchWheel(workspace, { deltaY: 48, metaKey: true });
    expect(ctrlZoom.defaultPrevented).toBe(false);
    expect(metaZoom.defaultPrevented).toBe(false);
    expect(workspace.scrollLeft).toBe(0);
  });

  it("routes a vertical-dominant gesture with micro horizontal drift by mode", async () => {
    mount();
    const workspace = await screen.findByLabelText("Planning workspace");
    const pane = screen.getByTestId("pane-m1");
    defineScrollMetrics(workspace, { scrollWidth: 1600, clientWidth: 500 });
    defineScrollMetrics(pane, { scrollHeight: 1200, clientHeight: 300 });

    const inactive = dispatchWheel(pane, { deltaX: 1, deltaY: 48 });
    expect(inactive.defaultPrevented).toBe(true);
    expect(workspace.scrollLeft).toBe(48);

    dispatchMouseDown(pane);
    defineScrollTop(pane, 0);
    const active = dispatchWheel(pane, { deltaX: 1, deltaY: 48 });
    expect(active.defaultPrevented).toBe(true);
    expect(pane.scrollTop).toBe(48);
    expect(workspace.scrollLeft).toBe(48);
  });

  it("keeps an active empty pane native instead of falling back to horizontal", async () => {
    mount();
    const workspace = await screen.findByLabelText("Planning workspace");
    const pane = screen.getByTestId("pane-m1");
    defineScrollMetrics(workspace, { scrollWidth: 1600, clientWidth: 500 });
    defineScrollMetrics(pane, { scrollHeight: 300, clientHeight: 300 });

    dispatchMouseDown(pane);
    const event = dispatchWheel(pane, { deltaY: 48 });

    expect(event.defaultPrevented).toBe(false);
    expect(workspace.scrollLeft).toBe(0);
  });

  it("keeps native vertical mode at both scroll boundaries", async () => {
    mount();
    const workspace = await screen.findByLabelText("Planning workspace");
    const pane = screen.getByTestId("pane-m1");
    defineScrollMetrics(workspace, { scrollWidth: 1600, clientWidth: 500 });
    defineScrollMetrics(pane, { scrollHeight: 1200, clientHeight: 300 });
    dispatchMouseDown(pane);

    defineScrollTop(pane, 0);
    const top = dispatchWheel(pane, { deltaY: -48 });
    defineScrollTop(pane, 900);
    const bottom = dispatchWheel(pane, { deltaY: 48 });

    expect(top.defaultPrevented).toBe(false);
    expect(bottom.defaultPrevented).toBe(false);
    expect(workspace.scrollLeft).toBe(0);
  });

  it("does not let an unselected task textarea intercept workspace browsing", async () => {
    mount();
    const workspace = await screen.findByLabelText("Planning workspace");
    const pane = screen.getByTestId("pane-m1");
    const title = document.createElement("textarea");
    pane.append(title);
    defineScrollMetrics(workspace, { scrollWidth: 1600, clientWidth: 500 });
    defineScrollMetrics(pane, { scrollHeight: 1200, clientHeight: 300 });
    title.focus();
    expect(dispatchWheel(title, { deltaY: 24 }).defaultPrevented).toBe(true);
    dispatchMouseDown(title);
    expect(dispatchWheel(title, { deltaY: 24 }).defaultPrevented).toBe(false);
  });

  it("treats the actual header outside a scrollport as an outside click", async () => {
    mount();
    const workspace = await screen.findByLabelText("Planning workspace");
    const pane = screen.getByTestId("pane-m1");
    const header = screen.getByTestId("header-m1");
    defineScrollMetrics(workspace, { scrollWidth: 1600, clientWidth: 500 });
    defineScrollMetrics(pane, { scrollHeight: 1200, clientHeight: 300 });

    dispatchMouseDown(pane);
    expect(dispatchWheel(pane, { deltaY: 24 }).defaultPrevented).toBe(false);
    dispatchMouseDown(header);
    const event = dispatchWheel(pane, { deltaY: 24 });

    expect(event.defaultPrevented).toBe(true);
    expect(workspace.scrollLeft).toBe(24);
  });

  it("stops routing while the workspace is hidden and recovers on show", async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const mounted = render(<QueryClientProvider client={queryClient}><PlannerWorkspace active /></QueryClientProvider>);
    const workspace = await screen.findByLabelText("Planning workspace");
    const pane = screen.getByTestId("pane-m1");
    const hint = screen.getByTestId("hint-m1") as HTMLSpanElement;
    defineScrollMetrics(workspace, { scrollWidth: 1600, clientWidth: 500 });
    defineScrollMetrics(pane, { scrollHeight: 1200, clientHeight: 300 });
    dispatchMouseDown(pane);
    expect(hint.hidden).toBe(false);

    mounted.rerender(<QueryClientProvider client={queryClient}><PlannerWorkspace active={false} /></QueryClientProvider>);
    const hidden = dispatchWheel(pane, { deltaY: 24 });
    expect(hidden.defaultPrevented).toBe(false);
    expect(workspace.scrollLeft).toBe(0);
    expect(hint.hidden).toBe(true);
    expect(pane.getAttribute("data-wheel-active")).toBeNull();

    mounted.rerender(<QueryClientProvider client={queryClient}><PlannerWorkspace active /></QueryClientProvider>);
    const restored = dispatchWheel(pane, { deltaY: 24 });
    expect(restored.defaultPrevented).toBe(true);
    expect(workspace.scrollLeft).toBe(24);
  });

});

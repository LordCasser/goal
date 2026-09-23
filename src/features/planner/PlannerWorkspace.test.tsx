import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, fireEvent, within, waitFor } from "@testing-library/react";
import { useEffect, useRef } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { Cycle } from "../../lib/ipc";
import { qk } from "../../lib/events";
const mocks = vi.hoisted(() => ({ getEditorWorkspace: vi.fn(), listSessions: vi.fn(), getPlannerState: vi.fn(), getEditorWorkspacesByCycleIds: vi.fn(), createPlanningCycle: vi.fn(), ensureDay: vi.fn(), getSettings: vi.fn() }));
vi.mock("../../lib/ipc", async (original) => ({ ...(await original<typeof import("../../lib/ipc")>()), ...mocks }));
vi.mock("./CycleColumn", () => ({
  CycleColumn: ({ cycle, relations, revealTask }: { cycle: Cycle; relations?: { tasks: Map<string, { id: string; cycle_id: string }>; selectedId: string | null; select: (id: string) => void }; revealTask?: { cycleId: string; taskId: string; requestId: number } }) => {
    const handled = useRef<number | undefined>(undefined);
    useEffect(() => {
      if (!revealTask || revealTask.cycleId !== cycle.id || handled.current === revealTask.requestId) return;
      const row = document.querySelector<HTMLElement>(`[data-task-id="${revealTask.taskId}"]`);
      if (!row) return;
      handled.current = revealTask.requestId;
      relations?.select(revealTask.taskId);
      row.scrollIntoView({ block: "nearest", inline: "center", behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth" });
    }, [cycle.id, relations, revealTask]);
    const rows = [...(relations?.tasks.values() ?? [])].filter((task) => task.cycle_id === cycle.id);
    return (
      <article>
        <header data-testid={`header-${cycle.id}`} className="workspace-scroll-heading">
          <span data-workspace-scroll-hint hidden data-testid={`hint-${cycle.id}`} />
        </header>
        <div data-testid={`pane-${cycle.id}`} data-workspace-scroll-pane className="overflow-y-auto" tabIndex={-1}>
          {cycle.id}
          {rows.map((task) => <div key={task.id} data-testid={`task-${task.id}`} data-task-id={task.id} data-selected={relations?.selectedId === task.id || undefined} onClick={() => relations?.select(task.id)}>{task.id}</div>)}
        </div>
      </article>
    );
  },
}));
vi.mock("./dates", async (original) => ({ ...(await original<typeof import("./dates")>()), todayISO: () => "2026-09-15" }));
import { PlannerWorkspace } from "./PlannerWorkspace";
import { PLAN_TRANSITION_LEAVE_MS } from "./PlanTransition";
function cycle(id: string, type: Cycle["type"], parent_id: string | null, starts_on: string, ends_on: string): Cycle {
  return { id, type, parent_id, starts_on, ends_on, position: 0, title: id, finished: false } as Cycle;
}
function relationTask(id: string, cycle_id: string, parent_id: string | null = null) {
  return { id, cycle_id, parent_id, title: id, children: [], subtasks: [], completed: false } as never;
}
const cycles = [cycle("m1", "month", null, "2026-09-01", "2026-12-01"), cycle("m2", "month", null, "2026-12-01", "2027-03-01"), cycle("w1", "week", "m1", "2026-09-14", "2026-09-21"), cycle("w2", "week", "m1", "2026-09-21", "2026-09-28"), cycle("d1", "day", "w1", "2026-09-15", "2026-09-16"), cycle("d2", "day", "w2", "2026-09-22", "2026-09-23")];
afterEach(() => vi.useRealTimers());
beforeEach(() => { mocks.getEditorWorkspace.mockResolvedValue({ tasks: [], work_mix: null }); mocks.listSessions.mockResolvedValue([]); mocks.getSettings.mockResolvedValue({ week_start_day: 1, locale: "en", theme: "white", show_relation_lines: true }); mocks.getPlannerState.mockResolvedValue({ cycles }); mocks.getEditorWorkspacesByCycleIds.mockResolvedValue({}); });
function workspaceElement(props: Partial<import("react").ComponentProps<typeof PlannerWorkspace>> = {}, client = new QueryClient({ defaultOptions: { queries: { retry: false } } })) {
  return <QueryClientProvider client={client}><PlannerWorkspace {...props} /></QueryClientProvider>;
}
function mount(props: Partial<import("react").ComponentProps<typeof PlannerWorkspace>> = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return { ...render(workspaceElement(props, client)), client };
}
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
  it("shows relation lines by default and responds to the shared settings preference", async () => {
    const { client, container } = mount();
    await screen.findByText("d1");
    await waitFor(() => expect(container.querySelector("[data-relation-layer]")).toBeTruthy());
    act(() => client.setQueryData(qk.settings(), { week_start_day: 1, locale: "en", theme: "white", show_relation_lines: false }));
    await waitFor(() => expect(container.querySelector("[data-relation-layer]")).toBeNull());
    act(() => client.setQueryData(qk.settings(), { week_start_day: 1, locale: "en", theme: "white", show_relation_lines: true }));
    await waitFor(() => expect(container.querySelector("[data-relation-layer]")).toBeTruthy());
  });
  const weekList = () => screen.getByRole("listbox", { name: "Weeks" });
  const dayList = () => screen.getByRole("listbox", { name: "Days" });
  const articles = () => screen.getAllByRole("article").map((node) => node.textContent);
  const settle = async () => {
    for (let frame = 0; frame < 60; frame++) await act(() => vi.advanceTimersByTimeAsync(20));
  };
  it("keeps both old panels until the target tasks and focus blocks are ready, then swaps together", async () => {
    let tasksReady!: (value: unknown) => void;
    let sessionsReady!: (value: unknown[]) => void;
    const tasks = new Promise((resolve) => { tasksReady = resolve; });
    const sessions = new Promise<unknown[]>((resolve) => { sessionsReady = resolve; });
    mocks.getEditorWorkspace.mockImplementation((id: string) => id === "w2" || id === "d2" ? tasks : Promise.resolve({ tasks: [], work_mix: null }));
    mocks.listSessions.mockImplementation((id: string) => id === "d2" ? sessions : Promise.resolve([]));
    mount();
    await screen.findByText("d1");
    vi.useFakeTimers();
    fireEvent.keyDown(dayList(), { key: "PageDown" });
    fireEvent.keyDown(dayList(), { key: "ArrowDown" });
    fireEvent.keyDown(dayList(), { key: "ArrowDown" });
    await settle();
    expect(articles()).toEqual(["m1", "w1", "d1"]);
    expect(screen.getByText("w1").closest(".plan-transition")?.hasAttribute("inert")).toBe(true);
    expect(screen.getByText("d1").closest(".plan-transition")?.hasAttribute("inert")).toBe(true);
    await act(async () => { tasksReady({ tasks: [], work_mix: null }); });
    await act(() => vi.advanceTimersByTimeAsync(20));
    expect(articles()).toEqual(["m1", "w1", "d1"]);
    await act(async () => { sessionsReady([]); });
    await act(() => vi.advanceTimersByTimeAsync(20));
    expect(articles()).toEqual(["m1", "w1", "d1"]);
    await act(() => vi.advanceTimersByTimeAsync(PLAN_TRANSITION_LEAVE_MS));
    expect(articles()).toEqual(["m1", "w2", "d2"]);
    expect([...document.querySelectorAll(".plan-transition")].map((node) => node.getAttribute("data-phase"))).toEqual(["idle", "entering", "entering"]);
  });
  it("does not show a stale target when its data arrives after another navigation", async () => {
    let resolveOld!: (value: unknown) => void;
    const oldData = new Promise((resolve) => { resolveOld = resolve; });
    mocks.getEditorWorkspace.mockImplementation((id: string) => id === "w2" ? oldData : Promise.resolve({ tasks: [], work_mix: null }));
    mount();
    await screen.findByText("d1");
    vi.useFakeTimers();
    fireEvent.click(within(weekList()).getByRole("option", { name: /W39/ }));
    await settle();
    expect(articles()).toEqual(["m1", "w1", "d1"]);
    fireEvent.click(within(weekList()).getByRole("option", { name: /This week/ }));
    await settle();
    expect(articles()).toEqual(["m1", "w1"]);
    await act(async () => { resolveOld({ tasks: [], work_mix: null }); });
    await settle();
    expect(articles()).toEqual(["m1", "w1"]);
    expect(screen.getByRole("button", { name: "Create daily plan" })).toBeTruthy();
  });
  it("keeps weekly and daily tasks visible without a long-term cycle", async () => {
    mocks.getPlannerState.mockResolvedValue({ cycles: [{ ...cycles[2], parent_id: null }, { ...cycles[4], parent_id: null }] });
    mount();
    await screen.findByText("w1");
    expect(articles()).toEqual(["w1", "d1"]);
    expect(within(weekList()).getAllByRole("option").filter((row) => row.hasAttribute("aria-current"))).toHaveLength(1);
  });
  it("selects an open long-term cycle after its start date", async () => {
    mocks.getPlannerState.mockResolvedValue({ cycles: [
      { ...cycles[0], id: "open", starts_on: "2026-09-01", ends_on: null, duration: null },
      { ...cycles[1], starts_on: "2026-10-01", ends_on: "2026-12-01" },
      ...cycles.slice(2),
    ] });
    mount();
    await screen.findByTestId("header-open");
    expect(articles()[0]).toBe("open");
  });
  it("clicking a week opens its first date, without creating an empty day", async () => {
    mount();
    await screen.findByText("m1");
    fireEvent.click(within(weekList()).getByRole("option", { name: /W39/ }));
    await screen.findByText("w2");
    await waitFor(() => expect(articles()).toEqual(["m1", "w2"]));
    expect(screen.getByRole("button", { name: "Create daily plan" })).toBeTruthy();
    expect(mocks.ensureDay).not.toHaveBeenCalled();
  });
  it("switching long-term goals preserves week and day", async () => {
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "Dec 1" }));
    await screen.findByTestId("header-m2");
    expect(articles()).toEqual(["m2", "w1", "d1"]);
  });
  it("reveals a related long-term task after its replacement panel mounts", async () => {
    const goal = relationTask("goal", "m2");
    const daily = relationTask("daily", "d1", "goal");
    mocks.getEditorWorkspacesByCycleIds.mockResolvedValue({ m2: { tasks: [goal] }, d1: { tasks: [daily] } });
    let resolveMonth!: (value: unknown) => void;
    const monthReady = new Promise((resolve) => { resolveMonth = resolve; });
    mocks.getEditorWorkspace.mockImplementation((id: string) => id === "m2" ? monthReady : Promise.resolve({ tasks: [], work_mix: null }));
    vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
    const scroll = vi.fn();
    Element.prototype.scrollIntoView = scroll;
    mount();
    await screen.findByTestId("task-daily");
    fireEvent.click(screen.getByTestId("task-daily"));
    const locate = await screen.findByRole("button", { name: /goal/i });
    vi.useFakeTimers();
    fireEvent.click(locate);
    expect(scroll).not.toHaveBeenCalled();
    expect(screen.queryByTestId("task-goal")).toBeNull();
    await act(() => vi.advanceTimersByTimeAsync(PLAN_TRANSITION_LEAVE_MS));
    expect(screen.queryByTestId("task-goal")).toBeNull();
    await act(async () => { resolveMonth({ tasks: [goal], work_mix: null }); await Promise.resolve(); });
    await act(() => vi.advanceTimersByTimeAsync(PLAN_TRANSITION_LEAVE_MS));
    expect(screen.getByTestId("task-goal")).toBeTruthy();
    await act(async () => { await Promise.resolve(); });
    expect(scroll).toHaveBeenCalledWith({ block: "nearest", inline: "center", behavior: "smooth" });
    expect(screen.getByTestId("task-goal").getAttribute("data-selected")).toBe("true");
    vi.unstubAllGlobals();
  });
  it("smoothly falls back to the remaining long-term cycle after deletion", async () => {
    const mounted = mount();
    await screen.findByTestId("header-m1");
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    vi.useFakeTimers();

    await act(async () => {
      mounted.client.setQueryData(qk.plannerState(), { cycles: cycles.filter((cycle) => cycle.id !== "m1") });
      await Promise.resolve();
      await Promise.resolve();
    });
    await act(() => vi.advanceTimersByTimeAsync(0));
    expect(articles()).toEqual(["m1", "w1", "d1"]);

    await act(() => vi.advanceTimersByTimeAsync(PLAN_TRANSITION_LEAVE_MS));
    await act(() => vi.advanceTimersByTimeAsync(PLAN_TRANSITION_LEAVE_MS));
    expect(articles()).toEqual(["m2", "w1", "d1"]);
  });
  it("smoothly replaces the long-term cycle with its empty state after the last one is deleted", async () => {
    const mounted = mount();
    await screen.findByTestId("header-m1");
    vi.useFakeTimers();

    act(() => mounted.client.setQueryData(qk.plannerState(), { cycles: cycles.filter((cycle) => cycle.type !== "month") }));
    expect(articles()).toEqual(["m1", "w1", "d1"]);

    await act(() => vi.advanceTimersByTimeAsync(PLAN_TRANSITION_LEAVE_MS));
    await act(() => vi.advanceTimersByTimeAsync(PLAN_TRANSITION_LEAVE_MS));
    expect(screen.getByRole("button", { name: "Set long-term goals" })).toBeTruthy();
    expect(articles()).toEqual(["w1", "d1"]);
  });
  it("browses dates with no saved week or day without writing data", async () => {
    mount();
    await screen.findByText("d1");
    vi.useFakeTimers();
    fireEvent.keyDown(dayList(), { key: "PageDown" });
    fireEvent.keyDown(dayList(), { key: "PageDown" });
    fireEvent.keyDown(dayList(), { key: "PageDown" });
    expect(articles()).toEqual(["m1", "w1", "d1"]);
    await settle();
    expect(articles()).toEqual(["m1"]);
    expect(mocks.ensureDay).not.toHaveBeenCalled();
    expect(mocks.createPlanningCycle).not.toHaveBeenCalled();
  });
  it("waits for the final wheel position before switching both day and week", async () => {
    mount();
    await screen.findByText("d1");
    vi.useFakeTimers();
    fireEvent.wheel(dayList(), { deltaY: 144 });
    act(() => vi.advanceTimersByTime(300));
    fireEvent.wheel(dayList(), { deltaY: 144 });
    act(() => vi.advanceTimersByTime(300));
    fireEvent.wheel(dayList(), { deltaY: 48 });
    act(() => vi.advanceTimersByTime(499));
    expect(articles()).toEqual(["m1", "w1", "d1"]);
    await settle();
    expect(articles()).toEqual(["m1", "w2", "d2"]);
    await settle();
    const selected = within(weekList()).getAllByRole("option").find((row) => row.getAttribute("aria-selected") === "true");
    expect(selected?.textContent).toContain("W39");
    expect(mocks.ensureDay).not.toHaveBeenCalled();
  });
  it("gives the latest drum gesture priority over an older pending gesture", async () => {
    mount();
    await screen.findByText("d1");
    vi.useFakeTimers();
    fireEvent.wheel(dayList(), { deltaY: 144 });
    act(() => vi.advanceTimersByTime(200));
    fireEvent.wheel(weekList(), { deltaY: 48 });
    await settle();
    expect(articles()).toEqual(["m1", "w2"]);
    await settle();
    expect(within(dayList()).getAllByRole("option").find((row) => row.getAttribute("aria-selected") === "true")?.textContent).toContain("Sep 21");
  });
  it("cancels a pending day gesture when a month or task pane is clicked", async () => {
    mount();
    await screen.findByText("d1");
    vi.useFakeTimers();

    fireEvent.wheel(dayList(), { deltaY: 48 });
    const month = screen.getByRole("button", { name: "Dec 1" });
    fireEvent.pointerDown(month, { button: 0 });
    fireEvent.click(month);
    await settle();
    await settle();

    let selectedDay = within(dayList()).getAllByRole("option").find((row) => row.getAttribute("aria-selected") === "true");
    expect(selectedDay?.textContent).toContain("Sep 15");
    expect(articles()).toEqual(["m2", "w1", "d1"]);

    fireEvent.wheel(dayList(), { deltaY: 48 });
    fireEvent.pointerDown(screen.getByTestId("pane-m2"), { button: 0 });
    await settle();
    await settle();

    selectedDay = within(dayList()).getAllByRole("option").find((row) => row.getAttribute("aria-selected") === "true");
    expect(selectedDay?.textContent).toContain("Sep 15");
  });
  it("keeps a sub-half-row week gesture uncommitted and returns the day drum to its prop date", async () => {
    mount();
    await screen.findByText("d1");
    vi.useFakeTimers();

    fireEvent.wheel(dayList(), { deltaY: 48 });
    fireEvent.wheel(weekList(), { deltaY: 23 });
    await settle();
    await settle();

    const selectedWeek = within(weekList()).getAllByRole("option").find((row) => row.getAttribute("aria-selected") === "true");
    const selectedDay = within(dayList()).getAllByRole("option").find((row) => row.getAttribute("aria-selected") === "true");
    expect(selectedWeek?.textContent).toContain("W38");
    expect(selectedDay?.textContent).toContain("Sep 15");
    expect(articles()).toEqual(["m1", "w1", "d1"]);
  });
  it("uses Sunday for virtual weeks when configured", async () => {
    mocks.getSettings.mockResolvedValue({ week_start_day: 7, locale: "en", theme: "white" });
    mocks.getPlannerState.mockResolvedValue({ cycles: [cycles[0]] });
    mount();
    await screen.findByText("m1");
    expect(within(weekList()).getAllByRole("option").find((row) => row.getAttribute("aria-selected") === "true")?.textContent).toContain("Sep 13");
  });
  it("hovering a drum owns both wheel axes; outside wheel still pans horizontally", async () => {
    mount();
    const horizontal = await screen.findByLabelText("Planning workspace");
    defineScrollMetrics(horizontal, { scrollWidth: 2400, clientWidth: 1000 });
    const wheel = dispatchWheel(dayList(), { deltaY: 48, deltaX: 3 });
    expect(wheel.defaultPrevented).toBe(true);
    expect(horizontal.scrollLeft).toBe(0);
    dispatchWheel(horizontal, { deltaY: 80 });
    expect(horizontal.scrollLeft).toBe(80);
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

  it("normalizes line and page wheel units for workspace and active-pane routing", async () => {
    mount();
    const workspace = await screen.findByLabelText("Planning workspace");
    const pane = screen.getByTestId("pane-m1");
    defineScrollMetrics(workspace, { scrollWidth: 1600, clientWidth: 500, clientHeight: 360 });
    defineScrollMetrics(pane, { scrollHeight: 1200, clientHeight: 240 });

    dispatchWheel(workspace, { deltaY: 2, deltaMode: WheelEvent.DOM_DELTA_LINE });
    expect(workspace.scrollLeft).toBe(32);
    dispatchWheel(workspace, { deltaY: 1, deltaMode: WheelEvent.DOM_DELTA_PAGE });
    expect(workspace.scrollLeft).toBe(392);

    dispatchMouseDown(pane);
    const activePageWheel = dispatchWheel(pane, {
      deltaX: 0.5,
      deltaY: 1,
      deltaMode: WheelEvent.DOM_DELTA_PAGE,
    });
    expect(activePageWheel.defaultPrevented).toBe(true);
    expect(pane.scrollTop).toBe(240);
    expect(workspace.scrollLeft).toBe(392);
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

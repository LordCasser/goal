import { useRef } from "react";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Cycle, TaskNode } from "../../lib/ipc";
import { indexTasks, type RelationView } from "./relations";
import { useDailyGoalTooltip } from "./useDailyGoalTooltip";

const goal = { id: "goal", cycle_id: "m", title: "Validate the product", parent_id: null, root_color_key: "amber", children: [] } as unknown as TaskNode;
const daily = { ...goal, id: "daily", cycle_id: "d", title: "Interview a user", root_color_key: null, parent_id: "goal" };
const relations: RelationView = {
  tasks: indexTasks([[goal], [daily]]), cycles: new Map([["m", { id: "m", type: "month" } as Cycle], ["d", { id: "d", type: "day" } as Cycle]]),
  selectedId: null, highlighted: new Set(), select: vi.fn(), preview: vi.fn(),
};
function Example({ active = true, task = daily }: { active?: boolean; task?: TaskNode }) {
  const ref = useRef<HTMLDivElement>(null);
  const hint = useDailyGoalTooltip(task, relations, active, ref);
  return <><div data-testid="row" ref={ref} {...hint.rowProps} onClick={relations.select.bind(null, task.id)}>
    <textarea defaultValue={task.title} aria-describedby={hint.descriptionId} />
    <button type="button">Change parent</button>{hint.tooltip}
  </div><button type="button">Outside</button></>;
}
const advance = (ms: number) => act(() => vi.advanceTimersByTime(ms));
function hover() { fireEvent.mouseEnter(screen.getByTestId("row")); advance(360); }
beforeEach(() => {
  vi.useFakeTimers(); vi.clearAllMocks();
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
});
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

describe("daily goal hover context", () => {
  it("waits for deliberate row hover and reveals the offscreen goal without moving focus", () => {
    render(<Example />);
    fireEvent.mouseEnter(screen.getByTestId("row"));
    advance(300);
    expect(screen.queryByRole("tooltip")).toBeNull();
    advance(60);
    expect(screen.getByRole("tooltip").textContent).toContain(goal.title);
    expect(screen.getByRole("tooltip").textContent).toContain("Linked long-term goal");
    expect(screen.getByRole("textbox").getAttribute("aria-describedby")).toBe(screen.getByRole("tooltip").id);
    expect(document.activeElement).toBe(document.body);
  });
  it("cancels fly-by hovers and lets the pointer cross into the card before fading out", () => {
    render(<Example />);
    fireEvent.mouseEnter(screen.getByTestId("row")); advance(100);
    fireEvent.mouseLeave(screen.getByTestId("row")); advance(500);
    expect(screen.queryByRole("tooltip")).toBeNull();
    hover();
    const card = screen.getByRole("tooltip");
    fireEvent.mouseLeave(screen.getByTestId("row"), { relatedTarget: card }); advance(80);
    fireEvent.mouseEnter(card); advance(500);
    expect(screen.getByRole("tooltip")).toBe(card);
    fireEvent.mouseLeave(card); advance(140);
    expect(card.getAttribute("data-closing")).toBe("true");
    advance(120);
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
  it("dismisses the goal hint when local ordering starts", () => {
    render(<Example />); hover();
    fireEvent.dragStart(screen.getByTestId("row"));
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
  it("supports keyboard focus and Escape without stealing or blurring editor focus", () => {
    render(<Example />);
    const input = screen.getByRole("textbox");
    act(() => input.focus()); advance(360);
    expect(screen.getByRole("tooltip")).toBeTruthy();
    fireEvent.keyDown(input, { key: "Escape" }); advance(500);
    expect(screen.queryByRole("tooltip")).toBeNull();
    expect(document.activeElement).toBe(input);
    act(() => screen.getByText("Outside").focus());
    act(() => input.focus()); advance(360);
    expect(screen.getByRole("tooltip")).toBeTruthy();
    fireEvent.keyDown(input, { key: "a" });
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
  it("keeps long goal text readable and does not select the source task when clicking the card", () => {
    render(<Example />); hover();
    const card = screen.getByRole("tooltip");
    fireEvent.pointerDown(card); fireEvent.click(card); fireEvent.scroll(card);
    expect(screen.getByRole("tooltip")).toBe(card);
    expect(relations.select).not.toHaveBeenCalled();
  });
  it.each(["pointerdown", "scroll", "resize", "blur"])("dismisses on %s, including a pending reveal", (event) => {
    render(<Example />); hover();
    fireEvent(event === "pointerdown" ? document : window, new Event(event, { bubbles: true }));
    expect(screen.queryByRole("tooltip")).toBeNull();
    fireEvent.mouseEnter(screen.getByTestId("row")); advance(50);
    fireEvent(event === "pointerdown" ? document : window, new Event(event, { bubbles: true }));
    advance(500);
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
  it("does not reopen during a mouse click that starts editing or opens a menu", () => {
    render(<Example />); hover();
    fireEvent.pointerDown(screen.getByRole("textbox"));
    act(() => screen.getByRole("textbox").focus()); advance(500);
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
  it("does not reveal under an open menu or dialog when the pointer returns to a row", () => {
    const view = render(<><Example /><div role="menu">Parent picker</div></>);
    hover(); expect(screen.queryByRole("tooltip")).toBeNull();
    view.rerender(<><Example /><div role="dialog">Settings</div></>);
    hover(); expect(screen.queryByRole("tooltip")).toBeNull();
  });
  it("cleans up when a row loses its link, becomes inactive, or unmounts", () => {
    const view = render(<Example />); hover();
    view.rerender(<Example task={{ ...daily, parent_id: null }} />);
    expect(screen.queryByRole("tooltip")).toBeNull();
    view.rerender(<Example />); hover();
    view.rerender(<Example active={false} />);
    expect(screen.queryByRole("tooltip")).toBeNull();
    view.rerender(<Example />);
    fireEvent.mouseEnter(screen.getByTestId("row"));
    view.unmount(); advance(1000);
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
  it("dismisses when the retained row's ancestor becomes inert", async () => {
    render(<Example />); hover();
    await act(async () => { screen.getByTestId("row").parentElement!.setAttribute("inert", ""); });
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
  it("flips above the row and stays inside the right viewport edge", () => {
    const rect = (x: number, y: number, width: number, height: number) => ({ left: x, top: y, right: x + width, bottom: y + height, width, height } as DOMRect);
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      return this.getAttribute("role") === "tooltip" ? rect(0, 0, 288, 96) : rect(window.innerWidth - 200, window.innerHeight - 70, 300, 36);
    });
    render(<Example />); hover();
    const card = screen.getByRole("tooltip");
    expect(card.dataset.above).toBe("true");
    expect(Number.parseFloat(card.style.left) + 288).toBeLessThanOrEqual(window.innerWidth - 10);
    expect(Number.parseFloat(card.style.top) + 96).toBeLessThan(window.innerHeight - 70);
  });
});

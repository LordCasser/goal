import { StrictMode, type ReactNode } from "react";
import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { PLAN_TRANSITION_ENTER_MS, PLAN_TRANSITION_LEAVE_MS, PlanTransition } from "./PlanTransition";

function panel(identity: string, label: string, ready = true): ReactNode {
  return <PlanTransition identity={identity} ready={ready}><button type="button">{label}</button></PlanTransition>;
}

function phase(): string | null {
  return screen.getByText(/./).parentElement?.getAttribute("data-phase") ?? null;
}

describe("PlanTransition", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("updates children immediately when the identity is unchanged", () => {
    const view = render(panel("day-a", "first"));
    view.rerender(panel("day-a", "refreshed"));

    expect(screen.getByRole("button", { name: "refreshed" })).toBeTruthy();
    expect(phase()).toBe("idle");
    expect(screen.getAllByRole("button")).toHaveLength(1);
  });

  it("keeps the old panel while the next identity is not ready", () => {
    const view = render(panel("day-a", "old"));
    view.rerender(panel("day-b", "new", false));

    const wrapper = screen.getByRole("button", { name: "old" }).parentElement!;
    expect(wrapper.getAttribute("data-phase")).toBe("waiting");
    expect(wrapper.hasAttribute("inert")).toBe(true);
    expect(screen.queryByRole("button", { name: "new" })).toBeNull();

    act(() => vi.advanceTimersByTime(PLAN_TRANSITION_LEAVE_MS + 20));
    expect(screen.getByRole("button", { name: "old" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "new" })).toBeNull();

    view.rerender(panel("day-b", "new", true));
    expect(screen.getByRole("button", { name: "old" })).toBeTruthy();
    expect(phase()).toBe("leaving");
    act(() => vi.advanceTimersByTime(PLAN_TRANSITION_LEAVE_MS));
    expect(screen.getByRole("button", { name: "new" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "old" })).toBeNull();
    expect(phase()).toBe("entering");
    expect(screen.getByRole("button", { name: "new" }).parentElement?.hasAttribute("inert")).toBe(false);

    act(() => vi.advanceTimersByTime(PLAN_TRANSITION_ENTER_MS));
    expect(phase()).toBe("idle");
  });

  it("cancels an interrupted target and only shows the latest identity", () => {
    const view = render(panel("day-a", "old"));
    view.rerender(panel("day-b", "middle"));
    act(() => vi.advanceTimersByTime(40));
    view.rerender(panel("day-c", "latest"));
    act(() => vi.advanceTimersByTime(PLAN_TRANSITION_LEAVE_MS));

    expect(screen.getByRole("button", { name: "latest" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "middle" })).toBeNull();
    expect(screen.queryByRole("button", { name: "old" })).toBeNull();
    expect(phase()).toBe("entering");

    act(() => vi.advanceTimersByTime(PLAN_TRANSITION_ENTER_MS));
    expect(phase()).toBe("idle");
  });

  it("cancels the transition when the props return to the displayed identity", () => {
    const view = render(panel("day-a", "old"));
    view.rerender(panel("day-b", "middle"));
    view.rerender(panel("day-a", "back"));

    expect(screen.getByRole("button", { name: "back" })).toBeTruthy();
    expect(phase()).toBe("idle");
    act(() => vi.advanceTimersByTime(PLAN_TRANSITION_LEAVE_MS + PLAN_TRANSITION_ENTER_MS));
    expect(phase()).toBe("idle");
  });

  it("starts a full fade after an interrupted target finishes loading", () => {
    const view = render(panel("day-a", "old"));
    view.rerender(panel("day-b", "skipped"));
    act(() => vi.advanceTimersByTime(PLAN_TRANSITION_LEAVE_MS / 2));
    view.rerender(panel("day-c", "latest", false));
    act(() => vi.advanceTimersByTime(1000));
    expect(phase()).toBe("waiting");
    expect(screen.queryByText("skipped")).toBeNull();
    view.rerender(panel("day-c", "latest"));
    act(() => vi.advanceTimersByTime(PLAN_TRANSITION_LEAVE_MS - 1));
    expect(screen.getByRole("button", { name: "old" })).toBeTruthy();
    act(() => vi.advanceTimersByTime(1));
    expect(screen.getByRole("button", { name: "latest" })).toBeTruthy();
    expect(phase()).toBe("entering");
  });

  it("can interrupt an entering panel without displaying a superseded target", () => {
    const view = render(panel("day-a", "first"));
    view.rerender(panel("day-b", "second"));
    act(() => vi.advanceTimersByTime(PLAN_TRANSITION_LEAVE_MS + 60));
    expect(phase()).toBe("entering");
    view.rerender(panel("day-c", "third"));
    act(() => vi.advanceTimersByTime(PLAN_TRANSITION_LEAVE_MS));
    expect(screen.getByRole("button", { name: "third" })).toBeTruthy();
    act(() => vi.advanceTimersByTime(PLAN_TRANSITION_ENTER_MS));
    expect(phase()).toBe("idle");
    expect(screen.getAllByRole("button")).toHaveLength(1);
  });

  it("replaces immediately under reduced motion while still waiting for ready data", () => {
    const media = { matches: true, media: "(prefers-reduced-motion: reduce)" } as MediaQueryList;
    const original = window.matchMedia;
    window.matchMedia = vi.fn(() => media);
    const view = render(panel("day-a", "old"));
    view.rerender(panel("day-b", "new", false));
    expect(screen.getByRole("button", { name: "old" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "new" })).toBeNull();

    view.rerender(panel("day-b", "new", true));
    expect(screen.getByRole("button", { name: "new" })).toBeTruthy();
    expect(phase()).toBe("idle");
    expect(screen.getByRole("button", { name: "new" }).parentElement?.hasAttribute("inert")).toBe(false);
    window.matchMedia = original;
  });

  it.each(["leaving", "entering"])("responds immediately to reduced motion during %s", (during) => {
    let listener: (() => void) | undefined;
    const media = {
      matches: false,
      media: "(prefers-reduced-motion: reduce)",
      addEventListener: (_: string, callback: () => void) => { listener = callback; },
      removeEventListener: () => {},
    } as unknown as MediaQueryList;
    const original = window.matchMedia;
    window.matchMedia = vi.fn(() => media);
    const view = render(panel("day-a", "old"));
    view.rerender(panel("day-b", "new"));
    if (during === "entering") act(() => vi.advanceTimersByTime(PLAN_TRANSITION_LEAVE_MS));
    (media as unknown as { matches: boolean }).matches = true;
    act(() => listener?.());
    expect(screen.getByRole("button", { name: "new" })).toBeTruthy();
    expect(phase()).toBe("idle");
    expect(vi.getTimerCount()).toBe(0);
    window.matchMedia = original;
  });

  it("does not lock a displayed identity when only its data is not ready", () => {
    render(panel("day-a", "visible", false));
    expect(screen.getByRole("button", { name: "visible" }).parentElement?.hasAttribute("inert")).toBe(false);
    expect(phase()).toBe("idle");
  });

  it("clears transition timers when unmounted and survives StrictMode", () => {
    const view = render(<StrictMode>{panel("day-a", "old")}</StrictMode>);
    view.rerender(<StrictMode>{panel("day-b", "new")}</StrictMode>);
    expect(vi.getTimerCount()).toBeGreaterThan(0);
    view.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });
});

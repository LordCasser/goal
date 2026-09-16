import { StrictMode } from "react";
import { createPortal } from "react-dom";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { RemovalList, REMOVAL_DURATION_MS } from "./RemovalList";

const originalAnimate = HTMLElement.prototype.animate;
let media: { matches: boolean; addEventListener: ReturnType<typeof vi.fn>; removeEventListener: ReturnType<typeof vi.fn> };
let preferenceChanged: () => void;
const animations: { cancel: ReturnType<typeof vi.fn> }[] = [];
beforeEach(() => {
  vi.useFakeTimers(); animations.length = 0;
  media = { matches: false, addEventListener: vi.fn((_event, listener) => { preferenceChanged = listener; }), removeEventListener: vi.fn() };
  vi.stubGlobal("matchMedia", () => media);
  HTMLElement.prototype.animate = vi.fn(() => {
    const animation = { cancel: vi.fn() }; animations.push(animation); return animation as unknown as Animation;
  });
});
afterEach(() => { vi.useRealTimers(); vi.restoreAllMocks(); vi.unstubAllGlobals(); HTMLElement.prototype.animate = originalAnimate; });

const list = (ids: string[]) => <RemovalList empty={<p>Empty</p>}>{ids.map(id => <button key={id}>{id}</button>)}</RemovalList>;
const exiting = () => document.querySelectorAll('[data-removal-state="exiting"]');

describe("RemovalList", () => {
  it("keeps surviving nodes and fresh props during ordinary edits and reordering", () => {
    const view = render(list(["a", "b"])); const a = screen.getByText("a");
    view.rerender(list(["b", "a", "c"]));
    expect(screen.getByText("a")).toBe(a);
    expect(screen.getAllByRole("button").map(x => x.textContent)).toEqual(["b", "a", "c"]);
    expect(animations).toHaveLength(0);
  });

  it("retains removed rows only for presentation and blocks stale click/blur handlers", () => {
    const action = vi.fn();
    const row = <input key="a" aria-label="Task" onBlur={action} onClick={action} />;
    const view = render(<RemovalList>{[row]}</RemovalList>);
    const input = screen.getByRole("textbox");
    view.rerender(<RemovalList>{[]}</RemovalList>);
    expect(exiting()).toHaveLength(1);
    expect(exiting()[0]?.hasAttribute("inert")).toBe(true);
    expect(exiting()[0]?.getAttribute("aria-hidden")).toBe("true");
    fireEvent.blur(input); fireEvent.click(input);
    expect(action).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(REMOVAL_DURATION_MS));
    expect(exiting()).toHaveLength(0);
  });

  it("removes cascaded rows together before showing the empty state", () => {
    const view = render(list(["parent", "child", "grandchild"]));
    view.rerender(list([]));
    expect(exiting()).toHaveLength(3);
    expect([...exiting()].map(row => row.textContent)).toEqual(["parent", "child", "grandchild"]);
    expect(screen.queryByText("Empty")).toBeNull();
    act(() => vi.advanceTimersByTime(REMOVAL_DURATION_MS - 1));
    expect(exiting()).toHaveLength(3);
    act(() => vi.advanceTimersByTime(1));
    expect(exiting()).toHaveLength(0);
    expect(screen.getByText("Empty")).toBeTruthy();
  });

  it("does not remove a restored key or newly added rows with an old exit timer", () => {
    const view = render(list(["a", "b"])); const a = screen.getByText("a");
    view.rerender(list(["b"]));
    act(() => vi.advanceTimersByTime(100));
    view.rerender(list(["a", "b", "new"]));
    act(() => vi.advanceTimersByTime(REMOVAL_DURATION_MS));
    expect(screen.getByText("a")).toBe(a);
    expect(screen.getAllByRole("button").map(x=>x.textContent)).toEqual(["a", "b", "new"]);
    expect(exiting()).toHaveLength(0);
  });

  it("keeps overlapping removals in place with independent completion", () => {
    const view = render(list(["a", "b", "c"]));
    view.rerender(list(["b", "c"]));
    act(() => vi.advanceTimersByTime(100));
    view.rerender(list(["c"]));
    expect([...document.querySelectorAll("button")].map(x=>x.textContent)).toEqual(["a", "b", "c"]);
    act(() => vi.advanceTimersByTime(REMOVAL_DURATION_MS - 100));
    expect(screen.queryByText("a")).toBeNull(); expect(screen.getByText("b")).toBeTruthy();
    act(() => vi.advanceTimersByTime(100));
    expect(screen.queryByText("b")).toBeNull(); expect(screen.getByText("c")).toBeTruthy();
  });

  it("keeps deleted rows above a newly created typing placeholder", () => {
    const view = render(list(["last task"]));
    view.rerender(list(["new blank row"]));
    expect([...document.querySelectorAll("button")].map(x=>x.textContent)).toEqual(["last task", "new blank row"]);
    act(() => vi.advanceTimersByTime(REMOVAL_DURATION_MS));
    expect(screen.getAllByRole("button").map(x=>x.textContent)).toEqual(["new blank row"]);
  });

  it("blocks events from portals belonging to outgoing rows", () => {
    const action = vi.fn();
    const row = <div key="a">Task{createPortal(<button onClick={action}>Menu action</button>, document.body)}</div>;
    const view = render(<RemovalList>{[row]}</RemovalList>);
    view.rerender(<RemovalList>{[]}</RemovalList>);
    fireEvent.click(screen.getByText("Menu action"));
    expect(action).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(REMOVAL_DURATION_MS));
    expect(screen.queryByText("Menu action")).toBeNull();
  });

  it("removes immediately with reduced motion and finishes an active exit when preferences change", () => {
    media.matches = true;
    const view = render(list(["a"])); view.rerender(list([]));
    expect(screen.queryByText("a")).toBeNull(); expect(animations).toHaveLength(0);
    media.matches = false;
    view.rerender(list(["b"])); view.rerender(list([]));
    expect(exiting()).toHaveLength(1);
    media.matches = true; act(() => preferenceChanged());
    expect(exiting()).toHaveLength(0); expect(vi.getTimerCount()).toBe(0);
  });

  it("cleans timers and animations on unmount under StrictMode", () => {
    const view = render(<StrictMode>{list(["a"])}</StrictMode>);
    view.rerender(<StrictMode>{list([])}</StrictMode>);
    view.unmount();
    expect(vi.getTimerCount()).toBe(0);
    expect(animations.every(animation=>animation.cancel.mock.calls.length > 0)).toBe(true);
  });
});

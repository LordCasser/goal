import { act, createEvent, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DateDrum, type DrumItem } from "./DateDrum";
import { Profiler } from "react";

const item = (index: number): DrumItem => ({
  label: `Item ${index}`,
  detail: `Detail ${index}`,
  title: `Item ${index}`,
});

function mount(options: Partial<React.ComponentProps<typeof DateDrum>> = {}) {
  const onSelect = options.onSelect ?? vi.fn();
  const onIntent = options.onIntent ?? vi.fn();
  const view = render(
    <DateDrum
      label="Date"
      value={0}
      current={0}
      item={item}
      {...options}
      onSelect={onSelect}
      onIntent={onIntent}
    />,
  );
  return { ...view, onSelect, onIntent };
}

function drum() {
  return screen.getByRole("navigation", { name: "Date" });
}

function wheel(node: HTMLElement, deltaY: number) {
  const event = createEvent.wheel(node, { deltaY });
  fireEvent(node, event);
  return event;
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("DateDrum", () => {
  it("paints smooth frames without rerendering or reformatting the whole window each frame", () => {
    const commits = vi.fn();
    const format = vi.fn(item);
    render(<Profiler id="drum" onRender={commits}><DateDrum label="Date" value={0} current={0} item={format} onSelect={() => {}} /></Profiler>);
    commits.mockClear();
    format.mockClear();
    const currentRow = drum().querySelector<HTMLButtonElement>('[data-drum-index="0"]')!;
    wheel(drum(), 48);
    const positions = new Set<string>();
    for (let frame = 0; frame < 60; frame++) {
      act(() => vi.advanceTimersByTime(16));
      positions.add(currentRow.style.transform);
    }
    expect(positions.size).toBeGreaterThan(10);
    expect(commits.mock.calls.length).toBeLessThanOrEqual(2);
    expect(format.mock.calls.length).toBe(1);
    expect(drum().querySelector('[data-drum-index="0"]')).toBe(currentRow);
  });

  it("owns the native wheel while hovered and commits only after quiet settle", () => {
    const { onSelect, onIntent } = mount();
    const event = wheel(drum(), 48);

    expect(event.defaultPrevented).toBe(true);
    expect(onIntent).toHaveBeenCalledTimes(1);
    expect(onSelect).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(499));
    expect(onSelect).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(1000));
    expect(onSelect).toHaveBeenCalledWith(1);
  });

  it("moves the accessible current marker independently of cached date labels", () => {
    const view = mount();
    expect(drum().querySelector('[aria-current="date"]')?.getAttribute("data-drum-index")).toBe("0");
    view.rerender(<DateDrum label="Date" value={0} current={1} item={item} onSelect={vi.fn()} />);
    expect(drum().querySelectorAll('[aria-current="date"]')).toHaveLength(1);
    expect(drum().querySelector('[aria-current="date"]')?.getAttribute("data-drum-index")).toBe("1");
  });

  it("restarts the quiet period for repeated wheel input", () => {
    const { onSelect } = mount();
    wheel(drum(), 48);
    act(() => vi.advanceTimersByTime(300));
    wheel(drum(), 48);

    act(() => vi.advanceTimersByTime(499));
    expect(onSelect).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(1000));
    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenCalledWith(2);
  });

  it("moves the visual center immediately under reduced motion but keeps wheel debounce", () => {
    const originalMatchMedia = window.matchMedia;
    window.matchMedia = vi.fn(() => ({
      matches: true,
      media: "(prefers-reduced-motion: reduce)",
      onchange: null,
      addListener: () => {},
      removeListener: () => {},
      addEventListener: () => {},
      removeEventListener: () => {},
      dispatchEvent: () => false,
    }) as MediaQueryList);

    const { onSelect } = mount();
    wheel(drum(), 48);
    act(() => vi.advanceTimersByTime(16));

    expect(drum().querySelector<HTMLButtonElement>('button[aria-selected="true"]')?.getAttribute("data-drum-index")).toBe("1");
    expect(onSelect).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(600));
    expect(onSelect).toHaveBeenCalledWith(1);

    window.matchMedia = originalMatchMedia;
  });

  it("keeps programmatic alignment silent", () => {
    const onSelect = vi.fn();
    const onIntent = vi.fn();
    const view = render(
      <DateDrum label="Date" value={0} current={0} item={item} onSelect={onSelect} onIntent={onIntent} />,
    );

    view.rerender(
      <DateDrum label="Date" value={1000} current={0} item={item} onSelect={onSelect} onIntent={onIntent} />,
    );
    act(() => vi.advanceTimersByTime(1200));

    expect(onSelect).not.toHaveBeenCalled();
    expect(onIntent).not.toHaveBeenCalled();
    expect(drum().querySelector<HTMLButtonElement>('button[aria-selected="true"]')?.getAttribute("data-drum-index")).toBe("1000");
  });

  it("rejects a stale gesture and smoothly returns to the external value", () => {
    const onSelect = vi.fn();
    const { rerender } = mount({ onSelect, canCommit: () => false });
    wheel(drum(), 48);

    act(() => vi.advanceTimersByTime(1200));

    expect(onSelect).not.toHaveBeenCalled();
    expect(drum().querySelector<HTMLButtonElement>('button[aria-selected="true"]')?.getAttribute("data-drum-index")).toBe("0");

    rerender(
      <DateDrum label="Date" value={0} current={0} item={item} onSelect={onSelect} canCommit={() => false} />,
    );
  });

  it("snaps positive and negative half-row wheel input in its travel direction", () => {
    const positive = mount();
    wheel(drum(), 24);
    act(() => vi.advanceTimersByTime(1200));
    expect(positive.onSelect).toHaveBeenCalledWith(1);

    positive.unmount();
    const negative = mount();
    wheel(drum(), -24);
    act(() => vi.advanceTimersByTime(1200));
    expect(negative.onSelect).toHaveBeenCalledWith(-1);
  });

  it("renders a bounded chronological window and one pinned current row at long range", () => {
    const { rerender } = mount({ value: 100, current: 0, item: item });
    const indexes = () => [...drum().querySelectorAll<HTMLButtonElement>("button.date-drum-row")].map((button) => Number(button.dataset.drumIndex));

    expect(new Set(indexes()).size).toBe(indexes().length);
    expect(indexes()).toContain(0);
    expect(drum().querySelectorAll('button[aria-current="date"]')).toHaveLength(1);
    expect(drum().querySelectorAll("button:not(.date-drum-row)")).toHaveLength(0);
    expect(drum().querySelector<HTMLButtonElement>('button[aria-current="date"]')?.dataset.pinnedEdge).toBe("top");

    rerender(
      <DateDrum label="Date" value={-100} current={0} item={item} onSelect={vi.fn()} />,
    );
    act(() => vi.advanceTimersByTime(1200));
    expect(new Set(indexes()).size).toBe(indexes().length);
    expect(indexes()).toContain(0);
    expect(drum().querySelector<HTMLButtonElement>('button[aria-current="date"]')?.dataset.pinnedEdge).toBe("bottom");
  });

  it("commits deliberate row clicks immediately and settles keyboard movement", () => {
    const { onSelect } = mount();
    fireEvent.click(drum().querySelector('button[data-drum-index="1"]')!);
    expect(onSelect).toHaveBeenCalledWith(1);

    const keyboardSelect = vi.fn();
    render(<DateDrum label="Keyboard" value={0} current={0} item={item} onSelect={keyboardSelect} />);
    const listbox = screen.getByRole("listbox", { name: "Keyboard" });
    fireEvent.keyDown(listbox, { key: "PageDown" });
    expect(keyboardSelect).not.toHaveBeenCalled();
    act(() => vi.advanceTimersByTime(1200));
    expect(keyboardSelect).toHaveBeenCalledWith(5);
  });
});

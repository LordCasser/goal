import { act, fireEvent, render, screen } from "@testing-library/react";
import type { ComponentProps } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Cycle } from "../../lib/ipc";
import { applyLocale } from "../../lib/i18n";
import { DayNavigation, weekContainingDate } from "./DayNavigation";

function cycle(id: string, type: Cycle["type"], starts_on: string | null, ends_on: string | null, position = 0): Cycle {
  return {
    id,
    title: id,
    type,
    parent_id: null,
    task_id: null,
    position,
    archived: false,
    started: false,
    finished: false,
    started_at: null,
    finished_at: null,
    duration: null,
    focused_time: 0,
    starts_on,
    ends_on,
    calendar_key: starts_on ? `${type}:${starts_on}` : null,
    repeat_id: null,
    created_at: 0,
  };
}

const week = cycle("week", "week", "2026-09-13", "2026-09-20");
const nextWeek = cycle("next-week", "week", "2026-09-20", "2026-09-27");

function mount({
  selectedDate = "2026-09-15",
  today = "2026-09-15",
  active = true,
}: Partial<ComponentProps<typeof DayNavigation>> = {}) {
  const onSelect = vi.fn();
  const onIntent = vi.fn();
  const view = render(
    <DayNavigation selectedDate={selectedDate} today={today} active={active} onSelect={onSelect} onIntent={onIntent} />,
  );
  return { ...view, onSelect, onIntent };
}

function navigation() {
  return screen.getByRole("navigation", { name: "Days" });
}

beforeEach(() => applyLocale("en"));
afterEach(() => {
  vi.useRealTimers();
  applyLocale("en");
});

describe("DayNavigation", () => {
  it("renders a bounded chronological date drum with one inline today row", () => {
    mount();
    const nav = navigation();
    const rows = [...nav.querySelectorAll<HTMLButtonElement>("button.date-drum-row")];
    const todayRow = nav.querySelector<HTMLButtonElement>('button[aria-current="date"]');

    expect(rows).toHaveLength(15);
    expect(new Set(rows.map((row) => Number(row.dataset.drumIndex))).size).toBe(15);
    expect(todayRow).toBeTruthy();
    expect(todayRow?.textContent).toContain("Today");
    expect(todayRow?.dataset.pinnedEdge).toBeUndefined();
    expect(nav.querySelectorAll("button:not(.date-drum-row)")).toHaveLength(0);
  });

  it("selects an empty date without creating a day", () => {
    const { onSelect } = mount();
    const emptyDate = navigation().querySelector<HTMLButtonElement>('button[title="September 17, 2026"]')!;

    fireEvent.click(emptyDate);

    expect(onSelect).toHaveBeenCalledWith("2026-09-17");
    expect(onSelect).toHaveBeenCalledTimes(1);
  });

  it("keeps day selection and owner-week resolution independent", () => {
    const overlapping = cycle("overlap", "week", "2026-09-14", "2026-09-21");
    expect(weekContainingDate([week, overlapping], "2026-09-15", week.id)).toBe(week);
    expect(weekContainingDate([week, overlapping], "2026-09-15")).toBe(overlapping);
    expect(weekContainingDate([week, nextWeek], "2026-10-01")).toBeUndefined();
  });

  it("keeps the same today row and pins it to the nearest edge", () => {
    vi.useFakeTimers();
    const { rerender, onSelect, onIntent } = mount({ selectedDate: "2026-09-01" });
    const todayRow = navigation().querySelector<HTMLButtonElement>('button[aria-current="date"]')!;

    expect(todayRow.dataset.pinnedEdge).toBe("bottom");
    expect(navigation().querySelectorAll('button[aria-current="date"]')).toHaveLength(1);

    rerender(
      <DayNavigation
        selectedDate="2026-10-01"
        today="2026-09-15"
        active
        onSelect={onSelect}
        onIntent={onIntent}
      />,
    );
    act(() => vi.advanceTimersByTime(1200));

    expect(navigation().querySelector<HTMLButtonElement>('button[aria-current="date"]')?.dataset.pinnedEdge).toBe("top");
  });

  it("keeps programmatic alignment silent", () => {
    vi.useFakeTimers();
    const onSelect = vi.fn();
    const onIntent = vi.fn();
    const view = render(
      <DayNavigation selectedDate="2026-09-15" today="2026-09-15" active onSelect={onSelect} onIntent={onIntent} />,
    );

    view.rerender(
      <DayNavigation selectedDate="2026-09-18" today="2026-09-15" active onSelect={onSelect} onIntent={onIntent} />,
    );
    act(() => vi.advanceTimersByTime(1200));

    expect(onSelect).not.toHaveBeenCalled();
    expect(onIntent).not.toHaveBeenCalled();
    expect(navigation().querySelector<HTMLButtonElement>('button[aria-selected="true"]')?.title).toBe("September 18, 2026");
  });

  it("does not add a footer shortcut or create while browsing", () => {
    const { onSelect } = mount({ selectedDate: "2026-09-22" });
    const nav = navigation();

    expect(nav.querySelectorAll("button:not(.date-drum-row)")).toHaveLength(0);
    fireEvent.click(nav.querySelector<HTMLButtonElement>('button[aria-current="date"]')!);

    expect(onSelect).toHaveBeenCalledWith("2026-09-15");
  });
});

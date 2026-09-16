import { act, createEvent, fireEvent, render, screen } from "@testing-library/react";
import type { ComponentProps } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Cycle } from "../../lib/ipc";
import { applyLocale } from "../../lib/i18n";
import { WeekNavigation, weekTimeline } from "./WeekNavigation";

function cycle(id: string, starts_on: string, ends_on: string, position = 0): Cycle {
  return {
    id,
    title: id,
    type: "week",
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
    calendar_key: `week:${starts_on}`,
    repeat_id: null,
    created_at: 0,
  };
}

const past = cycle("past", "2026-09-07", "2026-09-14");
const current = cycle("current", "2026-09-14", "2026-09-21");
const future = cycle("future", "2026-09-21", "2026-09-28");

function mount({
  cycles = [past, current, future],
  selectedDate = current.starts_on!,
  today = "2026-09-15",
  weekStartDay = 1,
  active = true,
}: Partial<ComponentProps<typeof WeekNavigation>> = {}) {
  const onSelect = vi.fn();
  const onIntent = vi.fn();
  const view = render(
    <WeekNavigation
      cycles={cycles}
      selectedDate={selectedDate}
      today={today}
      weekStartDay={weekStartDay}
      active={active}
      onSelect={onSelect}
      onIntent={onIntent}
    />,
  );
  return { ...view, onSelect, onIntent };
}

function navigation() {
  return screen.getByRole("navigation", { name: "Weeks" });
}

beforeEach(() => applyLocale("en"));
afterEach(() => {
  vi.useRealTimers();
  applyLocale("en");
});

describe("WeekNavigation", () => {
  it("renders a bounded chronological drum with one inline current row", () => {
    mount();
    const nav = navigation();
    const rows = [...nav.querySelectorAll<HTMLButtonElement>("button.date-drum-row")];
    const currentRow = nav.querySelector<HTMLButtonElement>('button[aria-current="date"]');

    expect(nav.hasAttribute("data-date-drum")).toBe(true);
    expect(nav.hasAttribute("data-wheel-native")).toBe(true);
    expect(rows).toHaveLength(15);
    expect(new Set(rows.map((row) => Number(row.dataset.drumIndex))).size).toBe(15);
    expect(currentRow).toBeTruthy();
    expect(currentRow?.textContent).toContain("This week");
    expect(currentRow?.dataset.pinnedEdge).toBeUndefined();
    expect(nav.querySelectorAll("button:not(.date-drum-row)")).toHaveLength(0);
  });

  it("maps row clicks to chronological week starts, including empty weeks", () => {
    const { onSelect } = mount({ cycles: [current], selectedDate: current.starts_on! });
    const emptyWeek = navigation().querySelector<HTMLButtonElement>('button[title="September 21, 2026"]')!;

    fireEvent.click(emptyWeek);

    expect(onSelect).toHaveBeenCalledWith("2026-09-21");
  });

  it("does not create a week while browsing and keeps current identity inline", () => {
    const { onSelect } = mount({ cycles: [current], selectedDate: "2026-09-21" });
    const nav = navigation();
    const currentRow = nav.querySelector<HTMLButtonElement>('button[aria-current="date"]')!;

    expect(currentRow.textContent).toContain("This week");
    fireEvent.click(currentRow);

    expect(onSelect).toHaveBeenCalledWith(current.starts_on);
    expect(nav.querySelectorAll('button[aria-current="date"]')).toHaveLength(1);
    expect(nav.querySelectorAll("button:not(.date-drum-row)")).toHaveLength(0);
  });

  it("uses the configured week-start anchor for an empty week", () => {
    const { onSelect } = mount({ cycles: [], selectedDate: "2026-09-07", today: "2026-09-09", weekStartDay: 1 });
    const center = navigation().querySelector<HTMLButtonElement>('button[aria-selected="true"]')!;

    expect(center.title).toBe("September 7, 2026");
    fireEvent.click(center);
    expect(onSelect).toHaveBeenCalledWith("2026-09-07");
  });

  it("preserves two saved starts in one Sunday timeline slot", () => {
    const duplicateSlot = [
      cycle("sunday", "2026-09-13", "2026-09-20"),
      cycle("monday", "2026-09-14", "2026-09-21"),
    ];
    const timeline = weekTimeline(duplicateSlot, 0, ["2026-09-06", "2026-09-20"]);
    const sundayIndex = timeline.index("2026-09-13");
    const mondayIndex = timeline.index("2026-09-14");

    expect(mondayIndex).toBe(sundayIndex + 1);
    expect(new Set([sundayIndex, mondayIndex]).size).toBe(2);
    expect(timeline.date(sundayIndex)).toBe("2026-09-13");
    expect(timeline.date(mondayIndex)).toBe("2026-09-14");
    expect(timeline.date(sundayIndex - 1)).toBe("2026-09-06");
    expect(timeline.date(mondayIndex + 1)).toBe("2026-09-20");
  });

  it("keeps the same current row and pins it to the nearest edge when far away", () => {
    vi.useFakeTimers();
    const { rerender, onSelect, onIntent } = mount({ selectedDate: "2026-01-05" });
    const nav = navigation();
    const currentRow = nav.querySelector<HTMLButtonElement>('button[aria-current="date"]')!;

    expect(currentRow.dataset.pinnedEdge).toBe("bottom");
    expect(nav.querySelectorAll('button[aria-current="date"]')).toHaveLength(1);

    rerender(
      <WeekNavigation
        cycles={[past, current, future]}
        selectedDate="2027-01-04"
        today="2026-09-15"
        weekStartDay={1}
        active
        onSelect={onSelect}
        onIntent={onIntent}
      />,
    );
    act(() => vi.advanceTimersByTime(1200));

    expect(navigation().querySelector<HTMLButtonElement>('button[aria-current="date"]')?.dataset.pinnedEdge).toBe("top");
  });

  it("owns wheel input without changing the workspace horizontal route", () => {
    vi.useFakeTimers();
    const { onSelect, onIntent } = mount();
    const nav = navigation();
    const event = createEvent.wheel(nav, { deltaY: 48, deltaX: 8 });
    fireEvent(nav, event);

    expect(event.defaultPrevented).toBe(true);
    expect(onIntent).toHaveBeenCalledTimes(1);
    expect(onSelect).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(1200));
    expect(onSelect).toHaveBeenCalledWith("2026-09-21");
  });
});

import { beforeEach, describe, expect, it, vi } from "vitest";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen } from "@testing-library/react";
import type { Cycle } from "../../lib/ipc";
import { applyLocale } from "../../lib/i18n";

const { listSessionsMock } = vi.hoisted(() => ({ listSessionsMock: vi.fn() }));

vi.mock("../../lib/ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/ipc")>()),
  listSessions: listSessionsMock,
}));
vi.mock("./CycleOptionsMenu", () => ({ CycleOptionsMenu: () => null }));
vi.mock("./TaskList", () => ({ TaskList: () => <div data-testid="task-list" /> }));

import { CycleColumn } from "./CycleColumn";

function makeCycle(progress_check?: Cycle["progress_check"]): Cycle {
  return {
    id: "m1",
    title: "Long-term goals",
    type: "month",
    parent_id: null,
    task_id: null,
    position: 0,
    archived: false,
    started: false,
    finished: false,
    started_at: null,
    finished_at: null,
    duration: 84 * 24 * 60 * 60 * 1000,
    focused_time: 0,
    starts_on: "2026-09-15",
    ends_on: "2026-12-08",
    calendar_key: "long-term:2026-09-15:2026-12-08",
    repeat_id: null,
    progress_check,
    created_at: 0,
  };
}

function renderColumn(cycle: Cycle) {
  return render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <CycleColumn cycle={cycle} />
    </QueryClientProvider>,
  );
}

describe("CycleColumn progress check entry", () => {
  beforeEach(() => {
    applyLocale("en");
    listSessionsMock.mockReset();
  });

  it("opens a compact popover with the persisted cadence and preview", () => {
    renderColumn(makeCycle({ kind: "repeat", every_days: 14 }));

    const trigger = screen.getByRole("button", { name: "Progress check: Every 2 weeks" });
    fireEvent.click(trigger);

    const popover = screen.getByRole("dialog", { name: "Progress check" });
    expect(popover.textContent).toContain("Every 2 weeks");
    expect(popover.textContent).toContain("Sep 29");
    expect(popover.textContent).toContain("5 progress checks");
  });

  it("closes on Escape and restores focus to the entry button", () => {
    renderColumn(makeCycle({ kind: "repeat", every_days: 14 }));
    const trigger = screen.getByRole("button", { name: "Progress check: Every 2 weeks" });
    fireEvent.click(trigger);
    expect(screen.getByRole("dialog", { name: "Progress check" })).toBeTruthy();

    fireEvent.keyDown(document, { key: "Escape" });

    expect(screen.queryByRole("dialog", { name: "Progress check" })).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it("uses the historical midpoint for cycles without a saved rule", () => {
    renderColumn(makeCycle(null));
    fireEvent.click(screen.getByRole("button", { name: "Progress check: Once on Oct 27" }));

    expect(screen.getByRole("dialog", { name: "Progress check" }).textContent).toContain("Oct 27, 2026");
  });
});

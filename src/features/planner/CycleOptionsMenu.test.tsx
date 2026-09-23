import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { Cycle, CycleDeletionPreview } from "../../lib/ipc";

const mocks = vi.hoisted(() => ({
  getCycleDeletionPreview: vi.fn(),
  deletePlanningCycle: vi.fn(),
}));

vi.mock("../../lib/ipc", async (original) => ({
  ...(await original<typeof import("../../lib/ipc")>()),
  ...mocks,
}));

import { CycleOptionsMenu } from "./CycleOptionsMenu";

function cycle(type: Cycle["type"], overrides: Partial<Cycle> = {}): Cycle {
  const session = type === "session";
  return {
    id: `${type}-1`,
    title: session ? "Deep work" : "Today",
    type,
    parent_id: null,
    position: 0,
    archived: false,
    started: session,
    finished: session,
    started_at: session ? 1_000 : null,
    finished_at: session ? 2_000 : null,
    duration: session ? 60_000 : null,
    focused_time: session ? 60_000 : 0,
    starts_on: session ? null : "2026-09-15",
    ends_on: session ? null : "2026-09-16",
    calendar_key: session ? null : `${type}:2026-09-15`,
    repeat_id: null,
    created_at: 1_000,
    ...overrides,
    task_id: overrides.task_id ?? null,
  };
}

function preview(cycleId: string, guard_code: string | null = null): CycleDeletionPreview {
  return {
    cycle_id: cycleId,
    guard_code,
    guard_message: guard_code ? "This planning container is protected." : null,
    descendant_cycles: 0,
    tasks: 0,
    started_sessions: 0,
    total_focus_blocks: 0,
    confirmation_token: "impact-1",
  };
}

function mount(target: Cycle): QueryClient {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <CycleOptionsMenu cycle={target} />
    </QueryClientProvider>,
  );
  return client;
}

async function openDelete(target: Cycle): Promise<HTMLElement> {
  fireEvent.click(screen.getByRole("button", { name: `${target.title} options` }));
  fireEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
  return screen.findByRole("dialog");
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.getCycleDeletionPreview.mockResolvedValue(preview("session-1"));
  mocks.deletePlanningCycle.mockResolvedValue(undefined);
});

describe("CycleOptionsMenu deletion", () => {
  it("confirms deletion of a completed focus block, closes, and refreshes queries", async () => {
    const target = cycle("session", { id: "focus-1", started: true, finished: true });
    mocks.getCycleDeletionPreview.mockResolvedValue(preview(target.id));
    const client = mount(target);
    const invalidateQueries = vi.spyOn(client, "invalidateQueries");

    await openDelete(target);
    const confirm = await screen.findByRole("button", { name: "Move focus block to Trash" });
    expect((confirm as HTMLButtonElement).disabled).toBe(false);

    fireEvent.click(confirm);

    await waitFor(() => expect(mocks.deletePlanningCycle).toHaveBeenCalledWith(target.id, "impact-1"));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["planner-state"] });
  });

  it("keeps confirmation disabled until the deletion preview returns", async () => {
    let resolvePreview!: (value: CycleDeletionPreview) => void;
    const pending = new Promise<CycleDeletionPreview>((resolve) => {
      resolvePreview = resolve;
    });
    const target = cycle("session", { id: "focus-pending" });
    mocks.getCycleDeletionPreview.mockReturnValue(pending);
    mount(target);

    await openDelete(target);
    const confirm = screen.getByRole("button", { name: "Move focus block to Trash" });
    expect((confirm as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(confirm);
    expect(mocks.deletePlanningCycle).not.toHaveBeenCalled();

    resolvePreview(preview(target.id));
    await waitFor(() => expect((confirm as HTMLButtonElement).disabled).toBe(false));
  });

  it("keeps protected day containers undeletable according to the backend guard", async () => {
    const target = cycle("day", { id: "day-protected", title: "Today" });
    mocks.getCycleDeletionPreview.mockResolvedValue(preview(target.id, "protected_container"));
    mount(target);

    await openDelete(target);
    expect((await screen.findByRole("alert")).textContent).toContain("This planning container cannot be deleted.");
    expect(screen.getByRole("alert").textContent).not.toContain("This planning container is protected.");
    const confirm = screen.getByRole("button", { name: "Move day plan to Trash" });
    expect((confirm as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(confirm);
    expect(mocks.deletePlanningCycle).not.toHaveBeenCalled();
  });

  it("cancels a deletable confirmation without writing", async () => {
    const target = cycle("session", { id: "focus-cancel" });
    mocks.getCycleDeletionPreview.mockResolvedValue(preview(target.id));
    mount(target);

    await openDelete(target);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(mocks.deletePlanningCycle).not.toHaveBeenCalled();
  });

  it("lists focus blocks separately and reloads changed impact before another confirmation", async () => {
    const target = cycle("week", { id: "week-preview" });
    mocks.getCycleDeletionPreview.mockResolvedValueOnce({ ...preview(target.id), descendant_cycles: 4, total_focus_blocks: 3, tasks: 7 })
      .mockResolvedValue({ ...preview(target.id), descendant_cycles: 5, total_focus_blocks: 4, tasks: 8, confirmation_token: "impact-2" });
    mocks.deletePlanningCycle.mockRejectedValueOnce({ code: "deletion_impact_changed" });
    mount(target);
    await openDelete(target);
    await screen.findByText("3 focus blocks");
    expect(screen.getByText("1 nested plan")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Move week plan to Trash" }));
    await screen.findByText("4 focus blocks");
    expect(mocks.deletePlanningCycle).toHaveBeenCalledTimes(1);
    expect((await screen.findByRole("alert")).textContent).toContain("Review the updated impact");
    fireEvent.click(screen.getByRole("button", { name: "Move week plan to Trash" }));
    await waitFor(() => expect(mocks.deletePlanningCycle).toHaveBeenLastCalledWith(target.id, "impact-2"));
  });
});

import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { TrashEntry } from "../../lib/ipc";
import { applyLocale } from "../../lib/i18n";

const mocks = vi.hoisted(() => ({ listTrash: vi.fn(), restoreTrashEntry: vi.fn(), deleteTrashEntries: vi.fn() }));
vi.mock("../../lib/ipc", async (original) => ({
  ...(await original<typeof import("../../lib/ipc")>()),
  listTrash: mocks.listTrash,
  restoreTrashEntry: mocks.restoreTrashEntry,
  deleteTrashEntries: mocks.deleteTrashEntries,
}));
import { TrashPanel } from "./TrashPanel";

const entries: TrashEntry[] = [
  { id: "goal-1", kind: "task", target_id: "t1", title: "A very long goal title that should be shortened in this narrow panel", origin: "Long-term", deleted_at: 1_758_600_000_000, task_count: 2, cycle_count: 1 },
  { id: "plan-1", kind: "cycle", target_id: "c1", title: "Release week", origin: "September", deleted_at: 1_758_600_001_000, task_count: 3, cycle_count: 2 },
];

function mount(onClose = vi.fn()) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const view = render(<QueryClientProvider client={queryClient}><TrashPanel onClose={onClose} /></QueryClientProvider>);
  return { onClose, queryClient, ...view };
}

beforeEach(() => {
  vi.clearAllMocks();
  applyLocale("en");
  mocks.listTrash.mockResolvedValue(entries);
  mocks.restoreTrashEntry.mockResolvedValue(undefined);
  mocks.deleteTrashEntries.mockResolvedValue(1);
});

describe("TrashPanel", () => {
  it("lists origin and deletion time and truncates long titles", async () => {
    mount();
    const title = await screen.findByText(entries[0]!.title);
    expect(title.className).toContain("truncate");
    expect(title.getAttribute("title")).toBe(entries[0]!.title);
    expect(screen.getByText("From Long-term")).toBeTruthy();
    expect(screen.getByText("Plan · September")).toBeTruthy();
    expect(screen.getByText(new Date(entries[0]!.deleted_at).toLocaleString())).toBeTruthy();
  });

  it("shows the empty state and closes from the keyboard", async () => {
    mocks.listTrash.mockResolvedValue([]);
    const onClose = vi.fn();
    mount(onClose);
    expect(await screen.findByText("Trash is empty")).toBeTruthy();
    expect(screen.queryByText(/Deleted goals and plans stay here/)).toBeNull();
    const panel = screen.getByLabelText("Trash");
    fireEvent.keyDown(panel, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("reports list and restore failures accessibly", async () => {
    mocks.listTrash.mockRejectedValueOnce(new Error("Could not load trash"));
    const first = mount();
    expect((await screen.findByRole("alert")).textContent).toContain("Could not load trash");
    first.unmount();

    mocks.listTrash.mockResolvedValue(entries);
    const second = render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><TrashPanel onClose={vi.fn()} /></QueryClientProvider>);
    mocks.restoreTrashEntry.mockRejectedValueOnce(new Error("Restore failed"));
    fireEvent.click((await screen.findAllByRole("button", { name: "Restore" }))[0]!);
    expect((await screen.findByRole("alert")).textContent).toContain("Restore failed");
    expect(mocks.restoreTrashEntry).toHaveBeenCalledWith("goal-1");
    second.unmount();
  });

  it("restores one entry and refreshes dependent planning queries", async () => {
    const { queryClient } = mount();
    const invalidate = vi.spyOn(queryClient, "invalidateQueries");
    fireEvent.click(await screen.findByRole("checkbox", { name: "Select A very long goal title that should be shortened in this narrow panel" }));
    fireEvent.click(await screen.findAllByRole("button", { name: "Restore" }).then((buttons) => buttons[0]!));
    await waitFor(() => expect(mocks.restoreTrashEntry).toHaveBeenCalledWith("goal-1"));
    await waitFor(() => expect(invalidate).toHaveBeenCalled());
    expect((screen.getByRole("checkbox", { name: "Select A very long goal title that should be shortened in this narrow panel" }) as HTMLInputElement).checked).toBe(false);
  });

  it("selects all or individual items and keeps selection when confirmation is cancelled", async () => {
    mount();
    fireEvent.click(await screen.findByRole("checkbox", { name: "Select all" }));
    expect((screen.getByRole("checkbox", { name: "Select all" }) as HTMLInputElement).checked).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Delete selected (2)" }));
    expect(await screen.findByRole("dialog")).toBeTruthy();
    expect(mocks.deleteTrashEntries).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect((screen.getByRole("checkbox", { name: "Select all" }) as HTMLInputElement).checked).toBe(true);
    expect(screen.getByRole("button", { name: "Delete selected (2)" })).toBeTruthy();
  });

  it("permanently deletes the selected IDs only after confirmation", async () => {
    mocks.listTrash.mockResolvedValueOnce(entries).mockResolvedValue([]);
    mount();
    fireEvent.click(await screen.findByRole("checkbox", { name: "Select A very long goal title that should be shortened in this narrow panel" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete selected (1)" }));
    expect(mocks.deleteTrashEntries).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Delete permanently" }));
    await waitFor(() => expect(mocks.deleteTrashEntries).toHaveBeenCalledWith(["goal-1"]));
    expect(await screen.findByText("Trash is empty")).toBeTruthy();
  });

  it("keeps the selection and confirmation open when permanent deletion fails", async () => {
    mocks.deleteTrashEntries.mockRejectedValueOnce(new Error("Delete failed"));
    mount();
    fireEvent.click(await screen.findByRole("checkbox", { name: "Select Release week" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete selected (1)" }));
    fireEvent.click(screen.getByRole("button", { name: "Delete permanently" }));
    expect((await screen.findByRole("alert")).textContent).toContain("Delete failed");
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect((screen.getByRole("checkbox", { name: "Select Release week" }) as HTMLInputElement).checked).toBe(true);
    expect(mocks.deleteTrashEntries).toHaveBeenCalledWith(["plan-1"]);
  });
});

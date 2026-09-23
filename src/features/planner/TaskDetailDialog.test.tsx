import { beforeEach, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { TaskNode } from "../../lib/ipc";
import { applyLocale } from "../../lib/i18n";

const mocks = vi.hoisted(() => ({ patchTask: vi.fn(), getTaskContinuity: vi.fn() }));
vi.mock("../../lib/ipc", async (original) => ({
  ...(await original<typeof import("../../lib/ipc")>()),
  patchTask: mocks.patchTask,
  getTaskContinuity: mocks.getTaskContinuity,
}));
import { TaskDetailDialog } from "./TaskDetailDialog";

const task = {
  id: "d1", cycle_id: "day", title: "Write report", note: "Monday outline",
  parent_id: null, proposal: null,
} as TaskNode;

beforeEach(() => {
  vi.clearAllMocks();
  applyLocale("en");
  mocks.getTaskContinuity.mockResolvedValue({
    title: "Write report", parent_goal_id: null, selected_episode_index: 0,
    episodes: [{
      started_on: "2026-09-21", last_recorded_on: "2026-09-23",
      completed: true, elapsed_days: 3, recorded_days: 2,
      records: [
        { task_id: "d1", date: "2026-09-21", note: "Monday outline", completed: false },
        { task_id: "d3", date: "2026-09-23", note: "Wednesday draft", completed: true },
      ],
    }],
  });
});

function mount(locked = false) {
  const onClose = vi.fn();
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
    <TaskDetailDialog task={task} locked={locked} onClose={onClose} cycleLabel="Wednesday" />
  </QueryClientProvider>);
  return onClose;
}

it("shows per-date notes and saves only the selected task note", async () => {
  mocks.patchTask.mockResolvedValue({ ...task, note: "Revised" });
  const onClose = mount();
  expect(await screen.findByText("Wednesday draft")).toBeTruthy();
  expect(screen.getByText(/3 days/)).toBeTruthy();
  fireEvent.change(screen.getByRole("textbox"), { target: { value: "Revised" } });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(mocks.patchTask).toHaveBeenCalledWith("d1", { note: "Revised" }));
  expect(onClose).toHaveBeenCalledTimes(1);
});

it("shows a locked note without offering save", () => {
  mount(true);
  expect((screen.getByRole("textbox") as HTMLTextAreaElement).readOnly).toBe(true);
  expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
});

it("keeps the note draft visible when saving fails", async () => {
  mocks.patchTask.mockRejectedValue({ code: "db_error", message: "Save failed" });
  const onClose = mount();
  fireEvent.change(screen.getByRole("textbox"), { target: { value: "Keep this draft" } });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(await screen.findByRole("alert")).toBeTruthy();
  expect((screen.getByRole("textbox") as HTMLTextAreaElement).value).toBe("Keep this draft");
  expect(onClose).not.toHaveBeenCalled();
});

import { beforeEach, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { TaskNode } from "../../lib/ipc";
import { applyLocale } from "../../lib/i18n";

const mocks = vi.hoisted(() => ({ patchTask: vi.fn(), getTaskContinuity: vi.fn(), getDirectLinkedChildren: vi.fn() }));
const reminderMocks = vi.hoisted(() => ({ setReminder: vi.fn() }));
vi.mock("../reminders/api", async (original) => ({
  ...(await original<typeof import("../reminders/api")>()),
  setReminder: reminderMocks.setReminder,
}));
vi.mock("../../lib/ipc", async (original) => ({
  ...(await original<typeof import("../../lib/ipc")>()),
  patchTask: mocks.patchTask,
  getTaskContinuity: mocks.getTaskContinuity,
  getDirectLinkedChildren: mocks.getDirectLinkedChildren,
}));
import { TaskDetailDialog } from "./TaskDetailDialog";

const task = {
  id: "d3", cycle_id: "day", title: "Write report", note: "Wednesday draft",
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
  mocks.getDirectLinkedChildren.mockResolvedValue([]);
  reminderMocks.setReminder.mockResolvedValue({ id: "r1" });
});

function mount(locked = false) {
  const onClose = vi.fn();
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
    <TaskDetailDialog task={task} locked={locked} onClose={onClose} cycleLabel="Wednesday" />
  </QueryClientProvider>);
  return onClose;
}

it("shows prior notes without duplicating the current note and saves only the selected task note", async () => {
  mocks.patchTask.mockResolvedValue({ ...task, note: "Revised" });
  const onClose = mount();
  expect(await screen.findByText("Monday outline")).toBeTruthy();
  const history = screen.getByRole("region", { name: "Daily history" });
  expect(history.textContent).not.toContain("Wednesday draft");
  expect(screen.getByRole("textbox", { name: "Note" })).toBeTruthy();
  expect(screen.getByText(/3 days/)).toBeTruthy();
  fireEvent.change(screen.getByRole("textbox", { name: "Note" }), { target: { value: "Revised" } });
  fireEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(mocks.patchTask).toHaveBeenCalledWith("d3", { note: "Revised" }));
  expect(onClose).toHaveBeenCalledTimes(1);
});

it("hides daily history when the selected task is the only record", async () => {
  mocks.getTaskContinuity.mockResolvedValueOnce({
    title: "Write report", parent_goal_id: null, selected_episode_index: 0,
    episodes: [{
      started_on: "2026-09-23", last_recorded_on: "2026-09-23",
      completed: false, elapsed_days: 1, recorded_days: 1,
      records: [{ task_id: "d3", date: "2026-09-23", note: "Wednesday draft", completed: false }],
    }],
  });
  mount();
  expect(await screen.findByRole("textbox", { name: "Note" })).toBeTruthy();
  expect(screen.queryByRole("region", { name: "Daily history" })).toBeNull();
});

it("compresses consecutive blank-note dates while keeping noted dates as separate rows", async () => {
  mocks.getTaskContinuity.mockResolvedValueOnce({
    title: "Write report", parent_goal_id: null, selected_episode_index: 0,
    episodes: [{
      started_on: "2026-09-23", last_recorded_on: "2026-10-03",
      completed: false, elapsed_days: 11, recorded_days: 9,
      records: [
        { task_id: "d1", date: "2026-09-24", note: "", completed: false },
        { task_id: "d2", date: "2026-09-25", note: "", completed: false },
        { task_id: "d4", date: "2026-09-26", note: "", completed: false },
        { task_id: "d5", date: "2026-09-27", note: "A useful update", completed: false },
        { task_id: "d6", date: "2026-09-28", note: "", completed: false },
        { task_id: "d7", date: "2026-09-29", note: "", completed: false },
        { task_id: "d8", date: "2026-10-02", note: "", completed: false },
        { task_id: "d3", date: "2026-10-03", note: "Wednesday draft", completed: true },
      ],
    }],
  });
  mount();
  const history = await screen.findByRole("region", { name: "Daily history" });
  expect(screen.getByText("2026-09-24 ~ 2026-09-26")).toBeTruthy();
  expect(screen.getByText("2026-09-28 / 2026-09-29 / 2026-10-02")).toBeTruthy();
  expect(screen.getByText("A useful update")).toBeTruthy();
  expect(history.textContent).not.toContain("2026-10-03");
  expect(history.querySelectorAll("li")).toHaveLength(3);
  expect(history.textContent?.indexOf("2026-09-24")).toBeLessThan(history.textContent!.indexOf("A useful update"));
  expect(history.textContent?.indexOf("A useful update")).toBeLessThan(history.textContent!.indexOf("2026-09-28"));
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

it("opens a reminder picker for the selected task and saves its target", async () => {
  mount();
  fireEvent.click(screen.getByRole("button", { name: "Set reminder" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Reminder time · Date" }), {
    target: { value: "2026-09-24" },
  });
  fireEvent.change(screen.getByRole("textbox", { name: "Reminder time · Time" }), {
    target: { value: "09:00" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Save reminder" }));
  await waitFor(() => expect(reminderMocks.setReminder).toHaveBeenCalledWith({
    target_kind: "task", target_id: "d3", fire_at: new Date("2026-09-24T09:00").getTime(), quiet_ok: false,
  }));
});

it("lists direct weekly and daily links at the bottom of long-term details", async () => {
  mocks.getDirectLinkedChildren.mockResolvedValue([
    { id: "w1", title: "Prepare launch", cycle_type: "week", starts_on: "2026-09-21", completed: false },
    { id: "d2", title: "Review copy", cycle_type: "day", starts_on: "2026-09-23", completed: true },
  ]);
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
    <TaskDetailDialog task={{ ...task, cycle_id: "month" }} cycleType="month" locked={false} onClose={vi.fn()} />
  </QueryClientProvider>);
  await screen.findByText("Prepare launch");
  const section = screen.getByRole("region", { name: "Directly linked items" });
  expect(section.textContent).toContain("Prepare launch");
  expect(section.textContent).toContain("Review copy");
  expect(section.textContent).toContain("Week");
  expect(section.textContent).toContain("Day");
  expect(mocks.getDirectLinkedChildren).toHaveBeenCalledWith("d3");
  expect(mocks.getTaskContinuity).not.toHaveBeenCalled();
});

it("shows an empty direct-link section for weekly details", async () => {
  render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
    <TaskDetailDialog task={{ ...task, cycle_id: "week" }} cycleType="week" locked={false} onClose={vi.fn()} />
  </QueryClientProvider>);
  expect(await screen.findByText("No directly linked items")).toBeTruthy();
});

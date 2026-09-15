import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { Cycle, EditorWorkspace, TaskNode } from "../../lib/ipc";

const backend = vi.hoisted(() => ({
  getPlannerState: vi.fn(), getEditorWorkspace: vi.fn(), getEditorWorkspacesByCycleIds: vi.fn(),
  getPreviewSummary: vi.fn(), patchTask: vi.fn(), addTask: vi.fn(),
  getCalendarRange: vi.fn(),
}));
vi.mock("../../lib/ipc", async (original) => ({ ...(await original<typeof import("../../lib/ipc")>()), ...backend }));
vi.mock("./api", () => ({
  getCalendarRange: backend.getCalendarRange, getTimeBudget: async () => ({ capacity_minutes: null }),
  getScheduleOverlaps: async () => [], moveDayCycle: vi.fn(), setSessionSchedule: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: async () => () => {} }));
vi.mock("../planner/dates", async (original) => ({ ...(await original<typeof import("../planner/dates")>()), todayISO: () => "2026-09-15" }));

import CalendarView from "./CalendarView";
import { TaskList } from "../planner/TaskList";
import { calendarPlanCycleIds, calendarTasks, PREFERRED_VIEW_KEY, savePreferredView } from "./calendar-model";

const month = { id: "month", type: "month", parent_id: null, title: "Long-term plan", starts_on: "2026-09-01", ends_on: "2026-12-01", archived: false } as Cycle;
const week = { id: "week", type: "week", parent_id: "month", title: "This week", starts_on: "2026-09-14", ends_on: "2026-09-21", archived: false } as Cycle;
const day = { id: "day", type: "day", parent_id: "week", title: "Today", starts_on: "2026-09-15", ends_on: "2026-09-16", archived: false, finished: false } as Cycle;
function task(id: string, cycle_id: string, title: string, parent_id: string | null = null): TaskNode {
  return { id, cycle_id, title, parent_id, completed: false, proposal: null, root_color_key: null, children: [], subtasks: [], position: 0 } as unknown as TaskNode;
}
let content: Record<string, EditorWorkspace>;
beforeEach(() => {
  vi.clearAllMocks(); localStorage.clear();
  content = {
    month: { cycle: month, work_mix: null, tasks: [{ ...task("goal", "month", "Launch the product"), root_color_key: "green" }] },
    week: { cycle: week, work_mix: null, tasks: [task("outcome", "week", "Validate the prototype", "goal")] },
    day: { cycle: day, work_mix: null, tasks: [task("work", "day", "Review the layout", "outcome"), task("blank", "day", "")] },
  };
  backend.getPlannerState.mockResolvedValue({ cycles: [month, week, day] });
  backend.getEditorWorkspace.mockImplementation(async (id: string) => structuredClone(content[id]));
  backend.getEditorWorkspacesByCycleIds.mockImplementation(async (ids: string[]) => structuredClone(Object.fromEntries(ids.map((id) => [id, content[id]]))));
  backend.getPreviewSummary.mockResolvedValue({ tasks: [] });
  backend.patchTask.mockImplementation(async (id: string, patch: object) => {
    const target = Object.values(content).flatMap((workspace) => workspace.tasks).find((task) => task.id === id)!;
    Object.assign(target, patch); return structuredClone(target);
  });
  backend.getCalendarRange.mockResolvedValue({ start: day.starts_on, end: day.starts_on, grid_start: day.starts_on, grid_end: day.starts_on,
    days: [{ date: day.starts_on, in_range: true, day_cycle: day, sessions: [] }] });
});
function mount() {
  return render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
    <section aria-label="Workspace editor"><TaskList cycleId="day" cycleType="day" locked={false} /></section>
    <CalendarView />
  </QueryClientProvider>);
}

describe("calendar content uses the workspace tasks", () => {
  it("keeps the block count separate from the day-move grip", async () => {
    backend.getCalendarRange.mockResolvedValue({ start: day.starts_on, end: day.starts_on, grid_start: day.starts_on, grid_end: day.starts_on,
      days: [{ date: day.starts_on, in_range: true, day_cycle: day, sessions: [{ session: { id: "focus", title: "Focus", duration: 1500000, finished: false }, schedule: null }] }] });
    mount();
    const count = await screen.findByText("1 block");
    expect(count.closest('[draggable="true"]')).toBeNull();
    expect(count.className).toContain("cursor-default");
    const grip = screen.getByTitle("Move 2026-09-15");
    expect(grip.draggable).toBe(true);
    expect(grip.textContent).not.toContain("block");
  });
  it("completes a task directly in Week and locates the same task in the detail editor", async () => {
    localStorage.setItem(PREFERRED_VIEW_KEY, "week");
    mount();
    const complete = await screen.findByRole("checkbox", { name: "Mark “Review the layout” complete in calendar" });
    fireEvent.click(complete);
    await waitFor(() => expect(complete.getAttribute("aria-checked")).toBe("true"));
    expect(backend.patchTask).toHaveBeenCalledWith("work", { completed: true });
    const editor = within(screen.getByLabelText("Day plan 2026-09-15"));
    await waitFor(() => expect(editor.getByRole("checkbox", { name: "Mark “Review the layout” complete" }).getAttribute("aria-checked")).toBe("true"));
    fireEvent.click(screen.getByRole("button", { name: "View task Review the layout" }));
    await waitFor(() => expect(document.activeElement).toBe(editor.getByDisplayValue("Review the layout")));
  });

  it("failed completion keeps the original state and reports the error in the card", async () => {
    localStorage.setItem(PREFERRED_VIEW_KEY, "week");
    backend.patchTask.mockRejectedValue({ code: "db_error", message: "Save failed" });
    mount();
    const complete = await screen.findByRole("checkbox", { name: "Mark “Review the layout” complete in calendar" });
    fireEvent.click(complete);
    expect(await screen.findByText("Save failed")).toBeTruthy();
    expect(complete.getAttribute("aria-checked")).toBe("false");
  });
  it("week shows every task so busy days grow, while month keeps its summary", async () => {
    content.day!.tasks = Array.from({ length: 6 }, (_, i) => task(`work-${i}`, "day", `Task ${i + 1}`)).concat(task("blank", "day", ""));
    mount();
    expect(await screen.findByRole("button", { name: "View task Task 2" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "View task Task 6" })).toBeNull();
    fireEvent.click(screen.getByRole("tab", { name: "week" }));
    expect(await screen.findByRole("button", { name: "View task Task 6" })).toBeTruthy();
    expect(screen.queryByText("+4 more")).toBeNull();
  });
  it("shows real day tasks, their goal context and inherited color without counting the input row", async () => {
    mount();
    const preview = await screen.findByRole("button", { name: "View task Review the layout" });
    expect(preview.querySelector<HTMLElement>(".task-color-slot")?.style.backgroundColor).toBe("rgb(22, 163, 74)");
    expect(screen.getByText("0/1 tasks")).toBeTruthy();
    expect(within(screen.getByRole("region", { name: "Weekly goals" })).getByText("Validate the prototype")).toBeTruthy();
    expect(within(screen.getByRole("region", { name: "Long-term goals" })).getByText("Launch the product")).toBeTruthy();
    expect(backend.getEditorWorkspacesByCycleIds).toHaveBeenCalledWith(["day", "month", "week"]);
    expect(backend.addTask).not.toHaveBeenCalled();
  });

  it("completion and renaming in either editor refresh the calendar projection without event delivery", async () => {
    mount();
    const workspace = within(screen.getByRole("region", { name: "Workspace editor" }));
    const calendar = within(await screen.findByLabelText("Day plan 2026-09-15"));
    fireEvent.click(await workspace.findByRole("checkbox", { name: "Mark “Review the layout” complete" }));
    await waitFor(() => expect(screen.getByText("1/1 tasks")).toBeTruthy());
    expect(calendar.getByRole("checkbox", { name: "Mark “Review the layout” complete" }).getAttribute("aria-checked")).toBe("true");
    fireEvent.click(calendar.getByRole("checkbox", { name: "Mark “Review the layout” complete" }));
    await waitFor(() => expect(workspace.getByRole("checkbox", { name: "Mark “Review the layout” complete" }).getAttribute("aria-checked")).toBe("false"));
    const title = calendar.getByDisplayValue("Review the layout");
    fireEvent.change(title, { target: { value: "Review the revised layout" } });
    fireEvent.blur(title);
    expect(await screen.findByRole("button", { name: "View task Review the revised layout" })).toBeTruthy();
    expect(await workspace.findByDisplayValue("Review the revised layout")).toBeTruthy();
    expect(backend.patchTask).toHaveBeenLastCalledWith("work", { title: "Review the revised layout" });
    const workspaceTitle = workspace.getByDisplayValue("Review the revised layout");
    fireEvent.change(workspaceTitle, { target: { value: "Updated from Workspace" } });
    fireEvent.blur(workspaceTitle);
    expect(await calendar.findByDisplayValue("Updated from Workspace")).toBeTruthy();
    expect(await screen.findByRole("button", { name: "View task Updated from Workspace" })).toBeTruthy();
  });

  it("rejects failed edits without claiming the other view changed", async () => {
    backend.patchTask.mockRejectedValue({ code: "db_error", message: "Save failed" });
    mount();
    const calendar = within(await screen.findByLabelText("Day plan 2026-09-15"));
    const title = await calendar.findByDisplayValue("Review the layout");
    fireEvent.change(title, { target: { value: "Unsaved title" } }); fireEvent.blur(title);
    expect(await calendar.findByText("Save failed")).toBeTruthy();
    expect(screen.getByRole("button", { name: "View task Review the layout" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "View task Unsaved title" })).toBeNull();
    fireEvent.click(screen.getByRole("tab", { name: "schedule" }));
    fireEvent.click(screen.getByRole("tab", { name: "plan" }));
    expect(calendar.getByDisplayValue("Unsaved title")).toBeTruthy();
    expect(calendar.getByText("Save failed")).toBeTruthy();
  });

  it("a failed task query is not presented as zero work or missing goals", async () => {
    backend.getEditorWorkspacesByCycleIds.mockRejectedValue(new Error("Read failed"));
    mount();
    expect(await screen.findByText("Tasks unavailable")).toBeTruthy();
    expect(screen.queryByText("0/0 tasks")).toBeNull();
    expect(screen.queryByText("No tasks in this week yet.")).toBeNull();
  });
});

describe("calendar projection boundaries", () => {
  it("includes ancestors but excludes Later, archived plans and unrelated dates", () => {
    expect(calendarPlanCycleIds([month, week, day, { ...month, id: "later" }, { ...month, id: "archived", archived: true },
      { ...day, id: "other", starts_on: "2027-01-01", ends_on: "2027-01-02" }], "2026-09-15", "2026-09-15"))
      .toEqual(["day", "month", "week"]);
  });
  it("filters empty inputs but retains labeled previews and completed tasks", () => {
    const real = { ...task("real", "day", "Done"), completed: true };
    expect(calendarTasks([real, task("empty", "day", "  "), { ...real, id: "proposed", proposal: "upsert" }])).toEqual([real, { ...real, id: "proposed", proposal: "upsert" }]);
  });
  it("calendar density never overwrites the app view preference", () => {
    localStorage.setItem("planner.preferred-view", "calendar");
    savePreferredView("week");
    expect(localStorage.getItem("planner.preferred-view")).toBe("calendar");
    expect(localStorage.getItem(PREFERRED_VIEW_KEY)).toBe("week");
  });
});

it("shows the same pending task in Week and locks completion until confirmation",async()=>{
  localStorage.setItem(PREFERRED_VIEW_KEY,"week");
  content.day!.tasks[0]!.proposal="upsert";
  mount();
  const checkbox=await screen.findByRole("checkbox",{name:"Mark “Review the layout” complete in calendar"});
  expect(checkbox.hasAttribute("disabled")).toBe(true);
  expect(screen.getByRole("button",{name:"View task Review the layout"}).textContent).toContain("预览");
  expect(screen.getByText("0/0 tasks · 1 预览")).toBeTruthy();
  fireEvent.click(checkbox);
  expect(backend.patchTask).not.toHaveBeenCalled();
});

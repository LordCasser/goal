import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { Cycle, TaskNode } from "../../lib/ipc";
import type { RelationView } from "./relations";
const mocks = vi.hoisted(() => ({ setTaskParentLink: vi.fn() }));
vi.mock("../../lib/ipc", async (original) => ({ ...(await original<typeof import("../../lib/ipc")>()), ...mocks }));
import { ParentGoalPicker } from "./ParentGoalPicker";
const root = { id: "goal", title: "Launch the product", cycle_id: "m", root_color_key: "blue", children: [] } as unknown as TaskNode;
const week = { id: "weekly", title: "Validate prototype", cycle_id: "w", parent_id: null, children: [] } as unknown as TaskNode;
const day = { ...week, id: "daily", title: "Interview users", cycle_id: "d" };
function mount(task = week, options = { locked: false, nested: false }) {
  const relations: RelationView = { tasks: new Map([[root.id, root], [week.id, week], [task.id, task]]), cycles: new Map([["m", { id: "m", type: "month" } as Cycle], ["w", { id: "w", type: "week", parent_id: "m" } as Cycle], ["d", { id: "d", type: "day", parent_id: "w" } as Cycle]]), selectedId: null, highlighted: new Set(), select: vi.fn(), preview: vi.fn(), setDragging: vi.fn() };
  render(<QueryClientProvider client={new QueryClient()}><ParentGoalPicker task={task} relations={relations} {...options} /></QueryClientProvider>);
  fireEvent.click(screen.getByRole("button"));
  return relations;
}
beforeEach(() => vi.clearAllMocks());
describe("parent goal selection", () => {
  it("offers daily tasks both weekly and long-term goals and saves direct ownership", async () => {
    mocks.setTaskParentLink.mockResolvedValue({ ...day, parent_id: root.id });
    const relations = mount(day);
    expect(screen.getByRole("menu", { name: "Link weekly or long-term goal" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "Link to Validate prototype" })).toBeTruthy();
    fireEvent.click(screen.getByRole("menuitem", { name: "Link to Launch the product" }));
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(mocks.setTaskParentLink).toHaveBeenCalledWith(day.id, root.id);
    expect(relations.select).toHaveBeenCalledWith(day.id);
  });
  it("names the actual long-term parent when unlinking a daily task", async () => {
    mocks.setTaskParentLink.mockResolvedValue(day);
    mount({ ...day, parent_id: root.id });
    fireEvent.click(screen.getByRole("menuitem", { name: "Unlink long-term goal" }));
    await waitFor(() => expect(mocks.setTaskParentLink).toHaveBeenCalledWith(day.id, null));
  });
  it("saves a selected goal and keeps the new relationship selected", async () => {
    mocks.setTaskParentLink.mockResolvedValue({ ...week, parent_id: root.id });
    const relations = mount();
    fireEvent.click(screen.getByRole("menuitem", { name: "Link to Launch the product" }));
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(mocks.setTaskParentLink).toHaveBeenCalledWith(week.id, root.id);
    expect(relations.select).toHaveBeenCalledWith(week.id);
  });
  it("allows removing ownership explicitly", async () => {
    mocks.setTaskParentLink.mockResolvedValue(week);
    mount({ ...week, parent_id: root.id });
    fireEvent.click(screen.getByRole("menuitem", { name: "Unlink long-term goal" }));
    await waitFor(() => expect(mocks.setTaskParentLink).toHaveBeenCalledWith(week.id, null));
  });
  it("preserves the picker and old parent if saving fails", async () => {
    mocks.setTaskParentLink.mockRejectedValue({ message: "Could not save connection" });
    const relations = mount();
    fireEvent.click(screen.getByRole("menuitem", { name: "Link to Launch the product" }));
    await screen.findByRole("alert");
    expect(screen.getByRole("menu")).toBeTruthy();
    expect(relations.select).not.toHaveBeenCalled();
  });
  it("keeps ended plans read-only while allowing relationship inspection", () => {
    mount({ ...week, parent_id: root.id }, { locked: true, nested: false });
    expect((screen.getByRole("menuitem", { name: "Unlink long-term goal" }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole("menuitem", { name: "View connections" }) as HTMLButtonElement).disabled).toBe(false);
  });
});

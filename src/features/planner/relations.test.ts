import { describe, expect, it } from "vitest";
import type { Cycle, TaskNode } from "../../lib/ipc";
import { canAssignParent, canLink, dailyGoal, directRelations, highlightedTasks, indexTasks, ROOT_PALETTE, taskColor } from "./relations";
const cycles = new Map([
  { id: "m", type: "month", parent_id: null }, { id: "w", type: "week", parent_id: "m" },
  { id: "d", type: "day", parent_id: "w" }, { id: "other", type: "week", parent_id: "elsewhere" },
].map((cycle) => [cycle.id, cycle as Cycle]));
const task = (id: string, cycle_id: string, parent_id: string | null = null, extra: Partial<TaskNode> = {}): TaskNode => ({ id, cycle_id, parent_id, title: id, proposal: null, root_color_key: null, children: [], ...extra } as TaskNode);
const root = task("goal", "m", null, { root_color_key: "blue" });
const week = task("weekly", "w", root.id);
const day = task("daily", "d", week.id);
describe("task ownership", () => {
  it("resolves direct and weekly goal ownership through daily child steps", () => {
    const step = task("step", "d", day.id);
    const graph = indexTasks([[root], [week], [day, step]]);
    expect(dailyGoal(step, graph, cycles)).toEqual({ goal: root, via: week });
    graph.set(day.id, { ...day, parent_id: root.id });
    expect(dailyGoal(step, graph, cycles)).toEqual({ goal: root, via: null });
    const childGoal = task("goal-step", "m", root.id);
    graph.set(childGoal.id, childGoal);
    graph.set(day.id, { ...day, parent_id: childGoal.id });
    expect(dailyGoal(step, graph, cycles)?.goal).toBe(childGoal);
  });
  it("does not invent long-term ownership for independent, broken or cyclic ancestry", () => {
    const graph = indexTasks([[root], [{ ...week, parent_id: null }], [day]]);
    expect(dailyGoal(day, graph, cycles)).toBeNull();
    graph.set(week.id, { ...week, parent_id: day.id });
    expect(dailyGoal(day, graph, cycles)).toBeNull();
    expect(dailyGoal({ ...day, parent_id: "missing" }, graph, cycles)).toBeNull();
    expect(dailyGoal(week, graph, cycles)).toBeNull();
  });
  it("links weekly and daily tasks to long-term goals independently of the container branch", () => {
    expect(canLink(week, root, cycles)).toBe(true);
    expect(canLink(day, week, cycles)).toBe(true);
    expect(canLink(day, root, cycles)).toBe(true);
    expect(canLink(root, day, cycles)).toBe(false);
    expect(canLink(week, day, cycles)).toBe(false);
    expect(canLink(task("unrelated", "other"), root, cycles)).toBe(true);
  });
  it("applies the weekly date constraint only to weekly parents", () => {
    const dated = new Map(cycles);
    dated.set("d", { ...cycles.get("d")!, starts_on: "2026-09-16" });
    dated.set("w", { ...cycles.get("w")!, starts_on: "2026-09-01", ends_on: "2026-09-08" });
    dated.set("m", { ...cycles.get("m")!, starts_on: "2026-10-01", ends_on: "2026-11-01" });
    expect(canAssignParent(day, week, dated)).toBe(false);
    expect(canAssignParent(day, root, dated)).toBe(true);
    expect(canAssignParent(day, { ...root, proposal: "upsert" }, dated)).toBe(false);
    expect(canAssignParent(day, { ...root, title: " " }, dated)).toBe(false);
    dated.set("later", { ...dated.get("m")!, id: "later" });
    expect(canAssignParent(day, { ...root, cycle_id: "later" }, dated)).toBe(false);
  });
  it("shows direct daily ownership and inherits its goal color through child steps", () => {
    const step = task("step", "d", day.id);
    const directDay = { ...day, parent_id: root.id, children: [step] };
    const graph = indexTasks([[root], [week], [directDay]]);
    expect(directRelations(directDay.id, graph, cycles).map(([a, b]) => [a.id, b.id])).toEqual([[root.id, day.id]]);
    expect(highlightedTasks(root.id, graph, cycles)).toEqual(new Set([root.id, week.id, day.id, step.id]));
    expect(taskColor(step, graph)).toBe(ROOT_PALETTE.blue);
  });
  it("inherits color through linked goals and same-cycle steps, updates on reassignment", () => {
    const step = task("step", "d", day.id);
    const alternate = task("alternate", "m", null, { root_color_key: "plum" });
    const graph = indexTasks([[root, alternate], [week], [day, step]]);
    expect(taskColor(step, graph)).toBe(ROOT_PALETTE.blue);
    graph.set(week.id, { ...week, parent_id: alternate.id });
    expect(taskColor(step, graph)).toBe(ROOT_PALETTE.plum);
    graph.set(week.id, { ...week, parent_id: null });
    expect(taskColor(step, graph)).toBeNull();
  });
  it("shows direct relations without expanding transitive descendants", () => {
    const graph = indexTasks([[root], [week], [day]]);
    expect(directRelations(root.id, graph, cycles).map(([a,b]) => [a.id,b.id])).toEqual([[root.id,week.id]]);
    expect(directRelations(week.id, graph, cycles)).toHaveLength(2);
    expect(highlightedTasks(root.id, graph, cycles).has(day.id)).toBe(false);
  });
  it("keeps preview identity in the graph while excluding empty inputs", () => {
    const step = task("step", "w", week.id);
    const graph = indexTasks([[root], [{ ...week, children: [step] }, task("blank", "w", null, { title: " " }), task("proposed", "w", null, { proposal: "upsert" })]]);
    expect([...highlightedTasks(root.id, graph, cycles)]).toEqual([root.id,week.id,step.id]);
    expect(graph.has("blank")).toBe(false);
    expect(graph.has("proposed")).toBe(true);
    const previewGraph=indexTasks([[root],[{...week,proposal:"upsert"}],[day]]);
    expect(taskColor(day,previewGraph)).toBe(ROOT_PALETTE.blue);
  });
  it("does not hang on malformed parent cycles", () => {
    const a = task("a", "w", "b"), b = task("b", "w", "a");
    expect(taskColor(a, indexTasks([[a,b]]))).toBeNull();
  });
});

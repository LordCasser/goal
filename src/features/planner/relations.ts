import type { Cycle, RootColorKey, TaskNode } from "../../lib/ipc";

export const ROOT_PALETTE: Record<RootColorKey, string> = {
  red: "#e53935", amber: "#df7800", gold: "#b58b00", green: "#16a34a",
  teal: "#009b91", blue: "#2563eb", indigo: "#6546ed", plum: "#ae32cc",
};
export type TaskGraph = Map<string, TaskNode>;
export type RelationView = {
  tasks: TaskGraph;
  cycles: Map<string, Cycle>;
  selectedId: string | null;
  highlighted: Set<string>;
  select: (id: string) => void;
  preview: (id: string | null) => void;
  setDragging: (dragging: boolean) => void;
};

export function indexTasks(trees: TaskNode[][]): TaskGraph {
  const tasks: TaskGraph = new Map();
  const visit = (nodes: TaskNode[]) => nodes.forEach((task) => {
    if (task.title.trim()) tasks.set(task.id, task);
    visit(task.children);
  });
  trees.forEach(visit);
  return tasks;
}

/** Traversing same-cycle steps is allowed; cross-cycle ownership is adjacent only. */
export function canLink(child: TaskNode, parent: TaskNode, cycles: Map<string, Cycle>): boolean {
  const childCycle = cycles.get(child.cycle_id);
  const parentCycle = cycles.get(parent.cycle_id);
  return !!childCycle && !!parentCycle && parentCycle.id !== "later"
    && ((childCycle.type === "week" && parentCycle.type === "month") || (childCycle.type === "day" && parentCycle.type === "week"));
}

/** Menu and drag ownership use the same planning/date constraints. */
export function canAssignParent(child: TaskNode, parent: TaskNode, cycles: Map<string, Cycle>): boolean {
  if (child.proposal || parent.proposal || !parent.title.trim() || !canLink(child, parent, cycles)) return false;
  const day = cycles.get(child.cycle_id);
  const week = cycles.get(parent.cycle_id);
  return day?.type !== "day" || !day.starts_on || !week?.starts_on || !week.ends_on
    || (week.starts_on <= day.starts_on && day.starts_on < week.ends_on);
}

export function taskColor(task: TaskNode, tasks: TaskGraph): string | null {
  const seen = new Set<string>();
  let current: TaskNode | undefined = task;
  while (current && !seen.has(current.id)) {
    seen.add(current.id);
    const color = current.root_color_key as RootColorKey | null;
    if (color && color in ROOT_PALETTE) return ROOT_PALETTE[color];
    current = current.parent_id ? tasks.get(current.parent_id) : undefined;
  }
  return null;
}

export function directRelations(id: string | null, tasks: TaskGraph, cycles: Map<string, Cycle>) {
  const selected = id ? tasks.get(id) : undefined;
  const parent = selected?.parent_id ? tasks.get(selected.parent_id) : undefined;
  const edges: [TaskNode, TaskNode][] = [];
  if (selected && parent && canLink(selected, parent, cycles)) edges.push([parent, selected]);
  if (selected) for (const child of tasks.values()) {
    if (child.parent_id === selected.id && canLink(child, selected, cycles)) edges.push([selected, child]);
  }
  return edges;
}

export function highlightedTasks(id: string | null, tasks: TaskGraph, cycles: Map<string, Cycle>): Set<string> {
  const result = new Set<string>();
  if (!id || !tasks.has(id)) return result;
  result.add(id);
  directRelations(id, tasks, cycles).forEach(([a, b]) => { result.add(a.id); result.add(b.id); });
  // Include the steps of related rows, without recursively opening other horizons.
  const visit = (task: TaskNode) => task.children.forEach((child) => {
    if (!result.has(child.id)) { result.add(child.id); visit(child); }
  });
  [...result].forEach((key) => { const task = tasks.get(key); if (task) visit(task); });
  return result;
}

import { createContext, useContext, useState, type Dispatch, type SetStateAction, type ReactNode } from "react";
import type { TaskNode } from "../../lib/ipc";

export const TASK_DRAG_TYPE = "application/x-planner-task";
type TaskDrag = { task: TaskNode; parentId: string | null } | null;
const TaskDragContext = createContext<[TaskDrag, Dispatch<SetStateAction<TaskDrag>>] | null>(null);

/** One drag session across visible horizons; isolated editors can still reorder. */
export function TaskDragProvider({ children }: { children: ReactNode }) {
  const state = useState<TaskDrag>(null);
  return <TaskDragContext.Provider value={state}>{children}</TaskDragContext.Provider>;
}

export function useTaskDrag() {
  const local = useState<TaskDrag>(null);
  return useContext(TaskDragContext) ?? local;
}

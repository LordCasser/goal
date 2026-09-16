import { useLayoutEffect, useState, type RefObject } from "react";
import type { TaskNode } from "../../lib/ipc";
import { taskColor, type TaskGraph } from "./relations";

type Curve = { id: string; path: string; x1: number; y1: number; x2: number; y2: number; color: string };
export function RelationLayer({ viewportRef, edges, tasks, hidden }: {
  viewportRef: RefObject<HTMLDivElement | null>; edges: [TaskNode, TaskNode][]; tasks: TaskGraph; hidden: boolean;
}) {
  const [curves, setCurves] = useState<Curve[]>([]);
  useLayoutEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport || hidden || !edges.length) { setCurves([]); return; }
    let frame = 0;
    let disposed = false;
    const rows = new Map(Array.from(viewport.querySelectorAll<HTMLElement>("[data-task-id]")).map((el) => [el.dataset.taskId!, el]));
    const measure = () => {
      frame = 0;
      if (disposed) return;
      const bounds = viewport.getBoundingClientRect();
      const anchor = (task: TaskNode, side: "left" | "right") => {
        const row = rows.get(task.id);
        const title = row?.querySelector("textarea");
        const scroll = row?.closest("[data-plan-scroll]");
        if (!row || !title || !scroll) return null;
        const rect = row.getBoundingClientRect();
        const text = title.getBoundingClientRect();
        const clip = scroll.getBoundingClientRect();
        const x = side === "right" ? rect.right + 3 : rect.left - 3;
        const y = text.top + 14;
        if (x < bounds.left || x > bounds.right || y < Math.max(bounds.top, clip.top) + 3 || y > Math.min(bounds.bottom, clip.bottom) - 3) return null;
        return { x: x - bounds.left, y: y - bounds.top };
      };
      const next: Curve[] = [];
      for (const [parent, child] of edges) {
        const start = anchor(parent, "right"); const end = anchor(child, "left");
        if (!start || !end || end.x <= start.x) continue;
        const mid = (start.x + end.x) / 2;
        next.push({ id: `${parent.id}:${child.id}`, x1: start.x, y1: start.y, x2: end.x, y2: end.y,
          path: `M${start.x},${start.y} C${mid},${start.y} ${mid},${end.y} ${end.x},${end.y}`,
          color: taskColor(parent, tasks) ?? "#858c96" });
      }
      setCurves(next);
    };
    const schedule = () => { if (!frame && !disposed) frame = requestAnimationFrame(measure); };
    const observer = new ResizeObserver(schedule);
    observer.observe(viewport);
    rows.forEach((row) => observer.observe(row));
    viewport.addEventListener("scroll", schedule, true);
    window.addEventListener("resize", schedule);
    document.fonts?.ready.then(schedule);
    measure();
    return () => { disposed = true; cancelAnimationFrame(frame); observer.disconnect(); viewport.removeEventListener("scroll", schedule, true); window.removeEventListener("resize", schedule); };
  }, [edges, tasks, hidden, viewportRef]);
  return <svg aria-hidden="true" data-relation-layer className="pointer-events-none absolute inset-0 z-10 h-full w-full overflow-hidden">
    {curves.map((curve) => <g key={curve.id} className="relation-line" stroke={curve.color} fill={curve.color}>
      <path d={curve.path} fill="none" strokeWidth="1.25" />
      <rect x={curve.x1 - 2.5} y={curve.y1 - 2.5} width="5" height="5" stroke="none" />
      <rect x={curve.x2 - 2.5} y={curve.y2 - 2.5} width="5" height="5" stroke="none" />
    </g>)}
  </svg>;
}

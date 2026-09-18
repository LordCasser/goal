import { useEffect, useRef, useState, type RefObject } from "react";
import type { TaskNode } from "../../lib/ipc";
import { taskColor, type TaskGraph } from "./relations";

type Curve = { id: string; path: string; x1: number; y1: number; x2: number; y2: number; clipped1: number; clipped2: number; dashed: boolean; color: string };
export function RelationLayer({ viewportRef, edges, tasks, hidden }: {
  viewportRef: RefObject<HTMLDivElement | null>; edges: [TaskNode, TaskNode][]; tasks: TaskGraph; hidden: boolean;
}) {
  const [curves, setCurves] = useState<Curve[]>([]);
  const layerRef = useRef<SVGSVGElement>(null);
  // The viewport belongs to our parent; wait until its ref is attached too.
  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport || hidden || !edges.length) { setCurves([]); return; }
    let frame = 0;
    let disposed = false;
    const observed = new Set<HTMLElement>();
    const measure = () => {
      frame = 0;
      if (disposed) return;
      // Queries and plan transitions can replace rows without changing edges.
      // Always measure the current DOM and keep resize subscriptions in sync.
      const elements = new Set(viewport.querySelectorAll<HTMLElement>("[data-task-id], [data-plan-scroll], [data-plan-horizon]"));
      for (const element of observed) if (!elements.has(element)) {
        observer.unobserve(element);
        observed.delete(element);
      }
      for (const element of elements) if (!observed.has(element)) {
        observer.observe(element);
        observed.add(element);
      }
      const rows = new Map(Array.from(elements)
        .filter((el) => el.dataset.taskId && !el.closest('[inert], [hidden], [data-removal-state="exiting"]'))
        .map((el) => [el.dataset.taskId!, el]));
      const bounds = viewport.getBoundingClientRect();
      const horizons = [...elements].filter((el) => el.hasAttribute("data-plan-horizon"));
      const anchor = (task: TaskNode, side: "left" | "right") => {
        const row = rows.get(task.id);
        const title = row?.querySelector("textarea");
        const scroll = row?.closest("[data-plan-scroll]");
        if (!row || !title || !scroll) return null;
        const rect = row.getBoundingClientRect();
        const text = title.getBoundingClientRect();
        const clip = scroll.getBoundingClientRect();
        if (!rect.width || !rect.height || !clip.width || !clip.height) return null;
        const x = side === "right" ? rect.right + 3 : rect.left - 3;
        const titleY = text.top + 14;
        const top = Math.max(bounds.top, clip.top) + 6;
        const bottom = Math.min(bounds.bottom, clip.bottom) - 6;
        if (bottom <= top) return null;
        // Keep off-screen relationships traceable. A chevron at the scroll
        // edge distinguishes a clipped task from a real visible endpoint.
        const clipped = titleY < top ? -1 : titleY > bottom ? 1 : 0;
        return { x: x - bounds.left, y: Math.max(top, Math.min(bottom, titleY)) - bounds.top,
          clipped, horizon: row.closest<HTMLElement>("[data-plan-horizon]") };
      };
      const next: Curve[] = [];
      for (const [parent, child] of edges) {
        const start = anchor(parent, "right"); const end = anchor(child, "left");
        if (!start || !end || end.x <= start.x) continue;
        const mid = (start.x + end.x) / 2;
        let path = `M${start.x},${start.y} C${mid},${start.y} ${mid},${end.y} ${end.x},${end.y}`;
        const first = start.horizon ? horizons.indexOf(start.horizon) : -1;
        const last = end.horizon ? horizons.indexOf(end.horizon) : -1;
        const dashed = first >= 0 && last > first + 1;
        if (dashed) {
          const skipped = horizons.slice(first + 1, last);
          const body = [...rows.values()].filter((row) => skipped.includes(row.closest<HTMLElement>("[data-plan-horizon]")!))
            .filter((row) => row.querySelector("textarea")?.value.trim())
            .map((row) => {
              const rect = row.getBoundingClientRect();
              const clip = row.closest("[data-plan-scroll]")?.getBoundingClientRect();
              return { left: rect.left - bounds.left, right: rect.right - bounds.left,
                top: Math.max(rect.top, clip?.top ?? rect.top, bounds.top) - bounds.top,
                bottom: Math.min(rect.bottom, clip?.bottom ?? rect.bottom, bounds.bottom) - bounds.top };
            }).filter((rect) => rect.bottom > rect.top);
          if (body.length) {
            const top = Math.min(...body.map((rect) => rect.top));
            const bottom = Math.max(...body.map((rect) => rect.bottom));
            // Prefer the nearer side of the actual text block. A long block
            // may be crossed instead of forcing a large, distracting detour.
            const above = Math.max(12, top - 24); const below = Math.min(bounds.height - 12, bottom + 24);
            const cost = (y: number) => Math.abs(start.y - y) + Math.abs(end.y - y);
            const route = cost(above) <= cost(below) ? above : below;
            const detour = (cost(route) - Math.abs(start.y - end.y)) / 2;
            const overlaps = Math.max(start.y, end.y) >= top && Math.min(start.y, end.y) <= bottom;
            if (overlaps && detour <= Math.min(240, bounds.height * 0.35)) {
              // Turn beside the body, not at the midpoint of the whole link.
              // Both transitions meet the quiet middle span tangentially.
              const left = Math.max(start.x + 24, Math.min(...body.map((rect) => rect.left)) - 20);
              const right = Math.min(end.x - 24, Math.max(...body.map((rect) => rect.right)) + 20);
              if (right > left) {
                const enter = (start.x + left) / 2; const leave = (right + end.x) / 2;
                path = `M${start.x},${start.y} C${enter},${start.y} ${enter},${route} ${left},${route}`
                  + ` L${right},${route} C${leave},${route} ${leave},${end.y} ${end.x},${end.y}`;
              }
            }
          }
        }
        next.push({ id: `${parent.id}:${child.id}`, x1: start.x, y1: start.y, x2: end.x, y2: end.y,
          clipped1: start.clipped, clipped2: end.clipped, path, dashed,
          color: taskColor(parent, tasks) ?? "#858c96" });
      }
      setCurves(next);
    };
    const schedule = () => { if (!frame && !disposed) frame = requestAnimationFrame(measure); };
    const observer = new ResizeObserver(schedule);
    observer.observe(viewport);
    const mutations = new MutationObserver((records) => {
      // Rendering the SVG itself must not schedule another measurement.
      if (records.some((record) => !layerRef.current?.contains(record.target))) schedule();
    });
    mutations.observe(viewport, {
      childList: true, subtree: true, attributes: true,
      attributeFilter: ["inert", "hidden", "data-task-id", "data-removal-state"],
    });
    viewport.addEventListener("scroll", schedule, true);
    window.addEventListener("resize", schedule);
    document.fonts?.ready.then(schedule);
    measure();
    return () => { disposed = true; cancelAnimationFrame(frame); observer.disconnect(); mutations.disconnect(); viewport.removeEventListener("scroll", schedule, true); window.removeEventListener("resize", schedule); };
  }, [edges, tasks, hidden, viewportRef]);
  return <svg ref={layerRef} aria-hidden="true" data-relation-layer className="pointer-events-none absolute inset-0 z-10 h-full w-full overflow-hidden">
    {!hidden && curves.filter((curve) => edges.some(([parent, child]) => curve.id === `${parent.id}:${child.id}`)).map((curve) => <g key={curve.id} className="relation-line" stroke={curve.color} fill={curve.color}>
      <path d={curve.path} fill="none" strokeWidth="1.25" strokeLinejoin="round" strokeLinecap="round"
        strokeDasharray={curve.dashed ? "4 5" : undefined} strokeOpacity={curve.dashed ? 0.8 : 1} />
      {([[curve.x1, curve.y1, curve.clipped1], [curve.x2, curve.y2, curve.clipped2]] as const).map(([x, y, clipped], index) => clipped
        ? <path key={index} data-clipped-endpoint d={`M${x - 3},${y - clipped * 3} L${x},${y + clipped} L${x + 3},${y - clipped * 3}`} fill="none" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
        : <rect key={index} x={x - 2.5} y={y - 2.5} width="5" height="5" stroke="none" />)}
    </g>)}
  </svg>;
}

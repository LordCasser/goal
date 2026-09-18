import { afterEach, expect, it, vi } from "vitest";
import { render, waitFor } from "@testing-library/react";
import { createRef, useEffect, useState } from "react";
import { RelationLayer } from "./RelationLayer";
import type { TaskNode } from "../../lib/ipc";

afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });

it.each([
  ["adjacent", 1, 100, 36, 100, null],
  ["above a short weekly body", 2, 100, 36, 100, 76],
  ["with an empty weekly column", 2, 100, 36, 100, null],
  ["below a short weekly body", 2, 100, 180, 248, 304],
  ["through a long weekly body", 2, 80, 490, 294, null],
] as const)("connects a daily link %s with smooth end tangents", (label, dailyColumn, bodyTop, bodyHeight, rowTop, routeY) => {
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  const rect = (x: number, y: number, width: number, height: number) => new DOMRect(x, y, width, height);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.dataset.viewport !== undefined) return rect(0, 0, 1000, 600);
    const index = Number(this.closest("[data-plan-horizon]")?.getAttribute("data-plan-horizon") ?? 0);
    if (this.dataset.planHorizon !== undefined) return rect(index * 340, 80, 300, 500);
    if (this.dataset.planScroll !== undefined) return rect(index * 340, 80, 300, 500);
    if (this.dataset.taskId === "weekly") return rect(index * 340 + 20, bodyTop, 260, bodyHeight);
    return rect(index * 340 + 20, rowTop, 260, 36);
  });
  const goal = { id: "goal", cycle_id: "month", root_color_key: "blue" } as TaskNode;
  const day = { id: "daily", cycle_id: "day", parent_id: goal.id } as TaskNode;
  const viewportRef = createRef<HTMLDivElement>();
  const view = (edges: [TaskNode, TaskNode][]) => <div ref={viewportRef} data-viewport>
    <div data-plan-horizon="0"><div data-plan-scroll="0"><div data-task-id="goal"><textarea /></div></div></div>
    {Array.from({ length: dailyColumn - 1 }, (_, index) => <div key={index} data-plan-horizon={index + 1}>
      {label === "with an empty weekly column" ? <p>No weekly plan</p> : <div data-plan-scroll={index + 1}><div data-task-id="weekly"><textarea defaultValue="Weekly work" /></div></div>}
    </div>)}
    <div data-plan-horizon={dailyColumn}><div data-plan-scroll={dailyColumn}><div data-task-id="daily"><textarea /></div></div></div>
    <RelationLayer viewportRef={viewportRef} edges={edges} tasks={new Map([[goal.id, goal], [day.id, day]])} hidden={false} />
  </div>;
  const { container, rerender } = render(view([]));
  rerender(view([[goal, day]]));
  const line = container.querySelector("g.relation-line")!;
  const path = line.querySelector("path")!;
  const start = 283;
  const end = dailyColumn * 340 + 17;
  const mid = (start + end) / 2;
  const y = rowTop + 14;
  expect(path.getAttribute("d")).toBe(routeY === null
    ? `M${start},${y} C${mid},${y} ${mid},${y} ${end},${y}`
    : `M${start},${y} C311.5,${y} 311.5,${routeY} 340,${routeY} L640,${routeY} C668.5,${routeY} 668.5,${y} ${end},${y}`);
  expect(line.getAttribute("stroke")).toBe("#2563eb");
  expect(path.getAttribute("stroke-width")).toBe("1.25");
  expect(path.getAttribute("stroke-dasharray")).toBe(dailyColumn === 1 ? null : "4 5");
  const endpoints = [...container.querySelectorAll("g.relation-line > rect")].map((el) => [Number(el.getAttribute("x")) + 2.5, Number(el.getAttribute("y")) + 2.5]);
  expect(endpoints).toEqual([[start, y], [end, y]]);
});

it.each([["above", -100, 86], ["below", 700, 574]] as const)("keeps a vertically clipped %s endpoint at the scroll edge", (_label, parentTop, endpointY) => {
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.dataset.viewport !== undefined) return new DOMRect(0, 0, 1000, 600);
    if (this.dataset.planScroll !== undefined) return new DOMRect(0, 80, 900, 500);
    const id = this.closest<HTMLElement>("[data-task-id]")?.dataset.taskId;
    return new DOMRect(id === "goal" ? 100 : 500, id === "goal" ? parentTop : 100, 260, 36);
  });
  const goal = { id: "goal", cycle_id: "month", root_color_key: "blue" } as TaskNode;
  const day = { id: "daily", cycle_id: "day", parent_id: goal.id } as TaskNode;
  const viewportRef = createRef<HTMLDivElement>();
  const { container } = render(<div ref={viewportRef} data-viewport>
    <div data-plan-scroll><div data-task-id="goal"><textarea /></div><div data-task-id="daily"><textarea /></div></div>
    <RelationLayer viewportRef={viewportRef} edges={[[goal, day]]} tasks={new Map([[goal.id, goal], [day.id, day]])} hidden={false} />
  </div>);
  const line = container.querySelector("g.relation-line");
  expect(line?.querySelector("path")?.getAttribute("d")).toBe(`M363,${endpointY} C430,${endpointY} 430,114 497,114`);
  expect(line?.querySelectorAll("rect")).toHaveLength(1);
  expect(line?.querySelector("[data-clipped-endpoint]")).not.toBeNull();
});

it("keeps a curve when one horizontal endpoint is outside the viewport", () => {
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  const rect = (x: number, y: number, width: number, height: number) => new DOMRect(x, y, width, height);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.dataset.viewport !== undefined) return rect(0, 0, 400, 600);
    if (this.dataset.planScroll !== undefined) {
      const column = Number(this.dataset.planScroll);
      return rect(column === 0 ? -380 : 280, 80, 300, 500);
    }
    const row = this.closest<HTMLElement>("[data-task-id]");
    if (row?.dataset.taskId === "goal") return rect(-360, 100, 260, 36);
    if (row?.dataset.taskId === "daily") return rect(300, 100, 260, 36);
    return rect(0, 0, 0, 0);
  });
  const goal = { id: "goal", cycle_id: "month", root_color_key: "blue" } as TaskNode;
  const day = { id: "daily", cycle_id: "day", parent_id: goal.id } as TaskNode;
  const edges: [TaskNode, TaskNode][] = [[goal, day]];
  const tasks = new Map([[goal.id, goal], [day.id, day]]);
  const viewportRef = createRef<HTMLDivElement>();
  const view = (relationEdges: [TaskNode, TaskNode][]) => <div ref={viewportRef} data-viewport>
    <div data-plan-scroll="0"><div data-task-id="goal"><textarea /></div></div>
    <div data-plan-scroll="1"><div data-task-id="daily"><textarea /></div></div>
    <RelationLayer viewportRef={viewportRef} edges={relationEdges} tasks={tasks} hidden={false} />
  </div>;
  const { container, rerender } = render(view([]));
  rerender(view(edges));

  const path = container.querySelector("g.relation-line path");
  expect(path).not.toBeNull();
  expect(path?.getAttribute("d")).toBe("M-97,114 C100,114 100,114 297,114");
});

it("renders edges that are present on the initial mount", () => {
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  const rect = (x: number, y: number, width: number, height: number) => new DOMRect(x, y, width, height);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.dataset.viewport !== undefined) return rect(0, 0, 1000, 600);
    if (this.dataset.planScroll !== undefined) return rect(0, 80, 900, 500);
    const row = this.closest<HTMLElement>("[data-task-id]");
    if (row?.dataset.taskId === "goal") return rect(100, 100, 260, 36);
    if (row?.dataset.taskId === "daily") return rect(500, 100, 260, 36);
    return rect(0, 0, 0, 0);
  });
  const goal = { id: "goal", cycle_id: "month", root_color_key: "blue" } as TaskNode;
  const day = { id: "daily", cycle_id: "day", parent_id: goal.id } as TaskNode;
  const edges: [TaskNode, TaskNode][] = [[goal, day]];
  const tasks = new Map([[goal.id, goal], [day.id, day]]);
  const viewportRef = createRef<HTMLDivElement>();
  const { container } = render(
    <div ref={viewportRef} data-viewport>
      <div data-plan-scroll="0"><div data-task-id="goal"><textarea /></div></div>
      <div data-plan-scroll="1"><div data-task-id="daily"><textarea /></div></div>
      <RelationLayer viewportRef={viewportRef} edges={edges} tasks={tasks} hidden={false} />
    </div>,
  );

  expect(container.querySelector("g.relation-line path")).not.toBeNull();
});

it.each(["inert", "hidden", "exiting"] as const)("ignores a retained %s copy of an endpoint", (state) => {
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.dataset.viewport !== undefined) return new DOMRect(0, 0, 1000, 600);
    if (this.dataset.planScroll !== undefined) return new DOMRect(0, 80, 900, 500);
    const row = this.closest<HTMLElement>("[data-task-id]");
    const x = row?.hasAttribute("data-old-row") ? 700 : row?.dataset.taskId === "goal" ? 100 : 500;
    return new DOMRect(x, 100, 260, 36);
  });
  const goal = { id: "goal", cycle_id: "month", root_color_key: "blue" } as TaskNode;
  const day = { id: "daily", cycle_id: "day", parent_id: goal.id } as TaskNode;
  const viewportRef = createRef<HTMLDivElement>();
  const { container } = render(<div ref={viewportRef} data-viewport>
    <div data-plan-scroll>
      <div data-task-id="goal"><textarea /></div><div data-task-id="daily"><textarea /></div>
      <div inert={state === "inert"} hidden={state === "hidden"} data-removal-state={state === "exiting" ? "exiting" : undefined}>
        <div data-task-id="goal" data-old-row><textarea /></div>
      </div>
    </div>
    <RelationLayer viewportRef={viewportRef} edges={[[goal, day]]} tasks={new Map([[goal.id, goal], [day.id, day]])} hidden={false} />
  </div>);
  expect(container.querySelector("g.relation-line path")?.getAttribute("d")).toBe("M363,114 C430,114 430,114 497,114");
});

it("recovers when related rows mount asynchronously with stable edge and task props", async () => {
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  const rect = (x: number, y: number, width: number, height: number) => new DOMRect(x, y, width, height);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.dataset.viewport !== undefined) return rect(0, 0, 1000, 600);
    if (this.dataset.planScroll !== undefined) return rect(0, 80, 900, 500);
    const row = this.closest<HTMLElement>("[data-task-id]");
    if (row?.dataset.taskId === "goal") return rect(100, 100, 260, 36);
    if (row?.dataset.taskId === "daily") return rect(500, 100, 260, 36);
    return rect(0, 0, 0, 0);
  });
  const goal = { id: "goal", cycle_id: "month", root_color_key: "blue" } as TaskNode;
  const day = { id: "daily", cycle_id: "day", parent_id: goal.id } as TaskNode;
  const edges: [TaskNode, TaskNode][] = [[goal, day]];
  const tasks = new Map([[goal.id, goal], [day.id, day]]);
  const viewportRef = createRef<HTMLDivElement>();
  function AsyncRows() {
    const [ready, setReady] = useState(false);
    useEffect(() => {
      const timer = window.setTimeout(() => setReady(true), 0);
      return () => window.clearTimeout(timer);
    }, []);
    return <div ref={viewportRef} data-viewport>
      {ready && <>
        <div data-plan-scroll="0"><div data-task-id="goal"><textarea /></div></div>
        <div data-plan-scroll="1"><div data-task-id="daily"><textarea /></div></div>
      </>}
      <RelationLayer viewportRef={viewportRef} edges={edges} tasks={tasks} hidden={false} />
    </div>;
  }

  const { container } = render(<AsyncRows />);
  expect(container.querySelector("g.relation-line")).toBeNull();
  await waitFor(() => expect(container.querySelector("g.relation-line")).not.toBeNull());
});

it("updates when a related row with the same id is replaced with stable edge and task props", async () => {
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  const rect = (x: number, y: number, width: number, height: number) => new DOMRect(x, y, width, height);
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    if (this.dataset.viewport !== undefined) return rect(0, 0, 1000, 600);
    if (this.dataset.planScroll !== undefined) return rect(0, 80, 900, 500);
    const row = this.closest<HTMLElement>("[data-task-id]");
    if (row?.dataset.taskId === "goal") return rect(row.dataset.rowVersion === "1" ? 200 : 100, 100, 260, 36);
    if (row?.dataset.taskId === "daily") return rect(500, 100, 260, 36);
    return rect(0, 0, 0, 0);
  });
  const goal = { id: "goal", cycle_id: "month", root_color_key: "blue" } as TaskNode;
  const day = { id: "daily", cycle_id: "day", parent_id: goal.id } as TaskNode;
  const edges: [TaskNode, TaskNode][] = [[goal, day]];
  const tasks = new Map([[goal.id, goal], [day.id, day]]);
  const viewportRef = createRef<HTMLDivElement>();
  function ReplacedRow({ relationEdges }: { relationEdges: [TaskNode, TaskNode][] }) {
    const [version, setVersion] = useState(0);
    useEffect(() => {
      const timer = window.setTimeout(() => setVersion(1), 0);
      return () => window.clearTimeout(timer);
    }, []);
    return <div ref={viewportRef} data-viewport>
      <div data-plan-scroll="0"><div key={version} data-task-id="goal" data-row-version={version}><textarea /></div></div>
      <div data-plan-scroll="1"><div data-task-id="daily"><textarea /></div></div>
      <RelationLayer viewportRef={viewportRef} edges={relationEdges} tasks={tasks} hidden={false} />
    </div>;
  }

  const { container, rerender } = render(<ReplacedRow relationEdges={[]} />);
  rerender(<ReplacedRow relationEdges={edges} />);
  const path = () => container.querySelector("g.relation-line path")?.getAttribute("d");
  expect(path()).toBe("M363,114 C430,114 430,114 497,114");
  await waitFor(() => expect(path()).toBe("M463,114 C480,114 480,114 497,114"));
});

import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { Cycle, TaskNode } from "../../lib/ipc";
const mocks = vi.hoisted(() => ({ getEditorWorkspace: vi.fn(), getPreviewSummary: vi.fn(), getTaskContinuity: vi.fn(), addTask: vi.fn(), patchTask: vi.fn(), reorderTasks: vi.fn(), setTaskParentLink: vi.fn() }));
vi.mock("../../lib/ipc", async (original) => ({ ...(await original<typeof import("../../lib/ipc")>()), ...mocks }));
import { TaskList } from "./TaskList";
import { indexTasks, type RelationView } from "./relations";
import { applyLocale } from "../../lib/i18n";
const empty = { id: "empty", title: "", parent_id: null, completed: false, children: [], proposal: null } as unknown as TaskNode;
function mount(active = true) { return render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><TaskList active={active} cycleId="c1" cycleType="day" locked={false} /></QueryClientProvider>); }
beforeEach(() => {
  vi.clearAllMocks();
  mocks.getEditorWorkspace.mockResolvedValue({ cycle: { id: "c1" }, tasks: [empty] });
  mocks.getPreviewSummary.mockResolvedValue({ tasks: [] });
  mocks.getTaskContinuity.mockResolvedValue({ title: "", parent_goal_id: null, selected_episode_index: 0, episodes: [] });
  mocks.reorderTasks.mockResolvedValue(undefined);
  mocks.setTaskParentLink.mockResolvedValue(undefined);
  applyLocale("en");
});

describe("task row actions", () => {
  const task = (id: string, cycleId = "c1", parentId: string | null = null): TaskNode => ({
    ...empty, id, cycle_id: cycleId, title: id, parent_id: parentId,
  });
  const row = (title: string) => screen.getByDisplayValue(title).closest("[data-task-id]")!;
  const handle = (title: string) => screen.getByRole("button", { name: `Reorder ${title}` });
  const dragData = () => {
    const values = new Map<string, string>();
    return { setData: (type: string, value: string) => { values.set(type, value); }, getData: (type: string) => values.get(type) ?? "",
      setDragImage: vi.fn(), effectAllowed: "none", dropEffect: "none" };
  };
  it("describes the inherited long-term goal from the focused daily row control", async () => {
    vi.stubGlobal("ResizeObserver", class { observe() {} disconnect() {} });
    try {
      const goal = task("Ship the product", "m");
      const week = task("Prepare release", "w", goal.id);
      const day = task("Verify onboarding", "c1", week.id);
      const relations: RelationView = {
        tasks: indexTasks([[goal], [week], [day]]),
        cycles: new Map([...["m", "w", "c1"].map((id, index) => [id, { id, type: ["month", "week", "day"][index] } as Cycle] as const)]),
        selectedId: null, highlighted: new Set(), select: vi.fn(), preview: vi.fn(),
      };
      mocks.getEditorWorkspace.mockResolvedValue({ tasks: [day, task("Another task"), empty] });
      render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <TaskList cycleId="c1" cycleType="day" locked={false} relations={relations} />
      </QueryClientProvider>);
      const grip = await screen.findByRole("button", { name: "Reorder Verify onboarding" });
      act(() => grip.focus());
      const tooltip = await screen.findByRole("tooltip");
      expect(tooltip.textContent).toContain(goal.title);
      expect(tooltip.textContent).toContain(week.title);
      expect(document.activeElement).toBe(grip);
      expect(grip.getAttribute("aria-describedby")).toBe(tooltip.id);
      const checkbox = screen.getByRole("checkbox", { name: 'Mark “Verify onboarding” complete' });
      act(() => checkbox.focus());
      expect(checkbox.getAttribute("aria-describedby")).toBe(tooltip.id);
    } finally { vi.unstubAllGlobals(); }
  });
  it("uses only a compact draggable handle for local ordering", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [task("First"), task("Second"), empty] });
    mount();
    await screen.findByDisplayValue("First");
    const item = row("First") as HTMLElement;
    expect(handle("First").getAttribute("draggable")).toBe("true");
    expect(item.draggable).toBe(false);
    expect(item.querySelector("[data-task-actions]")).toBeNull();
  });
  it("hides the drag handle until the row is hovered while keeping it keyboard accessible", async () => {
    mocks.getEditorWorkspace.mockImplementation((cycleId: string) => Promise.resolve(cycleId === "preview"
      ? { tasks: [{ ...task("Preview", "preview"), proposal: "upsert" }, { ...empty, id: "preview-empty", cycle_id: "preview" }] }
      : { tasks: [task("Only task"), empty] }));
    render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <TaskList active cycleId="c1" cycleType="day" locked={false} />
      <TaskList active cycleId="preview" cycleType="day" locked={false} />
    </QueryClientProvider>);

    const grip = await screen.findByRole("button", { name: "Reorder Only task" });
    expect(grip.hasAttribute("data-task-drag-handle")).toBe(true);
    expect(grip.getAttribute("aria-label")).toBe("Reorder Only task");
    expect(grip.getAttribute("tabindex")).toBe("0");
    expect(grip.classList.contains("opacity-0")).toBe(true);
    expect(grip.classList.contains("group-hover:opacity-100")).toBe(true);
    expect(grip.classList.contains("focus-visible:opacity-100")).toBe(true);
    act(() => grip.focus());
    expect(document.activeElement).toBe(grip);

    const blank = screen.getAllByRole("textbox", { name: "Add a task…" })[0]!.closest("[data-task-id]")!;
    expect(blank.querySelector("[data-task-drag-handle]")).toBeNull();
    const preview = screen.getByDisplayValue("Preview").closest("[data-task-id]")!;
    expect(preview.querySelector("[data-task-drag-handle]")).toBeNull();
  });
  it("reorders sibling roots by dragging to the blank row and leaves it last", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [task("First", "c1", "goal-a"), task("Second", "c1", "goal-b"), empty] });
    mount();
    await screen.findByDisplayValue("Second");
    const dataTransfer = dragData();
    fireEvent.dragStart(handle("First"), { dataTransfer });
    fireEvent.dragOver(row("Second"), { dataTransfer, clientY: 1 });
    fireEvent.drop(row("Second"), { dataTransfer, clientY: 1 });
    await waitFor(() => expect(mocks.reorderTasks).toHaveBeenCalledWith("c1", null, ["Second", "First", "empty"]));
    expect(mocks.setTaskParentLink).not.toHaveBeenCalled();
  });
  it("moves past hidden historical blank rows in the same sibling group", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [task("First"), { ...empty, id: "old-blank" }, task("Second"), empty] });
    mount();
    await screen.findByDisplayValue("Second");
    fireEvent.keyDown(handle("Second"), { key: "ArrowUp" });
    await waitFor(() => expect(mocks.reorderTasks).toHaveBeenCalledWith("c1", null, ["Second", "First", "old-blank", "empty"]));
  });
  it("supports keyboard ordering on the handle", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [task("First"), task("Second"), empty] });
    mount();
    await screen.findByDisplayValue("Second");
    const grip = handle("Second");
    grip.focus();
    fireEvent.keyDown(grip, { key: "ArrowUp" });
    expect(document.activeElement).toBe(grip);
    await waitFor(() => expect(mocks.reorderTasks).toHaveBeenCalledWith("c1", null, ["Second", "First", "empty"]));
  });
  it("does not move a sibling past the final blank row", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [task("First"), task("Second"), empty] });
    mount();
    await screen.findByDisplayValue("Second");
    fireEvent.keyDown(handle("Second"), { key: "ArrowDown" });
    expect(mocks.reorderTasks).not.toHaveBeenCalled();
  });
  it("ignores a drop after cancelling the drag", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [task("First"), task("Second"), empty] });
    mount(); await screen.findByDisplayValue("Second");
    const dataTransfer = dragData();
    fireEvent.dragStart(handle("First"), { dataTransfer });
    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.drop(row("Second"), { dataTransfer, clientY: 1 });
    expect(mocks.reorderTasks).not.toHaveBeenCalled();
  });
  it("rolls an optimistic handle reorder back and reports a persistence failure", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [task("First"), task("Second"), empty] });
    mocks.reorderTasks.mockRejectedValue({ code: "db_error", message: "Could not access local data. Try again or check the diagnostic logs." });
    mount(); await screen.findByDisplayValue("Second");
    fireEvent.keyDown(handle("First"), { key: "ArrowDown" });
    await screen.findByText("Could not access local data. Try again or check the diagnostic logs.");
    expect((screen.getAllByRole("textbox")[0] as HTMLTextAreaElement).value).toBe("First");
  });
  it("does not mix nested sibling groups or reorder a locked preview", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [
      { ...task("Parent"), children: [task("Step", "c1", "Parent"), task("Other step", "c1", "Parent")] },
      { ...task("Preview"), proposal: "upsert" }, empty,
    ] });
    mount(); await screen.findByDisplayValue("Step");
    const dataTransfer = dragData();
    fireEvent.dragStart(handle("Step"), { dataTransfer });
    fireEvent.dragOver(row("Parent"), { dataTransfer, clientY: 1 });
    fireEvent.drop(row("Parent"), { dataTransfer, clientY: 1 });
    expect(mocks.reorderTasks).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "Reorder Preview" })).toBeNull();
  });
  it("does not reorder across plan lists", async () => {
    mocks.getEditorWorkspace.mockImplementation((id: string) => Promise.resolve({ tasks: [task(`${id}-first`, id), task(`${id}-second`, id), { ...empty, id: `${id}-blank`, cycle_id: id }] }));
    render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <TaskList cycleId="c1" cycleType="day" locked={false} />
      <TaskList cycleId="c2" cycleType="day" locked={false} />
    </QueryClientProvider>);
    await screen.findByDisplayValue("c2-first");
    const dataTransfer = dragData();
    fireEvent.dragStart(handle("c1-first"), { dataTransfer });
    fireEvent.dragOver(row("c2-second"), { dataTransfer, clientY: 1 });
    fireEvent.drop(row("c2-second"), { dataTransfer, clientY: 1 });
    expect(mocks.reorderTasks).not.toHaveBeenCalled();
    expect(mocks.setTaskParentLink).not.toHaveBeenCalled();
  });

  it("opens the task note and continuity details on double-click", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [task("Daily task"), empty] });
    mocks.getTaskContinuity.mockResolvedValue({
      title: "Daily task", parent_goal_id: null, selected_episode_index: 0,
      episodes: [{ started_on: "2026-09-21", last_recorded_on: "2026-09-21", completed: false,
        elapsed_days: 1, recorded_days: 1, records: [{ task_id: "Daily task", date: "2026-09-21", note: "First note", completed: false }] }],
    });
    mount();
    fireEvent.doubleClick(await screen.findByDisplayValue("Daily task"));
    expect(await screen.findByRole("heading", { name: "Daily task" })).toBeTruthy();
    expect(await screen.findByRole("textbox", { name: "Note" })).toBeTruthy();
    expect(await screen.findByText("First note")).toBeTruthy();
  });

  it("clears the double-click text selection before returning focus after saving details", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [task("Daily task"), empty] });
    mocks.patchTask.mockResolvedValue(undefined);
    mount();
    const title = await screen.findByDisplayValue("Daily task") as HTMLTextAreaElement;
    title.focus();
    title.setSelectionRange(0, title.value.length);
    fireEvent.doubleClick(title);
    const note = await screen.findByRole("textbox", { name: "Note" });
    fireEvent.change(note, { target: { value: "Saved note" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(document.activeElement).toBe(title));
    expect(title.selectionStart).toBe(title.selectionEnd);
    expect(mocks.patchTask).toHaveBeenCalledWith("Daily task", { note: "Saved note" });
  });

  async function mountHorizons(dayStart = "2026-09-15") {
    const goal = task("Goal", "month");
    const week = task("Weekly", "week");
    const daily = task("Daily", "day");
    const tasks = indexTasks([[goal, week, daily]]);
    const relations: RelationView = {
      tasks, cycles: new Map([
        ["month", { id: "month", type: "month" } as Cycle],
        ["week", { id: "week", type: "week", starts_on: "2026-09-14", ends_on: "2026-09-21" } as Cycle],
        ["day", { id: "day", type: "day", starts_on: dayStart } as Cycle],
      ]), selectedId: null, highlighted: new Set(), select: vi.fn(), preview: vi.fn(),
    };
    mocks.getEditorWorkspace.mockImplementation((id: string) => Promise.resolve({ tasks: [tasks.get(id === "month" ? "Goal" : id === "week" ? "Weekly" : "Daily"), { ...empty, id: `${id}-blank`, cycle_id: id }] }));
    render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      {(["month", "week", "day"] as const).map((type) => <TaskList key={type} cycleId={type} cycleType={type} locked={false} relations={relations} />)}
    </QueryClientProvider>);
    await screen.findByDisplayValue("Daily"); await screen.findByDisplayValue("Weekly"); await screen.findByDisplayValue("Goal");
  }
  it("links a weekly root to a long-term goal through the parent picker", async () => {
    await mountHorizons();
    fireEvent.click(screen.getByRole("button", { name: "Link Weekly to long-term goal" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Link to Goal" }));
    await waitFor(() => expect(mocks.setTaskParentLink).toHaveBeenCalledWith("Weekly", "Goal"));
    expect(mocks.reorderTasks).not.toHaveBeenCalled();
  });
  it("links a daily task directly to a long-term goal with its picker", async () => {
    await mountHorizons();
    fireEvent.click(screen.getByRole("button", { name: "Link Daily to weekly or long-term goal" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: "Link to Goal" }));
    await waitFor(() => expect(mocks.setTaskParentLink).toHaveBeenCalledWith("Daily", "Goal"));
  });
  it("refuses a daily parent link outside the weekly date range", async () => {
    await mountHorizons("2026-09-22");
    fireEvent.click(screen.getByRole("button", { name: "Link Daily to weekly or long-term goal" }));
    expect(screen.queryByRole("menuitem", { name: "Link to Weekly" })).toBeNull();
    expect(mocks.setTaskParentLink).not.toHaveBeenCalled();
  });
});
describe("task editing", () => {
  it("localizes goal color names in the picker and current color label", async () => {
    await act(async () => { applyLocale("zh-CN"); });
    try {
      mocks.getEditorWorkspace.mockResolvedValue({ tasks: [{ ...empty, id: "goal", title: "Goal", root_color_key: "blue" }, empty] });
      render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <TaskList active cycleId="month" cycleType="month" locked={false} />
      </QueryClientProvider>);
      const color = await screen.findByRole("button", { name: "目标颜色: 蓝色" });
      fireEvent.click(color);
      expect(await screen.findByRole("menuitem", { name: "蓝色" })).toBeTruthy();
      expect(screen.queryByRole("menuitem", { name: "blue" })).toBeNull();
    } finally {
      await act(async () => { applyLocale("en"); });
    }
  });

  it("a retained inactive editor does not create an input row", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [{ ...empty, id: "goal", title: "Existing task" }] });
    mount(false);
    await screen.findByDisplayValue("Existing task");
    expect(mocks.addTask).not.toHaveBeenCalled();
  });
  it("shows one root input affordance and Enter focuses it without creating another row", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [{ ...empty, id: "goal", title: "Existing task" }, empty, { ...empty, id: "last" }] });
    mount();
    const goal = await screen.findByDisplayValue("Existing task");
    expect(screen.getAllByPlaceholderText("Add a task…")).toHaveLength(2);
    fireEvent.keyDown(goal, { key: "Enter" });
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("textbox", { name: "Add a task…" })));
    expect(mocks.addTask).not.toHaveBeenCalled();
  });
  it("keeps the input below a task restored after it and uses the visible predecessor for nesting", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [
      { ...empty, id: "first", title: "First" },
      empty,
      { ...empty, id: "restored", title: "Restored" },
    ] });
    mount();
    const restored = await screen.findByDisplayValue("Restored");
    const list = restored.closest("[data-task-list]")!;
    expect(Array.from(list.querySelectorAll("[data-task-id]")).map((element) => element.getAttribute("data-task-id")))
      .toEqual(["first", "restored", "empty"]);
    fireEvent.keyDown(restored, { key: "Tab" });
    await waitFor(() => expect(mocks.setTaskParentLink).toHaveBeenCalledWith("restored", "first"));
  });
  it("does not create another empty task when Enter is pressed on the blank input", async () => {
    mount();
    fireEvent.keyDown(await screen.findByRole("textbox"), { key: "Enter" });
    expect(mocks.addTask).not.toHaveBeenCalled();
  });
  it("keeps the draft and stops advancing when saving fails", async () => {
    mocks.patchTask.mockRejectedValue({ code: "db_error", message: "Could not access local data. Try again or check the diagnostic logs." });
    mount();
    const input = await screen.findByRole("textbox");
    fireEvent.change(input, { target: { value: "Write the outline" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await screen.findByText("Could not access local data. Try again or check the diagnostic logs.");
    expect((input as HTMLInputElement).value).toBe("Write the outline");
    expect(mocks.addTask).not.toHaveBeenCalled();
  });
  it("IME Enter confirms composition without submitting the task", async () => {
    mount();
    const input = await screen.findByRole("textbox");
    fireEvent.change(input, { target: { value: "梳理计划" } });
    fireEvent.keyDown(input, { key: "Enter", isComposing: true });
    await waitFor(() => expect(mocks.patchTask).not.toHaveBeenCalled());
    expect(mocks.addTask).not.toHaveBeenCalled();
  });
  it("Escape discards an edit without committing it during blur", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [{ ...empty, id: "goal", title: "Original" }, empty] });
    mount();
    const field = await screen.findByDisplayValue("Original");
    field.focus();
    fireEvent.change(field, { target: { value: "Discard this" } });
    fireEvent.keyDown(field, { key: "Escape" });
    expect((field as HTMLTextAreaElement).value).toBe("Original");
    expect(mocks.patchTask).not.toHaveBeenCalled();
  });
  it("Enter on a linked goal creates a local blank without copying its cross-cycle parent", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [{ ...empty, id: "goal", title: "Linked goal", parent_id: "other-cycle-goal" }] });
    mocks.addTask.mockImplementation(() => new Promise(() => {}));
    mount();
    const field = await screen.findByRole("textbox");
    fireEvent.keyDown(field, { key: "Enter" });
    await waitFor(() => expect(mocks.addTask).toHaveBeenLastCalledWith({ cycle_id: "c1", title: "", parent_id: null }));
  });

});

it("keeps a preview in its tree position, locks it, and restores editing when rejected",async()=>{
  const proposed={...empty,id:"goal",title:"AI revised",proposal:"upsert",position:0,children:[]} as TaskNode;
  const untouched={...empty,id:"other",title:"Untouched",position:1} as TaskNode;
  mocks.getEditorWorkspace.mockResolvedValue({tasks:[proposed,untouched,empty]});
  const client=new QueryClient({defaultOptions:{queries:{retry:false}}});
  render(<QueryClientProvider client={client}><TaskList cycleId="c1" cycleType="day" locked={false}/></QueryClientProvider>);
  const preview=await screen.findByDisplayValue("AI revised") as HTMLTextAreaElement;
  expect(preview.readOnly).toBe(true);
  expect((screen.getByDisplayValue("Untouched") as HTMLTextAreaElement).readOnly).toBe(false);
  expect(screen.getByRole("checkbox",{name:"Mark “AI revised” complete"}).hasAttribute("disabled")).toBe(true);
  expect(screen.queryByRole("button",{name:"Delete “AI revised”"})).toBeNull();
  expect(screen.queryByRole("button",{name:/Keep|Revert|确认修改/})).toBeNull();
  expect(screen.getAllByRole("textbox")[0]).toBe(preview);
  fireEvent.keyDown(preview,{key:"Enter"});fireEvent.blur(preview);
  expect(mocks.patchTask).not.toHaveBeenCalled();
  mocks.getEditorWorkspace.mockResolvedValue({tasks:[{...proposed,title:"Original",proposal:null},untouched,empty]});
  await client.invalidateQueries({queryKey:["editor-workspace"]});
  expect((await screen.findByDisplayValue("Original") as HTMLTextAreaElement).readOnly).toBe(false);
});

it("reveals a diagnostic task after its editor query resolves, without changing data",async()=>{
  let deliver!: (value: unknown)=>void;
  mocks.getEditorWorkspace.mockImplementation(()=>new Promise(resolve=>{deliver=resolve;}));
  const scroll=vi.fn(); Element.prototype.scrollIntoView=scroll;
  vi.stubGlobal("matchMedia",()=>({matches:true}));
  const client=new QueryClient({defaultOptions:{queries:{retry:false}}});
  render(<QueryClientProvider client={client}><TaskList cycleId="c1" cycleType="day" locked={false} revealTask={{taskId:"target",requestId:1}}/></QueryClientProvider>);
  await waitFor(()=>expect(deliver).toBeDefined());
  deliver({tasks:[{...empty,id:"target",title:"Inspect this"},empty]});
  const input=await screen.findByDisplayValue("Inspect this");
  await waitFor(()=>expect(document.activeElement).toBe(input));
  expect(scroll).toHaveBeenCalledWith({block:"nearest",inline:"center",behavior:"instant"});
  expect(mocks.patchTask).not.toHaveBeenCalled(); expect(mocks.addTask).not.toHaveBeenCalled();
  vi.unstubAllGlobals();
});

it("does not replay a completed external reveal after a local reveal", async () => {
  let deliver!: (value: unknown) => void;
  mocks.getEditorWorkspace.mockImplementation(() => new Promise(resolve => { deliver = resolve; }));
  const originalScroll = Element.prototype.scrollIntoView;
  const scroll = vi.fn();
  Element.prototype.scrollIntoView = scroll;
  const select = vi.fn();
  const relations = { tasks: new Map(), cycles: new Map(), selectedId: null, highlighted: new Set(), select, preview: vi.fn() } as unknown as RelationView;
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const view = (revealTask: { taskId: string; requestId: number }) => <QueryClientProvider client={client}><TaskList cycleId="c1" cycleType="day" locked={false} revealTask={revealTask} relations={relations} /></QueryClientProvider>;
  const mounted = render(view({ taskId: "first", requestId: 1 }));
  deliver({ tasks: [{ ...empty, id: "first", title: "First" }, { ...empty, id: "second", title: "Second" }, empty] });
  await screen.findByDisplayValue("First");
  await waitFor(() => expect(scroll).toHaveBeenCalledTimes(1));

  mounted.rerender(view({ taskId: "second", requestId: -1 }));
  await waitFor(() => expect(scroll).toHaveBeenCalledTimes(2));
  mounted.rerender(view({ taskId: "first", requestId: 1 }));
  await act(async () => { await Promise.resolve(); });

  expect(scroll).toHaveBeenCalledTimes(2);
  expect(select).toHaveBeenNthCalledWith(1, "first");
  expect(select).toHaveBeenNthCalledWith(2, "second");
  Element.prototype.scrollIntoView = originalScroll;
});

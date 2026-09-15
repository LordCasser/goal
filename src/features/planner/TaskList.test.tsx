import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { Cycle, TaskNode } from "../../lib/ipc";
const mocks = vi.hoisted(() => ({ getEditorWorkspace: vi.fn(), getPreviewSummary: vi.fn(), addTask: vi.fn(), patchTask: vi.fn(), reorderTasks: vi.fn(), setTaskParentLink: vi.fn() }));
vi.mock("../../lib/ipc", async (original) => ({ ...(await original<typeof import("../../lib/ipc")>()), ...mocks }));
import { TaskList } from "./TaskList";
import { TASK_DRAG_TYPE, TaskDragProvider } from "./TaskDragContext";
import { indexTasks, type RelationView } from "./relations";
const empty = { id: "empty", title: "", parent_id: null, completed: false, children: [], proposal: null } as unknown as TaskNode;
function mount(active = true) { return render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><TaskList active={active} cycleId="c1" cycleType="day" locked={false} /></QueryClientProvider>); }
beforeEach(() => {
  vi.clearAllMocks();
  mocks.getEditorWorkspace.mockResolvedValue({ cycle: { id: "c1" }, tasks: [empty] });
  mocks.getPreviewSummary.mockResolvedValue({ tasks: [] });
  mocks.reorderTasks.mockResolvedValue(undefined);
  mocks.setTaskParentLink.mockResolvedValue(undefined);
});

describe("task drag interactions", () => {
  const task = (id: string, cycleId = "c1", parentId: string | null = null): TaskNode => ({
    ...empty, id, cycle_id: cycleId, title: id, parent_id: parentId,
  });
  function transfer() {
    const data = new Map<string, string>();
    return { types: [] as string[], effectAllowed: "", dropEffect: "", setDragImage: vi.fn(),
      setData(type: string, value: string) { data.set(type, value); this.types.push(type); },
      getData(type: string) { return data.get(type) ?? ""; } };
  }
  const row = (title: string) => screen.getByDisplayValue(title).closest("[data-task-id]")!;
  function start(title: string) {
    const dataTransfer = transfer();
    fireEvent.dragStart(screen.getByRole("button", { name: `Reorder ${title}` }), { dataTransfer });
    return dataTransfer;
  }
  it("has a draggable handle before mouse-down, while keeping the editable row selectable", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [task("First"), empty] });
    mount();
    const handle = await screen.findByRole("button", { name: "Reorder First" });
    expect(handle.draggable).toBe(true);
    expect((row("First") as HTMLElement).draggable).toBe(false);
  });
  it("reorders visible roots with different goal links and leaves the blank last", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [task("First", "c1", "goal-a"), task("Second", "c1", "goal-b"), empty] });
    mount();
    await screen.findByDisplayValue("Second");
    const dataTransfer = start("First");
    fireEvent.dragOver(row("Second"), { dataTransfer });
    fireEvent.drop(row("Second"), { dataTransfer });
    await waitFor(() => expect(mocks.reorderTasks).toHaveBeenCalledWith("c1", null, ["Second", "First", "empty"]));
    expect(mocks.setTaskParentLink).not.toHaveBeenCalled();
    expect(dataTransfer.getData("text/plain")).toBe("");
  });
  it("supports keyboard ordering from the focused grip without crossing the blank", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [task("First"), task("Second"), empty] });
    mount();
    const second = await screen.findByRole("button", { name: "Reorder Second" });
    second.focus();
    fireEvent.keyDown(second, { key: "ArrowDown" });
    expect(mocks.reorderTasks).not.toHaveBeenCalled();
    fireEvent.keyDown(second, { key: "ArrowUp" });
    await waitFor(() => expect(mocks.reorderTasks).toHaveBeenCalledWith("c1", null, ["Second", "First", "empty"]));
  });
  it("rolls an optimistic reorder back and reports a persistence failure", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [task("First"), task("Second"), empty] });
    mocks.reorderTasks.mockRejectedValue({ code: "db_error", message: "Order could not be saved" });
    mount(); await screen.findByDisplayValue("Second");
    fireEvent.drop(row("Second"), { dataTransfer: start("First") });
    await screen.findByText("Order could not be saved");
    expect((screen.getAllByRole("textbox")[0] as HTMLTextAreaElement).value).toBe("First");
  });
  it("cancels with Escape and ignores subsequent stale or unrelated drops", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [task("First"), task("Second"), empty] });
    mount(); await screen.findByDisplayValue("Second");
    const dataTransfer = start("First");
    fireEvent.keyDown(window, { key: "Escape" });
    fireEvent.drop(row("Second"), { dataTransfer });
    expect(mocks.reorderTasks).not.toHaveBeenCalled();
    start("First");
    const external = transfer(); external.setData("text/plain", "First");
    fireEvent.drop(row("Second"), { dataTransfer: external });
    expect(mocks.reorderTasks).not.toHaveBeenCalled();
  });
  it("does not mix nested sibling groups or reorder a locked preview", async () => {
    mocks.getEditorWorkspace.mockResolvedValue({ tasks: [
      { ...task("Parent"), children: [task("Step", "c1", "Parent")] },
      { ...task("Preview"), proposal: "upsert" }, empty,
    ] });
    mount(); await screen.findByDisplayValue("Step");
    fireEvent.drop(row("Parent"), { dataTransfer: start("Step") });
    expect(mocks.reorderTasks).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "Reorder Preview" })).toBeNull();
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
      ]), selectedId: null, highlighted: new Set(), select: vi.fn(), preview: vi.fn(), setDragging: vi.fn(),
    };
    mocks.getEditorWorkspace.mockImplementation((id: string) => Promise.resolve({ tasks: [tasks.get(id === "month" ? "Goal" : id === "week" ? "Weekly" : "Daily"), { ...empty, id: `${id}-blank`, cycle_id: id }] }));
    render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <TaskDragProvider>{(["month", "week", "day"] as const).map((type) => <TaskList key={type} cycleId={type} cycleType={type} locked={false} relations={relations} />)}</TaskDragProvider>
    </QueryClientProvider>);
    await screen.findByDisplayValue("Daily"); await screen.findByDisplayValue("Weekly"); await screen.findByDisplayValue("Goal");
  }
  it("links a weekly root to a long-term goal with a distinct drop hint", async () => {
    await mountHorizons();
    const dataTransfer = start("Weekly");
    fireEvent.dragOver(row("Goal"), { dataTransfer });
    expect(screen.getByRole("status").textContent).toBe("Link to Goal");
    fireEvent.drop(row("Goal"), { dataTransfer });
    await waitFor(() => expect(mocks.setTaskParentLink).toHaveBeenCalledWith("Weekly", "Goal"));
    expect(mocks.reorderTasks).not.toHaveBeenCalled();
    expect(screen.queryByRole("status")).toBeNull();
  });
  it("links a daily task to this week's task but refuses skipping a horizon", async () => {
    await mountHorizons();
    fireEvent.drop(row("Goal"), { dataTransfer: start("Daily") });
    expect(mocks.setTaskParentLink).not.toHaveBeenCalled();
    fireEvent.drop(row("Weekly"), { dataTransfer: start("Daily") });
    await waitFor(() => expect(mocks.setTaskParentLink).toHaveBeenCalledWith("Daily", "Weekly"));
  });
  it("refuses a daily task's ownership drop outside the weekly date range", async () => {
    await mountHorizons("2026-09-22");
    const dataTransfer = start("Daily");
    fireEvent.dragOver(row("Weekly"), { dataTransfer });
    expect(screen.queryByRole("status")).toBeNull();
    fireEvent.drop(row("Weekly"), { dataTransfer });
    expect(mocks.setTaskParentLink).not.toHaveBeenCalled();
    expect(dataTransfer.types).toContain(TASK_DRAG_TYPE);
  });
});
describe("task editing", () => {
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
  it("does not create another empty task when Enter is pressed on the blank input", async () => {
    mount();
    fireEvent.keyDown(await screen.findByRole("textbox"), { key: "Enter" });
    expect(mocks.addTask).not.toHaveBeenCalled();
  });
  it("keeps the draft and stops advancing when saving fails", async () => {
    mocks.patchTask.mockRejectedValue({ code: "db_error", message: "Save failed" });
    mount();
    const input = await screen.findByRole("textbox");
    fireEvent.change(input, { target: { value: "Write the outline" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await screen.findByText("Save failed");
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

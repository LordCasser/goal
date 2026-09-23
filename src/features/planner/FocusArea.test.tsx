import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { applyLocale } from "../../lib/i18n";
import { qk } from "../../lib/events";
import type { EditorWorkspace, TaskNode } from "../../lib/ipc";

const mocks = vi.hoisted(() => ({
  addSession: vi.fn(),
  getEditorWorkspace: vi.fn(),
}));

vi.mock("../../lib/ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/ipc")>()),
  addSession: mocks.addSession,
  getEditorWorkspace: mocks.getEditorWorkspace,
}));

import { AddFocusBlockForm } from "./FocusArea";

function task(id: string, title: string, cycle_id: string, over: Partial<TaskNode> = {}): TaskNode {
  return {
    id,
    cycle_id,
    parent_id: null,
    title,
    note: "",
    subtasks: [],
    position: 0,
    completed: false,
    goal_breakdown: null,
    needs_refinement: null,
    needs_breakdown: null,
    root_color_key: null,
    copied_from_task_id: null,
    proposal: null,
    created_at: 0,
    children: [],
    subtasks_markdown: "",
    ...over,
    later_plan_type: over.later_plan_type ?? null,
    focused_time: over.focused_time ?? 0,
  };
}

function workspace(tasks: TaskNode[]): EditorWorkspace {
  return { cycle: null, tasks, work_mix: null };
}

function mount(dayId = "day-a", tasks: TaskNode[] = []) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  mocks.getEditorWorkspace.mockResolvedValue(workspace(tasks));
  const view = render(
    <QueryClientProvider client={client}>
      <AddFocusBlockForm dayId={dayId} open onCancel={() => {}} onAdded={() => {}} />
    </QueryClientProvider>,
  );
  return { client, ...view };
}

describe("AddFocusBlockForm task association", () => {
  beforeEach(() => {
    applyLocale("en");
    mocks.addSession.mockReset().mockResolvedValue({});
    mocks.getEditorWorkspace.mockReset();
  });

  it("submits null when no task is selected", async () => {
    mount();
    fireEvent.change(screen.getByLabelText("Focus block title"), { target: { value: "Read" } });
    fireEvent.click(screen.getByRole("button", { name: /^Add$/ }));

    await waitFor(() => expect(mocks.addSession).toHaveBeenCalledWith({
      day_cycle_id: "day-a",
      title: "Read",
      duration_ms: null,
      task_id: null,
    }));
  });

  it("offers only nonempty committed tasks from the current day", async () => {
    mount("day-a", [
      task("today", "Today task", "day-a"),
      task("blank", "  ", "day-a"),
      task("proposal", "Draft task", "day-a", { proposal: "upsert" }),
      task("other-day", "Other day", "day-b"),
      task("parent", "Parent", "day-a", { children: [task("nested", "Nested task", "day-a")] }),
    ]);
    const select = await screen.findByRole("combobox", { name: "Task (optional)" });
    fireEvent.click(select);

    expect(await screen.findByRole("option", { name: "Today task" })).toBeTruthy();
    expect(screen.getByRole("option", { name: "Parent" })).toBeTruthy();
    expect(screen.getByRole("option", { name: "Nested task" })).toBeTruthy();
    expect(screen.queryByRole("option", { name: "Draft task" })).toBeNull();
    expect(screen.queryByRole("option", { name: "Other day" })).toBeNull();
    expect(screen.queryByRole("option", { name: /^\s*$/ })).toBeNull();
  });

  it("submits the selected task id", async () => {
    mount("day-a", [task("today", "Today task", "day-a")]);
    fireEvent.click(await screen.findByRole("combobox", { name: "Task (optional)" }));
    fireEvent.click(await screen.findByRole("option", { name: "Today task" }));
    fireEvent.change(screen.getByLabelText("Focus block title"), { target: { value: "Read" } });
    fireEvent.click(screen.getByRole("button", { name: /^Add$/ }));

    await waitFor(() => expect(mocks.addSession).toHaveBeenCalledWith(expect.objectContaining({ task_id: "today" })));
  });

  it("resets the selected task when the day changes or the task disappears", async () => {
    const mounted = mount("day-a", [task("today", "Today task", "day-a")]);
    const select = await screen.findByRole("combobox", { name: "Task (optional)" });
    fireEvent.click(select);
    fireEvent.click(await screen.findByRole("option", { name: "Today task" }));
    expect(select.textContent).toContain("Today task");

    mounted.client.setQueryData(qk.editorWorkspace("day-a"), workspace([]));
    await waitFor(() => expect(select.textContent).toContain("No task"));

    mounted.client.setQueryData(qk.editorWorkspace("day-b"), workspace([]));
    mounted.rerender(
      <QueryClientProvider client={mounted.client}>
        <AddFocusBlockForm dayId="day-b" open onCancel={() => {}} onAdded={() => {}} />
      </QueryClientProvider>,
    );
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Task (optional)" }).textContent).toContain("No task"));
  });

  it("disables the form controls while saving", async () => {
    let resolve: (() => void) | undefined;
    mocks.addSession.mockImplementation(() => new Promise<void>((done) => { resolve = done; }));
    mount("day-a", [task("today", "Today task", "day-a")]);
    fireEvent.change(screen.getByLabelText("Focus block title"), { target: { value: "Read" } });
    fireEvent.click(screen.getByRole("button", { name: /^Add$/ }));

    await waitFor(() => {
      expect(screen.getByRole("combobox", { name: "Task (optional)" }).getAttribute("disabled")).not.toBeNull();
      expect(screen.getByRole("button", { name: "Cancel" }).getAttribute("disabled")).not.toBeNull();
      expect(screen.getByRole("button", { name: "25m" }).getAttribute("disabled")).not.toBeNull();
    });
    resolve?.();
  });
});

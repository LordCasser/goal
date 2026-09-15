/**
 * IssuePanel 行为契约（openspec planning-issues「问题报告」「忽略的持久化
 * 与作用域」）：列表按类型 label + detail 渲染、忽略调用区分任务级/周期级
 * 作用域并携带所选原因、空报告只留一行轻提示。invoke 按 src/lib/ipc.test.ts
 * 的方式 mock——组件经由 lib/ipc 走真实包装，同时钉住线上的参数键。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { commands, type PlanningIssue } from "../../lib/ipc";
import { IssuePanel } from "./IssuePanel";

function issueFixture(over: Partial<PlanningIssue> = {}): PlanningIssue {
  return {
    issue_type: "too_many_goals",
    cycle_id: "c1",
    task_id: null,
    title: "目标过多",
    detail: "这个周期塞了 9 个目标。",
    ...over,
  };
}

/** 按命令名分发固定返回值；未覆盖的命令一律返回 null。 */
function mockBackend(issues: PlanningIssue[] = []): void {
  invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case commands.getPlanningIssueReport:
        return Promise.resolve(issues);
      default:
        return Promise.resolve(null);
    }
  });
}

function renderPanel(): void {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <IssuePanel cycleId="c1" onClose={() => {}} />
    </QueryClientProvider>,
  );
}

/** 在某条问题行内点开忽略菜单并选择一项。 */
async function dismissIssue(typeLabel: string, reasonLabel: string): Promise<void> {
  const label = await screen.findByText(typeLabel);
  const row = label.closest("li");
  expect(row).not.toBeNull();
  fireEvent.click(within(row as HTMLElement).getByRole("button", { name: "忽略" }));
  fireEvent.click(await screen.findByRole("menuitem", { name: reasonLabel }));
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("IssuePanel", () => {
  it("renders each issue with its Chinese type label and detail", async () => {
    mockBackend([
      issueFixture({ issue_type: "too_many_goals", detail: "这个周期塞了 9 个目标。" }),
      issueFixture({
        issue_type: "not_sure_what_to_do_next",
        task_id: "t1",
        title: "学英语",
        detail: "「学英语」没有可执行的下一步。",
      }),
    ]);
    renderPanel();

    expect(await screen.findByText("目标过多")).toBeDefined();
    expect(screen.getByText("这个周期塞了 9 个目标。")).toBeDefined();
    expect(screen.getByText("不清楚下一步")).toBeDefined();
    expect(screen.getByText("「学英语」没有可执行的下一步。")).toBeDefined();
    // 顶部说明行：诊断而非评分（spec: 诊断而非评分）。
    expect(screen.getByText("不评分，只指出具体问题。")).toBeDefined();
  });

  it("dismisses a task-level issue with its task_id and no reason", async () => {
    mockBackend([
      issueFixture({
        issue_type: "not_sure_what_to_do_next",
        task_id: "t1",
        detail: "「学英语」没有可执行的下一步。",
      }),
    ]);
    renderPanel();

    await dismissIssue("不清楚下一步", "直接忽略");
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.dismissPlanningIssue, {
        cycle_id: "c1",
        issue_type: "not_sure_what_to_do_next",
        task_id: "t1",
        reason: null,
      }),
    );
  });

  it("dismisses a cycle-level issue without task_id, carrying the chosen reason", async () => {
    mockBackend([issueFixture({ issue_type: "too_much_work", task_id: null })]);
    renderPanel();

    await dismissIssue("工作量过大", "Planning felt like too much work");
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.dismissPlanningIssue, {
        cycle_id: "c1",
        issue_type: "too_much_work",
        task_id: null,
        reason: "Planning felt like too much work",
      }),
    );
  });

  it("shows a single light line when no issue was found", async () => {
    mockBackend([]);
    renderPanel();

    expect(await screen.findByText("没有发现计划问题。")).toBeDefined();
    expect(screen.queryByText("目标过多")).toBeNull();
  });
});

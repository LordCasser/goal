import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ComponentProps } from "react";
import { applyLocale, formatDate } from "../../lib/i18n";
const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
import { commands, type PlanningIssueReport } from "../../lib/ipc";
import { IssuePanel } from "./IssuePanel";

const finding: PlanningIssueReport["issues"][number] = {
  issue_type: "not_sure_what_to_do_next", cycle_id: "c1", task_id: "t1",
  title: "明确验证方式", detail: "只有项目名，请补充一个可以验证的结果。", task_title: "Prototype", source: "ai",
};
function report(over: Partial<PlanningIssueReport> = {}): PlanningIssueReport {
  return { cycle_id: "c1", cycle_title: "Tuesday", cycle_type: "day", starts_on: "2026-09-15", task_count: 2,
    pending_count: 0, ignored_count: 0, issues: [], ai_status: "not_checked", checked_at: null, checked_count: 0, model: null, ...over };
}
function mockBackend(data = report(), available = true) {
  invokeMock.mockImplementation((cmd: string) => Promise.resolve(cmd === commands.getPlanningIssueReport ? data : cmd === commands.getAiAvailability ? available : null));
}
function mount(props: Partial<ComponentProps<typeof IssuePanel>> = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  return render(<QueryClientProvider client={client}><IssuePanel cycleId="c1" onClose={() => {}} {...props} /></QueryClientProvider>);
}
beforeEach(() => { applyLocale("zh-CN"); invokeMock.mockReset(); });

describe("plan diagnostics", () => {
  it("distinguishes an unchecked plan from a successful empty AI report", async () => {
    mockBackend(); mount();
    expect(await screen.findByText(`日计划 · ${formatDate("2026-09-15")}`)).toBeTruthy();
    expect(screen.getByText("规则提示已更新，AI 尚未检查。")).toBeTruthy();
    expect(screen.queryByText("本次检查未发现需要处理的问题。")).toBeNull();
    expect(invokeMock).toHaveBeenCalledWith(commands.getPlanningIssueReport, { cycleId: "c1", refresh: false });
  });
  it("requires verified AI and offers settings without hiding structural findings", async () => {
    const settings = vi.fn();
    mockBackend(report({ issues: [{ ...finding, source: "structure" }] }), false); mount({ onOpenSettings: settings });
    expect(await screen.findByText("规则提示")).toBeTruthy();
    expect((screen.getByRole("button", { name: "AI 检查" }) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "打开设置" })); expect(settings).toHaveBeenCalledOnce();
  });
  it("shows real pending feedback then the model, checked count and completed result", async () => {
    let data = report(); let resolve!: (value: PlanningIssueReport) => void;
    invokeMock.mockImplementation((cmd: string, args: { refresh?: boolean }) => cmd === commands.getPlanningIssueReport
      ? args.refresh ? new Promise<PlanningIssueReport>(r => { resolve = r; }) : Promise.resolve(data)
      : Promise.resolve(cmd === commands.getAiAvailability));
    mount(); await screen.findByText(`日计划 · ${formatDate("2026-09-15")}`);
    fireEvent.click(screen.getByRole("button", { name: "AI 检查" }));
    expect(await screen.findByText("正在检查 2 项待办…")).toBeTruthy();
    expect(screen.queryByText("本次检查未发现需要处理的问题。")).toBeNull();
    data = report({ ai_status: "completed", checked_at: 1789459200000, checked_count: 2, model: "audit-model" });
    await act(async () => resolve(data));
    expect(await screen.findByText(/AI 已检查 2 项.*audit-model/)).toBeTruthy();
    expect(screen.getByText("本次检查未发现需要处理的问题。")).toBeTruthy();
    expect(screen.getByRole("button", { name: "重新检查" })).toBeTruthy();
  });
  it("never turns a provider failure into all-clear and retains earlier findings", async () => {
    const data = report({ issues: [finding] });
    invokeMock.mockImplementation((cmd: string, args: { refresh?: boolean }) => cmd === commands.getPlanningIssueReport
      ? args.refresh ? Promise.reject(new Error("Network unavailable")) : Promise.resolve(data)
      : Promise.resolve(true));
    mount(); await screen.findAllByText(finding.title);
    fireEvent.click(screen.getByRole("button", { name: "AI 检查" }));
    expect(await screen.findByText("AI 检查未完成")).toBeTruthy();
    expect(screen.getByText(/Network unavailable/)).toBeTruthy(); expect(screen.getAllByText(finding.title)[0]).toBeTruthy();
    expect(screen.queryByText("本次检查未发现需要处理的问题。")).toBeNull();
  });
  it("makes stale findings and pending 助理 previews explicit", async () => {
    mockBackend(report({ ai_status: "stale", pending_count: 1 })); mount();
    expect(await screen.findByText("计划或模型已变化，请重新运行 AI 检查。")).toBeTruthy();
    expect(screen.getByText(/1 项助理预览尚未确认/)).toBeTruthy();
  });
  it("rereads the locale-specific report without starting an AI check", async () => {
    mockBackend(report({ issues: [finding] })); mount();
    await screen.findAllByText(finding.title);
    invokeMock.mockClear();
    applyLocale("en");
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(commands.getPlanningIssueReport, { cycleId: "c1", refresh: false }));
    expect(invokeMock.mock.calls.some(([command, args]) => command === commands.getPlanningIssueReport && args?.refresh === true)).toBe(false);
  });
  it("locates the exact task and carries the diagnostic into a 助理 draft without sending", async () => {
    const locate = vi.fn(), discuss = vi.fn();
    mockBackend(report({ issues: [finding] })); mount({ onLocateTask: locate, onDiscuss: discuss });
    fireEvent.click(await screen.findByRole("button", { name: /定位任务/ }));
    expect(locate).toHaveBeenCalledWith("c1", "t1");
    fireEvent.click(screen.getByRole("button", { name: "与助理讨论" }));
    expect(discuss).toHaveBeenCalledWith("c1", expect.stringContaining("Prototype"), "t1");
    expect(discuss.mock.calls[0]![1]).toContain(finding.detail);
    expect(invokeMock.mock.calls.some(([cmd]) => cmd === commands.sendAgentMessage)).toBe(false);
  });
  it.each([
    ["t1", "不适用于我的情况", null],
    [null, "规划过程太费力", "Planning felt like too much work"],
  ])("keeps dismissal scope %s and reason", async (taskId, label, reason) => {
    mockBackend(report({ issues: [{ ...finding, task_id: taskId }] })); mount();
    const row = (await screen.findAllByText(finding.title))[0]!.closest("li")!;
    fireEvent.click(within(row).getByRole("button", { name: "忽略" }));
    fireEvent.click(await screen.findByRole("menuitem", { name: label! }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(commands.dismissPlanningIssue, {
      cycleId: "c1", issueType: finding.issue_type, taskId, reason,
    }));
  });
  it("an empty plan cannot run an unnecessary check", async () => {
    mockBackend(report({ ai_status: "empty", task_count: 0 })); mount();
    expect(await screen.findByText("当前没有待检查的任务。")).toBeTruthy();
    expect((screen.getByRole("button", { name: "AI 检查" }) as HTMLButtonElement).disabled).toBe(true);
  });
});

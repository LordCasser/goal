/**
 * ReviewPanel 行为契约（tasks §6.6）：空周期分支（无可复盘内容而非 0%）、
 * 部分保存后续填、草稿关闭不丢、去向按钮逐条调用、导出写入剪贴板。invoke
 * 按 src/lib/ipc.test.ts 的方式 mock——面板经由本特性的 api.ts 走真实包装。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { applyLocale } from "../../lib/i18n";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { commands, type CycleReviewView, type CycleReviewFacts } from "./api";
import { clearDraft } from "./draft";
import { ReviewPanel } from "./ReviewPanel";

const CYCLE = "c1";

function factsFixture(overrides: Partial<CycleReviewFacts> = {}): CycleReviewFacts {
  return {
    cycle_id: CYCLE,
    has_content: true,
    total_items: 2,
    completed_items: 1,
    completion_rate: 0.5,
    focused_time_ms: 3_600_000,
    linked_lower_items: 1,
    incomplete: [
      { task_id: "t1", title: "unfinished goal", cycle_id: CYCLE, cycle_type: "week" },
    ],
    ...overrides,
  };
}

function reviewFixture(overrides: Partial<CycleReviewView> = {}): CycleReviewView {
  return {
    id: "r1",
    cycle_id: CYCLE,
    kind: "full",
    is_final: false,
    facts: factsFixture(),
    answers: [{ id: "what_went_well", status: "answered", text: "steady week" }],
    dispositions: [],
    snapshot_at: 1_757_000_000_000,
    created_at: 1_757_000_000_000,
    updated_at: 1_757_000_000_000,
    ...overrides,
  };
}

/** invoke 的固定路由：复盘 + 实时事实（未复盘分支会取后者）。 */
function mockBackend(review: CycleReviewView | null, liveFacts: CycleReviewFacts): void {
  invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case commands.getCycleReview:
        return Promise.resolve(review);
      case commands.getCycleFacts:
        return Promise.resolve(liveFacts);
      case commands.saveCycleReview:
        return Promise.resolve(reviewFixture());
      case commands.applyReviewDisposition:
        return Promise.resolve(null);
      case commands.exportCycleReviewMarkdown:
        return Promise.resolve("# Cycle Review — test\n");
      default:
        return Promise.resolve(null);
    }
  });
}

function renderPanel(): ReturnType<typeof render> {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <ReviewPanel cycleId={CYCLE} onClose={() => undefined} />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  applyLocale("en");
  invokeMock.mockReset();
  clearDraft(CYCLE);
});

afterEach(() => {
  clearDraft(CYCLE);
});

describe("ReviewPanel", () => {
  it("shows the no-content branch instead of a misleading 0%", async () => {
    mockBackend(null, factsFixture({
      has_content: false,
      total_items: 0,
      completed_items: 0,
      completion_rate: null,
      focused_time_ms: 0,
      linked_lower_items: 0,
      incomplete: [],
    }));
    renderPanel();

    expect(
      await screen.findByText("This cycle has no reviewable content."),
    ).toBeTruthy();
    // 空周期不渲染完成率行，也没有 0%。
    expect(screen.queryByText("Completion")).toBeNull();
    expect(screen.queryByText(/0%/)).toBeNull();
    // 未复盘状态标注。
    expect(screen.getByText("Not reviewed yet · so far")).toBeTruthy();
  });

  it("shows the frozen snapshot facts and the snapshot time for a saved review", async () => {
    mockBackend(reviewFixture(), factsFixture());
    renderPanel();

    expect(await screen.findByText("50% (1/2)")).toBeTruthy();
    expect(screen.getByText("1 h")).toBeTruthy();
    expect(screen.getByText(/Snapshotted/)).toBeTruthy();
    expect(screen.getByText(/interim/)).toBeTruthy();
  });

  it("continues a partially saved review and submits both answers", async () => {
    mockBackend(reviewFixture(), factsFixture());
    renderPanel();

    const second = await screen.findByLabelText("What held you back?");
    fireEvent.change(second, { target: { value: "scope creep" } });
    fireEvent.click(screen.getByRole("button", { name: "Save review" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.saveCycleReview, {
        args: {
          cycle_id: CYCLE,
          answers: [
            { id: "what_went_well", status: "answered", text: "steady week" },
            { id: "what_held_you_back", status: "answered", text: "scope creep" },
          ],
        },
      }),
    );
    expect(await screen.findByText(/Saved\./)).toBeTruthy();
  });

  it("keeps typed answers across close and reopen", async () => {
    mockBackend(null, factsFixture());
    const first = renderPanel();
    const box = await screen.findByLabelText("Where did the plan and reality diverge?");
    fireEvent.change(box, { target: { value: "planned four, finished one" } });
    first.unmount();

    // 重新挂载（模拟关闭面板后重开）：草稿来自模块级 store。
    renderPanel();
    const reopened = await screen.findByLabelText(
      "Where did the plan and reality diverge?",
    ) as HTMLTextAreaElement;
    expect(reopened.value).toBe("planned four, finished one");
  });

  it("records a disposition per item and leaves undecided items unwritten", async () => {
    mockBackend(
      reviewFixture({
        facts: factsFixture({
          incomplete: [
            { task_id: "t1", title: "unfinished goal", cycle_id: CYCLE, cycle_type: "week" },
            { task_id: "t2", title: "other item", cycle_id: CYCLE, cycle_type: "week" },
          ],
        }),
      }),
      factsFixture(),
    );
    renderPanel();

    const row = (await screen.findByText("unfinished goal")).closest("li");
    expect(row).toBeTruthy();
    fireEvent.click(within(row as HTMLElement).getByRole("button", { name: "Carry" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.applyReviewDisposition, {
        cycleId: CYCLE,
        taskId: "t1",
        disposition: "carry",
      }),
    );
    expect(invokeMock).not.toHaveBeenCalledWith(commands.applyReviewDisposition, {
      cycleId: CYCLE,
      taskId: "t2",
      disposition: expect.anything(),
    });
  });

  it("copies the exported markdown to the clipboard and says so", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });
    mockBackend(reviewFixture(), factsFixture());
    renderPanel();

    // 等到已保存复盘加载完成（导出按钮在此之前是禁用的）。
    await screen.findByText("50% (1/2)");
    fireEvent.click(screen.getByRole("button", { name: "Copy Markdown" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("# Cycle Review — test\n"));
    expect(await screen.findByText("Markdown copied to clipboard")).toBeTruthy();
  });

  it("skips a question and marks it as skipped on save", async () => {
    mockBackend(reviewFixture({ answers: [] }), factsFixture());
    renderPanel();

    await screen.findByText("50% (1/2)");
    const question = screen
      .getByLabelText("What went well this cycle?")
      .closest("div");
    expect(question).toBeTruthy();
    fireEvent.click(within(question as HTMLElement).getByRole("button", { name: "Skip" }));
    expect(screen.getByText(/Will be saved as skipped/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Save review" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.saveCycleReview, {
        args: {
          cycle_id: CYCLE,
          answers: [{ id: "what_went_well", status: "skipped", text: "" }],
        },
      }),
    );
  });
});

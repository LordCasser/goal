/**
 * ReviewTrend 行为契约（spec 跨周期汇总）：按快照顺序展示；只有一份复盘时
 * 退化为单点展示并说明趋势需要更多数据，不渲染对比图；空历史给出开始引导。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { applyLocale } from "../../lib/i18n";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { commands, type ReviewSummaryPoint } from "./api";
import { ReviewTrend } from "./ReviewTrend";

function point(overrides: Partial<ReviewSummaryPoint>): ReviewSummaryPoint {
  return {
    cycle_id: "c1",
    cycle_title: "September cycle",
    cycle_type: "month",
    kind: "full",
    is_final: true,
    completion_rate: 0.75,
    focused_time_ms: 7_200_000,
    snapshot_at: 1_757_000_000_000,
    ...overrides,
  };
}

function renderTrend(): void {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <ReviewTrend />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  applyLocale("en");
  invokeMock.mockReset();
  invokeMock.mockReturnValue(Promise.resolve([]));
});

describe("ReviewTrend", () => {
  it("renders a single review as one point with a needs-more-data note", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getReviewSummary) {
        return Promise.resolve([point({})]);
      }
      return Promise.resolve(null);
    });
    renderTrend();

    expect(await screen.findByText("September cycle")).toBeTruthy();
    expect(screen.getByText("75%")).toBeTruthy();
    expect(screen.getByText("2 h")).toBeTruthy();
    expect(
      screen.getByText("Only one review so far — trends need more than one review to show."),
    ).toBeTruthy();
    // 单点：没有任何比例条（不画误导性对比图）。
    expect(screen.queryByRole("presentation")).toBeNull();
  });

  it("lists every snapshot in order once there is more than one", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getReviewSummary) {
        return Promise.resolve([
          point({}),
          point({
            cycle_id: "c2",
            cycle_title: "December cycle",
            completion_rate: 0.25,
            focused_time_ms: 1_800_000,
          }),
        ]);
      }
      return Promise.resolve(null);
    });
    renderTrend();

    expect(await screen.findByText("September cycle")).toBeTruthy();
    expect(screen.getByText("December cycle")).toBeTruthy();
    // 比例与专注时长合并在同一个读数里。
    expect(screen.getByText("75% · 2 h")).toBeTruthy();
    expect(screen.getByText("25% · 30 min")).toBeTruthy();
    expect(
      screen.queryByText(/trends need more than one review/),
    ).toBeNull();
  });

  it("guides the user when nothing has been reviewed yet", async () => {
    renderTrend();
    expect(
      await screen.findByText("No saved reviews yet. Review a cycle to start the trend."),
    ).toBeTruthy();
  });

  it("shows an empty-content snapshot as 'No content', never as a percent", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getReviewSummary) {
        return Promise.resolve([point({ completion_rate: null })]);
      }
      return Promise.resolve(null);
    });
    renderTrend();
    expect(await screen.findByText("No content")).toBeTruthy();
    expect(screen.queryByText(/%/)).toBeNull();
  });
});

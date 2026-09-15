/**
 * useCycleReviewStatus 契约（spec: 列头复盘入口 未复盘/已复盘两态）：加载中
 * 返回 "loading"（避免入口闪烁），已保存返回 "reviewed"，否则 "unreviewed"。
 */
import type * as React from "react";
import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { commands } from "./api";
import { useCycleReviewStatus } from "./useCycleReviewStatus";

function Harness({ cycleId }: { cycleId: string }): React.JSX.Element {
  const status = useCycleReviewStatus(cycleId);
  return <p data-testid="status">{status}</p>;
}

function renderHarness(cycleId: string): void {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <Harness cycleId={cycleId} />
    </QueryClientProvider>,
  );
}

const REVIEW = {
  id: "r1",
  cycle_id: "c1",
  kind: "full",
  is_final: true,
  facts: {},
  answers: [],
  dispositions: [],
  snapshot_at: 0,
  created_at: 0,
  updated_at: 0,
};

describe("useCycleReviewStatus", () => {
  it("reports reviewed when a saved review exists", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getCycleReview) return Promise.resolve(REVIEW);
      return Promise.resolve(null);
    });
    renderHarness("c1");
    await waitFor(() => expect(screen.getByTestId("status").textContent).toBe("reviewed"));
  });

  it("reports unreviewed when the cycle has no review", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getCycleReview) return Promise.resolve(null);
      return Promise.resolve(null);
    });
    renderHarness("c1");
    await waitFor(() => expect(screen.getByTestId("status").textContent).toBe("unreviewed"));
  });

  it("degrades to unreviewed when the read fails", async () => {
    invokeMock.mockImplementation(() => Promise.reject(new Error("boom")));
    renderHarness("c1");
    await waitFor(() => expect(screen.getByTestId("status").textContent).toBe("unreviewed"));
  });
});

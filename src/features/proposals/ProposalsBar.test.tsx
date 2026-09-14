/**
 * ProposalsBar 行为契约（rebuild-baseline 7.9）：跨周期汇总计数、全部为 0
 * 时渲染 null、Keep all / Revert all 作用于每个有 pending 的周期。invoke 按
 * src/lib/ipc.test.ts 的方式 mock——经由 lib/ipc 走真实包装。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import {
  commands,
  type Cycle,
  type PlannerState,
  type PreviewSummary,
} from "../../lib/ipc";
import { qk } from "../../lib/events";
import { ProposalsBar } from "./ProposalsBar";

type CycleFixture = { id: string; title: string; count: number };

function cycleFixture(id: string, title: string): Cycle {
  return {
    id,
    title,
    type: "month",
    parent_id: null,
    position: 0,
    archived: false,
    started: false,
    finished: false,
    started_at: null,
    finished_at: null,
    duration: null,
    focused_time: 0,
    starts_on: null,
    ends_on: null,
    calendar_key: null,
    repeat_id: null,
    created_at: 0,
  };
}

/** invoke 的固定路由：规划状态 + 每周期的 preview 计数。 */
function makeRouter(spec: CycleFixture[]) {
  const planner: PlannerState = {
    cycles: spec.map(({ id, title }) => cycleFixture(id, title)),
    later: cycleFixture("later", "Later"),
  };
  const counts = new Map(spec.map((entry) => [entry.id, entry.count]));
  return (cmd: string, args?: { cycle_id?: string }) => {
    switch (cmd) {
      case commands.getPlannerState:
        return Promise.resolve(planner);
      case commands.getPreviewSummary: {
        const cycleId = args?.cycle_id ?? "";
        const summary: PreviewSummary = {
          cycle_id: cycleId,
          count: counts.get(cycleId) ?? 0,
          tasks: [],
        };
        return Promise.resolve(summary);
      }
      default:
        return Promise.resolve(null);
    }
  };
}

function mockBackend(spec: CycleFixture[]): void {
  invokeMock.mockImplementation(makeRouter(spec));
}

function renderBar(): ReturnType<typeof render> & { client: QueryClient } {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const utils = render(
    <QueryClientProvider client={client}>
      <ProposalsBar />
    </QueryClientProvider>,
  );
  return { ...utils, client };
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("ProposalsBar", () => {
  it("sums pending counts across cycles", async () => {
    mockBackend([
      { id: "c1", title: "Focus", count: 1 },
      { id: "c2", title: "Health", count: 2 },
    ]);
    renderBar();
    expect(
      await screen.findByText("You have 3 pending edits by agent"),
    ).toBeTruthy();
  });

  it("names the single affected cycle", async () => {
    mockBackend([{ id: "c1", title: "Focus", count: 2 }]);
    renderBar();
    expect(
      await screen.findByText("You have 2 pending edits by agent in Focus"),
    ).toBeTruthy();
  });

  it("renders nothing when every cycle is clean", async () => {
    mockBackend([
      { id: "c1", title: "Focus", count: 0 },
      { id: "c2", title: "Health", count: 0 },
    ]);
    const { client, container } = renderBar();
    await waitFor(() => {
      expect(client.getQueryState(qk.previewSummary("c2"))?.status).toBe("success");
    });
    expect(container.firstChild).toBeNull();
  });

  it("keeps every pending cycle on Keep all", async () => {
    mockBackend([
      { id: "c1", title: "Focus", count: 1 },
      { id: "c2", title: "Health", count: 2 },
    ]);
    renderBar();
    fireEvent.click(await screen.findByRole("button", { name: "Keep all" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.keepAllPreviews, {
        cycle_id: "c1",
      }),
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.keepAllPreviews, {
        cycle_id: "c2",
      }),
    );
  });

  it("reverts every pending cycle on Revert all", async () => {
    mockBackend([
      { id: "c1", title: "Focus", count: 1 },
      { id: "c2", title: "Health", count: 2 },
    ]);
    renderBar();
    fireEvent.click(await screen.findByRole("button", { name: "Revert all" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.undoAllPreviews, {
        cycle_id: "c1",
      }),
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.undoAllPreviews, {
        cycle_id: "c2",
      }),
    );
  });

  it("disables both batch buttons while a batch action is in flight", async () => {
    const router = makeRouter([{ id: "c1", title: "Focus", count: 1 }]);
    invokeMock.mockImplementation((cmd: string, args?: { cycle_id?: string }) => {
      // keep_all_previews 永不落定：mutate 停在进行中来检验禁用态。
      if (cmd === commands.keepAllPreviews) {
        return new Promise<number>(() => undefined);
      }
      return router(cmd, args);
    });
    renderBar();
    fireEvent.click(await screen.findByRole("button", { name: "Keep all" }));

    await waitFor(() => {
      const keep = screen.getByRole("button", {
        name: "Keep all",
      }) as HTMLButtonElement;
      const revert = screen.getByRole("button", {
        name: "Revert all",
      }) as HTMLButtonElement;
      expect(keep.disabled).toBe(true);
      expect(revert.disabled).toBe(true);
    });
  });
});

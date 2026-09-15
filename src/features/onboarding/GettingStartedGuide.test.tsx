/**
 * GettingStartedGuide 行为契约（§1）：全新状态 "0 of 5"；进度用真实推导
 * 的计数；点开展开五步；跳过走 skip_getting_started_guide 且重启（skipped
 * 状态）后头部不再渲染；周期/任务变更事件触发 reconcile 重新核对。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { applyLocale } from "../../lib/i18n";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
const { listeners } = vi.hoisted(() => ({ listeners: {} as Record<string, (payload: unknown) => void> }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn((event: string, handler: (payload: unknown) => void) => {
    listeners[event] = handler;
    return Promise.resolve(() => delete listeners[event]);
  }),
}));

import { commands, type GettingStartedGuide as GuidePayload, type GuideStep } from "./api";
import { GettingStartedGuide } from "./GettingStartedGuide";

function guideFixture(over: Partial<GuidePayload> = {}): GuidePayload {
  const statuses = over.steps?.map((s) => s.status) ?? ["not_done", "not_done", "not_done", "not_done", "not_done"];
  const titles = [
    "Set a clear long-term goal",
    "Add something to Do Later",
    "Connect a weekly goal to the long-term goal",
    "Connect a daily task to the weekly goal",
    "Complete a 30-minute focus block",
  ];
  return {
    state: "active",
    completed: over.completed ?? 0,
    total: 5,
    steps: over.steps ?? titles.map((title, i): GuideStep => ({
      id: `step-${i}`,
      title,
      detail: `detail ${i}`,
      status: statuses[i] ?? "not_done",
    })),
    ...over,
  };
}

function renderGuide() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <GettingStartedGuide />
    </QueryClientProvider>,
  );
}

function backendWith(guide: GuidePayload) {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === commands.getOnboarding || cmd === commands.reconcileGettingStartedGuide) {
      return Promise.resolve(guide);
    }
    return Promise.resolve(null);
  });
}

beforeEach(() => {
  applyLocale("en");
  invokeMock.mockReset();
  delete listeners["cycles:changed"];
  delete listeners["tasks:changed"];
});

describe("GettingStartedGuide", () => {
  it("renders 0 of 5 on a fresh database", async () => {
    backendWith(guideFixture());
    renderGuide();
    expect(await screen.findByText("Getting started 0 of 5")).toBeTruthy();
  });

  it("counts only derived done steps in the header", async () => {
    const steps = guideFixture().steps.map((s, i) => ({
      ...s,
      status: i < 3 ? ("done" as const) : ("not_done" as const),
    }));
    backendWith(guideFixture({ completed: 3, steps }));
    renderGuide();
    expect(await screen.findByText("Getting started 3 of 5")).toBeTruthy();
  });

  it("expands the five steps with their statuses", async () => {
    const stepIds = [
      "set_long_term_goal",
      "add_later_item",
      "link_week_to_long_term",
      "link_day_to_week",
      "complete_focus_block",
    ];
    const steps = guideFixture().steps.map((s, i) => ({
      ...s,
      id: stepIds[i] ?? s.id,
      status: i === 0 ? ("done" as const) : i === 4 ? ("skipped" as const) : ("not_done" as const),
    }));
    backendWith(guideFixture({ completed: 1, steps }));
    renderGuide();
    fireEvent.click(await screen.findByRole("button", { name: /Getting started 1 of 5/ }));
    expect(screen.getByText("Set a clear long-term goal")).toBeTruthy();
    expect(screen.getByLabelText("Done")).toBeTruthy();
    expect(screen.getAllByLabelText("Not done")).toHaveLength(3);
    expect(screen.getByLabelText("Skipped")).toBeTruthy();
  });

  it("skips through skip_getting_started_guide and hides the header", async () => {
    let state = "active";
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getOnboarding || cmd === commands.reconcileGettingStartedGuide) {
        return Promise.resolve(guideFixture({ state: state as GuidePayload["state"] }));
      }
      if (cmd === commands.skipGettingStartedGuide) {
        state = "skipped";
        return Promise.resolve({ deleted_cycle_ids: [], deleted_task_count: 0 });
      }
      return Promise.resolve(null);
    });
    renderGuide();
    fireEvent.click(await screen.findByRole("button", { name: "Skip guide" }));
    await waitFor(() =>
      expect(screen.queryByLabelText("Getting started")).toBeNull(),
    );
    expect(invokeMock).toHaveBeenCalledWith(commands.skipGettingStartedGuide);
  });

  it("renders nothing when the guide was already skipped (restart)", () => {
    backendWith(guideFixture({ state: "skipped" }));
    renderGuide();
    expect(screen.queryByLabelText("Getting started")).toBeNull();
  });

  it("re-checks via reconcile when a cycle/task change event arrives", async () => {
    let fetches = 0;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getOnboarding || cmd === commands.reconcileGettingStartedGuide) {
        fetches += 1;
        return Promise.resolve(guideFixture());
      }
      return Promise.resolve(null);
    });
    renderGuide();
    await screen.findByText("Getting started 0 of 5");
    const before = fetches;
    listeners["cycles:changed"]?.({ payload: { cycle_ids: ["c1"] } });
    await waitFor(() => expect(fetches).toBeGreaterThan(before));
  });
});

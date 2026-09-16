/**
 * MissedSummary 行为契约（add-reminders-notifications §4.4 / §5.5）：
 * 无摘要不渲染；少量完整展示；多条只展开三条明细并可折叠；关闭时把
 * 全部 ids（含折叠的）一次性 acknowledge，之后不再出现。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { applyLocale } from "../../lib/i18n";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
const { listenMock } = vi.hoisted(() => ({ listenMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));

import { commands, type MissedSummary as MissedSummaryData } from "./api";
import { MissedSummary } from "./MissedSummary";

function summaryFixture(over: Partial<MissedSummaryData>): MissedSummaryData {
  return {
    total: 0,
    items: [],
    has_more: false,
    ids: [],
    ...over,
  };
}

function item(id: string, title: string) {
  return {
    id,
    target_kind: "task" as const,
    target_id: `target-${id}`,
    title,
    fire_at: new Date("2026-09-16T08:00").getTime(),
  };
}

function renderSummary(): QueryClient {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <MissedSummary />
    </QueryClientProvider>,
  );
  return client;
}

beforeEach(() => {
  applyLocale("zh-CN");
  invokeMock.mockReset();
  listenMock.mockReset();
  listenMock.mockResolvedValue(() => {});
});

describe("MissedSummary", () => {
  it("renders nothing when nothing was missed", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getMissedSummary) {
        return Promise.resolve(summaryFixture({ total: 0 }));
      }
      return Promise.resolve(null);
    });
    renderSummary();
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.getMissedSummary),
    );
    expect(screen.queryByLabelText("错过的提醒")).toBeNull();
  });

  it("shows the total and every detail when there are only a few", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getMissedSummary) {
        return Promise.resolve(
          summaryFixture({
            total: 2,
            items: [item("a", "One"), item("b", "Two")],
            has_more: false,
            ids: ["a", "b"],
          }),
        );
      }
      return Promise.resolve(null);
    });
    renderSummary();

    expect(await screen.findByText("您离开时有 2 条提醒到期")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "展开" }));
    expect(screen.getByText(/One/)).toBeTruthy();
    expect(screen.getByText(/Two/)).toBeTruthy();
  });

  it("caps details at three and folds the rest behind the count", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getMissedSummary) {
        return Promise.resolve(
          summaryFixture({
            total: 5,
            items: [item("a", "One"), item("b", "Two"), item("c", "Three")],
            has_more: true,
            ids: ["a", "b", "c", "d", "e"],
          }),
        );
      }
      return Promise.resolve(null);
    });
    renderSummary();

    expect(await screen.findByText("您离开时有 5 条提醒到期")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "展开" }));
    expect(screen.getByText(/Three/)).toBeTruthy();
    expect(screen.getByText(/以及其余 2 条/)).toBeTruthy();
    expect(screen.queryByText(/Four/)).toBeNull();
  });

  it("acknowledges every id on close and never shows again", async () => {
    const full = summaryFixture({
      total: 5,
      items: [item("a", "One"), item("b", "Two"), item("c", "Three")],
      has_more: true,
      ids: ["a", "b", "c", "d", "e"],
    });
    const empty = summaryFixture({ total: 0 });
    // acknowledge 之后收集命令改返回空摘要（对应后端 dismissed 标记）。
    let acknowledged = false;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getMissedSummary) {
        return Promise.resolve(acknowledged ? empty : full);
      }
      if (cmd === commands.acknowledgeMissedSummary) {
        acknowledged = true;
        return Promise.resolve(5);
      }
      return Promise.resolve(null);
    });
    renderSummary();

    fireEvent.click(await screen.findByRole("button", { name: "全部标为已读" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.acknowledgeMissedSummary, {
        ids: ["a", "b", "c", "d", "e"],
      }),
    );
    await waitFor(() =>
      expect(screen.queryByLabelText("错过的提醒")).toBeNull(),
    );
  });
});

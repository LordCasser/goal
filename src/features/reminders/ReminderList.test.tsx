/**
 * ReminderList 行为契约（add-reminders-notifications §5.6）：列表按触发时间
 * 排序；改时间后旧时间失效（走 update_reminder 而不是新建）；删除走
 * delete_reminder；fired-not-dismissed 的应用内提示可逐条关闭。
 * invoke 按 src/lib/ipc.test.ts 的方式 mock，钉住线上的参数键。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
const { listenMock } = vi.hoisted(() => ({ listenMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));

import {
  commands,
  formatFireAt,
  type Reminder,
  type ReminderStatusFilter,
} from "./api";
import { ReminderList } from "./ReminderList";

function reminderFixture(
  id: string,
  fire_at: number,
  over: Partial<Reminder> = {},
): Reminder {
  return {
    id,
    target_kind: "task",
    target_id: id,
    fire_at,
    quiet_ok: false,
    fired_at: null,
    dismissed_at: null,
    created_at: 0,
    ...over,
  };
}

const T1 = new Date("2026-09-16T09:00").getTime();
const T2 = new Date("2026-09-16T10:00").getTime();
const T3 = new Date("2026-09-16T11:00").getTime();

/** pending 与 fired 两个查询按 status 参数分发；pending 列表可中途切换。 */
function mockLists(
  initialPending: Reminder[],
  fired: Reminder[] = [],
): { setPending: (next: Reminder[]) => void } {
  let pending = initialPending;
  invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case commands.listReminders:
        return Promise.resolve(
          (args?.status as ReminderStatusFilter) === "fired" ? fired : pending,
        );
      case commands.reconcileReminders:
        return Promise.resolve(null);
      default:
        return Promise.resolve(null);
    }
  });
  return { setPending: (next) => (pending = next) };
}

function renderList(cycleId?: string): QueryClient {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <ReminderList cycleId={cycleId} />
    </QueryClientProvider>,
  );
  return client;
}

beforeEach(() => {
  invokeMock.mockReset();
  listenMock.mockReset();
  listenMock.mockResolvedValue(() => {});
});

describe("ReminderList", () => {
  it("renders pending reminders sorted by fire_at regardless of payload order", async () => {
    mockLists([
      reminderFixture("r-late", T3),
      reminderFixture("r-early", T1),
      reminderFixture("r-mid", T2),
    ]);
    renderList();

    await screen.findByLabelText("删除提醒 r-early");
    const order = screen
      .getAllByRole("listitem")
      .map((row) => row.textContent ?? "");
    const positions = ["r-early", "r-mid", "r-late"].map((id) =>
      order.findIndex((text) => text.includes(id)),
    );
    expect(positions).not.toContain(-1);
    expect(positions).toEqual([...positions].sort((a, b) => a - b));
  });

  it("shows the empty hint when nothing is pending", async () => {
    mockLists([]);
    renderList();
    expect(await screen.findByText("暂无提醒")).toBeTruthy();
  });

  it("reschedules via update_reminder and drops the old time from the list", async () => {
    const lists = mockLists([reminderFixture("r1", T1)]);
    const client = renderList();

    fireEvent.click(await screen.findByLabelText("修改提醒时间 r1"));
    fireEvent.change(screen.getByLabelText("提醒时间"), {
      target: { value: "2026-09-16T10:00" },
    });
    fireEvent.click(screen.getByRole("checkbox", { name: "免打扰时段内静音" }));
    fireEvent.click(screen.getByRole("button", { name: "保存提醒" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.updateReminder, {
        reminderId: "r1",
        args: { fire_at: T2, quiet_ok: true },
      }),
    );

    // 失效后重取：新时间出现，旧时间不再渲染（§5.6 改时间后旧时间失效）。
    lists.setPending([reminderFixture("r1", T2, { quiet_ok: true })]);
    await client.refetchQueries({ queryKey: ["reminders"] });
    expect(await screen.findByText(formatFireAt(T2))).toBeTruthy();
    expect(screen.queryByText(formatFireAt(T1))).toBeNull();
  });

  it("deletes a reminder through delete_reminder", async () => {
    mockLists([reminderFixture("r1", T1)]);
    renderList();
    fireEvent.click(await screen.findByLabelText("删除提醒 r1"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.deleteReminder, {
        reminderId: "r1",
      }),
    );
  });

  it("keeps fired-not-dismissed reminders visible and dismisses them in-app", async () => {
    mockLists([], [reminderFixture("r-fired", T1, { fired_at: T1 + 1 })]);
    renderList();

    expect(await screen.findByText("已到期，待查看")).toBeTruthy();
    fireEvent.click(screen.getByLabelText("知道了"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.acknowledgeMissedSummary, {
        ids: ["r-fired"],
      }),
    );
  });

  it("requests reconciliation on mount so drift never skips a reminder", async () => {
    mockLists([]);
    renderList();
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.reconcileReminders),
    );
  });
});

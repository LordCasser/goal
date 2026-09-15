/**
 * ReminderPicker 行为契约（add-reminders-notifications §5.1）：时间选择 +
 * 保存；创建走 set_reminder（重复设定由后端复用既有行），传入 reminderId
 * 走 update_reminder（改时间，旧时间失效）；无效时间禁用保存。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
const { listenMock } = vi.hoisted(() => ({ listenMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));

import { commands, type Reminder } from "./api";
import { ReminderPicker } from "./ReminderPicker";

function savedReminder(): Reminder {
  return {
    id: "r-new",
    target_kind: "task",
    target_id: "t1",
    fire_at: new Date("2026-09-16T09:30").getTime(),
    quiet_ok: false,
    fired_at: null,
    dismissed_at: null,
    created_at: 0,
  };
}

function renderPicker(over: Partial<Parameters<typeof ReminderPicker>[0]> = {}) {
  const onSaved = vi.fn();
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <ReminderPicker
        target_kind="task"
        target_id="t1"
        initialFireAt={new Date("2026-09-16T09:00").getTime()}
        onSaved={onSaved}
        {...over}
      />
    </QueryClientProvider>,
  );
  return onSaved;
}

beforeEach(() => {
  invokeMock.mockReset();
  listenMock.mockReset();
  listenMock.mockResolvedValue(() => {});
  invokeMock.mockResolvedValue(savedReminder());
});

describe("ReminderPicker", () => {
  it("creates through set_reminder with the chosen local time", async () => {
    const onSaved = renderPicker();
    fireEvent.change(screen.getByLabelText("提醒时间"), {
      target: { value: "2026-09-16T09:30" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存提醒" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.setReminder, {
        args: {
          target_kind: "task",
          target_id: "t1",
          fire_at: new Date("2026-09-16T09:30").getTime(),
          quiet_ok: false,
        },
      }),
    );
    await waitFor(() => expect(onSaved).toHaveBeenCalled());
  });

  it("marks the reminder quiet-ok when silence is requested", async () => {
    renderPicker();
    fireEvent.click(screen.getByRole("checkbox", { name: "免打扰时段内静音" }));
    fireEvent.click(screen.getByRole("button", { name: "保存提醒" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.setReminder, {
        args: expect.objectContaining({ quiet_ok: true }),
      }),
    );
  });

  it("updates the existing row instead of creating when reminderId is given", async () => {
    renderPicker({
      reminderId: "r-existing",
      initialQuietOk: true,
    });
    fireEvent.change(screen.getByLabelText("提醒时间"), {
      target: { value: "2026-09-17T18:00" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存提醒" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.updateReminder, {
        reminderId: "r-existing",
        args: {
          fire_at: new Date("2026-09-17T18:00").getTime(),
          quiet_ok: true,
        },
      }),
    );
  });

  it("disables saving while the time is invalid", () => {
    renderPicker();
    fireEvent.change(screen.getByLabelText("提醒时间"), { target: { value: "" } });
    const saveButton = screen.getByRole("button", {
      name: "保存提醒",
    }) as HTMLButtonElement;
    expect(saveButton.disabled).toBe(true);
  });
});

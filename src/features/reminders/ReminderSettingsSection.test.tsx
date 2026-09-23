/**
 * ReminderSettingsSection 行为契约（add-reminders-notifications §5.4）：
 * 设置保存走全量替换（quiet_hours + daily_plan_time 一起提交）；权限被拒
 * 时展示状态与恢复指引；请求权限后状态刷新；投递失败记录可见。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { applyLocale } from "../../lib/i18n";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
const { listenMock } = vi.hoisted(() => ({ listenMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));

import { commands, type DeliveryStatus, type ReminderSettings } from "./api";
import { ReminderSettingsSection } from "./ReminderSettingsSection";

const SETTINGS: ReminderSettings = {
  quiet_hours: { start: "23:00", end: "07:00" },
  daily_plan_time: "08:30",
};

function permissionDenied(): DeliveryStatus {
  return {
    permission: "denied",
    last_error: "notification_permission_denied",
    last_delivery_at: 1,
  };
}

function renderSection(): QueryClient {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <ReminderSettingsSection />
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

describe("ReminderSettingsSection", () => {
  it("saves quiet hours and daily plan time as one full-state write", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getReminderSettings) return Promise.resolve(SETTINGS);
      if (cmd === commands.getNotificationPermission) {
        return Promise.resolve<DeliveryStatus>({
          permission: "granted",
          last_error: null,
          last_delivery_at: null,
        });
      }
      return Promise.resolve(null);
    });
    renderSection();

    // 等本地编辑态与查询数据同步（免打扰输入框只在启用后出现）。
    const start = (await screen.findByLabelText("免打扰开始")) as HTMLInputElement;
    expect(start.value).toBe("23:00");
    fireEvent.click(screen.getByRole("button", { name: "保存提醒设置" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.setReminderSettings, {
        args: {
          quiet_hours: { start: "23:00", end: "07:00" },
          daily_plan_time: "08:30",
        },
      }),
    );
  });

  it("clears a disabled quiet window to null on save", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getReminderSettings) return Promise.resolve(SETTINGS);
      if (cmd === commands.getNotificationPermission) {
        return Promise.resolve<DeliveryStatus>({
          permission: "granted",
          last_error: null,
          last_delivery_at: null,
        });
      }
      return Promise.resolve(null);
    });
    renderSection();

    await screen.findByLabelText("免打扰开始");
    fireEvent.click(screen.getByRole("checkbox", { name: "免打扰时段" }));
    fireEvent.click(screen.getByRole("button", { name: "保存提醒设置" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.setReminderSettings, {
        args: { quiet_hours: null, daily_plan_time: "08:30" },
      }),
    );
  });

  it("does not save an invalid typed time", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getReminderSettings) return Promise.resolve(SETTINGS);
      if (cmd === commands.getNotificationPermission) return Promise.resolve({ permission: "granted", last_error: null, last_delivery_at: null });
      return Promise.resolve(null);
    });
    renderSection();
    const start = await screen.findByRole("textbox", { name: "免打扰开始" });
    fireEvent.change(start, { target: { value: "25:00" } });
    expect(start.getAttribute("aria-invalid")).toBe("true");
    expect((screen.getByRole("button", { name: "保存提醒设置" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("shows the denied permission state with recovery guidance", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getReminderSettings) return Promise.resolve(SETTINGS);
      if (cmd === commands.getNotificationPermission) {
        return Promise.resolve(permissionDenied());
      }
      return Promise.resolve(null);
    });
    renderSection();

    await screen.findByText(/通知权限：/);
    // 权限查询异步返回后，拒绝态与恢复指引出现（状态行与指引都含「被拒绝」）。
    await waitFor(() =>
      expect(screen.getAllByText(/被拒绝/).length).toBeGreaterThanOrEqual(2),
    );
    expect(screen.getByText(/系统设置 → 通知/)).toBeTruthy();
    expect(
      screen.getByText(/最近一次投递失败：notification_permission_denied/),
    ).toBeTruthy();
  });

  it("requests permission and reflects the refreshed state", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getReminderSettings) return Promise.resolve(SETTINGS);
      if (cmd === commands.getNotificationPermission) {
        return Promise.resolve<DeliveryStatus>({
          permission: "denied",
          last_error: null,
          last_delivery_at: null,
        });
      }
      if (cmd === commands.requestNotificationPermission) {
        return Promise.resolve("granted");
      }
      return Promise.resolve(null);
    });
    renderSection();

    fireEvent.click(
      await screen.findByRole("button", { name: "请求通知权限" }),
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        commands.requestNotificationPermission,
      ),
    );
  });

  it("offers the request button while permission is still only prompted", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getReminderSettings) return Promise.resolve(SETTINGS);
      if (cmd === commands.getNotificationPermission) {
        return Promise.resolve<DeliveryStatus>({
          permission: "prompt",
          last_error: null,
          last_delivery_at: null,
        });
      }
      return Promise.resolve(null);
    });
    renderSection();

    expect(
      await screen.findByRole("button", { name: "请求通知权限" }),
    ).toBeTruthy();
    expect(screen.queryByText(/被拒绝/)).toBeNull();
  });

  it("shows system-managed desktop permission without a fake request action", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getReminderSettings) return Promise.resolve(SETTINGS);
      if (cmd === commands.getNotificationPermission) {
        return Promise.resolve<DeliveryStatus>({
          permission: "system_managed",
          last_error: null,
          last_delivery_at: null,
        });
      }
      return Promise.resolve(null);
    });
    renderSection();

    expect(await screen.findByText(/由系统管理/)).toBeTruthy();
    expect(screen.getByText(/通知权限由操作系统管理/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "请求通知权限" })).toBeNull();
  });
});

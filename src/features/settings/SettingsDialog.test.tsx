/**
 * SettingsDialog 行为契约：主题点击立即翻 <html data-theme> 并落库
 * setTheme；周起始日与日志级别走各自 IPC；打开日志目录串联
 * getDebugLogDir → revealItemInDir；open=false 完全不渲染、不发 IPC。
 * IPC 与 opener 全部 mock；lib/theme 用真实现——applyTheme 在 jsdom 下
 * 直接操作 documentElement，可被断言。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

const mocks = vi.hoisted(() => ({
  getSettings: vi.fn(),
  setTheme: vi.fn(),
  setWeekStartDay: vi.fn(),
  setLogLevel: vi.fn(),
  getSchemaVersion: vi.fn(),
  getDebugLogDir: vi.fn(),
  revealItemInDir: vi.fn(),
}));

// 保留真实模块以复用 isAppError 等纯函数，只替换会触达 Tauri 的命令。
vi.mock("../../lib/ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/ipc")>()),
  getSettings: mocks.getSettings,
  setTheme: mocks.setTheme,
  setWeekStartDay: mocks.setWeekStartDay,
  setLogLevel: mocks.setLogLevel,
  getSchemaVersion: mocks.getSchemaVersion,
  getDebugLogDir: mocks.getDebugLogDir,
}));

vi.mock("@tauri-apps/plugin-opener", () => ({
  revealItemInDir: mocks.revealItemInDir,
}));

import { SettingsDialog } from "./SettingsDialog";

function renderDialog(open: boolean) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <SettingsDialog open={open} onClose={() => {}} />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.getSettings.mockResolvedValue({ week_start_day: 1, theme: "white" });
  mocks.setTheme.mockResolvedValue(undefined);
  mocks.setWeekStartDay.mockResolvedValue(undefined);
  mocks.setLogLevel.mockResolvedValue(undefined);
  mocks.getSchemaVersion.mockResolvedValue(3);
  mocks.getDebugLogDir.mockResolvedValue("/tmp/planner-logs");
  mocks.revealItemInDir.mockResolvedValue(undefined);
  delete document.documentElement.dataset.theme;
});

describe("SettingsDialog", () => {
  it("renders nothing and fires no IPC while closed", () => {
    const { container } = renderDialog(false);
    expect(container.firstElementChild).toBeNull();
    expect(mocks.getSettings).not.toHaveBeenCalled();
    expect(mocks.getSchemaVersion).not.toHaveBeenCalled();
  });

  it("applies and persists the gray theme on click", async () => {
    renderDialog(true);
    fireEvent.click(await screen.findByRole("button", { name: "灰底" }));
    // applyTheme 同步生效；setTheme 经 react-query 的异步 mutation 路径，
    // mutationFn 会附带收到第二个上下文参数（v5 行为）。
    expect(document.documentElement.dataset.theme).toBe("gray");
    await waitFor(() =>
      expect(mocks.setTheme).toHaveBeenCalledWith("gray", expect.anything()),
    );
  });

  it("keeps the current theme when the selected card is re-clicked", async () => {
    renderDialog(true);
    fireEvent.click(await screen.findByRole("button", { name: "白底" }));
    expect(mocks.setTheme).not.toHaveBeenCalled();
  });

  it("persists the week start day on change", async () => {
    renderDialog(true);
    fireEvent.change(await screen.findByLabelText("周起始日"), {
      target: { value: "7" },
    });
    await waitFor(() =>
      expect(mocks.setWeekStartDay).toHaveBeenCalledWith(7, expect.anything()),
    );
  });

  it("persists the log level on change", async () => {
    renderDialog(true);
    fireEvent.change(await screen.findByLabelText("日志级别"), {
      target: { value: "debug" },
    });
    await waitFor(() =>
      expect(mocks.setLogLevel).toHaveBeenCalledWith("debug", expect.anything()),
    );
  });

  it("reveals the debug log directory and shows its path", async () => {
    renderDialog(true);
    fireEvent.click(
      await screen.findByRole("button", { name: "打开日志目录" }),
    );
    await waitFor(() => {
      expect(mocks.revealItemInDir).toHaveBeenCalledWith("/tmp/planner-logs");
    });
    expect(mocks.getDebugLogDir).toHaveBeenCalled();
    expect(await screen.findByText("/tmp/planner-logs")).toBeTruthy();
  });

  it("shows the schema version", async () => {
    renderDialog(true);
    expect(await screen.findByText("Schema v3")).toBeTruthy();
  });
});

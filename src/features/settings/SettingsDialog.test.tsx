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
import { applyLocale } from "../../lib/i18n";
import { qk } from "../../lib/events";

const mocks = vi.hoisted(() => ({
  getSettings: vi.fn(),
  setLocale: vi.fn(),
  setShowRelationLines: vi.fn(),
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
  setLocale: mocks.setLocale,
  setShowRelationLines: mocks.setShowRelationLines,
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
  const view = render(
    <QueryClientProvider client={client}>
      <SettingsDialog open={open} onClose={() => {}} />
    </QueryClientProvider>,
  );
  return { ...view, client };
}

beforeEach(() => {
  applyLocale("zh-CN");
  vi.clearAllMocks();
  mocks.getSettings.mockResolvedValue({ locale: "zh-CN", week_start_day: 1, theme: "white", show_relation_lines: false });
  mocks.setLocale.mockResolvedValue(undefined);
  mocks.setShowRelationLines.mockResolvedValue(undefined);
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
    const gray = await screen.findByRole("button", { name: "灰底" });
    await waitFor(() => expect((gray as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(gray);
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
    const weekStart = await screen.findByRole("combobox", { name: "周起始日" });
    await waitFor(() => expect((weekStart as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(weekStart);
    fireEvent.click(await screen.findByRole("option", { name: "周日" }));
    await waitFor(() =>
      expect(mocks.setWeekStartDay).toHaveBeenCalledWith(7, expect.anything()),
    );
  });

  it("optimistically toggles relation lines and rolls back when saving fails", async () => {
    let rejectSave!: (reason: unknown) => void;
    mocks.setShowRelationLines.mockReturnValueOnce(new Promise<void>((_resolve, reject) => {
      rejectSave = reject;
    }));
    const { client } = renderDialog(true);
    const toggle = await screen.findByRole("checkbox", { name: "显示关联连线" });
    await waitFor(() => expect((toggle as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(toggle);
    await waitFor(() => expect(toggle.getAttribute("aria-checked")).toBe("true"));
    expect(client.getQueryData<{ show_relation_lines: boolean }>(qk.settings())?.show_relation_lines).toBe(true);
    rejectSave({ code: "db_error", message: "failed" });
    await waitFor(() => expect(toggle.getAttribute("aria-checked")).toBe("false"));
    expect(client.getQueryData<{ show_relation_lines: boolean }>(qk.settings())?.show_relation_lines).toBe(false);
    expect(mocks.setShowRelationLines).toHaveBeenCalledWith(true, expect.anything());
  });

  it("applies the selected language only after the setting is saved", async () => {
    let resolveSave!: () => void;
    mocks.setLocale.mockReturnValue(new Promise<void>((resolve) => { resolveSave = resolve; }));
    const { client } = renderDialog(true);
    const language = await screen.findByRole("combobox", { name: "语言" }) as HTMLButtonElement;
    await waitFor(() => expect(language.disabled).toBe(false));
    fireEvent.click(language);
    fireEvent.click(await screen.findByRole("option", { name: "English" }));
    await waitFor(() => expect(language.disabled).toBe(true));
    expect(document.documentElement.lang).toBe("zh-CN");
    // App subscribes to this cache too; publishing an optimistic language
    // would change the mounted application even while the save is pending.
    expect(client.getQueryData<{ locale: string }>(qk.settings())?.locale).toBe("zh-CN");
    mocks.getSettings.mockResolvedValue({ locale: "en", week_start_day: 1, theme: "white", show_relation_lines: false });
    resolveSave();
    await waitFor(() => expect(document.documentElement.lang).toBe("en"));
    expect(mocks.setLocale).toHaveBeenCalledWith("en");
  });

  it("rolls the language select back and shows a localized error when saving fails", async () => {
    mocks.setLocale.mockRejectedValue({ code: "invalid_locale", message: "invalid" });
    renderDialog(true);
    const language = await screen.findByRole("combobox", { name: "语言" });
    await waitFor(() => expect((language as HTMLButtonElement).disabled).toBe(false));
    fireEvent.click(language);
    fireEvent.click(await screen.findByRole("option", { name: "English" }));
    await waitFor(() => expect(language.textContent).toContain("简体中文"));
    expect(await screen.findByText("请选择简体中文或 English。"));
  });

  it("persists the log level on change", async () => {
    renderDialog(true);
    fireEvent.click(screen.getByRole("button", { name: /^诊断/ }));
    fireEvent.click(await screen.findByRole("combobox", { name: "日志级别" }));
    fireEvent.click(await screen.findByRole("option", { name: "debug" }));
    await waitFor(() =>
      expect(mocks.setLogLevel).toHaveBeenCalledWith("debug", expect.anything()),
    );
  });

  it("reveals the debug log directory and shows its path", async () => {
    renderDialog(true);
    fireEvent.click(screen.getByRole("button", { name: /^诊断/ }));
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
    fireEvent.click(screen.getByRole("button", { name: /^诊断/ }));
    expect(await screen.findByText("数据版本 v3")).toBeTruthy();
  });
});

/**
 * AiSettingsPage 行为契约（tasks §5.8）：
 * - 空态：无供应商时左栏给出空提示与添加入口；
 * - 校验禁用：必填项缺失时「保存供应商」禁用并行列出缺什么；
 * - Key 不回显：编辑已配置 Key 的供应商时密码框为空、只显示「已配置」；
 * - 删除确认：文案包含供应商名与钥匙串一并清除的说明；
 * - 连接测试：成功显示延迟，失败按 error_code（design D3）映射文案；
 * - 保存链路：saveProvider → saveProviderApiKey，之后清空 Key 草稿。
 *
 * ../../lib/ipc 全部 mock（保留 isAppError 等纯函数）；react-query 用真实
 * 实现，走真实的失效重取路径。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

const mocks = vi.hoisted(() => ({
  getAiSettings: vi.fn(),
  saveProvider: vi.fn(),
  saveProviderApiKey: vi.fn(),
  removeProviderApiKey: vi.fn(),
  deleteProvider: vi.fn(),
  setActiveProvider: vi.fn(),
  testProviderConnection: vi.fn(),
}));

// 保留真实模块以复用类型与 isAppError；只替换会触达 Tauri 的命令。
vi.mock("../../lib/ipc", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../lib/ipc")>()),
  getAiSettings: mocks.getAiSettings,
  saveProvider: mocks.saveProvider,
  saveProviderApiKey: mocks.saveProviderApiKey,
  removeProviderApiKey: mocks.removeProviderApiKey,
  deleteProvider: mocks.deleteProvider,
  setActiveProvider: mocks.setActiveProvider,
  testProviderConnection: mocks.testProviderConnection,
}));

import { AiSettingsPage } from "./AiSettingsPage";
import type {
  AiSettingsSummary,
  ConnectionTestResult,
  ModelConfig,
  ProviderConfig,
  ProviderSummary,
} from "../../lib/ipc";

const model: ModelConfig = {
  model_id: "llama3",
  context_window: 8192,
  max_output_tokens: 2048,
  input_types: ["text"],
  output_types: ["text"],
  supports_tools: true,
};

const provider: ProviderSummary = {
  id: "p1",
  name: "Local runtime",
  base_url: "http://127.0.0.1:11434/v1",
  api_format: "openai_chat_completions",
  extra_headers: [],
  models: [model],
  created_at: 1,
  archived: false,
  has_api_key: true,
  is_active: true,
};

const emptySummary: AiSettingsSummary = {
  active_provider: null,
  providers: [],
  ai_available: false,
};

const oneSummary: AiSettingsSummary = {
  active_provider: provider,
  providers: [provider],
  ai_available: true,
};

function renderPage() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <AiSettingsPage />
    </QueryClientProvider>,
  );
}

const asInput = (element: HTMLElement): HTMLInputElement =>
  element as HTMLInputElement;
const asButton = (element: HTMLElement): HTMLButtonElement =>
  element as HTMLButtonElement;

beforeEach(() => {
  vi.clearAllMocks();
  mocks.getAiSettings.mockResolvedValue(emptySummary);
  mocks.saveProvider.mockResolvedValue(undefined);
  mocks.saveProviderApiKey.mockResolvedValue(undefined);
  mocks.removeProviderApiKey.mockResolvedValue(undefined);
  mocks.deleteProvider.mockResolvedValue(undefined);
  mocks.setActiveProvider.mockResolvedValue(undefined);
  mocks.testProviderConnection.mockResolvedValue({
    ok: true,
    latency_ms: 1,
    error_code: null,
    error_message: null,
  } satisfies ConnectionTestResult);
});

describe("AiSettingsPage", () => {
  it("empty state: left column shows the empty hint and the add entry", async () => {
    renderPage();
    expect(await screen.findByText("暂无供应商")).toBeTruthy();
    expect(screen.getByRole("button", { name: "+ 添加供应商" })).toBeTruthy();
    // ai_available=false：说明行引导先添加并激活供应商。
    expect(screen.getByText(/先添加并激活供应商/)).toBeTruthy();
  });

  it("keeps the save button disabled and lists what is missing", async () => {
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "+ 添加供应商" }));
    const save = await screen.findByRole("button", { name: "保存供应商" });
    expect(asButton(save).disabled).toBe(true);
    expect(screen.getByText(/还需完善：名称/)).toBeTruthy();
    expect(mocks.saveProvider).not.toHaveBeenCalled();
  });

  it("never echoes the stored key: password field starts empty with an 已配置 badge", async () => {
    mocks.getAiSettings.mockResolvedValue(oneSummary);
    renderPage();
    const input = await screen.findByLabelText("API Key");
    expect(asInput(input).value).toBe("");
    expect(screen.getByText("已配置")).toBeTruthy();
    // 列表与说明行同时给出激活状态。
    expect(screen.getByText("当前激活：Local runtime")).toBeTruthy();
  });

  it("delete confirmation names the provider and the keychain cleanup", async () => {
    mocks.getAiSettings.mockResolvedValue(oneSummary);
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "删除" }));
    const dialog = await screen.findByRole("dialog");
    expect(dialog.textContent).toContain("Local runtime");
    expect(dialog.textContent).toContain("钥匙串中的 API Key 将一并清除");
  });

  it("reports connection success with the measured latency", async () => {
    mocks.getAiSettings.mockResolvedValue(oneSummary);
    mocks.testProviderConnection.mockResolvedValue({
      ok: true,
      latency_ms: 42,
      error_code: null,
      error_message: null,
    } satisfies ConnectionTestResult);
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Test connection" }));
    expect(await screen.findByText("Connected in 42ms")).toBeTruthy();
    expect(mocks.testProviderConnection).toHaveBeenCalledWith("p1");
  });

  it("maps auth_failed to a check-your-key hint", async () => {
    mocks.getAiSettings.mockResolvedValue(oneSummary);
    mocks.testProviderConnection.mockResolvedValue({
      ok: false,
      latency_ms: null,
      error_code: "auth_failed",
      error_message: "authentication failed",
    } satisfies ConnectionTestResult);
    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "Test connection" }));
    expect(await screen.findByText(/检查 API Key/)).toBeTruthy();
  });

  it("saves the provider then the key, and clears the key draft afterwards", async () => {
    const savedConfig: ProviderConfig = {
      id: "p2",
      name: "New provider",
      base_url: "https://api.example.com/v1",
      api_format: "openai_chat_completions",
      extra_headers: [],
      models: [model],
      created_at: 9,
      archived: false,
    };
    const savedSummary: AiSettingsSummary = {
      active_provider: null,
      providers: [
        {
          ...savedConfig,
          has_api_key: true,
          is_active: false,
        },
      ],
      ai_available: false,
    };
    mocks.getAiSettings
      .mockResolvedValueOnce(emptySummary) // 初次加载：空
      .mockResolvedValueOnce(savedSummary); // 保存后的失效重取
    mocks.saveProvider.mockResolvedValue(savedConfig);

    renderPage();
    fireEvent.click(await screen.findByRole("button", { name: "+ 添加供应商" }));
    fireEvent.change(await screen.findByLabelText("名称"), {
      target: { value: "New provider" },
    });
    fireEvent.change(screen.getByLabelText("Base URL"), {
      target: { value: "https://api.example.com/v1" },
    });
    fireEvent.change(screen.getByLabelText("API Key"), {
      target: { value: "sk-secret" },
    });

    // 添加模型：填满对话框的三个必填项后保存。
    fireEvent.click(screen.getByRole("button", { name: "添加模型" }));
    fireEvent.change(await screen.findByLabelText("模型 ID"), {
      target: { value: "llama3" },
    });
    fireEvent.change(screen.getByLabelText("上下文窗口"), {
      target: { value: "8192" },
    });
    fireEvent.change(screen.getByLabelText("最大输出 Token"), {
      target: { value: "2048" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    // 模型进列表后主按钮解禁。
    const save = screen.getByRole("button", { name: "保存供应商" });
    await waitFor(() => expect(asButton(save).disabled).toBe(false));
    fireEvent.click(save);

    await waitFor(() => expect(mocks.saveProvider).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(mocks.saveProviderApiKey).toHaveBeenCalledWith("p2", "sk-secret"),
    );
    // 保存成功后 Key 草稿清空、徽标随失效重取出现（不回显明文）。
    await waitFor(() =>
      expect(asInput(screen.getByLabelText("API Key")).value).toBe(""),
    );
    expect(await screen.findByText("已配置")).toBeTruthy();
  });
});

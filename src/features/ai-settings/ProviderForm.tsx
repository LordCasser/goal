/**
 * 供应商添加/编辑表单与详情操作（tasks §5.2–§5.5，design D6）。
 *
 * - 草稿只随 `key={provider.id}` 挂载初始化一次；查询失效刷新列表与
 *   徽标，但不覆盖未提交的表单草稿（design.md §8.3/§11）。
 * - API Key 只进不出：输入框永不回显（服务端也从不返回 Key），已有 Key
 *   仅以「已配置」徽标 + 移除入口表达（design.md §9.3：密钥不出现在普通
 *   状态）。本地端点留空 Key 是正常状态，不显示警告（design D4）。
 * - 保存 = saveProvider（配置文件）+ 可选 saveProviderApiKey（钥匙串）；
 *   后端校验错误（AppError code）行内显示（task 5.3）。
 * - 操作（设为激活 / 删除 / 连接测试）只对已保存供应商开放；连接测试
 *   结果按 design D3 的 error_code 映射文案，行内反馈（task 5.4）。
 * - 所有写操作成功后经 onChanged 请求页面失效 ai-settings 查询；查询
 *   key 由页面持有（见 AiSettingsPage.tsx 文件头注释）。
 */
import { useId, useState, type JSX } from "react";
import { useMutation } from "@tanstack/react-query";

import { Button, Dialog, Input, ProgressDot, cn } from "../../ui";
import {
  deleteProvider,
  isAppError,
  removeProviderApiKey,
  saveProvider,
  testProviderConnection,
} from "../../lib/ipc";
import type {
  ApiFormat,
  ConnectionTestResult,
  ModelConfig,
  ProviderConfig,
  ProviderSummary,
} from "../../lib/ipc";
import { AddModelDialog } from "./AddModelDialog";

export type ProviderFormProps = {
  /** null = 新增（id 由后端分配）；否则为正在编辑的已保存供应商。 */
  provider: ProviderSummary | null;
  /** 保存成功后回调（新供应商把选中切到刚创建的 id）。 */
  onSaved: (providerId: string) => void;
  /** 任何写操作成功后的失效通知；ai-settings 查询 key 由页面持有。 */
  onChanged: () => Promise<void> | void;
};

/* 原生 select 复用 Input 的表面样式（design.md 4.1/4.3）；箭头留给平台。 */
const SELECT_CLASS =
  "h-9 w-full min-w-0 rounded-md border border-control bg-content px-3 text-[14px] text-primary transition-colors duration-100";

/** 三种 API 格式 + 各自的请求端点（design D2），下拉项直接显示端点路径。 */
const API_FORMATS: ReadonlyArray<{ value: ApiFormat; label: string }> = [
  { value: "anthropic_messages", label: "Anthropic Messages" },
  {
    value: "openai_chat_completions",
    label: "OpenAI Chat Completions",
  },
  { value: "openai_responses", label: "OpenAI Responses" },
];

/** 镜像 providers::config::is_http_base_url：http(s) scheme + 非空主机，本地 http 合法。 */
function isHttpBaseUrl(url: string): boolean {
  if (/\s/.test(url)) return false;
  const separator = url.indexOf("://");
  if (separator < 0) return false;
  const scheme = url.slice(0, separator);
  const rest = url.slice(separator + 3);
  return (scheme === "http" || scheme === "https") && rest !== "";
}

/** 连接测试失败文案：按 design D3 的 error_code 映射（task 5.4）。 */
function connectionFailureText(result: ConnectionTestResult): string {
  switch (result.error_code) {
    case "auth_failed":
      return "认证失败：请检查 API Key。";
    case "provider_unreachable":
    case "timeout":
      return "无法连接到端点：请检查 Base URL 与网络连接。";
    case "invalid_request":
      return `请求被拒绝：${result.error_message ?? ""}`;
    case "rate_limited":
      return "请求被限流：请稍后重试。";
    default:
      return result.error_message ?? result.error_code ?? "连接失败。";
  }
}

/** AppError 保留稳定 code（如 invalid_base_url），与 message 一起行内展示。 */
function errorText(error: unknown): string {
  if (isAppError(error)) return `${error.code}：${error.message}`;
  return error instanceof Error ? error.message : String(error);
}

export function ProviderForm({
  provider,
  onSaved,
  onChanged,
}: ProviderFormProps): JSX.Element {
  const nameInputId = useId();
  const urlInputId = useId();
  const keyInputId = useId();
  const formatInputId = useId();

  const providerId = provider?.id ?? null;

  const [name, setName] = useState(provider?.name ?? "");
  const [baseUrl, setBaseUrl] = useState(provider?.base_url ?? "");
  const [apiFormat, setApiFormat] = useState<ApiFormat>(
    provider?.api_format ?? "openai_chat_completions",
  );
  const [apiKey, setApiKey] = useState("");
  const [models, setModels] = useState<ModelConfig[]>(provider?.models ?? []);
  const [modelDialogOpen, setModelDialogOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [testResult, setTestResult] = useState<ConnectionTestResult | null>(null);

  // 前端预校验（task 5.3）：名称非空、http(s) URL、至少一个模型。
  const missing: string[] = [];
  if (name.trim() === "") missing.push("名称");
  if (!isHttpBaseUrl(baseUrl.trim())) missing.push("Base URL（http/https 地址）");
  if (models.length === 0) missing.push("至少一个模型");
  const canSave = missing.length === 0;

  const requireProviderId = (): string => {
    if (!providerId) throw new Error("provider is not saved yet");
    return providerId;
  };

  const saveMutation = useMutation({
    mutationFn: async (): Promise<ProviderConfig> => {
      const config: ProviderConfig = {
        id: provider?.id ?? "",
        name: name.trim(),
        base_url: baseUrl.trim(),
        api_format: apiFormat,
        // 表单不编辑 extra_headers/created_at/archived；编辑时原样保留。
        extra_headers: provider?.extra_headers ?? [],
        models,
        created_at: provider?.created_at ?? 0,
        archived: provider?.archived ?? false,
        connection_verified_at: null,
      };
      return saveProvider(config, apiKey.trim() || null);
    },
    onSuccess: async (saved) => {
      setName(saved.name);
      setBaseUrl(saved.base_url);
      setApiKey(""); // 保存成功即清空 Key 草稿：不回显（design §9.3）
      setTestResult(null);
      await onChanged(); // 等失效重取完成后再切换选中，避免右栏闪空态
      onSaved(saved.id);
    },
  });

  const removeKeyMutation = useMutation({
    mutationFn: () => removeProviderApiKey(requireProviderId()),
    onSuccess: () => void onChanged(),
  });

  const deleteMutation = useMutation({
    mutationFn: () => deleteProvider(requireProviderId()),
    onSuccess: async () => {
      setDeleteOpen(false);
      await onChanged(); // 选中项的自愈回落由页面根据新数据完成
    },
  });

  const testMutation = useMutation({
    mutationFn: () => testProviderConnection(requireProviderId()),
    onMutate: () => setTestResult(null),
    onSuccess: async (result) => { setTestResult(result); await onChanged(); },
    onError: () => void onChanged(),
  });

  const busy = saveMutation.isPending || testMutation.isPending || removeKeyMutation.isPending;
  const dirty = name !== (provider?.name ?? "") || baseUrl !== (provider?.base_url ?? "") || apiFormat !== provider?.api_format
    || apiKey !== "" || JSON.stringify(models) !== JSON.stringify(provider?.models ?? []);

  return (
    <div className="flex min-h-0 flex-col">
      {/* 详情头部（task 5.4/5.5）：操作只对已保存供应商开放。 */}
      {provider ? (
        <>
          <div className="flex flex-wrap items-center justify-between gap-2 border-b border-light px-4 py-2.5">
            <div className="flex min-w-0 items-center gap-2">
              <ProgressDot tone={provider.is_active ? "active" : "idle"} />
              <h4 className="truncate text-block-title font-semibold text-primary">
                {provider.name}
              </h4>
              <span className="shrink-0 text-caption text-secondary">
                {provider.is_active ? "激活" : "未激活"}
              </span>
              <span className="text-caption text-hint">{provider.connection_verified_at ? "已测通" : "待测试"}</span>
            </div>
            <div className="flex items-center gap-2">

              <Button
                variant="secondary"
                size="compact"
                onClick={() => testMutation.mutate()}
                loading={testMutation.isPending}
                disabled={busy || dirty}
                title={dirty ? "有未保存修改，请使用下方的测试并保存" : "测试已保存配置"}
              >
                测试连接
              </Button>
              <Button
                variant="ghost"
                size="compact"
                className="text-danger"
                onClick={() => setDeleteOpen(true)}
                disabled={busy}
              >
                删除
              </Button>
            </div>
          </div>
          {testResult && (
            <p
              className={cn(
                "flex items-center gap-1.5 px-4 pt-2 text-caption",
                testResult.ok ? "text-secondary" : "text-danger",
              )}
            >
              <ProgressDot tone={testResult.ok ? "done" : "alert"} />
              {testResult.ok
                ? `连接成功 · ${testResult.latency_ms ?? 0} ms`
                : connectionFailureText(testResult)}
            </p>
          )}
          {testMutation.isError && (
            <p className="px-4 pt-2 text-caption text-danger">
              {errorText(testMutation.error)}
            </p>
          )}
        </>
      ) : (
        <div className="border-b border-light px-4 py-2.5">
          <h4 className="text-block-title font-semibold text-primary">添加供应商</h4>
        </div>
      )}

      <fieldset disabled={busy} className="flex min-w-0 flex-col gap-4 px-4 py-4">
        <div className="flex flex-col gap-1">
          <label htmlFor={nameInputId} className="text-caption text-secondary">
            名称
          </label>
          <Input
            id={nameInputId}
            value={name}
            onChange={(e) => setName(e.currentTarget.value)}
            placeholder="如 Ollama 本地、OpenRouter"
          />
        </div>
        <div className="flex flex-col gap-1">
          <label htmlFor={urlInputId} className="text-caption text-secondary">
            Base URL
          </label>
          <Input
            id={urlInputId}
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.currentTarget.value)}
            placeholder="https://api.example.com/v1 或 http://127.0.0.1:11434/v1"
          />
        </div>
        <div className="flex flex-col gap-1">
          <label htmlFor={keyInputId} className="text-caption text-secondary">
            API Key
          </label>
          {/* 已有 Key：只显示「已配置」徽标 + 移除入口；明文既不回显也不截尾。 */}
          {provider?.has_api_key && (
            <div className="flex items-center gap-2">
              <span className="inline-flex items-center rounded-[2px] border border-light bg-subtle px-1.5 py-0.5 text-caption text-secondary">
                已配置
              </span>
              <Button
                variant="ghost"
                size="compact"
                className="text-danger"
                onClick={() => removeKeyMutation.mutate()}
                loading={removeKeyMutation.isPending}
              >
                移除
              </Button>
              {removeKeyMutation.isError && (
                <span className="text-caption text-danger">
                  {errorText(removeKeyMutation.error)}
                </span>
              )}
            </div>
          )}
          <Input
            id={keyInputId}
            type="password"
            autoComplete="new-password"
            value={apiKey}
            onChange={(e) => setApiKey(e.currentTarget.value)}
            placeholder={
              provider?.has_api_key
                ? "输入新 Key 以替换；留空保持不变"
                : "sk-…（本地端点可留空）"
            }
          />
          <p className="text-caption text-hint">密钥保存在系统钥匙串，不写入配置文件。</p>
        </div>
        <div className="flex flex-col gap-1">
          <label htmlFor={formatInputId} className="text-caption text-secondary">
            API 格式
          </label>
          <select
            id={formatInputId}
            value={apiFormat}
            onChange={(e) => setApiFormat(e.currentTarget.value as ApiFormat)}
            className={SELECT_CLASS}
          >
            {API_FORMATS.map((format) => (
              <option key={format.value} value={format.value}>
                {format.label}
              </option>
            ))}
          </select>
        </div>

        {/* 模型列表（task 5.2）：model_id + 窗口 + 最大输出 + 工具调用标记。 */}
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-2">
            <span className="text-block-title font-semibold text-primary">模型</span>
            <Button size="compact" onClick={() => setModelDialogOpen(true)}>
              添加模型
            </Button>
          </div>
          {models.length === 0 ? (
            <p className="text-caption text-hint">
              尚未添加模型；每个供应商至少需要一个模型。
            </p>
          ) : (
            <ul className="border border-light">
              {models.map((model, index) => (
                <li
                  key={`${model.model_id}-${index}`}
                  className={cn(
                    "flex items-center justify-between gap-2 px-2 py-1.5",
                    index > 0 && "border-t border-light",
                  )}
                >
                  <div className="flex min-w-0 flex-col">
                    <span className="truncate text-body text-primary">{model.model_id}</span>
                    <span className="text-caption text-secondary">
                      上下文 {model.context_window} · 最大输出 {model.max_output_tokens}
                      {model.supports_tools ? " · 支持工具调用" : " · 不支持工具调用"}
                    </span>
                  </div>
                  <button
                    type="button"
                    aria-label={`移除模型 ${model.model_id}`}
                    onClick={() =>
                      setModels((previous) => previous.filter((_, i) => i !== index))
                    }
                    className={cn(
                      "flex h-7 w-7 shrink-0 items-center justify-center rounded-sm",
                      "text-secondary transition-colors duration-100 hover:bg-hover hover:text-primary",
                    )}
                  >
                    <svg
                      viewBox="0 0 16 16"
                      className="h-3.5 w-3.5"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="1.5"
                      strokeLinecap="round"
                      aria-hidden="true"
                    >
                      <path d="M4 4l8 8M12 4l-8 8" />
                    </svg>
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
      </fieldset>

      {/* 底部：行内校验提示（列出缺什么）+ 主按钮（task 5.3）。 */}
      <div className="mt-auto flex flex-wrap items-center justify-between gap-2 border-t border-light px-4 py-3">
        <div className="flex min-w-0 flex-col">
          {!canSave && <p className="text-caption text-hint">还需完善：{missing.join("、")}</p>}
          {canSave && <p className="text-caption text-hint">{saveMutation.isPending ? "正在测试模型，成功后保存…" : "测试通过后才会保存；失败时保留当前配置。"}</p>}
          {saveMutation.isError && (
            <p className="text-caption text-danger">{errorText(saveMutation.error)}</p>
          )}
        </div>
        <Button
          variant="primary"
          disabled={!canSave || busy}
          loading={saveMutation.isPending}
          onClick={() => saveMutation.mutate()}
        >
          测试并保存
        </Button>
      </div>

      <AddModelDialog
        open={modelDialogOpen}
        onClose={() => setModelDialogOpen(false)}
        onSave={(model) => {
          setModels((previous) => [...previous, model]);
          setModelDialogOpen(false);
        }}
      />

      {provider && (
        <Dialog
          open={deleteOpen}
          onClose={() => setDeleteOpen(false)}
          title="删除供应商"
          footer={
            <>
              <Button variant="secondary" size="compact" onClick={() => setDeleteOpen(false)}>
                取消
              </Button>
              <Button
                variant="primary"
                size="compact"
                className="bg-danger!"
                loading={deleteMutation.isPending}
                onClick={() => deleteMutation.mutate()}
              >
                删除
              </Button>
            </>
          }
        >
          <p className="text-body text-primary">确定删除供应商「{provider.name}」？</p>
          <p className="mt-1 text-caption text-secondary">
            钥匙串中的 API Key 将一并清除。
          </p>
        </Dialog>
      )}
    </div>
  );
}

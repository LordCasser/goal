/**
 * 供应商添加/编辑表单与详情操作（tasks §5.2–§5.5，design D6）。
 *
 * - 草稿只随 `key={provider.id}` 挂载初始化一次；查询失效刷新列表与
 *   徽标，但不覆盖未提交的表单草稿（design.md §8.3/§11）。
 * - API Key 只进不出：输入框永不回显（服务端也从不返回 Key），已有 Key
 *   仅以「已配置」徽标 + 移除入口表达（design.md §9.3：密钥不出现在普通
 *   状态）。本地端点留空 Key 是正常状态，不显示警告（design D4）。
 * - 保存 = saveProvider（配置文件 + API Key/Header 凭据输入）；
 *   后端校验错误（AppError code）行内显示（task 5.3）。
 * - 操作（设为激活 / 删除 / 连接测试）只对已保存供应商开放；连接测试
 *   结果按 design D3 的 error_code 映射文案，行内反馈（task 5.4）。
 * - 所有写操作成功后经 onChanged 请求页面失效 ai-settings 查询；查询
 *   key 由页面持有（见 AiSettingsPage.tsx 文件头注释）。
 */
import { useId, useRef, useState, type JSX } from "react";
import { useMutation } from "@tanstack/react-query";

import { Button, Dialog, Input, ProgressDot, Select, SelectItem, cn } from "../../ui";
import { errorMessage, formatNumber, t, useTranslation } from "../../lib/i18n";
import {
  deleteProvider,
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

/** 三种 API 格式 + 各自的请求端点（design D2），下拉项直接显示端点路径。 */
const API_FORMATS: ReadonlyArray<{ value: ApiFormat; label: string }> = [
  { value: "anthropic_messages", label: "Anthropic Messages" },
  {
    value: "openai_chat_completions",
    label: "OpenAI Chat Completions",
  },
  { value: "openai_responses", label: "OpenAI Responses" },
];

type HeaderDraft = {
  id: string;
  name: string;
  value: string;
  /** Name returned by the backend when this row was loaded. */
  savedName: string | null;
};

type HeaderIssue =
  | "invalid_header_name"
  | "duplicate_header_name"
  | "reserved_header_name"
  | "invalid_header_value"
  | "missing_header_value"
  | "unknown_header_value";

const RESERVED_HEADER_NAMES = new Set([
  "host",
  "content-length",
  "transfer-encoding",
  "connection",
  "keep-alive",
  "te",
  "trailer",
  "upgrade",
  "proxy-authorization",
  "proxy-authenticate",
  "content-type",
  "accept",
]);

// RFC 9110 §5.6.2 token. Header values may contain horizontal tabs and
// printable ASCII only; this also rejects newlines before they reach IPC.
const HEADER_NAME_RE = /^[!#$%&'*+\-.^_`|~0-9A-Za-z]+$/;
const HEADER_VALUE_RE = /^[\x09\x20-\x7e]*$/;

function headerName(name: string): string {
  return name.trim().toLowerCase();
}

function initialHeaders(provider: ProviderSummary | null): HeaderDraft[] {
  return (provider?.extra_headers ?? []).map((name, index) => ({
    id: `saved-header-${index}`,
    name,
    value: "",
    savedName: headerName(name),
  }));
}

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
      return t("ai:settings.authFailed");
    case "provider_unreachable":
    case "timeout":
      return t("ai:settings.unreachable");
    case "invalid_request":
      return t("ai:settings.invalidRequest", { message: result.error_message ?? "" });
    case "rate_limited":
      return t("ai:settings.rateLimited");
    default:
      return result.error_message ?? result.error_code ?? t("ai:settings.connectionFailed");
  }
}

/** AppError 保留稳定 code（如 invalid_base_url），与 message 一起行内展示。 */
function errorText(error: unknown): string {
  return errorMessage(error);
}

export function ProviderForm({
  provider,
  onSaved,
  onChanged,
}: ProviderFormProps): JSX.Element {
  const { t: translate } = useTranslation("ai");
  const nameInputId = useId();
  const urlInputId = useId();
  const keyInputId = useId();
  const formatInputId = useId();
  const headerSectionId = useId();
  const headerId = useRef(0);

  const providerId = provider?.id ?? null;

  const [name, setName] = useState(provider?.name ?? "");
  const [baseUrl, setBaseUrl] = useState(provider?.base_url ?? "");
  const [apiFormat, setApiFormat] = useState<ApiFormat>(
    provider?.api_format ?? "openai_chat_completions",
  );
  const [apiKey, setApiKey] = useState("");
  const [models, setModels] = useState<ModelConfig[]>(provider?.models ?? []);
  const [headers, setHeaders] = useState<HeaderDraft[]>(() => initialHeaders(provider));
  const [visibleHeaders, setVisibleHeaders] = useState<Set<string>>(() => new Set());
  const [touchedHeaders, setTouchedHeaders] = useState<Set<string>>(() => new Set());
  const [headerSubmitError, setHeaderSubmitError] = useState<HeaderIssue | null>(null);
  const [modelDialogOpen, setModelDialogOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [testResult, setTestResult] = useState<ConnectionTestResult | null>(null);

  // 前端预校验（task 5.3）：名称非空、http(s) URL、至少一个模型。
  const missing: string[] = [];
  if (name.trim() === "") missing.push(translate("settings.name"));
  if (!isHttpBaseUrl(baseUrl.trim())) missing.push(translate("settings.baseUrl"));
  if (models.length === 0) missing.push(translate("settings.models"));
  const canSave = missing.length === 0;

  const requireProviderId = (): string => {
    if (!providerId) throw new Error("provider is not saved yet");
    return providerId;
  };

  const headerIssues = new Map<string, HeaderIssue>();
  const normalizedHeaderNames = headers.map((header) => headerName(header.name));
  normalizedHeaderNames.forEach((name, index) => {
    const header = headers[index]!;
    if (!name || !HEADER_NAME_RE.test(name)) {
      headerIssues.set(header.id, "invalid_header_name");
      return;
    }
    if (RESERVED_HEADER_NAMES.has(name)) {
      headerIssues.set(header.id, "reserved_header_name");
      return;
    }
    if (normalizedHeaderNames.indexOf(name) !== index) {
      headerIssues.set(header.id, "duplicate_header_name");
      return;
    }
    const savedName = header.savedName;
    const needsValue = savedName === null || savedName !== name;
    if (!header.value && needsValue) {
      headerIssues.set(header.id, "missing_header_value");
      return;
    }
    if (header.value && !HEADER_VALUE_RE.test(header.value)) {
      headerIssues.set(header.id, "invalid_header_value");
    }
  });

  const headerValues = Object.fromEntries(
    headers.flatMap((header) => {
      const name = headerName(header.name);
      return !headerIssues.has(header.id) && name && header.value
        ? [[name, header.value] as const]
        : [];
    }),
  );

  const headerErrorText = (issue: HeaderIssue): string =>
    translate(`settings.${issue}`);

  const addHeader = () => {
    headerId.current += 1;
    const id = `new-header-${headerId.current}`;
    setHeaders((previous) => [...previous, { id, name: "", value: "", savedName: null }]);
  };

  const touchHeader = (id: string) => {
    setTouchedHeaders((previous) => {
      if (previous.has(id)) return previous;
      const next = new Set(previous);
      next.add(id);
      return next;
    });
  };

  const saveMutation = useMutation({
    mutationFn: async (): Promise<ProviderConfig> => {
      const config: ProviderConfig = {
        id: provider?.id ?? "",
        name: name.trim(),
        base_url: baseUrl.trim(),
        api_format: apiFormat,
        extra_headers: normalizedHeaderNames,
        models,
        created_at: provider?.created_at ?? 0,
        archived: provider?.archived ?? false,
        connection_verified_at: null,
      };
      return saveProvider(config, apiKey.trim() || null, headerValues);
    },
    onSuccess: async (saved) => {
      setName(saved.name);
      setBaseUrl(saved.base_url);
      setApiKey(""); // 保存成功即清空 Key 草稿：不回显（design §9.3）
      setHeaders(initialHeaders({ ...saved, has_api_key: provider?.has_api_key ?? false, is_active: provider?.is_active ?? false }));
      setVisibleHeaders(new Set());
      setTouchedHeaders(new Set());
      setHeaderSubmitError(null);
      setTestResult(null);
      await onChanged(); // 等失效重取完成后再切换选中，避免右栏闪空态
      onSaved(saved.id);
    },
    onError: (error) => {
      const code = typeof error === "object" && error !== null && "code" in error
        ? (error as { code?: unknown }).code
        : null;
      setHeaderSubmitError(
        typeof code === "string" && [
          "invalid_header_name",
          "duplicate_header_name",
          "reserved_header_name",
          "invalid_header_value",
          "missing_header_value",
          "unknown_header_value",
        ].includes(code)
          ? code as HeaderIssue
          : null,
      );
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
    || apiKey !== "" || JSON.stringify(models) !== JSON.stringify(provider?.models ?? [])
    || JSON.stringify(normalizedHeaderNames) !== JSON.stringify((provider?.extra_headers ?? []).map(headerName))
    || headers.some((header) => header.value !== "");
  const hasHeaderIssues = headerIssues.size > 0;
  const canSaveWithHeaders = canSave && !hasHeaderIssues;

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
                {provider.is_active ? translate("settings.active") : translate("settings.inactive")}
              </span>
              <span className="text-caption text-hint">{provider.connection_verified_at ? translate("settings.verified") : translate("settings.unverified")}</span>
            </div>
            <div className="flex items-center gap-2">

              <Button
                variant="secondary"
                size="compact"
                onClick={() => testMutation.mutate()}
                loading={testMutation.isPending}
                disabled={busy || dirty}
                title={dirty ? translate("settings.unsavedTitle") : translate("settings.testSavedTitle")}
              >
                {translate("settings.testConnection")}
              </Button>
              <Button
                variant="ghost"
                size="compact"
                className="text-danger"
                onClick={() => setDeleteOpen(true)}
                disabled={busy}
              >
                {translate("settings.deleteProvider")}
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
                ? translate("settings.connectionSuccess", { latency: formatNumber(testResult.latency_ms ?? 0) })
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
          <h4 className="text-block-title font-semibold text-primary">{translate("settings.addProviderTitle")}</h4>
        </div>
      )}

      <fieldset disabled={busy} className="flex min-w-0 flex-col gap-4 px-4 py-4">
        <div className="flex flex-col gap-1">
          <label htmlFor={nameInputId} className="text-caption text-secondary">
            {translate("settings.name")}
          </label>
          <Input
            id={nameInputId}
            value={name}
            onChange={(e) => setName(e.currentTarget.value)}
            placeholder={translate("settings.namePlaceholder")}
          />
        </div>
        <div className="flex flex-col gap-1">
          <label htmlFor={urlInputId} className="text-caption text-secondary">
            {translate("settings.baseUrl")}
          </label>
          <Input
            id={urlInputId}
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.currentTarget.value)}
            placeholder={translate("settings.baseUrlPlaceholder")}
          />
        </div>
        <div className="flex flex-col gap-1">
          <label htmlFor={keyInputId} className="text-caption text-secondary">
            {translate("settings.apiKey")}
          </label>
          {/* 已有 Key：只显示「已配置」徽标 + 移除入口；明文既不回显也不截尾。 */}
          {provider?.has_api_key && (
            <div className="flex items-center gap-2">
              <span className="inline-flex items-center rounded-[2px] border border-light bg-subtle px-1.5 py-0.5 text-caption text-secondary">
                {translate("settings.configured")}
              </span>
              <Button
                variant="ghost"
                size="compact"
                className="text-danger"
                onClick={() => removeKeyMutation.mutate()}
                loading={removeKeyMutation.isPending}
              >
                {translate("settings.remove")}
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
                ? translate("settings.replaceKeyPlaceholder")
                : translate("settings.localKeyPlaceholder")
            }
          />
          <p className="text-caption text-hint">{translate("settings.keychainHelp")}</p>
        </div>
        <div className="flex flex-col gap-1">
          <label htmlFor={formatInputId} className="text-caption text-secondary">
            {translate("settings.apiFormat")}
          </label>
          <Select
            id={formatInputId}
            value={apiFormat}
            onValueChange={(value) => setApiFormat(value as ApiFormat)}
            triggerClassName="w-full"
          >
            {API_FORMATS.map((format) => (
              <SelectItem key={format.value} value={format.value}>
                {format.label}
              </SelectItem>
            ))}
          </Select>
        </div>

        <section id={headerSectionId} className="flex min-w-0 flex-col gap-2">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="min-w-0">
              <h4 className="text-block-title font-semibold text-primary">{translate("settings.requestHeaders")}</h4>
              <p className="text-caption text-hint">{translate("settings.requestHeadersHelp")}</p>
            </div>
            <Button size="compact" className="cursor-pointer duration-150" onClick={addHeader}>
              {translate("settings.addHeader")}
            </Button>
          </div>
          {headers.length > 0 && (
            <ul className="flex min-w-0 flex-col gap-2" aria-label={translate("settings.requestHeaders")}>
              {headers.map((header, index) => {
                const issue = headerIssues.get(header.id);
                const displayedIssue = touchedHeaders.has(header.id) ? issue : undefined;
                const nameInputIdForRow = `${header.id}-name`;
                const valueInputIdForRow = `${header.id}-value`;
                const errorIdForRow = `${header.id}-error`;
                const labelSuffix = `${index + 1}`;
                const valueVisible = visibleHeaders.has(header.id);
                const savedNameUnchanged = header.savedName !== null && header.savedName === headerName(header.name);
                return (
                  <li key={header.id} className="flex min-w-0 flex-wrap items-end gap-2 rounded-md border border-light bg-subtle p-2">
                    <div className="min-w-[9rem] flex-1">
                      <label htmlFor={nameInputIdForRow} className="mb-1 block text-caption text-secondary">
                        {translate("settings.headerName")}
                      </label>
                      <Input
                        id={nameInputIdForRow}
                        aria-label={`${translate("settings.headerName")} ${labelSuffix}`}
                        aria-invalid={displayedIssue === "invalid_header_name" || displayedIssue === "duplicate_header_name" || displayedIssue === "reserved_header_name" || undefined}
                        aria-describedby={displayedIssue === "invalid_header_name" || displayedIssue === "duplicate_header_name" || displayedIssue === "reserved_header_name" ? errorIdForRow : undefined}
                        autoFocus={header.savedName === null && index === headers.length - 1}
                        value={header.name}
                        onChange={(event) => {
                          const value = event.currentTarget.value;
                          touchHeader(header.id);
                          setHeaderSubmitError(null);
                          setHeaders((previous) => previous.map((item) => item.id === header.id ? { ...item, name: value } : item));
                        }}
                        onBlur={() => touchHeader(header.id)}
                        placeholder={translate("settings.headerNamePlaceholder")}
                      />
                    </div>
                    <div className="min-w-[11rem] flex-[1.4]">
                      <label htmlFor={valueInputIdForRow} className="mb-1 block text-caption text-secondary">
                        {translate("settings.headerValue")}
                      </label>
                      <div className="flex min-w-0 gap-1">
                        <Input
                          id={valueInputIdForRow}
                          aria-label={`${translate("settings.headerValue")} ${labelSuffix}`}
                          aria-invalid={displayedIssue === "invalid_header_value" || displayedIssue === "missing_header_value" || undefined}
                          aria-describedby={displayedIssue === "invalid_header_value" || displayedIssue === "missing_header_value" ? errorIdForRow : undefined}
                          type={valueVisible ? "text" : "password"}
                          autoComplete="off"
                          value={header.value}
                          onChange={(event) => {
                            const value = event.currentTarget.value;
                            touchHeader(header.id);
                            setHeaderSubmitError(null);
                            setHeaders((previous) => previous.map((item) => item.id === header.id ? { ...item, value } : item));
                          }}
                          onBlur={() => touchHeader(header.id)}
                          placeholder={translate(savedNameUnchanged ? "settings.savedHeaderValuePlaceholder" : "settings.headerValuePlaceholder")}
                        />
                        <button
                          type="button"
                          className="flex h-9 w-9 shrink-0 cursor-pointer items-center justify-center rounded-md text-secondary transition-colors duration-150 hover:bg-hover hover:text-primary disabled:cursor-default disabled:opacity-45 disabled:hover:bg-transparent"
                          disabled={header.value.length === 0}
                          aria-label={translate(valueVisible ? "settings.hideHeaderValue" : "settings.showHeaderValue", { name: header.name || labelSuffix })}
                          title={translate(valueVisible ? "settings.hideHeaderValue" : "settings.showHeaderValue", { name: header.name || labelSuffix })}
                          aria-pressed={valueVisible}
                          onClick={() => setVisibleHeaders((previous) => {
                            const next = new Set(previous);
                            if (next.has(header.id)) next.delete(header.id); else next.add(header.id);
                            return next;
                          })}
                        >
                          <svg viewBox="0 0 16 16" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="1.35" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                            <path d="M2.25 8s2.05-3.25 5.75-3.25S13.75 8 13.75 8 11.7 11.25 8 11.25 2.25 8 2.25 8Z" />
                            <circle cx="8" cy="8" r="1.5" />
                            {valueVisible && <path d="m2.5 2.5 11 11" />}
                          </svg>
                        </button>
                      </div>
                    </div>
                    <button
                      type="button"
                      className="flex h-9 shrink-0 cursor-pointer items-center justify-center rounded-md px-2 text-caption text-danger transition-colors duration-150 hover:bg-hover"
                      aria-label={translate("settings.removeHeader", { name: header.name || labelSuffix })}
                      onClick={(event) => {
                        // Move focus before removing its row so keyboard editing
                        // continues at the next row, previous row, or Add control.
                        const row = event.currentTarget.closest("li");
                        const nextInput = row?.nextElementSibling?.querySelector("input")
                          ?? row?.previousElementSibling?.querySelector("input");
                        const focusTarget = nextInput
                          ?? document.getElementById(headerSectionId)?.querySelector("button");
                        focusTarget?.focus({ preventScroll: true });
                        setHeaders((previous) => previous.filter((item) => item.id !== header.id));
                        setTouchedHeaders((previous) => {
                          const next = new Set(previous);
                          next.delete(header.id);
                          return next;
                        });
                        setVisibleHeaders((previous) => {
                          const next = new Set(previous);
                          next.delete(header.id);
                          return next;
                        });
                        setHeaderSubmitError(null);
                      }}
                    >
                      {translate("settings.remove")}
                    </button>
                    {displayedIssue && (
                      <p id={errorIdForRow} role="alert" className="basis-full text-caption text-danger">
                        {headerErrorText(displayedIssue)}
                      </p>
                    )}
                  </li>
                );
              })}
            </ul>
          )}
          {headerSubmitError && (
            <p role="alert" className="text-caption text-danger">{headerErrorText(headerSubmitError)}</p>
          )}
        </section>

        {/* 模型列表（task 5.2）：model_id + 窗口 + 最大输出 + 工具调用标记。 */}
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-2">
            <span className="text-block-title font-semibold text-primary">{translate("settings.models")}</span>
            <Button size="compact" onClick={() => setModelDialogOpen(true)}>
              {translate("settings.addModel")}
            </Button>
          </div>
          {models.length === 0 ? (
            <p className="text-caption text-hint">
              {translate("settings.noModels")}
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
                      {translate("settings.modelDetails", { context: formatNumber(model.context_window), output: formatNumber(model.max_output_tokens) })}
                      {model.supports_tools ? ` · ${translate("settings.toolsSupported")}` : ` · ${translate("settings.toolsUnsupported")}`}
                    </span>
                  </div>
                  <button
                    type="button"
                    aria-label={translate("settings.removeModel", { model: model.model_id })}
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
          {!canSaveWithHeaders && <p className="text-caption text-hint">{translate("settings.needComplete", { items: missing.concat(hasHeaderIssues ? [translate("settings.requestHeaders")] : []).join(translate("common.listSeparator")) })}</p>}
          {canSaveWithHeaders && <p className="text-caption text-hint">{saveMutation.isPending ? translate("settings.testingSave") : translate("settings.saveHelp")}</p>}
          {saveMutation.isError && (
            <p className="text-caption text-danger">{errorText(saveMutation.error)}</p>
          )}
        </div>
        <Button
          variant="primary"
          disabled={!canSaveWithHeaders || busy}
          loading={saveMutation.isPending}
          onClick={() => saveMutation.mutate()}
        >
          {translate("settings.testSave")}
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
          title={translate("settings.deleteProvider")}
          footer={
            <>
              <Button variant="secondary" size="compact" onClick={() => setDeleteOpen(false)}>
                {translate("settings.cancel")}
              </Button>
              <Button
                variant="primary"
                size="compact"
                className="bg-danger!"
                loading={deleteMutation.isPending}
                onClick={() => deleteMutation.mutate()}
              >
                {translate("settings.deleteProvider")}
              </Button>
            </>
          }
        >
          <p className="text-body text-primary">{translate("settings.deleteConfirm", { provider: provider.name })}</p>
          <p className="mt-1 text-caption text-secondary">
            {translate("settings.deleteHelp")}
          </p>
        </Dialog>
      )}
    </div>
  );
}

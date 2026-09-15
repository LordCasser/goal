/**
 * Settings → AI: provider navigation beside its editable detail panel.
 * AI settings have no backend event; successful mutations invalidate this page’s
 * query while ProviderForm keeps its own unsaved fields until selection changes.
 */
import { useCallback, useEffect, useState, type JSX } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { Button, Input, Checkbox, EmptyState, ProgressDot, Select, SelectGroup, SelectItem, SelectLabel, cn } from "../../ui";
import { getAiSettings, getAppFlag, setAppFlag, setActiveProvider } from "../../lib/ipc";
import { errorMessage, useTranslation } from "../../lib/i18n";
import type { AiSettingsSummary } from "../../lib/ipc";
import { ProviderForm } from "./ProviderForm";
import { AI_AVAILABILITY_KEY, AI_SETTINGS_KEY, PLAN_WITH_AI_FLAG, PLAN_WITH_AI_KEY, usePlanWithAIPreference } from "../agent/PlanWithAI";

/** 仅本页消费的字面量查询键（见文件头注释）。 */

type Selection = { kind: "provider"; id: string } | { kind: "new" } | null;

export function AiSettingsPage(): JSX.Element {
  const { t } = useTranslation("ai");
  const queryClient = useQueryClient();
  const contextSetting = useQuery({ queryKey: ["app-flag", "ai.context-idle-minutes"], queryFn: () => getAppFlag("ai.context-idle-minutes") });
  const [contextMinutes, setContextMinutes] = useState("15");
  useEffect(() => { if (contextSetting.isSuccess) setContextMinutes(contextSetting.data ?? "15"); }, [contextSetting.data, contextSetting.isSuccess]);
  const saveTimeout = useMutation({
    mutationFn: () => setAppFlag("ai.context-idle-minutes", contextMinutes),
    onSuccess: async () => {
      queryClient.setQueryData(["app-flag", "ai.context-idle-minutes"], contextMinutes);
      await queryClient.invalidateQueries({ queryKey: ["agent-conversation"] });
    },
  });
  const validTimeout = /^\d+$/.test(contextMinutes) && Number(contextMinutes) >= 1 && Number(contextMinutes) <= 1440;
  const preference = usePlanWithAIPreference();
  const planEntry = useMutation({
    mutationFn: (enabled: boolean) => setAppFlag(PLAN_WITH_AI_FLAG, String(enabled)),
    onSuccess: (_result, enabled) => queryClient.setQueryData(PLAN_WITH_AI_KEY, String(enabled)),
  });
  const settingsQuery = useQuery({
    queryKey: AI_SETTINGS_KEY,
    queryFn: getAiSettings,
  });

  // 写操作成功后的失效通知；key 只在本页出现（见文件头注释）。
  const invalidate = useCallback(
    async () => { await Promise.all([
      queryClient.invalidateQueries({ queryKey: AI_SETTINGS_KEY }),
      queryClient.invalidateQueries({ queryKey: AI_AVAILABILITY_KEY }),
      queryClient.invalidateQueries({ queryKey: ["issue-report"] }),
    ]); },
    [queryClient],
  );

  const activate = useMutation({
    mutationFn: ({ providerId, modelId }: { providerId: string; modelId: string }) => setActiveProvider(providerId, modelId),
    onSuccess: invalidate,
  });

  const [selection, setSelection] = useState<Selection>(null);

  const summary: AiSettingsSummary = settingsQuery.data ?? {
    active_provider: null,
    active_model_id: null,
    providers: [],
    ai_available: false,
  };

  // 选中态自愈：初次数据到达时选中激活项（否则第一项）；选中项被删除后
  // 回落到剩余第一项或空态——删除激活供应商后后端本就不会自动挑替代
  // （task 1.5），这里的回落只是列表浏览位置。
  useEffect(() => {
    const data = settingsQuery.data;
    if (data === undefined) return;
    setSelection((current) => {
      if (current === null) {
        const preferred = data.providers.find((p) => p.is_active) ?? data.providers[0];
        return preferred ? { kind: "provider", id: preferred.id } : null;
      }
      if (
        current.kind === "provider" &&
        !data.providers.some((p) => p.id === current.id)
      ) {
        const next = data.providers[0];
        return next ? { kind: "provider", id: next.id } : null;
      }
      return current;
    });
  }, [settingsQuery.data]);

  const selected =
    selection?.kind === "provider"
      ? (summary.providers.find((p) => p.id === selection.id) ?? null)
      : null;

  const select = (id: string) => setSelection({ kind: "provider", id });

  return (
    <div className="flex flex-col gap-5">
      {/* 说明行（task 5.1/§9.3）：先说明接入方式与费用来源，再给当前状态。 */}
      <header className="flex flex-col gap-1">
        <h3 className="text-section-title font-semibold text-primary">{t("settings.title")}</h3>
        <p className="text-caption text-secondary">
          {t("settings.description")}
        </p>
        {settingsQuery.isError && (
          <p className="text-caption text-danger">{errorMessage(settingsQuery.error)}</p>
        )}
      </header>

      <section className="rounded-lg border border-light bg-content px-4 py-3">
        <div className="mb-2 flex items-center justify-between gap-3">
          <label htmlFor="active-ai-model" className="text-body font-medium">{t("settings.current")}</label>
          <span className="flex items-center gap-1.5 text-caption text-secondary"><ProgressDot tone={summary.ai_available ? "active" : "idle"} />{summary.ai_available ? t("settings.connected") : t("settings.selectVerified")}</span>
        </div>
        <Select id="active-ai-model" aria-label={t("settings.selectProviderModel")}
          placeholder={t("settings.selectPlaceholder")}
          triggerClassName="h-10 w-full text-body"
          value={summary.active_provider && summary.active_model_id ? JSON.stringify([summary.active_provider.id, summary.active_model_id]) : ""}
          disabled={settingsQuery.isPending || activate.isPending}
          onValueChange={(value) => { const [providerId, modelId] = JSON.parse(value) as [string, string]; activate.mutate({ providerId, modelId }); }}>
          {summary.providers.map((p) => <SelectGroup key={p.id}>
            <SelectLabel>{`${p.name}${p.connection_verified_at ? "" : ` · ${t("settings.unverified")}`}`}</SelectLabel>
            {p.models.map((m) => <SelectItem key={m.model_id} value={JSON.stringify([p.id, m.model_id])} disabled={!p.connection_verified_at}>{p.name} / {m.model_id}{m.supports_tools ? "" : ` · ${t("settings.chatOnly")}`}</SelectItem>)}
          </SelectGroup>)}
        </Select>
        <p className="mt-2 text-caption text-secondary">{t("settings.activeHelp")}</p>
        {activate.isError && <p role="alert" className="mt-2 text-caption text-danger">{errorMessage(activate.error)}</p>}
      </section>

      <section className="flex items-center justify-between gap-4 rounded-lg border border-light px-4 py-3">
        <div><h4 className="text-body font-medium">{t("settings.planEntryTitle")}</h4>
          <p className="mt-1 text-caption text-secondary">{t("settings.planEntryHelp")}</p>
          {planEntry.isError && <p role="alert" className="mt-1 text-caption text-danger">{errorMessage(planEntry.error)}</p>}
        </div>
        <Checkbox aria-label={t("settings.planEntryAria")} checked={preference.data !== "false"}
          disabled={preference.isPending || preference.isError || planEntry.isPending} onChange={(enabled) => planEntry.mutate(enabled)} />
      </section>

      <section className="rounded-lg border border-light px-4 py-3">
        <div className="flex items-center justify-between gap-4">
          <div><label htmlFor="coach-context-timeout" className="text-body font-medium">{t("settings.contextTitle")}</label>
            <p className="mt-1 text-caption text-secondary">{t("settings.contextHelp")}</p></div>
          <div className="flex shrink-0 items-center gap-2">
            <Input id="coach-context-timeout" aria-label={t("settings.contextAria")} className="w-20!" type="number" min={1} max={1440} step={1}
              value={contextMinutes} disabled={contextSetting.isPending || contextSetting.isError || saveTimeout.isPending} onChange={(e) => setContextMinutes(e.target.value)} />
            <span className="text-caption text-secondary">{t("settings.minutes")}</span>
            <Button variant="secondary" size="compact" loading={saveTimeout.isPending} disabled={!validTimeout || contextMinutes === (contextSetting.data ?? "15")} onClick={() => saveTimeout.mutate()}>{t("settings.saveDuration")}</Button>
          </div>
        </div>
        {!validTimeout && <p role="alert" className="mt-2 text-caption text-danger">{t("settings.invalidMinutes")}</p>}
        {saveTimeout.isError && <p role="alert" className="mt-2 text-caption text-danger">{errorMessage(saveTimeout.error)}</p>}
      </section>

      <h4 className="text-block-title font-semibold">{t("settings.manage")}</h4>
      {/* 两栏：外层 bg-subtle 画布 + 两块 bg-content 面板（design D6 映射）。 */}
      <div className="flex items-stretch gap-4">
        <aside className="flex w-[148px] shrink-0 flex-col overflow-hidden rounded-lg border border-light bg-subtle">
          <div className="border-b border-light px-3 py-2 text-block-title font-semibold text-primary">
            {t("settings.providers")}
          </div>
          <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
            {settingsQuery.isPending ? (
              <p className="px-3 py-6 text-center text-caption text-hint">{t("common.loading")}</p>
            ) : summary.providers.length === 0 ? (
              <p className="px-3 py-6 text-center text-caption text-hint">{t("settings.noProviders")}</p>
            ) : (
              summary.providers.map((item) => {
                const isSelected =
                  selection?.kind === "provider" && selection.id === item.id;
                return (
                  <button
                    key={item.id}
                    type="button"
                    onClick={() => select(item.id)}
                    aria-current={isSelected ? "true" : undefined}
                    className={cn(
                      "flex w-full items-start gap-2 border-b border-light px-3 py-2 text-left",
                      "transition-colors duration-100 hover:bg-hover",
                      isSelected && "bg-focus-surface",
                    )}
                  >
                    {/* 状态点附文字说明（task 5.5），激活态不只靠颜色。 */}
                    <ProgressDot
                      tone={item.is_active ? "active" : "idle"}
                      className="mt-1.5"
                    />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-body text-primary">
                        {item.name}
                      </span>
                      <span className="block text-caption text-secondary">
                        {t("settings.providerModels", { count: item.models.length })}{item.is_active ? ` · ${t("settings.active")}` : ""}
                      </span>
                    </span>
                  </button>
                );
              })
            )}
          </div>
          <button
            type="button"
            onClick={() => setSelection({ kind: "new" })}
            className={cn(
              "flex w-full items-center gap-2 border-t border-light px-3 py-2 text-left",
              "text-menu text-primary transition-colors duration-100 hover:bg-hover",
              selection?.kind === "new" && "bg-focus-surface",
            )}
          >
            + {t("settings.addProvider")}
          </button>
        </aside>

        <section className="min-w-0 flex-1 overflow-hidden rounded-lg border border-light bg-content">
          {settingsQuery.isPending ? (
            <p className="px-4 py-6 text-caption text-hint">{t("common.loading")}</p>
          ) : selection?.kind === "new" ? (
            <ProviderForm
              key="new"
              provider={null}
              onSaved={select}
              onChanged={invalidate}
            />
          ) : selected ? (
            <ProviderForm
              key={selected.id}
              provider={selected}
              onSaved={select}
              onChanged={invalidate}
            />
          ) : (
            <EmptyState
              title={t("settings.title")}
              description={t("settings.emptyDescription")}
              className="border-0! px-5! py-8!"
            />
          )}
        </section>
      </div>
    </div>
  );
}

/**
 * Settings dialog: theme choice, week start day and diagnostics
 * (design.md §4.4 主题切换、§9.3 AI 设置预留；spec: local-logging).
 *
 * Every write goes through the app_settings-backed commands. No settings
 * event exists, so each mutator invalidates qk.settings() itself
 * (lib/events.ts). Theme picks call applyTheme() first — 选中立即生效
 * (§4.4) — while app_settings stays the durable truth; the localStorage
 * copy written by applyTheme only bridges the first paint (initTheme).
 */
import { useId, useState } from "react";
import type { JSX } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { revealItemInDir } from "@tauri-apps/plugin-opener";

import { ReminderSettingsSection } from "../reminders/ReminderSettingsSection";
import { AiSettingsPage } from "../ai-settings/AiSettingsPage";
import { Button, Dialog, Select, SelectItem, cn } from "../../ui";
import { qk } from "../../lib/events";
import {
  getDebugLogDir,
  getSchemaVersion,
  getSettings,
  setLocale,
  setLogLevel,
  setTheme,
  setWeekStartDay,
} from "../../lib/ipc";
import type { LogLevel, Settings, Theme } from "../../lib/ipc";
import { applyLocale, errorMessage, useTranslation } from "../../lib/i18n";
import { applyTheme } from "../../lib/theme";

/** 两个用户确认的浅色主题（design.md §4.4）；默认白底。 */
const THEME_OPTIONS: ReadonlyArray<{ value: Theme }> = [
  { value: "white" },
  { value: "gray" },
];

const LOG_LEVELS: readonly LogLevel[] = ["error", "warn", "info", "debug"];

/*
 * 主题示意小图（48×32、2px 圆角）：canvas 上承托一块 content 面板。
 * 预览本身就是两个主题的图例本体，允许字面色值；页面其余部分仍只
 * 消费语义 token（design.md §4.4、index.css）。
 */
function ThemePreview({ theme }: { theme: Theme }) {
  const canvas = theme === "gray" ? "#f6f6f6" : "#ffffff";
  const edge = theme === "gray" ? "#e3e3e3" : "#edf0f2";
  return (
    <div aria-hidden="true" className="overflow-hidden rounded-md border p-3" style={{ height: 104, backgroundColor: canvas, borderColor: edge }}>
      <div className="mb-3 flex items-center gap-1"><i className="h-1 w-1 rounded-full bg-[#bac0c7]" /><i className="h-1 w-1 rounded-full bg-[#bac0c7]" /><i className="h-1 w-1 rounded-full bg-[#bac0c7]" /></div>
      <div className="flex h-16 gap-2">
        {[0, 1].map((column) => <div key={column} className="flex-1 rounded-sm bg-white p-2" style={{ border: `1px solid ${edge}` }}>
          <div className="mb-2 h-1 w-8 rounded bg-[#949ca7]" />
          <div className="mb-1.5 h-1 w-12 rounded bg-[#e0e4e9]" />
          <div className="h-1 w-9 rounded bg-[#e0e4e9]" />
        </div>)}
      </div>
    </div>
  );
}

/** 行内错误（AppError 优先取 message），不打断、不弹窗（design.md 9.2）。 */
function InlineError({ error }: { error: unknown }) {
  if (error === null || error === undefined) return null;
  return <p className="text-caption text-danger">{errorMessage(error)}</p>;
}

export function SettingsDialog({
  open,
  onClose,
  onPreviewExitPoll,
}: {
  open: boolean;
  onClose: () => void;
  /** 主动触发退出调查（onboarding §3.6 菜单入口的设置页形态）。 */
  onPreviewExitPoll?: () => void;
}): JSX.Element | null {
  const queryClient = useQueryClient();
  const { t } = useTranslation("shell");
  const weekStartId = useId();
  const logLevelId = useId();
  const localeId = useId();

  // 关闭期间不发 IPC；settings 键与 App 共享同一份缓存。
  const settingsQuery = useQuery({
    queryKey: qk.settings(),
    queryFn: getSettings,
    enabled: open,
  });
  // 叶子查询，无对应工厂键；schema 版本只在打开时取一次。
  const schemaQuery = useQuery({
    queryKey: ["schema-version"],
    queryFn: getSchemaVersion,
    enabled: open,
  });

  // 乐观写回 settings 缓存，让选中态立即跟随；失败回滚，成功后以
  // app_settings 为准重取（无 settings 事件，见 lib/events.ts）。
  const themeMutation = useMutation({
    mutationFn: setTheme,
    onMutate: async (theme) => {
      await queryClient.cancelQueries({ queryKey: qk.settings() });
      const previous = queryClient.getQueryData<Settings>(qk.settings());
      queryClient.setQueryData<Settings>(qk.settings(), (current) =>
        current ? { ...current, theme } : current,
      );
      return { previous };
    },
    onError: (_error, _theme, context) => {
      if (!context?.previous) return;
      queryClient.setQueryData(qk.settings(), context.previous);
      // 缓存回滚后把表面也翻回去，预览与选中态保持一致。
      applyTheme(context.previous.theme ?? "white");
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: qk.settings() });
    },
  });

  const weekStartMutation = useMutation({
    mutationFn: setWeekStartDay,
    onMutate: async (day) => {
      await queryClient.cancelQueries({ queryKey: qk.settings() });
      const previous = queryClient.getQueryData<Settings>(qk.settings());
      queryClient.setQueryData<Settings>(qk.settings(), (current) =>
        current ? { ...current, week_start_day: day } : current,
      );
      return { previous };
    },
    onError: (_error, _day, context) => {
      if (context?.previous) {
        queryClient.setQueryData(qk.settings(), context.previous);
      }
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: qk.settings() });
    },
  });

  const localeMutation = useMutation({
    mutationFn: async (locale: Settings["locale"]) => {
      await setLocale(locale);
      return locale;
    },
    onMutate: () => queryClient.cancelQueries({ queryKey: qk.settings() }),
    onSuccess: (locale) => {
      // Publish the durable preference only after success. App also observes
      // this cache, so optimistic writes would switch the whole UI too early.
      queryClient.setQueryData<Settings>(qk.settings(), (current) =>
        current ? { ...current, locale } : current,
      );
      applyLocale(locale);
      void queryClient.invalidateQueries({ queryKey: qk.settings() });
    },
  });

  // 后端没有日志级别回读命令，先显示 info；之后的取值以本地选择为准。
  const [logLevel, setLogLevelValue] = useState<LogLevel>("info");
  const logLevelMutation = useMutation({ mutationFn: setLogLevel });

  const [openingLogs, setOpeningLogs] = useState(false);
  const [logDir, setLogDir] = useState<string | null>(null);
  const [logDirError, setLogDirError] = useState<unknown>(null);
  // 模型设置二级弹窗：设置弹窗保持打开，AI 页在上层展示。
  const [section, setSection] = useState<"general" | "ai" | "reminders" | "diagnostics">("general");

  const selectedTheme = settingsQuery.data?.theme ?? "white";
  const weekStartDay = settingsQuery.data?.week_start_day ?? 1;
  const selectedLocale = settingsQuery.data?.locale ?? "en";

  const chooseTheme = (theme: Theme) => {
    if (theme === selectedTheme) return;
    applyTheme(theme); // 选中立即生效（§4.4），落库紧随其后
    themeMutation.mutate(theme);
  };

  const chooseWeekStart = (day: number) => {
    if (day === weekStartDay) return;
    weekStartMutation.mutate(day);
  };

  const chooseLogLevel = (level: LogLevel) => {
    setLogLevelValue(level);
    logLevelMutation.mutate(level);
  };

  // 定位日志目录（spec: local-logging）：取路径 → 在系统文件管理器中定位。
  const openLogDir = async () => {
    setOpeningLogs(true);
    setLogDirError(null);
    try {
      const path = await getDebugLogDir();
      await revealItemInDir(path);
      setLogDir(path);
    } catch (error) {
      setLogDirError(error);
    } finally {
      setOpeningLogs(false);
    }
  };

  if (!open) return null;

  return (
    <Dialog open={open} onClose={onClose} title={t("settings.title")} wide className="max-w-[1040px]! h-[680px] max-h-[calc(100vh-64px)]" bodyClassName="overflow-hidden!">
      <div className="flex h-full gap-7 border-t border-light pt-5">
        <nav aria-label={t("settings.navLabel")} className="flex w-[136px] shrink-0 flex-col gap-1">
          {([
            ["general", t("settings.nav.general"), t("settings.nav.generalDescription")],
            ["ai", t("settings.nav.ai"), t("settings.nav.aiDescription")],
            ["reminders", t("settings.nav.reminders"), t("settings.nav.remindersDescription")],
            ["diagnostics", t("settings.nav.diagnostics"), t("settings.nav.diagnosticsDescription")],
          ] as const).map(([id, label, description]) => (
            <button key={id} type="button" aria-current={section === id ? "page" : undefined}
              onClick={() => setSection(id)}
              className={cn("rounded-lg px-3 py-3 text-left transition-colors hover:bg-hover", section === id && "bg-subtle")}>
              <span className={cn("block text-body font-medium", section === id ? "text-primary" : "text-secondary")}>{label}</span>
              <span className="mt-0.5 block text-caption text-hint">{description}</span>
            </button>
          ))}
        </nav>
        <div className="min-h-0 min-w-0 flex-1 overflow-y-auto pr-1 pb-2">
          {section === "general" && <div className="flex flex-col gap-7">
            <section>
              <h3 className="text-section-title font-semibold text-primary">{t("settings.appearance.title")}</h3>
              <p className="mt-1 text-body text-secondary">{t("settings.appearance.description")}</p>
              <div className="mt-5 grid grid-cols-2 gap-3">
                {THEME_OPTIONS.map((option) => (
                  <button key={option.value} type="button" aria-label={t(`settings.theme.${option.value}`)}
                    aria-pressed={selectedTheme === option.value}
                    disabled={settingsQuery.isPending || themeMutation.isPending}
                    onClick={() => chooseTheme(option.value)}
                    className={cn("overflow-hidden rounded-lg border p-2 text-left transition-colors", selectedTheme === option.value ? "border-focus" : "border-light hover:border-control")}>
                    <ThemePreview theme={option.value} />
                    <span className="flex items-center justify-between px-2 pb-1 pt-3 text-body font-medium text-primary">
                      {t(`settings.theme.${option.value}`)}
                      <span aria-hidden="true" className={cn("flex h-4 w-4 items-center justify-center rounded-full border text-[10px]", selectedTheme === option.value ? "border-focus bg-focus text-white" : "border-control")}>{selectedTheme === option.value ? "✓" : ""}</span>
                    </span>
                    <span className="block px-2 pb-2 text-caption text-secondary">{t(`settings.theme.${option.value}Description`)}</span>
                  </button>
                ))}
              </div>
              <InlineError error={themeMutation.error} />
            </section>
            <section className="border-t border-light pt-5">
              <div className="flex items-center justify-between gap-4">
                <div>
                  <label htmlFor={localeId} className="text-body font-medium text-primary">{t("settings.language.label")}</label>
                  <p className="mt-1 text-caption text-secondary">{t("settings.language.description")}</p>
                </div>
                <Select id={localeId} value={selectedLocale} disabled={settingsQuery.isPending || localeMutation.isPending}
                  onValueChange={(value) => {
                    const locale = value as Settings["locale"];
                    if (locale !== selectedLocale) localeMutation.mutate(locale);
                  }} triggerClassName="w-[132px]">
                  <SelectItem value="zh-CN">{t("settings.language.zhCN")}</SelectItem>
                  <SelectItem value="en">{t("settings.language.en")}</SelectItem>
                </Select>
              </div>
              <InlineError error={localeMutation.error} />
            </section>
            <section className="border-t border-light pt-5">
              <div className="flex items-center justify-between gap-4">
                <div>
                  <label htmlFor={weekStartId} className="text-body font-medium text-primary">{t("settings.weekStart.label")}</label>
                  <p className="mt-1 text-caption text-secondary">{t("settings.weekStart.description")}</p>
                </div>
                <Select id={weekStartId} value={String(weekStartDay)} disabled={settingsQuery.isPending || weekStartMutation.isPending}
                  onValueChange={(value) => chooseWeekStart(Number(value))} triggerClassName="w-[116px]">
                  {Array.from({ length: 7 }, (_, index) => <SelectItem key={index + 1} value={String(index + 1)}>{t(`settings.weekDay.${index + 1}`)}</SelectItem>)}
                </Select>
              </div>
              <InlineError error={weekStartMutation.error} />
            </section>
            {onPreviewExitPoll && <section className="flex items-center justify-between gap-4 border-t border-light pt-5">
              <div><h3 className="text-body font-medium text-primary">{t("settings.feedback.title")}</h3><p className="mt-1 text-caption text-secondary">{t("settings.feedback.description")}</p></div>
              <Button size="compact" onClick={onPreviewExitPoll}>{t("settings.feedback.action")}</Button>
            </section>}
            <InlineError error={settingsQuery.error} />
            <p className="text-caption text-hint" role="status">{themeMutation.isPending || weekStartMutation.isPending || localeMutation.isPending ? t("settings.saveStatus.saving") : t("settings.saveStatus.auto")}</p>
          </div>}
          {section === "ai" && <AiSettingsPage />}
          {section === "reminders" && <div className="flex flex-col gap-6"><div><h3 className="text-section-title font-semibold text-primary">{t("settings.reminders.title")}</h3><p className="mt-1 text-body text-secondary">{t("settings.reminders.description")}</p></div><ReminderSettingsSection /></div>}
          {section === "diagnostics" && <div className="flex flex-col gap-6">
            <div><h3 className="text-section-title font-semibold text-primary">{t("settings.diagnostics.title")}</h3><p className="mt-1 text-body text-secondary">{t("settings.diagnostics.description")}</p></div>
            <section className="border-t border-light pt-5">
              <div className="flex items-center justify-between gap-3">
                <div><label htmlFor={logLevelId} className="text-body font-medium text-primary">{t("settings.logLevel.label")}</label><p className="mt-1 text-caption text-secondary">{t("settings.logLevel.description")}</p></div>
                <Select id={logLevelId} value={logLevel} onValueChange={(value) => chooseLogLevel(value as LogLevel)} triggerClassName="w-[116px]">
                  {LOG_LEVELS.map((level) => <SelectItem key={level} value={level}>{level}</SelectItem>)}
                </Select>
              </div>
              <InlineError error={logLevelMutation.error} />
            </section>
            <section className="border-t border-light pt-5">
              <div className="flex items-center justify-between gap-3">
                <div><h3 className="text-body font-medium text-primary">{t("settings.logs.title")}</h3><p className="mt-1 text-caption text-secondary">{t("settings.logs.description")}</p></div>
                <Button size="compact" loading={openingLogs} onClick={() => void openLogDir()}>{t("settings.logs.open")}</Button>
              </div>
              {logDir !== null && <p className="mt-3 break-all rounded-md bg-subtle p-3 text-caption text-secondary">{logDir}</p>}
              <InlineError error={logDirError} />
            </section>
            {schemaQuery.data !== undefined && <p className="border-t border-light pt-5 text-caption text-hint">{t("settings.schemaVersion", { version: schemaQuery.data })}</p>}
            <InlineError error={schemaQuery.error} />
          </div>}
        </div>
      </div>
    </Dialog>
  );
}

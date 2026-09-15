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

import { AiSettingsPage } from "../ai-settings/AiSettingsPage";
import { Button, Dialog, cn } from "../../ui";
import { qk } from "../../lib/events";
import {
  getDebugLogDir,
  getSchemaVersion,
  getSettings,
  isAppError,
  setLogLevel,
  setTheme,
  setWeekStartDay,
} from "../../lib/ipc";
import type { LogLevel, Settings, Theme } from "../../lib/ipc";
import { applyTheme } from "../../lib/theme";

/** 两个用户确认的浅色主题（design.md §4.4）；默认白底。 */
const THEME_OPTIONS: ReadonlyArray<{ value: Theme; label: string }> = [
  { value: "white", label: "白底" },
  { value: "gray", label: "灰底" },
];

/** 周一…周日 ↔ ISO 1…7，与 setWeekStartDay 的取值域一致。 */
const WEEK_DAY_LABELS = [
  "周一",
  "周二",
  "周三",
  "周四",
  "周五",
  "周六",
  "周日",
] as const;

const LOG_LEVELS: readonly LogLevel[] = ["error", "warn", "info", "debug"];

/* 原生 select 复用 Input 的表面样式（design.md 4.1/4.3）；箭头留给平台。 */
const SELECT_CLASS =
  "h-8 rounded-sm border border-control bg-content px-2 text-[14px] text-primary transition-colors duration-100";

/*
 * 主题示意小图（48×32、2px 圆角）：canvas 上承托一块 content 面板。
 * 预览本身就是两个主题的图例本体，允许字面色值；页面其余部分仍只
 * 消费语义 token（design.md §4.4、index.css）。
 */
function ThemePreview({ theme }: { theme: Theme }) {
  const canvas = theme === "gray" ? "#f6f6f6" : "#ffffff";
  const edge = theme === "gray" ? "#e3e3e3" : "#edf0f2";
  return (
    <div
      aria-hidden="true"
      className="rounded-[2px] p-[5px]"
      style={{ width: 48, height: 32, backgroundColor: canvas }}
    >
      <div
        className="h-full w-full rounded-[2px] bg-white"
        style={{ border: `1px solid ${edge}` }}
      />
    </div>
  );
}

/** 行内错误（AppError 优先取 message），不打断、不弹窗（design.md 9.2）。 */
function InlineError({ error }: { error: unknown }) {
  if (error === null || error === undefined) return null;
  const message = isAppError(error)
    ? error.message
    : error instanceof Error
      ? error.message
      : String(error);
  return <p className="text-caption text-danger">{message}</p>;
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
  const weekStartId = useId();
  const logLevelId = useId();

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

  // 后端没有日志级别回读命令，先显示 info；之后的取值以本地选择为准。
  const [logLevel, setLogLevelValue] = useState<LogLevel>("info");
  const logLevelMutation = useMutation({ mutationFn: setLogLevel });

  const [openingLogs, setOpeningLogs] = useState(false);
  const [logDir, setLogDir] = useState<string | null>(null);
  const [logDirError, setLogDirError] = useState<unknown>(null);
  // 模型设置二级弹窗：设置弹窗保持打开，AI 页在上层展示。
  const [aiSettingsOpen, setAiSettingsOpen] = useState(false);

  const selectedTheme = settingsQuery.data?.theme ?? "white";
  const weekStartDay = settingsQuery.data?.week_start_day ?? 1;

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
    <Dialog open={open} onClose={onClose} title="Settings">
      {/* 分区之间 24px（design.md 4.3 间距节奏）。 */}
      <div className="flex flex-col gap-6">
        <section className="flex flex-col gap-2">
          <h3 className="text-block-title font-semibold text-primary">Appearance</h3>
          <div className="flex gap-2">
            {THEME_OPTIONS.map((option) => (
              <button
                key={option.value}
                type="button"
                aria-pressed={selectedTheme === option.value}
                onClick={() => chooseTheme(option.value)}
                className={cn(
                  "flex flex-1 flex-col items-start gap-1.5 rounded-sm border p-2",
                  "transition-colors duration-100",
                  selectedTheme === option.value
                    ? "border-focus bg-focus-surface"
                    : "border-light hover:bg-hover",
                )}
              >
                <ThemePreview theme={option.value} />
                <span className="text-caption text-primary">{option.label}</span>
              </button>
            ))}
          </div>
          <InlineError error={themeMutation.error} />
        </section>

        <section className="flex flex-col gap-2">
          <h3 className="text-block-title font-semibold text-primary">General</h3>
          <div className="flex flex-col gap-1">
            <div className="flex items-center justify-between gap-3">
              <label htmlFor={weekStartId} className="text-caption text-secondary">
                周起始日
              </label>
              <select
                id={weekStartId}
                value={weekStartDay}
                onChange={(e) => chooseWeekStart(Number(e.currentTarget.value))}
                className={cn(SELECT_CLASS, "w-auto")}
              >
                {WEEK_DAY_LABELS.map((label, index) => (
                  <option key={label} value={index + 1}>
                    {label}
                  </option>
                ))}
              </select>
            </div>
            <InlineError error={weekStartMutation.error} />
          </div>
        </section>

        {/* AI 分区（§9.3）：先说明接入状态，管理入口打开模型设置页。 */}
        <section className="flex flex-col gap-2">
          <h3 className="text-block-title font-semibold text-primary">AI</h3>
          <p className="text-caption text-secondary">
            Bring your own key — requests go directly to your provider, and you
            pay your provider directly.
          </p>
          <Button
            variant="secondary"
            size="compact"
            onClick={() => setAiSettingsOpen(true)}
          >
            模型设置…
          </Button>
          {onPreviewExitPoll && (
            <Button
              variant="ghost"
              size="compact"
              onClick={onPreviewExitPoll}
            >
              反馈：退出调查…
            </Button>
          )}
        </section>

        <section className="flex flex-col gap-2">
          <h3 className="text-block-title font-semibold text-primary">Diagnostics</h3>
          <div className="flex flex-col gap-1">
            <div className="flex items-center justify-between gap-3">
              <label htmlFor={logLevelId} className="text-caption text-secondary">
                日志级别
              </label>
              <select
                id={logLevelId}
                value={logLevel}
                onChange={(e) => chooseLogLevel(e.currentTarget.value as LogLevel)}
                className={cn(SELECT_CLASS, "w-auto")}
              >
                {LOG_LEVELS.map((level) => (
                  <option key={level} value={level}>
                    {level}
                  </option>
                ))}
              </select>
            </div>
            <InlineError error={logLevelMutation.error} />
          </div>
          <div className="flex flex-col gap-1">
            <Button
              variant="secondary"
              size="compact"
              loading={openingLogs}
              onClick={() => void openLogDir()}
            >
              打开日志目录
            </Button>
            {logDir !== null && (
              <p className="break-all text-caption text-secondary">{logDir}</p>
            )}
            <InlineError error={logDirError} />
          </div>
          {schemaQuery.data !== undefined && (
            <p className="text-hint">Schema v{schemaQuery.data}</p>
          )}
        </section>
      </div>
      {aiSettingsOpen && (
        <Dialog
          open
          wide
          title="AI models"
          onClose={() => setAiSettingsOpen(false)}
        >
          <AiSettingsPage />
        </Dialog>
      )}
    </Dialog>
  );
}

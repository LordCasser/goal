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
  "h-9 rounded-md border border-control bg-content px-3 text-[14px] text-primary transition-colors duration-100";

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
  const [section, setSection] = useState<"general" | "ai" | "reminders" | "diagnostics">("general");

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
    <Dialog open={open} onClose={onClose} title="设置" wide className="max-w-[1040px]! h-[680px] max-h-[calc(100vh-64px)]" bodyClassName="overflow-hidden!">
      <div className="flex h-full gap-7 border-t border-light pt-5">
        <nav aria-label="设置分类" className="flex w-[136px] shrink-0 flex-col gap-1">
          {([
            ["general", "通用", "外观与规划偏好"],
            ["ai", "AI 模型", "供应商与连接"],
            ["reminders", "提醒", "时间与免打扰"],
            ["diagnostics", "诊断", "日志与应用信息"],
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
              <h3 className="text-section-title font-semibold text-primary">外观与偏好</h3>
              <p className="mt-1 text-body text-secondary">选择工作台底色与每周的开始时间。</p>
              <div className="mt-5 grid grid-cols-2 gap-3">
                {THEME_OPTIONS.map((option) => (
                  <button key={option.value} type="button" aria-label={option.label}
                    aria-pressed={selectedTheme === option.value}
                    disabled={settingsQuery.isPending || themeMutation.isPending}
                    onClick={() => chooseTheme(option.value)}
                    className={cn("overflow-hidden rounded-lg border p-2 text-left transition-colors", selectedTheme === option.value ? "border-focus" : "border-light hover:border-control")}>
                    <ThemePreview theme={option.value} />
                    <span className="flex items-center justify-between px-2 pb-1 pt-3 text-body font-medium text-primary">
                      {option.label}
                      <span aria-hidden="true" className={cn("flex h-4 w-4 items-center justify-center rounded-full border text-[10px]", selectedTheme === option.value ? "border-focus bg-focus text-white" : "border-control")}>{selectedTheme === option.value ? "✓" : ""}</span>
                    </span>
                    <span className="block px-2 pb-2 text-caption text-secondary">{option.value === "white" ? "轻盈留白，让内容成为主角" : "柔和灰底，衬托白色计划面"}</span>
                  </button>
                ))}
              </div>
              <InlineError error={themeMutation.error} />
            </section>
            <section className="border-t border-light pt-5">
              <div className="flex items-center justify-between gap-4">
                <div>
                  <label htmlFor={weekStartId} className="text-body font-medium text-primary">周起始日</label>
                  <p className="mt-1 text-caption text-secondary">用于日历与新建周计划。</p>
                </div>
                <select id={weekStartId} value={weekStartDay} disabled={settingsQuery.isPending || weekStartMutation.isPending}
                  onChange={(e) => chooseWeekStart(Number(e.currentTarget.value))} className={cn(SELECT_CLASS, "w-[116px]")}>
                  {WEEK_DAY_LABELS.map((label, index) => <option key={label} value={index + 1}>{label}</option>)}
                </select>
              </div>
              <InlineError error={weekStartMutation.error} />
            </section>
            {onPreviewExitPoll && <section className="flex items-center justify-between gap-4 border-t border-light pt-5">
              <div><h3 className="text-body font-medium text-primary">使用反馈</h3><p className="mt-1 text-caption text-secondary">记录是什么打断了这次专注。</p></div>
              <Button size="compact" onClick={onPreviewExitPoll}>填写反馈</Button>
            </section>}
            <InlineError error={settingsQuery.error} />
            <p className="text-caption text-hint" role="status">{themeMutation.isPending || weekStartMutation.isPending ? "正在保存…" : "更改自动保存"}</p>
          </div>}
          {section === "ai" && <AiSettingsPage />}
          {section === "reminders" && <div className="flex flex-col gap-6"><div><h3 className="text-section-title font-semibold text-primary">提醒与通知</h3><p className="mt-1 text-body text-secondary">安排提醒时间，为专注保留安静的空间。</p></div><ReminderSettingsSection /></div>}
          {section === "diagnostics" && <div className="flex flex-col gap-6">
            <div><h3 className="text-section-title font-semibold text-primary">诊断与日志</h3><p className="mt-1 text-body text-secondary">遇到问题时，在这里查看本地诊断信息。</p></div>
            <section className="border-t border-light pt-5">
              <div className="flex items-center justify-between gap-3">
                <div><label htmlFor={logLevelId} className="text-body font-medium text-primary">日志级别</label><p className="mt-1 text-caption text-secondary">调整当前运行期间的记录详细程度。</p></div>
                <select id={logLevelId} value={logLevel} onChange={(e) => chooseLogLevel(e.currentTarget.value as LogLevel)} className={cn(SELECT_CLASS, "w-[116px]")}>
                  {LOG_LEVELS.map((level) => <option key={level} value={level}>{level}</option>)}
                </select>
              </div>
              <InlineError error={logLevelMutation.error} />
            </section>
            <section className="border-t border-light pt-5">
              <div className="flex items-center justify-between gap-3">
                <div><h3 className="text-body font-medium text-primary">应用日志</h3><p className="mt-1 text-caption text-secondary">在 Finder 中打开日志所在文件夹。</p></div>
                <Button size="compact" loading={openingLogs} onClick={() => void openLogDir()}>打开日志目录</Button>
              </div>
              {logDir !== null && <p className="mt-3 break-all rounded-md bg-subtle p-3 text-caption text-secondary">{logDir}</p>}
              <InlineError error={logDirError} />
            </section>
            {schemaQuery.data !== undefined && <p className="border-t border-light pt-5 text-caption text-hint">Schema v{schemaQuery.data}</p>}
            <InlineError error={schemaQuery.error} />
          </div>}
        </div>
      </div>
    </Dialog>
  );
}

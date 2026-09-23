/**
 * Reminders feature IPC (change: add-reminders-notifications §5).
 *
 * 本模块直连 @tauri-apps/api/core——src/lib 由协调者持有，本 feature 自带
 * 一份命令名清单与查询键工厂。命令名与 src-tauri/src/commands/reminders.rs
 * 一一对应；invoke 顶层参数键使用 Tauri 默认的 camelCase，嵌套 serde 数据保留 snake_case，整结构参数（args）按参数名整体嵌套。
 *
 * 错误形状是 `{ code, message }`（见 src/lib/ipc.ts 的 isAppError）；本
 * feature 的错误码：invalid_target_kind / not_found / reminder_already_fired /
 * invalid_quiet_hours / invalid_daily_plan_time / invalid_status_filter。
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect } from "react";
import type { QueryClient } from "@tanstack/react-query";
import { formatDate } from "../../lib/i18n";

export type TargetKind = "task" | "session" | "day" | "cycle";

/** "pending" = 未触发；"fired" = 已到期未关闭（应用内提示面）；"all"。 */
export type ReminderStatusFilter = "pending" | "fired" | "all";

/** Mirrors `repository::reminders::Reminder`. */
export interface Reminder {
  id: string;
  target_kind: TargetKind;
  target_id: string;
  /** Resolved display title in list responses; unset for mutation responses. */
  title?: string | null;
  /** 毫秒时间戳（Unix epoch）——计划触发点。 */
  fire_at: number;
  /** 是否允许在免打扰时段静音系统通知（true = 软提醒）。 */
  quiet_ok: boolean;
  fired_at: number | null;
  dismissed_at: number | null;
  created_at: number;
}

/** `HH:MM` 边界；start > end 表示跨午夜。 */
export interface QuietHours {
  start: string;
  end: string;
}

/** Mirrors `service::reminders::ReminderSettings`. 全量替换语义。 */
export interface ReminderSettings {
  quiet_hours: QuietHours | null;
  daily_plan_time: string | null;
}

export interface MissedItem {
  id: string;
  target_kind: TargetKind;
  target_id: string;
  title: string | null;
  fire_at: number;
}

/** Mirrors `service::reminders::MissedSummary`. ids 覆盖全部错过的提醒。 */
export interface MissedSummary {
  total: number;
  items: MissedItem[];
  has_more: boolean;
  ids: string[];
}

/** Mirrors `service::reminders::DeliveryStatus`. */
export interface DeliveryStatus {
  permission: "granted" | "denied" | "prompt" | "system_managed" | null;
  last_error: string | null;
  last_delivery_at: number | null;
}

export interface SetReminderArgs {
  target_kind: TargetKind;
  target_id: string;
  fire_at: number;
  quiet_ok: boolean;
}

export interface UpdateReminderArgs {
  fire_at: number;
  /** 缺省保持原值。 */
  quiet_ok?: boolean | null;
}

export interface ReminderSettingsArgs {
  quiet_hours: QuietHours | null;
  daily_plan_time: string | null;
}

export const commands = {
  setReminder: "set_reminder",
  listReminders: "list_reminders",
  updateReminder: "update_reminder",
  deleteReminder: "delete_reminder",
  reconcileReminders: "reconcile_reminders",
  getMissedSummary: "get_missed_summary",
  acknowledgeMissedSummary: "acknowledge_missed_summary",
  getReminderSettings: "get_reminder_settings",
  setReminderSettings: "set_reminder_settings",
  requestNotificationPermission: "request_notification_permission",
  getNotificationPermission: "get_notification_permission",
} as const;

/** 后端投递/关闭后的失效通知；payload 为空（仅通知，不带业务数据）。 */
export const REMINDERS_CHANGED = "reminders:changed";

// --- 本 feature 的查询键（勿与 lib/events.ts 的 qk 混用） -------------------

export const reminderKeys = {
  list: (cycleId: string | null, status: ReminderStatusFilter) =>
    ["reminders", cycleId, status] as const,
  settings: () => ["reminder-settings"] as const,
  permission: () => ["notification-permission"] as const,
  missedSummary: () => ["missed-reminders"] as const,
};

/** 让本 feature 的全部提醒查询失效（收到 reminders:changed 时）。 */
function invalidateReminders(queryClient: QueryClient): void {
  void queryClient.invalidateQueries({ queryKey: ["reminders"] });
  void queryClient.invalidateQueries({ queryKey: reminderKeys.missedSummary() });
}

/**
 * 订阅 reminders:changed；返回清理函数。装载失败（非 Tauri 环境）只失去
 * 主动刷新，不影响查询本身。
 */
export function useRemindersChanged(queryClient: QueryClient): void {
  useEffect(() => {
    let disposed = false;
    let unlisten: UnlistenFn | null = null;
    listen(REMINDERS_CHANGED, () => invalidateReminders(queryClient))
      .then((fn) => {
        if (disposed) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // 无事件通道时 UI 靠焦点对账兜底。
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [queryClient]);
}

/**
 * 对账入口（spec: 触发与投递——不依赖定时器精度）：应用唤醒与窗口聚焦时
 * 触发后端立即重查到期队列。挂载与 focus/visibilitychange 都会触发。
 */
export function useReminderReconcile(enabled = true): void {
  useEffect(() => {
    if (!enabled) return;
    const reconcile = () => {
      void invoke(commands.reconcileReminders).catch(() => {
        // 后端未接线（如测试环境）时静默；对账是尽力而为。
      });
    };
    reconcile();
    window.addEventListener("focus", reconcile);
    document.addEventListener("visibilitychange", reconcile);
    return () => {
      window.removeEventListener("focus", reconcile);
      document.removeEventListener("visibilitychange", reconcile);
    };
  }, [enabled]);
}

// --- 命令包装 ---------------------------------------------------------------

export function setReminder(args: SetReminderArgs): Promise<Reminder> {
  return invoke<Reminder>(commands.setReminder, { args });
}

export function listReminders(
  cycle_id: string | null,
  status: ReminderStatusFilter = "pending",
): Promise<Reminder[]> {
  return invoke<Reminder[]>(commands.listReminders, { cycleId: cycle_id, status });
}

export function updateReminder(
  reminder_id: string,
  args: UpdateReminderArgs,
): Promise<Reminder> {
  return invoke<Reminder>(commands.updateReminder, { reminderId: reminder_id, args });
}

export function deleteReminder(reminder_id: string): Promise<void> {
  return invoke<void>(commands.deleteReminder, { reminderId: reminder_id });
}

export function reconcileReminders(): Promise<void> {
  return invoke<void>(commands.reconcileReminders);
}

export function getMissedSummary(): Promise<MissedSummary> {
  return invoke<MissedSummary>(commands.getMissedSummary);
}

/** 关闭启动摘要（或单条应用内提示）：ids 全部标记已处理。 */
export function acknowledgeMissedSummary(ids: string[]): Promise<number> {
  return invoke<number>(commands.acknowledgeMissedSummary, { ids });
}

export function getReminderSettings(): Promise<ReminderSettings> {
  return invoke<ReminderSettings>(commands.getReminderSettings);
}

export function setReminderSettings(
  args: ReminderSettingsArgs,
): Promise<void> {
  return invoke<void>(commands.setReminderSettings, { args });
}

export function requestNotificationPermission(): Promise<string> {
  return invoke<string>(commands.requestNotificationPermission);
}

export function getNotificationPermission(): Promise<DeliveryStatus> {
  return invoke<DeliveryStatus>(commands.getNotificationPermission);
}

// --- 小工具 -----------------------------------------------------------------

/** ms → datetime-local 的本地时间字符串（秒截断）。无效输入返回空串。 */
export function toLocalInputValue(fireAt: number): string {
  const date = new Date(fireAt);
  if (Number.isNaN(date.getTime())) return "";
  const pad = (n: number) => String(n).padStart(2, "0");
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}` +
    `T${pad(date.getHours())}:${pad(date.getMinutes())}`
  );
}

/** datetime-local 值 → ms；无效（含空串）返回 null。 */
export function fromLocalInputValue(value: string): number | null {
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}$/.test(value)) return null;
  const ms = new Date(value).getTime();
  return Number.isNaN(ms) || toLocalInputValue(ms) !== value ? null : ms;
}

export function formatFireAt(fireAt: number): string {
  return formatDate(fireAt, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

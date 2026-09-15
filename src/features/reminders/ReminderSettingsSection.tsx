/**
 * 免打扰与通知权限设置区（change: add-reminders-notifications §5.4）。
 *
 * quiet_hours / daily_plan_time 走 set_reminder_settings 的全量替换语义
 * （读-改-写完整状态）；权限状态来自 get_notification_permission（缓存值 +
 * 最近一次投递失败），被拒时给出系统设置的恢复指引（spec: 权限被拒绝）。
 */
import { useEffect, useState } from "react";
import type { JSX } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { Button, Checkbox, Input } from "../../ui";
import {
  getNotificationPermission,
  getReminderSettings,
  reminderKeys,
  requestNotificationPermission,
  setReminderSettings,
} from "./api";

export function ReminderSettingsSection(): JSX.Element {
  const queryClient = useQueryClient();
  const settingsQuery = useQuery({
    queryKey: reminderKeys.settings(),
    queryFn: getReminderSettings,
  });
  const permissionQuery = useQuery({
    queryKey: reminderKeys.permission(),
    queryFn: getNotificationPermission,
  });

  // 本地编辑态；查询返回后同步一次（保存失败回滚由 invalidate 兜底）。
  const [quietEnabled, setQuietEnabled] = useState(false);
  const [quietStart, setQuietStart] = useState("23:00");
  const [quietEnd, setQuietEnd] = useState("07:00");
  const [dailyEnabled, setDailyEnabled] = useState(false);
  const [dailyTime, setDailyTime] = useState("08:30");

  useEffect(() => {
    const data = settingsQuery.data;
    if (!data) return;
    setQuietEnabled(data.quiet_hours !== null);
    if (data.quiet_hours) {
      setQuietStart(data.quiet_hours.start);
      setQuietEnd(data.quiet_hours.end);
    }
    setDailyEnabled(data.daily_plan_time !== null);
    if (data.daily_plan_time) setDailyTime(data.daily_plan_time);
  }, [settingsQuery.data]);

  const save = useMutation({
    mutationFn: () =>
      setReminderSettings({
        quiet_hours: quietEnabled ? { start: quietStart, end: quietEnd } : null,
        daily_plan_time: dailyEnabled ? dailyTime : null,
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: reminderKeys.settings() });
    },
  });

  const requestPermission = useMutation({
    mutationFn: requestNotificationPermission,
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: reminderKeys.permission(),
      });
    },
  });

  const permission = permissionQuery.data?.permission ?? null;
  const lastError = permissionQuery.data?.last_error ?? null;

  return (
    <section aria-label="提醒设置" className="flex flex-col gap-3">
      <div className="flex flex-col gap-1">
        <Checkbox checked={quietEnabled} onChange={setQuietEnabled}>
          <span className="text-caption text-primary">免打扰时段</span>
        </Checkbox>
        {quietEnabled && (
          <div className="flex items-center gap-2">
            <Input
              type="time"
              aria-label="免打扰开始"
              value={quietStart}
              onChange={(event) => setQuietStart(event.currentTarget.value)}
              className="w-auto"
            />
            <span className="text-caption text-secondary">至</span>
            <Input
              type="time"
              aria-label="免打扰结束"
              value={quietEnd}
              onChange={(event) => setQuietEnd(event.currentTarget.value)}
              className="w-auto"
            />
          </div>
        )}
        <p className="text-hint text-caption">
          该时段内到期的软提醒不弹系统通知，但仍在应用内可见。
        </p>
      </div>

      <div className="flex flex-col gap-1">
        <Checkbox checked={dailyEnabled} onChange={setDailyEnabled}>
          <span className="text-caption text-primary">每日计划提醒</span>
        </Checkbox>
        {dailyEnabled && (
          <Input
            type="time"
            aria-label="每日计划提醒时间"
            value={dailyTime}
            onChange={(event) => setDailyTime(event.currentTarget.value)}
            className="w-auto"
          />
        )}
      </div>

      <div className="flex items-center gap-2">
        <Button
          variant="secondary"
          size="compact"
          loading={save.isPending}
          onClick={() => save.mutate()}
        >
          保存提醒设置
        </Button>
        {save.isError && <span className="text-caption text-danger">保存失败</span>}
        {save.isSuccess && !save.isPending && (
          <span role="status" className="text-caption text-hint">
            已保存
          </span>
        )}
      </div>

      <div className="flex flex-col gap-1 border-t border-light pt-2">
        <p className="text-caption text-secondary">
          通知权限：
          {permission === "granted"
            ? "已授予"
            : permission === "denied"
              ? "被拒绝"
              : "未请求"}
        </p>
        {permission !== "granted" && (
          <>
            <Button
              variant="secondary"
              size="compact"
              loading={requestPermission.isPending}
              onClick={() => requestPermission.mutate()}
            >
              请求通知权限
            </Button>
            {permission === "denied" && (
              <p className="text-caption text-danger">
                通知权限被拒绝：请在系统设置 → 通知中允许本应用后重试。应用内
                提醒不受影响，到期仍会出现在提醒列表。
              </p>
            )}
          </>
        )}
        {lastError && (
          <p className="text-caption text-danger">最近一次投递失败：{lastError}</p>
        )}
      </div>
    </section>
  );
}

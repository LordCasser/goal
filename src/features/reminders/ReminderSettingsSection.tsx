/**
 * 免打扰与通知权限设置区（change: add-reminders-notifications §5.4）。
 *
 * quiet_hours / daily_plan_time 走 set_reminder_settings 的全量替换语义
 * （读-改-写完整状态）；权限状态来自 get_notification_permission（平台状态 +
 * 最近一次同步投递失败），桌面无法查询时显示由系统管理。
 */
import { useEffect, useState } from "react";
import type { JSX } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { Button, Checkbox, TimePicker } from "../../ui";
import { errorMessage, useTranslation } from "../../lib/i18n";
import {
  getNotificationPermission,
  getReminderSettings,
  reminderKeys,
  requestNotificationPermission,
  setReminderSettings,
} from "./api";

export function ReminderSettingsSection(): JSX.Element {
  const { t } = useTranslation("shell");
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
    mutationFn: setReminderSettings,
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
  const validTime = (value: string) => /^([01]\d|2[0-3]):[0-5]\d$/.test(value);
  const quietStartValid = validTime(quietStart);
  const quietEndValid = validTime(quietEnd);
  const dailyTimeValid = validTime(dailyTime);

  return (
    <section aria-label={t("reminders.settings.sectionLabel")} className="flex flex-col gap-6">
      <div className="flex flex-col gap-3">
        <Checkbox checked={quietEnabled} onChange={setQuietEnabled}>
          <span className="text-body font-medium text-primary">{t("reminders.settings.quietHours")}</span>
        </Checkbox>
        {quietEnabled && (
          <div className="flex items-center gap-2">
            <TimePicker
              aria-label={t("reminders.settings.quietStart")}
              aria-invalid={!quietStartValid}
              value={quietStart}
              onChange={setQuietStart}
              className="w-28"
            />
            <span className="text-caption text-secondary">{t("reminders.settings.to")}</span>
            <TimePicker
              aria-label={t("reminders.settings.quietEnd")}
              aria-invalid={!quietEndValid}
              value={quietEnd}
              onChange={setQuietEnd}
              className="w-28"
            />
          </div>
        )}
        <p className="text-hint text-caption">
          {t("reminders.settings.quietDescription")}
        </p>
      </div>

      <div className="flex flex-col gap-3">
        <Checkbox checked={dailyEnabled} onChange={setDailyEnabled}>
          <span className="text-body font-medium text-primary">{t("reminders.settings.dailyPlan")}</span>
        </Checkbox>
        {dailyEnabled && (
          <TimePicker
            aria-label={t("reminders.settings.dailyPlanTime")}
            aria-invalid={!dailyTimeValid}
            value={dailyTime}
            onChange={setDailyTime}
            className="w-28"
          />
        )}
      </div>

      <div className="flex items-center gap-2">
        <Button
          variant="secondary"
          size="compact"
          loading={save.isPending}
          disabled={settingsQuery.isPending || (quietEnabled && (!quietStartValid || !quietEndValid)) || (dailyEnabled && !dailyTimeValid)}
          onClick={() => save.mutate({
            quiet_hours: quietEnabled ? { start: quietStart, end: quietEnd } : null,
            daily_plan_time: dailyEnabled ? dailyTime : null,
          })}
        >
          {t("reminders.settings.save")}
        </Button>
        {save.isError && <span className="text-caption text-danger">{errorMessage(save.error)}</span>}
        {save.isSuccess && !save.isPending && (
          <span role="status" className="text-caption text-hint">
            {t("reminders.settings.saved")}
          </span>
        )}
      </div>

      <div className="flex flex-col items-start gap-3 border-t border-light pt-5">
        <p className="text-caption text-secondary">
          {t("reminders.settings.permission")}
          {permission === "granted"
            ? t("reminders.settings.permission.granted")
            : permission === "denied"
              ? t("reminders.settings.permission.denied")
              : permission === "system_managed"
                ? t("reminders.settings.permission.systemManaged")
                : t("reminders.settings.permission.prompt")}
        </p>
        {permission !== "granted" && permission !== "system_managed" && (
          <>
            <Button
              variant="secondary"
              size="compact"
              loading={requestPermission.isPending}
              onClick={() => requestPermission.mutate()}
            >
              {t("reminders.settings.permission.request")}
            </Button>
            {permission === "denied" && (
              <p className="text-caption text-danger">
                {t("reminders.settings.permission.deniedDescription")}
              </p>
            )}
          </>
        )}
        {permission === "system_managed" && (
          <p className="text-caption text-secondary">
            {t("reminders.settings.permission.systemManagedDescription")}
          </p>
        )}
        {lastError && (
          <p className="text-caption text-danger">{t("reminders.settings.deliveryFailed", { error: lastError })}</p>
        )}
      </div>
    </section>
  );
}

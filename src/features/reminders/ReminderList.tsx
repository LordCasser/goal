/**
 * 提醒列表（change: add-reminders-notifications §5.3）。
 *
 * 展示尚未触发的提醒（按触发时间升序），支持改时间与删除；另设「已到期
 * 待查看」区展示 fired-but-not-dismissed 的应用内提示（免打扰/权限降级都
 * 落在这里，spec: 免打扰结束后查看、权限被拒绝）。
 *
 * 数据新鲜度：挂载/聚焦时 reconcile_reminders 对账（不依赖定时器精度），
 * reminders:changed 事件失效缓存。
 */
import { useState } from "react";
import type { JSX } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { Button } from "../../ui";
import { useTranslation } from "../../lib/i18n";
import {
  acknowledgeMissedSummary,
  deleteReminder,
  formatFireAt,
  listReminders,
  reminderKeys,
  useReminderReconcile,
  useRemindersChanged,
  type Reminder,
} from "./api";
import { ReminderPicker } from "./ReminderPicker";

function reminderTitle(t: (key: string, options?: Record<string, unknown>) => string, reminder: Reminder): string {
  return reminder.title?.trim() || t(`reminders.target.${reminder.target_kind}`);
}

export function ReminderList({ cycleId }: { cycleId?: string }): JSX.Element {
  const { t } = useTranslation("shell");
  const queryClient = useQueryClient();
  useReminderReconcile();
  useRemindersChanged(queryClient);

  const scope = cycleId ?? null;
  const pendingQuery = useQuery({
    queryKey: reminderKeys.list(scope, "pending"),
    queryFn: () => listReminders(scope, "pending"),
  });
  const alertsQuery = useQuery({
    queryKey: reminderKeys.list(scope, "fired"),
    queryFn: () => listReminders(scope, "fired"),
  });

  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: ["reminders"] });
  };
  const remove = useMutation({
    mutationFn: (id: string) => deleteReminder(id),
    onSuccess: invalidate,
  });
  const dismiss = useMutation({
    mutationFn: (id: string) => acknowledgeMissedSummary([id]),
    onSuccess: invalidate,
  });

  // 防御性排序：后端已按 fire_at 升序返回，这里保证展示契约不依赖实现。
  const pending = [...(pendingQuery.data ?? [])].sort((a, b) => a.fire_at - b.fire_at);
  const alerts = alertsQuery.data ?? [];

  return (
    <section aria-label={t("reminders.sectionLabel")} className="flex flex-col gap-2">
      {pending.length === 0 ? (
        <p className="text-caption text-hint">{t("reminders.empty")}</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {pending.map((reminder) => (
            <ReminderRow
              key={reminder.id}
              reminder={reminder}
              onDelete={() => remove.mutate(reminder.id)}
              onChanged={invalidate}
            />
          ))}
        </ul>
      )}

      {alerts.length > 0 && (
        <div className="flex flex-col gap-1 rounded-sm border border-light bg-content p-2">
          <p className="text-caption text-secondary">{t("reminders.firedTitle")}</p>
          <ul className="flex flex-col gap-1">
            {alerts.map((reminder) => (
              <li
                key={reminder.id}
                className="flex items-center justify-between gap-2"
              >
                <span className="text-caption text-primary">
                  {reminderTitle(t, reminder)} · {formatFireAt(reminder.fire_at)}
                </span>
                <Button
                  variant="ghost"
                  size="compact"
                  aria-label={t("reminders.acknowledge")}
                  onClick={() => dismiss.mutate(reminder.id)}
                >
                  {t("reminders.acknowledge")}
                </Button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </section>
  );
}

function ReminderRow({
  reminder,
  onDelete,
  onChanged,
}: {
  reminder: Reminder;
  onDelete: () => void;
  onChanged: () => void;
}): JSX.Element {
  const { t } = useTranslation("shell");
  const [editing, setEditing] = useState(false);

  if (editing) {
    return (
      <li>
        <ReminderPicker
          target_kind={reminder.target_kind}
          target_id={reminder.target_id}
          reminderId={reminder.id}
          initialFireAt={reminder.fire_at}
          initialQuietOk={reminder.quiet_ok}
          onSaved={() => {
            // 旧触发时间已被 update_reminder 覆盖，失效后按新时间重取。
            onChanged();
            setEditing(false);
          }}
          onCancel={() => setEditing(false)}
        />
      </li>
    );
  }

  return (
    <li className="flex items-center justify-between gap-2 rounded-sm px-1 hover:bg-hover">
      <span className="min-w-0 truncate text-caption text-primary">
        {reminderTitle(t, reminder)}
        {reminder.quiet_ok && (
          <span className="ml-1 text-hint">{t("reminders.quietMuted")}</span>
        )}
      </span>
      <span className="flex shrink-0 items-center gap-1">
        <time className="text-caption text-secondary">
          {formatFireAt(reminder.fire_at)}
        </time>
        <Button
          variant="ghost"
          size="compact"
          aria-label={t("reminders.editTime", { id: reminder.id })}
          onClick={() => setEditing(true)}
        >
          {t("reminders.edit")}
        </Button>
        <Button
          variant="ghost"
          size="compact"
          aria-label={t("reminders.delete", { id: reminder.id })}
          onClick={onDelete}
        >
          {t("reminders.deleteAction")}
        </Button>
      </span>
    </li>
  );
}

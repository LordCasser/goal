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
import {
  acknowledgeMissedSummary,
  deleteReminder,
  formatFireAt,
  listReminders,
  reminderKeys,
  useReminderReconcile,
  useRemindersChanged,
  type Reminder,
  type TargetKind,
} from "./api";
import { ReminderPicker } from "./ReminderPicker";

const TARGET_KIND_LABEL: Record<TargetKind, string> = {
  task: "任务",
  session: "专注块",
  day: "日计划",
  cycle: "周期",
};

function reminderTitle(reminder: Reminder): string {
  return `${TARGET_KIND_LABEL[reminder.target_kind]} · ${reminder.target_id.slice(0, 8)}`;
}

export function ReminderList({ cycleId }: { cycleId?: string }): JSX.Element {
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
    <section aria-label="提醒" className="flex flex-col gap-2">
      {pending.length === 0 ? (
        <p className="text-caption text-hint">暂无提醒</p>
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
          <p className="text-caption text-secondary">已到期，待查看</p>
          <ul className="flex flex-col gap-1">
            {alerts.map((reminder) => (
              <li
                key={reminder.id}
                className="flex items-center justify-between gap-2"
              >
                <span className="text-caption text-primary">
                  {reminderTitle(reminder)} · {formatFireAt(reminder.fire_at)}
                </span>
                <Button
                  variant="ghost"
                  size="compact"
                  aria-label="知道了"
                  onClick={() => dismiss.mutate(reminder.id)}
                >
                  知道了
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
        {reminderTitle(reminder)}
        {reminder.quiet_ok && (
          <span className="ml-1 text-hint">（免打扰时段静音）</span>
        )}
      </span>
      <span className="flex shrink-0 items-center gap-1">
        <time className="text-caption text-secondary">
          {formatFireAt(reminder.fire_at)}
        </time>
        <Button
          variant="ghost"
          size="compact"
          aria-label={`修改提醒时间 ${reminder.id}`}
          onClick={() => setEditing(true)}
        >
          改时间
        </Button>
        <Button
          variant="ghost"
          size="compact"
          aria-label={`删除提醒 ${reminder.id}`}
          onClick={onDelete}
        >
          删除
        </Button>
      </span>
    </li>
  );
}

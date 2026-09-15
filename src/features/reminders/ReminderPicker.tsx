/**
 * 提醒时间选择与保存（change: add-reminders-notifications §5.1）。
 *
 * 挂在任务与专注块条目上：选一个本地时间点，保存为一条指向该目标的提醒。
 * 同一目标同一时刻重复保存由后端复用既有行（spec: 重复设定同一提醒），
 * 这里只管把 `fire_at` 与静音偏好交给 set_reminder。
 */
import { useState } from "react";
import type { JSX } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { Button, Checkbox, Input, cn } from "../../ui";
import { errorMessage, useTranslation } from "../../lib/i18n";
import {
  fromLocalInputValue,
  setReminder,
  toLocalInputValue,
  updateReminder,
  type Reminder,
  type TargetKind,
} from "./api";

export interface ReminderPickerProps {
  target_kind: TargetKind;
  target_id: string;
  /** 传入即走 update_reminder（改时间，旧时间失效）；缺省走 set_reminder。 */
  reminderId?: string;
  /** 预填触发时间（ms）；缺省 = 一小时后。 */
  initialFireAt?: number;
  /** 缺省 false：用户手动设的是硬时间点，免打扰时段也弹（spec: 不可免打扰的提醒）。 */
  initialQuietOk?: boolean;
  onSaved?: (reminder: Reminder) => void;
  onCancel?: () => void;
}

export function ReminderPicker({
  target_kind,
  target_id,
  reminderId,
  initialFireAt,
  initialQuietOk = false,
  onSaved,
  onCancel,
}: ReminderPickerProps): JSX.Element {
  const { t } = useTranslation("shell");
  const queryClient = useQueryClient();
  const [value, setValue] = useState(() =>
    toLocalInputValue(initialFireAt ?? Date.now() + 60 * 60 * 1000),
  );
  const [quietOk, setQuietOk] = useState(initialQuietOk);

  const fireAt = fromLocalInputValue(value);
  const save = useMutation({
    mutationFn: () => {
      if (fireAt === null) return Promise.reject(new Error("invalid time"));
      return reminderId !== undefined
        ? updateReminder(reminderId, { fire_at: fireAt, quiet_ok: quietOk })
        : setReminder({ target_kind, target_id, fire_at: fireAt, quiet_ok: quietOk });
    },
    onSuccess: (reminder) => {
      void queryClient.invalidateQueries({ queryKey: ["reminders"] });
      onSaved?.(reminder);
    },
  });

  return (
    <div className="flex flex-col gap-2 rounded-sm border border-light bg-content p-2">
      <label className="flex flex-col gap-1 text-caption text-secondary">
        {t("reminders.picker.time")}
        <Input
          type="datetime-local"
          value={value}
          onChange={(event) => setValue(event.currentTarget.value)}
          aria-label={t("reminders.picker.time")}
          className="w-auto"
        />
      </label>
      <Checkbox checked={quietOk} onChange={setQuietOk}>
        <span className="text-caption text-secondary">{t("reminders.picker.quiet")}</span>
      </Checkbox>
      <div className="flex items-center gap-2">
        <Button
          variant="primary"
          size="compact"
          disabled={fireAt === null}
          loading={save.isPending}
          onClick={() => save.mutate()}
        >
          {t("reminders.picker.save")}
        </Button>
        {onCancel && (
          <Button variant="ghost" size="compact" onClick={onCancel}>
            {t("reminders.picker.cancel")}
          </Button>
        )}
        <span
          role="status"
          className={cn("text-caption", save.isError ? "text-danger" : "text-hint")}
        >
          {save.isError ? errorMessage(save.error) : ""}
        </span>
      </div>
    </div>
  );
}

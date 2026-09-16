/**
 * 启动补偿摘要（change: add-reminders-notifications §5.5）。
 *
 * 挂载时调用 get_missed_summary——第一次调用即完成「收集并标记」；后端缓存
 * 同一份摘要，StrictMode 双挂载与聚焦重取都不会重复计数或弄丢摘要。
 * total = 0 不渲染任何东西（spec: 没有错过提醒）。关闭 → acknowledge 把
 * 全部 ids（含折叠的）标记已处理（spec: 摘要被关闭）。
 */
import { useState } from "react";
import type { JSX } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { Button } from "../../ui";
import { useTranslation } from "../../lib/i18n";
import {
  acknowledgeMissedSummary,
  formatFireAt,
  getMissedSummary,
  reminderKeys,
  useRemindersChanged,
} from "./api";

export function MissedSummary(): JSX.Element | null {
  const { t } = useTranslation("shell");
  const queryClient = useQueryClient();
  const [expanded, setExpanded] = useState(false);
  useRemindersChanged(queryClient);

  const summaryQuery = useQuery({
    queryKey: reminderKeys.missedSummary(),
    queryFn: getMissedSummary,
  });
  const summary = summaryQuery.data;

  const acknowledge = useMutation({
    mutationFn: () => acknowledgeMissedSummary(summary?.ids ?? []),
    onSuccess: () => {
      void queryClient.invalidateQueries({
        queryKey: reminderKeys.missedSummary(),
      });
      void queryClient.invalidateQueries({ queryKey: ["reminders"] });
    },
  });

  if (!summary || summary.total === 0) return null;

  return (
    <aside
      aria-label={t("reminders.missed.aria")}
      className="flex flex-col gap-2 rounded-sm border border-light bg-content p-3"
    >
      <div className="flex items-center justify-between gap-2">
        <p className="text-caption text-primary">
          {t("reminders.missed.count", { count: summary.total })}
        </p>
        <span className="flex shrink-0 gap-1">
          <Button
            variant="ghost"
            size="compact"
            aria-expanded={expanded}
            onClick={() => setExpanded((value) => !value)}
          >
            {expanded ? t("reminders.missed.collapse") : t("reminders.missed.expand")}
          </Button>
          <Button
            variant="secondary"
            size="compact"
            loading={acknowledge.isPending}
            onClick={() => acknowledge.mutate()}
          >
            {t("reminders.missed.markAllRead")}
          </Button>
        </span>
      </div>
      {expanded && (
        <ul className="flex flex-col gap-1">
          {summary.items.map((item) => (
            <li key={item.id} className="text-caption text-secondary">
              {t(`reminders.target.${item.target_kind}`)} ·{" "}
              {item.title ?? item.target_id.slice(0, 8)} ·{" "}
              {formatFireAt(item.fire_at)}
            </li>
          ))}
          {summary.has_more && (
            <li className="text-caption text-hint">
              {t("reminders.missed.remaining", { count: summary.total - summary.items.length })}
            </li>
          )}
        </ul>
      )}
      {acknowledge.isError && (
        <p className="text-caption text-danger">{t("reminders.missed.actionError")}</p>
      )}
    </aside>
  );
}

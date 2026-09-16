/**
 * 跨周期汇总视图（change: add-review-retrospective §6.5 / spec 跨周期汇总）。
 *
 * 只基于已提交的复盘快照（get_review_summary），按快照时间顺序展示各周期的
 * 完成率与专注时长。只有一份复盘时退化为单点展示并说明趋势需要更多数据，
 * 不画误导性的折线/柱状对比；多份复盘也只用朴素的水平比例条，不伪造坐标轴。
 *
 * 覆盖视图定位：本组件只渲染内容，宿主负责以可关闭的覆盖层挂载它
 * （planner-workspace 汇总视图入口），关闭后回到原工作台状态。
 */

import type * as React from "react";
import { useQuery } from "@tanstack/react-query";
import { getReviewSummary, reviewQk } from "./api";
import { formatPercent } from "./format";
import { formatDate, formatDuration, useTranslation } from "../../lib/i18n";

export function ReviewTrend(): React.JSX.Element {
  const { t } = useTranslation("ai");
  const query = useQuery({
    queryKey: reviewQk.reviewSummary(),
    queryFn: getReviewSummary,
    retry: false,
  });

  if (query.isPending) {
    return (
      <section aria-label={t("trend.label")} className="text-body text-secondary">
        {t("trend.loading")}
      </section>
    );
  }

  const points = query.data ?? [];

  if (points.length === 0) {
    return (
      <section aria-label={t("trend.label")} className="text-body text-secondary">
        {t("trend.empty")}
      </section>
    );
  }

  // 单份复盘：单点数值 + 说明，不渲染对比图（spec: 不显示误导性的折线）。
  const single = points.length === 1 ? points[0] : undefined;
  if (single) {
    return (
      <section aria-label={t("trend.label")} className="flex flex-col gap-2">
        <h3 className="text-block-title font-semibold text-primary">{t("trend.label")}</h3>
        <SinglePoint point={single} />
        <p className="text-menu text-secondary">
          {t("trend.onlyOne")}
        </p>
      </section>
    );
  }

  return (
    <section aria-label={t("trend.label")} className="flex flex-col gap-2">
      <h3 className="text-block-title font-semibold text-primary">{t("trend.label")}</h3>
      <p className="text-caption text-hint">{t("trend.oldestFirst")}</p>
      <ul className="flex flex-col gap-2">
        {points.map((point) => (
          <li key={point.cycle_id} className="flex flex-col gap-1">
            <div className="flex items-baseline justify-between gap-3">
              <span className="truncate text-body text-primary">{point.cycle_title}</span>
              <span className="shrink-0 text-menu text-secondary">
                {point.completion_rate == null
                  ? t("trend.noContent")
                  : formatPercent(point.completion_rate)}{" "}
                · {formatDuration(point.focused_time_ms)}
              </span>
            </div>
            <div
              className="h-1.5 w-full rounded-sm bg-subtle"
              role="presentation"
            >
              <div
                className="h-full rounded-sm bg-primary"
                style={{
                  width: `${Math.round((point.completion_rate ?? 0) * 100)}%`,
                }}
              />
            </div>
            <span className="text-caption text-hint">
              {formatDate(point.snapshot_at, { dateStyle: "medium", timeStyle: "short" })}
              {point.is_final ? "" : ` · ${t("trend.interim")}`}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

function SinglePoint({
  point,
}: {
  point: { cycle_title: string; completion_rate: number | null; focused_time_ms: number };
}): React.JSX.Element {
  const { t } = useTranslation("ai");
  return (
    <dl className="rounded-sm border border-light px-3 py-2 text-body text-primary">
      <div className="flex justify-between gap-3 py-0.5">
        <dt className="text-secondary">{t("trend.cycle")}</dt>
        <dd>{point.cycle_title}</dd>
      </div>
      <div className="flex justify-between gap-3 py-0.5">
        <dt className="text-secondary">{t("trend.completion")}</dt>
        <dd>
          {point.completion_rate == null ? t("trend.noContent") : formatPercent(point.completion_rate)}
        </dd>
      </div>
      <div className="flex justify-between gap-3 py-0.5">
        <dt className="text-secondary">{t("review.focusedTime")}</dt>
        <dd>{formatDuration(point.focused_time_ms)}</dd>
      </div>
    </dl>
  );
}

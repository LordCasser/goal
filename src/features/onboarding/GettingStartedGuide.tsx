/**
 * 引导清单（openspec onboarding-guidance）：头部进度环 + "Getting started
 * N of 5"，点开展开五步与各自状态。进度完全由后端从真实数据推导
 * （needs_refinement=false 的月目标、Later 有项、两条跨层链接、≥30 分钟的
 * 已结束专注块），本组件不做任何手动勾选；周期/任务变更事件到达时用
 * reconcile_getting_started_guide 重新核对（回退也因此自动成立）。
 *
 * 整份跳过持久化到后端，重启后 state=skipped，头部不再渲染；跳过同时
 * 清理引导遗留的空周期与空任务（后端负责，报告经 events 通知刷新）。
 */
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useTranslation } from "../../lib/i18n";
import type { CycleIdsPayload } from "../../lib/events";
import {
  getOnboarding,
  reconcileGettingStartedGuide,
  skipGettingStartedGuide,
  type GettingStartedGuide,
  type GuideStep,
} from "./api";

export function GettingStartedGuide() {
  const { t } = useTranslation("shell");
  const queryClient = useQueryClient();
  const [expanded, setExpanded] = useState(false);

  // 本地 query key："onboarding" 根是 src/lib 不可改动下的本地例外，不进
  // 全局失效矩阵，因此这里自己订阅两条变更事件来触发核对。
  const guide = useQuery({
    queryKey: guideKey(),
    queryFn: getOnboarding,
    staleTime: Infinity,
  });

  useEffect(() => {
    const refetch = () => {
      void queryClient.invalidateQueries({ queryKey: guideKey() });
      void reconcileGettingStartedGuide().catch(() => undefined);
    };
    let unlisteners: UnlistenFn[] = [];
    let disposed = false;
    const bind = async () => {
      try {
        const offCycles = await listen<CycleIdsPayload>("cycles:changed", refetch);
        const offTasks = await listen<CycleIdsPayload>("tasks:changed", refetch);
        if (disposed) {
          offCycles();
          offTasks();
          return;
        }
        unlisteners = [offCycles, offTasks];
      } catch {
        // No event channel (tests, non-Tauri host): the header still renders
        // from the initial fetch.
      }
    };
    void bind();
    return () => {
      disposed = true;
      for (const off of unlisteners) off();
    };
  }, [queryClient]);

  const skip = useMutation({
    mutationFn: skipGettingStartedGuide,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: guideKey() });
    },
  });

  if (guide.isLoading || guide.isError) return null;
  const current: GettingStartedGuide | undefined = guide.data;
  // 跳过（或用户明确完成）后不再主动显示（1.5）。
  if (!current || current.state !== "active") return null;

  const doneRatio = current.total === 0 ? 0 : current.completed / current.total;

  return (
    <section
      aria-label={t("onboarding.guide.aria")}
      className="flex flex-col border-b border-light bg-content px-4 py-2"
    >
      <div className="flex items-center gap-3">
        <button
          type="button"
          className="flex items-center gap-2 rounded-sm px-1 py-1 text-left hover:bg-hover"
          aria-expanded={expanded}
          onClick={() => setExpanded((open) => !open)}
        >
          <ProgressRing ratio={doneRatio} label={t("onboarding.guide.progress", { completed: current.completed, total: current.total })} />
          <span className="text-body font-medium text-primary">
            {t("onboarding.guide.progress", { completed: current.completed, total: current.total })}
          </span>
        </button>
        <button
          type="button"
          className="ml-auto rounded-sm px-2 py-1 text-menu text-secondary hover:bg-hover hover:text-primary"
          onClick={() => skip.mutate()}
          disabled={skip.isPending}
          aria-label={t("onboarding.guide.skip")}
        >
          {t("onboarding.guide.skip")}
        </button>
      </div>
      {expanded && (
        <ol className="mt-2 flex flex-col gap-1 pb-1">
          {current.steps.map((step) => (
            <GuideRow key={step.id} step={step} />
          ))}
        </ol>
      )}
    </section>
  );
}

function GuideRow({ step }: { step: GuideStep }) {
  const { t } = useTranslation("shell");
  const copy = GUIDE_STEP_COPY[step.id];
  const title = copy ? t(copy.title) : step.title;
  const detail = copy ? t(copy.detail) : step.detail;
  const statusLabel = step.status === "done"
    ? t("onboarding.guide.status.done")
    : step.status === "skipped"
      ? t("onboarding.guide.status.skipped")
      : t("onboarding.guide.status.notDone");
  return (
    <li className="flex items-start gap-2" data-step-id={step.id}>
      <span
        aria-label={
          statusLabel
        }
        className={
          step.status === "done"
            ? "mt-[3px] font-semibold text-primary"
            : "mt-[3px] text-hint"
        }
      >
        {step.status === "done" ? "✓" : step.status === "skipped" ? "–" : "○"}
      </span>
      <span className="flex flex-col">
        <span
          className={
            step.status === "done"
              ? "text-body text-secondary line-through decoration-hint"
              : "text-body text-primary"
          }
        >
          {title}
        </span>
        <span className="text-menu text-hint">{detail}</span>
      </span>
    </li>
  );
}

const GUIDE_STEP_COPY: Record<string, { title: string; detail: string }> = {
  set_long_term_goal: {
    title: "onboarding.guide.step.setLongTermGoal.title",
    detail: "onboarding.guide.step.setLongTermGoal.detail",
  },
  add_later_item: {
    title: "onboarding.guide.step.addLaterItem.title",
    detail: "onboarding.guide.step.addLaterItem.detail",
  },
  link_week_to_long_term: {
    title: "onboarding.guide.step.linkWeekToLongTerm.title",
    detail: "onboarding.guide.step.linkWeekToLongTerm.detail",
  },
  link_day_to_week: {
    title: "onboarding.guide.step.linkDayToWeek.title",
    detail: "onboarding.guide.step.linkDayToWeek.detail",
  },
  complete_focus_block: {
    title: "onboarding.guide.step.completeFocusBlock.title",
    detail: "onboarding.guide.step.completeFocusBlock.detail",
  },
};

/**
 * 进度环用真实进度语义：周长按 completed/total 的实数比例上色，四舍五入
 * 永远不会把 0 画成有进度、也不会把满进度画成缺口。
 */
export function ProgressRing({ ratio, label }: { ratio: number; label: string }) {
  const { t } = useTranslation("shell");
  const clamped = Math.min(1, Math.max(0, ratio));
  const radius = 7;
  const circumference = 2 * Math.PI * radius;
  const filled = circumference * clamped;
  return (
    <svg
      viewBox="0 0 20 20"
      className="h-5 w-5"
      role="img"
      aria-label={t("onboarding.guide.progressAria", { label })}
    >
      <circle
        cx="10"
        cy="10"
        r={radius}
        fill="none"
        stroke="currentColor"
        className="text-light"
        strokeWidth="2.5"
      />
      <circle
        cx="10"
        cy="10"
        r={radius}
        fill="none"
        stroke="currentColor"
        className="text-primary"
        strokeWidth="2.5"
        strokeLinecap="round"
        strokeDasharray={`${filled} ${circumference - filled}`}
        transform="rotate(-90 10 10)"
      />
    </svg>
  );
}

/** Local query key; see the module comment for the src/lib exception. */
export function guideKey(): readonly ["onboarding", "guide"] {
  return ["onboarding", "guide"] as const;
}

/**
 * 横向工作台（任务 7.4）：长期目标列组 → 周计划列组 → 日计划列组（日列内
 * 含专注块区）从左到右并列；列总宽超出视口时横向滚动，列本身不压缩
 * （openspec planner-workspace「横向并列的周期列」）。不含顶栏、Later 面板
 * 与待确认底栏——那些由 App.tsx 接线。
 *
 * 数据：qk.plannerState() 一次性取全部周期（month/week/day）。分组注意：
 * Later 容器以 id='later'、type='month' 播种且 list_planner_cycles 不过滤
 * 它，按 type 分组时必须显式排除（LATER_CYCLE_ID），否则会渲染出一个假的
 * "Later" 长期列；Later 面板/底栏另有自己的口径。专注块（session）不在
 * 规划载荷里，由日列自己经 list_sessions 拉取（见 CycleColumn）。
 */
import { useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Button, EmptyState } from "../../ui";
import {
  createPlanningCycle,
  ensureDay,
  getPlannerState,
  LATER_CYCLE_ID,
} from "../../lib/ipc";
import { qk } from "../../lib/events";
import type * as React from "react";
import { todayISO } from "./dates";
import { invalidateCycles, useActionError } from "./actions";
import { CycleColumn } from "./CycleColumn";
import { DurationDialog } from "./DurationDialog";

export function PlannerWorkspace(): React.JSX.Element {
  const qc = useQueryClient();
  const { data: state, isLoading, isError } = useQuery({
    queryKey: qk.plannerState(),
    queryFn: getPlannerState,
  });
  const [durationOpen, setDurationOpen] = useState(false);
  const [selectedMonthId, setSelectedMonthId] = useState<string | null>(null);
  const { error, run, dismiss } = useActionError();

  const cycles = state?.cycles ?? [];

  const months = useMemo(
    // 排除 Later 容器（它也以 type='month' 出现在列表里，见文件头注释）。
    () => cycles.filter((c) => c.type === "month" && c.id !== LATER_CYCLE_ID),
    [cycles],
  );
  const weeks = useMemo(() => cycles.filter((c) => c.type === "week"), [cycles]);
  const days = useMemo(() => cycles.filter((c) => c.type === "day"), [cycles]);

  const today = todayISO();

  // 周计划创建的默认 parent：优先用户点选的 month，其次最新的未结束 month。
  const activeMonth =
    months.find((m) => m.id === selectedMonthId && !m.finished) ??
    [...months].reverse().find((m) => !m.finished) ??
    null;
  // ensureDay 要求「今天」落在某个未结束长周期内（no_covering_long_term_cycle）。
  const coveringMonth = months.find(
    (m) =>
      !m.finished &&
      m.starts_on !== null &&
      m.ends_on !== null &&
      m.starts_on <= today &&
      today < m.ends_on,
  );

  const createWeek = async () => {
    if (!activeMonth) return;
    if (await run(() => createPlanningCycle({ cycle_type: "week", parent_id: activeMonth.id }))) {
      invalidateCycles(qc);
    }
  };

  const createToday = async () => {
    if (await run(() => ensureDay(null))) invalidateCycles(qc);
  };

  if (isLoading) {
    return (
      <div className="flex h-full items-center justify-center text-body text-hint">
        Loading workspace…
      </div>
    );
  }

  if (isError || state === undefined) {
    return (
      <div className="flex h-full items-center justify-center">
        <EmptyState
          title="Workspace"
          description="The planner state could not be loaded. Check the app log and retry."
          action={
            <Button onClick={() => void qc.invalidateQueries({ queryKey: qk.plannerState() })}>
              Retry
            </Button>
          }
        />
      </div>
    );
  }

  /* 空状态的禁用提示并入说明文案：spec 场景「前置条件未满足 → 主按钮禁用 + 提示」。 */
  const weekHint =
    months.length === 0
      ? "Create a long-term cycle first — weekly plans live inside it."
      : activeMonth === null
        ? "All long-term cycles have ended — start a new one to plan weeks."
        : null;
  const dayHint = coveringMonth
    ? null
    : "Today must fall inside an active long-term cycle before a day column can be created.";

  return (
    <>
      {/* 外层 min-w-0 + overflow-x-auto：列宽不压缩，超出部分横向滚动。 */}
      <div className="h-full min-w-0 overflow-x-auto">
        <div className="flex h-full w-max gap-4 p-4">
          {months.length > 0 ? (
            months.map((month) => (
              <CycleColumn
                key={month.id}
                cycle={month}
                selected={month.id === (selectedMonthId ?? activeMonth?.id)}
                onSelect={() => setSelectedMonthId(month.id)}
              />
            ))
          ) : (
            /* 无 month：空状态解剖（图标 + 大写标题 + 用途说明 + 唯一主入口）→ 时长弹窗。 */
            <EmptyState
              title="Long-term goals"
              description="Set the outcome you want over the next months; weeks and days hang below it."
              action={
                <Button variant="primary" onClick={() => setDurationOpen(true)}>
                  Set long-term goals
                </Button>
              }
              className="h-full w-plan shrink-0"
            />
          )}
          {weeks.length > 0 ? (
            weeks.map((week) => (
              <CycleColumn key={week.id} cycle={week} />
            ))
          ) : (
            /* 禁用态的提示文案放在空状态卡下方：前置条件未满足时主按钮禁用并说明下一步（spec 场景）。 */
            <div className="flex h-full w-plan shrink-0 flex-col gap-2">
              <EmptyState
                title="Week plan"
                description="Break this week into concrete work that serves your long-term goals."
                action={
                  <Button
                    variant="primary"
                    disabled={!activeMonth}
                    title={weekHint ?? "Create this week's plan under the selected long-term cycle"}
                    onClick={() => void createWeek()}
                  >
                    Create this week
                  </Button>
                }
                className="flex-1"
              />
              {weekHint && (
                <p className="px-6 text-center text-caption text-secondary">{weekHint}</p>
              )}
            </div>
          )}
          {days.length > 0 ? (
            days.map((day) => (
              <CycleColumn key={day.id} cycle={day} />
            ))
          ) : (
            /* 日列：ensureDay() 找到或创建「今天」列（date 传 null = 本地今天）。 */
            <div className="flex h-full w-plan shrink-0 flex-col gap-2">
              <EmptyState
                title="Day plan"
                description="Plan today's tasks and focus blocks, tied to this week."
                action={
                  <Button
                    variant="primary"
                    disabled={!coveringMonth}
                    title={dayHint ?? "Find or create today's column"}
                    onClick={() => void createToday()}
                  >
                    Add today
                  </Button>
                }
                className="flex-1"
              />
              {dayHint && (
                <p className="px-6 text-center text-caption text-secondary">{dayHint}</p>
              )}
            </div>
          )}
        </div>
      </div>
      <DurationDialog open={durationOpen} onClose={() => setDurationOpen(false)} />
      {error && (
        <div
          role="alert"
          className="absolute bottom-4 left-1/2 z-40 -translate-x-1/2 rounded-sm border border-light bg-content px-3 py-2 text-caption text-danger shadow-[0_8px_24px_rgba(0,0,0,0.12)]"
        >
          {error}
          <button type="button" className="ml-2 underline" onClick={dismiss}>
            Dismiss
          </button>
        </div>
      )}
    </>
  );
}

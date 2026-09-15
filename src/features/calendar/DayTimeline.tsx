/**
 * 单日时间轴（tasks.md §5.5）：已排专注块按开始时间与时长排布，未排的
 * 出现在待排区；把待排块拖入时间轴即获得开始时间（时长不变）。重叠的
 * 两个块都保留并以危险色标注（spec：不自动移动或删除任何一方）。
 *
 * 底部是时间预算条（§5.6）：未设置时渲染 null——绝不出现在何空进度条；
 * 未超显示剩余，超出显式提示但不阻止继续安排。
 */
import { useRef, useState } from "react";
import type { DragEvent } from "react";

import { Button, EmptyState, cn } from "../../ui";
import { formatDuration } from "../planner/dates";
import type { CalendarDay, TimeBudget } from "./api";
import { budgetState } from "./calendar-model";

/** 每小时像素高；24 小时全部铺开，容器内滚动。 */
const HOUR_PX = 48;
const DAY_HOURS = 24;
/** 拖放落点按 15 分钟吸附，让「拖到 9 点附近」得到可读的时间。 */
const SNAP_MINUTES = 15;

function two(n: number): string {
  return String(n).padStart(2, "0");
}

/** epoch 毫秒 → 本地 `HH:MM`。 */
function localClock(ms: number): string {
  const at = new Date(ms);
  return `${two(at.getHours())}:${two(at.getMinutes())}`;
}

export function DayTimeline({
  date,
  day,
  budget,
  overlaps,
  onCreateDay,
  onSchedule,
}: {
  date: string;
  day: CalendarDay | undefined;
  budget: TimeBudget | null;
  overlaps: Set<string>;
  onCreateDay: (date: string) => void;
  onSchedule: (sessionId: string, startsAt: number, durationMs: number | null) => void;
}) {
  const timelineRef = useRef<HTMLDivElement>(null);
  const [draggingSessionId, setDraggingSessionId] = useState<string | null>(null);

  // 时间轴锚定该日本地零点（墙钟概念，与 ISO 网格的 UTC 算术刻意分开）。
  const [y, m, d] = date.split("-").map(Number);
  const dayStartMs = new Date(y ?? 1970, (m ?? 1) - 1, d ?? 1).getTime();

  const dropToStartsAt = (event: DragEvent<HTMLDivElement>): number => {
    const rect = timelineRef.current?.getBoundingClientRect();
    const offsetY = rect === undefined ? 0 : event.clientY - rect.top;
    const rawMinutes = (offsetY / HOUR_PX) * 60;
    const snapped = Math.round(rawMinutes / SNAP_MINUTES) * SNAP_MINUTES;
    const minuteOfDay = Math.min(Math.max(snapped, 0), DAY_HOURS * 60 - SNAP_MINUTES);
    return dayStartMs + minuteOfDay * 60_000;
  };

  const scheduled = (day?.sessions ?? []).filter((s) => s.schedule !== null);
  const staged = (day?.sessions ?? []).filter((s) => s.schedule === null);

  return (
    <section aria-label={`Day timeline ${date}`} className="flex min-h-0 flex-1 flex-col gap-3">
      {day?.day_cycle ? (
        <>
          {/* 待排区：只有时长没有开始时间的专注块。 */}
          <div data-testid="staging-area" className="flex flex-col gap-1.5">
            <h3 className="text-block-title font-medium text-secondary">Unscheduled</h3>
            {staged.length === 0 ? (
              <p className="text-caption text-hint">Every focus block has a time slot.</p>
            ) : (
              staged.map(({ session }) => (
                <div
                  key={session.id}
                  draggable={session.duration !== null}
                  title={
                    session.duration === null
                      ? "Set a duration first — the timeline needs a length"
                      : "Drag onto the timeline to give it a start time"
                  }
                  onDragStart={(event) => {
                    if (session.duration === null) return;
                    event.dataTransfer.setData("text/plain", session.id);
                    event.dataTransfer.effectAllowed = "copy";
                    setDraggingSessionId(session.id);
                  }}
                  onDragEnd={() => setDraggingSessionId(null)}
                  className={cn(
                    "flex items-center justify-between rounded-sm border border-light px-2 py-1 text-caption",
                    session.duration !== null ? "cursor-grab hover:bg-hover" : "text-hint",
                    draggingSessionId === session.id && "opacity-50",
                  )}
                >
                  <span className={cn("truncate", session.finished && "text-hint line-through")}>
                    {session.title}
                  </span>
                  <span className="ml-2 shrink-0 text-hint">{formatDuration(session.duration)}</span>
                </div>
              ))
            )}
          </div>

          {/* 时间轴本体：落点换算成当日零点起的毫秒。 */}
          <div
            ref={timelineRef}
            data-testid="timeline"
            onDragOver={(event) => event.preventDefault()}
            onDrop={(event) => {
              event.preventDefault();
              const sessionId = draggingSessionId ?? event.dataTransfer.getData("text/plain");
              setDraggingSessionId(null);
              if (!sessionId) return;
              const duration =
                staged.find((s) => s.session.id === sessionId)?.session.duration ?? null;
              onSchedule(sessionId, dropToStartsAt(event), duration);
            }}
            className="relative min-h-0 flex-1 overflow-y-auto rounded-sm border border-light bg-subtle"
          >
            <div className="relative" style={{ height: DAY_HOURS * HOUR_PX }}>
              {Array.from({ length: DAY_HOURS }, (_, hour) => (
                <div
                  key={hour}
                  className="absolute left-0 right-0 border-t border-light text-caption text-hint"
                  style={{ top: hour * HOUR_PX }}
                >
                  <span className="absolute left-1 top-0.5">{two(hour)}:00</span>
                </div>
              ))}
              {scheduled.map(({ session, schedule }) => {
                const slot = schedule!;
                const top = ((slot.starts_at - dayStartMs) / 3_600_000) * HOUR_PX;
                const height = Math.max(((slot.ends_at - slot.starts_at) / 3_600_000) * HOUR_PX, 20);
                const conflict = overlaps.has(session.id);
                return (
                  <div
                    key={session.id}
                    data-scheduled-block={session.id}
                    title={`${session.title} · ${localClock(slot.starts_at)}–${localClock(slot.ends_at)}`}
                    style={{ top, height }}
                    className={cn(
                      "absolute left-10 right-2 overflow-hidden rounded-sm border px-1.5 py-0.5 text-caption",
                      conflict
                        ? "border-danger bg-danger-surface text-danger"
                        : "border-focus-surface bg-focus-surface text-primary",
                    )}
                  >
                    <span className="block truncate font-medium">
                      {session.title}
                      {slot.truncated ? " →" : ""}
                    </span>
                    <span className={cn("block", conflict ? "text-danger" : "text-secondary")}>
                      {localClock(slot.starts_at)}–{localClock(slot.ends_at)}
                      {conflict ? " · overlaps" : ""}
                    </span>
                  </div>
                );
              })}
            </div>
          </div>
        </>
      ) : (
        // 该日还没有日周期：与工作台同款空状态，唯一入口一键创建。
        <EmptyState
          title="Day plan"
          description={`Nothing planned for ${date} yet.`}
          action={
            <Button variant="primary" onClick={() => onCreateDay(date)}>
              Create day plan
            </Button>
          }
        />
      )}

      <BudgetBar budget={budget} />
    </section>
  );
}

/**
 * 预算条三态（tasks.md §5.8 的测试对象之一）：
 * unset → null（不渲染任何东西）；under → 进度 + 剩余；over → 危险色 + 超出量。
 */
export function BudgetBar({ budget }: { budget: TimeBudget | null }) {
  if (budget === null) return null;
  const state = budgetState(budget);
  if (state.kind === "unset") return null; // 单点/未设置数据不渲染误导性图表

  const hours = (minutes: number) => {
    const h = Math.floor(minutes / 60);
    const rest = minutes % 60;
    return h > 0 ? `${h}h ${rest}m` : `${rest}m`;
  };
  const percent = Math.min(
    100,
    Math.round((budget.scheduled_minutes / budget.capacity_minutes!) * 100),
  );

  return (
    <div data-testid="budget-bar" className="flex flex-col gap-1">
      <div className="flex items-center justify-between text-caption">
        <span className="text-secondary">
          {budget.scheduled_minutes} of {budget.capacity_minutes} min scheduled
        </span>
        {state.kind === "under" ? (
          <span className="text-secondary">{hours(state.remainingMinutes)} left</span>
        ) : (
          <span className="font-medium text-danger">Over by {hours(state.overMinutes)}</span>
        )}
      </div>
      <div
        role="progressbar"
        aria-valuemin={0}
        aria-valuemax={budget.capacity_minutes ?? 0}
        aria-valuenow={budget.scheduled_minutes}
        className="h-1.5 w-full overflow-hidden rounded-full bg-subtle"
      >
        <div
          className={cn("h-full rounded-full", state.kind === "over" ? "bg-danger" : "bg-focus")}
          style={{ width: `${percent}%` }}
        />
      </div>
    </div>
  );
}

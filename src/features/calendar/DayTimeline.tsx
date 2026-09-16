/**
 * 单日时间轴（tasks.md §5.5）：已排专注块按开始时间与时长排布，未排的
 * 出现在待排区；把待排块拖入时间轴即获得开始时间（时长不变）。重叠的
 * 两个块都保留并以危险色标注（spec：不自动移动或删除任何一方）。
 *
 * 底部是时间预算条（§5.6）：未设置时渲染 null——绝不出现在何空进度条；
 * 未超显示剩余，超出显式提示但不阻止继续安排。
 */
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { DragEvent } from "react";

import { Button, EmptyState, Popover, cn } from "../../ui";
import { todayISO } from "../planner/dates";
import { useTranslation, formatDuration, formatDate } from "../../lib/i18n";
import type { CalendarDay, TimeBudget } from "./api";
import { budgetState, scheduleOverlapLanes } from "./calendar-model";
import { createDragToken, readDragToken, SESSION_DRAG_TYPE, writeDragToken } from "./calendar-dnd";

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
  onSchedule: (sessionId: string, startsAt: number | null, durationMs: number | null) => Promise<boolean>;
}) {
  const { t } = useTranslation("planning");
  const displayDate = formatDate(date, { year: "numeric", month: "short", day: "numeric" });
  const timelineRef = useRef<HTMLDivElement>(null);
  const initialScrollDate = useRef<string | null>(null);
  const [draggingSession, setDraggingSession] = useState<{
    id: string;
    token: string;
    grabOffsetPx: number;
  } | null>(null);
  const [dragPreviewStartsAt, setDragPreviewStartsAt] = useState<number | null>(null);
  const [editingSessionId, setEditingSessionId] = useState<string | null>(null);
  const [editingStart, setEditingStart] = useState("");
  const [editingDuration, setEditingDuration] = useState("");
  const editAnchorRef = useRef<HTMLButtonElement | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    const cancelDrag = (event: KeyboardEvent) => {
      if (event.key === "Escape") { setDraggingSession(null); setDragPreviewStartsAt(null); }
    };
    window.addEventListener("keydown", cancelDrag);
    return () => window.removeEventListener("keydown", cancelDrag);
  }, []);

  // 时间轴锚定该日本地零点（墙钟概念，与 ISO 网格的 UTC 算术刻意分开）。
  const [y = 1970, m = 1, d = 1] = date.split("-").map(Number);
  const dayStartMs = new Date(y ?? 1970, (m ?? 1) - 1, d ?? 1).getTime();

  const pointerOffset = (event: DragEvent<HTMLDivElement>): number => {
    const timeline = timelineRef.current;
    const rect = timeline?.getBoundingClientRect();
    return timeline && rect
      ? event.clientY - rect.top - timeline.clientTop + timeline.scrollTop
      : 0;
  };

  const dropToStartsAt = (event: DragEvent<HTMLDivElement>, grabOffsetPx = 0): number => {
    const offsetY = pointerOffset(event) - grabOffsetPx;
    const rawMinutes = (offsetY / HOUR_PX) * 60;
    const snapped = Math.round(rawMinutes / SNAP_MINUTES) * SNAP_MINUTES;
    const minuteOfDay = Math.min(Math.max(snapped, 0), DAY_HOURS * 60 - SNAP_MINUTES);
    return dayStartMs + minuteOfDay * 60_000;
  };

  const scheduled = (day?.sessions ?? []).filter((s) => s.schedule !== null);
  const staged = (day?.sessions ?? []).filter((s) => s.schedule === null);
  const lanes = scheduleOverlapLanes(scheduled);
  const canReschedule = (session: CalendarDay["sessions"][number]["session"]): boolean =>
    !saving && !day?.day_cycle?.finished && !session.started && !session.finished;
  const editingItem = scheduled.find(({ session }) => session.id === editingSessionId);
  const save = async (id: string, start: number | null, duration: number | null) => {
    if (saving) return;
    setSaving(true);
    try { if (await onSchedule(id, start, duration)) setEditingSessionId(null); }
    finally { setSaving(false); }
  };

  useLayoutEffect(() => {
    if (initialScrollDate.current === date || !day?.day_cycle || !timelineRef.current) return;
    initialScrollDate.current = date;
    const firstStart = scheduled[0]?.schedule?.starts_at;
    const current = new Date();
    const target = firstStart ?? (
      date === todayISO()
        ? new Date(y ?? 1970, (m ?? 1) - 1, d ?? 1, current.getHours(), current.getMinutes()).getTime()
        : dayStartMs + 9 * 3_600_000
    );
    timelineRef.current.scrollTop = Math.max(
      0,
      ((target - dayStartMs) / 3_600_000) * HOUR_PX - timelineRef.current.clientHeight / 3,
    );
  }, [date, day?.day_cycle?.id, dayStartMs, d, m, scheduled, y]);

  const clearDrag = () => {
    setDraggingSession(null);
    setDragPreviewStartsAt(null);
  };

  const startDrag = (event: DragEvent<HTMLDivElement>, sessionId: string, scheduledBlock: boolean) => {
    const source = (scheduledBlock ? scheduled : staged).find((item) => item.session.id === sessionId);
    if (!source || !canReschedule(source.session)) return;
    const token = createDragToken();
    const rect = event.currentTarget.getBoundingClientRect();
    const grabOffsetPx = scheduledBlock && Number.isFinite(event.clientY) ? Math.max(0, event.clientY - rect.top) : 0;
    writeDragToken(event.dataTransfer, SESSION_DRAG_TYPE, token);
    setDraggingSession({ id: sessionId, token, grabOffsetPx });
    setDragPreviewStartsAt(scheduledBlock ? source.schedule?.starts_at ?? null : null);
  };

  const [editHours = Number.NaN, editMinutes = Number.NaN] = editingStart.split(":").map(Number);
  const validStart = /^\d{2}:\d{2}$/.test(editingStart) && editHours >= 0 && editHours < 24 && editMinutes >= 0 && editMinutes < 60;
  const durationMinutes = Number(editingDuration);
  const validDuration = editingDuration.trim() !== "" && Number.isSafeInteger(durationMinutes)
    && durationMinutes > 0 && Number.isSafeInteger(durationMinutes * 60_000);
  const editedStartsAt = new Date(y, m - 1, d, editHours, editMinutes).getTime();
  const editedEndsAt = new Date(editedStartsAt + durationMinutes * 60_000);
  const validEdit = validStart && validDuration && Number.isFinite(editedEndsAt.getTime());
  const endsOnAnotherDay = editedEndsAt.getFullYear() !== y || editedEndsAt.getMonth() !== m - 1 || editedEndsAt.getDate() !== d;

  const finishTimeEdit = (sessionId: string) => {
    if (!validEdit) return;
    void save(sessionId, editedStartsAt, durationMinutes * 60_000);
  };

  return (
    <section aria-label={t("calendar.timeline", { date: displayDate })} className="flex min-h-0 flex-1 flex-col gap-3">
      {day?.day_cycle ? (
        <>
          {/* 待排区：只有时长没有开始时间的专注块。 */}
          <div className={cn("rounded-md transition-colors", draggingSession && "bg-focus-surface ring-1 ring-inset ring-focus/30")}
            onDragOver={(event) => {
              if (scheduled.some(({ session }) => session.id === draggingSession?.id)) { event.preventDefault(); setDragPreviewStartsAt(null); }
            }}
            onDrop={(event) => {
              const source = scheduled.find(({ session }) => session.id === draggingSession?.id);
              const accepted = source && canReschedule(source.session) && readDragToken(event.dataTransfer, SESSION_DRAG_TYPE) === draggingSession?.token;
              event.preventDefault(); clearDrag();
              if (accepted) void save(source.session.id, null, null);
            }}>
          {staged.length === 0 ? (
            <div data-testid="staging-area" className="flex min-h-8 items-center gap-2 text-caption">
              <span className="font-medium text-secondary">{t("calendar.unscheduledTitle")}</span>
              <span className="text-hint">{draggingSession ? t("calendar.unscheduledEmpty") : t("calendar.allScheduled")}</span>
            </div>
          ) : (
            <div data-testid="staging-area" className="flex flex-col gap-1.5">
              <h3 className="text-block-title font-medium text-secondary">{t("calendar.unscheduledTitle")}</h3>
              {staged.map(({ session }) => (
                <div
                  key={session.id}
                  data-focus-block={session.id}
                  tabIndex={-1}
                  draggable={session.duration !== null && canReschedule(session)}
                  title={
                    session.duration === null
                      ? t("calendar.durationNeeded")
                      : t("calendar.dragToTimeline")
                  }
                  aria-disabled={!canReschedule(session) || undefined}
                  onDragStart={(event) => {
                    startDrag(event, session.id, false);
                  }}
                  onDragEnd={clearDrag}
                  className={cn(
                    "flex items-center justify-between rounded-sm border border-light px-2 py-1 text-caption",
                    session.duration !== null && canReschedule(session) ? "cursor-grab hover:bg-hover" : "text-hint",
                    draggingSession?.id === session.id && "opacity-50",
                  )}
                >
                  <span className={cn("truncate", session.finished && "text-hint line-through")}>
                    {session.title}
                  </span>
                  <span className="ml-2 shrink-0 text-hint">{formatDuration(session.duration ?? 0)}</span>
                </div>
              ))}
            </div>
          )}
          </div>

          {/* 时间轴本体：落点换算成当日零点起的毫秒。 */}
          <div
            ref={timelineRef}
            data-testid="timeline"
            onDragOver={(event) => {
              // Browsers may hide custom payload values during dragover. The
              // drop handler still validates the token before scheduling.
              if (!draggingSession) return;
              event.preventDefault();
              setDragPreviewStartsAt(dropToStartsAt(event, draggingSession.grabOffsetPx));
            }}
            onDrop={(event) => {
              event.preventDefault();
              const source = draggingSession;
              const token = readDragToken(event.dataTransfer, SESSION_DRAG_TYPE);
              const sourceItem = [...scheduled, ...staged].find((item) => item.session.id === source?.id);
              const duration = sourceItem?.schedule?.duration_ms ?? sourceItem?.session.duration ?? null;
              const startsAt = source ? dropToStartsAt(event, source.grabOffsetPx) : null;
              clearDrag();
              if (!source || token !== source.token || !sourceItem || duration === null || startsAt === null || !canReschedule(sourceItem.session)) return;
              void save(source.id, startsAt, duration);
            }}
            onDragLeave={(event) => {
              if (event.currentTarget === event.target) setDragPreviewStartsAt(null);
            }}
            className="relative min-h-0 flex-1 overflow-y-auto rounded-md border border-light bg-content"
          >
            <div className="relative" style={{ height: DAY_HOURS * HOUR_PX }}>
              {Array.from({ length: DAY_HOURS }, (_, hour) => (
                <div
                  key={hour}
                  className="pointer-events-none absolute left-14 right-0 border-t border-light text-caption text-hint"
                  style={{ top: hour * HOUR_PX }}
                >
                  <span className="absolute -left-12 top-0.5 w-10 text-right">{two(hour)}:00</span>
                </div>
              ))}
              {draggingSession && dragPreviewStartsAt !== null && (
                <div
                  data-testid="drag-ghost"
                  className="pointer-events-none absolute left-14 right-2 z-20 border-t border-dashed border-focus text-caption text-focus"
                  style={{ top: ((dragPreviewStartsAt - dayStartMs) / 3_600_000) * HOUR_PX }}
                >
                  <span className="absolute -top-5 right-0 rounded-sm bg-focus-surface px-1.5 py-0.5 font-medium">
                    {localClock(dragPreviewStartsAt)}
                  </span>
                </div>
              )}
              {scheduled.map(({ session, schedule }) => {
                const slot = schedule!;
                const top = ((slot.starts_at - dayStartMs) / 3_600_000) * HOUR_PX;
                const height = Math.max(((slot.ends_at - slot.starts_at) / 3_600_000) * HOUR_PX, 2);
                const conflict = overlaps.has(session.id);
                const reorderable = canReschedule(session);
                const lane = lanes.get(session.id) ?? { lane: 0, lanes: 1 };
                return <div key={session.id} data-scheduled-block={session.id} data-focus-block={session.id}
                  tabIndex={-1} draggable={reorderable} aria-disabled={!reorderable || undefined}
                  title={`${session.title} · ${localClock(slot.starts_at)}–${localClock(slot.ends_at)}`}
                  style={{ top, height, left: `calc(56px + (100% - 64px) * ${lane.lane / lane.lanes})`, width: `calc((100% - 64px) / ${lane.lanes} - ${lane.lanes > 1 ? 4 : 0}px)` }}
                  onDragStart={(event) => {
                    if ((event.target as HTMLElement).closest("button")) { event.preventDefault(); return; }
                    startDrag(event, session.id, true);
                  }}
                  onDragEnd={clearDrag}
                  className={cn("group/block absolute flex items-start overflow-visible rounded-md border-l-[3px] pl-2 pr-6 text-caption",
                    conflict ? "border-danger bg-danger-surface text-danger ring-1 ring-danger/40" : "border-focus bg-focus-surface text-primary",
                    reorderable ? "cursor-grab active:cursor-grabbing" : "opacity-70", draggingSession?.id === session.id && "opacity-40")}>
                  <div className="min-w-0 flex-1 overflow-hidden" style={{ maxHeight: height }}>
                    <span className="block truncate font-medium leading-5">{session.title}</span>
                    {height >= 40 && <span className="block truncate text-[11px] leading-4 text-secondary">
                      {localClock(slot.starts_at)}–{localClock(slot.ends_at)}{conflict ? ` · ${t("calendar.overlaps")}` : ""}
                    </span>}
                  </div>
                  {reorderable && <button type="button" draggable={false}
                    aria-label={t("calendar.editSchedule", { title: session.title })} aria-haspopup="dialog" aria-expanded={editingSessionId === session.id}
                    title={t("calendar.editScheduleTitle")}
                    onClick={(event) => {
                      event.stopPropagation(); editAnchorRef.current = event.currentTarget;
                      setEditingSessionId(session.id); setEditingStart(localClock(slot.starts_at));
                      setEditingDuration(String(slot.duration_ms / 60_000));
                    }}
                    className="absolute right-0.5 top-0 flex h-5 w-5 items-center justify-center rounded-sm text-secondary opacity-40 hover:bg-content/70 group-hover/block:opacity-100 focus-visible:opacity-100">
                    <svg width="14" height="14" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><circle cx="3" cy="8" r="1"/><circle cx="8" cy="8" r="1"/><circle cx="13" cy="8" r="1"/></svg>
                  </button>}
                </div>;
              })}
            </div>
          </div>
        </>
      ) : (
        // 该日还没有日周期：与工作台同款空状态，唯一入口一键创建。
        <EmptyState
          title={t("calendar.dayPlanTitle")}
          description={t("calendar.nothingPlanned", { date: displayDate })}
          action={
            <Button variant="primary" onClick={() => onCreateDay(date)}>
              {t("calendar.createDay")}
            </Button>
          }
        />
      )}

      <Popover open={!!editingItem} onClose={() => { if (!saving) setEditingSessionId(null); }} anchorRef={editAnchorRef}
        role="dialog" label={t("calendar.focusBlockTime")} className="w-72 p-3">
        {editingItem && <>
          <p className="mb-1 truncate text-menu font-medium text-primary">{editingItem.session.title}</p>
          <p className="mb-3 text-caption text-hint">{displayDate}</p>
          <form onSubmit={(event) => { event.preventDefault(); finishTimeEdit(editingItem.session.id); }}>
            <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-3">
              <div>
                <label className="mb-1 block text-caption text-secondary" htmlFor="focus-start">{t("calendar.startTime")}</label>
                <input id="focus-start" aria-label={t("calendar.startTimeFor", { title: editingItem.session.title })} type="time" step={60}
                  required value={editingStart} disabled={saving} onChange={(event) => setEditingStart(event.target.value)}
                  className="h-10 w-full min-w-0 rounded-md border border-control bg-content px-2 py-2 text-menu text-primary outline-none focus:border-focus" />
              </div>
              <div>
                <label className="mb-1 block text-caption text-secondary" htmlFor="focus-duration">{t("calendar.durationMinutes")}</label>
                  <input id="focus-duration" aria-label={t("calendar.durationFor", { title: editingItem.session.title })} type="number" min={1} step={1}
                    required value={editingDuration} disabled={saving} onChange={(event) => setEditingDuration(event.target.value)}
                    className="h-10 w-full min-w-0 rounded-md border border-control bg-content px-2 py-2 text-menu text-primary outline-none focus:border-focus" />
              </div>
            </div>
            <p className="my-3 min-h-4 text-caption text-hint" aria-live="polite">
              {validEdit ? <>{t("calendar.endsAt", { time: localClock(editedEndsAt.getTime()) })}{endsOnAnotherDay && ` · ${formatDate(editedEndsAt, { month: "short", day: "numeric" })}`}</> : t("calendar.editValidation")}
            </p>
            <div className="flex justify-end gap-2">
              <Button disabled={saving} onClick={() => setEditingSessionId(null)}>{t("calendar.cancel")}</Button>
              <Button type="submit" variant="primary" disabled={saving || !validEdit} aria-label={t("calendar.saveSchedule", { title: editingItem.session.title })}>{saving ? t("calendar.saving") : t("calendar.save")}</Button>
            </div>
          </form>
          <button type="button" disabled={saving} aria-label={t("calendar.moveUnscheduled", { title: editingItem.session.title })}
            onClick={() => void save(editingItem.session.id, null, null)}
            className="mt-3 w-full border-t border-light pt-3 text-left text-caption text-secondary hover:text-primary disabled:opacity-50">{t("calendar.moveToUnscheduled")}</button>
        </>}
      </Popover>
      <BudgetBar budget={budget} />
    </section>
  );
}

/**
 * 预算条三态（tasks.md §5.8 的测试对象之一）：
 * unset → null（不渲染任何东西）；under → 进度 + 剩余；over → 危险色 + 超出量。
 */
function budgetDuration(minutes: number, t: (key: string, options?: Record<string, unknown>) => string): string {
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return hours ? t("calendar.hoursMinutesShort", { hours, minutes: rest }) : t("calendar.minutesShort", { count: rest });
}

export function BudgetBar({ budget }: { budget: TimeBudget | null }) {
  const { t } = useTranslation("planning");
  if (budget === null) return null;
  const state = budgetState(budget);
  if (state.kind === "unset") return null; // 单点/未设置数据不渲染误导性图表

  const percent = Math.min(
    100,
    Math.round((budget.scheduled_minutes / budget.capacity_minutes!) * 100),
  );

  return (
    <div data-testid="budget-bar" className="flex flex-col gap-1">
      <div className="flex items-center justify-between text-caption">
        <span className="text-secondary">
          {t("calendar.budgetScheduled", { scheduled: budget.scheduled_minutes, capacity: budget.capacity_minutes })}
        </span>
        {state.kind === "under" ? (
          <span className="text-secondary">{t("calendar.left", { duration: budgetDuration(state.remainingMinutes, t) })}</span>
        ) : (
          <span className="font-medium text-danger">{t("calendar.overBy", { duration: budgetDuration(state.overMinutes, t) })}</span>
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

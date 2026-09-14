/**
 * 日计划列内的专注块区（design.md §7）。跟随所选日，列出该日的 session：
 * 标题、计划时长、运行中显示 Stop + 剩余时间（1s 本地刷新，等宽数字防抖
 * 动）、ProgressDot 表达状态；已结束显示实际/计划时长（focused_time 与
 * duration 是两个不同概念，见 §7「计划时长、剩余时间、实际投入用不同名称」）。
 *
 * 数据由 CycleColumn 经 list_sessions 拉取后传入；session 变更会为所属日
 * 发 cycles:changed，查询随之失效刷新。
 */
import { useEffect, useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Button, Input, ProgressDot } from "../../ui";
import { addSession, finishCycle, startCycle, type Cycle } from "../../lib/ipc";
import { formatClock, formatDuration } from "./dates";
import { invalidateCycles, useActionError } from "./actions";
import { CycleOptionsMenu } from "./CycleOptionsMenu";

const DURATION_PRESETS: ReadonlyArray<{ label: string; ms: number | null }> = [
  { label: "No duration", ms: null },
  { label: "25m", ms: 25 * 60_000 },
  { label: "45m", ms: 45 * 60_000 },
  { label: "60m", ms: 60 * 60_000 },
  { label: "90m", ms: 90 * 60_000 },
];

/** 运行中的计时读数每秒重算一次；没有运行块时不挂定时器。 */
function useNow(active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [active]);
  return now;
}

export function FocusArea({
  day,
  sessions,
  /** 全工作台当前运行中的 session id（一次只推进一个块，design.md §7）。 */
  runningSessionId,
}: {
  day: Cycle;
  sessions: Cycle[];
  runningSessionId: string | null;
}) {
  const qc = useQueryClient();
  const locked = day.finished;
  const [adding, setAdding] = useState(false);
  const { error, run } = useActionError();

  const sorted = useMemo(
    () => sessions.filter((s) => s.parent_id === day.id).sort((a, b) => a.position - b.position),
    [sessions, day.id],
  );
  const hasRunning = runningSessionId !== null;
  const now = useNow(hasRunning);

  const onStart = async (session: Cycle) => {
    if (await run(() => startCycle(session.id))) invalidateCycles(qc);
  };

  const onStop = async (session: Cycle) => {
    if (await run(() => finishCycle(session.id))) invalidateCycles(qc);
  };

  return (
    <section aria-label="Focus blocks" className="border-b border-light pb-2">
      <header className="flex items-center justify-between px-2 pb-1">
        <h4 className="text-caption font-semibold uppercase tracking-[0.08em] text-secondary">
          Focus blocks
        </h4>
        {day.focused_time > 0 && (
          <span className="text-caption text-hint" title="Actual focused time">
            {formatDuration(day.focused_time)} focused
          </span>
        )}
      </header>
      {sorted.length === 0 && !adding && (
        <p className="px-2 pb-1 text-caption text-hint">
          Timed work sessions for this day. Add one to start focusing.
        </p>
      )}
      <ol className="flex flex-col">
        {sorted.map((session) => {
          const running = session.started && !session.finished;
          // duration 为空的块无法启动（cycle_duration_required）；运行中的块
          // duration 必非空，剩余时间可安全计算。
          const remaining =
            running && session.duration !== null && session.started_at !== null
              ? session.duration - (now - session.started_at)
              : null;
          return (
            <li
              key={session.id}
              data-session-id={session.id}
              className="flex items-center gap-2 rounded-sm px-2 py-1.5 transition-colors duration-100 hover:bg-hover"
            >
              <ProgressDot
                tone={running ? "active" : session.finished ? "done" : "idle"}
                className="mt-0.5"
              />
              <div className="min-w-0 flex-1">
                <p className="flex items-baseline gap-1.5 text-block-title font-semibold text-primary">
                  <span className="truncate">{session.title}</span>
                  {session.repeat_id && (
                    <span className="text-caption font-normal text-hint" title="Repeats daily">
                      ↻ daily
                    </span>
                  )}
                </p>
                <p className="text-caption text-hint">
                  {session.finished
                    ? `${formatDuration(session.focused_time)} / ${formatDuration(session.duration)}`
                    : `${formatDuration(session.duration)} planned`}
                </p>
              </div>
              {running && remaining !== null && (
                /* 计时读数：等宽数字 + 固定字号，逐秒更新不抖布局（§4.2/§10）。 */
                <span className="text-timer tabular-nums text-accent-strong" aria-label="Time remaining">
                  {formatClock(remaining)}
                </span>
              )}
              {running ? (
                <Button size="compact" onClick={() => void onStop(session)}>
                  Stop
                </Button>
              ) : (
                !session.finished &&
                !locked && (
                  <Button
                    size="compact"
                    disabled={
                      hasRunning ||
                      session.duration === null ||
                      session.duration <= 0
                    }
                    title={
                      hasRunning && session.id !== runningSessionId
                        ? "Another focus block is running"
                        : session.duration === null || session.duration <= 0
                          ? "Set a duration first"
                          : "Start this focus block"
                    }
                    onClick={() => void onStart(session)}
                  >
                    Start
                  </Button>
                )
              )}
              <CycleOptionsMenu
                cycle={session}
                runningElsewhere={hasRunning && session.id !== runningSessionId}
              />
            </li>
          );
        })}
      </ol>
      {!locked && (
        <AddFocusBlockForm
          dayId={day.id}
          open={adding}
          onOpen={() => setAdding(true)}
          onCancel={() => setAdding(false)}
          onAdded={() => setAdding(false)}
        />
      )}
      {error && (
        <p role="alert" className="px-2 pt-1 text-caption text-danger">
          {error}
        </p>
      )}
    </section>
  );
}

function AddFocusBlockForm({
  dayId,
  open,
  onOpen,
  onCancel,
  onAdded,
}: {
  dayId: string;
  open: boolean;
  onOpen: () => void;
  onCancel: () => void;
  onAdded: () => void;
}) {
  const qc = useQueryClient();
  const [title, setTitle] = useState("");
  const [durationMs, setDurationMs] = useState<number | null>(null);
  const [saving, setSaving] = useState(false);
  const { error, run } = useActionError();

  if (!open) {
    return (
      <button
        type="button"
        className="w-full rounded-sm px-2 py-1.5 text-left text-caption text-hint transition-colors duration-100 hover:bg-hover"
        onClick={onOpen}
      >
        + Add focus block
      </button>
    );
  }

  const submit = async () => {
    const trimmed = title.trim();
    if (!trimmed) return;
    setSaving(true);
    const ok = await run(() =>
      addSession({ day_cycle_id: dayId, title: trimmed, duration_ms: durationMs }),
    );
    setSaving(false);
    if (ok) {
      setTitle("");
      setDurationMs(null);
      invalidateCycles(qc);
      onAdded();
    }
  };

  return (
    <form
      className="flex flex-col gap-2 px-2 pt-1"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <Input
        autoFocus
        value={title}
        aria-label="Focus block title"
        placeholder="Focus block title"
        disabled={saving}
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && e.nativeEvent.isComposing) return;
          // Enter 走 form submit（default 行为），组合期间只确认候选。
        }}
      />
      <div className="flex flex-wrap items-center gap-1">
        {DURATION_PRESETS.map((preset) => {
          const selected = durationMs === preset.ms;
          return (
            <button
              key={preset.label}
              type="button"
              aria-pressed={selected}
              onClick={() => setDurationMs(preset.ms)}
              className={[
                "h-7 rounded-sm border px-2 text-caption",
                "transition-colors duration-100",
                selected ? "border-focus bg-focus-surface text-focus" : "border-control text-secondary hover:bg-hover",
              ].join(" ")}
            >
              {preset.label}
            </button>
          );
        })}
        <span className="flex-1" />
        <Button type="submit" size="compact" variant="primary" loading={saving} disabled={!title.trim()}>
          Add
        </Button>
        <Button type="button" size="compact" onClick={onCancel} disabled={saving}>
          Cancel
        </Button>
      </div>
      {error && (
        <p role="alert" className="text-caption text-danger">
          {error}
        </p>
      )}
    </form>
  );
}

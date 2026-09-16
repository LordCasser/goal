import { RemovalList } from "../../ui/RemovalList";
/**
 * 与日计划并排的专注块区（design.md §7）。跟随所选日，列出该日的 session：
 * 标题、计划时长、运行中显示 Stop + 剩余时间（1s 本地刷新，等宽数字防抖
 * 动）、ProgressDot 表达状态；已结束显示实际/计划时长（focused_time 与
 * duration 是两个不同概念，见 §7「计划时长、剩余时间、实际投入用不同名称」）。
 *
 * 数据由 CycleColumn 经 list_sessions 拉取后传入；session 变更会为所属日
 * 发 cycles:changed，查询随之失效刷新。
 */
import { useEffect, useId, useMemo, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ScrollModeHint } from "./ScrollModeHint";
import { Button, Input, ProgressDot, Select, SelectItem } from "../../ui";
import { addSession, finishCycle, getEditorWorkspace, startCycle, type Cycle, type TaskNode } from "../../lib/ipc";
import { formatClock } from "./dates";
import { invalidateCycles, useActionError } from "./actions";
import { CycleOptionsMenu } from "./CycleOptionsMenu";
import { useTranslation, formatDuration as formatLocalizedDuration } from "../../lib/i18n";
import { qk } from "../../lib/events";

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
  const { t } = useTranslation("planning");
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
    <section aria-label={t("focus.blocks")} className="flex min-h-0 flex-auto flex-col">
      <header className="workspace-scroll-heading shrink-0 border-b border-light px-6 pb-5 pt-5">
        <div className="flex items-center justify-between gap-2">
          <h3 className="flex-1 text-section-title font-semibold text-primary">{t("focus.blocks")}</h3>
          <ScrollModeHint />
          {!locked && <Button size="icon" variant="ghost" aria-label={t("focus.add")} title={t("focus.add")} onClick={() => setAdding(true)}>
            <svg className="h-4 w-4 shrink-0" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><path d="M8 3v10M3 8h10" strokeLinecap="round" /></svg>
          </Button>}
        </div>
        <p className="mt-0.5 text-caption text-hint">
          {t("focus.focusedPlanned", { focused: formatLocalizedDuration(day.focused_time), planned: formatLocalizedDuration(sorted.reduce((sum, session) => sum + (session.duration ?? 0), 0)) })}
        </p>
      </header>
      <div data-workspace-scroll-pane className="min-h-0 flex-auto overflow-y-auto px-4 py-5">
      <RemovalList as="ol" gap={12} empty={!adding && (
        <div className="rounded-lg border border-light bg-content px-5 py-6">
          <svg className="mb-4 h-7 w-7 text-hint" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.3" aria-hidden="true"><rect x="4" y="7" width="16" height="10" rx="2" /><path d="M4 3h16M4 21h16" strokeLinecap="round" /></svg>
          <p className="text-block-title font-medium text-primary">{t("focus.makeTime")}</p>
          <p className="mb-4 mt-2 text-menu text-secondary">{t("focus.description")}</p>
          {!locked && <Button size="compact" onClick={() => setAdding(true)}>{t("focus.addFirst")}</Button>}
        </div>
      )}>
        {sorted.map((session) => {
          const running = session.started && !session.finished;
          // duration 为空的块无法启动（cycle_duration_required）；运行中的块
          // duration 必非空，剩余时间可安全计算。
          const remaining =
            running && session.duration !== null && session.started_at !== null
              ? session.duration - (now - session.started_at)
              : null;
          return (
            <div
              key={session.id}
              data-session-id={session.id}
              className="flex items-center gap-2 rounded-lg border border-light bg-content px-3 py-3 transition-colors duration-100 hover:bg-hover"
            >
              <ProgressDot
                tone={running ? "active" : session.finished ? "done" : "idle"}
                className="mt-0.5"
              />
              <div className="min-w-0 flex-1">
                <p className="flex items-baseline gap-1.5 text-block-title font-semibold text-primary">
                  <span className="truncate">{session.title}</span>
                  {session.repeat_id && (
                    <span className="text-caption font-normal text-hint" title={t("focus.repeatDaily")}>
                      ↻ {t("focus.repeatDaily")}
                    </span>
                  )}
                </p>
                <p className="text-caption text-hint">
                  {session.finished
                    ? `${formatLocalizedDuration(session.focused_time)} / ${formatLocalizedDuration(session.duration ?? 0)}`
                    : t("focus.planned", { duration: formatLocalizedDuration(session.duration ?? 0) })}
                </p>
              </div>
              {running && remaining !== null && (
                /* 计时读数：等宽数字 + 固定字号，逐秒更新不抖布局（§4.2/§10）。 */
                <span className="text-timer tabular-nums text-accent-strong" aria-label={t("focus.timeRemaining")}>
                  {formatClock(remaining)}
                </span>
              )}
              {running ? (
                <Button size="compact" onClick={() => void onStop(session)}>
                  {t("focus.stop")}
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
                        ? t("focus.anotherRunning")
                        : session.duration === null || session.duration <= 0
                          ? t("focus.durationFirst")
                          : t("focus.start")
                    }
                    onClick={() => void onStart(session)}
                  >
                    {t("focus.start")}
                  </Button>
                )
              )}
              <CycleOptionsMenu
                cycle={session}
                runningElsewhere={hasRunning && session.id !== runningSessionId}
              />
            </div>
          );
        })}
      </RemovalList>
      {!locked && (
        <AddFocusBlockForm
          dayId={day.id}
          open={adding}
          onCancel={() => setAdding(false)}
          onAdded={() => setAdding(false)}
        />
      )}
      {error && (
        <p role="alert" className="px-2 pt-1 text-caption text-danger">
          {error}
        </p>
      )}
      </div>
    </section>
  );
}

const NO_TASK_VALUE = "__no_task__";

function flattenTasks(nodes: TaskNode[], result: TaskNode[] = []): TaskNode[] {
  for (const node of nodes) {
    result.push(node);
    flattenTasks(node.children ?? [], result);
  }
  return result;
}

export function AddFocusBlockForm({
  dayId,
  open,
  onCancel,
  onAdded,
}: {
  dayId: string;
  open: boolean;
  onCancel: () => void;
  onAdded: () => void;
}) {
  const { t } = useTranslation("planning");
  const taskSelectId = useId();
  const qc = useQueryClient();
  const [title, setTitle] = useState("");
  const [durationMs, setDurationMs] = useState<number | null>(null);
  const [taskId, setTaskId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const { error, run } = useActionError();
  const workspace = useQuery({
    queryKey: qk.editorWorkspace(dayId),
    queryFn: () => getEditorWorkspace(dayId),
    enabled: open,
  });
  const taskOptions = useMemo(
    () => flattenTasks(workspace.data?.tasks ?? []).filter(
      (task) => task.cycle_id === dayId && task.proposal == null && task.title.trim().length > 0,
    ),
    [workspace.data?.tasks, dayId],
  );

  useEffect(() => {
    setTaskId(null);
  }, [dayId]);

  useEffect(() => {
    if (taskId !== null && !taskOptions.some((task) => task.id === taskId)) {
      setTaskId(null);
    }
  }, [taskId, taskOptions]);

  if (!open) {
    return null;
  }

  const submit = async () => {
    const trimmed = title.trim();
    if (!trimmed) return;
    setSaving(true);
    const ok = await run(() =>
      addSession({ day_cycle_id: dayId, title: trimmed, duration_ms: durationMs, task_id: taskId }),
    );
    setSaving(false);
    if (ok) {
      setTitle("");
      setDurationMs(null);
      setTaskId(null);
      invalidateCycles(qc);
      onAdded();
    }
  };

  return (
    <form
      className="mt-3 flex flex-col gap-3 rounded-lg border border-light bg-content p-3"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <Input
        autoFocus
        value={title}
        aria-label={t("focus.addTitle")}
        placeholder={t("focus.addTitle")}
        disabled={saving}
        onChange={(e) => setTitle(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && e.nativeEvent.isComposing) return;
          // Enter 走 form submit（default 行为），组合期间只确认候选。
        }}
      />
      <div className="flex min-w-0 flex-col gap-1">
        <label htmlFor={taskSelectId} className="text-caption text-secondary">
          {t("focus.taskOptional")}
        </label>
        <Select
          id={taskSelectId}
          value={taskId ?? NO_TASK_VALUE}
          onValueChange={(value) => setTaskId(value === NO_TASK_VALUE ? null : value)}
          disabled={saving}
          aria-label={t("focus.taskOptional")}
          triggerClassName="w-full"
        >
          <SelectItem value={NO_TASK_VALUE}>{t("focus.noTask")}</SelectItem>
          {taskOptions.map((task) => (
            <SelectItem key={task.id} value={task.id}>{task.title.trim()}</SelectItem>
          ))}
        </Select>
      </div>
      <div className="flex flex-wrap items-center gap-1">
        {DURATION_PRESETS.map((preset) => {
          const selected = durationMs === preset.ms;
          return (
            <button
              key={preset.ms === null ? t("focus.noDuration") : t(`focus.preset${preset.ms / 60_000}`)}
              type="button"
              aria-pressed={selected}
              disabled={saving}
              onClick={() => setDurationMs(preset.ms)}
              className={[
                "h-7 rounded-sm border px-2 text-caption",
                "transition-colors duration-100",
                selected ? "border-focus bg-focus-surface text-focus" : "border-control text-secondary hover:bg-hover",
              ].join(" ")}
            >
              {preset.ms === null ? t("focus.noDuration") : t(`focus.preset${preset.ms / 60_000}`)}
            </button>
          );
        })}
        <span className="flex-1" />
        <Button type="submit" size="compact" variant="primary" loading={saving} disabled={saving || !title.trim()}>
          {t("focus.add")}
        </Button>
        <Button type="button" size="compact" onClick={onCancel} disabled={saving}>
          {t("focus.cancel")}
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

/** Long-term cycle duration and custom progress-check configuration. */
import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { Button, Dialog, Select, SelectItem } from "../../ui";
import { createPlanningCycle, type Cycle, type ProgressCheck } from "../../lib/ipc";
import { errorMessage, formatDate, useTranslation } from "../../lib/i18n";
import { todayISO, addDaysISO } from "./dates";
import { deriveCustomTimeline, deriveTimeline, type TimelineMonths } from "./timeline";
import { useActionError } from "./actions";

type DurationChoice = TimelineMonths | "custom";
type CheckMode = "repeat" | "once";
type RepeatUnit = "days" | "weeks";

const OPTIONS: ReadonlyArray<TimelineMonths> = [1, 3, 6];
const CUSTOM_DEFAULT_DAYS = 84;

function intervalDays(amount: string, unit: RepeatUnit): number {
  const parsed = Number(amount);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) return 0;
  return unit === "weeks" ? parsed * 7 : parsed;
}

export function DurationDialog({
  open,
  onClose,
  onCreated,
}: {
  open: boolean;
  onClose: () => void;
  onCreated?: (cycle: Cycle) => void;
}) {
  const { t } = useTranslation("planning");
  const [choice, setChoice] = useState<DurationChoice>(3);
  const [today, setToday] = useState(() => todayISO());
  const [customStart, setCustomStart] = useState(() => todayISO());
  const [customEnd, setCustomEnd] = useState(() => addDaysISO(todayISO(), CUSTOM_DEFAULT_DAYS));
  const [checkMode, setCheckMode] = useState<CheckMode>("repeat");
  const [repeatAmount, setRepeatAmount] = useState("2");
  const [repeatUnit, setRepeatUnit] = useState<RepeatUnit>("weeks");
  const [onceDate, setOnceDate] = useState(() => addDaysISO(todayISO(), CUSTOM_DEFAULT_DAYS / 2));
  const [creating, setCreating] = useState(false);
  const creatingRef = useRef(false);
  const { error, run, fail, dismiss } = useActionError();

  useEffect(() => {
    if (!open) return;
    const now = todayISO();
    setChoice(3);
    setToday(now);
    setCustomStart(now);
    setCustomEnd(addDaysISO(now, CUSTOM_DEFAULT_DAYS));
    setCheckMode("repeat");
    setRepeatAmount("2");
    setRepeatUnit("weeks");
    setOnceDate(addDaysISO(now, CUSTOM_DEFAULT_DAYS / 2));
    dismiss();
  }, [open, dismiss]);

  const customCheck = useMemo<ProgressCheck>(() => (
    checkMode === "once"
      ? { kind: "once", date: onceDate }
      : { kind: "repeat", every_days: intervalDays(repeatAmount, repeatUnit) }
  ), [checkMode, onceDate, repeatAmount, repeatUnit]);
  const customTimeline = useMemo(
    () => deriveCustomTimeline(customStart, customEnd, customCheck),
    [customStart, customEnd, customCheck],
  );
  const customError = customTimeline.error
    ? errorMessage({ code: customTimeline.error, message: customTimeline.error })
    : null;
  const canCreate = choice !== "custom" || customTimeline.error === null;
  const durationChoiceRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const moveDurationChoice = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const direction = event.key === "ArrowDown" || event.key === "ArrowRight"
      ? 1
      : event.key === "ArrowUp" || event.key === "ArrowLeft"
        ? -1
        : 0;
    if (direction === 0) return;
    event.preventDefault();
    const next = (index + direction + 4) % 4;
    const nextChoice: DurationChoice = next === 3 ? "custom" : OPTIONS[next]!;
    setChoice(nextChoice);
    dismiss();
    durationChoiceRefs.current[next]?.focus();
  };

  const changeCustom = <T,>(setter: (value: T) => void, value: T) => {
    dismiss();
    setter(value);
  };

  const onConfirm = async () => {
    if (creating || creatingRef.current) return;
    if (!canCreate) {
      if (customTimeline.error) fail(customError ?? errorMessage({ code: customTimeline.error, message: customTimeline.error }));
      return;
    }
    creatingRef.current = true;
    setCreating(true);
    const args = choice === "custom"
      ? {
        cycle_type: "month" as const,
        parent_id: null,
        starts_on: customStart,
        ends_on: customEnd,
        progress_check: customCheck,
      }
      : {
        cycle_type: "month" as const,
        duration_months: choice,
        parent_id: null,
      };
    const created = await run(() => createPlanningCycle(args));
    creatingRef.current = false;
    setCreating(false);
    if (created) {
      onCreated?.(created);
      onClose();
    }
  };

  return (
    <Dialog
      open={open}
      onClose={() => { if (!creating) onClose(); }}
      wide
      title={t("duration.longTerm")}
      footer={(
        <>
          <Button onClick={onClose} disabled={creating}>{t("cycle.cancel")}</Button>
          <Button variant="primary" loading={creating} disabled={!canCreate} onClick={() => void onConfirm()}>
            {t("duration.create")}
          </Button>
        </>
      )}
    >
      <div className="flex flex-col gap-5 sm:flex-row">
        <div role="radiogroup" aria-label={t("duration.length")} className="flex flex-col gap-2 sm:w-40 sm:shrink-0">
          {OPTIONS.map((option) => {
            const selected = choice === option;
            const endsOn = deriveTimeline(today, option).nodes[2].date;
            return (
              <button
                key={option}
                ref={(element) => { durationChoiceRefs.current[OPTIONS.indexOf(option)] = element; }}
                type="button"
                role="radio"
                aria-checked={selected}
                tabIndex={selected ? 0 : -1}
                onKeyDown={(event) => moveDurationChoice(event, OPTIONS.indexOf(option))}
                onClick={() => { dismiss(); setChoice(option); }}
                className={[
                  "flex cursor-pointer flex-col items-start gap-0.5 rounded-lg border px-4 py-3 text-left",
                  "transition-colors duration-150",
                  selected ? "border-focus bg-focus-surface" : "border-light hover:bg-hover",
                ].join(" ")}
              >
                <span className="text-body font-semibold text-primary">{t("duration.month", { count: option })}</span>
                <span className="text-caption text-hint">
                  <span>{t("duration.weeks", { count: option * 4 })}</span> · {t("duration.ends")} {formatDate(endsOn, { month: "short", day: "numeric" })}
                </span>
              </button>
            );
          })}
          <button
            ref={(element) => { durationChoiceRefs.current[3] = element; }}
            type="button"
            role="radio"
            aria-checked={choice === "custom"}
            tabIndex={choice === "custom" ? 0 : -1}
            onKeyDown={(event) => moveDurationChoice(event, 3)}
            onClick={() => { dismiss(); setChoice("custom"); }}
            className={[
              "flex cursor-pointer flex-col items-start gap-0.5 rounded-lg border px-4 py-3 text-left",
              "transition-colors duration-150",
              choice === "custom" ? "border-focus bg-focus-surface" : "border-light hover:bg-hover",
            ].join(" ")}
          >
            <span className="text-body font-semibold text-primary">{t("duration.custom")}</span>
            <span className="text-caption text-hint">{t("duration.customHelp")}</span>
          </button>
        </div>

        <div className="min-w-0 flex-1 sm:border-l sm:border-light sm:pl-5">
          {choice === "custom" ? (
            <CustomCycleEditor
              t={t}
              start={customStart}
              end={customEnd}
              checkMode={checkMode}
              repeatAmount={repeatAmount}
              repeatUnit={repeatUnit}
              onceDate={onceDate}
              timeline={customTimeline}
              error={customError}
              errorCode={customTimeline.error}
              onStart={(value) => changeCustom(setCustomStart, value)}
              onEnd={(value) => changeCustom(setCustomEnd, value)}
              onCheckMode={(value) => changeCustom(setCheckMode, value)}
              onRepeatAmount={(value) => changeCustom(setRepeatAmount, value)}
              onRepeatUnit={(value) => changeCustom(setRepeatUnit, value)}
              onOnceDate={(value) => changeCustom(setOnceDate, value)}
            />
          ) : (
            <PresetTimeline today={today} months={choice} t={t} />
          )}
        </div>
      </div>
      {error && <p role="alert" className="mt-3 text-caption text-danger">{error}</p>}
    </Dialog>
  );
}

function PresetTimeline({
  today,
  months,
  t,
}: {
  today: string;
  months: TimelineMonths;
  t: (key: string, options?: Record<string, unknown>) => string;
}) {
  const timeline = deriveTimeline(today, months);
  return (
    <>
      <ol className="flex flex-col gap-4">
        {timeline.nodes.map((node) => (
          <li key={node.key} data-timeline-node={node.key}>
            <p className="text-caption text-secondary">{t(`timeline.${node.key}`)}</p>
            <p className="text-body font-medium text-primary">
              {formatDate(node.date, { month: "short", day: "numeric" })} <span className="ml-1 font-normal text-hint">{node.date.slice(0, 4)}</span>
            </p>
          </li>
        ))}
      </ol>
      <p className="mt-4 border-t border-light pt-3 text-caption font-medium text-secondary">
        {t("duration.weeksTotal", { count: timeline.weeks })}
      </p>
    </>
  );
}

function CustomCycleEditor({
  t,
  start,
  end,
  checkMode,
  repeatAmount,
  repeatUnit,
  onceDate,
  timeline,
  error,
  errorCode,
  onStart,
  onEnd,
  onCheckMode,
  onRepeatAmount,
  onRepeatUnit,
  onOnceDate,
}: {
  t: (key: string, options?: Record<string, unknown>) => string;
  start: string;
  end: string;
  checkMode: CheckMode;
  repeatAmount: string;
  repeatUnit: RepeatUnit;
  onceDate: string;
  timeline: ReturnType<typeof deriveCustomTimeline>;
  error: string | null;
  errorCode: ReturnType<typeof deriveCustomTimeline>["error"];
  onStart: (value: string) => void;
  onEnd: (value: string) => void;
  onCheckMode: (value: CheckMode) => void;
  onRepeatAmount: (value: string) => void;
  onRepeatUnit: (value: RepeatUnit) => void;
  onOnceDate: (value: string) => void;
}) {
  const checkModeLabel = (mode: CheckMode) => mode === "repeat" ? t("duration.repeat") : t("duration.once");
  const errorId = "duration-custom-error";
  const rangeInvalid = errorCode === "invalid_cycle_range";
  const progressInvalid = errorCode === "invalid_progress_check";
  const checkModeRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const moveCheckMode = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const direction = event.key === "ArrowRight" || event.key === "ArrowDown"
      ? 1
      : event.key === "ArrowLeft" || event.key === "ArrowUp"
        ? -1
        : 0;
    if (direction === 0) return;
    event.preventDefault();
    const next = (index + direction + 2) % 2;
    onCheckMode(next === 0 ? "repeat" : "once");
    checkModeRefs.current[next]?.focus();
  };
  return (
    <div className="flex flex-col gap-4">
      <div className="grid min-w-0 grid-cols-1 gap-2.5">
        <label className="flex min-w-0 flex-col gap-1 text-caption text-secondary">
          {t("duration.startsOn")}
          <input aria-label={t("duration.startsOn")} aria-invalid={rangeInvalid} aria-describedby={rangeInvalid ? errorId : undefined} className="h-9 min-w-0 rounded-md border border-control bg-content px-3 text-body text-primary" type="date" value={start} onChange={(event) => onStart(event.currentTarget.value)} />
        </label>
        <label className="flex min-w-0 flex-col gap-1 text-caption text-secondary">
          {t("duration.endsOn")}
          <input aria-label={t("duration.endsOn")} aria-invalid={rangeInvalid} aria-describedby={rangeInvalid ? errorId : undefined} className="h-9 min-w-0 rounded-md border border-control bg-content px-3 text-body text-primary" type="date" min={start || undefined} value={end} onChange={(event) => onEnd(event.currentTarget.value)} />
        </label>
      </div>

      <fieldset className="flex min-w-0 flex-col gap-2">
        <legend className="text-caption text-secondary">{t("duration.progressCheck")}</legend>
        <div role="radiogroup" aria-label={t("duration.progressCheck")} className="flex flex-wrap items-center gap-x-5 gap-y-1">
          {(["repeat", "once"] as const).map((mode) => (
            <button
              key={mode}
              ref={(element) => { checkModeRefs.current[mode === "repeat" ? 0 : 1] = element; }}
              type="button"
              role="radio"
              aria-checked={checkMode === mode}
              tabIndex={checkMode === mode ? 0 : -1}
              onKeyDown={(event) => moveCheckMode(event, mode === "repeat" ? 0 : 1)}
              onClick={() => onCheckMode(mode)}
              className={[
                "inline-flex h-8 cursor-pointer items-center gap-2 rounded-sm px-0.5 text-body transition-colors duration-150 focus-visible:outline-2 focus-visible:outline-focus",
                checkMode === mode ? "font-medium text-primary" : "text-secondary hover:text-primary",
              ].join(" ")}
            >
              <span aria-hidden="true" className={`flex h-3.5 w-3.5 shrink-0 items-center justify-center rounded-full border ${checkMode === mode ? "border-focus" : "border-control"}`}>
                {checkMode === mode && <span className="h-1.5 w-1.5 rounded-full bg-focus" />}
              </span>
              {checkModeLabel(mode)}
            </button>
          ))}
        </div>
        {checkMode === "repeat" ? (
          <div className="flex min-w-0 flex-wrap items-center gap-2 text-body text-secondary">
            <span className="shrink-0">{t("duration.every")}</span>
            <div className="inline-flex min-w-0 items-center gap-1">
              <input aria-label={t("duration.interval")} aria-invalid={progressInvalid} aria-describedby={progressInvalid ? errorId : undefined} className="h-8 w-16 shrink-0 appearance-none rounded-md border border-control bg-content px-2 text-center text-body tabular-nums text-primary [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none" type="number" min={1} step={1} value={repeatAmount} onChange={(event) => onRepeatAmount(event.currentTarget.value)} />
              <Select aria-label={t("duration.intervalUnit")} value={repeatUnit} onValueChange={(value) => onRepeatUnit(value as RepeatUnit)} triggerClassName="h-8! w-auto cursor-pointer border-transparent! bg-transparent! px-2! hover:bg-hover">
                <SelectItem value="days">{t("duration.days")}</SelectItem>
                <SelectItem value="weeks">{t("duration.weeksUnit")}</SelectItem>
              </Select>
            </div>
          </div>
        ) : (
          <label className="flex min-w-0 flex-wrap items-center gap-2 text-body text-secondary">
            <span className="shrink-0">{t("duration.onceDate")}</span>
            <input aria-label={t("duration.onceDate")} aria-invalid={progressInvalid} aria-describedby={progressInvalid ? errorId : undefined} className="h-8 w-40 min-w-0 max-w-full rounded-md border border-control bg-content px-2 text-body text-primary" type="date" min={start || undefined} max={addDaysISO(end, -1)} value={onceDate} onChange={(event) => onOnceDate(event.currentTarget.value)} />
          </label>
        )}
      </fieldset>

      {error && <p id={errorId} role="alert" className="text-caption text-danger">{error}</p>}
      <div className="pt-1">
        <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
          <p className="text-body font-medium text-primary">
            {timeline.days === null ? t("duration.rangeUnavailable") : t("duration.customDays", { count: timeline.days })}
          </p>
          <p className="text-caption text-secondary">{t("duration.checkCount", { count: timeline.totalChecks })}</p>
        </div>
        {timeline.totalChecks > 0 ? (
          <>
            <ul className="mt-1 flex flex-wrap gap-x-2 gap-y-0.5 text-caption text-hint">
              {timeline.checkDates.map((date) => <li key={date}>{formatDate(date, { month: "short", day: "numeric" })}</li>)}
            </ul>
            {timeline.totalChecks > timeline.checkDates.length && <p className="text-caption text-hint">{t("duration.moreChecks", { count: timeline.totalChecks - timeline.checkDates.length })}</p>}
          </>
        ) : (
          <p className="mt-1 text-caption text-hint">{t("duration.noChecks")}</p>
        )}
      </div>
    </div>
  );
}

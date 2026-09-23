import { useRef, useState, type KeyboardEvent } from "react";
import { cn } from "../../ui";
import { formatDate, useTranslation } from "../../lib/i18n";
import { addDaysISO, todayISO } from "../planner/dates";
import { addMonthsISO, monthBounds } from "./calendar-model";

/** An inline date grid sized for the planner dialog, with no native popup. */
export function CalendarDatePicker({ value, onChange, weekStartDay = 1, disabled = false }: {
  value: string;
  onChange: (date: string) => void;
  weekStartDay?: number;
  disabled?: boolean;
}) {
  const { t } = useTranslation("planning");
  const [visibleMonth, setVisibleMonth] = useState(() => monthBounds(value || todayISO()).start);
  const [focusedDate, setFocusedDate] = useState(value || todayISO());
  const gridRef = useRef<HTMLDivElement>(null);
  const firstDay = monthBounds(visibleMonth).start;
  const weekday = new Date(`${firstDay}T12:00:00`).getDay();
  const offset = (weekday - weekStartDay + 7) % 7;
  const firstCell = addDaysISO(firstDay, -offset);
  const dates = Array.from({ length: 42 }, (_, index) => addDaysISO(firstCell, index));
  const tabStop = dates.includes(focusedDate) ? focusedDate : firstDay;
  const weekdayLabels = Array.from({ length: 7 }, (_, index) => {
    const date = addDaysISO("2026-09-20", (weekStartDay + index) % 7);
    return formatDate(date, { weekday: "short" });
  });
  const pick = (date: string) => {
    if (disabled) return;
    onChange(date);
    setVisibleMonth(monthBounds(date).start);
    setFocusedDate(date);
  };
  const changeMonth = (offset: number) => {
    if (disabled) return;
    const next = monthBounds(addMonthsISO(visibleMonth, offset)).start;
    setVisibleMonth(next);
    setFocusedDate(next);
  };
  const moveFocus = (event: KeyboardEvent<HTMLButtonElement>, date: string) => {
    if (disabled) return;
    const dayDelta = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 }[event.key];
    if (dayDelta === undefined && event.key !== "PageUp" && event.key !== "PageDown") return;
    event.preventDefault();
    const next = dayDelta === undefined
      ? addMonthsISO(date, (event.key === "PageUp" ? -1 : 1) * (event.shiftKey ? 12 : 1))
      : addDaysISO(date, dayDelta);
    setFocusedDate(next);
    setVisibleMonth(monthBounds(next).start);
    requestAnimationFrame(() => gridRef.current?.querySelector<HTMLButtonElement>(`[data-picker-date="${next}"]`)?.focus());
  };

  return <div role="group" className="mx-auto mt-3 w-fit rounded-lg border border-light bg-content p-3" aria-label={t("calendar.chooseDate")}>
    <div className="mb-3 flex items-center justify-between gap-2">
      <button type="button" disabled={disabled} className="flex size-8 items-center justify-center rounded-md text-body text-secondary hover:bg-hover focus-visible:outline-2 focus-visible:outline-focus disabled:cursor-not-allowed disabled:opacity-50"
        aria-label={t("calendar.previousMonth")} onClick={() => changeMonth(-1)}>‹</button>
      <strong className="text-menu font-semibold text-primary">{formatDate(visibleMonth, { year: "numeric", month: "long" })}</strong>
      <button type="button" disabled={disabled} className="flex size-8 items-center justify-center rounded-md text-body text-secondary hover:bg-hover focus-visible:outline-2 focus-visible:outline-focus disabled:cursor-not-allowed disabled:opacity-50"
        aria-label={t("calendar.nextMonth")} onClick={() => changeMonth(1)}>›</button>
    </div>
    <div ref={gridRef} className="grid grid-cols-7 gap-1 text-center">
      {weekdayLabels.map((label, index) => <span key={index} className="py-1 text-caption font-medium text-hint">{label}</span>)}
      {dates.map((date) => <button key={date} type="button" data-picker-date={date}
        disabled={disabled}
        aria-label={formatDate(date, { year: "numeric", month: "long", day: "numeric", weekday: "long" })}
        aria-pressed={date === value}
        aria-current={date === todayISO() ? "date" : undefined}
        tabIndex={date === tabStop ? 0 : -1}
        onClick={() => pick(date)} onKeyDown={(event) => moveFocus(event, date)}
        className={cn("flex size-10 items-center justify-center rounded-md text-menu tabular-nums transition-colors focus-visible:outline-2 focus-visible:outline-focus disabled:cursor-not-allowed disabled:opacity-50",
          date.slice(0, 7) === visibleMonth.slice(0, 7) ? "text-primary hover:bg-hover" : "text-hint hover:bg-hover",
          date === todayISO() && date !== value && "ring-1 ring-inset ring-control",
          date === value && "bg-focus text-white hover:bg-focus")}>{Number(date.slice(8, 10))}</button>)}
    </div>
  </div>;
}

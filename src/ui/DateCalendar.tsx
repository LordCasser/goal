import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { formatDate, useTranslation } from "../lib/i18n";
import { cn } from "./cn";

function iso(date: Date): string {
  return `${date.getUTCFullYear()}-${String(date.getUTCMonth() + 1).padStart(2, "0")}-${String(date.getUTCDate()).padStart(2, "0")}`;
}

function dateOf(value: string): Date {
  const [year = 2026, month = 1, day = 1] = value.split("-").map(Number);
  return new Date(Date.UTC(year, month - 1, day));
}

function addDays(value: string, amount: number): string {
  const date = dateOf(value);
  date.setUTCDate(date.getUTCDate() + amount);
  return iso(date);
}

function addMonths(value: string, amount: number): string {
  const date = dateOf(value);
  const target = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth() + amount, 1));
  const last = new Date(Date.UTC(target.getUTCFullYear(), target.getUTCMonth() + 1, 0)).getUTCDate();
  target.setUTCDate(Math.min(date.getUTCDate(), last));
  return iso(target);
}

function today(): string {
  const now = new Date();
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
}

function validDate(value: string): boolean {
  return /^\d{4}-\d{2}-\d{2}$/.test(value) && iso(dateOf(value)) === value;
}

export type DateCalendarProps = {
  value: string;
  onChange: (date: string) => void;
  min?: string;
  max?: string;
  weekStartDay?: number;
  disabled?: boolean;
  className?: string;
};

/** Shared local-day calendar. The ISO strings are calendar identities, not UTC instants. */
export function DateCalendar({ value, onChange, min, max, weekStartDay = 1, disabled = false, className }: DateCalendarProps) {
  const { t } = useTranslation("common");
  const initial = validDate(value) ? value : today();
  const [visibleDate, setVisibleDate] = useState(initial);
  const [focusedDate, setFocusedDate] = useState(initial);
  const [monthsView, setMonthsView] = useState(false);
  const gridRef = useRef<HTMLDivElement>(null);
  const month = visibleDate.slice(0, 7);
  const first = `${month}-01`;
  const offset = (dateOf(first).getUTCDay() - weekStartDay + 7) % 7;
  const dates = Array.from({ length: 42 }, (_, index) => addDays(first, index - offset));
  const tabStop = dates.includes(focusedDate) && (!min || focusedDate >= min) && (!max || focusedDate <= max)
    ? focusedDate : dates.find((date) => date.slice(0, 7) === month && (!min || date >= min) && (!max || date <= max));
  const weekdayLabels = Array.from({ length: 7 }, (_, index) => formatDate(addDays("2026-09-20", (weekStartDay + index) % 7), { weekday: "short" }));

  useEffect(() => {
    if (!validDate(value)) return;
    setVisibleDate(value);
    setFocusedDate(value);
  }, [value]);

  const choose = (date: string) => {
    if (disabled || (min && date < min) || (max && date > max)) return;
    setVisibleDate(date);
    setFocusedDate(date);
    onChange(date);
  };
  const moveFocus = (event: KeyboardEvent<HTMLButtonElement>, date: string) => {
    const delta = { ArrowLeft: -1, ArrowRight: 1, ArrowUp: -7, ArrowDown: 7 }[event.key];
    if (delta === undefined && event.key !== "PageUp" && event.key !== "PageDown" && event.key !== "Home" && event.key !== "End") return;
    event.preventDefault();
    const next = delta !== undefined ? addDays(date, delta)
      : event.key === "PageUp" || event.key === "PageDown" ? addMonths(date, (event.key === "PageUp" ? -1 : 1) * (event.shiftKey ? 12 : 1))
      : addDays(date, event.key === "Home" ? -((dateOf(date).getUTCDay() - weekStartDay + 7) % 7) : 6 - ((dateOf(date).getUTCDay() - weekStartDay + 7) % 7));
    if ((min && next < min) || (max && next > max)) return;
    setFocusedDate(next);
    setVisibleDate(next);
    requestAnimationFrame(() => gridRef.current?.querySelector<HTMLButtonElement>(`[data-picker-date="${next}"]`)?.focus());
  };
  const shift = (amount: number) => {
    const next = addMonths(visibleDate, amount);
    setVisibleDate(next);
    setFocusedDate(next);
  };
  const year = Number(visibleDate.slice(0, 4));

  return <div role="group" aria-label={t("picker.chooseDate")} className={cn("w-[292px] max-w-full rounded-lg bg-content p-3", className)}>
    <div className="mb-2 flex h-8 items-center justify-between gap-2">
      <button type="button" disabled={disabled} aria-label={t(monthsView ? "picker.previousYear" : "picker.previousMonth")}
        onClick={() => shift(monthsView ? -12 : -1)} className="flex size-8 cursor-pointer items-center justify-center rounded-md text-secondary hover:bg-hover disabled:opacity-50">
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><path d="m10 3-5 5 5 5" /></svg>
      </button>
      <button type="button" disabled={disabled} aria-label={t("picker.chooseMonth")} aria-expanded={monthsView}
        onClick={() => setMonthsView(!monthsView)} className="cursor-pointer rounded-md px-2 py-1 text-menu font-semibold text-primary hover:bg-hover disabled:opacity-50">
        {formatDate(visibleDate, monthsView ? { year: "numeric" } : { year: "numeric", month: "long" })}
      </button>
      <button type="button" disabled={disabled} aria-label={t(monthsView ? "picker.nextYear" : "picker.nextMonth")}
        onClick={() => shift(monthsView ? 12 : 1)} className="flex size-8 cursor-pointer items-center justify-center rounded-md text-secondary hover:bg-hover disabled:opacity-50">
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><path d="m6 3 5 5-5 5" /></svg>
      </button>
    </div>
    {monthsView ? <div className="grid grid-cols-3 gap-1 py-1">
      {Array.from({ length: 12 }, (_, index) => {
        const candidate = `${year}-${String(index + 1).padStart(2, "0")}-01`;
        const monthEnd = iso(new Date(Date.UTC(year, index + 1, 0)));
        return <button key={candidate} type="button" disabled={disabled || !!(min && monthEnd < min) || !!(max && candidate > max)}
          onClick={() => { setVisibleDate(candidate); setFocusedDate(candidate); setMonthsView(false); }}
          className={cn("h-10 cursor-pointer rounded-md text-menu text-primary hover:bg-hover disabled:cursor-not-allowed disabled:text-hint disabled:opacity-40", candidate.slice(0, 7) === month && "bg-focus-surface font-semibold text-focus")}>
          {formatDate(candidate, { month: "short" })}
        </button>;
      })}
    </div> : <div ref={gridRef} className="grid grid-cols-7 gap-0.5 text-center">
      {weekdayLabels.map((label, index) => <span key={index} className="flex h-7 items-center justify-center text-caption font-medium text-hint">{label}</span>)}
      {dates.map((date) => {
        const outOfRange = !!(min && date < min) || !!(max && date > max);
        return <button key={date} type="button" data-picker-date={date} disabled={disabled || outOfRange}
          aria-label={formatDate(date, { year: "numeric", month: "long", day: "numeric", weekday: "long" })}
          aria-pressed={date === value} aria-current={date === today() ? "date" : undefined}
          tabIndex={date === tabStop ? 0 : -1} onClick={() => choose(date)} onKeyDown={(event) => moveFocus(event, date)}
          className={cn("flex size-9 cursor-pointer items-center justify-center rounded-md text-menu tabular-nums transition-colors focus-visible:outline-2 focus-visible:outline-focus disabled:cursor-not-allowed disabled:opacity-35",
            date.slice(0, 7) === month ? "text-primary hover:bg-hover" : "text-hint hover:bg-hover",
            date === today() && date !== value && "ring-1 ring-inset ring-control",
            date === value && "bg-focus font-semibold text-white hover:bg-focus")}>{Number(date.slice(8, 10))}</button>;
      })}
    </div>}
  </div>;
}

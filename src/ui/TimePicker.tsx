import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { useTranslation } from "../lib/i18n";
import { cn } from "./cn";
import { Input } from "./Input";
import { Popover } from "./Popover";

export type TimePickerProps = {
  value: string;
  onChange: (value: string) => void;
  "aria-label": string;
  "aria-invalid"?: boolean;
  disabled?: boolean;
  compact?: boolean;
  className?: string;
};

const two = (value: number) => String(value).padStart(2, "0");

/** Text entry plus an app-styled hour/minute list. Values remain local HH:mm. */
export function TimePicker({ value, onChange, disabled, compact, className, "aria-label": label, "aria-invalid": invalid }: TimePickerProps) {
  const { t } = useTranslation("common");
  const [open, setOpen] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const matched = /^(\d{2}):(\d{2})$/.exec(value);
  const hour = matched && Number(matched[1]) < 24 ? Number(matched[1]) : 9;
  const minute = matched && Number(matched[2]) < 60 ? Number(matched[2]) : 0;

  useEffect(() => {
    if (!open) return;
    const frame = requestAnimationFrame(() => {
      panelRef.current?.querySelector<HTMLElement>(`[data-time-hour="${hour}"]`)?.scrollIntoView({ block: "center" });
      panelRef.current?.querySelector<HTMLElement>(`[data-time-minute="${minute}"]`)?.scrollIntoView({ block: "center" });
    });
    return () => cancelAnimationFrame(frame);
  }, [open, hour, minute]);

  const move = (event: KeyboardEvent<HTMLButtonElement>, part: "hour" | "minute", current: number) => {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp" && event.key !== "Home" && event.key !== "End") return;
    event.preventDefault();
    const limit = part === "hour" ? 24 : 60;
    const next = event.key === "Home" ? 0 : event.key === "End" ? limit - 1
      : (current + (event.key === "ArrowDown" ? 1 : -1) + limit) % limit;
    onChange(part === "hour" ? `${two(next)}:${two(minute)}` : `${two(hour)}:${two(next)}`);
    requestAnimationFrame(() => panelRef.current?.querySelector<HTMLButtonElement>(`[data-time-${part}="${next}"]`)?.focus());
  };

  return <div className={cn("relative min-w-0", className)}>
    <Input type="text" inputMode="numeric" autoComplete="off" value={value} placeholder={t("picker.timeFormat")}
      aria-label={label} aria-invalid={invalid} disabled={disabled} onChange={(event) => onChange(event.currentTarget.value)}
      className={cn("w-full pr-10 font-mono tabular-nums aria-invalid:border-danger", compact && "h-8 px-2 pr-9 text-menu")} />
    <button ref={buttonRef} type="button" disabled={disabled} aria-label={`${label}: ${t("picker.openTime")}`}
      aria-haspopup="dialog" aria-expanded={open} onClick={() => setOpen(!open)}
      className={cn("absolute right-0 top-0 flex h-9 w-9 cursor-pointer items-center justify-center rounded-r-md text-secondary transition-colors hover:bg-hover hover:text-primary disabled:cursor-not-allowed disabled:opacity-40", compact && "h-8 w-8")}>
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
        <circle cx="8" cy="8" r="6"/><path d="M8 4.5V8l2.5 1.5" />
      </svg>
    </button>
    <Popover open={open} onClose={() => setOpen(false)} anchorRef={buttonRef} role="dialog" label={`${label}: ${t("picker.openTime")}`}
      initialFocus={`[data-time-hour="${hour}"]`}
      className="w-[184px] overflow-hidden p-2">
      <div ref={panelRef} className="grid grid-cols-2 gap-1.5">
        {(["hour", "minute"] as const).map((part) => <div key={part}>
          <div className="mb-1 px-1 text-center text-caption font-medium text-secondary">{t(`picker.${part}`)}</div>
          <div className="max-h-48 overflow-y-auto overscroll-contain rounded-md bg-subtle p-0.5" role="group" aria-label={t(`picker.${part}`)}>
            {Array.from({ length: part === "hour" ? 24 : 60 }, (_, number) => {
              const selected = number === (part === "hour" ? hour : minute);
              return <button key={number} type="button" data-time-hour={part === "hour" ? number : undefined}
                data-time-minute={part === "minute" ? number : undefined} aria-pressed={selected}
                onKeyDown={(event) => move(event, part, number)}
                onClick={() => { onChange(part === "hour" ? `${two(number)}:${two(minute)}` : `${two(hour)}:${two(number)}`); if (part === "minute") setOpen(false); }}
                className={cn("flex h-8 w-full cursor-pointer items-center justify-center rounded-md text-menu tabular-nums transition-colors hover:bg-hover",
                  selected ? "bg-focus text-white hover:bg-focus" : "text-primary")}>{two(number)}</button>;
            })}
          </div>
        </div>)}
      </div>
    </Popover>
  </div>;
}

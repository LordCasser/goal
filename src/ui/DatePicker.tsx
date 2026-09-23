import { useRef, useState } from "react";
import { useTranslation } from "../lib/i18n";
import { cn } from "./cn";
import { DateCalendar } from "./DateCalendar";
import { Input } from "./Input";
import { Popover } from "./Popover";

export type DatePickerProps = {
  value: string;
  onChange: (value: string) => void;
  "aria-label": string;
  "aria-invalid"?: boolean;
  "aria-describedby"?: string;
  min?: string;
  max?: string;
  disabled?: boolean;
  compact?: boolean;
  className?: string;
};

/** Editable ISO date with the same calendar on every WebView platform. */
export function DatePicker({ value, onChange, min, max, disabled, compact, className,
  "aria-label": label, "aria-invalid": invalid, "aria-describedby": describedBy }: DatePickerProps) {
  const { t } = useTranslation("common");
  const [open, setOpen] = useState(false);
  const buttonRef = useRef<HTMLButtonElement>(null);
  return <div className={cn("relative min-w-0", className)}>
    <Input type="text" inputMode="numeric" autoComplete="off" value={value} placeholder={t("picker.dateFormat")}
      aria-label={label} aria-invalid={invalid} aria-describedby={describedBy} disabled={disabled}
      onChange={(event) => onChange(event.currentTarget.value)}
      className={cn("w-full pr-10 font-mono tabular-nums aria-invalid:border-danger", compact && "h-8 px-2 pr-9 text-menu")} />
    <button ref={buttonRef} type="button" disabled={disabled} aria-label={`${label}: ${t("picker.openCalendar")}`}
      aria-haspopup="dialog" aria-expanded={open} onClick={() => setOpen(!open)}
      className={cn("absolute right-0 top-0 flex h-9 w-9 cursor-pointer items-center justify-center rounded-r-md text-secondary transition-colors hover:bg-hover hover:text-primary disabled:cursor-not-allowed disabled:opacity-40", compact && "h-8 w-8")}>
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
        <rect x="2" y="3" width="12" height="11" rx="2"/><path d="M5 1.5v3M11 1.5v3M2 6.5h12" />
      </svg>
    </button>
    <Popover open={open} onClose={() => setOpen(false)} anchorRef={buttonRef} role="dialog" label={`${label}: ${t("picker.chooseDate")}`}
      initialFocus={/^\d{4}-\d{2}-\d{2}$/.test(value) ? `[data-picker-date="${value}"]:not([disabled])` : undefined}
      className="w-[292px] p-0">
      <DateCalendar value={value} onChange={(date) => { onChange(date); setOpen(false); }} min={min} max={max} />
    </Popover>
  </div>;
}

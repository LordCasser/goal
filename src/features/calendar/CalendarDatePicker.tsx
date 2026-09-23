import { DateCalendar } from "../../ui/DateCalendar";

/** The calendar's move-day dialog uses the same date grid as form fields. */
export function CalendarDatePicker({ value, onChange, weekStartDay = 1, disabled = false }: {
  value: string;
  onChange: (date: string) => void;
  weekStartDay?: number;
  disabled?: boolean;
}) {
  return <DateCalendar value={value} onChange={onChange} weekStartDay={weekStartDay} disabled={disabled}
    className="mx-auto mt-3 border border-light" />;
}

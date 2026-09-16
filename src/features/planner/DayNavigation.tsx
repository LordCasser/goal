import type { Cycle } from "../../lib/ipc";
import { formatDate, useTranslation } from "../../lib/i18n";
import { addDaysISO, parseISODay } from "./dates";
import { DateDrum } from "./DateDrum";

export function weekContainingDate(weeks: Cycle[], date: string, preferredId?: string): Cycle | undefined {
  const contains = (week: Cycle) => !!week.starts_on && !!week.ends_on && week.starts_on <= date && date < week.ends_on;
  return weeks.find((week) => week.id === preferredId && contains(week))
    ?? [...weeks].reverse().find(contains);
}

export function DayNavigation({ selectedDate, today, active, onSelect, onIntent, canCommit }: {
  selectedDate: string;
  today: string;
  active: boolean;
  onSelect: (date: string) => void;
  onIntent?: () => void;
  canCommit?: () => boolean;
}) {
  const { t } = useTranslation("planning");
  const current = parseISODay(today)!;
  return <DateDrum label={t("workspace.days")} value={parseISODay(selectedDate) ?? current} current={current}
    active={active} onIntent={onIntent} canCommit={canCommit}
    onSelect={(value) => onSelect(addDaysISO("1970-01-01", value))}
    item={(value) => {
      const date = addDaysISO("1970-01-01", value);
      return {
        label: `${formatDate(date, { weekday: "short" })}${value === current ? ` · ${t("workspace.today")}` : ""}`,
        detail: formatDate(date, { month: "short", day: "numeric" }),
        title: formatDate(date, { year: "numeric", month: "long", day: "numeric" }),
      };
    }} />;
}

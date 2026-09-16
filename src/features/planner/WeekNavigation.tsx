import { useMemo } from "react";
import type { Cycle } from "../../lib/ipc";
import { formatDate, useTranslation } from "../../lib/i18n";
import { addDaysISO, isoWeekNumber, parseISODay } from "./dates";
import { DateDrum } from "./DateDrum";

export function weekStartForDate(date: string, weekStartDay: number): string {
  const epoch = parseISODay(date);
  if (epoch === null) return date;
  const weekday = ((epoch + 3) % 7 + 7) % 7 + 1;
  return addDaysISO(date, -((weekday - weekStartDay + 7) % 7));
}

/** Saved starts remain distinct even after changing the week-start preference.
 * Sparse exceptions replace calendar slots; two saved starts in the same slot
 * each retain a row. The unsaved tails remain unbounded and cost no storage.
 */
export function weekTimeline(cycles: Cycle[], weekStartDay: number, anchors: string[] = []) {
  const epoch = weekStartForDate("1970-01-05", weekStartDay);
  const baseIndex = (date: string) => Math.floor(((parseISODay(date) ?? 0) - parseISODay(epoch)!) / 7);
  const grouped = new Map<number, Set<string>>();
  for (const date of [...cycles.map((cycle) => cycle.starts_on), ...anchors]) {
    if (!date) continue;
    const key = baseIndex(date);
    if (!grouped.has(key)) grouped.set(key, new Set());
    grouped.get(key)!.add(date);
  }
  let extra = 0;
  const groups = [...grouped].sort(([a], [b]) => a - b).map(([base, starts]) => {
    const dates = [...starts].sort();
    const group = { base, dates, rank: base + extra, extraBefore: extra };
    extra += dates.length - 1;
    return group;
  });
  const descending = [...groups].reverse();
  return {
    index(date: string) {
      const base = baseIndex(date);
      const group = descending.find((entry) => entry.base <= base);
      if (!group) return base;
      if (group.base === base) return group.rank + Math.max(0, group.dates.indexOf(date));
      return base + group.extraBefore + group.dates.length - 1;
    },
    date(index: number) {
      const group = descending.find((entry) => entry.rank <= index);
      if (!group) return addDaysISO(epoch, index * 7);
      const offset = index - group.rank;
      return group.dates[offset] ?? addDaysISO(epoch, (index - group.extraBefore - group.dates.length + 1) * 7);
    },
  };
}

export function WeekNavigation({ cycles, selectedDate, today, weekStartDay, active, onSelect, onIntent, canCommit }: {
  cycles: Cycle[];
  selectedDate: string;
  today: string;
  weekStartDay: number;
  active: boolean;
  onSelect: (date: string) => void;
  onIntent?: () => void;
  canCommit?: () => boolean;
}) {
  const { t } = useTranslation("planning");
  const current = cycles.find((cycle) => cycle.starts_on && cycle.ends_on && cycle.starts_on <= today && today < cycle.ends_on);
  const currentDate = current?.starts_on ?? weekStartForDate(today, weekStartDay);
  const timeline = useMemo(() => weekTimeline(cycles, weekStartDay, [selectedDate, currentDate]), [cycles, weekStartDay, selectedDate, currentDate]);
  const currentIndex = timeline.index(currentDate);
  return <DateDrum label={t("workspace.weeks")} value={timeline.index(selectedDate)} current={currentIndex}
    active={active} onIntent={onIntent} canCommit={canCommit}
    onSelect={(value) => onSelect(timeline.date(value))}
    item={(value) => {
      const date = timeline.date(value);
      return {
        label: `${t("workspace.weekNumber", { count: isoWeekNumber(date) })}${value === currentIndex ? ` · ${t("workspace.thisWeek")}` : ""}`,
        detail: formatDate(date, { year: "numeric", month: "short", day: "numeric" }),
        title: formatDate(date, { year: "numeric", month: "long", day: "numeric" }),
      };
    }} />;
}

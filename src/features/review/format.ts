/**
 * Review formatting helpers shared by the panel and the trend view. Pure
 * functions so the numbers the UI shows can be tested without a backend.
 */

import { formatDate, formatDuration } from "../../lib/i18n";

/** Milliseconds → a locale-aware duration while retaining the old floor semantics. */
export function formatFocusedTime(ms: number): string {
  return formatDuration(Math.max(0, Math.floor(ms / 60_000) * 60_000));
}

/** Rate in [0,1] → `60%`; rates round half up, never show decimal noise. */
export function formatPercent(rate: number): string {
  return `${Math.round(rate * 100)}%`;
}

/** Snapshot milliseconds → locale-aware local date and time. */
export function formatSnapshotAt(ms: number): string {
  return formatDate(ms, { dateStyle: "medium", timeStyle: "short" });
}

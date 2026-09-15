/**
 * Review formatting helpers shared by the panel and the trend view. Pure
 * functions so the numbers the UI shows can be tested without a backend.
 */

/** Milliseconds → compact `1h 23m` / `45m` / `0m`. */
export function formatFocusedTime(ms: number): string {
  const minutes = Math.floor(ms / 60_000);
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (hours === 0) return `${rest}m`;
  if (rest === 0) return `${hours}h`;
  return `${hours}h ${rest}m`;
}

/** Rate in [0,1] → `60%`; rates round half up, never show decimal noise. */
export function formatPercent(rate: number): string {
  return `${Math.round(rate * 100)}%`;
}

/** Snapshot milliseconds → local `YYYY-MM-DD HH:mm` (快照采集时间标注). */
export function formatSnapshotAt(ms: number): string {
  const date = new Date(ms);
  const pad = (value: number) => String(value).padStart(2, "0");
  return (
    `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ` +
    `${pad(date.getHours())}:${pad(date.getMinutes())}`
  );
}

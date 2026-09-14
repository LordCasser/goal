/**
 * Theme runtime for the two light themes (design.md 4.4).
 *
 * Components style themselves with CSS custom properties from index.css, so
 * switching themes only flips `data-theme` on <html> — no React context, no
 * re-render. Call `initTheme()` once as early as possible (before first paint)
 * to restore the stored preference.
 */

export type Theme = "white" | "gray";

const STORAGE_KEY = "planner.theme";

export function isTheme(value: unknown): value is Theme {
  return value === "white" || value === "gray";
}

export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  try {
    localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    // Storage can be unavailable (private mode); switching still works this run.
  }
}

/** Restores the stored preference (or the white default) and applies it. */
export function initTheme(): Theme {
  let stored: string | null = null;
  try {
    stored = localStorage.getItem(STORAGE_KEY);
  } catch {
    // Ignore unreadable storage and fall through to the default.
  }
  const theme: Theme = isTheme(stored) ? stored : "white";
  applyTheme(theme);
  return theme;
}

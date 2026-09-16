import { createInstance } from "i18next";
import { initReactI18next } from "react-i18next";
export { useTranslation, Trans } from "react-i18next";

export type Locale = "en" | "zh-CN";
const CACHE_KEY = "goal.locale";
export const SUPPORTED_LOCALES: readonly Locale[] = ["en", "zh-CN"];

export function isLocale(value: unknown): value is Locale {
  return value === "en" || value === "zh-CN";
}

export function matchLocale(value: string | null | undefined): Locale {
  return /^zh(?:[-_]|$)/i.test(value ?? "") ? "zh-CN" : "en";
}

export function initialLocale(): Locale {
  try {
    const cached = localStorage.getItem(CACHE_KEY);
    if (isLocale(cached)) return cached;
  } catch { /* The database remains the durable preference. */ }
  return matchLocale(typeof navigator === "undefined" ? "en" : navigator.language);
}

// Bundled resources only: no network backend and no language-specific UI tree.
export const resources: Record<string, Record<string, Record<string, string>>> = {};
const catalogs = import.meta.glob<Record<string, string>>("./locales/*/*.json", { eager: true, import: "default" });
for (const [path, catalog] of Object.entries(catalogs)) {
  const [, locale, namespace] = /\/locales\/([^/]+)\/([^/]+)\.json$/.exec(path)!;
  (resources[locale!] ??= {})[namespace!] = catalog;
}

export const i18n = createInstance();
void i18n.use(initReactI18next).init({
  resources,
  lng: initialLocale(),
  fallbackLng: "en",
  supportedLngs: [...SUPPORTED_LOCALES],
  load: "currentOnly",
  defaultNS: "common",
  ns: Object.keys(resources.en ?? {}),
  keySeparator: false,
  initAsync: false,
  interpolation: { escapeValue: false }, // React escapes text nodes.
  react: { useSuspense: false },
});

export const t = i18n.t.bind(i18n);
export function getLocale(): Locale { return isLocale(i18n.language) ? i18n.language : "en"; }

export function applyLocale(locale: Locale): void {
  if (!isLocale(locale)) return;
  void i18n.changeLanguage(locale);
  if (typeof document !== "undefined") document.documentElement.lang = locale;
  try { localStorage.setItem(CACHE_KEY, locale); } catch { /* Cache is optional. */ }
}

if (typeof document !== "undefined") document.documentElement.lang = getLocale();

export function formatDate(value: Date | number | string, options?: Intl.DateTimeFormatOptions): string {
  // Date-only identifiers are local dates. Never parse them as UTC midnight.
  const date = typeof value === "string" && /^\d{4}-\d{2}-\d{2}$/.test(value)
    ? new Date(Number(value.slice(0, 4)), Number(value.slice(5, 7)) - 1, Number(value.slice(8, 10)))
    : new Date(value);
  if (!Number.isFinite(date.getTime())) return String(value);
  return new Intl.DateTimeFormat(getLocale(), options).format(date);
}

export function formatNumber(value: number): string {
  return new Intl.NumberFormat(getLocale()).format(value);
}

export interface LocalizedMessage { key: string; args: Record<string, unknown> }
export function formatMessage(message: LocalizedMessage | string): string {
  // Plain strings are historical/user content. Never infer keys from their text.
  if (typeof message === "string") return message;
  const resolve = (value: unknown): unknown => {
    if (value && typeof value === "object" && "key" in value && "args" in value && typeof value.key === "string") {
      return formatMessage(value as LocalizedMessage);
    }
    if (Array.isArray(value)) return value.map(resolve).join(getLocale() === "zh-CN" ? "、" : ", ");
    return value;
  };
  const args = Object.fromEntries(Object.entries(message.args).map(([key, value]) => [key, resolve(value)]));
  return t(message.key.includes(":") ? message.key : `backend:${message.key}`, args);
}

export function formatDuration(milliseconds: number): string {
  const minutes = Math.max(0, Math.floor(milliseconds / 60_000));
  const hours = Math.floor(minutes / 60);
  if (!hours) return t("duration.minutes", { count: minutes });
  return minutes % 60
    ? t("duration.hoursMinutes", { hours: formatNumber(hours), minutes: formatNumber(minutes % 60) })
    : t("duration.hours", { count: hours });
}

/** Localize stable error codes at render time; retain unknown diagnostics. */
export function errorMessage(error: unknown): string {
  const object = typeof error === "object" && error !== null ? error as { code?: unknown; message?: unknown } : null;
  if (typeof object?.code === "string" && i18n.exists(object.code, { ns: "errors" })) {
    return t(object.code, { ns: "errors" });
  }
  const detail = typeof object?.message === "string" ? object.message : String(error ?? "");
  return detail ? t("error.detail", { detail }) : t("error.generic");
}

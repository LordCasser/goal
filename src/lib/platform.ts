/**
 * The one front-end boundary for target platform behavior.
 *
 * `__DESKTOP_PLATFORM__` is replaced by the desktop build before the first
 * render. A plain Vite build has no replacement and intentionally resolves to
 * `web`; browser-only code may use the host keyboard convention, while it
 * still remains a Web shell.
 */

declare const __DESKTOP_PLATFORM__: unknown;

export type DesktopPlatform = "macos" | "windows" | "linux" | "web";

/** A small structural event type so this helper accepts DOM and React events. */
export type PrimaryShortcutEvent = {
  key: string;
  metaKey?: boolean;
  ctrlKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
  repeat?: boolean;
  isComposing?: boolean;
  defaultPrevented?: boolean;
  nativeEvent?: {
    repeat?: boolean;
    isComposing?: boolean;
  };
};

/**
 * Normalize the value supplied by the build chain. `darwin` is accepted here
 * as a defensive boundary because Tauri names the Apple target that way.
 * Unsupported values deliberately become the standalone Web shell; a native
 * build must be rejected by the build hook before it can reach this fallback.
 */
export function resolveDesktopPlatform(value: unknown): DesktopPlatform {
  switch (value) {
    case "darwin":
    case "macos":
      return "macos";
    case "windows":
    case "win32":
      return "windows";
    case "linux":
      return "linux";
    case "web":
      return "web";
    default:
      return "web";
  }
}

function buildPlatformValue(): unknown {
  // `typeof` keeps an ordinary `npm run dev`/Vitest environment valid when
  // Vite has not supplied the compile-time replacement.
  return typeof __DESKTOP_PLATFORM__ === "undefined" ? undefined : __DESKTOP_PLATFORM__;
}

/** Target platform selected at build time; never derived from user settings. */
export const platform: DesktopPlatform = resolveDesktopPlatform(buildPlatformValue());

type ShortcutConvention = "command" | "control";

function browserHostPlatform(): string {
  if (typeof navigator === "undefined") return "";
  const browserNavigator = navigator as Navigator & {
    userAgentData?: { platform?: string };
  };
  return browserNavigator.userAgentData?.platform
    || browserNavigator.platform
    || browserNavigator.userAgent
    || "";
}

function shortcutConventionFor(
  targetPlatform: DesktopPlatform,
  hostPlatform?: string,
): ShortcutConvention {
  if (targetPlatform === "macos") return "command";
  if (targetPlatform !== "web") return "control";
  const webHostPlatform = hostPlatform ?? browserHostPlatform();
  if (/mac|iphone|ipad|ipod/i.test(webHostPlatform)) return "command";
  return "control";
}

/**
 * Pure form used by tests and by any caller that needs an explicit target.
 * The optional host value matters only for a Web preview.
 */
export function primaryShortcutFor(
  targetPlatform: DesktopPlatform,
  key: string,
  shift = false,
  hostPlatform?: string,
): string {
  const convention = shortcutConventionFor(targetPlatform, hostPlatform);
  if (convention === "command") return `⌘${shift ? "⇧" : ""}${key}`;
  return `Ctrl${shift ? "+Shift" : ""}+${key}`;
}

/** Render the primary shortcut for the compiled target platform. */
export function primaryShortcut(key: string, shift = false): string {
  return primaryShortcutFor(platform, key, shift);
}

/**
 * Pure matching form. A shortcut is accepted only with the target's primary
 * modifier, its exact Shift state, and no Alt, IME composition, or repeat.
 * This is intentionally stricter than `(metaKey || ctrlKey)` so a shortcut
 * cannot fire twice when the wrong platform modifier is also held.
 */
export function matchesPrimaryShortcutFor(
  targetPlatform: DesktopPlatform,
  event: PrimaryShortcutEvent,
  key: string,
  shift = false,
  hostPlatform?: string,
): boolean {
  if (event.key.toLowerCase() !== key.toLowerCase() || event.defaultPrevented === true) return false;
  if (event.altKey === true || (event.shiftKey === true) !== shift) return false;
  if (event.repeat === true || event.nativeEvent?.repeat === true) return false;
  if (event.isComposing === true || event.nativeEvent?.isComposing === true) return false;

  const convention = shortcutConventionFor(targetPlatform, hostPlatform);
  if (convention === "command") {
    return event.metaKey === true && event.ctrlKey !== true;
  }
  return event.ctrlKey === true && event.metaKey !== true;
}

/** Match a shortcut against the compiled target platform. */
export function matchesPrimaryShortcut(
  event: PrimaryShortcutEvent,
  key: string,
  shift = false,
): boolean {
  return matchesPrimaryShortcutFor(platform, event, key, shift);
}

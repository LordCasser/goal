import { describe, expect, it } from "vitest";

import {
  matchesPrimaryShortcutFor,
  primaryShortcutFor,
  resolveDesktopPlatform,
  type PrimaryShortcutEvent,
} from "./platform";

function event(overrides: Partial<PrimaryShortcutEvent> = {}): PrimaryShortcutEvent {
  return { key: "1", ...overrides };
}

describe("desktop platform boundary", () => {
  it("normalizes the build target and keeps unsupported values in Web preview", () => {
    expect(resolveDesktopPlatform("darwin")).toBe("macos");
    expect(resolveDesktopPlatform("macos")).toBe("macos");
    expect(resolveDesktopPlatform("windows")).toBe("windows");
    expect(resolveDesktopPlatform("linux")).toBe("linux");
    expect(resolveDesktopPlatform("web")).toBe("web");
    expect(resolveDesktopPlatform("android")).toBe("web");
    expect(resolveDesktopPlatform(undefined)).toBe("web");
  });

  it("renders the platform convention for each native target", () => {
    expect(primaryShortcutFor("macos", "1")).toBe("⌘1");
    expect(primaryShortcutFor("macos", "L", true)).toBe("⌘⇧L");
    expect(primaryShortcutFor("windows", "1")).toBe("Ctrl+1");
    expect(primaryShortcutFor("linux", "L", true)).toBe("Ctrl+Shift+L");
  });

  it("uses the browser host convention only inside the Web shell", () => {
    expect(primaryShortcutFor("web", "1", false, "MacIntel")).toBe("⌘1");
    expect(primaryShortcutFor("web", "1", false, "Win32")).toBe("Ctrl+1");
    expect(primaryShortcutFor("windows", "1", false, "MacIntel")).toBe("Ctrl+1");
    expect(primaryShortcutFor("macos", "1", false, "Win32")).toBe("⌘1");
  });

  it("accepts the exact primary modifier and shift state", () => {
    expect(matchesPrimaryShortcutFor("macos", event({ metaKey: true }), "1")).toBe(true);
    expect(matchesPrimaryShortcutFor("windows", event({ ctrlKey: true }), "1")).toBe(true);
    expect(matchesPrimaryShortcutFor("macos", event({ key: "L", metaKey: true, shiftKey: true }), "l", true)).toBe(true);
    expect(matchesPrimaryShortcutFor("macos", event({ key: "l", metaKey: true, shiftKey: true }), "L", true)).toBe(true);
    expect(matchesPrimaryShortcutFor("linux", event({ ctrlKey: true, shiftKey: true, key: "L" }), "L", true)).toBe(true);
    expect(matchesPrimaryShortcutFor("web", event({ metaKey: true }), "1", false, "MacIntel")).toBe(true);
    expect(matchesPrimaryShortcutFor("web", event({ ctrlKey: true }), "1", false, "Linux x86_64")).toBe(true);
  });

  it("rejects wrong Ctrl/Meta, Alt, extra Shift, IME, and repeat", () => {
    const rejected: PrimaryShortcutEvent[] = [
      event({ ctrlKey: true }),
      event({ metaKey: true, ctrlKey: true }),
      event({ metaKey: true, altKey: true }),
      event({ metaKey: true, shiftKey: true }),
      event({ metaKey: true, isComposing: true }),
      event({ metaKey: true, nativeEvent: { isComposing: true } }),
      event({ metaKey: true, repeat: true }),
      event({ metaKey: true, nativeEvent: { repeat: true } }),
    ];
    for (const candidate of rejected) {
      expect(matchesPrimaryShortcutFor("macos", candidate, "1")).toBe(false);
    }
    expect(matchesPrimaryShortcutFor("windows", event({ metaKey: true }), "1")).toBe(false);
    expect(matchesPrimaryShortcutFor("windows", event({ ctrlKey: true, shiftKey: true }), "1")).toBe(false);
    expect(matchesPrimaryShortcutFor("macos", event({ metaKey: true, defaultPrevented: true }), "1")).toBe(false);
  });
});

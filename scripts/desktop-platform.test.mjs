import { describe, expect, it } from "vitest";
import {
  normalizePlatform,
  platformFromTargetTriple,
  resolveBuildPlatform,
} from "./desktop-platform.mjs";

describe("desktop platform boundary", () => {
  it.each([
    ["darwin", "macos"],
    ["macos", "macos"],
    ["win32", "windows"],
    ["windows", "windows"],
    ["linux", "linux"],
    ["web", "web"],
  ])("normalizes %s to %s", (raw, expected) => {
    expect(normalizePlatform(raw)).toBe(expected);
  });

  it.each([
    ["aarch64-apple-darwin", "macos"],
    ["x86_64-pc-windows-msvc", "windows"],
    ["x86_64-unknown-linux-gnu", "linux"],
  ])("maps %s to %s", (triple, expected) => {
    expect(platformFromTargetTriple(triple)).toBe(expected);
  });

  it("uses web only when the native target is absent", () => {
    expect(resolveBuildPlatform({ env: {} })).toBe("web");
    expect(() => resolveBuildPlatform({ env: {}, requireNative: true })).toThrow(/target is missing/);
  });

  it("normalizes Tauri's darwin marker before injection", () => {
    expect(
      resolveBuildPlatform({
        env: { TAURI_ENV_PLATFORM: "darwin", TAURI_ENV_TARGET_TRIPLE: "universal-apple-darwin" },
      }),
    ).toBe("macos");
  });

  it("rejects conflicting target sources and unknown native targets", () => {
    expect(() =>
      resolveBuildPlatform({
        env: { TAURI_ENV_PLATFORM: "linux", TAURI_ENV_TARGET_TRIPLE: "x86_64-pc-windows-msvc" },
      }),
    ).toThrow(/target mismatch/);
    expect(() => platformFromTargetTriple("x86_64-unknown-freebsd"))
      .toThrow(/Unsupported native target triple/);
    expect(() => normalizePlatform("solaris")).toThrow(/Unsupported desktop platform/);
  });
});

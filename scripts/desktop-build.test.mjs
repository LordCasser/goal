import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { afterEach, describe, expect, it } from "vitest";
import { assertBuildManifest, writeBuildManifest } from "./desktop-build.mjs";

const temporaryDirectories = [];

function temporaryDirectory() {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "planner-desktop-build-"));
  temporaryDirectories.push(directory);
  return directory;
}

afterEach(() => {
  while (temporaryDirectories.length > 0) {
    fs.rmSync(temporaryDirectories.pop(), { recursive: true, force: true });
  }
});

describe("desktop frontend build marker", () => {
  it("requires a Tauri target for native hooks even when a manual marker is set", () => {
    const script = path.resolve("scripts/desktop-build.mjs");
    expect(() => execFileSync(process.execPath, [script, "before-bundle"], {
      cwd: path.resolve("."),
      env: { ...process.env, DESKTOP_PLATFORM: "macos", TAURI_ENV_PLATFORM: "" },
      stdio: "pipe",
    })).toThrow(/must run from a Tauri hook/);
  });

  it("round-trips the target and product version", () => {
    const outputDir = temporaryDirectory();
    expect(writeBuildManifest(outputDir, "macos", "0.1.0")).toEqual({
      platform: "macos",
      version: "0.1.0",
    });
    expect(assertBuildManifest({ outputDir, platform: "macos", version: "0.1.0" })).toEqual({
      platform: "macos",
      version: "0.1.0",
    });
  });

  it.each([
    ["wrong platform", { platform: "windows", version: "0.1.0" }, /target mismatch/],
    ["wrong version", { platform: "macos", version: "9.9.9" }, /version mismatch/],
  ])("rejects a %s marker", (_label, marker, error) => {
    const outputDir = temporaryDirectory();
    fs.mkdirSync(outputDir, { recursive: true });
    fs.writeFileSync(path.join(outputDir, "desktop-build.json"), `${JSON.stringify(marker)}\n`);
    expect(() => assertBuildManifest({ outputDir, platform: "macos", version: "0.1.0" })).toThrow(error);
  });

  it("rejects a missing or augmented marker instead of treating it as Web", () => {
    const outputDir = temporaryDirectory();
    expect(() => assertBuildManifest({ outputDir, platform: "macos" })).toThrow(/Missing desktop-build.json/);
    fs.writeFileSync(
      path.join(outputDir, "desktop-build.json"),
      JSON.stringify({ platform: "macos", version: "0.1.0", target: "stale" }),
    );
    expect(() => assertBuildManifest({ outputDir, platform: "macos" })).toThrow(/only platform and version/);
  });
});

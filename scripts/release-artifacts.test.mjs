import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  RELEASE_TARGETS,
  assertProductVersion,
  stageTarget,
  verifyReleaseDirectory,
  writeChecksums,
} from "./release-artifacts.mjs";

const temporaryDirectories = [];

function temporaryDirectory() {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "planner-release-artifacts-"));
  temporaryDirectories.push(directory);
  return directory;
}

function writeFixture(rootDir) {
  fs.mkdirSync(path.join(rootDir, "src-tauri"), { recursive: true });
  fs.writeFileSync(path.join(rootDir, "package.json"), JSON.stringify({ version: "0.1.0" }));
  fs.writeFileSync(path.join(rootDir, "src-tauri", "tauri.conf.json"), JSON.stringify({ version: "0.1.0" }));
  fs.writeFileSync(path.join(rootDir, "src-tauri", "Cargo.toml"), '[package]\nname = "planner"\nversion = "0.1.0"\n');
}

function writeBundles(rootDir, targetId) {
  const target = RELEASE_TARGETS[targetId];
  for (const bundle of target.bundles) {
    const directory = path.join(rootDir, "src-tauri", "target", target.triple, "release", "bundle", bundle.directory);
    fs.mkdirSync(directory, { recursive: true });
    fs.writeFileSync(path.join(directory, `Planner-0.1.0${bundle.extension}`), `${targetId}:${bundle.directory}`);
  }
}

afterEach(() => {
  while (temporaryDirectories.length > 0) {
    fs.rmSync(temporaryDirectories.pop(), { recursive: true, force: true });
  }
});

describe("release artifact contract", () => {
  it("requires matching package, Cargo, Tauri, and tag versions", () => {
    const rootDir = temporaryDirectory();
    writeFixture(rootDir);
    expect(assertProductVersion({ rootDir, tag: "v0.1.0" })).toBe("0.1.0");
    expect(() => assertProductVersion({ rootDir, tag: "v0.2.0" })).toThrow(/must equal v0.1.0/);
  });

  it("stages each native bundle with the stable six-target naming scheme", () => {
    const rootDir = temporaryDirectory();
    const outputDir = path.join(rootDir, "release-artifacts");
    writeFixture(rootDir);
    for (const targetId of Object.keys(RELEASE_TARGETS)) {
      writeBundles(rootDir, targetId);
      stageTarget({ rootDir, targetId, outputDir });
    }
    const files = verifyReleaseDirectory({ rootDir, outputDir });
    expect(files.map((filePath) => path.basename(filePath))).toEqual([
      "Goal-v0.1.0-linux-amd64.AppImage",
      "Goal-v0.1.0-linux-amd64.deb",
      "Goal-v0.1.0-linux-arm64.AppImage",
      "Goal-v0.1.0-linux-arm64.deb",
      "Goal-v0.1.0-macos-amd64.dmg",
      "Goal-v0.1.0-macos-arm64.dmg",
      "Goal-v0.1.0-windows-amd64-setup.exe",
      "Goal-v0.1.0-windows-arm64-setup.exe",
    ]);
    const checksumPath = writeChecksums({ outputDir, files });
    expect(fs.readFileSync(checksumPath, "utf8").trim().split("\n")).toHaveLength(8);
  });

  it("rejects a release directory missing any target format", () => {
    const rootDir = temporaryDirectory();
    const outputDir = path.join(rootDir, "release-artifacts");
    writeFixture(rootDir);
    writeBundles(rootDir, "linux-amd64");
    stageTarget({ rootDir, targetId: "linux-amd64", outputDir });
    expect(() => verifyReleaseDirectory({ rootDir, outputDir })).toThrow(/six-target set/);
  });
});

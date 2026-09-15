import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import {
  RELEASE_TARGETS,
  assertProductVersion,
  stageTarget,
  verifyBinaryArchitecture,
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
    fs.writeFileSync(path.join(directory, `Goal-0.1.0${bundle.extension}`), `${targetId}:${bundle.directory}`);
  }
}

function writeNativeFixture(filePath, bytes) {
  fs.writeFileSync(filePath, bytes);
  return filePath;
}

function elfHeader(machine) {
  const header = Buffer.alloc(64);
  header.set([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1], 0);
  header.writeUInt16LE(machine, 18);
  return header;
}

function peHeader(machine) {
  const header = Buffer.alloc(0x120);
  header.write("MZ", 0, "ascii");
  header.writeUInt32LE(0x80, 0x3c);
  header.set([0x50, 0x45, 0, 0], 0x80);
  header.writeUInt16LE(machine, 0x84);
  header.writeUInt16LE(0x70, 0x94);
  header.writeUInt16LE(0x20b, 0x98);
  return header;
}

function machOHeader(cpuType) {
  const header = Buffer.alloc(32);
  header.writeUInt32LE(0xfeedfacf, 0);
  header.writeUInt32LE(cpuType, 4);
  return header;
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

  it("checks ELF, PE, and Mach-O architecture headers and rejects invalid targets", () => {
    const rootDir = temporaryDirectory();
    const elf = writeNativeFixture(path.join(rootDir, "planner-linux"), elfHeader(62));
    const pe = writeNativeFixture(path.join(rootDir, "planner-windows.exe"), peHeader(0xaa64));
    const machO = writeNativeFixture(path.join(rootDir, "planner-macos"), machOHeader(0x01000007));

    expect(verifyBinaryArchitecture({ targetId: "linux-amd64", binaryPath: elf })).toBe("amd64");
    expect(verifyBinaryArchitecture({ targetId: "windows-arm64", binaryPath: pe })).toBe("arm64");
    expect(verifyBinaryArchitecture({ targetId: "macos-amd64", binaryPath: machO })).toBe("amd64");

    const truncated = writeNativeFixture(path.join(rootDir, "truncated"), Buffer.from([0x7f, 0x45, 0x4c]));
    expect(() => verifyBinaryArchitecture({ targetId: "linux-amd64", binaryPath: truncated })).toThrow(/Truncated/);
    const wrongArchitecture = writeNativeFixture(path.join(rootDir, "wrong-architecture"), elfHeader(183));
    expect(() => verifyBinaryArchitecture({ targetId: "linux-amd64", binaryPath: wrongArchitecture })).toThrow(/expected amd64 binary, found arm64/);
    const fat = writeNativeFixture(path.join(rootDir, "fat-macos"), Buffer.from([0xca, 0xfe, 0xba, 0xbe]));
    expect(() => verifyBinaryArchitecture({ targetId: "macos-amd64", binaryPath: fat })).toThrow(/Universal Mach-O/);
  });
});

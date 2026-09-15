import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it, vi } from "vitest";

const { spawnSyncMock } = vi.hoisted(() => ({ spawnSyncMock: vi.fn() }));
vi.mock("node:fs", async (importOriginal) => {
  const actual = await importOriginal();
  return { ...actual, default: actual };
});
vi.mock("node:child_process", async (importOriginal) => {
  const actual = await importOriginal();
  return {
    ...actual,
    spawnSync: spawnSyncMock,
    default: { ...actual.default, spawnSync: spawnSyncMock },
  };
});

const { cleanup } = await import("./macos-signing.mjs");

const stateDirectories = [];

function fakeSigningState() {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "planner-signing-test-"));
  const keychain = path.join(directory, "release.keychain-db");
  const statePath = path.join(directory, "state.json");
  fs.writeFileSync(keychain, "synthetic keychain");
  fs.writeFileSync(
    statePath,
    JSON.stringify({
      directory,
      keychain,
      originalKeychains: ["/tmp/original-login.keychain-db"],
    }),
  );
  stateDirectories.push(directory);
  process.env.GOAL_SIGNING_STATE_FILE = statePath;
  return { directory, keychain, statePath };
}

afterEach(() => {
  delete process.env.GOAL_SIGNING_STATE_FILE;
  spawnSyncMock.mockReset();
  for (const directory of stateDirectories.splice(0)) {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

describe("macOS signing cleanup", () => {
  it("continues after search-list restoration fails and reports the failure", () => {
    const state = fakeSigningState();
    spawnSyncMock.mockImplementation((_command, args) => {
      if (args[0] === "list-keychains") {
        return { status: 1, stdout: "", stderr: "restore search list failed" };
      }
      return { status: 0, stdout: "", stderr: "" };
    });

    expect(() => cleanup()).toThrow(/macOS signing cleanup failed:.*restore search list failed/);
    expect(spawnSyncMock).toHaveBeenCalledWith(
      "security",
      ["list-keychains", "-d", "user", "-s", "/tmp/original-login.keychain-db"],
      expect.objectContaining({ timeout: 120_000 }),
    );
    expect(spawnSyncMock).toHaveBeenCalledWith(
      "security",
      ["delete-keychain", state.keychain],
      expect.objectContaining({ timeout: 120_000 }),
    );
    expect(fs.existsSync(state.directory)).toBe(false);
  });

  it("removes the temporary directory when keychain deletion fails", () => {
    const state = fakeSigningState();
    spawnSyncMock.mockImplementation((_command, args) => {
      if (args[0] === "delete-keychain") {
        return { status: 1, stdout: "", stderr: "delete keychain failed" };
      }
      return { status: 0, stdout: "", stderr: "" };
    });

    expect(() => cleanup()).toThrow(/macOS signing cleanup failed:.*delete keychain failed/);
    expect(fs.existsSync(state.directory)).toBe(false);
  });
});

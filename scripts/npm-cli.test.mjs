import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { npmCliInvocation, resolveNpmCliPath } from "./npm-cli.mjs";

const temporaryDirectories = [];

afterEach(() => {
  while (temporaryDirectories.length > 0) {
    fs.rmSync(temporaryDirectories.pop(), { recursive: true, force: true });
  }
});

describe("npm CLI resolution", () => {
  it("invokes npm through Node and its JavaScript entry point", () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "goal-npm-cli-"));
    temporaryDirectories.push(root);
    const execPath = path.join(root, "bin", "node");
    const cli = path.join(root, "bin", "node_modules", "npm", "bin", "npm-cli.js");
    fs.mkdirSync(path.dirname(cli), { recursive: true });
    fs.writeFileSync(cli, "");

    expect(resolveNpmCliPath({ env: {}, execPath })).toBe(fs.realpathSync(cli));
    expect(npmCliInvocation(["ls"], { env: {}, execPath })).toEqual({
      command: execPath,
      args: [fs.realpathSync(cli), "ls"],
    });
  });

  it("prefers a valid npm_execpath supplied by the host", () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "goal-npm-cli-"));
    temporaryDirectories.push(root);
    const execPath = path.join(root, "node");
    const cli = path.join(root, "npm", "bin", "npm-cli.js");
    fs.mkdirSync(path.dirname(cli), { recursive: true });
    fs.writeFileSync(cli, "");

    expect(resolveNpmCliPath({ env: { npm_execpath: cli }, execPath })).toBe(fs.realpathSync(cli));
  });

  it("rejects pnpm, yarn, and npm.cmd values in npm_execpath", () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "goal-npm-cli-"));
    temporaryDirectories.push(root);
    const execPath = path.join(root, "node");
    const pnpm = path.join(root, "pnpm.cjs");
    const yarn = path.join(root, "yarn.js");
    const npmCmd = path.join(root, "npm.cmd");
    fs.writeFileSync(pnpm, "");
    fs.writeFileSync(yarn, "");
    fs.writeFileSync(npmCmd, "");

    expect(() => resolveNpmCliPath({ env: { npm_execpath: pnpm }, execPath })).toThrow(
      /npm CLI JavaScript entrypoint not found/,
    );
    expect(() => resolveNpmCliPath({ env: { npm_execpath: npmCmd }, execPath })).toThrow(
      /npm CLI JavaScript entrypoint not found/,
    );
    expect(() => resolveNpmCliPath({ env: { npm_execpath: yarn }, execPath })).toThrow(
      /npm CLI JavaScript entrypoint not found/,
    );
    expect(resolveNpmCliPath({ env: { npm_execpath: pnpm }, execPath, allowMissing: true })).toBeUndefined();

    const fallbackExecPath = path.join(root, "bin", "node");
    const fallbackCli = path.join(fallbackExecPath, "..", "node_modules", "npm", "bin", "npm-cli.js");
    fs.mkdirSync(path.dirname(fallbackCli), { recursive: true });
    fs.writeFileSync(fallbackCli, "");
    expect(resolveNpmCliPath({ env: { npm_execpath: pnpm }, execPath: fallbackExecPath })).toBe(
      fs.realpathSync(fallbackCli),
    );
  });

  it("finds Homebrew's libexec npm layout relative to node", () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "goal-npm-cli-"));
    temporaryDirectories.push(root);
    const execPath = path.join(root, "Cellar", "node", "22.12.0", "bin", "node");
    const cli = path.join(
      root,
      "Cellar",
      "node",
      "22.12.0",
      "libexec",
      "lib",
      "node_modules",
      "npm",
      "bin",
      "npm-cli.js",
    );
    fs.mkdirSync(path.dirname(cli), { recursive: true });
    fs.writeFileSync(cli, "");

    expect(resolveNpmCliPath({ env: {}, execPath })).toBe(fs.realpathSync(cli));
  });
});

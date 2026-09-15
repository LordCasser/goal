import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import packageJson from "../package.json" with { type: "json" };
import { assertPlatformConfig } from "./desktop-config.mjs";
import { normalizePlatform, resolveBuildPlatform } from "./desktop-platform.mjs";

const ROOT_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const DIST_DIR = path.join(ROOT_DIR, "dist");
const MANIFEST_NAME = "desktop-build.json";

export function createBuildManifest(platform, version = packageJson.version) {
  const resolved = normalizePlatform(platform);
  if (typeof version !== "string" || !version.trim()) {
    throw new Error("A non-empty product version is required for desktop-build.json");
  }
  return { platform: resolved, version };
}

export function writeBuildManifest(outputDir, platform, version = packageJson.version) {
  const manifest = createBuildManifest(platform, version);
  fs.mkdirSync(outputDir, { recursive: true });
  fs.writeFileSync(path.join(outputDir, MANIFEST_NAME), `${JSON.stringify(manifest)}\n`, "utf8");
  return manifest;
}

function readBuildManifest(outputDir = DIST_DIR) {
  const manifestPath = path.join(outputDir, MANIFEST_NAME);
  if (!fs.existsSync(manifestPath)) {
    throw new Error(`Missing ${MANIFEST_NAME}; rebuild the frontend for the current native target`);
  }
  let manifest;
  try {
    manifest = JSON.parse(fs.readFileSync(manifestPath, "utf8"));
  } catch (error) {
    throw new Error(`Invalid ${MANIFEST_NAME}: ${error.message}`);
  }
  if (!manifest || typeof manifest !== "object" || Array.isArray(manifest)) {
    throw new Error(`${MANIFEST_NAME} must be a JSON object`);
  }
  const keys = Object.keys(manifest).sort();
  if (keys.length !== 2 || keys[0] !== "platform" || keys[1] !== "version") {
    throw new Error(`${MANIFEST_NAME} may contain only platform and version`);
  }
  return manifest;
}

export function assertBuildManifest({ outputDir = DIST_DIR, platform, version = packageJson.version } = {}) {
  const expectedPlatform = normalizePlatform(platform, { allowWeb: false });
  const manifest = readBuildManifest(outputDir);
  if (manifest.platform !== expectedPlatform) {
    throw new Error(
      `Frontend target mismatch: ${MANIFEST_NAME} is ${JSON.stringify(manifest.platform)}, expected ${expectedPlatform}`,
    );
  }
  if (manifest.version !== version) {
    throw new Error(
      `Frontend version mismatch: ${MANIFEST_NAME} is ${JSON.stringify(manifest.version)}, expected ${version}`,
    );
  }
  return manifest;
}

function nativePlatformFromEnvironment() {
  const hasTauriTarget = [
    "TAURI_ENV_PLATFORM",
    "TAURI_ENV_TARGET_TRIPLE",
    "TAURI_ENV_TARGET",
  ].some((name) => typeof process.env[name] === "string" && process.env[name].trim());
  if (!hasTauriTarget) {
    throw new Error(
      "Native desktop target is missing; before-dev/build/bundle must run from a Tauri hook",
    );
  }
  return resolveBuildPlatform({ requireNative: true });
}

function npmCliPath() {
  const candidates = [
    process.env.npm_execpath,
    path.join(path.dirname(process.execPath), "node_modules", "npm", "bin", "npm-cli.js"),
    path.resolve(path.dirname(process.execPath), "..", "lib", "node_modules", "npm", "bin", "npm-cli.js"),
  ].filter(Boolean);
  return candidates.find((candidate) => fs.existsSync(candidate));
}

function runNpm(args, env) {
  const cli = npmCliPath();
  const command = cli ? process.execPath : process.platform === "win32" ? "npm.cmd" : "npm";
  const commandArgs = cli ? [cli, ...args] : args;
  const result = spawnSync(command, commandArgs, {
    cwd: ROOT_DIR,
    env,
    // npm.cmd is a fixed executable name with no user-controlled arguments;
    // the shell fallback is only needed on Windows when npm's CLI path is
    // not discoverable from the Node installation.
    shell: !cli && process.platform === "win32",
    stdio: "inherit",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

function runFrontendBuild(platform) {
  fs.rmSync(DIST_DIR, { recursive: true, force: true });
  runNpm(["run", "build"], { ...process.env, DESKTOP_PLATFORM: platform });
  assertBuildManifest({ platform });
}

function runFrontendDev(platform) {
  runNpm(["run", "dev"], { ...process.env, DESKTOP_PLATFORM: platform });
}

export function beforeBuild() {
  const platform = nativePlatformFromEnvironment();
  assertPlatformConfig(platform);
  runFrontendBuild(platform);
  console.log(`desktop frontend OK: ${platform} ${packageJson.version}`);
}

export function beforeDev() {
  const platform = nativePlatformFromEnvironment();
  assertPlatformConfig(platform);
  console.log(`desktop dev target OK: ${platform}`);
  runFrontendDev(platform);
}

export function beforeBundle() {
  const platform = nativePlatformFromEnvironment();
  assertPlatformConfig(platform);
  assertBuildManifest({ platform });
  console.log(`desktop bundle contract OK: ${platform} ${packageJson.version}`);
}

export function check(platform) {
  if (platform) {
    const resolved = normalizePlatform(platform);
    if (resolved === "web") return createBuildManifest(resolved);
    assertPlatformConfig(resolved);
    assertBuildManifest({ platform: resolved });
    return { platform: resolved, version: packageJson.version };
  }
  const resolved = resolveBuildPlatform();
  if (resolved === "web") return createBuildManifest(resolved);
  assertPlatformConfig(resolved);
  assertBuildManifest({ platform: resolved });
  return { platform: resolved, version: packageJson.version };
}

function usage() {
  console.error(
    "Usage: node scripts/desktop-build.mjs <before-dev|before-build|before-bundle|check [platform]>",
  );
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, platform] = process.argv.slice(2);
  try {
    if (command === "before-dev") beforeDev();
    else if (command === "before-build") beforeBuild();
    else if (command === "before-bundle") beforeBundle();
    else if (command === "check") console.log(`desktop build contract OK: ${JSON.stringify(check(platform))}`);
    else {
      usage();
      process.exitCode = 2;
    }
  } catch (error) {
    console.error(`desktop build contract failed: ${error.message}`);
    process.exitCode = 1;
  }
}

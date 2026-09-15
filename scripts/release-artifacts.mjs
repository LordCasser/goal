import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const PRODUCT_NAME = "Goal";
const CHECKSUMS_NAME = "SHA256SUMS";

/**
 * The release matrix is deliberately explicit: a release is complete only
 * when every one of these native targets has produced its expected bundle.
 */
export const RELEASE_TARGETS = Object.freeze({
  "linux-amd64": Object.freeze({
    os: "linux",
    arch: "amd64",
    triple: "x86_64-unknown-linux-gnu",
    bundles: Object.freeze([
      Object.freeze({ directory: "appimage", extension: ".AppImage" }),
      Object.freeze({ directory: "deb", extension: ".deb" }),
    ]),
  }),
  "linux-arm64": Object.freeze({
    os: "linux",
    arch: "arm64",
    triple: "aarch64-unknown-linux-gnu",
    bundles: Object.freeze([
      Object.freeze({ directory: "appimage", extension: ".AppImage" }),
      Object.freeze({ directory: "deb", extension: ".deb" }),
    ]),
  }),
  "windows-amd64": Object.freeze({
    os: "windows",
    arch: "amd64",
    triple: "x86_64-pc-windows-msvc",
    bundles: Object.freeze([Object.freeze({ directory: "nsis", extension: "-setup.exe" })]),
  }),
  "windows-arm64": Object.freeze({
    os: "windows",
    arch: "arm64",
    triple: "aarch64-pc-windows-msvc",
    bundles: Object.freeze([Object.freeze({ directory: "nsis", extension: "-setup.exe" })]),
  }),
  "macos-amd64": Object.freeze({
    os: "macos",
    arch: "amd64",
    triple: "x86_64-apple-darwin",
    bundles: Object.freeze([Object.freeze({ directory: "dmg", extension: ".dmg" })]),
  }),
  "macos-arm64": Object.freeze({
    os: "macos",
    arch: "arm64",
    triple: "aarch64-apple-darwin",
    bundles: Object.freeze([Object.freeze({ directory: "dmg", extension: ".dmg" })]),
  }),
});

function readJson(filePath) {
  return JSON.parse(fs.readFileSync(filePath, "utf8"));
}

export function productVersion(rootDir = ROOT_DIR) {
  const packageVersion = readJson(path.join(rootDir, "package.json")).version;
  const tauriVersion = readJson(path.join(rootDir, "src-tauri", "tauri.conf.json")).version;
  const cargo = fs.readFileSync(path.join(rootDir, "src-tauri", "Cargo.toml"), "utf8");
  const packageSection = cargo.match(/\[package\]([\s\S]*?)(?:\n\[|$)/)?.[1] ?? "";
  const cargoVersion = packageSection.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  const versions = { package: packageVersion, cargo: cargoVersion, tauri: tauriVersion };
  const values = Object.values(versions);
  if (values.some((value) => typeof value !== "string" || !value.trim())) {
    throw new Error(`Product version is missing or empty: ${JSON.stringify(versions)}`);
  }
  if (new Set(values).size !== 1) {
    throw new Error(`Product versions do not match: ${JSON.stringify(versions)}`);
  }
  return packageVersion;
}

export function assertProductVersion({ rootDir = ROOT_DIR, tag } = {}) {
  const version = productVersion(rootDir);
  if (tag !== undefined && tag !== null && tag !== "") {
    if (tag !== `v${version}`) {
      throw new Error(`Release tag ${JSON.stringify(tag)} must equal v${version}`);
    }
  }
  return version;
}

function targetFor(targetId) {
  const target = RELEASE_TARGETS[targetId];
  if (!target) throw new Error(`Unknown release target ${JSON.stringify(targetId)}`);
  return target;
}

function walk(directory) {
  if (!fs.existsSync(directory)) return [];
  const entries = fs.readdirSync(directory, { withFileTypes: true });
  return entries.flatMap((entry) => {
    const fullPath = path.join(directory, entry.name);
    return entry.isDirectory() ? walk(fullPath) : [fullPath];
  });
}

function bundleCandidates(sourceDir, target, bundle) {
  const targetMarker = `${path.sep}${target.triple}${path.sep}`;
  const marker = `${path.sep}bundle${path.sep}${bundle.directory}${path.sep}`;
  return walk(sourceDir).filter((filePath) => {
    const normalized = `${path.sep}${filePath}${path.sep}`;
    return normalized.includes(targetMarker) && normalized.includes(marker) && filePath.endsWith(bundle.extension);
  });
}

function releaseName(target, version, extension) {
  return `${PRODUCT_NAME}-v${version}-${target.os}-${target.arch}${extension}`;
}

export function stageTarget({
  rootDir = ROOT_DIR,
  targetId,
  sourceDir = path.join(rootDir, "src-tauri", "target"),
  outputDir = path.join(rootDir, "release-artifacts"),
  version = assertProductVersion({ rootDir }),
} = {}) {
  const target = targetFor(targetId);
  const expectedVersion = assertProductVersion({ rootDir });
  if (version !== expectedVersion) {
    throw new Error(`Stage version ${version} does not match product version ${expectedVersion}`);
  }
  fs.mkdirSync(outputDir, { recursive: true });
  const staged = [];
  for (const bundle of target.bundles) {
    const candidates = bundleCandidates(sourceDir, target, bundle);
    if (candidates.length !== 1) {
      throw new Error(
        `${targetId} expected one ${bundle.directory} ${bundle.extension} bundle, found ${candidates.length}: ${candidates.join(", ")}`,
      );
    }
    const destination = path.join(outputDir, releaseName(target, version, bundle.extension));
    fs.copyFileSync(candidates[0], destination);
    staged.push(destination);
  }
  return staged;
}

function expectedReleaseNames(version) {
  return Object.values(RELEASE_TARGETS).flatMap((target) =>
    target.bundles.map((bundle) => releaseName(target, version, bundle.extension)),
  );
}

export function verifyReleaseDirectory({ rootDir = ROOT_DIR, outputDir, version = assertProductVersion({ rootDir }) } = {}) {
  if (!outputDir) throw new Error("An output directory is required");
  const expected = expectedReleaseNames(version).sort();
  const actual = fs.existsSync(outputDir)
    ? fs.readdirSync(outputDir, { withFileTypes: true }).filter((entry) => entry.isFile() && entry.name !== CHECKSUMS_NAME).map((entry) => entry.name).sort()
    : [];
  if (actual.length !== expected.length || actual.some((name, index) => name !== expected[index])) {
    throw new Error(`Release artifacts do not match the six-target set. Expected ${JSON.stringify(expected)}, found ${JSON.stringify(actual)}`);
  }
  return actual.map((name) => path.join(outputDir, name));
}

export function writeChecksums({ outputDir, files = verifyReleaseDirectory({ outputDir }) } = {}) {
  if (!outputDir) throw new Error("An output directory is required");
  const lines = files
    .map((filePath) => `${crypto.createHash("sha256").update(fs.readFileSync(filePath)).digest("hex")}  ${path.basename(filePath)}`)
    .sort((a, b) => a.localeCompare(b));
  const checksumPath = path.join(outputDir, CHECKSUMS_NAME);
  fs.writeFileSync(checksumPath, `${lines.join("\n")}\n`, "utf8");
  return checksumPath;
}

function parseOptions(args) {
  const options = {};
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (!argument.startsWith("--")) throw new Error(`Unexpected argument ${argument}`);
    const key = argument.slice(2).replaceAll("-", "_");
    options[key] = args[index + 1];
    index += 1;
  }
  return options;
}

function usage() {
  console.error(
    "Usage: node scripts/release-artifacts.mjs <verify-version|stage|verify|checksums> [--tag TAG] [--target TARGET] [--source-dir DIR] [--output-dir DIR] [--version VERSION]",
  );
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [command, ...args] = process.argv.slice(2);
  try {
    const options = parseOptions(args);
    if (command === "verify-version") {
      console.log(assertProductVersion({ tag: options.tag }));
    } else if (command === "stage") {
      const version = assertProductVersion({ tag: options.tag });
      const files = stageTarget({
        targetId: options.target,
        sourceDir: options.source_dir,
        outputDir: options.output_dir,
        version,
      });
      console.log(files.join("\n"));
    } else if (command === "verify") {
      const version = assertProductVersion({ tag: options.tag });
      console.log(verifyReleaseDirectory({ outputDir: options.output_dir, version }).join("\n"));
    } else if (command === "checksums") {
      const version = assertProductVersion({ tag: options.tag });
      const files = verifyReleaseDirectory({ outputDir: options.output_dir, version });
      console.log(writeChecksums({ outputDir: options.output_dir, files }));
    } else {
      usage();
      process.exitCode = 2;
    }
  } catch (error) {
    console.error(`release artifact contract failed: ${error.message}`);
    process.exitCode = 1;
  }
}

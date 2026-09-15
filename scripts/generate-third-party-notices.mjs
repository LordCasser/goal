#!/usr/bin/env node

import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { RELEASE_TARGETS } from "./release-artifacts.mjs";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const output = path.resolve(process.argv[2] ?? path.join(root, "public/licenses/dependencies.txt"));
const licenseName = /^(?:LICEN[CS]E|COPYING|NOTICE|COPYRIGHT)(?:[._-].*|)$/i;
const upstreamManifestPath = path.join(root, "public/licenses/upstream/sources.json");

function readUpstreamRecords() {
  if (!fs.existsSync(upstreamManifestPath)) return [];
  const records = JSON.parse(fs.readFileSync(upstreamManifestPath, "utf8"));
  if (!Array.isArray(records)) throw new Error(`${upstreamManifestPath} must contain an array`);
  return records;
}

const upstreamRecords = readUpstreamRecords();
const upstreamByPackage = new Map(
  upstreamRecords.map((record) => [`${record.name}@${record.version}`, record]),
);

function run(command, args) {
  return execFileSync(command, args, {
    cwd: root,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    stdio: ["ignore", "pipe", "pipe"],
  });
}

function uniqueByKey(items) {
  const values = new Map();
  for (const item of items) values.set(`${item.name}@${item.version}`, item);
  return [...values.values()];
}

function localLicenseFiles(directory) {
  if (!fs.existsSync(directory)) return [];
  return fs.readdirSync(directory, { withFileTypes: true })
    .filter((entry) => entry.isFile() && licenseName.test(entry.name))
    .map((entry) => ({ name: entry.name, text: fs.readFileSync(path.join(directory, entry.name), "utf8") }));
}

function npmPackages() {
  const directories = run("npm", ["ls", "--omit=dev", "--all", "--parseable"])
    .split(/\r?\n/)
    .filter(Boolean);
  const packages = [];
  for (const directory of directories) {
    const packageFile = path.join(directory, "package.json");
    if (!fs.existsSync(packageFile)) continue;
    const metadata = JSON.parse(fs.readFileSync(packageFile, "utf8"));
    if (!metadata.name || path.resolve(directory) === root) continue;
    const expression = metadata.license ?? metadata.licenses?.map((item) => item.type ?? item).join(" OR ");
    if (!expression) throw new Error(`npm package has no license expression: ${metadata.name}@${metadata.version}`);
    packages.push({
      ecosystem: "npm",
      name: metadata.name,
      version: metadata.version,
      expression,
      files: localLicenseFiles(directory),
    });
  }
  return uniqueByKey(packages);
}

function cargoCacheRoots() {
  const cache = path.join(os.homedir(), ".cargo", "registry", "cache");
  if (!fs.existsSync(cache)) return [];
  return fs.readdirSync(cache, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => path.join(cache, entry.name));
}

function archiveLicenseFiles(crate) {
  if (!crate) return [];
  const members = run("tar", ["-tf", crate])
    .split(/\r?\n/)
    .filter((member) => member && !member.endsWith("/") && licenseName.test(path.posix.basename(member)));
  return members.map((member) => ({
    name: path.posix.basename(member),
    text: run("tar", ["-xOf", crate, member]),
  }));
}

function upstreamLicenseFiles(item) {
  const record = upstreamByPackage.get(`${item.name}@${item.version}`);
  if (!record) return [];
  return (record.files ?? []).map((file) => {
    const relativePath = file.path;
    const absolutePath = path.resolve(root, relativePath);
    if (!absolutePath.startsWith(`${root}${path.sep}`)) {
      throw new Error(`upstream license path escapes repository: ${relativePath}`);
    }
    if (!fs.existsSync(absolutePath)) {
      throw new Error(`upstream license file is missing: ${relativePath}`);
    }
    return {
      name: `upstream/${path.basename(relativePath)}`,
      text: fs.readFileSync(absolutePath, "utf8"),
    };
  });
}

function cargoPackages() {
  const metadata = JSON.parse(run("cargo", ["metadata", "--manifest-path", "src-tauri/Cargo.toml", "--locked", "--format-version", "1"]));
  const cacheRoots = cargoCacheRoots();
  const releaseTargetsByPackage = cargoReleaseTargetsByPackage();
  return metadata.packages
    .filter((item) => item.source?.startsWith("registry+"))
    .map((item) => {
      const directory = path.dirname(item.manifest_path);
      let files = localLicenseFiles(directory);
      if (!files.length) {
        const crate = cacheRoots
          .map((cacheRoot) => path.join(cacheRoot, `${item.name}-${item.version}.crate`))
          .find((candidate) => fs.existsSync(candidate));
        files = archiveLicenseFiles(crate);
      }
      files = [...files, ...upstreamLicenseFiles(item)];
      const expression = item.license ?? (item.license_file ? `license-file:${item.license_file}` : null);
      if (!expression) throw new Error(`Cargo package has no license expression: ${item.name}@${item.version}`);
      return {
        ecosystem: "cargo",
        name: item.name,
        version: item.version,
        expression,
        files,
        releaseTargets: releaseTargetsByPackage.get(`${item.name}@${item.version}`) ?? [],
      };
    });
}

function cargoTreePackage(line) {
  const normalized = line.trim().replace(/\s+\(\*\)$/, "");
  const versionMarker = normalized.lastIndexOf(" v");
  if (versionMarker < 1) return null;
  const name = normalized.slice(0, versionMarker);
  const version = normalized.slice(versionMarker + 2).split(/\s+/)[0];
  if (!name || !version) return null;
  return `${name}@${version}`;
}

function cargoReleaseTargetsByPackage() {
  const targetsByPackage = new Map();
  for (const [targetId, target] of Object.entries(RELEASE_TARGETS)) {
    const lines = run("cargo", [
      "tree",
      "--manifest-path",
      "src-tauri/Cargo.toml",
      "--locked",
      "--target",
      target.triple,
      "--edges",
      "normal,build",
      "--prefix",
      "none",
      "--format",
      "{p}",
    ]);
    for (const line of lines.split(/\r?\n/)) {
      const key = cargoTreePackage(line);
      if (!key || key.startsWith("planner@")) continue;
      const targets = targetsByPackage.get(key) ?? [];
      if (!targets.includes(targetId)) targets.push(targetId);
      targetsByPackage.set(key, targets);
    }
  }
  return targetsByPackage;
}

function marker(text) {
  return crypto.createHash("sha256").update(text).digest("hex").slice(0, 16);
}

function markdown(value) {
  return String(value).replaceAll("|", "\\|").replaceAll("\n", " ");
}

const packages = [...npmPackages(), ...cargoPackages()].sort((a, b) =>
  `${a.ecosystem}:${a.name}@${a.version}`.localeCompare(`${b.ecosystem}:${b.name}@${b.version}`),
);
const texts = new Map();
for (const item of packages) {
  item.fileMarkers = [];
  for (const file of item.files) {
    const id = marker(file.text);
    item.fileMarkers.push(`${id}:${file.name}`);
    const entry = texts.get(id) ?? { text: file.text, owners: [] };
    entry.owners.push(`${item.ecosystem}:${item.name}@${item.version} (${file.name})`);
    texts.set(id, entry);
  }
}

const releaseTargetIds = Object.keys(RELEASE_TARGETS);
const releaseCargoPackages = packages.filter((item) => item.ecosystem === "cargo" && item.releaseTargets?.length);
const lockOnlyCargoPackages = packages.filter((item) => item.ecosystem === "cargo" && !item.releaseTargets?.length);
const missing = packages.filter((item) => item.files.length === 0);
const missingRelease = missing.filter((item) => item.ecosystem === "npm" || item.releaseTargets?.length);
const missingLockOnly = missing.filter((item) => item.ecosystem === "cargo" && !item.releaseTargets?.length);
const attention = packages.filter((item) => /(?:GPL|MPL-2\.0|CDLA|BSL-1\.0)/i.test(item.expression));
function releaseScope(item) {
  if (item.ecosystem === "npm") return "npm production";
  if (item.releaseTargets?.length) return item.releaseTargets.join(", ");
  return "Cargo.lock only";
}
const lines = [
  "# Dependency notices",
  "",
  "Generated from the production npm tree and every registry package in `src-tauri/Cargo.lock`.",
  "The Cargo inventory intentionally includes target-specific, build and development packages so a target switch does not silently omit a notice.",
  "License expressions come from package metadata; license and copyright blocks below are copied from the local package source, cached crate archive, or a pinned upstream revision recorded in `public/licenses/upstream/sources.json`.",
  "",
  `Inventory: ${packages.length} packages (${packages.filter((item) => item.ecosystem === "npm").length} npm, ${packages.filter((item) => item.ecosystem === "cargo").length} Cargo).`,
  `Release Cargo graph: ${releaseCargoPackages.length} packages across ${releaseTargetIds.length} targets; ${lockOnlyCargoPackages.length} Cargo.lock packages are outside all normal/build graphs.`,
  `Distinct copied license files: ${texts.size}. Missing texts: ${missing.length} in the full lock inventory, ${missingRelease.length} in the selected release graph.`,
  "",
  "## License expressions requiring release review",
  "",
  "These declarations are copied from dependency metadata and are not a conclusion that a package is unsafe to ship. Confirm the applicable notice and source obligations before publishing.",
  "",
];
for (const item of attention) lines.push(`- \`${item.ecosystem}:${item.name}@${item.version}\` (${releaseScope(item)}): ${item.expression}`);
if (!attention.length) lines.push("- None found.");
lines.push("", "## Pinned upstream source records", "", "These supplements are copied from the repository revision shown below. The selectors record explicitly identifies its local cached MPL-2.0 source because that upstream revision has no license file.", "");
for (const record of [...upstreamRecords].sort((a, b) => `${a.name}@${a.version}`.localeCompare(`${b.name}@${b.version}`))) {
  const files = (record.files ?? []).map((file) => String(file.name) + " (" + String(file.url) + ")").join(", ");
  const note = record.note ? ` — ${record.note}` : "";
  lines.push(`- \`${record.ecosystem}:${record.name}@${record.version}\`: ${record.repository} @ \`${record.revision}\`; ${files}${note}`);
}
if (!upstreamRecords.length) lines.push("- None.");
lines.push("", "## Package inventory", "", "| ecosystem | package | version | release scope | license expression | copied license files |", "| --- | --- | --- | --- | --- | --- |");
for (const item of packages) {
  lines.push(`| ${item.ecosystem} | \`${markdown(item.name)}\` | ${item.version} | ${markdown(releaseScope(item))} | ${markdown(item.expression)} | ${item.fileMarkers.length ? item.fileMarkers.map((value) => `\`${value}\``).join("<br>") : "**MISSING**"} |`);
}
lines.push("", "## Missing local license texts", "", "The full Cargo.lock inventory may contain packages outside the six release targets. A missing text blocks publication only when the package is in the npm production tree or a six-target normal/build graph. No text is fabricated here.", "", `Release-blocking missing texts: ${missingRelease.length}.`);
if (missingRelease.length) {
  for (const item of missingRelease) lines.push(`- \`${item.ecosystem}:${item.name}@${item.version}\` (${releaseScope(item)}): ${item.expression}`);
} else {
  lines.push("- None found.");
}
lines.push("", `Non-selected Cargo.lock packages with missing texts: ${missingLockOnly.length}.`);
if (missingLockOnly.length) {
  for (const item of missingLockOnly) lines.push(`- \`${item.ecosystem}:${item.name}@${item.version}\` (Cargo.lock only; absent from all six normal/build graphs): ${item.expression}`);
} else {
  lines.push("- None found.");
}
lines.push("", "## Copied license texts", "");
for (const [id, entry] of [...texts.entries()].sort(([a], [b]) => a.localeCompare(b))) {
  lines.push(`### ${id}`, "", `Used by: ${entry.owners.map(markdown).join(", ")}`, "", "```text", entry.text.trimEnd(), "```", "");
}

fs.mkdirSync(path.dirname(output), { recursive: true });
fs.writeFileSync(output, `${lines.join("\n")}\n`, "utf8");
console.log(`Wrote ${output} (${packages.length} packages, ${texts.size} license texts, ${missing.length} total missing, ${missingRelease.length} release-blocking missing).`);
if (missingRelease.length) {
  process.exitCode = 1;
}

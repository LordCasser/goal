import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import packageJson from "../package.json" with { type: "json" };
import { normalizePlatform } from "./desktop-platform.mjs";

const ROOT_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const TAURI_DIR = path.join(ROOT_DIR, "src-tauri");
const PRODUCT_NAME = "Goal";

export const CONFIG_FILES = Object.freeze({
  base: path.join(TAURI_DIR, "tauri.conf.json"),
  macos: path.join(TAURI_DIR, "tauri.macos.conf.json"),
  windows: path.join(TAURI_DIR, "tauri.windows.conf.json"),
  linux: path.join(TAURI_DIR, "tauri.linux.conf.json"),
});

function readJson(filePath) {
  try {
    return JSON.parse(fs.readFileSync(filePath, "utf8"));
  } catch (error) {
    throw new Error(`Unable to read JSON config ${path.relative(ROOT_DIR, filePath)}: ${error.message}`);
  }
}

/** Apply the JSON Merge Patch semantics used by Tauri platform configs. */
export function mergePatch(base, patch) {
  if (patch === null || typeof patch !== "object" || Array.isArray(patch)) {
    return patch;
  }
  const result = base && typeof base === "object" && !Array.isArray(base) ? { ...base } : {};
  for (const [key, value] of Object.entries(patch)) {
    if (value === null) {
      delete result[key];
    } else {
      result[key] = mergePatch(result[key], value);
    }
  }
  return result;
}

export function loadMergedConfig(platform) {
  const resolved = normalizePlatform(platform, { allowWeb: false });
  return mergePatch(readJson(CONFIG_FILES.base), readJson(CONFIG_FILES[resolved]));
}

function assertWindowContract(config, platform) {
  if (config.productName !== PRODUCT_NAME) {
    throw new Error(`${platform} config product name must be ${PRODUCT_NAME}`);
  }
  if (config.identifier !== "dev.lordcasser.planner") {
    throw new Error(`${platform} config changed the application identifier`);
  }
  if (config.version !== packageJson.version) {
    throw new Error(`${platform} config version ${config.version} does not match package version ${packageJson.version}`);
  }
  if (!Array.isArray(config.app?.windows) || config.app.windows.length !== 1) {
    throw new Error(`${platform} config must contain the complete main window array`);
  }
  const [window] = config.app.windows;
  const expected = {
    label: "main",
    title: PRODUCT_NAME,
    width: 1280,
    height: 800,
    minWidth: 960,
    minHeight: 600,
    decorations: true,
    theme: "Light",
    dragDropEnabled: false,
  };
  for (const [key, value] of Object.entries(expected)) {
    if (window[key] !== value) {
      throw new Error(`${platform} main window ${key} must be ${JSON.stringify(value)}`);
    }
  }
}

export function assertPlatformConfig(platform) {
  const resolved = normalizePlatform(platform, { allowWeb: false });
  const config = loadMergedConfig(resolved);
  assertWindowContract(config, resolved);

  const targetFormats = config.bundle?.targets;
  if (!Array.isArray(targetFormats) || targetFormats.length === 0) {
    throw new Error(`${resolved} config must declare at least one native bundle target`);
  }
  const icons = config.bundle?.icon;
  if (!Array.isArray(icons) || icons.length === 0 || icons.some((icon) => !fs.existsSync(path.join(TAURI_DIR, icon)))) {
    throw new Error(`${resolved} config references a missing bundle icon`);
  }
  if (resolved === "macos") {
    const window = config.app.windows[0];
    if (window.titleBarStyle !== "Overlay" || window.hiddenTitle !== true || !window.trafficLightPosition) {
      throw new Error("macos config must retain Overlay, hiddenTitle and trafficLightPosition");
    }
    if (!targetFormats.includes("app") || !targetFormats.includes("dmg")) {
      throw new Error("macos config must target app and dmg");
    }
  } else if (resolved === "windows") {
    const window = config.app.windows[0];
    if (window.visible !== false || window.decorations !== true) {
      throw new Error("windows startup must be hidden with decorations enabled for safe fallback");
    }
    if (!targetFormats.includes("nsis")) {
      throw new Error("windows config must target NSIS");
    }
  } else if (resolved === "linux") {
    const window = config.app.windows[0];
    if (Object.hasOwn(window, "titleBarStyle") || Object.hasOwn(window, "hiddenTitle") || Object.hasOwn(window, "trafficLightPosition")) {
      throw new Error("linux config must not contain macOS window properties");
    }
    if (!targetFormats.includes("appimage") || !targetFormats.includes("deb")) {
      throw new Error("linux config must target AppImage and deb");
    }
  }
  return config;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const platform = process.argv[2];
  if (!platform) {
    console.error("Usage: node scripts/desktop-config.mjs <macos|windows|linux>");
    process.exitCode = 2;
  } else {
    try {
      assertPlatformConfig(platform);
      console.log(`desktop config OK: ${normalizePlatform(platform, { allowWeb: false })}`);
    } catch (error) {
      console.error(`desktop config invalid: ${error.message}`);
      process.exitCode = 1;
    }
  }
}

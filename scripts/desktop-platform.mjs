/**
 * The only platform boundary used by the desktop build and Vite config.
 *
 * Tauri exposes `darwin` for Apple targets. The UI deliberately consumes the
 * stable product vocabulary below instead of leaking the CLI's vocabulary or
 * the host browser's user agent into a native build.
 */

export const DESKTOP_PLATFORMS = Object.freeze(["macos", "windows", "linux", "web"]);
export const NATIVE_DESKTOP_PLATFORMS = Object.freeze(["macos", "windows", "linux"]);

const aliases = new Map([
  ["darwin", "macos"],
  ["mac", "macos"],
  ["macos", "macos"],
  ["osx", "macos"],
  ["win32", "windows"],
  ["windows", "windows"],
  ["win", "windows"],
  ["linux", "linux"],
  ["web", "web"],
  ["browser", "web"],
]);

function normaliseInput(value) {
  return typeof value === "string" ? value.trim().toLowerCase() : "";
}

/**
 * Normalise a Tauri platform value, platform name, or browser marker.
 *
 * Unknown values throw intentionally. Falling back to `web` for a typo would
 * allow a native bundle to contain browser-only resources.
 */
export function normalizePlatform(value, { allowWeb = true } = {}) {
  const input = normaliseInput(value);
  const platform = aliases.get(input);
  if (!platform || (!allowWeb && platform === "web")) {
    const supported = allowWeb ? DESKTOP_PLATFORMS : NATIVE_DESKTOP_PLATFORMS;
    throw new Error(
      `Unsupported desktop platform ${JSON.stringify(value)}; expected one of ${supported.join(", ")}`,
    );
  }
  return platform;
}

/** Resolve a Rust target triple into the product platform vocabulary. */
export function platformFromTargetTriple(value) {
  const triple = normaliseInput(value);
  if (!triple) {
    throw new Error("A target triple is required to resolve a native desktop platform");
  }
  if (triple.endsWith("-apple-darwin") || triple === "universal-apple-darwin") {
    return "macos";
  }
  if (triple.includes("-windows-") || triple.endsWith("-windows")) {
    return "windows";
  }
  if (triple.includes("-linux-") || triple.endsWith("-linux")) {
    return "linux";
  }
  throw new Error(`Unsupported native target triple ${JSON.stringify(value)}`);
}

function envValue(env, ...names) {
  for (const name of names) {
    const value = env?.[name];
    if (typeof value === "string" && value.trim()) return value;
  }
  return undefined;
}

/**
 * Resolve the platform for a frontend build.
 *
 * Native hooks must provide TAURI_ENV_PLATFORM or TAURI_ENV_TARGET_TRIPLE.
 * With neither value, a plain Vite build is an explicit Web preview. An
 * explicit DESKTOP_PLATFORM is useful for deterministic component/build
 * checks and remains subject to the target-triple consistency check.
 */
export function resolveBuildPlatform({ env = process.env, requireNative = false } = {}) {
  const explicit = envValue(env, "DESKTOP_PLATFORM");
  const tauriPlatform = envValue(env, "TAURI_ENV_PLATFORM");
  const targetTriple = envValue(env, "TAURI_ENV_TARGET_TRIPLE", "TAURI_ENV_TARGET", "TARGET_TRIPLE");

  const explicitPlatform = explicit ? normalizePlatform(explicit) : undefined;
  const tauriResolved = tauriPlatform ? normalizePlatform(tauriPlatform, { allowWeb: false }) : undefined;
  const tripleResolved = targetTriple ? platformFromTargetTriple(targetTriple) : undefined;

  if (tauriResolved && tripleResolved && tauriResolved !== tripleResolved) {
    throw new Error(
      `Desktop target mismatch: TAURI_ENV_PLATFORM=${tauriResolved} but target triple=${targetTriple}`,
    );
  }
  if (explicitPlatform && tauriResolved && explicitPlatform !== tauriResolved) {
    throw new Error(
      `Desktop target mismatch: DESKTOP_PLATFORM=${explicitPlatform} but TAURI_ENV_PLATFORM=${tauriPlatform}`,
    );
  }
  if (explicitPlatform && tripleResolved && explicitPlatform !== tripleResolved) {
    throw new Error(
      `Desktop target mismatch: DESKTOP_PLATFORM=${explicitPlatform} but target triple=${targetTriple}`,
    );
  }

  const resolved = explicitPlatform ?? tauriResolved ?? tripleResolved ?? "web";
  if (requireNative && resolved === "web") {
    throw new Error(
      "Native desktop target is missing; set TAURI_ENV_PLATFORM and/or TAURI_ENV_TARGET_TRIPLE",
    );
  }
  return resolved;
}


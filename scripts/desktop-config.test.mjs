import fs from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { assertPlatformConfig, CONFIG_FILES, loadMergedConfig, mergePatch } from "./desktop-config.mjs";

describe("Tauri JSON Merge Patch platform configs", () => {
  it("replaces arrays as JSON Merge Patch requires", () => {
    expect(mergePatch({ windows: [{ label: "main" }] }, { windows: [{ label: "override" }] }))
      .toEqual({ windows: [{ label: "override" }] });
    expect(mergePatch({ macOS: { minimumSystemVersion: "11.0" } }, { macOS: null }))
      .toEqual({});
  });

  it.each(["macos", "windows", "linux"])("keeps the shared main window contract for %s", (platform) => {
    const config = assertPlatformConfig(platform);
    const window = config.app.windows[0];
    expect(window).toMatchObject({
      label: "main",
      width: 1280,
      height: 800,
      minWidth: 960,
      minHeight: 600,
      decorations: true,
      theme: "Light",
      dragDropEnabled: false,
    });
    expect(config.identifier).toBe("dev.lordcasser.planner");
    expect(config.version).toBe("0.1.0");
  });

  it("keeps macOS-only window properties out of Windows and Linux", () => {
    expect(loadMergedConfig("macos").app.windows[0]).toMatchObject({
      titleBarStyle: "Overlay",
      hiddenTitle: true,
      trafficLightPosition: { x: 20, y: 26 },
    });
    for (const platform of ["windows", "linux"]) {
      const window = loadMergedConfig(platform).app.windows[0];
      expect(window).not.toHaveProperty("titleBarStyle");
      expect(window).not.toHaveProperty("hiddenTitle");
      expect(window).not.toHaveProperty("trafficLightPosition");
    }
  });

  it("uses the target-specific bundle format and existing icon", () => {
    expect(loadMergedConfig("macos").bundle.targets).toEqual(["app", "dmg"]);
    expect(loadMergedConfig("windows").bundle.targets).toEqual(["nsis"]);
    expect(loadMergedConfig("linux").bundle.targets).toEqual(["appimage", "deb"]);
    for (const file of Object.values(CONFIG_FILES)) expect(fs.existsSync(file)).toBe(true);
  });

  it("keeps native window permissions scoped to the intended platform and window", () => {
    const capability = (file) => JSON.parse(fs.readFileSync(file, "utf8"));
    const capabilitiesDir = path.join(path.dirname(CONFIG_FILES.base), "capabilities");
    const defaultCapability = capability(path.join(capabilitiesDir, "default.json"));
    const macosCapability = capability(path.join(capabilitiesDir, "macos.json"));
    const windowsCapability = capability(path.join(capabilitiesDir, "windows.json"));
    expect(defaultCapability.permissions).not.toContain("core:window:allow-start-dragging");
    expect(macosCapability).toMatchObject({ windows: ["main"], platforms: ["macOS"] });
    expect(macosCapability.permissions).toContain("core:window:allow-start-dragging");
    expect(windowsCapability).toMatchObject({ windows: ["main"], platforms: ["windows"] });
    expect(windowsCapability.permissions).not.toContain("core:window:allow-start-dragging");
    expect(windowsCapability.permissions).toEqual(expect.arrayContaining([
      "allow-desktop-shell-state",
      "allow-desktop-shell-ready",
      "allow-desktop-shell-regions",
    ]));
  });
});

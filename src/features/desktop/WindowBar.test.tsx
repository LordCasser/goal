import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

type Target = "macos" | "windows" | "linux" | "web";
type ShellState = {
  mode: "pending" | "custom" | "native";
  maximized: boolean;
  focused: boolean;
  fullscreen: boolean;
  pointer: { hovered: boolean; pressed: boolean };
  act: ReturnType<typeof vi.fn>;
};

function shellState(over: Partial<ShellState> = {}): ShellState {
  return {
    mode: "native",
    maximized: false,
    focused: true,
    fullscreen: false,
    pointer: { hovered: false, pressed: false },
    act: vi.fn().mockResolvedValue(undefined),
    ...over,
  };
}

async function loadWindowBar(target: Target, shell: ShellState) {
  vi.resetModules();
  vi.doMock("../../lib/platform", () => ({
    platform: target,
    primaryShortcut: (key: string, shift = false) => target === "macos"
      ? `⌘${shift ? "⇧" : ""}${key}`
      : `Ctrl${shift ? "+Shift" : ""}+${key}`,
  }));
  vi.doMock("./useDesktopWindow", () => ({ useDesktopWindow: () => shell }));
  vi.doMock("../proposals/ProposalsBar", () => ({ ProposalsBar: () => null }));
  return import("./WindowBar");
}

function props() {
  return {
    hasCycle: true,
    laterActive: false,
    agentActive: false,
    issuesActive: false,
    onToggleLater: vi.fn(),
    onToggleAgent: vi.fn(),
    onReviewChanges: vi.fn(),
    onToggleIssues: vi.fn(),
    onOpenSettings: vi.fn(),
    view: "workspace" as const,
    onSwitchView: vi.fn(),
  };
}

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => {
  cleanup();
});

describe("WindowBar platform shell", () => {
  it.each(["macos", "windows", "linux", "web"] as const)(
    "keeps the application groups and platform boundary for %s",
    async (target) => {
      const shell = shellState({ mode: target === "windows" ? "custom" : "native" });
      const { WindowBar } = await loadWindowBar(target, shell);
      render(<WindowBar {...props()} />);

      const header = document.querySelector("header.window-bar") as HTMLElement;
      expect(header.dataset.platform).toBe(target);
      expect(header.dataset.shell).toBe(shell.mode);
      expect(screen.getByRole("button", { name: "Later" }).getAttribute("title"))
        .toContain(target === "macos" ? "⌘⇧L" : "Ctrl+Shift+L");
      expect(screen.getByRole("tablist", { name: "View" })).toBeTruthy();
      expect(screen.getByRole("group", { name: "Planning tools" })).toBeTruthy();

      const safeArea = header.querySelector(".window-safe-area") as HTMLElement;
      const dragArea = header.querySelector(".window-drag-area") as HTMLElement;
      expect(safeArea.hasAttribute("data-tauri-drag-region")).toBe(target === "macos");
      expect(dragArea.hasAttribute("data-tauri-drag-region")).toBe(target === "macos");
      const controls = screen.queryByRole("group", { name: "Window controls" });
      if (target === "windows") expect(controls).toBeTruthy();
      else expect(controls).toBeNull();
    },
  );

  it("keeps Windows controls hidden when native decoration is the fallback", async () => {
    const shell = shellState({ mode: "native" });
    const { WindowBar } = await loadWindowBar("windows", shell);
    render(<WindowBar {...props()} />);

    const header = document.querySelector("header.window-bar") as HTMLElement;
    expect(header.dataset.shell).toBe("native");
    expect(screen.queryByRole("group", { name: "Window controls" })).toBeNull();
  });

  it("uses actual Windows state for labels and dispatches each action once", async () => {
    const shell = shellState({ mode: "custom", pointer: { hovered: true, pressed: true } });
    const { WindowBar } = await loadWindowBar("windows", shell);
    const view = props();
    const { rerender } = render(<WindowBar {...view} />);

    const maximize = screen.getByRole("button", { name: "Maximize window" });
    expect(maximize.getAttribute("data-hovered")).toBe("true");
    expect(maximize.getAttribute("data-pressed")).toBe("true");
    fireEvent.click(screen.getByRole("button", { name: "Minimize window" }));
    fireEvent.click(maximize);
    fireEvent.click(screen.getByRole("button", { name: "Close window" }));
    expect(shell.act).toHaveBeenNthCalledWith(1, "minimize");
    expect(shell.act).toHaveBeenNthCalledWith(2, "toggleMaximize");
    expect(shell.act).toHaveBeenNthCalledWith(3, "close");

    shell.maximized = true;
    rerender(<WindowBar {...view} />);
    expect(screen.getByRole("button", { name: "Restore window" })).toBeTruthy();
  });

  it("exposes macOS fullscreen state for the safe-area CSS contract", async () => {
    const shell = shellState({ fullscreen: false });
    const { WindowBar } = await loadWindowBar("macos", shell);
    const view = props();
    const { rerender } = render(<WindowBar {...view} />);
    const header = document.querySelector("header.window-bar") as HTMLElement;
    const safeArea = header.querySelector(".window-safe-area") as HTMLElement;

    expect(header.dataset.fullscreen).toBe("false");
    expect(safeArea.hasAttribute("data-tauri-drag-region")).toBe(true);
    shell.fullscreen = true;
    rerender(<WindowBar {...view} />);
    expect(header.dataset.fullscreen).toBe("true");
  });
});

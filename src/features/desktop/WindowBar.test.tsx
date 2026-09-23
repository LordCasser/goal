import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { applyLocale } from "../../lib/i18n";
import { qk } from "../../lib/events";
import { LATER_CYCLE_ID, type EditorWorkspace, type TaskNode } from "../../lib/ipc";

type Target = "macos" | "windows" | "linux" | "web";
type ShellState = {
  mode: "pending" | "custom" | "native";
  maximized: boolean;
  focused: boolean;
  fullscreen: boolean;
  pointer: { hovered: boolean; pressed: boolean };
  act: ReturnType<typeof vi.fn>;
};

const ipcMocks = vi.hoisted(() => ({
  getEditorWorkspace: vi.fn(),
  getSettings: vi.fn(),
}));

function taskNode(
  title: string,
  { completed = false, proposal = null, children = [] }: {
    completed?: boolean;
    proposal?: TaskNode["proposal"];
    children?: TaskNode[];
  } = {},
): TaskNode {
  return {
    id: title,
    cycle_id: LATER_CYCLE_ID,
    later_plan_type: null,
    parent_id: null,
    title,
    note: "",
    subtasks: [],
    position: 0,
    completed,
    goal_breakdown: null,
    needs_refinement: null,
    needs_breakdown: null,
    root_color_key: null,
    copied_from_task_id: null,
    proposal,
    created_at: 0,
    children,
    subtasks_markdown: "",
    focused_time: 0,
  };
}

function laterWorkspace(tasks: TaskNode[]): EditorWorkspace {
  return { cycle: null, tasks, work_mix: null };
}

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
  vi.doMock("../../lib/ipc", async (importOriginal) => ({
    ...(await importOriginal<typeof import("../../lib/ipc")>()),
    getEditorWorkspace: ipcMocks.getEditorWorkspace,
    getSettings: ipcMocks.getSettings,
  }));
  const [{ WindowBar }, query] = await Promise.all([
    import("./WindowBar"),
    import("@tanstack/react-query"),
  ]);
  return { WindowBar, ...query };
}

function renderWindowBar(
  ui: ReactNode,
  { QueryClient, QueryClientProvider }: typeof import("@tanstack/react-query"),
) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  }
  const view = render(ui, { wrapper: Wrapper });
  return { ...view, client };
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
  applyLocale("en");
  vi.clearAllMocks();
  ipcMocks.getEditorWorkspace.mockResolvedValue(laterWorkspace([]));
  ipcMocks.getSettings.mockResolvedValue({
    locale: "en",
    week_start_day: 1,
    theme: "white",
    show_relation_lines: false,
    auto_carry_unfinished: false,
    show_later_count: true,
  });
});

afterEach(() => {
  cleanup();
});

describe("WindowBar platform shell", () => {
  it.each(["macos", "windows", "linux", "web"] as const)(
    "keeps the application groups and platform boundary for %s",
    async (target) => {
      const shell = shellState({ mode: target === "windows" ? "custom" : "native" });
      const query = await loadWindowBar(target, shell);
      const { WindowBar } = query;
      renderWindowBar(<WindowBar {...props()} />, query);

      const header = document.querySelector("header.window-bar") as HTMLElement;
      expect(header.dataset.platform).toBe(target);
      expect(header.dataset.shell).toBe(shell.mode);
      expect((await screen.findByRole("button", { name: "Do Later, 0 items" })).getAttribute("title"))
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
    const query = await loadWindowBar("windows", shell);
    const { WindowBar } = query;
    renderWindowBar(<WindowBar {...props()} />, query);

    const header = document.querySelector("header.window-bar") as HTMLElement;
    expect(header.dataset.shell).toBe("native");
    expect(screen.queryByRole("group", { name: "Window controls" })).toBeNull();
  });

  it("uses actual Windows state for labels and dispatches each action once", async () => {
    const shell = shellState({ mode: "custom", pointer: { hovered: true, pressed: true } });
    const query = await loadWindowBar("windows", shell);
    const { WindowBar } = query;
    const view = props();
    const { rerender } = renderWindowBar(<WindowBar {...view} />, query);

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
    const query = await loadWindowBar("macos", shell);
    const { WindowBar } = query;
    const view = props();
    const { rerender } = renderWindowBar(<WindowBar {...view} />, query);
    const header = document.querySelector("header.window-bar") as HTMLElement;
    const safeArea = header.querySelector(".window-safe-area") as HTMLElement;

    expect(header.dataset.fullscreen).toBe("false");
    expect(safeArea.hasAttribute("data-tauri-drag-region")).toBe(true);
    shell.fullscreen = true;
    rerender(<WindowBar {...view} />);
    expect(header.dataset.fullscreen).toBe("true");
  });

  it("counts confirmed non-empty top-level Later items, including completed ones", async () => {
    const completedRoot = taskNode("Finished", { completed: true });
    const parentWithChild = taskNode("Root", { children: [taskNode("Child")] });
    ipcMocks.getEditorWorkspace.mockResolvedValue(laterWorkspace([
      completedRoot,
      taskNode("   "),
      taskNode("Pending", { proposal: "upsert" }),
      parentWithChild,
    ]));
    const query = await loadWindowBar("web", shellState());
    const { WindowBar } = query;
    const { client } = renderWindowBar(<WindowBar {...props()} />, query);

    expect(await screen.findByRole("button", { name: "Do Later, 2 items" })).toBeTruthy();
    expect(screen.getByTestId("later-count-badge").textContent).toBe("2");
    expect(client.getQueryData(qk.editorWorkspace(LATER_CYCLE_ID))).toEqual(
      laterWorkspace([completedRoot, taskNode("   "), taskNode("Pending", { proposal: "upsert" }), parentWithChild]),
    );
  });

  it("caps the visible badge at 99+ while announcing the exact count", async () => {
    ipcMocks.getEditorWorkspace.mockResolvedValue(laterWorkspace(
      Array.from({ length: 100 }, (_, index) => taskNode(`Item ${index + 1}`)),
    ));
    const query = await loadWindowBar("web", shellState());
    const { WindowBar } = query;
    renderWindowBar(<WindowBar {...props()} />, query);

    expect(await screen.findByRole("button", { name: "Do Later, 100 items" })).toBeTruthy();
    expect(screen.getByTestId("later-count-badge").textContent).toBe("99+");
  });

  it("hides the count when disabled and keeps the Later action available", async () => {
    ipcMocks.getSettings.mockResolvedValue({
      locale: "en",
      week_start_day: 1,
      theme: "white",
      show_relation_lines: false,
      auto_carry_unfinished: false,
      show_later_count: false,
    });
    const query = await loadWindowBar("web", shellState());
    const { WindowBar } = query;
    const view = props();
    renderWindowBar(<WindowBar {...view} />, query);

    const laterButton = await screen.findByRole("button", { name: "Later" });
    expect(screen.queryByTestId("later-count-badge")).toBeNull();
    fireEvent.click(laterButton);
    expect(view.onToggleLater).toHaveBeenCalledOnce();
  });
});

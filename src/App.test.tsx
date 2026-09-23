import { useEffect, useRef, useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

const planning = vi.hoisted(() => vi.fn());
const exitPollReady = vi.hoisted(() => vi.fn(async () => ({ show: false })));
vi.mock("./lib/ipc", () => ({
  LATER_CYCLE_ID: "later",
  getPlannerState: async () => ({ cycles: ["month", "week", "day"].map((type) => ({ type, id: `${type}-plan` })) }),
  getSettings: async () => ({ theme: "gray" }),
  getEditorWorkspace: async () => ({ tasks: [] }),
  startPlanning: planning,
}));
vi.mock("./lib/events", () => ({
  qk: { plannerState: () => ["planner"], editorWorkspace: (id: string) => ["editor-workspace", id], settings: () => ["settings"], agentConversation: () => ["conversation"], agentDecision: () => ["agent-decision"] },
  initEventInvalidation: async () => () => {},
  invalidateAgentEffects: () => {},
  completeAgentTurn: () => {},
}));
// App tests pin the desktop target explicitly; the host running Vitest must
// not decide whether Later uses Command or Control.
vi.mock("./lib/platform", () => ({
  platform: "macos",
  primaryShortcut: (key: string, shift = false) => `⌘${shift ? "⇧" : ""}${key}`,
  matchesPrimaryShortcut: (
    event: {
      key: string;
      metaKey?: boolean;
      ctrlKey?: boolean;
      altKey?: boolean;
      shiftKey?: boolean;
      repeat?: boolean;
      isComposing?: boolean;
      defaultPrevented?: boolean;
      nativeEvent?: { repeat?: boolean; isComposing?: boolean };
    },
    key: string,
    shift = false,
  ) => event.key.toLowerCase() === key.toLowerCase() && event.metaKey === true && event.ctrlKey !== true
    && event.altKey !== true && (event.shiftKey === true) === shift && event.repeat !== true
    && event.nativeEvent?.repeat !== true && event.isComposing !== true
    && event.nativeEvent?.isComposing !== true && event.defaultPrevented !== true,
}));
// App behavior tests stay independent from Tauri's native window object. The
// native hook has its own contract tests; here only its stable shell state is
// needed for WindowBar rendering.
vi.mock("./features/desktop/useDesktopWindow", () => ({
  useDesktopWindow: () => ({
    mode: "native",
    maximized: false,
    focused: true,
    fullscreen: false,
    pointer: { hovered: false, pressed: false },
    act: vi.fn(),
  }),
}));
vi.mock("./features/onboarding/api", () => ({ markExitPollListenerReady: exitPollReady }));
vi.mock("./features/onboarding/GettingStartedGuide", () => ({
  GettingStartedGuide: () => <section aria-label="Getting started guide" />,
}));
vi.mock("./features/onboarding/FeedbackDialog", () => ({
  FeedbackDialog: ({ open }: { open: boolean }) => open ? <div role="dialog" aria-label="Send feedback" /> : null,
}));
let nextCoachInstance = 0;
vi.mock("./features/agent/AgentPanel", () => ({ AgentPanel: ({ cycleId,initialDraft,focusedTaskId }: { cycleId:string|null;initialDraft?:string;focusedTaskId?:string }) => {
  const instance = useRef(++nextCoachInstance).current;
  const [draft, setDraft] = useState(initialDraft ?? "");
  useEffect(() => { if (initialDraft !== undefined && draft === "") setDraft(initialDraft); }, [draft, initialDraft]);
  return <section aria-label="Coach context" data-instance={instance}><span>{cycleId}</span><input aria-label="Coach draft" value={draft} onChange={(event) => setDraft(event.target.value)} />{initialDraft && <input aria-label="Issue discussion" value={initialDraft} data-task={focusedTaskId} readOnly/>}</section>;
} }));
vi.mock("./features/agent/IssuePanel", () => ({ IssuePanel: ({onDiscuss,onLocateTask}:{onDiscuss: (cycleId:string,prompt:string,taskId:string)=>void;onLocateTask:(cycleId:string,taskId:string)=>void}) => <><button onClick={()=>onDiscuss("day-plan","Check this issue","task-1")}>Discuss diagnostic</button><button onClick={()=>onLocateTask("day-plan","task-1")}>Locate diagnostic</button></> }));
vi.mock("./features/later/LaterPanel", () => ({
  LaterPanel: () => <section role="region" aria-label="Later drawer" />,
}));
vi.mock("./features/trash/TrashPanel", () => ({
  TrashPanel: () => <section role="region" aria-label="Trash drawer" />,
}));
vi.mock("./features/proposals/ProposalsBar", () => ({ ProposalsBar: () => null }));
vi.mock("./features/reminders/MissedSummary", () => ({ MissedSummary: () => null }));
vi.mock("./features/settings/SettingsDialog", () => ({
  SettingsDialog: ({ open, onOpenFeedback, onPreviewExitPoll }: { open: boolean; onOpenFeedback?: () => void; onPreviewExitPoll?: () => void }) => open ? <div role="dialog" aria-modal="true"><button onClick={onOpenFeedback}>Give feedback</button><button onClick={onPreviewExitPoll}>Exit survey</button></div> : null,
}));
vi.mock("./features/onboarding/ExitPollDialog", () => ({ ExitPollDialog: ({ open }: { open: boolean }) => open ? <div role="dialog" aria-label="Exit survey" /> : null }));
vi.mock("./features/planner/PlannerWorkspace", () => ({
  PlannerWorkspace: ({ active, onPlanWithAI, onPageContextChange }: { active: boolean; onPlanWithAI: (id: string) => void; onPageContextChange?: (context: { view: "workspace"; long_term_cycle_id: string|null; week_cycle_id: string|null; day_cycle_id: string|null; week_starts_on: string|null; selected_date: string|null }) => void }) => {
    const [draft, setDraft] = useState("");
    useEffect(() => { onPageContextChange?.({ view: "workspace", long_term_cycle_id: "month-plan", week_cycle_id: "week-plan", day_cycle_id: "day-plan", week_starts_on: null, selected_date: null }); }, [onPageContextChange]);
    return <><input aria-label="Plan draft" data-active={active} value={draft} onChange={(e) => setDraft(e.target.value)} />
      {["month", "week", "day"].map((type) => <button key={type} onClick={() => onPlanWithAI(`${type}-plan`)}>Plan {type}</button>)}</>;
  },
}));
vi.mock("./features/calendar/CalendarView", () => ({
  default: () => {
    const [month, setMonth] = useState(9);
    return <button onClick={() => setMonth((value) => value + 1)}>Month {month}</button>;
  },
}));

import App from "./App";
import { applyLocale } from "./lib/i18n";

beforeEach(() => { applyLocale("en"); localStorage.clear(); planning.mockReset().mockResolvedValue({}); exitPollReady.mockReset().mockResolvedValue({ show: false }); });
function mount() {
  return render(<QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}><App /></QueryClientProvider>);
}

describe("workspace / calendar transitions", () => {
  it("mounts the getting-started guide in the application shell", () => {
    mount();
    expect(screen.getByRole("region", { name: "Getting started guide" })).toBeTruthy();
  });

  it("opens the feedback dialog from the settings feedback entry", () => {
    mount();
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    fireEvent.click(screen.getByRole("button", { name: "Give feedback" }));
    expect(screen.getByRole("dialog", { name: "Send feedback" })).toBeTruthy();
  });

  it("opens the exit survey through the settings manual entry", async () => {
    exitPollReady.mockResolvedValueOnce({ show: false }).mockResolvedValueOnce({ show: true });
    mount();
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    fireEvent.click(screen.getByRole("button", { name: "Exit survey" }));
    await waitFor(() => expect(exitPollReady).toHaveBeenCalledWith(true));
    expect(screen.getByRole("dialog", { name: "Exit survey" })).toBeTruthy();
  });

  it.each(["month", "week", "day"])("opens Coach for the exact %s plan and starts it once", async (type) => {
    mount();
    fireEvent.click(screen.getByRole("button", { name: `Plan ${type}` }));
    await waitFor(() => expect(planning).toHaveBeenCalledWith(`${type}-plan`, expect.objectContaining({ view: "workspace" })));
    expect(planning).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.getByRole("region", { name: "Coach context" }).textContent).toBe(`${type}-plan`));
  });
  it("preserves page state and scroll while the outgoing page becomes inert immediately", () => {
    mount();
    const workspace = screen.getByRole("tabpanel", { name: "Workspace" });
    const draft = screen.getByRole("textbox", { name: "Plan draft" });
    fireEvent.change(draft, { target: { value: "Keep this draft" } });
    workspace.scrollLeft = 240;

    fireEvent.click(screen.getByRole("tab", { name: "Calendar" }));
    expect(workspace.hasAttribute("inert")).toBe(true);
    expect(workspace.getAttribute("aria-hidden")).toBe("true");
    expect(draft.getAttribute("data-active")).toBe("false");
    expect(screen.queryByRole("textbox", { name: "Plan draft" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Month 9" }));

    fireEvent.click(screen.getByRole("tab", { name: "Workspace" }));
    expect(screen.getByRole("tabpanel", { name: "Workspace" })).toBe(workspace);
    expect(workspace.scrollLeft).toBe(240);
    expect((screen.getByRole("textbox", { name: "Plan draft" }) as HTMLInputElement).value).toBe("Keep this draft");
    fireEvent.click(screen.getByRole("tab", { name: "Calendar" }));
    expect(screen.getByRole("button", { name: "Month 10" })).toBeTruthy();
  });

  it("rapid keyboard reversals select the latest destination without waiting for animation events", () => {
    mount();
    const tabs = within(screen.getByRole("tablist", { name: "View" }));
    const workspace = tabs.getByRole("tab", { name: "Workspace" });
    const calendar = tabs.getByRole("tab", { name: "Calendar" });
    workspace.focus();
    fireEvent.keyDown(workspace, { key: "ArrowRight" });
    fireEvent.keyDown(calendar, { key: "ArrowLeft" });
    fireEvent.keyDown(workspace, { key: "End" });
    expect(document.activeElement).toBe(calendar);
    expect(calendar.getAttribute("aria-selected")).toBe("true");
    expect(screen.getAllByRole("tabpanel")).toHaveLength(1);
    expect(screen.getByRole("tabpanel", { name: "Calendar" }).hasAttribute("inert")).toBe(false);
    expect(localStorage.getItem("planner.preferred-view")).toBe("calendar");
  });

  it("opens the preferred view without mounting an unvisited page", () => {
    localStorage.setItem("planner.preferred-view", "calendar");
    mount();
    expect(screen.queryByRole("textbox", { name: "Plan draft", hidden: true })).toBeNull();
    expect(screen.getByRole("tabpanel", { name: "Calendar" })).toBeTruthy();
    fireEvent.click(screen.getByRole("tab", { name: "Workspace" }));
    expect(screen.getByRole("textbox", { name: "Plan draft" })).toBeTruthy();
  });
});

it("hands an issue to Coach as a draft and can locate it from Calendar",async()=>{
  mount();
  const issues=await screen.findByRole("button",{name:"Issues"});
  await waitFor(()=>expect((issues as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("tab",{name:"Calendar"}));
  fireEvent.click(issues);
  fireEvent.click(await screen.findByRole("button",{name:"Locate diagnostic"}));
  expect(screen.getByRole("tab",{name:"Workspace"}).getAttribute("aria-selected")).toBe("true");
  fireEvent.click(screen.getByRole("button",{name:"Discuss diagnostic"}));
  const draft=await screen.findByRole("textbox",{name:"Issue discussion"});
  expect((draft as HTMLInputElement).value).toBe("Check this issue");
  expect(draft.getAttribute("data-task")).toBe("task-1");
  expect(planning).not.toHaveBeenCalled();
});

it("keeps one Coach instance and its draft when the panel closes and reopens", async () => {
  mount();
  fireEvent.click(screen.getByRole("button", { name: "Coach" }));
  const panel = await screen.findByRole("region", { name: "Coach context" });
  const instance = panel.getAttribute("data-instance");
  const draft = screen.getByRole("textbox", { name: "Coach draft" });
  fireEvent.change(draft, { target: { value: "留住这段上下文" } });
  fireEvent.click(screen.getByRole("button", { name: "Coach" }));
  fireEvent.click(screen.getByRole("button", { name: "Coach" }));
  expect(screen.getByRole("region", { name: "Coach context" })).toBe(panel);
  expect(screen.getByRole("region", { name: "Coach context" }).getAttribute("data-instance")).toBe(instance);
  expect((screen.getByRole("textbox", { name: "Coach draft" }) as HTMLInputElement).value).toBe("留住这段上下文");
});

describe("Do Later keyboard shortcut", () => {
  function laterButton(): HTMLButtonElement {
    return screen.getByRole("button", { name: "Later" }) as HTMLButtonElement;
  }

  it("toggles once for the explicit macOS Command+Shift+L shortcut, including key case", () => {
    mount();
    fireEvent.keyDown(window, { key: "l", metaKey: true, shiftKey: true });
    expect(laterButton().getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("region", { name: "Later drawer" })).toBeTruthy();

    fireEvent.keyDown(window, { key: "L", metaKey: true, shiftKey: true });
    expect(laterButton().getAttribute("aria-pressed")).toBe("false");
  });

  it.each([
    ["Control instead of Command", { key: "L", ctrlKey: true, shiftKey: true }],
    ["both Command and Control", { key: "L", metaKey: true, ctrlKey: true, shiftKey: true }],
    ["Alt", { key: "L", metaKey: true, shiftKey: true, altKey: true }],
    ["missing Shift", { key: "L", metaKey: true }],
    ["IME composition", { key: "L", metaKey: true, shiftKey: true, isComposing: true }],
    ["auto repeat", { key: "L", metaKey: true, shiftKey: true, repeat: true }],
  ] as const)("does not toggle for %s", (_label, shortcut) => {
    mount();
    fireEvent.keyDown(window, shortcut);
    expect(laterButton().getAttribute("aria-pressed")).toBe("false");
    expect(screen.queryByRole("region", { name: "Later drawer" })).toBeNull();
  });

  it("does not toggle while a modal dialog is active", () => {
    mount();
    fireEvent.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.getByRole("dialog")).toBeTruthy();

    fireEvent.keyDown(window, { key: "L", metaKey: true, shiftKey: true });
    expect(laterButton().getAttribute("aria-pressed")).toBe("false");
    expect(screen.getByRole("dialog")).toBeTruthy();
  });
});

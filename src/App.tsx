import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AgentPanel } from "./features/agent/AgentPanel";
import CalendarView from "./features/calendar/CalendarView";
import { MissedSummary } from "./features/reminders/MissedSummary";
import { IssuePanel } from "./features/agent/IssuePanel";
import { LaterPanel } from "./features/later/LaterPanel";
import { PlannerWorkspace } from "./features/planner/PlannerWorkspace";
import { ProposalsBar } from "./features/proposals/ProposalsBar";
import { SettingsDialog } from "./features/settings/SettingsDialog";
import { getPlannerState, getSettings, LATER_CYCLE_ID } from "./lib/ipc";
import { initEventInvalidation, qk } from "./lib/events";
import { applyTheme, isTheme } from "./lib/theme";

/**
 * Workspace shell: the unified window bar on top, the optional Later panel on
 * the left, the horizontally scrolling planner columns filling the rest, and
 * the pending-proposals bar inside the workspace area (design.md §3.1).
 * Feature modules own their data; this file only wires layout and the global
 * keyboard shortcut.
 */
export default function App() {
  const queryClient = useQueryClient();
  const [laterOpen, setLaterOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  // Right-side context panel (design.md §3.1): the coach conversation or the
  // planning-issue report, both scoped to the cycle the workspace targets.
  const [rightPanel, setRightPanel] = useState<"agent" | "issues" | null>(null);
  // Top-level view switch (calendar change §5.7): workspace or calendar grid.
  const [view, setView] = useState<"workspace" | "calendar">(() =>
    (localStorage.getItem("planner.preferred-view") as "workspace" | "calendar" | null) ?? "workspace",
  );
  const switchView = (next: "workspace" | "calendar") => {
    setView(next);
    try { localStorage.setItem("planner.preferred-view", next); } catch { /* storage optional */ }
  };

  // The panel targets the plan the user is working in: the most recent day
  // column, falling back to week, then long-term (never the Later container).
  const { data: state } = useQuery({ queryKey: qk.plannerState(), queryFn: getPlannerState });
  const cycles = state?.cycles ?? [];
  const days = cycles.filter((c) => c.type === "day");
  const weeks = cycles.filter((c) => c.type === "week");
  const months = cycles.filter((c) => c.type === "month" && c.id !== LATER_CYCLE_ID);
  const activeCycleId = days.at(-1)?.id ?? weeks.at(-1)?.id ?? months.at(-1)?.id ?? null;

  // Backend events → react-query invalidation. The unlisten cleanup is async
  // because the listeners themselves are registered asynchronously.
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | null = null;
    initEventInvalidation(queryClient).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    }).catch(() => {
      // Without events the UI just stops self-refreshing; queries still work.
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [queryClient]);

  // app_settings is the durable theme truth; the localStorage copy read at
  // boot (initTheme in main.tsx) only exists to paint before this resolves.
  const { data: settings } = useQuery({ queryKey: qk.settings(), queryFn: getSettings });
  useEffect(() => {
    const theme = settings?.theme;
    if (isTheme(theme)) applyTheme(theme);
  }, [settings?.theme]);

  // ⌘⇧L toggles the Do Later drawer (spec: planner-workspace, 键盘优先).
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const isLaterShortcut =
        (event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === "l";
      if (isLaterShortcut) {
        event.preventDefault();
        setLaterOpen((open) => !open);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <div className="flex h-full flex-col bg-canvas">
      <WindowBar
        laterActive={laterOpen}
        agentActive={rightPanel === "agent"}
        issuesActive={rightPanel === "issues"}
        onToggleLater={() => setLaterOpen((open) => !open)}
        onToggleAgent={() => setRightPanel((p) => (p === "agent" ? null : "agent"))}
        onToggleIssues={() => setRightPanel((p) => (p === "issues" ? null : "issues"))}
        view={view}
        onSwitchView={switchView}
        onOpenSettings={() => setSettingsOpen(true)}
      />
      <div className="flex min-h-0 flex-1">
        {laterOpen && <LaterPanel onClose={() => setLaterOpen(false)} />}
        <main className="flex min-w-0 flex-1 flex-col">
          {view === "calendar" ? (
            <div className="min-h-0 flex-1">
              <CalendarView />
            </div>
          ) : (
            <>
              <MissedSummary />
              <div className="min-h-0 flex-1">
                <PlannerWorkspace />
              </div>
              <ProposalsBar />
            </>
          )}
        </main>
        {rightPanel === "agent" && activeCycleId && (
          <AgentPanel cycleId={activeCycleId} onClose={() => setRightPanel(null)} />
        )}
        {rightPanel === "issues" && activeCycleId && (
          <IssuePanel cycleId={activeCycleId} onClose={() => setRightPanel(null)} />
        )}
      </div>
      <SettingsDialog open={settingsOpen} onClose={() => setSettingsOpen(false)} />
    </div>
  );
}

/**
 * Unified window bar (design.md §3.4): native traffic lights sit in the left
 * safe area, application entries follow it, the middle stays draggable. The
 * bar shares the workspace canvas color and one hairline bottom border.
 */
function WindowBar({
  laterActive,
  agentActive,
  issuesActive,
  onToggleLater,
  onToggleAgent,
  onToggleIssues,
  onOpenSettings,
  view,
  onSwitchView,
}: {
  laterActive: boolean;
  agentActive: boolean;
  issuesActive: boolean;
  onToggleLater: () => void;
  onToggleAgent: () => void;
  onToggleIssues: () => void;
  onOpenSettings: () => void;
  view: "workspace" | "calendar";
  onSwitchView: (view: "workspace" | "calendar") => void;
}) {
  const buttonBase =
    "flex h-8 items-center gap-1.5 rounded-sm px-2 text-menu font-medium text-primary";
  const active = "bg-focus-surface text-focus";
  const hover = "hover:bg-hover";

  return (
    <header className="flex h-[var(--app-header-height)] shrink-0 items-center border-b border-light bg-canvas pr-4">
      {/* macOS traffic lights overlay this reserved strip (design.md §3.4). */}
      <div className="w-[104px] shrink-0" data-tauri-drag-region />
      <button
        type="button"
        aria-pressed={laterActive}
        title="Do Later (⌘⇧L)"
        className={`${buttonBase} ${laterActive ? active : hover}`}
        onClick={onToggleLater}
      >
        <ClockIcon />
        Later
      </button>
      {/* The spacer keeps dragging and double-click zoom native to the window. */}
      <div className="h-full min-w-4 flex-1" data-tauri-drag-region />
      <button
        type="button"
        aria-pressed={agentActive}
        title="Plan with AI"
        className={`${buttonBase} ${agentActive ? active : hover}`}
        onClick={onToggleAgent}
      >
        <CoachIcon />
        Coach
      </button>
      <button
        type="button"
        aria-pressed={issuesActive}
        title="Planning issues"
        className={`${buttonBase} ${issuesActive ? active : hover}`}
        onClick={onToggleIssues}
      >
        <FlagIcon />
        Issues
      </button>
      <div className="mr-2 flex items-center rounded-sm border border-light" role="tablist" aria-label="View">
        {(["workspace", "calendar"] as const).map((name) => (
          <button
            key={name}
            type="button"
            role="tab"
            aria-selected={view === name}
            className={`${buttonBase} ${view === name ? active : hover} capitalize`}
            onClick={() => onSwitchView(name)}
          >
            {name}
          </button>
        ))}
      </div>
      <button
        type="button"
        title="Settings"
        className={`${buttonBase} ${hover}`}
        onClick={onOpenSettings}
      >
        <GearIcon />
        Settings
      </button>
    </header>
  );
}

function ClockIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" aria-hidden="true">
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3.5 2" />
    </svg>
  );
}

function CoachIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M21 11.5a8.4 8.4 0 0 1-8.7 8.3 9 9 0 0 1-3.9-.9L3 20l1.2-4.1a8 8 0 0 1-1-3.9A8.4 8.4 0 0 1 12 3.7a8.4 8.4 0 0 1 9 7.8Z" />
    </svg>
  );
}

function FlagIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M5 21V4" />
      <path d="M5 4h13l-2.5 4L18 12H5" />
    </svg>
  );
}

function GearIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <circle cx="12" cy="12" r="3.2" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1.03 1.56V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1.11-1.56 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.7 1.7 0 0 0 .34-1.87 1.7 1.7 0 0 0-1.56-1.03H3a2 2 0 1 1 0-4h.09a1.7 1.7 0 0 0 1.56-1.11 1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34h.01a1.7 1.7 0 0 0 1.03-1.56V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1.03 1.56 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87v.01a1.7 1.7 0 0 0 1.56 1.03H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.51 1.03Z" />
    </svg>
  );
}

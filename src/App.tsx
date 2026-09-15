import { useEffect, useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AgentPanel } from "./features/agent/AgentPanel";
import { AI_TURN_MUTATION_KEY } from "./features/agent/PlanWithAI";
import { ExitPollDialog, type ExitPollResolution } from "./features/onboarding/ExitPollDialog";
import { markExitPollListenerReady } from "./features/onboarding/api";
import CalendarView from "./features/calendar/CalendarView";
import { MissedSummary } from "./features/reminders/MissedSummary";
import { IssuePanel } from "./features/agent/IssuePanel";
import { LaterPanel } from "./features/later/LaterPanel";
import { PlannerWorkspace } from "./features/planner/PlannerWorkspace";
import { WindowBar } from "./features/desktop/WindowBar";
import { matchesPrimaryShortcut } from "./lib/platform";
import { SettingsDialog } from "./features/settings/SettingsDialog";
import { getPlannerState, getSettings, LATER_CYCLE_ID, startPlanning } from "./lib/ipc";
import { initEventInvalidation, invalidateAgentEffects, qk } from "./lib/events";
import { PanelMotion } from "./ui/PanelMotion";
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
  const [coachSeed, setCoachSeed] = useState<{prompt:string;taskId:string|null}|null>(null);
  const [revealTask,setRevealTask] = useState<{cycleId:string;taskId:string;requestId:number}|null>(null);
  // Right-side context panel (design.md §3.1): the coach conversation or the
  // planning-issue report, both scoped to the cycle the workspace targets.
  const [rightPanel, setRightPanel] = useState<"agent" | "issues" | null>(null);
  // Exit survey (onboarding §3): the backend only answers once the frontend
  // signals readiness, so late-arriving decisions are never lost. A manual
  // trigger can force the survey for feedback purposes.
  const [exitPollOpen, setExitPollOpen] = useState(false);
  useEffect(() => {
    let cancelled = false;
    markExitPollListenerReady()
      .then((presentation) => {
        if (!cancelled && presentation.show) setExitPollOpen(true);
      })
      .catch(() => { /* survey is never load-bearing */ });
    return () => { cancelled = true; };
  }, []);

  // Top-level view switch (calendar change §5.7): workspace or calendar grid.
  const [view, setView] = useState<"workspace" | "calendar">(() => {
    try { return localStorage.getItem("planner.preferred-view") === "calendar" ? "calendar" : "workspace"; }
    catch { return "workspace"; }
  });
  const [visitedViews, setVisitedViews] = useState(() => ({
    workspace: view === "workspace",
    calendar: view === "calendar",
  }));
  const switchView = (next: "workspace" | "calendar") => {
    if (next === view) return;
    setVisitedViews((visited) => ({ ...visited, [next]: true }));
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
  const [workingCycleId, setWorkingCycleId] = useState<string | null>(null);
  const activeCycleId = cycles.find((c) => c.id === workingCycleId)?.id ?? days.at(-1)?.id ?? weeks.at(-1)?.id ?? months.at(-1)?.id ?? null;
  const planning = useMutation({ mutationKey: AI_TURN_MUTATION_KEY, mutationFn: startPlanning,
    onSettled: (_data, _error, id) => {
      invalidateAgentEffects(queryClient);
      return queryClient.invalidateQueries({ queryKey: qk.agentConversation(id) });
    },
  });
  const planWithAI = (id: string) => {
    if (planning.isPending) return;
    setWorkingCycleId(id);
    setCoachSeed(null);
    setRightPanel("agent");
    planning.mutate(id);
  };

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

  // The platform primary modifier toggles Later, outside modal interactions.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (matchesPrimaryShortcut(event, "L", true)
        && !document.querySelector('[role="dialog"][aria-modal="true"]')) {
        event.preventDefault();
        setLaterOpen((open) => !open);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const retainedPanel = useRef(rightPanel);
  if (rightPanel) retainedPanel.current = rightPanel;

  return (
    <div className="flex h-full flex-col bg-canvas">
      <WindowBar
        hasCycle={activeCycleId !== null}
        laterActive={laterOpen}
        agentActive={rightPanel === "agent"}
        issuesActive={rightPanel === "issues"}
        onToggleLater={() => setLaterOpen((open) => !open)}
        onReviewChanges={(id) => { setCoachSeed(null); setWorkingCycleId(id); setRightPanel("agent"); }}
        onToggleAgent={() => { setCoachSeed(null); setRightPanel((p) => (p === "agent" ? null : "agent")); }}
        onToggleIssues={() => setRightPanel((p) => (p === "issues" ? null : "issues"))}
        view={view}
        onSwitchView={switchView}
        onOpenSettings={() => setSettingsOpen(true)}
      />
      <div className="flex min-h-0 flex-1">
        <PanelMotion open={laterOpen} side="left"><LaterPanel onClose={() => setLaterOpen(false)} /></PanelMotion>
        <main className="relative min-h-0 min-w-0 flex-1 overflow-hidden">
          {/* Retain visited pages so a round trip preserves navigation and scroll.
              The outgoing page becomes inert immediately, independent of motion. */}
          <div id="view-workspace" role="tabpanel" aria-labelledby="tab-workspace"
            className="view-page" data-view="workspace" data-active={view === "workspace"}
            inert={view !== "workspace"} aria-hidden={view !== "workspace"}>
            {visitedViews.workspace && <>
              <MissedSummary />
              <div className="min-h-0 flex-1">
                <PlannerWorkspace revealTask={revealTask ?? undefined} active={view === "workspace"} onActiveCycleChange={setWorkingCycleId} onReviewIssues={(id) => { setWorkingCycleId(id); setRightPanel("issues"); }} onPlanWithAI={planWithAI} />
              </div>
            </>}
          </div>
          <div id="view-calendar" role="tabpanel" aria-labelledby="tab-calendar"
            className="view-page" data-view="calendar" data-active={view === "calendar"}
            inert={view !== "calendar"} aria-hidden={view !== "calendar"}>
            {visitedViews.calendar && <CalendarView active={view === "calendar"} onActiveCycleChange={setWorkingCycleId} onReviewIssues={(id) => { setWorkingCycleId(id); setRightPanel("issues"); }} onPlanWithAI={planWithAI} />}
          </div>
        </main>
        <PanelMotion open={rightPanel !== null && activeCycleId !== null}>
          {activeCycleId && (retainedPanel.current === "agent" ? <AgentPanel key={`agent:${activeCycleId}`} cycleId={activeCycleId} initialDraft={coachSeed?.prompt} focusedTaskId={coachSeed?.taskId} onClose={() => {setCoachSeed(null);setRightPanel(null);}}
            externalPlanning={planning.isPending && planning.variables === activeCycleId} planningError={planning.variables === activeCycleId ? planning.error : null} /> : <IssuePanel key={`issues:${activeCycleId}`} cycleId={activeCycleId} onClose={() => setRightPanel(null)}
            onOpenSettings={() => setSettingsOpen(true)}
            onLocateTask={(cycleId,taskId) => {switchView("workspace");setWorkingCycleId(cycleId);setRevealTask({cycleId,taskId,requestId:Date.now()});}}
            onDiscuss={(cycleId,prompt,taskId) => {setWorkingCycleId(cycleId);setCoachSeed({prompt,taskId});setRightPanel("agent");}} />)}
        </PanelMotion>
      </div>
      <SettingsDialog
        open={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        onPreviewExitPoll={() => setExitPollOpen(true)}
      />
      <ExitPollDialog
        open={exitPollOpen}
        onClose={(resolution: ExitPollResolution) => {
          setExitPollOpen(false);
          // "continued" keeps the app alive; the other resolutions were
          // recorded by the dialog itself and the window close proceeds.
          if (resolution !== "continued") window.close();
        }}
      />
    </div>
  );
}

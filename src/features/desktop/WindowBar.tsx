import { useRef } from "react";
import { useQuery } from "@tanstack/react-query";
import { platform, primaryShortcut } from "../../lib/platform";
import { useTranslation } from "../../lib/i18n";
import { qk } from "../../lib/events";
import { getEditorWorkspace, getSettings, LATER_CYCLE_ID } from "../../lib/ipc";
import { ProposalsBar } from "../proposals/ProposalsBar";
import { useDesktopWindow } from "./useDesktopWindow";

/**
 * Shared application bar with platform-specific system controls (design.md
 * §3.4). Only macOS reserves traffic-light space; Windows owns right controls.
 */
export function WindowBar({
  hasCycle,
  laterActive,
  agentActive,
  issuesActive,
  onToggleLater,
  onToggleAgent,
  onReviewChanges,
  onToggleIssues,
  onOpenSettings,
  view,
  onSwitchView,
}: {
  hasCycle: boolean;
  laterActive: boolean;
  agentActive: boolean;
  issuesActive: boolean;
  onToggleLater: () => void;
  onToggleAgent: () => void;
  onReviewChanges: (cycleId:string|null) => void;
  onToggleIssues: () => void;
  onOpenSettings: () => void;
  view: "workspace" | "calendar";
  onSwitchView: (view: "workspace" | "calendar") => void;
}) {
  const headerRef = useRef<HTMLElement>(null);
  const { t } = useTranslation("shell");
  // Shares the same cache entry as LaterPanel so task events refresh both the
  // panel and its navigation count together.
  const laterWorkspace = useQuery({
    queryKey: qk.editorWorkspace(LATER_CYCLE_ID),
    queryFn: () => getEditorWorkspace(LATER_CYCLE_ID),
  });
  const settings = useQuery({ queryKey: qk.settings(), queryFn: getSettings });
  const showLaterCount = settings.data?.show_later_count ?? true;
  const laterCount = (laterWorkspace.data?.tasks ?? []).filter(
    (task) => task.proposal === null && task.title.trim().length > 0,
  ).length;
  const showCount = showLaterCount && laterWorkspace.data !== undefined;
  const visibleCount = laterCount > 99 ? "99+" : String(laterCount);
  const maximizeRef = useRef<HTMLButtonElement>(null);
  const dragRef = useRef<HTMLDivElement>(null);
  const shell = useDesktopWindow(headerRef, maximizeRef, dragRef);
  const windowsControls = platform === "windows" && shell.mode !== "native";
  const buttonBase =
    "flex h-8 items-center gap-1.5 rounded-md px-2.5 text-menu font-medium text-secondary transition-colors duration-150 disabled:opacity-40";
  const active = "bg-focus-surface text-focus";
  const hover = "hover:bg-hover";

  return (
    <header ref={headerRef} className="window-bar" data-platform={platform}
      data-shell={shell.mode} data-fullscreen={shell.fullscreen} data-focused={shell.focused}>
      <div className="window-safe-area" data-tauri-drag-region={platform === "macos" ? true : undefined} />
      <button
        type="button"
        aria-pressed={laterActive}
        aria-label={showCount ? t("desktop.laterCountLabel", { count: laterCount }) : undefined}
        title={t("desktop.laterTitle", { shortcut: primaryShortcut("L", true) })}
        className={`${buttonBase} ${laterActive ? active : hover}`}
        onClick={onToggleLater}
      >
        <ClockIcon />
        {t("desktop.later")}
        {showCount && <span data-testid="later-count-badge" aria-hidden="true"
          className="rounded-full bg-subtle px-1.5 text-[11px] leading-4 tabular-nums text-secondary">
          {visibleCount}
        </span>}
      </button>
      {/* The spacer keeps dragging and double-click zoom native to the window. */}
      <div ref={dragRef} className="window-drag-area" data-tauri-drag-region={platform === "macos" ? true : undefined} />
      <div className="view-switch relative isolate grid grid-cols-2 rounded-lg border border-light bg-subtle p-0.5" role="tablist" aria-label={t("desktop.view")} data-view={view}>
        <span className="view-switch-thumb" aria-hidden="true" />
        {(["workspace", "calendar"] as const).map((name) => (
          <button key={name} id={`tab-${name}`} aria-controls={`view-${name}`} type="button" role="tab" aria-selected={view === name} tabIndex={view === name ? 0 : -1}
            className={`${buttonBase} relative justify-center ${view === name ? "text-primary" : "hover:text-primary"}`}
            onClick={() => onSwitchView(name)} onKeyDown={(event) => {
              if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
              event.preventDefault();
              const next = event.key === "Home" ? "workspace" : event.key === "End" ? "calendar" : view === "workspace" ? "calendar" : "workspace";
              onSwitchView(next);
              const tabs = event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>('[role="tab"]');
              tabs?.[next === "workspace" ? 0 : 1]?.focus();
            }}>{t(`desktop.${name}`)}</button>
        ))}
      </div>
      <div className="mx-3 h-5 w-px bg-light" aria-hidden="true" />
      <div role="group" aria-label={t("desktop.planningTools")} className="flex items-center gap-1">
        <button type="button" aria-expanded={agentActive}
          aria-label={t("desktop.coach")}
          title={t("desktop.openCoach")}
          className={`${buttonBase} ${agentActive ? active : hover}`} onClick={onToggleAgent}>
          <CoachIcon /><span className="window-tool-label">{t("desktop.coach")}</span>
        </button>
        {!agentActive && <ProposalsBar onOpen={onReviewChanges} />}
        <button type="button" aria-expanded={issuesActive} disabled={!hasCycle}
          aria-label={t("desktop.issues")}
          title={hasCycle ? t("desktop.openIssues") : t("desktop.createPlanIssues")}
          className={`${buttonBase} ${issuesActive ? active : hover}`} onClick={onToggleIssues}>
          <FlagIcon /><span className="window-tool-label">{t("desktop.issues")}</span>
        </button>
      </div>
      <div className="mx-3 h-5 w-px bg-light" aria-hidden="true" />
      <button
        type="button"
        title={t("desktop.settings")}
        aria-label={t("desktop.settings")}
        className={`${buttonBase} ${hover}`}
        onClick={onOpenSettings}
      >
        <GearIcon />
      </button>
      {windowsControls && <div className="window-controls" role="group" aria-label={t("desktop.windowControls")}>
        <button type="button" aria-label={t("desktop.minimize")} title={t("desktop.minimizeTitle")} onClick={() => void shell.act("minimize")}>
          <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M1 6.5h10" /></svg>
        </button>
        <button ref={maximizeRef} type="button" aria-label={shell.maximized ? t("desktop.restore") : t("desktop.maximize")}
          title={shell.maximized ? t("desktop.restoreTitle") : t("desktop.maximizeTitle")}
          data-hovered={shell.pointer.hovered} data-pressed={shell.pointer.pressed}
          onClick={() => void shell.act("toggleMaximize")}>
          <svg viewBox="0 0 12 12" aria-hidden="true">{shell.maximized
            ? <><path d="M3.5 3V1.5h7v7H9" /><rect x="1.5" y="3.5" width="7" height="7" rx=".5" /></>
            : <rect x="1.5" y="1.5" width="9" height="9" rx=".5" />}</svg>
        </button>
        <button type="button" className="window-close" aria-label={t("desktop.close")} title={t("desktop.closeTitle")} onClick={() => void shell.act("close")}>
          <svg viewBox="0 0 12 12" aria-hidden="true"><path d="m1.5 1.5 9 9m0-9-9 9" /></svg>
        </button>
      </div>}
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

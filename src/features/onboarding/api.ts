/**
 * Onboarding & lifecycle IPC surface (change: add-onboarding-and-lifecycle).
 *
 * 本地 api.ts 直连 @tauri-apps/api/core —— 与 src/lib/ipc.ts 同构（命令名
 * 集中声明、顶层参数键使用 camelCase、嵌套 serde 数据保持 snake_case），
 * 本模块的命令名落在本文件，前端测试按这些名字 mock invoke 以钉住线上契约。
 *
 * 隐私边界：退出调查与反馈在本版本没有上报通道，后端只写本地队列
 * （staged_events / staged_feedback）；`listStagedFeedback` 是将来显式导出
 * 入口的雏形。自动更新降级为版本比较 + `check_for_updates` 的三态反馈，
 * 未配置 `update_endpoint` 时返回 `{ status: "disabled" }`。
 */
import { invoke } from "@tauri-apps/api/core";

/** `service::onboarding::GuideState` — whole-guide state. */
export type GuideState = "active" | "completed" | "skipped";

/** `service::onboarding::StepStatus` — derived, never hand-checked. */
export type StepStatus = "not_done" | "done" | "skipped";

export interface GuideStep {
  id: string;
  title: string;
  detail: string;
  status: StepStatus;
}

/** `service::onboarding::GettingStartedGuide`. */
export interface GettingStartedGuide {
  state: GuideState;
  /** "N of 5" — real progress derived from live data. */
  completed: number;
  total: number;
  steps: GuideStep[];
}

/** `service::onboarding::SkipReport`. */
export interface SkipReport {
  deleted_cycle_ids: string[];
  deleted_task_count: number;
}

/** `service::onboarding::ExitPollPresentation`. */
export interface ExitPollPresentation {
  show: boolean;
  reason: "eligible" | "already_shown" | "not_eligible";
}

/** `service::onboarding::ExitPollAnswers`. */
export interface ExitPollAnswers {
  rating?: number | null;
  reason?: string | null;
  detail?: string | null;
}

/** `service::onboarding::StagedEvent` — 无上报通道，仅存本地。 */
export interface StagedEvent {
  id: string;
  kind: string;
  created_at: number;
  app_version: string;
  os: string;
  anonymous_id: string;
  payload: unknown;
}

/** `service::onboarding::StagedFeedback` — 无上报通道，仅存本地。 */
export interface StagedFeedback {
  id: string;
  created_at: number;
  app_version: string;
  os: string;
  anonymous_id: string;
  message: string;
}

/** `service::onboarding::FeedbackReceipt`. */
export interface FeedbackReceipt {
  staged: StagedFeedback;
  queue_len: number;
}

/** `service::onboarding::TalkToFounderEligibility`; `support_id` 即匿名标识。 */
export interface TalkToFounderEligibility {
  eligible: boolean;
  support_id: string;
  opened_at: number | null;
  open_count: number;
}

/** `service::onboarding::UpdateCheck` — 三态反馈 + 降级 `disabled`。 */
export type UpdateCheck =
  | { status: "disabled" }
  | { status: "up_to_date" }
  | { status: "available"; latest_version: string }
  | { status: "failed"; code: string; message: string };

/** `service::onboarding::LifecyclePrefs`. */
export interface LifecyclePrefs {
  daily_plan_time: string | null;
}

/** `commands::onboarding::SessionDueOutcome`. */
export type SessionDueOutcome =
  | { outcome: "notified" }
  | { outcome: "skipped"; reason: string };

/** Command names declared once, here, so the surface can be grepped. */
export const commands = {
  getOnboarding: "get_onboarding",
  completeOnboarding: "complete_onboarding",
  reconcileGettingStartedGuide: "reconcile_getting_started_guide",
  skipGettingStartedGuide: "skip_getting_started_guide",
  getDismissedHints: "get_dismissed_hints",
  dismissHint: "dismiss_hint",
  markExitPollListenerReady: "mark_exit_poll_listener_ready",
  acknowledgeExitPollShown: "acknowledge_exit_poll_shown",
  submitExitPoll: "submit_exit_poll",
  dismissExitPoll: "dismiss_exit_poll",
  continueAfterExitPoll: "continue_after_exit_poll",
  exitAfterExitPoll: "exit_after_exit_poll",
  sendFeedback: "send_feedback",
  listStagedFeedback: "list_staged_feedback",
  getTalkToFounderEligibility: "get_talk_to_founder_eligibility",
  recordTalkToFounderOpened: "record_talk_to_founder_opened",
  closeTalkToFounder: "close_talk_to_founder",
  getAppVersion: "get_app_version",
  checkForUpdates: "check_for_updates",
  getLifecyclePrefs: "get_lifecycle_prefs",
  setDailyPlanTime: "set_daily_plan_time",
  notifySessionDue: "notify_session_due",
} as const;

// --- §1 getting-started guide ------------------------------------------------

export function getOnboarding(): Promise<GettingStartedGuide> {
  return invoke<GettingStartedGuide>(commands.getOnboarding);
}

export function completeOnboarding(): Promise<GettingStartedGuide> {
  return invoke<GettingStartedGuide>(commands.completeOnboarding);
}

/** Re-derives progress from live data; call after cycle/task notices. */
export function reconcileGettingStartedGuide(): Promise<GettingStartedGuide> {
  return invoke<GettingStartedGuide>(commands.reconcileGettingStartedGuide);
}

/** Skips the whole guide and cleans up empty leftovers. */
export function skipGettingStartedGuide(): Promise<SkipReport> {
  return invoke<SkipReport>(commands.skipGettingStartedGuide);
}

// --- §2 one-shot hints ---------------------------------------------------------

export function getDismissedHints(): Promise<string[]> {
  return invoke<string[]>(commands.getDismissedHints);
}

export function dismissHint(hint_id: string): Promise<void> {
  return invoke<void>(commands.dismissHint, { hintId: hint_id });
}

// --- §3 exit poll ----------------------------------------------------------------

/**
 * The quit flow calls this once its listener is wired. `force = true` is the
 * manual menu entry; the automatic path leaves it unset and only presents
 * when the behaviour condition holds and the poll never showed before.
 */
export function markExitPollListenerReady(force?: boolean): Promise<ExitPollPresentation> {
  return invoke<ExitPollPresentation>(commands.markExitPollListenerReady, {
    force: force ?? null,
  });
}

export function acknowledgeExitPollShown(): Promise<void> {
  return invoke<void>(commands.acknowledgeExitPollShown);
}

export function submitExitPoll(args: ExitPollAnswers): Promise<StagedEvent> {
  return invoke<StagedEvent>(commands.submitExitPoll, { args });
}

export function dismissExitPoll(): Promise<StagedEvent> {
  return invoke<StagedEvent>(commands.dismissExitPoll);
}

export function continueAfterExitPoll(): Promise<void> {
  return invoke<void>(commands.continueAfterExitPoll);
}

export function exitAfterExitPoll(): Promise<void> {
  return invoke<void>(commands.exitAfterExitPoll);
}

// --- §4 feedback & talk-to-founder --------------------------------------------------

export function sendFeedback(message: string): Promise<FeedbackReceipt> {
  return invoke<FeedbackReceipt>(commands.sendFeedback, { message });
}

export function listStagedFeedback(): Promise<StagedFeedback[]> {
  return invoke<StagedFeedback[]>(commands.listStagedFeedback);
}

export function getTalkToFounderEligibility(): Promise<TalkToFounderEligibility> {
  return invoke<TalkToFounderEligibility>(commands.getTalkToFounderEligibility);
}

export function recordTalkToFounderOpened(): Promise<number> {
  return invoke<number>(commands.recordTalkToFounderOpened);
}

export function closeTalkToFounder(): Promise<void> {
  return invoke<void>(commands.closeTalkToFounder);
}

// --- §5 update check (degraded) --------------------------------------------------------

export function getAppVersion(): Promise<string> {
  return invoke<string>(commands.getAppVersion);
}

export function checkForUpdates(): Promise<UpdateCheck> {
  return invoke<UpdateCheck>(commands.checkForUpdates);
}

// --- daily plan time -----------------------------------------------------------------------

export function getLifecyclePrefs(): Promise<LifecyclePrefs> {
  return invoke<LifecyclePrefs>(commands.getLifecyclePrefs);
}

/** `null` clears the preference. */
export function setDailyPlanTime(daily_plan_time: string | null): Promise<void> {
  return invoke<void>(commands.setDailyPlanTime, { dailyPlanTime: daily_plan_time });
}

// --- §6 focus-block due notification -------------------------------------------------------

/**
 * The frontend timer fires this when a focus block reaches its planned end.
 * No duration → backend skips; permission denied → silent skip.
 */
export function notifySessionDue(
  session_id: string,
  title: string,
  duration_ms: number | null,
): Promise<SessionDueOutcome> {
  return invoke<SessionDueOutcome>(commands.notifySessionDue, {
    sessionId: session_id,
    title,
    durationMs: duration_ms,
  });
}

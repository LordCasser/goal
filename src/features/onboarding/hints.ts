/**
 * 一次性提示卡注册表（openspec onboarding-guidance「就地提示」）：
 * id + 触发条件 + 文案，全部收敛在这一个文件；后端只持久化已关闭的 id
 * 列表（get_dismissed_hints / dismiss_hint）。
 *
 * 触发条件是纯函数：输入当前工作区的少量事实（HintContext），输出该卡
 * 是否值得显示；已关闭的 id 一律不显示，与条件无关。
 */

/** The few workspace facts the trigger conditions may read. */
export interface HintContext {
  /** Visible items currently parked in the Later container. */
  laterCount?: number;
  /** Whether the day view just surfaced a focus block without a duration. */
  showedDurationlessSession?: boolean;
  /** Items currently linked across levels (week→long-term, day→week). */
  linkedItemCount?: number;
  /** Whether the getting-started guide reports 4 of 5 or better. */
  guideNearComplete?: boolean;
}

export interface HintDefinition {
  /** Stable id persisted in the backend dismissed list. */
  id: string;
  title: string;
  body: string;
  /** Trigger condition over the current workspace facts. */
  when: (context: HintContext) => boolean;
}

export const HINTS: HintDefinition[] = [
  {
    id: "later-explainer",
    title: "Park now, plan later",
    body:
      "Capture a goal the moment it shows up — nothing here needs a date " +
      "or a plan. Promote parked items into a long-term cycle when you are ready.",
    when: (context) => (context.laterCount ?? 0) === 0,
  },
  {
    id: "session-duration-explainer",
    title: "Durations power the timer",
    body:
      "A focus block with a set duration counts down to a notification when " +
      "time is up, and its focus time rolls up into your day, week and month.",
    when: (context) => context.showedDurationlessSession === true,
  },
  {
    id: "cross-link-explainer",
    title: "Link the levels",
    body:
      "Drag a weekly item under a long-term goal, or a daily task under a " +
      "weekly item, so every small step visibly serves the bigger plan.",
    when: (context) => (context.linkedItemCount ?? 0) === 0 && context.guideNearComplete === true,
  },
];

/** Hints whose trigger holds right now, minus the dismissed ids. */
export function visibleHints(context: HintContext, dismissed: string[]): HintDefinition[] {
  return HINTS.filter(
    (hint) =>
      !dismissed.includes(hint.id) && hint.when(context),
  );
}

/** The hint definition for one id, for the card renderer. */
export function hintById(id: string): HintDefinition | undefined {
  return HINTS.find((hint) => hint.id === id);
}

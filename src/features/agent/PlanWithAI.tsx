import { useIsMutating, useQuery } from "@tanstack/react-query";
import { getAiAvailability, getAppFlag, type Cycle } from "../../lib/ipc";
import { useTranslation } from "../../lib/i18n";
import { qk } from "../../lib/events";
import { Button } from "../../ui";

export const AI_SETTINGS_KEY = ["ai-settings"] as const;
export const AI_AVAILABILITY_KEY = ["ai-availability"] as const;
export const PLAN_WITH_AI_FLAG = "ui.plan-with-ai";
export const PLAN_WITH_AI_KEY = ["app-flag", PLAN_WITH_AI_FLAG] as const;
export const AI_TURN_MUTATION_KEY = ["ai-turn"] as const;

export function usePlanWithAIPreference() {
  return useQuery({ queryKey: PLAN_WITH_AI_KEY, queryFn: () => getAppFlag(PLAN_WITH_AI_FLAG) });
}

/** Existing plan and conversation ids remain the only context. */
export function PlanWithAI({ cycle, active, onPlan }: { cycle: Cycle; active: boolean; onPlan?: (id: string) => void }) {
  const { t } = useTranslation("ai");
  const preference = usePlanWithAIPreference();
  const ai = useQuery({ queryKey: AI_AVAILABILITY_KEY, queryFn: getAiAvailability, enabled: active });
  const turnBusy = useIsMutating({ mutationKey: AI_TURN_MUTATION_KEY }) > 0;
  const decisionBusy = useIsMutating({ mutationKey: qk.agentDecision() }) > 0;
  const busy = turnBusy || decisionBusy;
  if (!active || !onPlan || cycle.finished || cycle.archived || cycle.type === "session"
    || preference.isPending || preference.isError || preference.data === "false" || !ai.data) return null;
  return <div className="plan-ai-entry mx-3 mt-4">
    <Button variant="secondary" size="compact" className="gap-2! rounded-md!"
      disabled={busy} title={busy ? t("agent.planBusy") : t("agent.planThisCycle")}
      onClick={() => onPlan(cycle.id)}>
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true">
        <circle cx="12" cy="12" r="9" /><circle cx="12" cy="12" r="2" fill="currentColor" stroke="none" />
      </svg>
      {t("agent.planWithAi")}
    </Button>
  </div>;
}

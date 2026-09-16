/**
 * AI 侧栏 Coach（agent 会话；openspec planner-workspace「Agent 侧栏外壳」
 * 「会话消息的角色区分」「候选回答由模型内联给出」，design.md §8.2）。
 *
 * 外壳：标题栏（Coach + 当前技能小字 + 关闭）、可滚动消息区、固定底部
 * 输入区；挂载即 startAgentConversation 拿全局会话壳，切换周期只更新每次
 * turn 的页面上下文，不重建历史。消息按角色区分：用户靠右带浅底，模型文本靠左无底，
 * 工具/技能事件收敛为小号状态行——工具结果的 JSON 不裸露，只提炼状态
 * 短语。模型内联的 <next_steps> 剥离为可点快捷回复并附平台快捷键（有候选才
 * 拦截按键）。发送失败保留输入与历史，接入类错误给设置路径（8.2「失败」）。
 */
import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import type * as React from "react";
import { useIsMutating, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { completeAgentTurn, invalidateAgentEffects, qk } from "../../lib/events";
import {
  isAppError,
  sendAgentMessage,
  startAgentConversation,
  startPlanning,
  type AgentPageContext,
  type MessageView,
} from "../../lib/ipc";
import { errorMessage, formatMessage, t, useTranslation } from "../../lib/i18n";
import { matchesPrimaryShortcut, primaryShortcut } from "../../lib/platform";
import { Button, ProgressDot, cn } from "../../ui";
import { ChatMessage, ChatScrollArea, PromptInput } from "../../ui/ai";
import { PanelShell } from "./PanelShell";
import { parseNextSteps } from "./nextSteps";
import { AI_TURN_MUTATION_KEY } from "./PlanWithAI";
import { ChatMarkdown } from "./ChatMarkdown";
import { ThinkingIndicator } from "./ThinkingIndicator";
import { PlanApprovalCard } from "../proposals/PlanApprovalCard";
import { ActionApprovalCard } from "../proposals/ActionApprovalCard";

export type AgentPanelProps = {
  cycleId: string | null;
  pageContext?: AgentPageContext | null;
  onClose: () => void;
  externalPlanning?: boolean;
  planningError?: unknown;
  initialDraft?: string;
  focusedTaskId?: string | null;
};

/** 工具名 → 中文动作短语；未登记的工具退回显示工具名本身，不裸露 JSON。 */
const TOOL_LABEL_KEYS: Record<string, string> = {
  list_cycles: "ai:agent.tool.list_cycles", get_calendar: "ai:agent.tool.get_calendar", get_settings: "ai:agent.tool.get_settings",
  list_reminders: "ai:agent.tool.list_reminders", list_repeats: "ai:agent.tool.list_repeats", get_pending_changes: "ai:agent.tool.get_pending_changes",
  propose_cycle: "ai:agent.tool.propose_cycle", propose_focus_block: "ai:agent.tool.propose_focus_block", propose_task_organization: "ai:agent.tool.propose_task_organization",
  propose_day_move: "ai:agent.tool.propose_day_move", propose_reminder: "ai:agent.tool.propose_reminder", propose_repeat: "ai:agent.tool.propose_repeat", propose_settings: "ai:agent.tool.propose_settings",
  load_skill: "ai:agent.tool.load_skill",
  get_period_context: "ai:agent.tool.get_period_context",
  get_planning_issues: "ai:agent.tool.get_planning_issues",
  get_cycle_context: "ai:agent.tool.get_cycle_context",
  get_task_details: "ai:agent.tool.get_task_details",
  start_planning: "ai:agent.tool.start_planning",
  start_goal_setting: "ai:agent.tool.start_goal_setting",
  start_prioritization: "ai:agent.tool.start_prioritization",
  create_goal: "ai:agent.tool.create_goal",
  update_goal: "ai:agent.tool.update_goal",
  delete_goal: "ai:agent.tool.delete_goal",
  move_goal: "ai:agent.tool.move_goal",
  update_goal_breakdown: "ai:agent.tool.update_goal_breakdown",
  update_prioritization_breakdown: "ai:agent.tool.update_prioritization_breakdown",
};

/** 应用侧工具（app_tool_result）→ 已完成状态行。 */
const APP_TOOL_TEXT_KEYS: Record<string, string> = {
  start_planning: "ai:agent.app.start_planning",
  start_goal_setting: "ai:agent.app.start_goal_setting",
  start_prioritization: "ai:agent.app.start_prioritization",
};

/** 技能持久化值 → 标题栏小字（spec: 技能激活可见）。 */
const SKILL_LABEL_KEYS: Record<string, string> = {
  goal_setting: "ai:agent.skill.goal_setting",
  long_term_planning: "ai:agent.skill.long_term_planning",
  short_term_planning: "ai:agent.skill.short_term_planning",
  weekly_planning: "ai:agent.skill.weekly_planning", daily_planning: "ai:agent.skill.daily_planning",
  period_analysis: "ai:agent.skill.period_analysis", planning_issues: "ai:agent.skill.planning_issues", review: "ai:agent.skill.review",
  prioritization: "ai:agent.skill.prioritization",
};

/** 这些错误码意味着没接供应商：文案给设置路径，而不是当作临时失败（8.2）。 */
const PROVIDER_SETUP_CODES = new Set(["no_active_provider", "credentials_missing", "provider_not_verified"]);

/** 快捷回复的上限（主修饰键 + 1…9）。 */
const MAX_QUICK_REPLIES = 9;

export function AgentPanel({ cycleId, pageContext = null, onClose, externalPlanning = false, planningError, initialDraft = "", focusedTaskId }: AgentPanelProps): React.JSX.Element {
  const { t: translate } = useTranslation("ai");
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);
  const followLatest = useRef(true);
  const [showLatest, setShowLatest] = useState(false);
  useEffect(() => {
    // Issue discussions seed an empty composer once; never overwrite ordinary
    // user text when the active planning surface changes.
    if (initialDraft.trim() !== "") setDraft((current) => current.trim() === "" ? initialDraft : current);
  }, [initialDraft]);

  // One global conversation survives cycle/view changes. The page context is
  // attached to each turn, so changing selection never forks or resets history.
  const conversation = useQuery({
    queryKey: qk.agentConversation(),
    queryFn: startAgentConversation,
    refetchIntervalInBackground: true,
    refetchInterval: (query) => query.state.data?.active_turn_id
      ? 1000
      : query.state.data?.expires_at
        ? Math.max(1000, query.state.data.expires_at - Date.now())
        : false,
  });

  const invalidateConversation = () => {
    invalidateAgentEffects(queryClient);
  };

  // 发送失败保留输入与历史：只有成功才清空草稿、失效会话查询。
  const send = useMutation({
    mutationKey: AI_TURN_MUTATION_KEY,
    mutationFn: (input: { cycleId: string | null; text: string; focusedTaskId: string | null; pageContext: AgentPageContext | null; clearDraft: boolean }) =>
      sendAgentMessage(input.cycleId, input.text, input.focusedTaskId, input.pageContext),
    onSuccess: (result, input) => {
      completeAgentTurn(queryClient, result);
      if (input.clearDraft) setDraft((current) => current.trim() === input.text ? "" : current);
    },
    onSettled: invalidateConversation,
  });

  // 空会话的唯一起始入口：结果作为 app_tool_result 出现在会话里。
  const startPlan = useMutation({
    mutationKey: AI_TURN_MUTATION_KEY,
    mutationFn: (input: { cycleId: string; pageContext: AgentPageContext | null }) => startPlanning(input.cycleId, input.pageContext),
    onSuccess: (result) => completeAgentTurn(queryClient, result),
    onSettled: invalidateConversation,
  });
  const deciding = useIsMutating({ mutationKey: qk.agentDecision() }) > 0;
  const globallyBusy = useIsMutating({ mutationKey: AI_TURN_MUTATION_KEY }) > 0;
  const busy = globallyBusy || externalPlanning || Boolean(conversation.data?.active_turn_id);

  // 历史按序号排序渲染（spec: agent-conversation「消息模型」）。
  const messages = [...(conversation.data?.messages ?? [])].sort(
    (a, b) => a.sequence_number - b.sequence_number,
  );
  const transcript: Array<MessageView | MessageView[]> = [];
  for (const message of messages) {
    const receipt=message.payload.kind==="app_tool_result"&&message.payload.name==="approval_decision";
    const tool = !receipt&&["function_call", "function_result", "app_tool_result"].includes(message.payload.kind);
    const previous = transcript[transcript.length - 1];
    if (tool && Array.isArray(previous) && previous[0]?.turn_id === message.turn_id) previous.push(message);
    else transcript.push(tool ? [message] : message);
  }
  const lastMessage = messages[messages.length - 1];

  // 只有「最后一条消息是模型文本」时它才携带活的候选：用户一回复（或
  // 会话追加任何新消息），旧候选自然从候选区消失（spec: 用户点击候选）。
  const quickOptions =
    !busy && lastMessage?.payload.kind === "model_text"
      ? parseNextSteps(lastMessage.payload.text).options.slice(0, MAX_QUICK_REPLIES)
      : [];

  const submitMessage = (text: string, clearDraft: boolean) => {
    const trimmed = text.trim();
    if (trimmed === "" || busy || deciding) return;
    followLatest.current = true;
    setShowLatest(false);
    send.mutate({ cycleId, text: trimmed, focusedTaskId: focusedTaskId ?? null, pageContext, clearDraft });
  };

  // 面板内平台快捷键选中候选；有候选时才拦截对应组合（design.md 10）。
  const onPanelKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    const panel = event.currentTarget;
    const hasBlockingModal = [...document.querySelectorAll<HTMLElement>('[role="dialog"]')].some(
      (dialog) => !dialog.hidden && dialog.getAttribute("aria-hidden") !== "true" && !dialog.hasAttribute("inert"),
    );
    if (
      panel.hidden ||
      panel.getAttribute("aria-hidden") === "true" ||
      panel.hasAttribute("inert") ||
      event.defaultPrevented ||
      hasBlockingModal
    ) return;
    const digit = /^([1-9])$/.exec(event.key);
    if (!digit) return;
    const shortcutKey = digit[1];
    if (shortcutKey === undefined || !matchesPrimaryShortcut(event, shortcutKey)) return;
    const option = quickOptions[Number(shortcutKey) - 1];
    if (option === undefined) return;
    event.preventDefault();
    submitMessage(option, false);
  };

  // Follow replies only while reading the latest message; scrolling up keeps position.
  const messageCount = messages.length;
  useEffect(() => {
    const el = scrollRef.current;
    if (el && followLatest.current) el.scrollTop = el.scrollHeight;
  }, [messageCount, conversation.data?.revision, send.isPending]);

  const activeSkill = conversation.data?.active_skill ?? null;
  const skillLabel = activeSkill !== null && activeSkill !== "none"
    ? translate(SKILL_LABEL_KEYS[activeSkill] ?? activeSkill)
    : "";
  const skillSubtitle =
    activeSkill !== null && activeSkill !== "none"
      ? translate("agent.skillActive", { skill: skillLabel })
      : undefined;

  return (
    <PanelShell
      label={translate("agent.coach")}
      title={translate("agent.coach")}
      headerDetails={<>
        {skillSubtitle && <span className="min-w-0 flex-1 truncate text-caption text-secondary" title={skillSubtitle}>{skillLabel}</span>}
        <span className="ml-auto shrink-0 text-[11px] text-hint" title={translate("agent.contextRetentionTitle", { minutes: conversation.data?.context_idle_minutes ?? 15 })} aria-label={translate("agent.contextRetentionAria", { minutes: conversation.data?.context_idle_minutes ?? 15 })}>{translate("agent.contextRetentionValue", { minutes: conversation.data?.context_idle_minutes ?? 15 })}</span>
      </>}
      onClose={onClose}
      onKeyDown={onPanelKeyDown}
    >
      {/* 消息区独立滚动；输入区固定在底部（spec: Agent 侧栏外壳）。 */}
      <div className="relative min-h-0 flex-1">
      <ChatScrollArea ref={scrollRef} data-testid="coach-messages" className="h-full px-4 pb-4"
        onScroll={(event) => {
          const el = event.currentTarget;
          followLatest.current = el.scrollHeight - el.clientHeight - el.scrollTop < 40;
          setShowLatest(!followLatest.current);
        }}>
        <div className="flex min-w-0 flex-col gap-4">
          {conversation.isPending && (
            <p className="text-caption text-hint">{translate("agent.openingConversation")}</p>
          )}
          {conversation.isError && (
            <p className="text-caption text-danger">{errorText(conversation.error)}</p>
          )}
          {transcript.map((message) => Array.isArray(message) ? (
            <ToolActivity key={message[0]?.id} messages={message} decisions={messages} />
          ) : (
            <MessageRow
              key={message.id}
              message={message}
              quickOptions={message.id === lastMessage?.id ? quickOptions : undefined}
              onPickOption={(text) => submitMessage(text, false)}
            />
          ))}
          {/* 处理中：一行简短进度，不逐字刷屏（8.2「处理中」）。 */}
          {busy && <ThinkingIndicator />}
          {planningError != null && <p role="alert" className="text-caption text-danger">{errorText(planningError)}</p>}
          {conversation.data?.last_error && (
            <p className="flex items-center gap-1.5 text-caption text-danger">
              <ProgressDot tone="alert" />
              {conversation.data.last_error}
            </p>
          )}
          {conversation.isSuccess && messageCount === 0 && (
            <div className="flex flex-col items-start gap-3 pt-1">
              <p className="text-body text-secondary">
                {translate("agent.intro")}
              </p>
              <Button
                variant="secondary"
                size="compact"
                loading={startPlan.isPending}
                disabled={busy || deciding || cycleId === null}
                onClick={() => { if (cycleId !== null) startPlan.mutate({ cycleId, pageContext }); }}
              >
                {translate("agent.startPlanning")}
              </Button>
              {startPlan.isError && (
                <p className="text-caption text-danger">{errorText(startPlan.error)}</p>
              )}
            </div>
          )}
        </div>
      </ChatScrollArea>
      {showLatest && <div className="pointer-events-none absolute inset-x-0 bottom-3 flex justify-center">
        <Button size="icon" variant="secondary" className="pointer-events-auto h-8 w-8 rounded-full bg-content shadow-sm" aria-label={translate("agent.latestMessage")} title={translate("agent.latestMessage")} onClick={() => {
          const el = scrollRef.current;
          if (el) el.scrollTo({ top: el.scrollHeight, behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth" });
          followLatest.current = true;
          setShowLatest(false);
        }}><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M12 5v14m-6-6 6 6 6-6" /></svg></Button>
      </div>}
      </div>
      <footer className="shrink-0 border-t border-light px-4 py-3">
        <ChatScrollArea className="max-h-[38vh] space-y-3 empty:hidden [&:not(:empty)]:mb-3">
          <PlanApprovalCard disabled={busy} />
          <ActionApprovalCard disabled={busy} />
        </ChatScrollArea>
        {send.isError && (
          <div className="mb-2 flex flex-col gap-0.5">
            <p className="text-caption text-danger">{errorText(send.error)}</p>
            {isAppError(send.error) && PROVIDER_SETUP_CODES.has(send.error.code) && (
              <p className="text-caption text-secondary">{translate("agent.providerSetup")}</p>
            )}
          </div>
        )}
        <PromptInput value={draft} onValueChange={setDraft} onSubmit={() => submitMessage(draft, true)}
          disabled={busy || deciding} label={translate("agent.messageLabel")} placeholder={translate("agent.messagePlaceholder")}
          sendLabel={translate("agent.sendMessage")} hint={translate("agent.enterHint")} />
      </footer>
    </PanelShell>
  );
}

/**
 * 单条消息按角色渲染（spec: 会话消息的角色区分）：用户靠右带浅底，模型
 * 文本靠左无底，工具/技能事件是小号状态行。quickOptions 只在最后一条
 * 模型文本上由调用方传入——即当前还活着的候选。
 */
function MessageRow({
  message,
  quickOptions,
  onPickOption,
}: {
  message: MessageView;
  quickOptions?: string[];
  onPickOption?: (text: string) => void;
}) {
  const payload = message.payload;
  switch (payload.kind) {
    case "text":
      return (
        <ChatMessage role="user">{payload.text}</ChatMessage>
      );
    case "model_text": {
      const parsed = parseNextSteps(payload.text);
      const showOptions =
        quickOptions !== undefined &&
        quickOptions.length > 0 &&
        onPickOption !== undefined;
      return (
        <ChatMessage role="assistant">
          {parsed.body !== "" && (
            <ChatMarkdown>{parsed.body}</ChatMarkdown>
          )}
          {showOptions && onPickOption !== undefined && (
            <div className="flex w-full flex-col gap-1.5">
              {quickOptions.map((option, index) => (
                <button
                  key={option}
                  type="button"
                  onClick={() => onPickOption(option)}
                  className={cn(
                    "flex w-full items-center justify-between gap-2 rounded-md border border-light px-3 py-2 text-left",
                    "text-menu text-primary transition-colors duration-100 hover:bg-hover",
                  )}
                >
                  <span className="min-w-0 truncate">{option}</span>
                  {/* 快捷键在界面文案中可见（spec: 键盘优先）。 */}
                  <span className="shrink-0 text-caption text-hint">{primaryShortcut(String(index + 1))}</span>
                </button>
              ))}
            </div>
          )}
        </ChatMessage>
      );
    }
    case "app_tool_result": {
      const receipt = payload.name === "approval_decision" && isRecord(payload.result)
        ? formatApprovalReceipt(payload.result)
        : null;
      return receipt
        ? <p role="status" className="coach-message flex items-start gap-2 text-body text-primary"><span aria-hidden="true" className="text-secondary">✓</span><span className="min-w-0 break-words [overflow-wrap:anywhere]">{receipt}</span></p>
        : null;
    }
    case "function_call":
    case "function_result":
      return null; // Rendered together by ToolActivity.

  }
}

/** Collapse one turn's tool trace; a call and its result are one step. */
function ToolActivity({ messages, decisions }: { messages: MessageView[]; decisions: MessageView[] }) {
  const { t: translate } = useTranslation("ai");
  const [expanded, setExpanded] = useState(false);
  const detailsId = useId();
  const steps = new Map<string, { name: string; text: string; failed: boolean; complete: boolean; proposed: boolean; decision?:string }>();
  for (const message of messages) {
    const p = message.payload;
    if (p.kind === "function_call") {
      if (!steps.has(p.id)) {
        const label = TOOL_LABEL_KEYS[p.name] ? translate(TOOL_LABEL_KEYS[p.name]!) : p.name;
        steps.set(p.id, { name: p.name, text: translate("agent.stepIncomplete", { label }), failed: false, complete: false, proposed: false });
      }
    } else if (p.kind === "function_result") {
      const proposed=isRecord(p.result)&&p.result.status==="proposed";
      const result=isRecord(p.result)?p.result:{};
      const targetKind=result.action_id?"action":"task";
      const targetId=result.action_id??result.task_id;
      const receipt=proposed&&targetId?decisions.find(item=>item.sequence_number>message.sequence_number&&item.payload.kind==="app_tool_result"&&item.payload.name==="approval_decision"&&isRecord(item.payload.result)&&item.payload.result.target_kind===targetKind&&item.payload.result.target_id===targetId):undefined;
      const receiptResult = receipt?.payload.kind === "app_tool_result" && isRecord(receipt.payload.result) ? receipt.payload.result : undefined;
      const decision = receiptResult ? String(receiptResult.decision) : undefined;
      // Keep the aggregate status concise, but show the actual persisted operation in each step.
      const text = receiptResult
        ? formatApprovalReceipt(receiptResult)
        : functionResultText(p.name, p.result, p.is_error);
      steps.set(p.tool_call_id, { name: p.name, text: text ?? functionResultText(p.name, p.result, p.is_error), failed: p.is_error, complete: !p.is_error, proposed: proposed&&!decision, decision });
    } else if (p.kind === "app_tool_result") {
      const text = APP_TOOL_TEXT_KEYS[p.name]
        ? translate(APP_TOOL_TEXT_KEYS[p.name]!)
        : translate("agent.updatedPlanningContext");
      steps.set(message.id, { name: p.name, text, failed: false, complete: true, proposed: false });
    }
  }
  const entries = [...steps.values()];
  const failed = entries.filter((step) => step.failed).length;
  const incomplete = entries.filter((step) => !step.complete).length;
  const reads = entries.filter((step) => step.complete && step.name.startsWith("get_")).length;
  const proposed = entries.filter((step) => step.proposed).length;
  const confirmed=entries.filter(step=>step.decision==="applied").length;
  const rejected=entries.filter(step=>step.decision==="rejected").length;
  const summary = incomplete ? translate("agent.incompleteSteps", { count: incomplete }) : [
    reads ? translate("agent.reads", { count: reads }) : "",
    proposed ? translate("agent.pendingChanges", { count: proposed }) : "",
    confirmed ? translate("agent.confirmedChanges", { count: confirmed }) : "",
    rejected ? translate("agent.rejectedChanges", { count: rejected }) : "",
  ].filter(Boolean).join(" · ") || (
    entries.every((step) => step.name === "load_skill" || step.name.startsWith("start_"))
      ? translate("agent.preparedContext")
      : translate("agent.completedSteps", { count: entries.length })
  );
  return <div className="min-w-0">
    <button type="button" aria-label={`${summary}，${expanded ? translate("agent.collapse") : translate("agent.viewProcess")}`} aria-expanded={expanded} aria-controls={detailsId}
      onClick={() => setExpanded((value) => !value)}
      className={cn("-mx-1.5 flex max-w-full items-center gap-2 rounded-md px-1.5 py-1.5 text-left text-caption transition-colors hover:bg-hover", failed ? "text-danger" : "text-secondary")}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true" className={cn("shrink-0 transition-transform duration-150", expanded && "rotate-90")}><path d="m9 5 7 7-7 7" /></svg>
      <span>{summary}</span>
      <span className="shrink-0 text-hint">{expanded ? translate("agent.collapse") : translate("agent.viewProcess")}</span>
    </button>
    <div id={detailsId} role="region" aria-label={translate("agent.toolTrace")} aria-hidden={!expanded} inert={!expanded} data-expanded={expanded} className="coach-tool-details">
      <div className="min-h-0 overflow-hidden"><div className="ml-1.5 mt-1 flex flex-col gap-2 border-l border-light py-1 pl-4">
      {[...steps.entries()].map(([id, step]) => <StatusLine key={id} tone={step.failed ? "alert" : step.complete ? "done" : "idle"} danger={step.failed} text={step.text} />)}
      </div></div>
    </div>
  </div>;
}

/** 状态行：小号文字 + 状态点；错误用危险色（spec: 角色区分第三类）。 */
function StatusLine({
  tone,
  danger = false,
  text,
}: {
  tone: "idle" | "done" | "alert";
  danger?: boolean;
  text: string;
}) {
  return (
    <p className={cn("flex items-center gap-1.5 text-caption", danger ? "text-danger" : "text-secondary")}>
      <ProgressDot tone={tone} />
      {text}
    </p>
  );
}

/**
 * 工具结果：写入类以「待确认」口径陈述（预览是人与界面之间的契约，模型
 * 侧已报告成功）；结果 JSON 不裸露，错误只提炼 error/message 字段。
 */
function functionResultText(name: string, result: unknown, failed: boolean): string {
  const label = TOOL_LABEL_KEYS[name] ? t(TOOL_LABEL_KEYS[name]!) : name;
  if (failed) {
    const detail = resultDetail(result);
    return detail === "" ? t("ai:agent.resultFailed", { label }) : t("ai:agent.resultFailedDetail", { label, detail });
  }
  if (isRecord(result) && result["status"] === "proposed") {
    return t("ai:agent.resultProposed");
  }
  return t("ai:agent.resultCompleted", { label });
}

/** 从结果 JSON 里提炼一句人可读的错误说明；取不到就留空由调用方兜底。 */
function resultDetail(result: unknown): string {
  if (!isRecord(result)) return "";
  for (const key of ["error", "message", "detail"]) {
    const value = result[key];
    if (typeof value === "string" && value !== "") return value;
  }
  return "";
}

/** Render the structured approval receipt with the current UI locale. Historical
 * receipts may only have `text`, which remains a display fallback. */
function formatApprovalReceipt(result: Record<string, unknown>): string | null {
  // The transcript envelope keeps target metadata beside the structured
  // result (`{ text, result: { summary_key, details, ... } }`).
  const structured = isRecord(result.result) ? { ...result, ...result.result } : result;
  if (typeof structured.summary_key !== "string" || !structured.summary_key) {
    return typeof result.text === "string" ? result.text : null;
  }
  const summary = formatMessage({ key: structured.summary_key, args: {} });
  const rawDetails = Array.isArray(structured.details) ? structured.details : [];
  const detailMessages = rawDetails.map((detail) => {
    if (isRecord(detail) && typeof detail.key === "string" && isRecord(detail.args)) {
      return { key: detail.key, args: detail.args };
    }
    return typeof detail === "string" ? detail : "";
  });
  const status = typeof structured.decision === "string" && ["applied", "rejected", "closed"].includes(structured.decision)
    ? formatMessage({ key: `backend-actions:receipt.${structured.decision}`, args: {} })
    : "";
  const operation = structured.operation === "task_preview" || structured.target_kind === "task" ? "task" : "action";
  if (operation === "task") {
    const titleDetail = rawDetails.find((detail) => isRecord(detail) && isRecord(detail.args) && typeof detail.args.title === "string");
    const title = titleDetail && isRecord(titleDetail.args) && typeof titleDetail.args.title === "string"
      ? titleDetail.args.title
      : detailMessages.map((detail) => formatMessage(detail)).join(" · ");
    const receipt = formatMessage({ key: "backend-actions:receipt.task", args: { status, summary, title } });
    const destination = detailMessages.find((detail) => typeof detail !== "string" && detail.key === "backend-actions:task.destination");
    return destination ? `${receipt} ${formatMessage(destination)}` : receipt;
  }
  return formatMessage({ key: "backend-actions:receipt.action", args: { status, summary, details: detailMessages } });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function errorText(error: unknown): string {
  return errorMessage(error);
}

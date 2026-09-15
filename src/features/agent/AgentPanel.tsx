/**
 * AI 侧栏 Coach（agent 会话；openspec planner-workspace「Agent 侧栏外壳」
 * 「会话消息的角色区分」「候选回答由模型内联给出」，design.md §8.2）。
 *
 * 外壳：标题栏（Coach + 当前技能小字 + 关闭）、可滚动消息区、固定底部
 * 输入区；挂载即 startAgentConversation 拿会话壳，查询键带 cycleId，切换
 * 周期自动取该周期自己的会话（spec: agent-conversation「每周期一条会话」，
 * 历史互不串台）。消息按角色区分：用户靠右带浅底，模型文本靠左无底，
 * 工具/技能事件收敛为小号状态行——工具结果的 JSON 不裸露，只提炼状态
 * 短语。模型内联的 <next_steps> 剥离为可点快捷回复并附平台快捷键（有候选才
 * 拦截按键）。发送失败保留输入与历史，接入类错误给设置路径（8.2「失败」）。
 */
import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import type * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { invalidateAgentEffects, qk } from "../../lib/events";
import {
  isAppError,
  sendAgentMessage,
  startAgentConversation,
  startPlanning,
  type MessageView,
} from "../../lib/ipc";
import { matchesPrimaryShortcut, primaryShortcut } from "../../lib/platform";
import { Button, ProgressDot, cn } from "../../ui";
import { PanelShell } from "./PanelShell";
import { parseNextSteps } from "./nextSteps";
import { AI_TURN_MUTATION_KEY } from "./PlanWithAI";
import { ChatMarkdown } from "./ChatMarkdown";
import { ThinkingIndicator } from "./ThinkingIndicator";
import { PlanApprovalCard } from "../proposals/PlanApprovalCard";
import { ActionApprovalCard } from "../proposals/ActionApprovalCard";

export type AgentPanelProps = {
  cycleId: string;
  onClose: () => void;
  externalPlanning?: boolean;
  planningError?: unknown;
  initialDraft?: string;
  focusedTaskId?: string | null;
};

/** 工具名 → 中文动作短语；未登记的工具退回显示工具名本身，不裸露 JSON。 */
const TOOL_LABELS: Record<string, string> = {
  list_cycles: "查找计划周期", get_calendar: "读取日程", get_settings: "读取设置",
  list_reminders: "读取提醒", list_repeats: "读取重复模板", get_pending_changes: "读取待确认改动",
  propose_cycle: "提出周期操作", propose_focus_block: "安排专注块", propose_task_organization: "整理事务归属",
  propose_day_move: "调整日计划日期", propose_reminder: "安排提醒", propose_repeat: "调整重复模板", propose_settings: "提出设置修改",
  load_skill: "加载技能",
  get_period_context: "读取时段数据",
  get_planning_issues: "检查计划问题",
  get_cycle_context: "读取周期上下文",
  get_task_details: "读取任务详情",
  start_planning: "激活规划技能",
  start_goal_setting: "激活目标澄清技能",
  start_prioritization: "激活优先级技能",
  create_goal: "创建目标",
  update_goal: "更新目标",
  delete_goal: "删除目标",
  move_goal: "移动目标",
  update_goal_breakdown: "更新目标拆解",
  update_prioritization_breakdown: "更新优先级拆解",
};

/** 应用侧工具（app_tool_result）→ 已完成状态行。 */
const APP_TOOL_TEXTS: Record<string, string> = {
  start_planning: "已激活规划技能",
  start_goal_setting: "已激活目标澄清技能",
  start_prioritization: "已激活优先级技能",
};

/** 技能持久化值 → 标题栏小字（spec: 技能激活可见）。 */
const SKILL_LABELS: Record<string, string> = {
  goal_setting: "Goal setting",
  long_term_planning: "Long-term planning",
  short_term_planning: "Short-term planning",
  weekly_planning: "Weekly planning", daily_planning: "Daily planning",
  period_analysis: "Period analysis", planning_issues: "Plan issues", review: "Cycle review",
  prioritization: "Prioritization",
};

/** 这些错误码意味着没接供应商：文案给设置路径，而不是当作临时失败（8.2）。 */
const PROVIDER_SETUP_CODES = new Set(["no_active_provider", "credentials_missing", "provider_not_verified"]);

/** 输入框自动增高的上限；超过后内部滚动，不挤走消息区（8.2「输入中」）。 */
const INPUT_MAX_HEIGHT_PX = 120;

/** 快捷回复的上限（主修饰键 + 1…9）。 */
const MAX_QUICK_REPLIES = 9;

export function AgentPanel({ cycleId, onClose, externalPlanning = false, planningError, initialDraft = "", focusedTaskId }: AgentPanelProps): React.JSX.Element {
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState(initialDraft);
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const followLatest = useRef(true);
  const [showLatest, setShowLatest] = useState(false);

  // 会话壳：find-or-create，同一周期复用同一条会话；查询键含 cycleId，
  // 切换周期时 react-query 自动取该周期自己的会话。
  const conversation = useQuery({
    queryKey: qk.agentConversation(cycleId),
    queryFn: () => startAgentConversation(cycleId),
    enabled: cycleId !== "",
    refetchIntervalInBackground: true,
    refetchInterval: (query) => query.state.data?.expires_at
      ? Math.max(1000, query.state.data.expires_at - Date.now()) : false,
  });

  const invalidateConversation = () => {
    invalidateAgentEffects(queryClient);
    void queryClient.invalidateQueries({ queryKey: qk.agentConversation(cycleId) });
  };

  // 发送失败保留输入与历史：只有成功才清空草稿、失效会话查询。
  const send = useMutation({
    mutationKey: AI_TURN_MUTATION_KEY,
    mutationFn: (input: { text: string; clearDraft: boolean }) =>
      sendAgentMessage(cycleId, input.text, focusedTaskId),
    onSuccess: (_result, input) => {
      if (input.clearDraft) setDraft("");
    },
    onSettled: invalidateConversation,
  });

  // 空会话的唯一起始入口：结果作为 app_tool_result 出现在会话里。
  const startPlan = useMutation({
    mutationKey: AI_TURN_MUTATION_KEY,
    mutationFn: () => startPlanning(cycleId),
    onSettled: invalidateConversation,
  });
  const busy = send.isPending || startPlan.isPending || externalPlanning;

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
    if (trimmed === "" || cycleId === "" || busy) return;
    followLatest.current = true;
    setShowLatest(false);
    send.mutate({ text: trimmed, clearDraft });
  };

  const onInputKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    // Enter 发送、Shift+Enter 换行；输入法组合中的 Enter 不发送。
    if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing) return;
    event.preventDefault();
    submitMessage(draft, true);
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

  // 输入框自动增高：上限内随内容长高，之后内部滚动（8.2「输入中」）。
  useEffect(() => {
    const el = inputRef.current;
    if (!el) return;
    el.style.height = "auto";
    const next = Math.min(el.scrollHeight, INPUT_MAX_HEIGHT_PX);
    el.style.height = next > 0 ? `${next}px` : "";
  }, [draft]);

  // Follow replies only while reading the latest message; scrolling up keeps position.
  const messageCount = messages.length;
  useEffect(() => {
    followLatest.current = true;
    setShowLatest(false);
  }, [cycleId]);
  useEffect(() => {
    const el = scrollRef.current;
    if (el && followLatest.current) el.scrollTop = el.scrollHeight;
  }, [cycleId, messageCount, conversation.data?.revision, send.isPending]);

  const activeSkill = conversation.data?.active_skill ?? null;
  const skillSubtitle =
    activeSkill !== null && activeSkill !== "none"
      ? `${SKILL_LABELS[activeSkill] ?? activeSkill} active`
      : undefined;

  return (
    <PanelShell
      label="Coach"
      title="Coach"
      headerDetails={<>
        {skillSubtitle && <span className="min-w-0 flex-1 truncate text-caption text-secondary" title={skillSubtitle}>{SKILL_LABELS[activeSkill!] ?? activeSkill}</span>}
        <span className="ml-auto shrink-0 text-[11px] text-hint" title={`${conversation.data?.context_idle_minutes ?? 15} 分钟无新对话后自动清空上下文`} aria-label={`上下文保留 ${conversation.data?.context_idle_minutes ?? 15} 分钟`}>{conversation.data?.context_idle_minutes ?? 15}m</span>
      </>}
      onClose={onClose}
      onKeyDown={onPanelKeyDown}
    >
      {/* 消息区独立滚动；输入区固定在底部（spec: Agent 侧栏外壳）。 */}
      <div className="relative min-h-0 flex-1">
      <div ref={scrollRef} data-testid="coach-messages" className="h-full overflow-x-hidden overflow-y-auto px-4 pb-4"
        onScroll={(event) => {
          const el = event.currentTarget;
          followLatest.current = el.scrollHeight - el.clientHeight - el.scrollTop < 40;
          setShowLatest(!followLatest.current);
        }}>
        <div className="flex min-w-0 flex-col gap-4">
          {conversation.isPending && (
            <p className="text-caption text-hint">正在打开会话…</p>
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
                针对当前计划对话：汇报进展、提出调整，或先让 Coach 做一轮规划。
              </p>
              <Button
                variant="secondary"
                size="compact"
                loading={startPlan.isPending}
                disabled={busy}
                onClick={() => startPlan.mutate()}
              >
                Start planning
              </Button>
              {startPlan.isError && (
                <p className="text-caption text-danger">{errorText(startPlan.error)}</p>
              )}
            </div>
          )}
        </div>
      </div>
      {showLatest && <div className="pointer-events-none absolute inset-x-0 bottom-3 flex justify-center">
        <Button size="icon" variant="secondary" className="pointer-events-auto h-8 w-8 rounded-full bg-content shadow-sm" aria-label="回到最新消息" title="回到最新消息" onClick={() => {
          const el = scrollRef.current;
          if (el) el.scrollTo({ top: el.scrollHeight, behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "instant" : "smooth" });
          followLatest.current = true;
          setShowLatest(false);
        }}><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M12 5v14m-6-6 6 6 6-6" /></svg></Button>
      </div>}
      </div>
      <footer className="shrink-0 border-t border-light px-4 py-3">
        <div className="max-h-[38vh] overflow-y-auto">
          <PlanApprovalCard cycleId={cycleId} disabled={busy} />
          <ActionApprovalCard cycleId={cycleId} disabled={busy} />
        </div>
        {send.isError && (
          <div className="mb-2 flex flex-col gap-0.5">
            <p className="text-caption text-danger">{errorText(send.error)}</p>
            {isAppError(send.error) && PROVIDER_SETUP_CODES.has(send.error.code) && (
              <p className="text-caption text-secondary">先在设置中添加并激活供应商。</p>
            )}
          </div>
        )}
        <div className="coach-composer flex items-end gap-2 rounded-xl border border-light bg-subtle p-2 transition-colors focus-within:border-control focus-within:bg-content">
        <textarea
          ref={inputRef}
          aria-label="Message Coach"
          value={draft}
          rows={1}
          disabled={busy}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={onInputKeyDown}
          placeholder="说说你的想法…"
          className={cn(
            "max-h-[120px] min-w-0 flex-1 resize-none overflow-y-auto border-0 bg-transparent px-1.5 py-1.5 outline-none focus-visible:outline-none",
            "text-body text-primary placeholder:text-hint",
            "transition-colors duration-100",
            "disabled:cursor-not-allowed disabled:text-hint",
          )}
        />
        <Button variant="primary" size="icon" className="h-8 w-8 rounded-full" aria-label="Send message" title="发送 (Enter)"
          disabled={busy || !draft.trim() || !cycleId} onClick={() => submitMessage(draft, true)}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M12 19V5m-6 6 6-6 6 6" /></svg>
        </Button>
        </div>
        <p className="mt-1.5 px-1 text-[11px] text-hint">Enter 发送 · Shift+Enter 换行</p>
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
        <div className="coach-message flex justify-end">
          <p className="max-w-[85%] whitespace-pre-wrap break-words rounded-xl rounded-br-sm bg-subtle px-3.5 py-2.5 text-body text-primary">
            {payload.text}
          </p>
        </div>
      );
    case "model_text": {
      const parsed = parseNextSteps(payload.text);
      const showOptions =
        quickOptions !== undefined &&
        quickOptions.length > 0 &&
        onPickOption !== undefined;
      return (
        <div className="coach-message flex min-w-0 flex-col items-start gap-3">
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
        </div>
      );
    }
    case "app_tool_result":
      return payload.name==="approval_decision"&&isRecord(payload.result)&&typeof payload.result.text==="string"
        ? <p role="status" className="coach-message flex items-start gap-2 text-body text-primary"><span aria-hidden="true" className="text-secondary">✓</span><span>{payload.result.text}</span></p> : null;
    case "function_call":
    case "function_result":
      return null; // Rendered together by ToolActivity.

  }
}

/** Collapse one turn's tool trace; a call and its result are one step. */
function ToolActivity({ messages, decisions }: { messages: MessageView[]; decisions: MessageView[] }) {
  const [expanded, setExpanded] = useState(false);
  const detailsId = useId();
  const steps = new Map<string, { name: string; text: string; failed: boolean; complete: boolean; proposed: boolean; decision?:string }>();
  for (const message of messages) {
    const p = message.payload;
    if (p.kind === "function_call") {
      if (!steps.has(p.id)) steps.set(p.id, { name: p.name, text: `${TOOL_LABELS[p.name] ?? p.name}（未完成）`, failed: false, complete: false, proposed: false });
    } else if (p.kind === "function_result") {
      const proposed=isRecord(p.result)&&p.result.status==="proposed";
      const result=isRecord(p.result)?p.result:{};
      const targetKind=result.action_id?"action":"task";
      const targetId=result.action_id??result.task_id;
      const receipt=proposed&&targetId?decisions.find(item=>item.sequence_number>message.sequence_number&&item.payload.kind==="app_tool_result"&&item.payload.name==="approval_decision"&&isRecord(item.payload.result)&&item.payload.result.target_kind===targetKind&&item.payload.result.target_id===targetId):undefined;
      const receiptResult = receipt?.payload.kind === "app_tool_result" && isRecord(receipt.payload.result) ? receipt.payload.result : undefined;
      const decision = receiptResult ? String(receiptResult.decision) : undefined;
      // Keep the aggregate status concise, but show the actual persisted operation in each step.
      const text = receiptResult && typeof receiptResult.text === "string"
        ? receiptResult.text
        : functionResultText(p.name, p.result, p.is_error);
      steps.set(p.tool_call_id, { name: p.name, text, failed: p.is_error, complete: !p.is_error, proposed: proposed&&!decision, decision });
    } else if (p.kind === "app_tool_result") {
      steps.set(message.id, { name: p.name, text: APP_TOOL_TEXTS[p.name] ?? "已更新规划上下文", failed: false, complete: true, proposed: false });
    }
  }
  const entries = [...steps.values()];
  const failed = entries.filter((step) => step.failed).length;
  const incomplete = entries.filter((step) => !step.complete).length;
  const reads = entries.filter((step) => step.complete && step.name.startsWith("get_")).length;
  const proposed = entries.filter((step) => step.proposed).length;
  const confirmed=entries.filter(step=>step.decision==="applied").length;
  const rejected=entries.filter(step=>step.decision==="rejected").length;
  const summary = incomplete ? `${incomplete} 个步骤未完成` : [reads ? `已读取 ${reads} 项资料` : "", proposed ? `${proposed} 项修改待确认` : "", confirmed?`${confirmed} 项修改已确认`:"", rejected?`${rejected} 项修改已放弃`:""].filter(Boolean).join(" · ")
    || (entries.every((step) => step.name === "load_skill" || step.name.startsWith("start_")) ? "已准备规划上下文" : `已完成 ${entries.length} 个步骤`);
  return <div className="min-w-0">
    <button type="button" aria-label={`${summary}，${expanded ? "收起过程" : "查看过程"}`} aria-expanded={expanded} aria-controls={detailsId}
      onClick={() => setExpanded((value) => !value)}
      className={cn("-mx-1.5 flex max-w-full items-center gap-2 rounded-md px-1.5 py-1.5 text-left text-caption transition-colors hover:bg-hover", failed ? "text-danger" : "text-secondary")}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" aria-hidden="true" className={cn("shrink-0 transition-transform duration-150", expanded && "rotate-90")}><path d="m9 5 7 7-7 7" /></svg>
      <span>{summary}</span>
      <span className="shrink-0 text-hint">{expanded ? "收起" : "查看过程"}</span>
    </button>
    <div id={detailsId} role="region" aria-label="工具调用过程" aria-hidden={!expanded} inert={!expanded} data-expanded={expanded} className="coach-tool-details">
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
  const label = TOOL_LABELS[name] ?? name;
  if (failed) {
    const detail = resultDetail(result);
    return detail === "" ? `${label}失败` : `${label}失败：${detail}`;
  }
  if (isRecord(result) && result["status"] === "proposed") {
    return "已记录改动（待你确认）";
  }
  return `已${label}`;
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function errorText(error: unknown): string {
  return isAppError(error)
    ? error.message
    : error instanceof Error
      ? error.message
      : String(error);
}

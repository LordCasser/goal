/**
 * AI 侧栏 Coach（agent 会话；openspec planner-workspace「Agent 侧栏外壳」
 * 「会话消息的角色区分」「候选回答由模型内联给出」，design.md §8.2）。
 *
 * 外壳：标题栏（Coach + 当前技能小字 + 关闭）、可滚动消息区、固定底部
 * 输入区；挂载即 startAgentConversation 拿会话壳，查询键带 cycleId，切换
 * 周期自动取该周期自己的会话（spec: agent-conversation「每周期一条会话」，
 * 历史互不串台）。消息按角色区分：用户靠右带浅底，模型文本靠左无底，
 * 工具/技能事件收敛为小号状态行——工具结果的 JSON 不裸露，只提炼状态
 * 短语。模型内联的 <next_steps> 剥离为可点快捷回复并附 ⌘1…⌘9（有候选才
 * 拦截按键）。发送失败保留输入与历史，接入类错误给设置路径（8.2「失败」）。
 */
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { qk } from "../../lib/events";
import {
  isAppError,
  sendAgentMessage,
  startAgentConversation,
  startPlanning,
  type MessageView,
} from "../../lib/ipc";
import { Button, ProgressDot, cn } from "../../ui";
import { PanelShell } from "./PanelShell";
import { parseNextSteps } from "./nextSteps";

export type AgentPanelProps = {
  cycleId: string;
  onClose: () => void;
};

/** 工具名 → 中文动作短语；未登记的工具退回显示工具名本身，不裸露 JSON。 */
const TOOL_LABELS: Record<string, string> = {
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
  prioritization: "Prioritization",
};

/** 这些错误码意味着没接供应商：文案给设置路径，而不是当作临时失败（8.2）。 */
const PROVIDER_SETUP_CODES = new Set(["no_active_provider", "credentials_missing"]);

/** 输入框自动增高的上限；超过后内部滚动，不挤走消息区（8.2「输入中」）。 */
const INPUT_MAX_HEIGHT_PX = 120;

/** 快捷回复的上限（⌘1…⌘9）。 */
const MAX_QUICK_REPLIES = 9;

export function AgentPanel({ cycleId, onClose }: AgentPanelProps): React.JSX.Element {
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // 会话壳：find-or-create，同一周期复用同一条会话；查询键含 cycleId，
  // 切换周期时 react-query 自动取该周期自己的会话。
  const conversation = useQuery({
    queryKey: qk.agentConversation(cycleId),
    queryFn: () => startAgentConversation(cycleId),
    enabled: cycleId !== "",
  });

  const invalidateConversation = () => {
    void queryClient.invalidateQueries({ queryKey: qk.agentConversation(cycleId) });
  };

  // 发送失败保留输入与历史：只有成功才清空草稿、失效会话查询。
  const send = useMutation({
    mutationFn: (input: { text: string; clearDraft: boolean }) =>
      sendAgentMessage(cycleId, input.text),
    onSuccess: (_result, input) => {
      if (input.clearDraft) setDraft("");
      invalidateConversation();
    },
  });

  // 空会话的唯一起始入口：结果作为 app_tool_result 出现在会话里。
  const startPlan = useMutation({
    mutationFn: () => startPlanning(cycleId),
    onSuccess: invalidateConversation,
  });

  // 历史按序号排序渲染（spec: agent-conversation「消息模型」）。
  const messages = [...(conversation.data?.messages ?? [])].sort(
    (a, b) => a.sequence_number - b.sequence_number,
  );
  const lastMessage = messages[messages.length - 1];

  // 只有「最后一条消息是模型文本」时它才携带活的候选：用户一回复（或
  // 会话追加任何新消息），旧候选自然从候选区消失（spec: 用户点击候选）。
  const quickOptions =
    !send.isPending && lastMessage?.payload.kind === "model_text"
      ? parseNextSteps(lastMessage.payload.text).options.slice(0, MAX_QUICK_REPLIES)
      : [];

  const submitMessage = (text: string, clearDraft: boolean) => {
    const trimmed = text.trim();
    if (trimmed === "" || cycleId === "" || send.isPending) return;
    send.mutate({ text: trimmed, clearDraft });
  };

  const onInputKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    // Enter 发送、Shift+Enter 换行；输入法组合中的 Enter 不发送。
    if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing) return;
    event.preventDefault();
    submitMessage(draft, true);
  };

  // 面板内 ⌘1…⌘9 选中候选；有候选时才拦截这两个组合（design.md 10）。
  const onPanelKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (!(event.metaKey || event.ctrlKey)) return;
    const digit = /^([1-9])$/.exec(event.key);
    if (!digit) return;
    const option = quickOptions[Number(digit[1]) - 1];
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

  // 新消息到达时跟随到底部（8.2；这里没有流式，回合结束即滚动）。
  const messageCount = messages.length;
  useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [messageCount, conversation.data?.revision, send.isPending]);

  const activeSkill = conversation.data?.active_skill ?? null;
  const skillSubtitle =
    activeSkill !== null && activeSkill !== "none"
      ? `${SKILL_LABELS[activeSkill] ?? activeSkill} active`
      : undefined;

  return (
    <PanelShell
      label="Coach"
      title="Coach"
      subtitle={skillSubtitle}
      onClose={onClose}
      onKeyDown={onPanelKeyDown}
    >
      {/* 消息区独立滚动；输入区固定在底部（spec: Agent 侧栏外壳）。 */}
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
        <div className="flex flex-col gap-3">
          {conversation.isPending && (
            <p className="text-caption text-hint">正在打开会话…</p>
          )}
          {conversation.isError && (
            <p className="text-caption text-danger">{errorText(conversation.error)}</p>
          )}
          {messages.map((message) => (
            <MessageRow
              key={message.id}
              message={message}
              quickOptions={message.id === lastMessage?.id ? quickOptions : undefined}
              onPickOption={(text) => submitMessage(text, false)}
            />
          ))}
          {/* 处理中：一行简短进度，不逐字刷屏（8.2「处理中」）。 */}
          {send.isPending && (
            <p className="flex items-center gap-1.5 text-caption text-secondary">
              <ProgressDot tone="active" />
              正在思考…
            </p>
          )}
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
      <footer className="shrink-0 border-t border-light px-4 py-3">
        {send.isError && (
          <div className="mb-2 flex flex-col gap-0.5">
            <p className="text-caption text-danger">{errorText(send.error)}</p>
            {isAppError(send.error) && PROVIDER_SETUP_CODES.has(send.error.code) && (
              <p className="text-caption text-secondary">先在设置中添加并激活供应商。</p>
            )}
          </div>
        )}
        <textarea
          ref={inputRef}
          aria-label="Message Coach"
          value={draft}
          rows={1}
          disabled={send.isPending}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={onInputKeyDown}
          placeholder="说说你的想法…（Enter 发送 / Shift+Enter 换行）"
          className={cn(
            "max-h-[120px] w-full resize-none overflow-y-auto rounded-sm border border-control bg-content px-2 py-1.5",
            "text-body text-primary placeholder:text-hint",
            "transition-colors duration-100",
            "disabled:cursor-not-allowed disabled:bg-subtle disabled:text-hint",
          )}
        />
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
        <div className="flex justify-end">
          <p className="max-w-[85%] whitespace-pre-wrap rounded-sm bg-subtle px-3 py-2 text-body text-primary">
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
        <div className="flex flex-col items-start gap-2">
          {parsed.body !== "" && (
            <p className="whitespace-pre-wrap text-body text-primary">{parsed.body}</p>
          )}
          {showOptions && onPickOption !== undefined && (
            <div className="flex w-full flex-col gap-1.5">
              {quickOptions.map((option, index) => (
                <button
                  key={option}
                  type="button"
                  onClick={() => onPickOption(option)}
                  className={cn(
                    "flex w-full items-center justify-between gap-2 rounded-sm border border-control px-2.5 py-1.5 text-left",
                    "text-menu text-primary transition-colors duration-100 hover:bg-hover",
                  )}
                >
                  <span className="min-w-0 truncate">{option}</span>
                  {/* 快捷键在界面文案中可见（spec: 键盘优先）。 */}
                  <span className="shrink-0 text-caption text-hint">⌘{index + 1}</span>
                </button>
              ))}
            </div>
          )}
        </div>
      );
    }
    case "function_call":
      return <StatusLine tone="idle" text={functionCallText(payload.name)} />;
    case "function_result": {
      const failed = payload.is_error;
      return (
        <StatusLine
          tone={failed ? "alert" : "done"}
          danger={failed}
          text={functionResultText(payload.name, payload.result, failed)}
        />
      );
    }
    case "app_tool_result":
      return (
        <StatusLine tone="done" text={APP_TOOL_TEXTS[payload.name] ?? `已更新计划上下文（${payload.name}）`} />
      );
  }
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

/** 进行中的工具调用：`正在…`；未知工具显示工具名本身。 */
function functionCallText(name: string): string {
  const label = TOOL_LABELS[name];
  return label === undefined ? `正在调用 ${name}…` : `正在${label}…`;
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

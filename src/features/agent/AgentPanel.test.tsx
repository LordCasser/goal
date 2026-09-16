/**
 * AgentPanel 行为契约（openspec planner-workspace「Agent 侧栏外壳」
 * 「会话消息的角色区分」「候选回答由模型内联给出」）：挂载即取会话壳、
 * 消息按角色渲染、候选回答剥离正文并可点/快捷键发送、发送失败保留输入
 * 与历史、空会话给 Start planning 入口。invoke 按 src/lib/ipc.test.ts 的
 * 方式 mock——组件经由 lib/ipc 走真实包装，同时钉住线上的参数键。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { applyLocale } from "../../lib/i18n";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
// Component tests choose their target explicitly; the host running Vitest is
// not evidence that Coach is running in a macOS shell.
vi.mock("../../lib/platform", () => ({
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
      nativeEvent?: { repeat?: boolean; isComposing?: boolean };
    },
    key: string,
    shift = false,
  ) => event.key === key && event.metaKey === true && event.ctrlKey !== true && event.altKey !== true
    && (event.shiftKey === true) === shift && event.repeat !== true && event.nativeEvent?.repeat !== true
    && event.isComposing !== true && event.nativeEvent?.isComposing !== true,
}));

import {
  commands,
  type ConversationView,
  type MessageView,
  type TurnResult,
} from "../../lib/ipc";
import { AgentPanel } from "./AgentPanel";

function userMessage(text: string, sequence = 1): MessageView {
  return {
    id: `u-${sequence}`,
    sequence_number: sequence,
    message_type: "user",
    turn_id: "turn-1",
    payload: { kind: "text", text },
  };
}

function modelTextMessage(text: string, sequence: number): MessageView {
  return {
    id: `m-${sequence}`,
    sequence_number: sequence,
    message_type: "model_text",
    turn_id: "turn-1",
    payload: { kind: "model_text", text },
  };
}

it.each([undefined, { summary_key: "", details: [], decision: "rejected" }])("renders a persisted approval receipt inside the scrolling transcript, never in the composer footer (%j)",async(legacyResult)=>{
  mockBackend({conversation:conversationView([
    modelTextMessage("改动已准备好",1),
    {id:"receipt",turn_id:"decision",sequence_number:2,message_type:"app_tool_result",payload:{kind:"app_tool_result",name:"approval_decision",result:{text:"已应用到计划：示例任务", result: legacyResult}}},
    userMessage("继续安排明天",3),
  ])});
  renderPanel();
  const receipt=await screen.findByText("已应用到计划：示例任务");
  expect(receipt.closest("footer")).toBeNull();
  expect(receipt.closest('[role="status"]')).toBeTruthy();
  expect(receipt.closest('[class*="overflow-y-auto"]')).toBeTruthy();
});

function conversationView(
  messages: MessageView[],
  over: Partial<ConversationView> = {},
): ConversationView {
  return {
    expires_at: null,
    context_idle_minutes: 15,
    id: "conv-1",
    active_turn_id: null,
    revision: 1,
    active_skill: null,
    last_error: null,
    messages,
    ...over,
  };
}

function turnResult(messages: MessageView[] = []): TurnResult {
  return {
    conversation_id: "conv-1",
    revision: 2,
    active_skill: null,
    reply: "",
    messages,
  };
}

/** 按命令名分发固定返回值；未覆盖的命令一律返回 null。 */
function mockBackend(
  over: {
    conversation?: ConversationView;
    sendResult?: TurnResult;
    sendError?: unknown;
  } = {},
): void {
  invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case commands.startAgentConversation:
        return Promise.resolve(over.conversation ?? conversationView([]));
      case commands.sendAgentMessage:
        return over.sendError !== undefined
          ? Promise.reject(over.sendError)
          : Promise.resolve(over.sendResult ?? turnResult());
      default:
        return Promise.resolve(null);
    }
  });
}

function renderPanel(props: Partial<import("./AgentPanel").AgentPanelProps> = {}): { onClose: ReturnType<typeof vi.fn> } {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const onClose = vi.fn();
  render(
    <QueryClientProvider client={client}>
      <AgentPanel cycleId="c1" onClose={onClose} {...props} />
    </QueryClientProvider>,
  );
  return { onClose };
}

function typeDraft(text: string): HTMLTextAreaElement {
  const input = screen.getByLabelText("给助理发消息") as HTMLTextAreaElement;
  fireEvent.change(input, { target: { value: text } });
  return input;
}

beforeEach(() => {
  applyLocale("zh-CN");
  invokeMock.mockReset();
});

describe("AgentPanel", () => {
  it("fetches the global conversation shell on mount", async () => {
    mockBackend({
      conversation: conversationView([userMessage("帮我把目标拆细", 1)]),
    });
    renderPanel();

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.startAgentConversation),
    );
    expect(await screen.findByText("帮我把目标拆细")).toBeDefined();
  });

  it("renders user and model text with distinct roles", async () => {
    mockBackend({
      conversation: conversationView([
        userMessage("帮我把目标拆细", 1),
        modelTextMessage("好的，可以逐条来。", 2),
      ]),
    });
    renderPanel();

    const userEl = await screen.findByText("帮我把目标拆细");
    // 用户消息：靠右 + 浅底（spec: 用户消息）。
    expect(userEl.className).toContain("bg-subtle");
    expect((userEl.parentElement as HTMLElement).className).toContain("justify-end");

    // 模型文本：靠左、无底色。
    const modelEl = screen.getByText("好的，可以逐条来。");
    expect(modelEl.className).not.toContain("bg-subtle");
    expect((modelEl.parentElement as HTMLElement).className).not.toContain("justify-end");
  });

  it("strips the next_steps marker from the body and renders quick-reply buttons", async () => {
    mockBackend({
      conversation: conversationView([
        modelTextMessage("我们可以继续。<next_steps>先做周计划 | 先设目标</next_steps>", 1),
      ]),
    });
    renderPanel();

    // 正文里不出现标记本身，候选渲染为按钮并附快捷键编号。
    expect(await screen.findByText("我们可以继续。")).toBeDefined();
    expect(screen.queryByText(/<next_steps>/)).toBeNull();
    expect(screen.getByRole("button", { name: /先做周计划/ }).textContent).toContain("⌘1");
    expect(screen.getByRole("button", { name: /先设目标/ }).textContent).toContain("⌘2");
  });

  it("sends the candidate text as a user message on click", async () => {
    mockBackend({
      conversation: conversationView([
        modelTextMessage("我们可以继续。<next_steps>先做周计划 | 先设目标</next_steps>", 1),
      ]),
    });
    renderPanel();

    fireEvent.click(await screen.findByRole("button", { name: /先做周计划/ }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.sendAgentMessage, {
        cycleId: "c1",
        text: "先做周计划",
        focusedTaskId: null,
        pageContext: null,
      }),
    );
  });

  it("picks the first candidate with ⌘1", async () => {
    mockBackend({
      conversation: conversationView([
        modelTextMessage("<next_steps>先做周计划 | 先设目标</next_steps>", 1),
      ]),
    });
    renderPanel();
    await screen.findByRole("button", { name: /先做周计划/ });

    fireEvent.keyDown(screen.getByRole("complementary", { name: "助理" }), {
      key: "1",
      metaKey: true,
    });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.sendAgentMessage, {
        cycleId: "c1",
        text: "先做周计划",
        focusedTaskId: null,
        pageContext: null,
      }),
    );
  });

  it("rejects wrong modifiers, IME composition, and repeated candidate shortcuts", async () => {
    mockBackend({
      conversation: conversationView([
        modelTextMessage("<next_steps>先做周计划 | 先设目标</next_steps>", 1),
      ]),
    });
    renderPanel();
    const panel = await screen.findByRole("complementary", { name: "助理" });

    for (const event of [
      { key: "1", ctrlKey: true },
      { key: "1", metaKey: true, altKey: true },
      { key: "1", metaKey: true, shiftKey: true },
      { key: "1", metaKey: true, repeat: true },
      { key: "1", metaKey: true, isComposing: true },
    ]) {
      fireEvent.keyDown(panel, event);
    }
    expect(invokeMock.mock.calls.some(([command]) => command === commands.sendAgentMessage)).toBe(false);
  });

  it("does not send a candidate while 助理 is busy or hidden", async () => {
    mockBackend({
      conversation: conversationView([
        modelTextMessage("<next_steps>先做周计划</next_steps>", 1),
      ]),
    });
    const { onClose } = renderPanel({ externalPlanning: true });
    const panel = await screen.findByRole("complementary", { name: "助理" });
    expect(screen.queryByRole("button", { name: /先做周计划/ })).toBeNull();
    fireEvent.keyDown(panel, { key: "1", metaKey: true });
    expect(invokeMock.mock.calls.some(([command]) => command === commands.sendAgentMessage)).toBe(false);

    // The same guard remains effective if a host temporarily marks the panel
    // inert while changing context.
    panel.setAttribute("inert", "");
    fireEvent.keyDown(panel, { key: "1", metaKey: true });
    expect(onClose).not.toHaveBeenCalled();
    expect(invokeMock.mock.calls.some(([command]) => command === commands.sendAgentMessage)).toBe(false);
  });

  it("keeps the draft and the history when sending fails", async () => {
    mockBackend({
      conversation: conversationView([modelTextMessage("好的，可以逐条来。", 1)]),
      sendError: { code: "provider_unreachable", message: "Provider unreachable." },
    });
    renderPanel();
    await screen.findByText("好的，可以逐条来。");

    const input = typeDraft("帮我把目标拆细");
    fireEvent.keyDown(input, { key: "Enter" });

    // 行内显示错误，输入与历史都保留（spec: 失败）。
    expect(await screen.findByText("无法连接供应商，请检查网络和服务地址。")).toBeDefined();
    expect(input.value).toBe("帮我把目标拆细");
    expect(screen.getByText("好的，可以逐条来。")).toBeDefined();
    expect(invokeMock).toHaveBeenCalledWith(commands.sendAgentMessage, {
      cycleId: "c1",
      text: "帮我把目标拆细",
      focusedTaskId: null,
      pageContext: null,
    });
  });

  it("points at Settings for provider-setup errors", async () => {
    mockBackend({
      sendError: { code: "no_active_provider", message: "no active AI provider" },
    });
    renderPanel();

    const input = await screen.findByLabelText("给助理发消息");
    fireEvent.change(input, { target: { value: "你好" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(await screen.findByText(/先在设置中添加并激活供应商/)).toBeDefined();
  });

  it("offers Start planning on an empty conversation", async () => {
    mockBackend();
    renderPanel();

    fireEvent.click(await screen.findByRole("button", { name: "开始规划" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.startPlanning, { cycleId: "c1", pageContext: null }),
    );
  });

  it("closes the panel on Escape", async () => {
    mockBackend();
    const { onClose } = renderPanel();
    await screen.findByLabelText("给助理发消息");

  fireEvent.keyDown(screen.getByLabelText("给助理发消息"), { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
  });
  it("renders model emphasis and lists without interpreting raw HTML or loading remote images", async () => {
    mockBackend({ conversation: conversationView([modelTextMessage("**One priority**\n\n- Review the draft\n- Run the test\n\n<img src='https://example.com/track'>\n\n![hidden image](https://example.com/image)", 1)]) });
    renderPanel();
    const title = await screen.findByText("One priority");
    expect(title.tagName).toBe("STRONG");
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    expect(document.querySelector("img")).toBeNull();
  });

  it("renders a model table while keeping next-step actions outside Markdown", async () => {
    mockBackend({ conversation: conversationView([modelTextMessage("| Task | Status |\n| --- | --- |\n| Review | Open |\n\n<next_steps>继续规划</next_steps>", 1)]) });
    renderPanel();
    expect(await screen.findByRole("table")).toBeDefined();
    expect(screen.getByRole("cell", { name: "Review" })).toBeDefined();
    fireEvent.click(screen.getByRole("button", { name: /继续规划/ }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(commands.sendAgentMessage, { cycleId: "c1", text: "继续规划", focusedTaskId: null, pageContext: null }));
    expect(document.body.textContent).not.toContain("next_steps");
  });

});

it("automatically clears expired history using the backend deadline", async () => {
  let reads = 0;
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd !== commands.startAgentConversation) return Promise.resolve(null);
    reads += 1;
    return Promise.resolve(reads === 1
      ? conversationView([modelTextMessage("Expiring context", 1)], { active_turn_id: "turn-1" })
      : conversationView([], { revision: 2 }));
  });
  renderPanel();
  await screen.findByText("Expiring context");
  await waitFor(() => expect(screen.queryByText("Expiring context")).toBeNull(), { timeout: 2500 });
  expect(reads).toBeGreaterThan(1);
});

it("collapses tool activity and merges each call/result into one step", async () => {
  const tools: MessageView[] = [1, 2, 3, 4].flatMap((n) => [{ id: `call-${n}`, sequence_number: n, turn_id: "turn", message_type: "model_function_call" as const, payload: { kind: "function_call" as const, id: `call-${n}`, name: "get_task_details", arguments: { task_id: `task-${n}` } } },
    { id: `result-${n}`, sequence_number: n + 4, turn_id: "turn", message_type: "function_result" as const, payload: { kind: "function_result" as const, tool_call_id: `call-${n}`, name: "get_task_details", result: {}, is_error: false } }]);
  mockBackend({ conversation: conversationView([...tools, modelTextMessage("具体建议", 9)]) });
  renderPanel();
  const summary = await screen.findByRole("button", { name: /已读取 4 项资料.*查看过程/ });
  expect(summary.getAttribute("aria-expanded")).toBe("false");
  expect(screen.queryByRole("region", { name: "工具调用过程" })).toBeNull();
  expect(screen.queryByText(/正在读取任务详情/)).toBeNull();
  fireEvent.click(summary);
  expect(screen.getByRole("region", { name: "工具调用过程" })).toBeDefined();
  expect(screen.getAllByText("已读取任务详情")).toHaveLength(4);
  expect(screen.getByText("具体建议")).toBeTruthy();
  fireEvent.click(summary);
  expect(screen.queryByRole("region", { name: "工具调用过程" })).toBeNull();
});

it("sends from the composer button and disables duplicate sends while pending", async () => {
  let finish: (value: TurnResult) => void = () => {};
  mockBackend();
  invokeMock.mockImplementation((cmd: string) => ["get_pending_task_cycles","get_agent_actions"].includes(cmd) ? Promise.resolve([]) : cmd === commands.sendAgentMessage
    ? new Promise<TurnResult>((resolve) => { finish = resolve; }) : Promise.resolve(conversationView([])));
  renderPanel();
  await screen.findByText("开始规划");
  const button = screen.getByRole("button", { name: "发送消息" }) as HTMLButtonElement;
  expect(button.disabled).toBe(true);
  typeDraft("帮我规划");
  fireEvent.click(button);
  await screen.findByRole("status");
  expect(button.disabled).toBe(true);
  finish(turnResult());
  await waitFor(() => expect((screen.getByLabelText("给助理发消息") as HTMLTextAreaElement).value).toBe(""));
  expect(screen.queryByRole("status")).toBeNull();
});

it.each([true, false])("the first displayed import card is actionable after the turn completes (%s)", async (approve) => {
  let finish!: (value: TurnResult) => void;
  let pending = false;
  let done = false;
  const task = { id: "imported", cycle_id: "past-day", later_plan_type: null, parent_id: null,
    title: "整理记录", completed: false, subtasks: [], position: 0, proposal: "upsert",
    goal_breakdown: null, needs_refinement: null, needs_breakdown: null, root_color_key: null, copied_from_task_id: null, created_at: 1 };
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === commands.startAgentConversation) {
      // The verification read is slow. Completed IPC data must already release the buttons.
      if (done) return new Promise(() => {});
      return Promise.resolve(conversationView([], { active_turn_id: pending ? "import-turn" : null }));
    }
    if (cmd === commands.sendAgentMessage) return new Promise<TurnResult>((resolve) => { pending = true; finish = resolve; });
    if (cmd === "get_pending_task_cycles") return Promise.resolve(pending ? ["past-day"] : []);
    if (cmd === "get_agent_actions") return Promise.resolve([]);
    if (cmd === commands.getPlannerState) return Promise.resolve({ cycles: [{ id: "past-day", title: "2026/09/13", starts_on: "2026-09-13" }] });
    if (cmd === commands.getPreviewSummary) return Promise.resolve({ cycle_id: "past-day", count: 1, tasks: [task], originals: { imported: { ...task, original_exists: false } }, deletion_impacts: {} });
    return Promise.resolve(null);
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(<QueryClientProvider client={client}><AgentPanel cycleId="today" onClose={() => {}} /></QueryClientProvider>);
  await screen.findByText("开始规划");
  fireEvent.keyDown(typeDraft("2026/09/13 整理记录\n2026/09/14-2026/09/16 完成初稿"), { key: "Enter" });
  await waitFor(() => expect(pending).toBe(true));
  await act(async () => { await client.invalidateQueries(); });
  const button = await screen.findByRole("button", { name: `${approve ? "应用" : "放弃"}：整理记录` }) as HTMLButtonElement;
  expect(button.disabled).toBe(true);
  await act(async () => { done = true; finish(turnResult([modelTextMessage("已准备好，等待确认", 1)])); });
  await waitFor(() => expect(button.disabled).toBe(false));
  fireEvent.click(button);
  await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("resolve_coach_task_preview", { taskId: "imported", approve }));
});

it("keeps a new draft typed while the previous message is being processed", async () => {
  let finish!: (value: TurnResult) => void;
  mockBackend();
  const route = invokeMock.getMockImplementation()!;
  invokeMock.mockImplementation((cmd: string) => cmd === commands.sendAgentMessage
    ? new Promise<TurnResult>((resolve) => { finish = resolve; }) : route(cmd));
  renderPanel();
  await screen.findByText("开始规划");
  fireEvent.keyDown(typeDraft("导入第一份记录"), { key: "Enter" });
  await screen.findByRole("status");
  typeDraft("这是下一份记录");
  await act(async () => finish(turnResult()));
  expect((screen.getByLabelText("给助理发消息") as HTMLTextAreaElement).value).toBe("这是下一份记录");
});

it("preserves reading position on conversation refresh and offers an explicit return to latest", async () => {
  mockBackend({ conversation: conversationView([modelTextMessage("Initial reply", 1)]) });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={client}><AgentPanel cycleId="c1" onClose={() => {}} /></QueryClientProvider>);
  await screen.findByText("Initial reply");
  const messages = screen.getByTestId("coach-messages");
  Object.defineProperties(messages, { scrollHeight: { configurable: true, value: 1000 }, clientHeight: { configurable: true, value: 300 } });
  messages.scrollTop = 100;
  fireEvent.scroll(messages);
  expect(screen.getByRole("button", { name: /回到最新消息/ })).toBeDefined();
  mockBackend({ conversation: conversationView([modelTextMessage("Initial reply", 1), modelTextMessage("New reply", 2)], { revision: 2 }) });
  await client.invalidateQueries();
  await screen.findByText("New reply");
  expect(messages.scrollTop).toBe(100);
  const scrollTo = vi.fn();
  messages.scrollTo = scrollTo;
  const matchMedia = vi.spyOn(window, "matchMedia").mockReturnValue({ matches: true } as MediaQueryList);
  fireEvent.click(screen.getByRole("button", { name: /回到最新消息/ }));
  expect(scrollTo).toHaveBeenCalledWith({ top: 1000, behavior: "instant" });
  matchMedia.mockRestore();
});

function proposalResult(sequence: number): MessageView {
  return {id:`result-${sequence}`,turn_id:`turn-${sequence}`,sequence_number:sequence,message_type:"function_result",payload:{kind:"function_result",tool_call_id:`call-${sequence}`,name:"update_goal",result:{status:"proposed",task_id:"task"},is_error:false}};
}
function receiptMessage(sequence: number, decision: string): MessageView {
  return {id:`receipt-${sequence}`,turn_id:`decision-${sequence}`,sequence_number:sequence,message_type:"app_tool_result",payload:{kind:"app_tool_result",name:"approval_decision",result:{text:decision==="applied"?"已更新任务「Example」。":"已放弃改动「Example」。",target_kind:"task",target_id:"task",decision}}};
}
function structuredReceiptMessage(sequence: number): MessageView {
  return {id:`structured-receipt-${sequence}`,turn_id:`decision-${sequence}`,sequence_number:sequence,message_type:"app_tool_result",payload:{kind:"app_tool_result",name:"approval_decision",result:{text:"legacy receipt",result:{summary_key:"backend-actions:preview.updated",details:[{key:"backend-actions:preview.title",args:{title:"Example"}}],decision:"applied",operation:"task_preview"},target_kind:"task",target_id:"task",decision:"applied"}}};
}
it("resolves the original tool summary and leaves a later proposal on the same task pending",async()=>{
  mockBackend({conversation:conversationView([proposalResult(1),modelTextMessage("请确认修改",2),receiptMessage(3,"applied"),userMessage("再调整一次",4),proposalResult(5)])});
  renderPanel();
  expect(await screen.findByRole("button",{name:"1 项修改已确认，查看过程"})).toBeTruthy();
  expect(screen.getByRole("button",{name:"1 项修改待确认，查看过程"})).toBeTruthy();
  expect(within(screen.getByRole("status")).getByText("已更新任务「Example」。").closest("footer")).toBeNull();
  fireEvent.click(screen.getByRole("button",{name:"1 项修改已确认，查看过程"}));
  expect(within(screen.getByRole("region",{name:"工具调用过程"})).getByText("已更新任务「Example」。")).toBeTruthy();
  expect(screen.queryByText("改动已确认")).toBeNull();
});
it("shows rejected rather than pending after the user restores the preview",async()=>{
  mockBackend({conversation:conversationView([proposalResult(1),receiptMessage(2,"rejected")])});
  renderPanel();
  expect(await screen.findByRole("button",{name:"1 项修改已放弃，查看过程"})).toBeTruthy();
  expect(screen.queryByRole("button",{name:/修改待确认/})).toBeNull();
});
it("renders structured approval receipts in the active locale", async () => {
  mockBackend({conversation:conversationView([structuredReceiptMessage(1)])});
  renderPanel();
  const status = await screen.findByRole("status");
  expect(status.textContent).toBe("✓已更新任务「Example」。");
});

it("shows the persisted destination for an imported task even on a different page", async () => {
  const receipt = structuredReceiptMessage(1);
  if (receipt.payload.kind !== "app_tool_result") throw new Error("Expected receipt");
  const payload = receipt.payload.result as { result: { details: unknown[] } };
  payload.result.details.push({ key: "backend-actions:task.destination", args: { target: "2026-09-13" } });
  mockBackend({ conversation: conversationView([receipt]) });
  renderPanel({ cycleId: "different-cycle" });
  expect((await screen.findByRole("status")).textContent).toContain("目标：2026-09-13");
});
it("refreshes structured receipts on a locale switch but preserves historical text", async () => {
  mockBackend({conversation:conversationView([structuredReceiptMessage(1), receiptMessage(2, "applied")])});
  renderPanel();
  expect((await screen.findAllByRole("status"))[0]!.textContent).toBe("✓已更新任务「Example」。");
  applyLocale("en");
  await waitFor(() => expect(screen.getAllByRole("status")[0]!.textContent).toBe("✓Updated task: Example."));
  expect(screen.getAllByRole("status").some((status) => status.textContent?.includes("已更新任务「Example"))).toBe(true);
});

it("prepares an issue discussion as an editable draft and sends the exact task context only on request", async () => {
  mockBackend();
  renderPanel({initialDraft:"请核对 Prototype 的验证方式",focusedTaskId:"t1"});
  const input = await screen.findByLabelText("给助理发消息");
  expect((input as HTMLTextAreaElement).value).toBe("请核对 Prototype 的验证方式");
  expect(invokeMock.mock.calls.some(([cmd]) => cmd === commands.sendAgentMessage)).toBe(false);
  fireEvent.keyDown(input,{key:"Enter"});
  await waitFor(()=>expect(invokeMock).toHaveBeenCalledWith(commands.sendAgentMessage,{cycleId:"c1",text:"请核对 Prototype 的验证方式",focusedTaskId:"t1",pageContext:null}));
});

it("keeps the global transcript and draft while the selected page changes during a turn", async () => {
  const contextA = { view: "workspace" as const, long_term_cycle_id: "month-a", week_cycle_id: "week-a", day_cycle_id: "day-a", week_starts_on: "2026-09-14", selected_date: "2026-09-15" };
  const contextB = { view: "calendar" as const, long_term_cycle_id: null, week_cycle_id: "week-b", day_cycle_id: "day-b", week_starts_on: "2026-09-21", selected_date: "2026-09-22" };
  let finish!: (result: TurnResult) => void;
  const sends: Array<{ cycleId: string | null; text: string; focusedTaskId: string | null; pageContext: unknown }> = [];
  invokeMock.mockImplementation((cmd: string, args: { cycleId?: string | null; text?: string; focusedTaskId?: string | null; pageContext?: unknown }) => {
    if (cmd === commands.startAgentConversation) return Promise.resolve(conversationView([modelTextMessage("同一会话历史", 1)]));
    if (cmd === commands.sendAgentMessage) {
      sends.push({ cycleId: args.cycleId ?? null, text: args.text ?? "", focusedTaskId: args.focusedTaskId ?? null, pageContext: args.pageContext });
      return new Promise<TurnResult>((resolve) => { finish = resolve; });
    }
    if (cmd === "get_pending_task_cycles" || cmd === "get_agent_actions") return Promise.resolve([]);
    return Promise.resolve(null);
  });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const view = (cycleId: string | null, pageContext: typeof contextA | typeof contextB) => (
    <QueryClientProvider client={client}><AgentPanel cycleId={cycleId} pageContext={pageContext} onClose={() => {}} /></QueryClientProvider>
  );
  const mounted = render(view("day-a", contextA));
  expect(await screen.findByText("同一会话历史")).toBeDefined();
  const input = typeDraft("保留这段草稿");
  fireEvent.keyDown(input, { key: "Enter" });
  await screen.findByRole("status");

  mounted.rerender(view("day-b", contextB));
  expect((screen.getByLabelText("给助理发消息") as HTMLTextAreaElement).value).toBe("保留这段草稿");
  expect(screen.getByText("同一会话历史")).toBeDefined();
  expect(sends).toHaveLength(1);
  expect(sends[0]).toMatchObject({ cycleId: "day-a", text: "保留这段草稿", pageContext: contextA });

  finish(turnResult());
  await waitFor(() => expect((screen.getByLabelText("给助理发消息") as HTMLTextAreaElement).value).toBe(""));
  const nextInput = typeDraft("切换后继续");
  fireEvent.keyDown(nextInput, { key: "Enter" });
  await waitFor(() => expect(sends).toHaveLength(2));
  expect(sends[1]).toMatchObject({ cycleId: "day-b", text: "切换后继续", pageContext: contextB });
  finish(turnResult());
});

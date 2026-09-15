/**
 * AgentPanel 行为契约（openspec planner-workspace「Agent 侧栏外壳」
 * 「会话消息的角色区分」「候选回答由模型内联给出」）：挂载即取会话壳、
 * 消息按角色渲染、候选回答剥离正文并可点/快捷键发送、发送失败保留输入
 * 与历史、空会话给 Start planning 入口。invoke 按 src/lib/ipc.test.ts 的
 * 方式 mock——组件经由 lib/ipc 走真实包装，同时钉住线上的参数键。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

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

function conversationView(
  messages: MessageView[],
  over: Partial<ConversationView> = {},
): ConversationView {
  return {
    id: "conv-1",
    cycle_id: "c1",
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

function renderPanel(): { onClose: ReturnType<typeof vi.fn> } {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const onClose = vi.fn();
  render(
    <QueryClientProvider client={client}>
      <AgentPanel cycleId="c1" onClose={onClose} />
    </QueryClientProvider>,
  );
  return { onClose };
}

function typeDraft(text: string): HTMLTextAreaElement {
  const input = screen.getByLabelText("Message Coach") as HTMLTextAreaElement;
  fireEvent.change(input, { target: { value: text } });
  return input;
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("AgentPanel", () => {
  it("fetches the cycle's own conversation shell on mount", async () => {
    mockBackend({
      conversation: conversationView([userMessage("帮我把目标拆细", 1)]),
    });
    renderPanel();

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.startAgentConversation, {
        cycle_id: "c1",
      }),
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
        cycle_id: "c1",
        text: "先做周计划",
        focused_task_id: null,
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

    fireEvent.keyDown(screen.getByRole("complementary", { name: "Coach" }), {
      key: "1",
      metaKey: true,
    });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.sendAgentMessage, {
        cycle_id: "c1",
        text: "先做周计划",
        focused_task_id: null,
      }),
    );
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
    expect(await screen.findByText("Provider unreachable.")).toBeDefined();
    expect(input.value).toBe("帮我把目标拆细");
    expect(screen.getByText("好的，可以逐条来。")).toBeDefined();
    expect(invokeMock).toHaveBeenCalledWith(commands.sendAgentMessage, {
      cycle_id: "c1",
      text: "帮我把目标拆细",
      focused_task_id: null,
    });
  });

  it("points at Settings for provider-setup errors", async () => {
    mockBackend({
      sendError: { code: "no_active_provider", message: "no active AI provider" },
    });
    renderPanel();

    const input = await screen.findByLabelText("Message Coach");
    fireEvent.change(input, { target: { value: "你好" } });
    fireEvent.keyDown(input, { key: "Enter" });

    expect(await screen.findByText(/先在设置中添加并激活供应商/)).toBeDefined();
  });

  it("offers Start planning on an empty conversation", async () => {
    mockBackend();
    renderPanel();

    fireEvent.click(await screen.findByRole("button", { name: "Start planning" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.startPlanning, { cycle_id: "c1" }),
    );
  });

  it("closes the panel on Escape", async () => {
    mockBackend();
    const { onClose } = renderPanel();
    await screen.findByLabelText("Message Coach");

    fireEvent.keyDown(screen.getByLabelText("Message Coach"), { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
  });
});

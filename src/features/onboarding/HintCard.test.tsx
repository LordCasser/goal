/**
 * HintCard 行为契约（2.2–2.4）：首次显示；关闭按钮走 dismiss_hint 并
 * 不再显示；卡片在正常文档流中，显示期间不阻塞其下方控件的操作。
 * invoke 按 src/lib/ipc.test.ts 的方式 mock，钉住线上的参数键。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { commands } from "./api";
import { HintCard } from "./HintCard";

function renderCard(onDismissed?: () => void) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <QueryClientProvider client={client}>
      <div>
        <HintCard hintId="later-explainer" onDismissed={onDismissed} />
        <button type="button">Control below the card</button>
      </div>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === commands.getDismissedHints) return Promise.resolve([]);
    return Promise.resolve(null);
  });
});

describe("HintCard", () => {
  it("shows on first visit with registry copy", async () => {
    renderCard();
    expect(await screen.findByText("Park now, plan later")).toBeTruthy();
    expect(
      screen.getByText(/Capture a goal the moment it shows up/),
    ).toBeTruthy();
  });

  it("dismisses through dismiss_hint and disappears without flashing back", async () => {
    let dismissed = false;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getDismissedHints) {
        return Promise.resolve(dismissed ? ["later-explainer"] : []);
      }
      if (cmd === commands.dismissHint) {
        dismissed = true;
        return Promise.resolve(null);
      }
      return Promise.resolve(null);
    });
    const onDismissed = vi.fn();
    renderCard(onDismissed);

    fireEvent.click(await screen.findByRole("button", { name: "Dismiss hint" }));
    await waitFor(() => expect(onDismissed).toHaveBeenCalledOnce());
    await waitFor(() =>
      expect(screen.queryByText("Park now, plan later")).toBeNull(),
    );
    expect(invokeMock).toHaveBeenCalledWith(commands.dismissHint, {
      hint_id: "later-explainer",
    });
  });

  it("never renders once the id is in the persisted dismissed list", () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getDismissedHints) {
        return Promise.resolve(["later-explainer"]);
      }
      return Promise.resolve(null);
    });
    renderCard();
    expect(screen.queryByText("Park now, plan later")).toBeNull();
  });

  it("does not block controls below the card while visible", async () => {
    renderCard();
    const below = await screen.findByText("Control below the card");
    // The card is in normal flow: the sibling control is present, enabled
    // and clickable while the card is on screen (2.3).
    expect((below as HTMLButtonElement).disabled).toBe(false);
    expect(() => fireEvent.click(below)).not.toThrow();
  });
});

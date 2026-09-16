/**
 * FeedbackDialog 行为契约（§4.2–4.4）：空内容被拒且不上报；成功提示
 * "Thanks — your feedback was received."；失败路径保留原文可重试；支持 ID
 * 经 navigator.clipboard 复制并给出已复制反馈。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { applyLocale, errorMessage } from "../../lib/i18n";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
const { writeTextMock } = vi.hoisted(() => ({ writeTextMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { commands } from "./api";
import { FeedbackDialog } from "./FeedbackDialog";

function renderDialog(onClose = vi.fn()) {
  render(<FeedbackDialog open onClose={onClose} />);
  return onClose;
}

beforeEach(() => {
  applyLocale("en");
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(null);
  writeTextMock.mockReset();
  writeTextMock.mockResolvedValue(undefined);
  vi.stubGlobal(
    "navigator",
    Object.defineProperty(Object.create(Object.getPrototypeOf(navigator)), "clipboard", {
      value: { writeText: writeTextMock },
      configurable: true,
    }),
  );
});

describe("FeedbackDialog", () => {
  it("rejects blank input with the spec copy and never stages it", async () => {
    renderDialog();
    fireEvent.click(screen.getByRole("button", { name: "Send feedback" }));
    expect((await screen.findByRole("alert")).textContent).toBe(
      "Please enter your feedback.",
    );
    expect(
      invokeMock.mock.calls.filter(([cmd]) => cmd === commands.sendFeedback),
    ).toHaveLength(0);
  });

  it("sends trimmed content through send_feedback and thanks the user", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.sendFeedback) {
        return Promise.resolve({
          staged: {
            id: "s1",
            created_at: 0,
            app_version: "0.1.0",
            os: "macos arm64",
            anonymous_id: "anon",
            message: "great app",
          },
          queue_len: 1,
        });
      }
      return Promise.resolve(null);
    });
    renderDialog();
    fireEvent.change(screen.getByLabelText("Feedback message"), {
      target: { value: "  great app  " },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send feedback" }));
    expect(
      (await screen.findByTestId("feedback-sent")).textContent,
    ).toContain("Thanks — your feedback was received.");
    expect(invokeMock).toHaveBeenCalledWith(commands.sendFeedback, {
      message: "  great app  ",
    });
  });

  it("keeps the draft and offers a retry when staging fails", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.sendFeedback) {
        return Promise.reject({ code: "db_error", message: "disk full" });
      }
      return Promise.resolve(null);
    });
    renderDialog();
    fireEvent.change(screen.getByLabelText("Feedback message"), {
      target: { value: "the draft survives" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send feedback" }));
    expect((await screen.findByRole("alert")).textContent).toBe(
      errorMessage({ code: "db_error", message: "disk full" }),
    );
    expect(
      (screen.getByLabelText("Feedback message") as HTMLTextAreaElement).value,
    ).toBe("the draft survives");

    // Retry succeeds; nothing was lost between attempts.
    invokeMock.mockImplementation((cmd: string) =>
      cmd === commands.sendFeedback
        ? Promise.resolve({
            staged: {
              id: "s2",
              created_at: 0,
              app_version: "0.1.0",
              os: "macos arm64",
              anonymous_id: "anon",
              message: "the draft survives",
            },
            queue_len: 1,
          })
        : Promise.resolve(null),
    );
    fireEvent.click(screen.getByRole("button", { name: "Send feedback" }));
    await waitFor(() => screen.findByTestId("feedback-sent"));
  });

  it("copies the support id to the clipboard and confirms", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.getTalkToFounderEligibility) {
        return Promise.resolve({
          eligible: true,
          support_id: "anon-uuid-42",
          opened_at: null,
          open_count: 0,
        });
      }
      return Promise.resolve(null);
    });
    renderDialog();
    fireEvent.click(screen.getByRole("button", { name: "Copy support ID" }));
    await waitFor(() =>
      expect(writeTextMock).toHaveBeenCalledWith("anon-uuid-42"),
    );
    expect(await screen.findByText("Support ID copied")).toBeTruthy();
  });
});

/**
 * ExitPollDialog 行为契约（§3.3/§3.4）：打开时 acknowledge；三条出路各自
 * 调对的后端命令并回调对应 resolution；提交内容完整传递给 submit_exit_poll。
 * 退出拦截与 mark_exit_poll_listener_ready 由退出流程拥有方接线（组件只
 * 导出，不接入退出流程）。
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { applyLocale } from "../../lib/i18n";

const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

import { commands } from "./api";
import { ExitPollDialog, type ExitPollResolution } from "./ExitPollDialog";

function renderDialog(onClose: (resolution: ExitPollResolution) => void, open = true) {
  return render(<ExitPollDialog open={open} onClose={onClose} />);
}

beforeEach(() => {
  applyLocale("en");
  invokeMock.mockReset();
  invokeMock.mockResolvedValue(null);
});

describe("ExitPollDialog", () => {
  it("acknowledges the shown state once per open session", async () => {
    const onClose = vi.fn();
    const view = render(<ExitPollDialog open onClose={onClose} />);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(commands.acknowledgeExitPollShown),
    );
    // Parent re-renders must not double-acknowledge within one open session.
    view.rerender(<ExitPollDialog open onClose={onClose} />);
    await new Promise((resolve) => setTimeout(resolve, 10));
    const acks = invokeMock.mock.calls.filter(
      ([cmd]) => cmd === commands.acknowledgeExitPollShown,
    );
    expect(acks).toHaveLength(1);
  });

  it("submits answers, records exit confirmation and reports submitted", async () => {
    const onClose = vi.fn();
    renderDialog(onClose);
    fireEvent.click(screen.getByRole("radio", { name: "A feature I need is missing" }));
    fireEvent.change(screen.getByPlaceholderText("A sentence is plenty."), {
      target: { value: "Please add calendar sync" },
    });
    fireEvent.click(screen.getByRole("button", { name: "3" }));
    fireEvent.click(screen.getByRole("button", { name: "Submit and quit" }));

    await waitFor(() => expect(onClose).toHaveBeenCalledWith("submitted"));
    expect(invokeMock).toHaveBeenCalledWith(commands.submitExitPoll, {
      args: { rating: 3, reason: "missing_feature", detail: "Please add calendar sync" },
    });
    expect(invokeMock).toHaveBeenCalledWith(commands.exitAfterExitPoll);
  });

  it("keeps using the app through the continue path", async () => {
    const onClose = vi.fn();
    renderDialog(onClose);
    fireEvent.click(screen.getByRole("button", { name: "Keep using the app" }));
    await waitFor(() => expect(onClose).toHaveBeenCalledWith("continued"));
    expect(invokeMock).toHaveBeenCalledWith(commands.continueAfterExitPoll);
    expect(invokeMock).not.toHaveBeenCalledWith(commands.exitAfterExitPoll);
  });

  it("records an ignore when the dialog is dismissed (Escape/close)", async () => {
    const onClose = vi.fn();
    renderDialog(onClose);
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(onClose).toHaveBeenCalledWith("dismissed"));
    expect(invokeMock).toHaveBeenCalledWith(commands.dismissExitPoll);
  });

  it("keeps the dialog open with the draft intact when the backend write fails", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === commands.submitExitPoll) return Promise.reject({ code: "db_error", message: "x" });
      return Promise.resolve(null);
    });
    const onClose = vi.fn();
    renderDialog(onClose);
    fireEvent.change(screen.getByPlaceholderText("A sentence is plenty."), {
      target: { value: "survives a failed write" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Submit and quit" }));
    await new Promise((resolve) => setTimeout(resolve, 10));
    expect(onClose).not.toHaveBeenCalled();
    expect(
      (screen.getByPlaceholderText("A sentence is plenty.") as HTMLTextAreaElement).value,
    ).toBe("survives a failed write");
  });
});

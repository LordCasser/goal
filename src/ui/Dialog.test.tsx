import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { Dialog } from "./Dialog";

describe("Dialog", () => {
  it("closes on Escape", () => {
    const onClose = vi.fn();
    render(
      <Dialog open onClose={onClose} title="删除确认">
        正文
      </Dialog>,
    );
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("renders title and dialog role while open", () => {
    render(
      <Dialog open onClose={() => {}} title="删除确认">
        正文
      </Dialog>,
    );
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(screen.getByText("删除确认")).toBeTruthy();
  });

  it("renders nothing when closed", () => {
    render(
      <Dialog open={false} onClose={() => {}} title="删除确认">
        正文
      </Dialog>,
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("dialog interaction boundaries", () => {
  it("provides an explicit close button", () => {
    const onClose = vi.fn();
    render(<Dialog open title="Settings" onClose={onClose} />);
    fireEvent.click(screen.getByRole("button", { name: "Close dialog" }));
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("Escape closes only the uppermost dialog and respects IME composition", () => {
    const outerClose = vi.fn();
    const innerClose = vi.fn();
    render(<Dialog open title="Settings" onClose={outerClose}>
      <Dialog open title="Add model" onClose={innerClose} />
    </Dialog>);
    fireEvent.keyDown(document, { key: "Escape", isComposing: true });
    expect(innerClose).not.toHaveBeenCalled();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(innerClose).toHaveBeenCalledOnce();
    expect(outerClose).not.toHaveBeenCalled();
  });

  it("does not steal typing focus when an inline close callback changes", () => {
    const { rerender } = render(<Dialog open title="Settings" onClose={() => {}}><input aria-label="Provider name" /></Dialog>);
    const input = screen.getByRole("textbox");
    input.focus();
    rerender(<Dialog open title="Settings" onClose={() => {}}><input aria-label="Provider name" /></Dialog>);
    expect(document.activeElement).toBe(input);
  });
});

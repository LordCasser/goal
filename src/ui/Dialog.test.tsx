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

import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { Button } from "./Button";

describe("Button", () => {
  it("fires onClick when enabled", () => {
    const onClick = vi.fn();
    render(<Button onClick={onClick}>保存</Button>);
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    expect(onClick).toHaveBeenCalledOnce();
  });

  it("does not fire onClick while disabled", () => {
    const onClick = vi.fn();
    render(
      <Button disabled onClick={onClick}>
        保存
      </Button>,
    );
    const button = screen.getByRole("button", { name: "保存" }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    fireEvent.click(button);
    expect(onClick).not.toHaveBeenCalled();
  });

  it("does not fire onClick while loading", () => {
    const onClick = vi.fn();
    render(
      <Button loading onClick={onClick}>
        保存
      </Button>,
    );
    const button = screen.getByRole("button", { name: "保存" }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    fireEvent.click(button);
    expect(onClick).not.toHaveBeenCalled();
  });
});

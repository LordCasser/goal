import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { Checkbox } from "./Checkbox";

describe("Checkbox", () => {
  it("toggles checked and reports the next value", () => {
    const onChange = vi.fn();
    render(
      <Checkbox checked={false} onChange={onChange}>
        写周计划
      </Checkbox>,
    );
    fireEvent.click(screen.getByRole("checkbox", { name: "写周计划" }));
    expect(onChange).toHaveBeenCalledWith(true);
  });

  it("reports false when unchecking", () => {
    const onChange = vi.fn();
    render(
      <Checkbox checked onChange={onChange}>
        写周计划
      </Checkbox>,
    );
    const box = screen.getByRole("checkbox", { name: "写周计划" });
    expect(box.getAttribute("aria-checked")).toBe("true");
    fireEvent.click(box);
    expect(onChange).toHaveBeenCalledWith(false);
  });

  it("exposes mixed state via aria-checked", () => {
    render(
      <Checkbox checked={false} indeterminate onChange={() => {}}>
        部分完成
      </Checkbox>,
    );
    expect(screen.getByRole("checkbox").getAttribute("aria-checked")).toBe(
      "mixed",
    );
  });
});

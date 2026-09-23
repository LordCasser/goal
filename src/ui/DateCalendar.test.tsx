import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { applyLocale } from "../lib/i18n";
import { DateCalendar } from "./DateCalendar";

afterEach(() => applyLocale("en"));

describe("DateCalendar", () => {
  it("navigates months and selects a local date", () => {
    applyLocale("en");
    const onChange = vi.fn();
    render(<DateCalendar value="2026-09-23" onChange={onChange} />);
    fireEvent.click(screen.getByRole("button", { name: "Next month" }));
    fireEvent.click(document.querySelector('[data-picker-date="2026-10-21"]')!);
    expect(onChange).toHaveBeenCalledWith("2026-10-21");
  });

  it("disables dates outside bounds and retains keyboard movement", () => {
    applyLocale("en");
    const onChange = vi.fn();
    render(<DateCalendar value="2026-09-23" min="2026-09-22" max="2026-09-26" onChange={onChange} />);
    const before = document.querySelector<HTMLButtonElement>('[data-picker-date="2026-09-21"]')!;
    expect(before.disabled).toBe(true);
    fireEvent.click(before);
    expect(onChange).not.toHaveBeenCalled();
    const selected = document.querySelector<HTMLButtonElement>('[data-picker-date="2026-09-23"]')!;
    fireEvent.keyDown(selected, { key: "ArrowRight" });
    expect(document.querySelector('[data-picker-date="2026-09-24"]')?.getAttribute("tabindex")).toBe("0");
    fireEvent.click(document.querySelector('[data-picker-date="2026-09-26"]')!);
    expect(onChange).toHaveBeenCalledWith("2026-09-26");
  });

  it("uses localized month and weekday labels", () => {
    applyLocale("zh-CN");
    render(<DateCalendar value="2026-09-23" onChange={() => {}} />);
    expect(screen.getByRole("button", { name: "选择月份" }).textContent).toContain("2026");
    expect(screen.getByRole("button", { name: "上个月" })).toBeTruthy();
  });
});

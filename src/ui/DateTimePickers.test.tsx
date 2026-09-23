import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { applyLocale } from "../lib/i18n";
import { DatePicker } from "./DatePicker";
import { TimePicker } from "./TimePicker";

afterEach(() => applyLocale("en"));

describe("app date and time fields", () => {
  it("accepts direct date entry and chooses from a bounded calendar", () => {
    applyLocale("en");
    const onChange = vi.fn();
    render(<DatePicker aria-label="Start date" value="2026-09-23" min="2026-09-20" max="2026-09-28" onChange={onChange} />);
    fireEvent.change(screen.getByRole("textbox", { name: "Start date" }), { target: { value: "2026-09-25" } });
    expect(onChange).toHaveBeenCalledWith("2026-09-25");
    fireEvent.click(screen.getByRole("button", { name: "Start date: Open calendar" }));
    expect(document.activeElement?.getAttribute("data-picker-date")).toBe("2026-09-23");
    expect(document.querySelector<HTMLButtonElement>('[data-picker-date="2026-09-19"]')?.disabled).toBe(true);
    fireEvent.click(document.querySelector('[data-picker-date="2026-09-26"]')!);
    expect(onChange).toHaveBeenCalledWith("2026-09-26");
    expect(screen.queryByRole("dialog", { name: "Start date: Choose date" })).toBeNull();
  });

  it("closes the date popup on Escape without changing the value", () => {
    applyLocale("en");
    const onChange = vi.fn();
    render(<DatePicker aria-label="Start date" value="2026-09-23" onChange={onChange} />);
    const trigger = screen.getByRole("button", { name: "Start date: Open calendar" });
    fireEvent.click(trigger);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("dialog", { name: "Start date: Choose date" })).toBeNull();
    expect(onChange).not.toHaveBeenCalled();
  });

  it("accepts direct time entry and picks hour and minute", () => {
    applyLocale("en");
    const onChange = vi.fn();
    function Harness() {
      const [value, setValue] = useState("09:30");
      return <TimePicker aria-label="Start time" value={value} onChange={(next) => { setValue(next); onChange(next); }} />;
    }
    render(<Harness />);
    fireEvent.change(screen.getByRole("textbox", { name: "Start time" }), { target: { value: "10:45" } });
    expect(onChange).toHaveBeenCalledWith("10:45");
    fireEvent.click(screen.getByRole("button", { name: "Start time: Choose time" }));
    expect(document.activeElement?.getAttribute("data-time-hour")).toBe("10");
    fireEvent.click(document.querySelector('[data-time-hour="11"]')!);
    expect(onChange).toHaveBeenCalledWith("11:45");
    fireEvent.click(document.querySelector('[data-time-minute="30"]')!);
    expect(onChange).toHaveBeenCalledWith("11:30");
    expect((screen.getByRole("textbox", { name: "Start time" }) as HTMLInputElement).value).toBe("11:30");
  });
});

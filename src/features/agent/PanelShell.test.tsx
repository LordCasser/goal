import { useState } from "react";
import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { PanelShell } from "./PanelShell";
function Harness() {
  const [open, setOpen] = useState(false);
  return <><button onClick={() => setOpen(true)}>Open panel</button>{open && <PanelShell label="Test" title="Test" onClose={() => setOpen(false)}><input aria-label="Draft" /></PanelShell>}</>;
}
describe("panel focus", () => {
  it("opening from the toolbar places focus inside, Escape closes and restores the trigger", () => {
    render(<Harness />);
    const trigger = screen.getByRole("button", { name: "Open panel" });
    trigger.focus(); fireEvent.click(trigger);
    expect(document.activeElement).toBe(screen.getByRole("complementary"));
    fireEvent.keyDown(document.activeElement!, { key: "Escape" });
    expect(screen.queryByRole("complementary")).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });
  it("IME Escape does not close the panel", () => {
    render(<Harness />); fireEvent.click(screen.getByText("Open panel"));
    fireEvent.keyDown(screen.getByRole("complementary"), { key: "Escape", isComposing: true });
    expect(screen.getByRole("complementary")).toBeTruthy();
  });
});

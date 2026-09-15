import { useState } from "react";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Dialog } from "./Dialog";
import { Select, SelectItem } from "./Select";

function ExampleSelect() {
  const [value, setValue] = useState("short");
  return (
    <Select
      aria-label="Example choice"
      value={value}
      onValueChange={setValue}
      triggerClassName="w-40"
      placeholder="Choose"
    >
      <SelectItem value="short">Short</SelectItem>
      <SelectItem value="long">A model name that remains readable in the bounded list</SelectItem>
      <SelectItem value="disabled" disabled>Unavailable</SelectItem>
    </Select>
  );
}

function DialogSelect() {
  const [open, setOpen] = useState(true);
  return (
    <Dialog open={open} onClose={() => setOpen(false)} title="Dialog">
      <ExampleSelect />
    </Dialog>
  );
}

describe("Select", () => {
  it("opens a bounded list, selects by click, and keeps disabled items inert", async () => {
    render(<ExampleSelect />);
    const trigger = screen.getByRole("combobox", { name: "Example choice" });
    fireEvent.click(trigger);

    const content = await screen.findByRole("listbox");
    expect(content.className).toContain("min-w-[var(--radix-select-trigger-width)]");
    expect(content.getAttribute("style")).toContain("max-height");
    fireEvent.click(screen.getByRole("option", { name: /bounded list/ }));
    expect(trigger.textContent).toContain("A model name");

    fireEvent.click(trigger);
    const unavailable = await screen.findByRole("option", { name: "Unavailable" });
    expect(unavailable.getAttribute("aria-disabled")).toBe("true");
    fireEvent.click(unavailable);
    expect(trigger.textContent).toContain("A model name");
  });

  it("supports keyboard selection and returns focus after Escape", async () => {
    render(<ExampleSelect />);
    const trigger = screen.getByRole("combobox", { name: "Example choice" });
    trigger.focus();
    fireEvent.keyDown(trigger, { key: "ArrowDown" });
    await screen.findByRole("listbox");

    const focusedItem = document.activeElement;
    expect(focusedItem?.getAttribute("role")).toBe("option");
    fireEvent.keyDown(focusedItem!, { key: "ArrowDown" });
    await waitFor(() => expect(document.activeElement?.textContent).toContain("A model name"));
    fireEvent.keyDown(document.activeElement!, { key: "Enter" });
    expect(trigger.textContent).toContain("A model name");

    fireEvent.click(trigger);
    await screen.findByRole("listbox");
    fireEvent.keyDown(document.activeElement!, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("listbox")).toBeNull());
    expect(document.activeElement).toBe(trigger);
  });

  it("does not open or submit while an IME composition is active", async () => {
    render(<ExampleSelect />);
    const trigger = screen.getByRole("combobox", { name: "Example choice" });
    fireEvent.keyDown(trigger, { key: "Enter", keyCode: 229, isComposing: true });
    expect(screen.queryByRole("listbox")).toBeNull();

    fireEvent.click(trigger);
    const focusedItem = await screen.findByRole("option", { name: "Short" });
    fireEvent.keyDown(focusedItem, { key: "Enter", keyCode: 229, isComposing: true });
    expect(screen.getByRole("listbox")).toBeTruthy();
    expect(trigger.textContent).toContain("Short");
  });

  it("keeps both Select and Dialog open for a composing Escape", async () => {
    render(<DialogSelect />);
    const trigger = screen.getByRole("combobox", { name: "Example choice" });
    fireEvent.click(trigger);
    const focusedItem = await screen.findByRole("option", { name: "Short" });

    fireEvent.keyDown(focusedItem, {
      key: "Escape",
      keyCode: 229,
      isComposing: true,
    });

    expect(screen.getByRole("listbox")).toBeTruthy();
    expect(screen.getByRole("dialog", { hidden: true })).toBeTruthy();
  });

  it("lets the first Escape close only the list and the second close the dialog", async () => {
    render(<DialogSelect />);
    const trigger = screen.getByRole("combobox", { name: "Example choice" });
    fireEvent.click(trigger);
    await screen.findByRole("listbox");
    fireEvent.keyDown(document.activeElement!, { key: "Tab" });
    expect(screen.getByRole("listbox")).toBeTruthy();

    fireEvent.keyDown(document.activeElement!, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("listbox")).toBeNull());
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(document.activeElement).toBe(trigger);

    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});

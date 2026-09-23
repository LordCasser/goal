import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { AppContextMenu } from "./AppContextMenu";

afterEach(() => { cleanup(); vi.unstubAllGlobals(); });

function Editor() {
  const [value, setValue] = useState("alpha beta");
  return <><textarea aria-label="Draft" value={value} onChange={(event) => setValue(event.target.value)} />
    <AppContextMenu onOpenSettings={vi.fn()} /></>;
}

describe("application context menu", () => {
  it("suppresses the browser menu, keeps text selection, and edits controlled text", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    const readText = vi.fn().mockResolvedValue("new");
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText, readText } });
    render(<Editor />);
    const field = screen.getByRole("textbox", { name: "Draft" }) as HTMLTextAreaElement;
    field.focus();
    field.setSelectionRange(6, 10);
    const event = fireEvent.contextMenu(field, { clientX: 200, clientY: 100 });
    expect(event).toBe(false);
    fireEvent.click(screen.getByRole("menuitem", { name: "Copy" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("beta"));

    field.focus();
    field.setSelectionRange(6, 10);
    fireEvent.contextMenu(field, { clientX: 200, clientY: 100 });
    fireEvent.click(screen.getByRole("menuitem", { name: "Paste" }));
    await waitFor(() => expect(field.value).toBe("alpha new"));
    expect(readText).toHaveBeenCalledTimes(1);
  });

  it("opens from Shift+F10 and restores focus on Escape", async () => {
    render(<Editor />);
    const field = screen.getByRole("textbox", { name: "Draft" });
    field.focus();
    fireEvent.keyDown(field, { key: "F10", shiftKey: true });
    expect(screen.getByRole("menu", { name: "Context actions" })).toBeTruthy();
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(document.activeElement).toBe(field);
  });

  it("cuts the saved selection and supports arrow-key menu activation", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText } });
    const onOpenSettings = vi.fn();
    render(<><textarea aria-label="Draft" defaultValue="alpha beta" />
      <AppContextMenu onOpenSettings={onOpenSettings} /></>);
    const field = screen.getByRole("textbox", { name: "Draft" }) as HTMLTextAreaElement;
    field.focus();
    field.setSelectionRange(6, 10);
    fireEvent.contextMenu(field);
    fireEvent.click(screen.getByRole("menuitem", { name: "Cut" }));
    await waitFor(() => expect(field.value).toBe("alpha "));
    expect(writeText).toHaveBeenCalledWith("beta");

    field.focus();
    fireEvent.keyDown(field, { key: "ContextMenu" });
    const menu = screen.getByRole("menu");
    fireEvent.keyDown(menu, { key: "End" });
    const settings = screen.getByRole("menuitem", { name: "Settings" });
    expect(document.activeElement).toBe(settings);
    fireEvent.keyDown(settings, { key: "Enter" });
    expect(onOpenSettings).toHaveBeenCalledTimes(1);
  });

  it("clamps a pointer menu at the viewport edge", async () => {
    render(<Editor />);
    fireEvent.contextMenu(screen.getByRole("textbox", { name: "Draft" }), { clientX: window.innerWidth - 1, clientY: window.innerHeight - 1 });
    const menu = screen.getByRole("menu", { name: "Context actions" });
    await waitFor(() => expect(Number.parseFloat(menu.style.left)).toBeLessThan(window.innerWidth));
    expect(Number.parseFloat(menu.style.top)).toBeLessThan(window.innerHeight);
  });

  it("opens task details and leaves calendar cards to their own menu", async () => {
    const openDetail = vi.fn();
    const calendarMenu = vi.fn();
    render(<>
      <div data-task-detail onDoubleClick={openDetail}><button type="button">Task title</button></div>
      <div data-day-cell="2026-09-23" data-calendar-menu onContextMenu={calendarMenu}><button type="button">Date</button></div>
      <AppContextMenu onOpenSettings={vi.fn()} />
    </>);
    fireEvent.contextMenu(screen.getByRole("button", { name: "Task title" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Open details" }));
    await waitFor(() => expect(openDetail).toHaveBeenCalledTimes(1));
    fireEvent.contextMenu(screen.getByRole("button", { name: "Date" }));
    expect(calendarMenu).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("menuitem", { name: "Move day plan" })).toBeNull();
  });

  it("hides actions for empty rows and disables completion for locked rows", () => {
    render(<>
      <div data-task-detail data-task-empty><button type="button">Empty row</button></div>
      <div data-task-detail><button type="button" role="checkbox" disabled>Locked row</button></div>
      <AppContextMenu onOpenSettings={vi.fn()} />
    </>);
    fireEvent.contextMenu(screen.getByRole("button", { name: "Empty row" }));
    expect(screen.queryByRole("menuitem", { name: "Open details" })).toBeNull();
    fireEvent.contextMenu(screen.getByRole("checkbox", { name: "Locked row" }));
    expect(screen.getByRole("menuitem", { name: "Toggle complete" }).hasAttribute("disabled")).toBe(true);
  });
});

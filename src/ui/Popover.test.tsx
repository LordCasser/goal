import { useRef, useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { Popover, PopoverItem } from "./Popover";

function Harness({ onItemSelect }: { onItemSelect?: () => void }) {
  const anchor = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(true);
  return (
    <>
      <button ref={anchor} onClick={() => setOpen((v) => !v)}>
        更多
      </button>
      <Popover
        open={open}
        onClose={() => setOpen(false)}
        anchorRef={anchor}
        label="条目操作"
      >
        <PopoverItem onSelect={onItemSelect}>重命名</PopoverItem>
        <PopoverItem>移到稍后</PopoverItem>
      </Popover>
    </>
  );
}

describe("Popover", () => {
  it("renders menu items while open", () => {
    render(<Harness />);
    expect(screen.getByRole("menu")).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "重命名" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "移到稍后" })).toBeTruthy();
  });

  it("closes on Escape and restores focus to the anchor", () => {
    render(<Harness />);
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(screen.getByRole("button", { name: "更多" })).toBe(
      document.activeElement,
    );
  });

  it("closes when clicking outside and runs item actions", () => {
    const onItemSelect = vi.fn();
    render(<Harness onItemSelect={onItemSelect} />);
    fireEvent.click(screen.getByRole("menuitem", { name: "重命名" }));
    expect(onItemSelect).toHaveBeenCalledOnce();

    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("keeps menu navigation while allowing native input keys in a form popover", () => {
    const menu = render(<Harness />);
    fireEvent.keyDown(screen.getByRole("menuitem", { name: "重命名" }), { key: "ArrowDown" });
    expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: "移到稍后" }));
    menu.unmount();

    function FormPopover() {
      const anchor = useRef<HTMLButtonElement>(null);
      return <><button ref={anchor}>Edit schedule</button>
        <Popover open role="dialog" label="Schedule" anchorRef={anchor} onClose={() => {}}>
          <input aria-label="Time" type="time" defaultValue="09:00" />
          <input aria-label="Duration" type="number" defaultValue="25" />
        </Popover></>;
    }
    render(<FormPopover />);
    expect(screen.getByRole("dialog", { name: "Schedule" })).toBeTruthy();
    expect(document.activeElement).toBe(screen.getByLabelText("Time"));
    for (const field of ["Time", "Duration"]) {
      for (const key of ["ArrowUp", "ArrowDown", "Home", "End"]) {
        // true means the event was not canceled: native editing stays available.
        expect(fireEvent.keyDown(screen.getByLabelText(field), { key })).toBe(true);
      }
    }
  });
});

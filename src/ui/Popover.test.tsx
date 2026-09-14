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
        <PopoverItem>移到 Later</PopoverItem>
      </Popover>
    </>
  );
}

describe("Popover", () => {
  it("renders menu items while open", () => {
    render(<Harness />);
    expect(screen.getByRole("menu")).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "重命名" })).toBeTruthy();
    expect(screen.getByRole("menuitem", { name: "移到 Later" })).toBeTruthy();
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
});

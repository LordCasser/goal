import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ChatMarkdown } from "./ChatMarkdown";

describe("ChatMarkdown", () => {
  it("renders GFM tables inside a keyboard accessible horizontal scroll region", () => {
    render(<ChatMarkdown>{"| Task | Subtasks | Notes |\n| --- | --- | --- |\n| **Test** | — | No details captured |\n| Workspace content verified | — | Title only |"}</ChatMarkdown>);
    const region = screen.getByRole("region", { name: "AI 回复表格，可横向滚动" });
    expect(region.tabIndex).toBe(0);
    expect(within(region).getAllByRole("columnheader").map((el) => el.textContent)).toEqual(["Task", "Subtasks", "Notes"]);
    expect(within(region).getAllByRole("row")).toHaveLength(3);
    expect(within(region).getByText("Test").tagName).toBe("STRONG");
    expect(within(region).getAllByRole("cell")).toHaveLength(6);
  });

  it("updates incomplete Markdown to a complete reply without duplicating content", () => {
    const { rerender } = render(<ChatMarkdown isStreaming>{"**当前重点"}</ChatMarkdown>);
    expect(screen.getByText("当前重点").tagName).toBe("STRONG");
    rerender(<ChatMarkdown isStreaming>{"**当前重点**\n\n| Task | Status |\n| --- | --- |\n| Review |"}</ChatMarkdown>);
    expect(screen.getAllByRole("table")).toHaveLength(1);
    rerender(<ChatMarkdown>{"**当前重点**\n\n| Task | Status |\n| --- | --- |\n| Review | Ready |\n| Test | Open |\n\nNext step."}</ChatMarkdown>);
    expect(screen.getAllByRole("table")).toHaveLength(1);
    expect(screen.getAllByRole("row")).toHaveLength(3);
    expect(screen.getAllByText("当前重点")).toHaveLength(1);
    expect(screen.getByRole("cell", { name: "Ready" })).toBeDefined();
    expect(screen.getByText("Next step.")).toBeDefined();
  });

  it("supports task lists, strikethrough and literal fenced code without active HTML or media", () => {
    const { container } = render(<ChatMarkdown>{"- [x] Done\n- [ ] Review ~~old~~ plan\n\n```html\n<img src=\"example\">\n```\n\n<strong>raw HTML</strong>\n\n<iframe src=\"https://example.com\"></iframe>\n\n![Attachment](https://example.com/image.png)\n\n[unsafe](javascript:alert)"}</ChatMarkdown>);
    const boxes = screen.getAllByRole("checkbox") as HTMLInputElement[];
    expect(boxes.map((box) => [box.checked, box.disabled])).toEqual([[true, true], [false, true]]);
    expect(screen.getByText("old").tagName).toBe("DEL");
    expect(screen.getByLabelText("代码块，可横向滚动").textContent).toContain('<img src="example">');
    expect(container.querySelector("strong, img, iframe, script, a")).toBeNull();
    expect(container.innerHTML).not.toContain("javascript:");
    expect(screen.getByText("Attachment")).toBeDefined();
  });
});

import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { Button } from "./Button";
import { EmptyState } from "./EmptyState";

describe("EmptyState", () => {
  it("renders title text without any action button", () => {
    render(
      <EmptyState title="No cycles yet" description="先创建一个周期。" />,
    );
    expect(screen.getByText("No cycles yet")).toBeTruthy();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("renders the action and forwards its events", () => {
    const onCreate = vi.fn();
    render(
      <EmptyState
        title="No cycles yet"
        action={<Button variant="primary" onClick={onCreate}>创建周期</Button>}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "创建周期" }));
    expect(onCreate).toHaveBeenCalledOnce();
  });
});

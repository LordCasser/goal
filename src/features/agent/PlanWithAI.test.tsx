import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { Cycle } from "../../lib/ipc";
import { applyLocale } from "../../lib/i18n";
const backend = vi.hoisted(() => ({ getAiAvailability: vi.fn(), getAppFlag: vi.fn() }));
vi.mock("../../lib/ipc", () => backend);
import { AI_AVAILABILITY_KEY, PLAN_WITH_AI_KEY, PlanWithAI } from "./PlanWithAI";

beforeEach(() => { applyLocale("en"); vi.clearAllMocks(); backend.getAiAvailability.mockResolvedValue(true); backend.getAppFlag.mockResolvedValue(null); });
describe("contextual planning entry", () => {
  it.each(["month", "week", "day"] as const)("uses the exact %s cycle, and follows verification and preference changes", async (type) => {
    const cycle = { id: `${type}-target`, type, finished: false, archived: false } as Cycle;
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const onPlan = vi.fn();
    render(<QueryClientProvider client={client}><PlanWithAI cycle={cycle} active onPlan={onPlan} /></QueryClientProvider>);
    fireEvent.click(await screen.findByRole("button", { name: "Plan with AI" }));
    expect(onPlan).toHaveBeenCalledWith(cycle.id);
    client.setQueryData(PLAN_WITH_AI_KEY, "false");
    await waitFor(() => expect(screen.queryByRole("button", { name: "Plan with AI" })).toBeNull());
    client.setQueryData(AI_AVAILABILITY_KEY, false);
    client.setQueryData(PLAN_WITH_AI_KEY, "true");
    await waitFor(() => expect(screen.queryByRole("button", { name: "Plan with AI" })).toBeNull());
  });
  it("does not offer planning on finished or inactive surfaces", async () => {
    const cycle = { id: "done", type: "day", finished: true, archived: false } as Cycle;
    render(<QueryClientProvider client={new QueryClient()}><PlanWithAI cycle={cycle} active onPlan={vi.fn()} /><PlanWithAI cycle={{ ...cycle, finished: false }} active={false} onPlan={vi.fn()} /></QueryClientProvider>);
    await waitFor(() => expect(backend.getAiAvailability).toHaveBeenCalled());
    expect(screen.queryByRole("button", { name: "Plan with AI" })).toBeNull();
  });
});

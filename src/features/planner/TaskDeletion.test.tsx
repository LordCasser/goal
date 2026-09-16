import { beforeEach, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
const mocks = vi.hoisted(() => ({ getTaskDeletionPreview: vi.fn(), deleteTask: vi.fn() }));
vi.mock("../../lib/ipc", async (original) => ({ ...(await original<typeof import("../../lib/ipc")>()), ...mocks }));
import { useTaskDeletion } from "./TaskDeletion";

const target = { id: "goal", title: "Write a book", cycle_id: "month" };
function Fixture() {
  const deletion = useTaskDeletion();
  return <><button disabled={deletion.busy} onClick={() => void deletion.requestDelete(target)}>Delete</button>{deletion.dialog}{deletion.error && <p role="alert">{deletion.error}</p>}</>;
}
function mount() { render(<QueryClientProvider client={new QueryClient()}><Fixture /></QueryClientProvider>); }
beforeEach(() => {
  vi.resetAllMocks();
  mocks.getTaskDeletionPreview.mockResolvedValue({ task_id: target.id, descendant_tasks: 3, confirmation_token: "v1" });
  mocks.deleteTask.mockResolvedValue(undefined);
});

it("shows the complete descendant count and cancels without mutation", async () => {
  mount(); fireEvent.click(screen.getByText("Delete"));
  await screen.findByRole("dialog");
  expect(screen.getByText("This task and 3 linked descendants")).toBeTruthy();
  fireEvent.click(screen.getByText("Cancel"));
  expect(mocks.deleteTask).not.toHaveBeenCalled();
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("sends the preview token only after confirmation", async () => {
  mount(); fireEvent.click(screen.getByText("Delete"));
  fireEvent.click(await screen.findByRole("button", { name: "Delete task and descendants" }));
  await waitFor(() => expect(mocks.deleteTask).toHaveBeenCalledWith("goal", "v1"));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});

it("deletes a leaf after preview without an extra dialog", async () => {
  mocks.getTaskDeletionPreview.mockResolvedValue({ task_id: target.id, descendant_tasks: 0, confirmation_token: "leaf" });
  mount(); fireEvent.click(screen.getByText("Delete"));
  await waitFor(() => expect(mocks.deleteTask).toHaveBeenCalledWith("goal", "leaf"));
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("refreshes changed impact and requires a second explicit confirmation", async () => {
  mocks.deleteTask.mockRejectedValueOnce({ code: "deletion_impact_changed" });
  mocks.getTaskDeletionPreview.mockResolvedValueOnce({ task_id: target.id, descendant_tasks: 3, confirmation_token: "v1" })
    .mockResolvedValue({ task_id: target.id, descendant_tasks: 4, confirmation_token: "v2" });
  mount(); fireEvent.click(screen.getByText("Delete"));
  fireEvent.click(await screen.findByRole("button", { name: "Delete task and descendants" }));
  await screen.findByText("This task and 4 linked descendants");
  expect(mocks.deleteTask).toHaveBeenCalledTimes(1);
  expect((await screen.findByRole("alert")).textContent).toContain("Review the updated impact");
  fireEvent.click(screen.getByRole("button", { name: "Delete task and descendants" }));
  await waitFor(() => expect(mocks.deleteTask).toHaveBeenLastCalledWith("goal", "v2"));
});

it("fails closed when the impact cannot be read", async () => {
  mocks.getTaskDeletionPreview.mockRejectedValue({ code: "not_found", message: "Task was removed." });
  mount(); fireEvent.click(screen.getByText("Delete"));
  await screen.findByRole("alert");
  expect(mocks.deleteTask).not.toHaveBeenCalled();
});

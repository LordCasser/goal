import { useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { deleteTask, getTaskDeletionPreview, type TaskDeletionPreview } from "../../lib/ipc";
import { useTranslation } from "../../lib/i18n";
import { Button, Dialog } from "../../ui";
import { errorMessage, invalidateTasks } from "./actions";

type Target = { id: string; title: string; cycle_id: string };
type Pending = { target: Target; preview: TaskDeletionPreview };

/** Both planner and Later use the backend's complete cross-cycle impact. */
export function useTaskDeletion() {
  const { t } = useTranslation("planning");
  const qc = useQueryClient();
  const busyRef = useRef(false);
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState<Pending | null>(null);
  const [error, setError] = useState<unknown>(null);

  const remove = async (target: Target, preview: TaskDeletionPreview) => {
    try {
      await deleteTask(target.id, preview.confirmation_token);
      invalidateTasks(qc, target.cycle_id);
      setPending(null);
    } catch (failure) {
      // No automatic retry: a changed impact must be seen and confirmed again.
      if (typeof failure === "object" && failure !== null && "code" in failure && failure.code === "deletion_impact_changed") {
        const fresh = await getTaskDeletionPreview(target.id);
        setPending({ target, preview: fresh });
      }
      throw failure;
    }
  };

  const execute = async (action: () => Promise<void>) => {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    setError(null);
    try { await action(); }
    catch (failure) { setError(failure); }
    finally { busyRef.current = false; setBusy(false); }
  };

  const requestDelete = (target: Target) => execute(async () => {
    const preview = await getTaskDeletionPreview(target.id);
    if (preview.descendant_tasks > 0 || preview.total_focus_blocks > 0) setPending({ target, preview });
    else await remove(target, preview);
  });

  const close = () => { if (!busyRef.current) { setPending(null); setError(null); } };
  const dialog = pending && <Dialog open onClose={close}
    title={t("cycle.deleteTitle", { title: pending.target.title || t("task.untitled") })}
    footer={<>
      <Button onClick={close} disabled={busy} autoFocus>{t("cycle.cancel")}</Button>
      <Button variant="primary" loading={busy} style={{ backgroundColor: "var(--color-danger)" }}
        onClick={() => void execute(() => remove(pending.target, pending.preview))}>{t("task.confirmDelete")}</Button>
    </>}>
    <p className="text-body text-primary">{t("task.deleteCascadeHelp")}</p>
    <div className="mt-3 rounded-md bg-subtle px-3 py-2 text-body font-medium text-primary">
      {pending.preview.descendant_tasks > 0 && (
        <p>{t("task.deleteDescendants", { count: pending.preview.descendant_tasks })}</p>
      )}
      {pending.preview.total_focus_blocks > 0 && (
        <p className={pending.preview.descendant_tasks > 0 ? "mt-1" : undefined}>
          {t("task.deleteFocusBlocks", { count: pending.preview.total_focus_blocks })}
        </p>
      )}
      {pending.preview.started_focus_count > 0 && (
        <p className="mt-1 text-caption font-normal text-secondary">
          {t("task.deleteStartedFocus", { count: pending.preview.started_focus_count })}
        </p>
      )}
    </div>
    <p className="mt-3 text-caption text-secondary">{t("task.deleteCascadeNote")}</p>
    {error != null && <p role="alert" className="mt-3 text-caption text-danger">{errorMessage(error)}</p>}
  </Dialog>;

  return { requestDelete, busy, dialog, error: pending || error == null ? null : errorMessage(error), dismiss: () => setError(null) };
}

import { useState, type KeyboardEvent } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Button, Dialog } from "../../ui";
import { invalidateAgentEffects, qk } from "../../lib/events";
import { deleteTrashEntries, listTrash, restoreTrashEntry } from "../../lib/ipc";
import { useTranslation } from "../../lib/i18n";
import { errorMessage } from "../planner/actions";

/** The deletion action is one restorable unit, including its dependent rows. */
export function TrashPanel({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation("planning");
  const queryClient = useQueryClient();
  const { data = [], isPending, error: loadError } = useQuery({ queryKey: qk.trash(), queryFn: listTrash });
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const selectedIds = data.filter((entry) => selected.has(entry.id)).map((entry) => entry.id);
  const allSelected = data.length > 0 && selectedIds.length === data.length;

  const toggle = (id: string) => setSelected((old) => {
    const next = new Set(old);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    return next;
  });

  const restore = async (id: string) => {
    if (busy) return;
    setBusy(true);
    setBusyId(id);
    setActionError(null);
    try {
      await restoreTrashEntry(id);
      setSelected((old) => { const next = new Set(old); next.delete(id); return next; });
      invalidateAgentEffects(queryClient);
    } catch (error) { setActionError(errorMessage(error)); }
    finally { setBusy(false); setBusyId(null); }
  };

  const remove = async () => {
    if (busy || selectedIds.length === 0) return;
    setBusy(true);
    setActionError(null);
    try {
      await deleteTrashEntries(selectedIds);
      setSelected(new Set());
      setConfirmOpen(false);
      void queryClient.invalidateQueries({ queryKey: qk.trash() });
    } catch (error) { setActionError(errorMessage(error)); }
    finally { setBusy(false); }
  };

  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key !== "Escape" || event.nativeEvent.isComposing || event.defaultPrevented || confirmOpen) return;
    event.preventDefault();
    event.stopPropagation();
    onClose();
  };

  return <aside aria-label={t("trash.title")} onKeyDown={onKeyDown}
    className="flex h-full w-panel shrink-0 flex-col border-r border-light bg-content">
    <header className="flex items-center justify-between px-4 pb-3 pt-4">
      <h2 className="text-caption font-semibold uppercase tracking-[0.08em] text-secondary">{t("trash.title")}</h2>
      <Button variant="ghost" size="icon" onClick={onClose} aria-label={t("trash.close")} title={t("later.closeEsc")}>
        <svg viewBox="0 0 16 16" className="h-4 w-4" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><path d="m3 3 10 10M13 3 3 13" /></svg>
      </Button>
    </header>
    {data.length > 0 && <div className="flex items-center justify-between border-y border-light px-4 py-2">
      <label className="flex items-center gap-2 text-caption text-secondary">
        <input type="checkbox" checked={allSelected} disabled={busy}
          onChange={() => setSelected(allSelected ? new Set() : new Set(data.map((entry) => entry.id)))} />
        {t("trash.selectAll")}
      </label>
      <Button variant="ghost" disabled={selectedIds.length === 0 || busy} onClick={() => { setActionError(null); setConfirmOpen(true); }}>
        {t("trash.deleteSelected", { count: selectedIds.length })}
      </Button>
    </div>}
    {loadError && <p role="alert" className="px-4 text-body text-danger">{errorMessage(loadError)}</p>}
    {actionError && !confirmOpen && <p role="alert" className="px-4 pb-2 text-body text-danger">{actionError}</p>}
    <div className="min-h-0 flex-1 overflow-y-auto px-4 pb-4">
      {isPending ? <p className="pt-2 text-body text-hint">{t("trash.loading")}</p>
        : data.length === 0 ? <p className="pt-2 text-body text-hint">{t("trash.empty")}</p>
        : <ul className="divide-y divide-light">
          {data.map((entry) => <li key={entry.id} className="flex items-start gap-2 py-3">
            <input type="checkbox" className="mt-1.5 shrink-0" checked={selected.has(entry.id)} disabled={busy}
              aria-label={t("trash.selectItem", { title: entry.title })} onChange={() => toggle(entry.id)} />
            <div className="min-w-0 flex-1">
              <p className="truncate text-body font-medium text-primary" title={entry.title}>{entry.title || t("task.untitled")}</p>
              <p className="mt-0.5 truncate text-caption text-secondary" title={entry.origin}>
                {t(entry.kind === "cycle" ? "trash.planOrigin" : "trash.goalOrigin", { origin: entry.origin })}
              </p>
              <p className="mt-0.5 text-caption text-hint">{new Date(entry.deleted_at).toLocaleString()}</p>
            </div>
            <Button variant="ghost" disabled={busy} onClick={() => void restore(entry.id)}>
              {busyId === entry.id ? t("trash.restoring") : t("trash.restore")}
            </Button>
          </li>)}
        </ul>}
    </div>
    {confirmOpen && <Dialog open onClose={() => { if (!busy) setConfirmOpen(false); }}
      title={t("trash.confirmTitle", { count: selectedIds.length })}
      footer={<>
        <Button onClick={() => setConfirmOpen(false)} disabled={busy} autoFocus>{t("cycle.cancel")}</Button>
        <Button variant="primary" loading={busy} disabled={selectedIds.length === 0}
          style={{ backgroundColor: "var(--color-danger)" }} onClick={() => void remove()}>{t("trash.confirmDelete")}</Button>
      </>}>
      <p className="text-body text-primary">{t("trash.confirmDescription", { count: selectedIds.length })}</p>
      {actionError && <p role="alert" className="mt-3 text-body text-danger">{actionError}</p>}
    </Dialog>}
  </aside>;
}

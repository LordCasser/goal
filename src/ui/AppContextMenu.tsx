import { useEffect, useRef, useState } from "react";
import { useTranslation } from "../lib/i18n";
import { Popover, PopoverItem } from "./Popover";

type TextField = HTMLInputElement | HTMLTextAreaElement;
type MenuTarget = {
  element: HTMLElement;
  point: { x: number; y: number };
  field: TextField | null;
  selection: { start: number; end: number } | null;
  selectedText: string;
  taskRow: HTMLElement | null;
  scheduleEdit: HTMLButtonElement | null;
};

function textField(element: HTMLElement): TextField | null {
  const candidate = element.closest("input, textarea");
  if (candidate instanceof HTMLTextAreaElement) return candidate;
  if (candidate instanceof HTMLInputElement && ["text", "search", "url", "email", "tel", "password"].includes(candidate.type)) return candidate;
  return null;
}

/** Global webview menu. The original target and selection survive menu focus. */
export function AppContextMenu({ onOpenSettings }: { onOpenSettings: () => void }) {
  const { t } = useTranslation("shell");
  const [menu, setMenu] = useState<MenuTarget | null>(null);
  const [error, setError] = useState(false);
  const anchorRef = useRef<HTMLElement | null>(null);
  anchorRef.current = menu?.element ?? null;

  useEffect(() => {
    const open = (element: HTMLElement, x: number, y: number) => {
      const field = textField(element);
      const selection = field ? { start: field.selectionStart ?? 0, end: field.selectionEnd ?? 0 } : null;
      const selectedText = field && selection
        ? field.value.slice(selection.start, selection.end)
        : window.getSelection()?.toString() ?? "";
      setError(false);
      setMenu({
        element, point: { x, y }, field, selection, selectedText,
        taskRow: (() => {
          const row = element.closest<HTMLElement>("[data-task-detail]");
          return row?.hasAttribute("data-task-empty") ? null : row;
        })(),
        scheduleEdit: element.closest<HTMLElement>("[data-focus-block]")?.querySelector<HTMLButtonElement>("[data-edit-schedule]") ?? null,
      });
    };
    const onContextMenu = (event: MouseEvent) => {
      event.preventDefault();
      const target = event.target;
      const element = target instanceof HTMLElement ? target : target instanceof Element ? target.closest<HTMLElement>("button, [data-task-detail], [data-day-cell], [data-focus-block], main, [role='dialog']") : null;
      if (element?.closest("[data-calendar-menu]")) return;
      if (element) open(element, event.clientX, event.clientY);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey)) || event.isComposing) return;
      const target = document.activeElement;
      if (!(target instanceof HTMLElement)) return;
      if (target.closest("[data-calendar-menu]")) return;
      event.preventDefault();
      const rect = target.getBoundingClientRect();
      open(target, rect.left + Math.min(rect.width, 20), rect.bottom);
    };
    document.addEventListener("contextmenu", onContextMenu, true);
    document.addEventListener("keydown", onKeyDown, true);
    return () => {
      document.removeEventListener("contextmenu", onContextMenu, true);
      document.removeEventListener("keydown", onKeyDown, true);
    };
  }, []);

  const close = () => setMenu(null);
  const restoreSelection = () => {
    if (!menu?.field || !menu.selection) return;
    menu.field.focus({ preventScroll: true });
    menu.field.setSelectionRange(menu.selection.start, menu.selection.end);
  };
  const replaceSelection = (replacement: string) => {
    if (!menu?.field || !menu.selection || menu.field.readOnly || menu.field.disabled) return;
    restoreSelection();
    menu.field.setRangeText(replacement, menu.selection.start, menu.selection.end, "end");
    menu.field.dispatchEvent(new Event("input", { bubbles: true }));
  };
  const copy = async (cut: boolean) => {
    if (!menu?.selectedText) return;
    try {
      const clipboard: Clipboard | undefined = navigator.clipboard;
      if (clipboard) await clipboard.writeText(menu.selectedText);
      else { restoreSelection(); if (!document.execCommand(cut ? "cut" : "copy")) throw new Error("clipboard"); }
      if (cut && clipboard) replaceSelection("");
      close();
    } catch { setError(true); }
  };
  const paste = async () => {
    try {
      const clipboard: Clipboard | undefined = navigator.clipboard;
      if (clipboard) replaceSelection(await clipboard.readText());
      else { restoreSelection(); if (!document.execCommand("paste")) throw new Error("clipboard"); }
      close();
    } catch { setError(true); }
  };
  const activate = (element: HTMLElement | null) => {
    close();
    if (element) queueMicrotask(() => element.click());
  };

  const canEdit = !!menu?.field && !menu.field.readOnly && !menu.field.disabled;
  return <Popover open={menu !== null} onClose={close} anchorRef={anchorRef} point={menu?.point} label={t("context.title")}>
    {menu?.field && <>
      <PopoverItem disabled={!canEdit || !menu.selectedText} onSelect={() => { void copy(true); }}>{t("context.cut")}</PopoverItem>
      <PopoverItem disabled={!menu.selectedText} onSelect={() => { void copy(false); }}>{t("context.copy")}</PopoverItem>
      <PopoverItem disabled={!canEdit} onSelect={() => { void paste(); }}>{t("context.paste")}</PopoverItem>
      <PopoverItem onSelect={() => { restoreSelection(); menu.field?.select(); close(); }}>{t("context.selectAll")}</PopoverItem>
    </>}
    {!menu?.field && !!menu?.selectedText && <PopoverItem onSelect={() => { void copy(false); }}>{t("context.copy")}</PopoverItem>}
    {menu?.taskRow && <>
      <PopoverItem className="border-t border-light" onSelect={() => {
        const row = menu.taskRow;
        close();
        if (row) queueMicrotask(() => row.dispatchEvent(new MouseEvent("dblclick", { bubbles: true })));
      }}>{t("context.details")}</PopoverItem>
      <PopoverItem disabled={(() => {
        const checkbox = menu.taskRow?.querySelector<HTMLElement>('[role="checkbox"]');
        return !checkbox || checkbox.hasAttribute("disabled") || checkbox.getAttribute("aria-disabled") === "true";
      })()}
        onSelect={() => activate(menu.taskRow?.querySelector<HTMLElement>('[role="checkbox"]') ?? null)}>{t("context.toggleComplete")}</PopoverItem>
      {menu.taskRow.querySelector("[data-parent-picker]") && <PopoverItem onSelect={() => activate(menu.taskRow?.querySelector<HTMLElement>("[data-parent-picker]") ?? null)}>{t("context.parentGoal")}</PopoverItem>}
    </>}
    {menu?.scheduleEdit && <PopoverItem onSelect={() => activate(menu.scheduleEdit)}>{t("context.editSchedule")}</PopoverItem>}
    <PopoverItem className="border-t border-light" onSelect={() => { close(); onOpenSettings(); }}>{t("desktop.settings")}</PopoverItem>
    {error && <p role="alert" className="px-3 py-1 text-caption text-danger">{t("context.clipboardError")}</p>}
  </Popover>;
}

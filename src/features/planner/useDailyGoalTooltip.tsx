import { useCallback, useEffect, useId, useLayoutEffect, useRef, useState, type CSSProperties, type HTMLAttributes, type RefObject } from "react";
import { createPortal } from "react-dom";
import type { TaskNode } from "../../lib/ipc";
import { useTranslation } from "../../lib/i18n";
import { dailyGoal, taskColor, type RelationView } from "./relations";

/** Read-only context belongs beside the daily row, even with its goal offscreen. */
export function useDailyGoalTooltip(task: TaskNode, relations: RelationView | undefined, active: boolean, anchorRef: RefObject<HTMLElement | null>) {
  const id = useId();
  const target = relations ? dailyGoal(task, relations.tasks, relations.cycles) : null;
  const enabled = active && !!task.title.trim() && !task.proposal && target !== null;
  const [phase, setPhase] = useState<"closed" | "waiting" | "open" | "closing">("closed");
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const panelRef = useRef<HTMLDivElement>(null);
  const pointer = useRef(false);
  const focused = useRef(false);
  const dismissed = useRef(false);
  const cancel = useCallback(() => { clearTimeout(timer.current); }, []);
  const dismiss = useCallback(() => {
    cancel();
    dismissed.current = true;
    setPhase("closed");
  }, [cancel]);
  const show = () => {
    if (!enabled || dismissed.current || document.querySelector('[role="menu"], [role="dialog"]')
      || anchorRef.current?.closest('[inert], [hidden], [data-removal-state="exiting"]')) return;
    cancel();
    if (phase === "open" || phase === "closing") { setPhase("open"); return; }
    setPhase("waiting");
    timer.current = setTimeout(() => setPhase("open"), 360);
  };
  const leave = () => {
    if (focused.current && !dismissed.current) return;
    cancel();
    if (phase !== "open") { setPhase("closed"); return; }
    // Allow crossing the small gap into the tooltip to read long goal names.
    timer.current = setTimeout(() => {
      setPhase("closing");
      timer.current = setTimeout(() => setPhase("closed"), 120);
    }, 140);
  };
  useEffect(() => { dismiss(); return cancel; }, [enabled, task.id, target?.goal.id, dismiss, cancel]);
  const listening = phase !== "closed";
  useEffect(() => {
    if (!listening) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Tab") dismiss(); // No focus stealing or consuming editor shortcuts.
    };
    const onScroll = (event: Event) => {
      if (event.target instanceof Node && panelRef.current?.contains(event.target)) return;
      dismiss();
    };
    const onPointerDown = (event: Event) => {
      if (event.target instanceof Node && panelRef.current?.contains(event.target)) return;
      dismiss();
    };
    const onFocus = (event: FocusEvent) => {
      if (event.target instanceof Node && !anchorRef.current?.contains(event.target) && !panelRef.current?.contains(event.target)) dismiss();
    };
    const onMouseOver = (event: MouseEvent) => {
      const row = event.target instanceof Element ? event.target.closest("[data-task-id]") : null;
      if (row && row !== anchorRef.current) dismiss();
    };
    document.addEventListener("keydown", onKey, true);
    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("focusin", onFocus, true);
    document.addEventListener("mouseover", onMouseOver, true);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", dismiss);
    window.addEventListener("blur", dismiss);
    const observer = new MutationObserver(() => {
      if (anchorRef.current?.closest('[inert], [hidden], [data-removal-state="exiting"]')) dismiss();
    });
    for (let node = anchorRef.current; node; node = node.parentElement) {
      observer.observe(node, { attributes: true, attributeFilter: ["inert", "hidden", "data-removal-state"] });
    }
    return () => {
      document.removeEventListener("keydown", onKey, true);
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("focusin", onFocus, true);
      document.removeEventListener("mouseover", onMouseOver, true);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", dismiss);
      window.removeEventListener("blur", dismiss);
      observer.disconnect();
    };
  }, [listening, anchorRef, dismiss]);

  const rowProps: HTMLAttributes<HTMLDivElement> = enabled ? {
    onMouseEnter: () => { pointer.current = true; dismissed.current = false; show(); },
    onMouseLeave: () => { pointer.current = false; leave(); },
    onPointerDownCapture: (event) => {
      if (!panelRef.current?.contains(event.target as Node)) dismiss();
    },
    onDragStartCapture: dismiss,
    onFocusCapture: (event) => {
      if (event.relatedTarget instanceof Node && event.currentTarget.contains(event.relatedTarget)) return;
      focused.current = true;
      // Pointer-down already dismissed the hint; keyboard focus can reveal it.
      if (!pointer.current) dismissed.current = false;
      show();
    },
    onBlurCapture: (event) => {
      if (event.relatedTarget instanceof Node && event.currentTarget.contains(event.relatedTarget)) return;
      focused.current = false;
      if (!pointer.current) leave();
    },
  } : {};
  const visible = enabled && (phase === "open" || phase === "closing");
  return {
    rowProps,
    descriptionId: visible ? id : undefined,
    tooltip: visible && target && relations ? <DailyGoalCard id={id} anchorRef={anchorRef} panelRef={panelRef}
      goal={target.goal} via={target.via} color={taskColor(target.goal, relations.tasks)} closing={phase === "closing"}
      onMouseEnter={() => { cancel(); setPhase("open"); }} onMouseLeave={leave} /> : null,
  };
}

function DailyGoalCard({ id, anchorRef, panelRef, goal, via, color, closing, onMouseEnter, onMouseLeave }: {
  id: string; anchorRef: RefObject<HTMLElement | null>; panelRef: RefObject<HTMLDivElement | null>;
  goal: TaskNode; via: TaskNode | null; color: string | null; closing: boolean;
  onMouseEnter: () => void; onMouseLeave: () => void;
}) {
  const { t } = useTranslation("planning");
  const [position, setPosition] = useState({ left: 0, top: 0, above: false });
  useLayoutEffect(() => {
    const anchor = anchorRef.current;
    const panel = panelRef.current;
    if (!anchor || !panel) return;
    const update = () => {
      const rect = anchor.getBoundingClientRect();
      const title = anchor.querySelector("textarea")?.getBoundingClientRect() ?? rect;
      const { width, height } = panel.getBoundingClientRect();
      const above = window.innerHeight - rect.bottom < height + 18 && rect.top > height + 18;
      setPosition({
        left: Math.max(10, Math.min(title.left, window.innerWidth - width - 10)),
        top: Math.max(10, Math.min(above ? rect.top - height - 8 : rect.bottom + 8, window.innerHeight - height - 10)),
        above,
      });
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(panel);
    observer.observe(anchor);
    return () => observer.disconnect();
  }, [anchorRef, panelRef]);
  return createPortal(<div ref={panelRef} id={id} role="tooltip"
    onMouseEnter={onMouseEnter} onMouseLeave={onMouseLeave}
    onClick={(event) => event.stopPropagation()}
    data-closing={closing || undefined} data-above={position.above || undefined}
    className="daily-goal-tooltip fixed z-[90] w-[288px] max-w-[calc(100vw-20px)] overflow-y-auto rounded-lg border border-light bg-content p-3.5"
    style={{ left: position.left, top: position.top, maxHeight: "min(320px, calc(100dvh - 20px))", "--goal-color": color ?? "var(--border-control)" } as CSSProperties}>
    <p className="mb-2 flex items-center gap-1.5 text-caption text-secondary">
      <svg aria-hidden="true" width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round"><circle cx="8" cy="8" r="5.5"/><circle cx="8" cy="8" r="2"/></svg>
      {t("goalHint.label")}
    </p>
    <div className="flex items-start gap-2.5">
      <span aria-hidden="true" className="mt-1 h-3.5 w-1.5 shrink-0 rounded-[3px] bg-[var(--goal-color)]" />
      <p className="min-w-0 whitespace-pre-wrap break-words text-body font-medium text-primary [overflow-wrap:anywhere]">{goal.title}</p>
    </div>
    {via && <div className="mt-3 border-t border-light pt-2.5 text-caption text-secondary">
      <span className="mb-1 block">{t("goalHint.via")}</span>
      <p className="whitespace-pre-wrap break-words [overflow-wrap:anywhere]">{via.title}</p>
    </div>}
  </div>, document.body);
}

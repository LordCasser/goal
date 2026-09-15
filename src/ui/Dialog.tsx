import {
  useEffect,
  useId,
  useRef,
  useContext,
  createContext,
  type ReactNode,
  type RefObject,
} from "react";
import { cn } from "./cn";
import { createPortal } from "react-dom";

// Only the uppermost dialog owns Escape and Tab; provider/model dialogs may nest.
const DialogDepth = createContext(0);
const dialogStack: { token: symbol; depth: number }[] = [];

export type DialogProps = {
  open: boolean;
  onClose: () => void;
  title?: ReactNode;
  children?: ReactNode;
  /** Right-aligned action area; usually Buttons. */
  footer?: ReactNode;
  /** 约 640px 宽（周期选择类），默认约 420px（design.md 3.3）。 */
  wide?: boolean;
  /** 关闭后焦点还给该元素；不传则还给打开前的焦点元素（3.3）。 */
  returnFocusTo?: RefObject<HTMLElement | null>;
  className?: string;
  bodyClassName?: string;
};

const FOCUSABLE = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  '[tabindex]:not([tabindex="-1"])',
].join(", ");

/**
 * Modal dialog. Opens with a 160–220ms enter animation and releases
 * immediately on close (no exit animation, design.md 10). Escape closes;
 * Tab is trapped inside the panel.
 */
export function Dialog({
  open,
  onClose,
  title,
  children,
  footer,
  wide = false,
  returnFocusTo,
  className,
  bodyClassName,
}: DialogProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const restoreRef = useRef<HTMLElement | null>(null);
  const titleId = useId();
  const token = useRef(Symbol("dialog"));
  const depth = useContext(DialogDepth);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  useEffect(() => {
    if (!open) return;
    const previous =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    restoreRef.current = previous;
    const id = token.current;
    dialogStack.push({ token: id, depth });
    dialogStack.sort((a, b) => a.depth - b.depth);
    if (dialogStack.at(-1)?.token === id) panelRef.current?.focus({ preventScroll: true });

    // Escape 挂在 document 上：焦点在任意子控件时都能关闭。弹层内的菜单
    // 在捕获阶段先消费 Escape，不会连带关闭弹窗（design.md 6.2）。
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.isComposing || e.defaultPrevented || dialogStack.at(-1)?.token !== id) return;
      e.preventDefault();
      e.stopPropagation();
      closeRef.current();
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      const index = dialogStack.findIndex((item) => item.token === id);
      if (index >= 0) dialogStack.splice(index, 1);
      const target = returnFocusTo?.current ?? restoreRef.current;
      if (target?.isConnected) target.focus({ preventScroll: true });
    };
  }, [open, returnFocusTo, depth]);

  if (!open) return null;

  const trapTab = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "Tab" || dialogStack.at(-1)?.token !== token.current) return;
    const panel = panelRef.current;
    if (!panel) return;
    const items = Array.from(panel.querySelectorAll<HTMLElement>(FOCUSABLE));
    const first = items[0];
    const last = items[items.length - 1];
    if (!first || !last) {
      e.preventDefault();
      return;
    }
    const active = document.activeElement;
    if (!e.shiftKey && (active === last || active === panel)) {
      e.preventDefault();
      first.focus();
    } else if (e.shiftKey && (active === first || active === panel)) {
      e.preventDefault();
      last.focus();
    }
  };

  return createPortal(
    <DialogDepth.Provider value={depth + 1}>
    <div className="fixed inset-0 flex items-center justify-center p-4" style={{ zIndex: 50 + depth * 10 }}>
      <div
        className="absolute inset-0 animate-fade-in bg-black/15"
        aria-hidden="true"
      />
      <div
        ref={panelRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={title !== undefined ? titleId : undefined}
        tabIndex={-1}
        onKeyDown={trapTab}
        className={cn(
          "relative flex max-h-full w-full flex-col overflow-hidden outline-none",
          "rounded-xl border border-light bg-content",
          "shadow-[0_20px_80px_rgba(25,30,40,0.14)] animate-pop-in",
          wide ? "max-w-[640px]" : "max-w-[420px]",
          className,
        )}
      >
        {title !== undefined && (
          <header className="flex shrink-0 items-center justify-between gap-4 px-6 pb-4 pt-5">
            <h2 id={titleId} className="text-dialog-title font-semibold text-primary">{title}</h2>
            <button type="button" aria-label="Close dialog" onClick={onClose}
              className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-secondary hover:bg-hover">
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><path d="m4 4 8 8M12 4l-8 8" /></svg>
            </button>
          </header>
        )}
        {/* 长内容在正文中滚动，底部操作始终可达（3.3） */}
        <div className={cn("min-h-0 flex-1 overflow-y-auto px-6 pb-6", bodyClassName)}>{children}</div>
        {footer !== undefined && (
          <div className="flex shrink-0 justify-end gap-2 border-t border-light px-6 py-4">
            {footer}
          </div>
        )}
      </div>
    </div>
    </DialogDepth.Provider>, document.body
  );
}

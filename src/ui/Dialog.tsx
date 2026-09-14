import {
  useEffect,
  useId,
  useRef,
  type ReactNode,
  type RefObject,
} from "react";
import { cn } from "./cn";

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
}: DialogProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  const restoreRef = useRef<HTMLElement | null>(null);
  const titleId = useId();

  useEffect(() => {
    if (!open) return;
    const previous =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    restoreRef.current = previous;
    panelRef.current?.focus();

    // Escape 挂在 document 上：焦点在任意子控件时都能关闭。弹层内的菜单
    // 在捕获阶段先消费 Escape，不会连带关闭弹窗（design.md 6.2）。
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      onClose();
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      (returnFocusTo?.current ?? restoreRef.current)?.focus();
    };
  }, [open, onClose, returnFocusTo]);

  if (!open) return null;

  const trapTab = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "Tab") return;
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

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div
        className="absolute inset-0 animate-fade-in bg-black/30"
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
          "relative flex max-h-full w-full flex-col overflow-hidden",
          "rounded-sm border border-light bg-content",
          "shadow-[0_16px_48px_rgba(0,0,0,0.16)] animate-pop-in",
          wide ? "max-w-[640px]" : "max-w-[420px]",
          className,
        )}
      >
        {title !== undefined && (
          <h2
            id={titleId}
            className="px-5 pb-3 pt-4 text-dialog-title font-semibold text-primary"
          >
            {title}
          </h2>
        )}
        {/* 长内容在正文中滚动，底部操作始终可达（3.3） */}
        <div className="min-h-0 flex-1 overflow-y-auto px-5 pb-4">{children}</div>
        {footer !== undefined && (
          <div className="flex justify-end gap-2 border-t border-light px-5 py-3">
            {footer}
          </div>
        )}
      </div>
    </div>
  );
}

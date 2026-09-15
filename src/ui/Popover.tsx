import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type ReactNode,
  type RefObject,
} from "react";
import { cn } from "./cn";
import { createPortal } from "react-dom";

export type PopoverProps = {
  open: boolean;
  onClose: () => void;
  /** 锚定元素：菜单出现在其下方，视口放不下时翻到上方。 */
  anchorRef: RefObject<HTMLElement | null>;
  /** 菜单的可访问名称；锚点按钮自行携带 aria-haspopup/aria-expanded。 */
  label?: string;
  children?: ReactNode;
  className?: string;
};

export type PopoverItemProps = Omit<
  ButtonHTMLAttributes<HTMLButtonElement>,
  "onSelect"
> & {
  /** 点击选中菜单项；需要关闭菜单时由调用方在 onSelect 里调 onClose。 */
  onSelect?: () => void;
};

/**
 * 轻量弹出菜单（design.md 9.2）：靠近触发点，Escape / 点击外部关闭，
 * 关闭后焦点回到锚点。方向键可在菜单项间移动。位置在打开时按锚点矩形
 * 挂到 body 避免列内滚动裁切；滚动、窗口及菜单内容变化时重新定位。
 */
export function Popover({
  open,
  onClose,
  anchorRef,
  label,
  className,
  children,
}: PopoverProps) {
  const panelRef = useRef<HTMLDivElement>(null);
  // onClose 保持 ref 订阅稳定，避免调用方传内联函数导致打开态重复订阅。
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  const [position, setPosition] = useState({ top: 0, left: 0 });

  useLayoutEffect(() => {
    if (!open) return;
    const anchor = anchorRef.current;
    if (!anchor) return;
    const update = () => {
    const rect = anchor.getBoundingClientRect();
    const panel = panelRef.current;
    const height = panel?.offsetHeight ?? 0;
    const width = panel?.offsetWidth ?? 0;
    const margin = 4;
    const spaceBelow = window.innerHeight - rect.bottom;
    setPosition({
      top:
        spaceBelow >= height + margin
          ? rect.bottom + margin
          : Math.max(margin, rect.top - height - margin),
      left: Math.min(
        Math.max(margin, rect.left),
        Math.max(margin, window.innerWidth - width - margin),
      ),
    });
    };
    update();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(update);
    if (panelRef.current) observer?.observe(panelRef.current);
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    return () => { observer?.disconnect(); window.removeEventListener("resize", update); window.removeEventListener("scroll", update, true); };
  }, [open, anchorRef]);

  useEffect(() => {
    if (!open) return;
    const anchor = anchorRef.current;
    const first = panelRef.current?.querySelector<HTMLElement>(
      '[role="menuitem"]:not([disabled])',
    );
    (first ?? panelRef.current)?.focus();

    // 捕获阶段处理 Escape：先于外层 Dialog 的冒泡监听，内层菜单优先关闭。
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.isComposing) return;
      e.preventDefault();
      e.stopPropagation();
      onCloseRef.current();
    };
    const onPointerDown = (e: MouseEvent) => {
      const target = e.target as Node | null;
      if (!target) return;
      if (panelRef.current?.contains(target)) return;
      if (anchor?.contains(target)) return;
      onCloseRef.current();
    };
    document.addEventListener("keydown", onKeyDown, true);
    document.addEventListener("mousedown", onPointerDown, true);
    return () => {
      document.removeEventListener("keydown", onKeyDown, true);
      document.removeEventListener("mousedown", onPointerDown, true);
      anchor?.focus({ preventScroll: true });
    };
  }, [open, anchorRef]);

  if (!open) return null;

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>) => {
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) return;
    e.preventDefault();
    const items = Array.from(
      panelRef.current?.querySelectorAll<HTMLElement>('[role="menuitem"]:not([disabled])') ??
        [],
    );
    if (items.length === 0) return;
    const current =
      document.activeElement instanceof HTMLElement
        ? items.indexOf(document.activeElement)
        : -1;
    const next = e.key === "Home" ? items[0] : e.key === "End" ? items.at(-1) :
      current < 0
        ? e.key === "ArrowDown"
          ? items[0]
          : items[items.length - 1]
        : items[
            (current + (e.key === "ArrowDown" ? 1 : -1) + items.length) %
              items.length
          ];
    next?.focus();
  };

  return createPortal(
    <div
      ref={panelRef}
      role="menu"
      tabIndex={-1}
      aria-label={label}
      style={position}
      onKeyDown={onKeyDown}
      className={cn(
        "fixed z-[100] min-w-[160px] overflow-hidden rounded-lg border border-light bg-content py-1",
        "shadow-[0_8px_24px_rgba(0,0,0,0.12)] animate-pop-in",
        className,
      )}
    >
      {children}
    </div>, document.body
  );
}

/** 菜单项：13/20，内边距 8–12px（design.md 4.2/4.3）。 */
export function PopoverItem({
  onSelect,
  onClick,
  className,
  ...rest
}: PopoverItemProps) {
  return (
    <button
      type="button"
      role="menuitem"
      onClick={(e) => {
        onClick?.(e);
        onSelect?.();
      }}
      className={cn(
        "block w-full px-3 py-2 text-left text-menu text-primary",
        "transition-colors duration-100 hover:bg-hover",
        "disabled:cursor-not-allowed disabled:text-hint disabled:hover:bg-transparent",
        className,
      )}
      {...rest}
    />
  );
}

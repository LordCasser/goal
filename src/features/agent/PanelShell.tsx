/**
 * 右侧上下文面板共用外壳（design.md 3.2/3.3/11）：424px（w-panel）、
 * 标题栏（标题 + 可选状态小字 + 关闭按钮）与让位给内容的主区。问题面板
 * 与 AI 会话复用同一外壳；Escape 关面板（输入法组合中的 Escape 不当作
 * 退出），面板自身的边界用 1px 轻描边表达，无阴影。
 */
import { useEffect, useRef, type KeyboardEvent, type ReactNode } from "react";
import type * as React from "react";

import { useTranslation } from "../../lib/i18n";
import { Button } from "../../ui";

export type PanelShellProps = {
  /** aside 的可访问名称（也用于关闭按钮的标签）。 */
  label: string;
  title: string;
  /** 标题下的状态小字（如当前技能名）；缺省时不占位。 */
  subtitle?: string;
  /** Compact inline metadata, used by Coach to keep one header row. */
  headerDetails?: ReactNode;
  onClose: () => void;
  /** 外壳自管 Escape；调用方可追加自己的按键处理（如候选回答 ⌘1…⌘9）。 */
  onKeyDown?: (event: KeyboardEvent<HTMLElement>) => void;
  children: ReactNode;
};

export function PanelShell({
  label,
  title,
  subtitle,
  headerDetails,
  onClose,
  onKeyDown,
  children,
}: PanelShellProps): React.JSX.Element {
  const { t } = useTranslation("ai");
  const panelRef = useRef<HTMLElement>(null);
  const returnFocus = useRef<HTMLElement | null>(null);
  useEffect(() => {
    const panel = panelRef.current;
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    returnFocus.current = previous;
    // Changing the contextual cycle while typing must leave the task cursor intact.
    const editingTask = previous instanceof HTMLTextAreaElement && previous.closest("[data-task-id]");
    if (!panel?.contains(previous) && !editingTask) panel?.focus({ preventScroll: true });
    return () => {
      if (panel?.contains(document.activeElement) && previous?.isConnected) previous.focus({ preventScroll: true });
    };
  }, []);
  const close = () => {
    if (panelRef.current?.contains(document.activeElement) && returnFocus.current?.isConnected) returnFocus.current.focus({ preventScroll: true });
    onClose();
  };
  const handleKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    onKeyDown?.(event);
    if (event.key === "Escape" && !event.nativeEvent.isComposing && !event.defaultPrevented) {
      event.preventDefault();
      event.stopPropagation();
      close();
    }
  };

  return (
    <aside
      ref={panelRef}
      tabIndex={-1}
      aria-label={label}
      onKeyDown={handleKeyDown}
      className="flex h-full w-panel shrink-0 flex-col border-l border-light bg-content outline-none"
    >
      <header className={`flex justify-between gap-2 px-4 ${headerDetails ? "h-12 shrink-0 items-center" : "items-start pb-3 pt-4"}`}>
        <div className={headerDetails ? "flex min-w-0 flex-1 items-center gap-2" : "min-w-0"}>
          <h2 className="text-caption font-semibold uppercase tracking-[0.08em] text-secondary">
            {title}
          </h2>
          {headerDetails}
          {subtitle !== undefined && subtitle !== "" && (
            <p className="mt-0.5 text-caption text-secondary">{subtitle}</p>
          )}
        </div>
        <Button
          variant="ghost"
          size="icon"
          onClick={close}
          aria-label={t("common.closePanel", { label })}
          title={t("common.closeEsc")}
        >
          <CloseIcon />
        </Button>
      </header>
      <div className="flex min-h-0 flex-1 flex-col">{children}</div>
    </aside>
  );
}

/** 细线几何图标：16px、1.5 描边（design.md 4.3 图标语言）。 */
function CloseIcon() {
  return (
    <svg
      viewBox="0 0 16 16"
      className="h-4 w-4"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      aria-hidden="true"
    >
      <path d="M4 4l8 8M12 4l-8 8" />
    </svg>
  );
}

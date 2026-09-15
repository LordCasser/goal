import type { ReactNode } from "react";
import { cn } from "./cn";

export type EmptyStateProps = {
  /** Upper-case micro heading; pass the plain string, CSS handles casing. */
  title: string;
  description?: string;
  /** 可选主按钮等入口（9.1 空状态要提供下一步动作）。 */
  action?: ReactNode;
  /** 自定义图标；默认 20px 细线几何图标。 */
  icon?: ReactNode;
  className?: string;
};

/** 默认几何图标：尚未建立内容的空框与加号，细线风格（4.3）。 */
function DefaultIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      className="h-5 w-5"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <rect x="4" y="4" width="16" height="16" rx="2" />
      <path d="M12 9v6M9 12h6" />
    </svg>
  );
}

/**
 * 空状态/教学卡（design.md 9.1）：虚线边界只表达“这里尚未建立内容”，
 * 不用于错误提示。
 */
export function EmptyState({
  title,
  description,
  action,
  icon,
  className,
}: EmptyStateProps) {
  return (
    <div
      className={cn(
        "flex flex-col items-center justify-center gap-3 px-6 py-12 text-center",
        "rounded-lg border border-dashed border-light bg-content",
        className,
      )}
    >
      <span className="mb-1 flex h-10 w-10 items-center justify-center rounded-xl bg-subtle text-secondary">{icon ?? <DefaultIcon />}</span>
      <h3 className="text-section-title font-semibold text-primary">
        {title}
      </h3>
      {description && (
        <p className="max-w-[320px] text-body text-hint">{description}</p>
      )}
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

import type { ButtonHTMLAttributes, ReactNode } from "react";
import { cn } from "./cn";

export type CheckboxProps = Omit<
  ButtonHTMLAttributes<HTMLButtonElement>,
  "onChange" | "children"
> & {
  checked: boolean;
  onChange: (checked: boolean) => void;
  /** "mixed" wins over `checked` for the visual mark (aria-checked="mixed"). */
  indeterminate?: boolean;
  children?: ReactNode;
};

/**
 * Checkbox with a 16px visible box and a ≥28×28 hit area (design.md 4.3).
 * Text styling stays with the caller — this component never restyles the
 * label (no strikethrough; the done state is the task row's business).
 */
export function Checkbox({
  checked,
  onChange,
  indeterminate = false,
  disabled,
  className,
  children,
  ...rest
}: CheckboxProps) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={indeterminate ? "mixed" : checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={cn(
        "group inline-flex min-h-7 min-w-7 items-start gap-2 rounded-sm py-0.5 text-left",
        "transition-colors duration-100 disabled:cursor-not-allowed",
        className,
      )}
      {...rest}
    >
      <span
        aria-hidden="true"
        className={cn(
          // 与第一行文字基线对齐（4.3），多行正文不垂直居中
          "mt-[3px] flex h-4 w-4 shrink-0 items-center justify-center rounded-[2px] border",
          "transition-colors duration-100 group-disabled:opacity-45",
          indeterminate || checked
            ? "border-secondary bg-content text-primary"
            : "border-control bg-content",
        )}
      >
        {indeterminate ? (
          <svg viewBox="0 0 24 24" className="h-3 w-3" aria-hidden="true">
            <path
              d="M6 12h12"
              fill="none"
              stroke="currentColor"
              strokeWidth="2.5"
              strokeLinecap="round"
            />
          </svg>
        ) : checked ? (
          <svg viewBox="0 0 24 24" className="h-3 w-3" aria-hidden="true">
            <path
              d="M5 12.5 10 17.5 19 7"
              fill="none"
              stroke="currentColor"
              strokeWidth="2.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        ) : null}
      </span>
      {children}
    </button>
  );
}

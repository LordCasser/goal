import type { ButtonHTMLAttributes } from "react";
import { cn } from "./cn";

export type ButtonVariant = "primary" | "secondary" | "ghost";
export type ButtonSize = "md" | "compact";

export type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
  size?: ButtonSize;
  /** Shows a spinner and disables the button; layout width stays stable. */
  loading?: boolean;
};

/* 主按钮用主文字色反转成深底白字（4.1）；暖色留给运行状态，不用于按钮。 */
const VARIANT: Record<ButtonVariant, string> = {
  primary: "bg-primary text-white hover:brightness-90",
  secondary: "border border-control bg-transparent text-primary hover:bg-hover",
  ghost: "bg-transparent text-primary hover:bg-hover",
};

/* 主按钮约 36px，紧凑 28–32px（4.3）。 */
const SIZE: Record<ButtonSize, string> = {
  md: "h-9 px-4 text-[14px]",
  compact: "h-8 px-3 text-[13px]",
};

export function Button({
  variant = "secondary",
  size = "md",
  loading = false,
  disabled,
  type = "button",
  className,
  children,
  ...rest
}: ButtonProps) {
  return (
    <button
      type={type}
      disabled={disabled || loading}
      aria-busy={loading || undefined}
      className={cn(
        "inline-flex items-center justify-center gap-2 rounded-sm font-medium",
        "transition-colors duration-100 disabled:cursor-not-allowed disabled:opacity-45",
        VARIANT[variant],
        SIZE[size],
        className,
      )}
      {...rest}
    >
      {loading && (
        <svg
          viewBox="0 0 16 16"
          className="h-3.5 w-3.5 shrink-0 animate-spin"
          aria-hidden="true"
        >
          <circle
            cx="8"
            cy="8"
            r="6"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeOpacity="0.25"
          />
          <path
            d="M8 2a6 6 0 0 1 6 6"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
          />
        </svg>
      )}
      {children}
    </button>
  );
}

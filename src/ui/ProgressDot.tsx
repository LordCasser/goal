import type { HTMLAttributes } from "react";
import { cn } from "./cn";

export type ProgressDotTone = "idle" | "active" | "done" | "alert";

export type ProgressDotProps = Omit<
  HTMLAttributes<HTMLSpanElement>,
  "children"
> & {
  tone: ProgressDotTone;
};

const TONE: Record<ProgressDotTone, string> = {
  idle: "border border-control bg-subtle",
  // 运行态用暖色并轻微脉动；reduced-motion 下由全局规则关掉动画（4.1/10）。
  active: "bg-accent animate-dot-pulse",
  done: "bg-primary",
  alert: "bg-danger",
};

/** 6–8px 圆点状态指示（design.md 4.3）；装饰用，语义文字由调用方提供。 */
export function ProgressDot({ tone, className, ...rest }: ProgressDotProps) {
  return (
    <span
      aria-hidden={rest["aria-label"] ? undefined : true}
      className={cn(
        "inline-block h-2 w-2 shrink-0 rounded-full",
        TONE[tone],
        className,
      )}
      {...rest}
    />
  );
}

import type { InputHTMLAttributes } from "react";
import { cn } from "./cn";

export type InputProps = InputHTMLAttributes<HTMLInputElement>;

/** Single-line text input on the content surface (design.md 4.1/4.3). */
export function Input({ type = "text", className, ...rest }: InputProps) {
  return (
    <input
      type={type}
      className={cn(
        "h-8 w-full rounded-sm border border-control bg-content px-2",
        "text-[14px] text-primary placeholder:text-hint",
        "transition-colors duration-100",
        "disabled:cursor-not-allowed disabled:bg-subtle disabled:text-hint",
        className,
      )}
      {...rest}
    />
  );
}

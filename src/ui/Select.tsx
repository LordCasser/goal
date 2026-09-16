import * as SelectPrimitive from "@radix-ui/react-select";
import {
  forwardRef,
  type ComponentPropsWithoutRef,
  type ElementRef,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
} from "react";
import { cn } from "./cn";

type SelectRootProps = ComponentPropsWithoutRef<typeof SelectPrimitive.Root>;

export type SelectProps = Omit<SelectRootProps, "children"> & {
  children?: ReactNode;
  id?: string;
  "aria-label"?: string;
  "aria-labelledby"?: string;
  placeholder?: ReactNode;
  triggerClassName?: string;
  contentClassName?: string;
};

function isComposing(event: ReactKeyboardEvent<HTMLElement>): boolean {
  return event.nativeEvent.isComposing || event.keyCode === 229 || event.nativeEvent.keyCode === 229;
}

/**
 * Bounded select used by settings and forms.  The list is portalled so it is
 * never clipped by a dialog's scrolling body; Radix owns focus, typeahead,
 * and nested Escape handling while the local capture guards preserve IME
 * composition events.
 */
export function Select({
  children,
  id,
  "aria-label": ariaLabel,
  "aria-labelledby": ariaLabelledBy,
  placeholder,
  triggerClassName,
  contentClassName,
  ...rootProps
}: SelectProps) {
  return (
    <SelectPrimitive.Root {...rootProps}>
      <SelectPrimitive.Trigger
        id={id}
        aria-label={ariaLabel}
        aria-labelledby={ariaLabelledBy}
        onKeyDownCapture={(event) => {
          if (isComposing(event)) event.stopPropagation();
        }}
        className={cn(
          "inline-flex h-9 min-w-0 items-center justify-between gap-2 rounded-md border border-control bg-content px-3",
          "text-[14px] text-primary transition-colors duration-100",
          "focus:outline-2 focus:outline-focus disabled:cursor-not-allowed disabled:bg-subtle disabled:text-hint disabled:opacity-60",
          triggerClassName,
        )}
      >
        <span className="min-w-0 flex-1 truncate text-left">
          <SelectPrimitive.Value placeholder={placeholder} />
        </span>
        <SelectPrimitive.Icon aria-hidden="true" className="shrink-0 text-secondary">
          <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="m4 6 4 4 4-4" />
          </svg>
        </SelectPrimitive.Icon>
      </SelectPrimitive.Trigger>
      <SelectPrimitive.Portal>
        <SelectPrimitive.Content
          position="popper"
          align="start"
          sideOffset={6}
          collisionPadding={8}
          className={cn(
            "z-[110] flex max-h-[min(32rem,calc(100dvh-16px))] min-w-[var(--radix-select-trigger-width)] max-w-[calc(100vw-16px)] flex-col overflow-hidden rounded-md border border-light bg-content",
            "shadow-[0_8px_24px_rgba(25,30,40,0.14)] animate-pop-in",
            contentClassName,
          )}
          style={{ maxWidth: "calc(100vw - 16px)", maxHeight: "min(32rem, var(--radix-select-content-available-height, calc(100dvh - 16px)))" }}
          onKeyDownCapture={(event) => {
            if (isComposing(event)) event.stopPropagation();
          }}
          onEscapeKeyDown={(event) => {
            if (event.isComposing || event.keyCode === 229) event.preventDefault();
          }}
        >
          <SelectPrimitive.ScrollUpButton className="flex h-6 items-center justify-center text-hint">
            <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><path d="m4 10 4-4 4 4" /></svg>
          </SelectPrimitive.ScrollUpButton>
          <SelectPrimitive.Viewport className="min-h-0 flex-1 overflow-y-auto p-1">
            {children}
          </SelectPrimitive.Viewport>
          <SelectPrimitive.ScrollDownButton className="flex h-6 items-center justify-center text-hint">
            <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><path d="m4 6 4 4 4-4" /></svg>
          </SelectPrimitive.ScrollDownButton>
        </SelectPrimitive.Content>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  );
}

export type SelectItemProps = ComponentPropsWithoutRef<typeof SelectPrimitive.Item>;

export const SelectItem = forwardRef<
  ElementRef<typeof SelectPrimitive.Item>,
  SelectItemProps
>(function SelectItem({ className, children, ...props }, ref) {
  return (
    <SelectPrimitive.Item
      ref={ref}
      className={cn(
        "relative flex w-full cursor-default select-none items-start rounded-sm py-2 pl-8 pr-3 text-left text-[14px] leading-5 outline-none",
        "whitespace-normal break-words text-primary data-[highlighted]:bg-hover data-[highlighted]:text-primary",
        "data-[disabled]:pointer-events-none data-[disabled]:text-hint",
        className,
      )}
      {...props}
    >
      <SelectPrimitive.ItemIndicator className="absolute left-2 top-2.5 text-focus">
        <svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden="true"><path d="m3 8 3 3 7-7" /></svg>
      </SelectPrimitive.ItemIndicator>
      <SelectPrimitive.ItemText className="min-w-0 whitespace-normal break-words">
        {children}
      </SelectPrimitive.ItemText>
    </SelectPrimitive.Item>
  );
});

export const SelectGroup = SelectPrimitive.Group;

export const SelectLabel = forwardRef<
  ElementRef<typeof SelectPrimitive.Label>,
  ComponentPropsWithoutRef<typeof SelectPrimitive.Label>
>(function SelectLabel({ className, ...props }, ref) {
  return (
    <SelectPrimitive.Label
      ref={ref}
      className={cn("px-3 pb-1 pt-2 text-caption font-medium text-secondary", className)}
      {...props}
    />
  );
});

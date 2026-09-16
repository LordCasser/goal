/**
 * Controlled adaptations of Halaska Kit's PromptInputPattern,
 * MessageThreadPattern, ApprovalCardPattern and ScrollArea.
 * MIT · Copyright (c) 2026 Halaska Studio · https://ui.halaska.com
 * Full license: public/licenses/halaska-ui.txt. The kit's demo data, timers,
 * injected global theme and external fonts are omitted; Goal owns state.
 */
import { useLayoutEffect, useRef, type ComponentProps, type ReactNode } from "react";
import { Button } from "./Button";
import { cn } from "./cn";

// One scrollbar treatment for the transcript, pending changes and composer.
const scrollStyle = "[scrollbar-width:thin] [scrollbar-color:var(--border-light)_transparent] [&::-webkit-scrollbar]:w-1 [&::-webkit-scrollbar-thumb]:rounded-full [&::-webkit-scrollbar-thumb]:bg-control/30 [&::-webkit-scrollbar-track]:bg-transparent";

export function ChatScrollArea({ className, ...props }: ComponentProps<"div">) {
  return <div className={cn("min-h-0 overflow-x-hidden overflow-y-auto overscroll-contain", scrollStyle, className)} {...props} />;
}

export function PromptInput({ value, onValueChange, onSubmit, disabled, label, placeholder, sendLabel, hint }: {
  value: string;
  onValueChange: (value: string) => void;
  onSubmit: () => void;
  disabled: boolean;
  label: string;
  placeholder: string;
  sendLabel: string;
  hint: string;
}) {
  const input = useRef<HTMLTextAreaElement>(null);
  useLayoutEffect(() => {
    const element = input.current;
    if (!element) return;
    element.style.height = "auto";
    element.style.height = `${Math.min(160, Math.max(44, element.scrollHeight))}px`;
  }, [value]);
  return <form onSubmit={(event) => { event.preventDefault(); if (!disabled && value.trim()) onSubmit(); }}
    className="rounded-2xl border border-light bg-subtle p-1 transition-colors focus-within:border-control">
    <textarea ref={input} rows={1} aria-label={label} placeholder={placeholder}
      value={value} onChange={(event) => onValueChange(event.target.value)}
      onKeyDown={(event) => {
        if (event.key !== "Enter" || event.shiftKey || event.nativeEvent.isComposing || event.keyCode === 229) return;
        event.preventDefault();
        if (!disabled && value.trim()) onSubmit();
      }}
      className={cn("block max-h-40 min-h-11 w-full resize-none overflow-y-auto border-0 bg-transparent px-3 pt-2.5 pb-1 text-body text-primary outline-none placeholder:text-hint focus-visible:outline-none overscroll-contain", scrollStyle)} />
    <div className="flex items-center justify-between gap-2 px-2 pb-1.5 pt-1">
      <span className="text-[11px] text-hint">{hint}</span>
      <Button type="submit" variant="primary" size="icon" className="h-8! w-8! rounded-full!"
        aria-label={sendLabel} title={sendLabel} disabled={disabled || !value.trim()}>
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d="M8 12.5v-9M4 7.5l4-4 4 4" /></svg>
      </Button>
    </div>
  </form>;
}

export function ChatMessage({ role, children }: { role: "user" | "assistant"; children: ReactNode }) {
  return role === "user"
    ? <div className="coach-message flex justify-end"><p className="max-w-[85%] whitespace-pre-wrap break-words rounded-2xl rounded-br-sm border border-light bg-subtle px-3.5 py-2.5 text-body text-primary">{children}</p></div>
    : <div className="coach-message flex min-w-0 flex-col items-start gap-3 text-body text-primary">{children}</div>;
}

export function ApprovalCard({ label, title, subtitle, count, children, footer, error }: {
  label: string; title: string; subtitle?: string; count: string;
  children: ReactNode; footer?: ReactNode; error?: string;
}) {
  return <section aria-label={label} className="coach-approval overflow-hidden rounded-xl border border-light bg-content">
    <header className="flex items-center justify-between gap-3 bg-subtle px-3 py-3">
      <div className="min-w-0"><p className="text-caption font-medium text-primary">{title}</p>
        {subtitle && <p className="mt-0.5 text-caption text-secondary">{subtitle}</p>}</div>
      <span className="shrink-0 rounded-full border border-light bg-content px-2 py-0.5 text-caption text-secondary">{count}</span>
    </header>
    <div className="divide-y divide-light">{children}</div>
    {footer && <div className="flex flex-wrap justify-end gap-2 border-t border-light px-3 py-2">{footer}</div>}
    {error && <p role="alert" className="border-t border-light px-3 py-2 text-caption text-danger">{error}</p>}
  </section>;
}

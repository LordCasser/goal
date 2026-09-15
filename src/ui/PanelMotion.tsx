import { useEffect, useState, type ReactNode } from "react";

/** Keep the closing panel inert until its transition finishes. */
export function PanelMotion({ open, children, side = "right" }: { open: boolean; children: ReactNode; side?: "left" | "right" }) {
  const [mounted, setMounted] = useState(open);
  const [expanded, setExpanded] = useState(false);
  useEffect(() => {
    if (!open) { setExpanded(false); return; }
    setMounted(true);
    const frame = requestAnimationFrame(() => setExpanded(true));
    return () => cancelAnimationFrame(frame);
  }, [open]);
  useEffect(() => {
    if (open || !mounted) return;
    // Fallback also covers reduced-motion and an interrupted transition.
    const timeout = window.setTimeout(() => setMounted(false), 220);
    return () => window.clearTimeout(timeout);
  }, [open, mounted]);
  return <div className="panel-motion" data-open={expanded} data-side={side} inert={!open}
    onTransitionEnd={(event) => { if (event.target === event.currentTarget && !open) setMounted(false); }}>
    <div className="min-w-0 overflow-hidden">{mounted && <div className="panel-motion-content h-full w-panel">{children}</div>}</div>
  </div>;
}

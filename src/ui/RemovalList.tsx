import { Children, isValidElement, useLayoutEffect, useRef, useState, type ReactElement, type ReactNode, type SyntheticEvent } from "react";

export const REMOVAL_DURATION_MS = 340;
type Entry = { key: string; node: ReactElement; exiting: boolean };

function entriesFor(children: ReactNode): Entry[] {
  return Children.toArray(children).filter(isValidElement).map((node) => ({ key: String(node.key), node, exiting: false }));
}

/** Retain only the presentation of removed query results; never delay a write
 * or add removed objects back to the query cache. Keys must be domain IDs. */
export function RemovalList({ children, empty, as: Container = "div", className, gap = 0 }: {
  children: ReactNode;
  empty?: ReactNode;
  as?: "div" | "ol";
  className?: string;
  gap?: number;
}) {
  const [display, setDisplay] = useState(() => ({ source: children, entries: entriesFor(children) }));
  if (display.source !== children) {
    const next = entriesFor(children);
    const keys = new Set(next.map((entry) => entry.key));
    // Reinsert outgoing rows at their previous slots, including cascades.
    // Surviving rows always use fresh props and the current data order.
    for (let index = 0; index < display.entries.length; index++) {
      const old = display.entries[index];
      if (!old || keys.has(old.key)) continue;
      next.splice(Math.min(index, next.length), 0, { ...old, exiting: true });
    }
    setDisplay({ source: children, entries: next });
  }
  const remove = (key: string) => setDisplay((current) => ({
    ...current, entries: current.entries.filter((entry) => entry.key !== key || !entry.exiting),
  }));
  return <>{display.entries.length === 0 && empty}<Container className={className} data-removal-list>
    {display.entries.map((entry) => <RemovalItem key={entry.key} exiting={entry.exiting}
      as={Container === "ol" ? "li" : "div"} gap={gap} onExited={() => remove(entry.key)}>{entry.node}</RemovalItem>)}
  </Container></>;
}

function RemovalItem({ children, exiting, as: Item, gap, onExited }: {
  children: ReactNode; exiting: boolean; as: "div" | "li"; gap: number; onExited: () => void;
}) {
  const ref = useRef<HTMLDivElement & HTMLLIElement>(null);
  const exitRef = useRef(onExited);
  exitRef.current = onExited;
  useLayoutEffect(() => {
    if (!exiting) return;
    const element = ref.current;
    if (!element) return;
    const media = window.matchMedia?.("(prefers-reduced-motion: reduce)");
    if (media?.matches || typeof element.animate !== "function") { exitRef.current(); return; }
    const height = element.getBoundingClientRect().height;
    const content = element.firstElementChild as HTMLElement;
    // Fade while retaining the row's space, then let neighbors close the gap.
    // Both animations stay on a single DOM instance and finish together.
    const fade = content.animate([{ opacity: 1 }, { opacity: 0 }], {
      duration: 160, easing: "cubic-bezier(.4,0,1,1)", fill: "forwards",
    });
    const collapse = element.animate([{ height: `${height}px` }, { height: "0px" }], {
      delay: 80, duration: 260, easing: "cubic-bezier(.2,0,.2,1)", fill: "both",
    });
    const done = () => exitRef.current();
    const timer = window.setTimeout(done, REMOVAL_DURATION_MS);
    const preferenceChanged = () => { if (media?.matches) done(); };
    media?.addEventListener?.("change", preferenceChanged);
    return () => {
      window.clearTimeout(timer);
      media?.removeEventListener?.("change", preferenceChanged);
      fade.cancel(); collapse.cancel();
    };
  }, [exiting]);

  // inert protects the physical DOM; capture also blocks React events from
  // any portal owned by an outgoing row, and blur-triggered saves on removal.
  const guard = (event: SyntheticEvent) => {
    if (exiting) { event.preventDefault(); event.stopPropagation(); }
  };
  return <Item ref={ref} data-removal-state={exiting ? "exiting" : "present"}
    inert={exiting} aria-hidden={exiting || undefined}
    style={exiting ? { overflow: "hidden", pointerEvents: "none", overflowAnchor: "none" } : undefined}
    onBlurCapture={guard} onClickCapture={guard} onKeyDownCapture={guard} onPointerDownCapture={guard}
    onChangeCapture={guard} onInputCapture={guard} onDragStartCapture={guard}>
    <div style={gap ? { paddingBottom: gap } : undefined}>{children}</div>
  </Item>;
}

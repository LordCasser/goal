import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";

const ROW_HEIGHT = 48;
export const DRUM_SETTLE_MS = 500;
const SNAP_DELAY_MS = 100;
const WINDOW_RADIUS = 7;

function snapIndex(value: number, direction: number): number {
  const lower = Math.floor(value);
  return Math.abs(value - lower - 0.5) < 0.000001 && direction < 0 ? lower : Math.round(value);
}

export interface DrumItem {
  label: string;
  detail: string;
  title: string;
}

/** A bounded rendering window over chronological indices, never a looping list.
 * Only user input commits a value; external alignment is deliberately silent.
 */
export function DateDrum({ label, value, current, item, onSelect, onIntent, canCommit, active = true }: {
  label: string;
  value: number;
  current: number;
  item: (index: number) => DrumItem;
  onSelect: (index: number) => void;
  onIntent?: () => void;
  canCommit?: () => boolean;
  active?: boolean;
}) {
  const id = useId();
  const root = useRef<HTMLElement>(null);
  // React owns the bounded row window; the frame loop only paints positions.
  const [center, setCenter] = useState(value);
  const renderedCenter = useRef(value);
  const rowElements = useRef(new Map<number, HTMLButtonElement>());
  const entries = useRef({ item, values: new Map<number, DrumItem>() });
  const motion = useRef({ position: value, target: value, velocity: 0, frame: 0, lastFrame: 0,
    lastInput: 0, direction: 0, pending: false, reduced: false });
  const latest = useRef({ value, onSelect, onIntent, canCommit, active });
  latest.current = { value, onSelect, onIntent, canCommit, active };
  const start = useRef(() => {});
  const seen = useRef(value);
  const currentIndex = useRef(current);
  currentIndex.current = current;
  const paint = () => {
    const position = motion.current.position;
    for (const [index, row] of rowElements.current) {
      const naturalDistance = index - position;
      const isCurrent = index === currentIndex.current;
      const distance = isCurrent ? Math.max(-2, Math.min(2, naturalDistance)) : naturalDistance;
      const visible = Math.abs(distance) < 3;
      row.style.transform = `translate3d(0, ${distance * ROW_HEIGHT}px, 0)`;
      row.style.opacity = String(isCurrent ? 1 : Math.max(0, 1 - Math.abs(distance) * 0.3));
      row.style.pointerEvents = visible ? "auto" : "none";
      const hidden = String(!visible);
      if (row.getAttribute("aria-hidden") !== hidden) row.setAttribute("aria-hidden", hidden);
      const edge = isCurrent && naturalDistance !== distance ? (naturalDistance < 0 ? "top" : "bottom") : undefined;
      if (row.dataset.pinnedEdge !== edge) {
        if (edge) row.dataset.pinnedEdge = edge;
        else delete row.dataset.pinnedEdge;
      }
    }
  };
  const paintRef = useRef(paint);
  paintRef.current = paint;

  useEffect(() => {
    const state = motion.current;
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const updateMotion = () => { state.reduced = media.matches; };
    updateMotion();
    media.addEventListener("change", updateMotion);
    const tick = () => {
      state.frame = 0;
      if (!latest.current.active) return;
      const now = Date.now();
      const dt = Math.min((now - state.lastFrame) / 1000 || 1 / 60, 0.032);
      state.lastFrame = now;
      if (!state.pending || now - state.lastInput >= SNAP_DELAY_MS) state.target = snapIndex(state.target, state.direction);
      if (state.reduced) {
        state.position = snapIndex(state.target, state.direction);
        state.velocity = 0;
      } else {
        // Critically damped spring: momentum follows repeated wheel input,
        // with no elastic bounce or axis leakage when a row snaps into place.
        const omega = 24;
        const distance = state.position - state.target;
        const decay = Math.exp(-omega * dt);
        const next = (state.velocity + omega * distance) * dt;
        state.position = state.target + (distance + next) * decay;
        state.velocity = (state.velocity - omega * next) * decay;
      }
      const settled = Number.isInteger(state.target) && Math.abs(state.position - state.target) < 0.002
        && Math.abs(state.velocity) < 0.04;
      if (settled) { state.position = state.target; state.velocity = 0; }
      paintRef.current();
      const nextCenter = Math.round(state.position);
      if (renderedCenter.current !== nextCenter) {
        renderedCenter.current = nextCenter;
        setCenter(nextCenter);
      }
      if (state.pending && settled && now - state.lastInput >= DRUM_SETTLE_MS) {
        state.pending = false;
        if (latest.current.canCommit?.() === false) state.target = latest.current.value;
        else if (state.target !== latest.current.value) latest.current.onSelect(state.target);
      }
      if (!settled || state.pending || state.position !== state.target) state.frame = requestAnimationFrame(tick);
    };
    start.current = () => {
      if (!state.frame && latest.current.active) {
        state.lastFrame = Date.now();
        state.frame = requestAnimationFrame(tick);
      }
    };
    const wheel = (event: WheelEvent) => {
      if (!latest.current.active || event.ctrlKey || event.metaKey) return;
      // Own both axes while hovered. A touchpad's slight sideways drift must
      // not simultaneously pan the surrounding horizontal workspace.
      event.preventDefault();
      event.stopPropagation();
      if (!event.deltaY) return;
      latest.current.onIntent?.();
      const pixels = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? ROW_HEIGHT * 5 : 1);
      state.target += Math.max(-3, Math.min(3, pixels / ROW_HEIGHT));
      state.direction = Math.sign(pixels);
      state.lastInput = Date.now();
      state.pending = true;
      start.current();
    };
    const element = root.current;
    element?.addEventListener("wheel", wheel, { passive: false });
    if (state.position !== state.target) start.current();
    return () => {
      element?.removeEventListener("wheel", wheel);
      media.removeEventListener("change", updateMotion);
      cancelAnimationFrame(state.frame);
      state.frame = 0;
      state.pending = false;
      start.current = () => {};
    };
  }, [active]);

  useLayoutEffect(() => {
    if (seen.current === value) return;
    seen.current = value;
    const state = motion.current;
    state.pending = false;
    state.target = value;
    start.current();
  }, [value]);

  const select = (index: number) => {
    root.current?.querySelector<HTMLElement>('[role="listbox"]')?.focus({ preventScroll: true });
    onIntent?.();
    motion.current.pending = false;
    motion.current.target = index;
    start.current();
    // A deliberate row click (or return control) does not wait for wheel idle.
    onSelect(index);
  };
  const rows = useMemo(() => {
    const indices = Array.from({ length: WINDOW_RADIUS * 2 + 1 }, (_, offset) => center + offset - WINDOW_RADIUS);
    // The current date is the same keyed row at its natural position or at
    // either edge. It never needs a duplicate shortcut or a layout spacer.
    if (!indices.includes(current)) indices.push(current);
    const previous = entries.current.item === item ? entries.current.values : new Map<number, DrumItem>();
    const values = new Map(indices.map((index) => [index, previous.get(index) ?? item(index)]));
    entries.current = { item, values };
    return indices.sort((a, b) => a - b).map((index) => ({ index, entry: values.get(index)! }));
  }, [center, current, item]);
  // Paint newly mounted boundary rows before the browser shows them.
  useLayoutEffect(() => { paintRef.current(); }, [rows]);
  return <nav ref={root} aria-label={label} data-wheel-native data-date-drum className="date-drum flex w-[128px] shrink-0 flex-col pt-1">
    <h2 className="mb-2 px-3 text-caption font-medium text-secondary">{label}</h2>
    <div role="listbox" tabIndex={active ? 0 : -1} aria-label={label} aria-activedescendant={`${id}-${center}`}
      className="date-drum-viewport relative overflow-hidden rounded-lg outline-none focus-visible:ring-2 focus-visible:ring-focus"
      onKeyDown={(event) => {
        let target = Math.round(motion.current.target);
        if (event.key === "ArrowDown") target += 1;
        else if (event.key === "ArrowUp") target -= 1;
        else if (event.key === "PageDown") target += 5;
        else if (event.key === "PageUp") target -= 5;
        else if (event.key === "Home") target = current;
        else if (event.key === "Enter" || event.key === " ") { event.preventDefault(); select(target); return; }
        else return;
        event.preventDefault();
        onIntent?.();
        motion.current.target = target;
        motion.current.lastInput = Date.now();
        motion.current.pending = true;
        start.current();
      }}>
      <div aria-hidden="true" className="date-drum-selection pointer-events-none absolute inset-x-0 rounded-md bg-focus-surface" />
      {rows.map(({ index, entry }) => {
        const isCurrent = index === current;
        return <button key={index} id={`${id}-${index}`} type="button" role="option" tabIndex={-1}
          ref={(node) => { if (node) rowElements.current.set(index, node); else rowElements.current.delete(index); }}
          aria-selected={center === index} aria-current={isCurrent ? "date" : undefined}
          data-drum-index={index} title={entry.title}
          onClick={() => select(index)}
          className={`date-drum-row absolute inset-x-0 flex cursor-pointer flex-col justify-center rounded-md px-3 text-left text-caption ${center === index ? "font-medium text-focus" : "text-secondary"}`}
          style={{ zIndex: isCurrent ? 2 : undefined,
            backgroundColor: isCurrent ? (center === index ? "var(--color-focus-surface)" : "var(--color-canvas)") : undefined }}>
          <span className="whitespace-nowrap tabular-nums">{entry.label}</span>
          <span className="text-[10px] font-normal tabular-nums">{entry.detail}</span>
        </button>;
      })}
    </div>
  </nav>;
}

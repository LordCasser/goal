import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";

export const PLAN_TRANSITION_LEAVE_MS = 160;
export const PLAN_TRANSITION_ENTER_MS = 260;

type Phase = "idle" | "waiting" | "leaving" | "entering";
type View = { identity: string; children: ReactNode };
type Target = View & { ready: boolean };

function reducedMotionPreference(): boolean {
  return typeof window !== "undefined"
    && typeof window.matchMedia === "function"
    && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/** Keep plan content stable while a neighboring plan is settling into place. */
export function PlanTransition({ identity, ready = true, children }: {
  identity: string;
  ready?: boolean;
  children: ReactNode;
}) {
  const [reducedMotion, setReducedMotion] = useState(reducedMotionPreference);
  const [view, setView] = useState<View>({ identity, children });
  const [phase, setPhase] = useState<Phase>("idle");
  const viewRef = useRef(view);
  const latestRef = useRef<Target>({ identity, ready, children });
  const pendingRef = useRef<Target | null>(null);
  const leaveTimerRef = useRef<number | null>(null);
  const enterTimerRef = useRef<number | null>(null);
  const leaveElapsedRef = useRef(false);
  const reducedMotionRef = useRef(reducedMotion);
  const mountedRef = useRef(true);
  const elementRef = useRef<HTMLDivElement>(null);
  const animationsRef = useRef<Animation[]>([]);
  const heightRef = useRef<number | null>(null);

  viewRef.current = view;
  latestRef.current = { identity, ready, children };
  reducedMotionRef.current = reducedMotion;

  const clearLeaveTimer = () => {
    if (leaveTimerRef.current !== null) {
      window.clearTimeout(leaveTimerRef.current);
      leaveTimerRef.current = null;
    }
  };
  const clearEnterTimer = () => {
    if (enterTimerRef.current !== null) {
      window.clearTimeout(enterTimerRef.current);
      enterTimerRef.current = null;
    }
  };

  const commit = (target: Target, animate: boolean) => {
    if (!mountedRef.current) return;
    clearLeaveTimer();
    clearEnterTimer();
    pendingRef.current = null;
    leaveElapsedRef.current = false;
    const next = { identity: target.identity, children: target.children };
    viewRef.current = next;
    setView(next);
    if (!animate) {
      setPhase("idle");
      return;
    }
    setPhase("entering");
    enterTimerRef.current = window.setTimeout(() => {
      enterTimerRef.current = null;
      if (mountedRef.current && viewRef.current.identity === target.identity && latestRef.current.identity === target.identity) {
        setPhase("idle");
      }
    }, PLAN_TRANSITION_ENTER_MS);
  };

  const tryCommit = () => {
    const target = pendingRef.current;
    if (!target || target.identity !== latestRef.current.identity || !target.ready) return;
    if (!reducedMotionRef.current && !leaveElapsedRef.current) return;
    commit(target, !reducedMotionRef.current);
  };

  useEffect(() => {
    mountedRef.current = true;
    if (typeof window.matchMedia !== "function") return;
    const media = window.matchMedia("(prefers-reduced-motion: reduce)");
    const onChange = () => setReducedMotion(media.matches);
    if (media.addEventListener) media.addEventListener("change", onChange);
    else media.addListener?.(onChange);
    return () => {
      if (media.removeEventListener) media.removeEventListener("change", onChange);
      else media.removeListener?.(onChange);
    };
  }, []);

  useLayoutEffect(() => {
    const target: Target = { identity, ready, children };
    const committedIdentity = viewRef.current.identity;

    if (identity === committedIdentity) {
      const next = { identity, children };
      viewRef.current = next;
      setView((previous) => previous.identity === identity && previous.children === children ? previous : next);
      pendingRef.current = null;
      clearLeaveTimer();
      leaveElapsedRef.current = false;
      if (reducedMotionRef.current) clearEnterTimer();
      setPhase((previous) => reducedMotionRef.current || previous === "leaving" || previous === "waiting" ? "idle" : previous);
      return;
    }

    const wasSamePending = pendingRef.current?.identity === identity;
    pendingRef.current = target;

    if (reducedMotionRef.current) {
      clearLeaveTimer();
      leaveElapsedRef.current = false;
      setPhase("leaving");
      tryCommit();
      return;
    }

    // Loading is not part of the fade: keep readable content until both panels
    // can switch. A new pending target must not inherit a finished exit timer.
    if (!ready) {
      clearLeaveTimer();
      clearEnterTimer();
      leaveElapsedRef.current = false;
      setPhase("waiting");
      return;
    }

    if (!wasSamePending) {
      clearLeaveTimer();
      clearEnterTimer();
      leaveElapsedRef.current = false;
      setPhase("leaving");
      leaveTimerRef.current = window.setTimeout(() => {
        leaveTimerRef.current = null;
        leaveElapsedRef.current = true;
        tryCommit();
      }, PLAN_TRANSITION_LEAVE_MS);
    } else {
      if (leaveTimerRef.current === null && !leaveElapsedRef.current) {
        setPhase("leaving");
        leaveTimerRef.current = window.setTimeout(() => {
          leaveTimerRef.current = null;
          leaveElapsedRef.current = true;
          tryCommit();
        }, PLAN_TRANSITION_LEAVE_MS);
      }
      tryCommit();
    }
  }, [children, identity, ready, reducedMotion]);

  useEffect(() => () => {
    mountedRef.current = false;
    clearLeaveTimer();
    clearEnterTimer();
  }, []);

  const identityChanged = identity !== view.identity;
  const renderedPhase: Phase = identityChanged ? (ready ? "leaving" : "waiting")
    : phase === "leaving" || phase === "waiting" ? "idle" : phase;
  const displayedChildren = identityChanged ? view.children : children;
  const inert = identityChanged;

  useLayoutEffect(() => {
    const card = elementRef.current?.firstElementChild;
    if (!(card instanceof HTMLElement)) return;
    const content = [...card.children].filter((node): node is HTMLElement => node instanceof HTMLElement);
    // Read the displayed state BEFORE cancelling animations. Interruptions
    // continue from the current opacity/height instead of jumping to a keyframe.
    const height = card.getBoundingClientRect().height;
    const opacity = content.map((node) => getComputedStyle(node).opacity);
    animationsRef.current.forEach((animation) => animation.cancel());
    animationsRef.current = [];

    if (reducedMotion || typeof card.animate !== "function") {
      heightRef.current = null;
      return;
    }
    const animate = (node: HTMLElement, frames: Keyframe[], duration: number, easing: string, fill: FillMode = "both") => {
      const animation = node.animate(frames, { duration, easing, fill });
      // Use the shared frame timestamp for both columns, even when mounting a
      // larger task list takes longer than mounting the adjacent day's content.
      if (typeof document.timeline?.currentTime === "number") animation.startTime = document.timeline.currentTime;
      animationsRef.current.push(animation);
    };
    const entering = renderedPhase === "entering";
    const leaving = renderedPhase === "leaving";
    const duration = leaving ? PLAN_TRANSITION_LEAVE_MS : PLAN_TRANSITION_ENTER_MS;
    const easing = leaving ? "cubic-bezier(.4,0,1,1)" : "cubic-bezier(.2,0,.2,1)";
    content.forEach((node, index) => {
      const from = entering ? "0" : opacity[index];
      const to = leaving ? "0" : "1";
      if (from !== to) animate(node, [{ opacity: from }, { opacity: to }], duration, easing);
    });

    const startHeight = entering ? heightRef.current : height;
    if ((entering || renderedPhase === "idle") && startHeight !== null) {
      const nextHeight = card.getBoundingClientRect().height;
      if (Math.abs(startHeight - nextHeight) > 1) {
        animate(card, [{ height: `${startHeight}px` }, { height: `${nextHeight}px` }], duration, easing, "backwards");
      }
      heightRef.current = null;
    } else if (leaving || renderedPhase === "waiting") {
      heightRef.current = height;
      // An interrupted height transition holds at its current frame while the
      // next target loads/fades, rather than snapping to the old natural height.
      if (Math.abs(card.getBoundingClientRect().height - height) > 1) {
        animate(card, [{ height: `${height}px` }, { height: `${height}px` }], 0, easing);
      }
    } else {
      heightRef.current = null;
    }
  }, [renderedPhase, view.identity, reducedMotion]);

  useEffect(() => () => animationsRef.current.forEach((animation) => animation.cancel()), []);

  return <div ref={elementRef} className="plan-transition" data-phase={renderedPhase} inert={inert}>{displayedChildren}</div>;
}

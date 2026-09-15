import { useEffect, useRef, useState, type RefObject } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { platform } from "../../lib/platform";
import { getDesktopShellState, readyDesktopShell, updateDesktopRegions,
  type DesktopRegions, type DesktopShellState } from "../../lib/ipc";

type PointerState = { hovered: boolean; pressed: boolean };
type WindowState = Omit<DesktopShellState, "revision"> & { fullscreen: boolean; pointer: PointerState };

/** Native state is transient window state, never a planner query or setting. */
export function useDesktopWindow(header: RefObject<HTMLElement | null>,
  maximize: RefObject<HTMLButtonElement | null>, drag: RefObject<HTMLDivElement | null>) {
  const [state, setState] = useState<WindowState>({ mode: platform === "windows" ? "pending" : "native",
    maximized: false, focused: true, fullscreen: false, pointer: { hovered: false, pressed: false } });
  const refreshRef = useRef<() => void>(() => {});

  useEffect(() => {
    if (platform !== "macos" && platform !== "windows") return;
    const appWindow = getCurrentWindow();
    let disposed = false;
    let mode: DesktopShellState["mode"] = platform === "windows" ? "pending" : "native";
    let revision = 0;
    let running = false;
    let measureAgain = false;
    let measureTimer: ReturnType<typeof setTimeout> | undefined;
    let refreshSequence = 0;
    const unlisten: Array<() => void> = [];
    const bind = (listener: Promise<() => void>) => {
      void listener.then((stop) => { if (disposed) stop(); else unlisten.push(stop); })
        .catch((error) => console.warn("Window listener unavailable", error));
    };
    const apply = (next: DesktopShellState) => {
      if (disposed) return;
      mode = next.mode;
      revision = Math.max(revision, next.revision);
      setState((s) => ({ ...s, mode: next.mode, maximized: next.maximized, focused: next.focused }));
    };
    const measure = async () => {
      if (disposed || mode === "native" || !header.current || !maximize.current || !drag.current) return;
      if (running) { measureAgain = true; return; }
      const box = (element: Element) => {
        const rect = element.getBoundingClientRect();
        return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
      };
      const regions: DesktopRegions = { revision: ++revision,
        viewport: { width: window.innerWidth, height: window.innerHeight },
        maximize: box(maximize.current), drag: box(drag.current) };
      if (regions.maximize.width === 0 || regions.drag.width === 0) return;
      running = true;
      try { apply(await (mode === "pending" ? readyDesktopShell(regions) : updateDesktopRegions(regions))); }
      catch (error) { console.warn("Window caption layout unavailable", error); }
      finally {
        running = false;
        if (measureAgain) { measureAgain = false; scheduleMeasure(); }
      }
    };
    // A hidden native window may throttle animation frames. Geometry must be
    // ready before it is shown, so don't use requestAnimationFrame here.
    const scheduleMeasure = () => {
      if (disposed || platform !== "windows") return;
      clearTimeout(measureTimer);
      measureTimer = setTimeout(() => { void measure(); }, 0);
    };
    const refresh = async () => {
      const sequence = ++refreshSequence;
      try {
        if (platform === "windows") {
          const next = await getDesktopShellState();
          if (disposed || sequence !== refreshSequence) return;
          apply(next);
          scheduleMeasure();
        } else {
          const [fullscreen, focused] = await Promise.all([appWindow.isFullscreen(), appWindow.isFocused()]);
          if (!disposed && sequence === refreshSequence) setState((s) => ({ ...s, fullscreen, focused }));
        }
      } catch (error) { console.warn("Window state unavailable", error); }
    };
    refreshRef.current = () => { void refresh(); };
    bind(appWindow.onResized(() => {
      // Restore the conservative macOS safe area before asynchronous queries.
      if (platform === "macos") setState((s) => ({ ...s, fullscreen: false }));
      void refresh();
    }));
    bind(appWindow.onScaleChanged(() => { void refresh(); }));
    bind(appWindow.onFocusChanged(({ payload }) => { if (!disposed) setState((s) => ({ ...s, focused: payload })); }));
    let observer: ResizeObserver | undefined;
    if (platform === "windows") {
      bind(appWindow.listen("desktop:shell-changed", () => { void refresh(); }));
      bind(appWindow.listen("desktop:layout-invalidated", scheduleMeasure));
      bind(appWindow.listen<PointerState>("desktop:caption-pointer", ({ payload }) => {
        if (!disposed) setState((s) => ({ ...s, pointer: payload }));
      }));
      observer = new ResizeObserver(scheduleMeasure);
      for (const element of [header.current, maximize.current, drag.current]) if (element) observer.observe(element);
      window.addEventListener("resize", scheduleMeasure);
    }
    void refresh();
    return () => {
      disposed = true;
      refreshRef.current = () => {};
      clearTimeout(measureTimer);
      observer?.disconnect();
      window.removeEventListener("resize", scheduleMeasure);
      unlisten.forEach((stop) => stop());
    };
  }, [header, maximize, drag]);

  const act = async (action: "minimize" | "toggleMaximize" | "close") => {
    if (platform !== "windows") return;
    try { await getCurrentWindow()[action](); refreshRef.current(); }
    catch (error) { console.warn("Window action failed", error); }
  };
  return { ...state, act };
}

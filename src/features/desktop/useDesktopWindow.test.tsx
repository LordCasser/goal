import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useEffect, useRef } from "react";

type Target = "macos" | "windows";
type ShellState = {
  mode: "pending" | "custom" | "native";
  revision: number;
  maximized: boolean;
  focused: boolean;
};

type Listener<T = void> = (event: T) => void;

type NativeWindow = {
  isFullscreen: ReturnType<typeof vi.fn>;
  isFocused: ReturnType<typeof vi.fn>;
  onResized: ReturnType<typeof vi.fn>;
  onScaleChanged: ReturnType<typeof vi.fn>;
  onFocusChanged: ReturnType<typeof vi.fn>;
  listen: ReturnType<typeof vi.fn>;
  minimize: ReturnType<typeof vi.fn>;
  toggleMaximize: ReturnType<typeof vi.fn>;
  close: ReturnType<typeof vi.fn>;
  stops: Array<ReturnType<typeof vi.fn>>;
};

type IpcMocks = {
  getState: ReturnType<typeof vi.fn>;
  ready: ReturnType<typeof vi.fn>;
  regions: ReturnType<typeof vi.fn>;
};

function nativeWindow(): NativeWindow {
  const stops: Array<ReturnType<typeof vi.fn>> = [];
  const subscribe = <T,>(_listener: Listener<T>) => {
    const stop = vi.fn();
    stops.push(stop);
    return Promise.resolve(stop);
  };
  return {
    isFullscreen: vi.fn().mockResolvedValue(false),
    isFocused: vi.fn().mockResolvedValue(true),
    onResized: vi.fn((listener: Listener) => subscribe(listener)),
    onScaleChanged: vi.fn((listener: Listener) => subscribe(listener)),
    onFocusChanged: vi.fn((listener: Listener<{ payload: boolean }>) => subscribe(listener)),
    listen: vi.fn((_name: string, listener: Listener<{ payload: unknown }>) => subscribe(listener)),
    minimize: vi.fn().mockResolvedValue(undefined),
    toggleMaximize: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    stops,
  };
}

function ipcMocks(state: ShellState, readyState = state): IpcMocks {
  return {
    getState: vi.fn().mockResolvedValue(state),
    ready: vi.fn().mockResolvedValue(readyState),
    regions: vi.fn().mockResolvedValue(readyState),
  };
}

async function loadHook(target: Target, native: NativeWindow, ipc: IpcMocks) {
  vi.resetModules();
  vi.doMock("../../lib/platform", () => ({ platform: target }));
  vi.doMock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => native }));
  vi.doMock("../../lib/ipc", () => ({
    getDesktopShellState: ipc.getState,
    readyDesktopShell: ipc.ready,
    updateDesktopRegions: ipc.regions,
  }));
  return import("./useDesktopWindow");
}

function Harness({ useDesktopWindow }: { useDesktopWindow: typeof import("./useDesktopWindow").useDesktopWindow }) {
  const header = useRef<HTMLElement>(null);
  const maximize = useRef<HTMLButtonElement>(null);
  const drag = useRef<HTMLDivElement>(null);
  const state = useDesktopWindow(header, maximize, drag);

  useEffect(() => {
    for (const element of [header.current, maximize.current, drag.current]) {
      if (!element) continue;
      Object.defineProperty(element, "getBoundingClientRect", {
        configurable: true,
        value: () => ({ x: 10, y: 4, width: element === maximize.current ? 46 : 120, height: 44 }),
      });
    }
  }, []);

  return (
    <>
      <header ref={header} />
      <button ref={maximize} onClick={() => void state.act("toggleMaximize")}>
        {state.maximized ? "Restore" : "Maximize"}
      </button>
      <div ref={drag} />
      <output data-testid="window-state">
        {`${state.mode}|${state.maximized}|${state.focused}|${state.fullscreen}`}
      </output>
    </>
  );
}

let observerDisconnect: ReturnType<typeof vi.fn>;

beforeEach(() => {
  observerDisconnect = vi.fn();
  vi.stubGlobal("ResizeObserver", class {
    observe = vi.fn();
    disconnect = observerDisconnect;
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("useDesktopWindow", () => {
  it("completes the Windows ready handshake, forwards geometry, and reflects native state", async () => {
    const native = nativeWindow();
    const pending: ShellState = { mode: "pending", revision: 1, maximized: false, focused: true };
    const custom: ShellState = { mode: "custom", revision: 2, maximized: true, focused: false };
    const ipc = ipcMocks(pending, custom);
    const { useDesktopWindow } = await loadHook("windows", native, ipc);
    render(<Harness useDesktopWindow={useDesktopWindow} />);

    await waitFor(() => expect(ipc.ready).toHaveBeenCalledOnce());
    expect(ipc.ready.mock.calls[0]?.[0]).toMatchObject({
      viewport: { width: window.innerWidth, height: window.innerHeight },
      maximize: { width: 46, height: 44 },
      drag: { width: 120, height: 44 },
    });
    expect(screen.getByTestId("window-state").textContent).toBe("custom|true|false|false");
    expect(screen.getByRole("button", { name: "Restore" })).toBeTruthy();
  });

  it("keeps the native decoration path when the Windows shell is unavailable", async () => {
    const native = nativeWindow();
    const state: ShellState = { mode: "native", revision: 3, maximized: false, focused: true };
    const ipc = ipcMocks(state);
    const { useDesktopWindow } = await loadHook("windows", native, ipc);
    render(<Harness useDesktopWindow={useDesktopWindow} />);

    await waitFor(() => expect(ipc.getState).toHaveBeenCalledOnce());
    expect(ipc.ready).not.toHaveBeenCalled();
    expect(screen.getByTestId("window-state").textContent).toBe("native|false|true|false");
  });

  it("falls back from a rejected pending shell handshake without showing custom controls", async () => {
    const native = nativeWindow();
    const pending: ShellState = { mode: "pending", revision: 1, maximized: false, focused: true };
    const ipc = ipcMocks(pending);
    ipc.getState.mockRejectedValueOnce(new Error("permission denied"));
    const { useDesktopWindow } = await loadHook("windows", native, ipc);
    render(<Harness useDesktopWindow={useDesktopWindow} />);

    await waitFor(() => expect(screen.getByTestId("window-state").textContent).toBe("native|false|true|false"));
    expect(ipc.ready).not.toHaveBeenCalled();
  });

  it("falls back from a rejected pending ready call", async () => {
    const native = nativeWindow();
    const pending: ShellState = { mode: "pending", revision: 1, maximized: false, focused: true };
    const ipc = ipcMocks(pending);
    ipc.ready.mockRejectedValueOnce(new Error("ready denied"));
    const { useDesktopWindow } = await loadHook("windows", native, ipc);
    render(<Harness useDesktopWindow={useDesktopWindow} />);

    await waitFor(() => expect(ipc.ready).toHaveBeenCalledOnce());
    await waitFor(() => expect(screen.getByTestId("window-state").textContent).toBe("native|false|true|false"));
  });

  it("keeps custom controls visible when a later custom layout update fails", async () => {
    const native = nativeWindow();
    const pending: ShellState = { mode: "pending", revision: 1, maximized: false, focused: true };
    const custom: ShellState = { mode: "custom", revision: 2, maximized: true, focused: true };
    const ipc = ipcMocks(pending, custom);
    ipc.getState.mockResolvedValueOnce(pending).mockResolvedValue(custom);
    ipc.regions.mockRejectedValue(new Error("transient resize failure"));
    const { useDesktopWindow } = await loadHook("windows", native, ipc);
    render(<Harness useDesktopWindow={useDesktopWindow} />);

    await waitFor(() => expect(screen.getByTestId("window-state").textContent).toBe("custom|true|true|false"));
    const onResize = native.onResized.mock.calls[0]?.[0] as (() => void) | undefined;
    onResize?.();
    await waitFor(() => expect(ipc.regions).toHaveBeenCalledOnce());
    expect(screen.getByTestId("window-state").textContent).toBe("custom|true|true|false");
  });

  it("executes a Windows action once and refreshes state from the native window", async () => {
    const native = nativeWindow();
    const pending: ShellState = { mode: "pending", revision: 1, maximized: false, focused: true };
    const custom: ShellState = { mode: "custom", revision: 2, maximized: false, focused: true };
    const ipc = ipcMocks(pending, custom);
    const { useDesktopWindow } = await loadHook("windows", native, ipc);
    render(<Harness useDesktopWindow={useDesktopWindow} />);
    await waitFor(() => expect(ipc.ready).toHaveBeenCalledOnce());

    fireEvent.click(screen.getByRole("button", { name: "Maximize" }));
    await waitFor(() => expect(native.toggleMaximize).toHaveBeenCalledOnce());
    expect(native.minimize).not.toHaveBeenCalled();
    expect(native.close).not.toHaveBeenCalled();
  });

  it("cleans native listeners, resize observation, and window resize on unmount", async () => {
    const native = nativeWindow();
    const pending: ShellState = { mode: "pending", revision: 1, maximized: false, focused: true };
    const ipc = ipcMocks(pending, { ...pending, mode: "custom" });
    const { useDesktopWindow } = await loadHook("windows", native, ipc);
    const addResize = vi.spyOn(window, "addEventListener");
    const removeResize = vi.spyOn(window, "removeEventListener");
    const view = render(<Harness useDesktopWindow={useDesktopWindow} />);
    await waitFor(() => expect(native.listen).toHaveBeenCalledTimes(3));
    view.unmount();

    expect(removeResize).toHaveBeenCalledWith("resize", expect.any(Function));
    expect(addResize).toHaveBeenCalledWith("resize", expect.any(Function));
    expect(native.stops).toHaveLength(6);
    expect(native.stops.every((stop) => stop.mock.calls.length === 1)).toBe(true);
    expect(observerDisconnect).toHaveBeenCalledOnce();
  });

  it("tracks macOS fullscreen/focus from the native window and ignores browser fullscreen", async () => {
    const native = nativeWindow();
    native.isFullscreen.mockResolvedValue(true);
    native.isFocused.mockResolvedValue(false);
    const ipc = ipcMocks({ mode: "native", revision: 1, maximized: false, focused: false });
    const { useDesktopWindow } = await loadHook("macos", native, ipc);
    render(<Harness useDesktopWindow={useDesktopWindow} />);

    await waitFor(() => expect(screen.getByTestId("window-state").textContent).toBe("native|false|false|true"));
    expect(native.isFullscreen).toHaveBeenCalled();
    expect(native.isFocused).toHaveBeenCalled();
    expect(ipc.getState).not.toHaveBeenCalled();
  });
});

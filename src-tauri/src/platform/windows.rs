//! Windows shell lifecycle, isolated from planner state and storage.
use super::{
    geometry::{Regions, CLIENT_SIZE_CHANGED, STALE_LAYOUT},
    windows_frame,
};
use std::{cell::Cell, sync::Arc, time::Duration};
use tauri::{Emitter, Manager, WebviewWindow};

#[derive(Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Pending,
    Custom,
    Native,
}
thread_local! { static MODE: Cell<Mode> = const { Cell::new(Mode::Pending) }; }

#[derive(serde::Serialize)]
pub struct ShellState {
    pub mode: Mode,
    pub revision: u64,
    pub maximized: bool,
    pub focused: bool,
}

pub fn state(window: &WebviewWindow) -> Result<ShellState, String> {
    Ok(ShellState {
        mode: MODE.get(),
        revision: windows_frame::revision(),
        maximized: window.is_maximized().map_err(|e| e.to_string())?,
        focused: window.is_focused().map_err(|e| e.to_string())?,
    })
}

pub fn setup(app: &tauri::App) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    // Native UI-thread deadline also works if the web frontend never starts.
    let timeout_window = window.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        let window = timeout_window.clone();
        let _ = timeout_window.run_on_main_thread(move || {
            if MODE.get() == Mode::Pending {
                fallback(&window, "caption initialization timed out");
            }
        });
    });
}

fn fallback(window: &WebviewWindow, reason: &str) {
    MODE.set(Mode::Native);
    windows_frame::uninstall();
    // Don't short circuit: the window must be shown even if restoration fails.
    let restored = window.set_decorations(true);
    let shown = window.show();
    crate::logging::warn("desktop", &format!("Windows native title bar restored: {reason}; decorations={restored:?}; visible={shown:?}"));
    let _ = window.emit("desktop:shell-changed", ());
}

pub fn ready(window: &WebviewWindow, regions: Regions) -> Result<ShellState, String> {
    if MODE.get() == Mode::Native {
        return state(window);
    }
    if MODE.get() == Mode::Custom {
        return update(window, regions);
    }
    let event_window = window.clone();
    let callback = Arc::new(move |event: windows_frame::FrameEvent| match event {
        windows_frame::FrameEvent::Pointer(pointer) => {
            let _ = event_window.emit("desktop:caption-pointer", pointer);
        }
        windows_frame::FrameEvent::LayoutInvalidated => {
            let _ = event_window.emit("desktop:layout-invalidated", ());
        }
    });
    let install = window
        .hwnd()
        .map_err(|e| e.to_string())
        .and_then(|handle| windows_frame::install(handle.0, &regions, callback));
    if let Err(error) = install {
        fallback(window, &error);
        return state(window);
    }
    if let Err(error) = window.set_decorations(false) {
        fallback(window, &error.to_string());
        return state(window);
    }
    MODE.set(Mode::Custom);
    if let Err(error) = window.show() {
        fallback(window, &error.to_string());
        return state(window);
    }
    let _ = window.emit("desktop:shell-changed", ());
    state(window)
}

pub fn update(window: &WebviewWindow, regions: Regions) -> Result<ShellState, String> {
    if MODE.get() != Mode::Custom {
        return state(window);
    }
    if let Err(error) = windows_frame::update(&regions) {
        // Resize/DPI and DOM updates race normally. The native adapter hides
        // mismatched targets until fresh geometry arrives, without a permanent
        // fallback on every minimize or cross-monitor move.
        if error == STALE_LAYOUT || error == CLIENT_SIZE_CHANGED {
            return state(window);
        }
        fallback(window, &error);
    }
    state(window)
}

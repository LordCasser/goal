//! Win32 hit targets above WebView2. Called only on the window thread.
//!
//! A tiny child HWND over the maximize button lets Windows own HTMAXBUTTON
//! without subclassing undocumented WebView2 child windows. Its pointer path
//! is native; the accessible DOM button remains the keyboard path.
//! The overlay approach follows oovz/tauri-plugin-decoration (MIT); its notice
//! is shipped in public/licenses/tauri-decoration.txt. Lifecycle and region
//! validation here are local to Planner's single main window.
use super::geometry::{Regions, CLIENT_SIZE_CHANGED};
use std::{
    cell::RefCell,
    ptr::null_mut,
    sync::{Arc, OnceLock},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT},
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::*,
    },
};

const SUBCLASS_ID: usize = 0x474F414C;
const CLASS: &[u16] = &[
    71, 111, 97, 108, 67, 97, 112, 116, 105, 111, 110, 72, 105, 116, 0,
];

#[derive(Clone, Copy, Debug, Default, serde::Serialize)]
pub struct PointerState {
    pub hovered: bool,
    pub pressed: bool,
}

#[derive(Clone, Copy)]
pub enum FrameEvent {
    Pointer(PointerState),
    LayoutInvalidated,
}

struct Frame {
    parent: HWND,
    maximize: HWND,
    drag: HWND,
    revision: u64,
    pointer: PointerState,
    callback: Arc<dyn Fn(FrameEvent)>,
}

// HWNDs are never moved between threads. No mutex is held across Win32 calls,
// which can synchronously reenter our window procedures.
thread_local! { static FRAME: RefCell<Option<Frame>> = const { RefCell::new(None) }; }

pub fn revision() -> u64 {
    FRAME.with(|f| f.borrow().as_ref().map_or(0, |f| f.revision))
}

fn os_error(operation: &str) -> String {
    format!("{operation}: {}", std::io::Error::last_os_error())
}

fn client_size(parent: HWND) -> Result<(i32, i32), String> {
    let mut rect = RECT::default();
    // SAFETY: parent is supplied by the live Tauri window; rect is writable.
    if unsafe { GetClientRect(parent, &mut rect) } == 0 {
        return Err(os_error("GetClientRect"));
    }
    Ok((rect.right, rect.bottom))
}

pub fn install(
    parent: HWND,
    regions: &Regions,
    callback: Arc<dyn Fn(FrameEvent)>,
) -> Result<(), String> {
    if FRAME.with(|f| f.borrow().is_some()) {
        return update(regions);
    }
    let (width, height) = client_size(parent)?;
    regions.validate(0, width, height)?;
    static REGISTERED: OnceLock<Result<(), String>> = OnceLock::new();
    REGISTERED
        .get_or_init(|| {
            // SAFETY: static UTF-16 name and callback live for the whole process.
            let class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(hit_proc),
                hInstance: unsafe { GetModuleHandleW(std::ptr::null()) },
                lpszClassName: CLASS.as_ptr(),
                ..unsafe { std::mem::zeroed() }
            };
            if unsafe { RegisterClassExW(&class) } == 0 {
                Err(os_error("RegisterClassExW"))
            } else {
                Ok(())
            }
        })
        .clone()?;
    // The windows have no paint brush: WebView2 continues to draw the controls.
    let create = || unsafe {
        CreateWindowExW(
            0,
            CLASS.as_ptr(),
            CLASS.as_ptr(),
            WS_CHILD | WS_CLIPSIBLINGS,
            0,
            0,
            0,
            0,
            parent,
            null_mut(),
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        )
    };
    let maximize = create();
    if maximize.is_null() {
        return Err(os_error("CreateWindowExW maximize"));
    }
    let drag = create();
    if drag.is_null() {
        unsafe {
            DestroyWindow(maximize);
        }
        return Err(os_error("CreateWindowExW drag"));
    }
    if unsafe { SetWindowSubclass(parent, Some(parent_proc), SUBCLASS_ID, 0) } == 0 {
        unsafe {
            DestroyWindow(maximize);
            DestroyWindow(drag);
        }
        return Err(os_error("SetWindowSubclass"));
    }
    FRAME.with(|f| {
        *f.borrow_mut() = Some(Frame {
            parent,
            maximize,
            drag,
            revision: 0,
            pointer: PointerState::default(),
            callback,
        })
    });
    if let Err(error) = update(regions) {
        uninstall();
        return Err(error);
    }
    Ok(())
}

pub fn update(regions: &Regions) -> Result<(), String> {
    let (parent, maximize, drag, previous) = FRAME
        .with(|f| {
            f.borrow()
                .as_ref()
                .map(|f| (f.parent, f.maximize, f.drag, f.revision))
        })
        .ok_or("desktop frame is not installed")?;
    let (width, height) = client_size(parent)?;
    let (max_rect, drag_rect) = regions.validate(previous, width, height).map_err(|error| {
        // Resizing/minimizing can beat a queued DOM measurement. Hide the old
        // targets until the next resize measurement; never intercept stale UI.
        if error == CLIENT_SIZE_CHANGED {
            invalidate(false);
        }
        error
    })?;
    for (hwnd, rect) in [(maximize, max_rect), (drag, drag_rect)] {
        // SAFETY: live children owned by this frame, bounded validated geometry.
        if unsafe {
            SetWindowPos(
                hwnd,
                HWND_TOP,
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            )
        } == 0
        {
            return Err(os_error("SetWindowPos"));
        }
    }
    FRAME.with(|f| {
        if let Some(f) = f.borrow_mut().as_mut() {
            f.revision = regions.revision;
        }
    });
    Ok(())
}

pub fn uninstall() {
    let frame = FRAME.with(|f| f.borrow_mut().take());
    if let Some(frame) = frame {
        // Remove bookkeeping before destruction, which reenters hit_proc.
        unsafe {
            RemoveWindowSubclass(frame.parent, Some(parent_proc), SUBCLASS_ID);
            ShowWindow(frame.maximize, SW_HIDE);
            ShowWindow(frame.drag, SW_HIDE);
            DestroyWindow(frame.maximize);
            DestroyWindow(frame.drag);
        }
        (frame.callback)(FrameEvent::Pointer(PointerState::default()));
    }
}

fn invalidate(notify_layout: bool) {
    let snapshot = FRAME.with(|f| {
        f.borrow_mut().as_mut().map(|f| {
            f.pointer = PointerState::default();
            (f.maximize, f.drag, f.callback.clone())
        })
    });
    if let Some((maximize, drag, callback)) = snapshot {
        unsafe {
            ShowWindow(maximize, SW_HIDE);
            ShowWindow(drag, SW_HIDE);
        }
        callback(FrameEvent::Pointer(PointerState::default()));
        if notify_layout {
            callback(FrameEvent::LayoutInvalidated);
        }
    }
}

fn pointer(hwnd: HWND, hover: bool, pressed: Option<bool>) {
    let event = FRAME.with(|f| {
        let mut frame = f.borrow_mut();
        let f = frame.as_mut()?;
        if f.maximize != hwnd {
            return None;
        }
        let old = f.pointer;
        f.pointer.hovered = hover;
        if let Some(pressed) = pressed {
            f.pointer.pressed = pressed;
        }
        if old.hovered == f.pointer.hovered && old.pressed == f.pointer.pressed {
            return None;
        }
        Some((f.callback.clone(), f.pointer))
    });
    if let Some((callback, state)) = event {
        callback(FrameEvent::Pointer(state));
    }
}

unsafe extern "system" fn hit_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // Every callback boundary is non-unwinding. Internal actions don't hold a
    // RefCell borrow while forwarding messages or notifying the frontend.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        let owner = FRAME.with(|f| {
            f.borrow().as_ref().and_then(|f| {
                if f.maximize == hwnd {
                    Some((f.parent, true, f.pointer.pressed))
                } else if f.drag == hwnd {
                    Some((f.parent, false, false))
                } else {
                    None
                }
            })
        });
        if let Some((parent, is_maximize, pressed)) = owner {
            if msg == WM_NCHITTEST {
                return if is_maximize { HTMAXBUTTON } else { HTCAPTION } as isize;
            }
            if !is_maximize && matches!(msg, WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK | WM_NCRBUTTONUP)
            {
                // The parent, not this small child, owns move/zoom/system menu.
                return SendMessageW(parent, msg, HTCAPTION as usize, lparam);
            }
            if is_maximize {
                match msg {
                    WM_NCMOUSEMOVE => {
                        pointer(hwnd, true, None);
                        let mut track = TRACKMOUSEEVENT {
                            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE | TME_NONCLIENT,
                            hwndTrack: hwnd,
                            dwHoverTime: 0,
                        };
                        TrackMouseEvent(&mut track);
                        return 0;
                    }
                    WM_NCMOUSELEAVE => {
                        pointer(hwnd, false, Some(false));
                        return 0;
                    }
                    WM_NCLBUTTONDOWN => {
                        pointer(hwnd, true, Some(true));
                        return 0;
                    }
                    WM_NCLBUTTONUP => {
                        pointer(hwnd, true, Some(false));
                        if pressed {
                            let command = if IsZoomed(parent) != 0 {
                                SC_RESTORE
                            } else {
                                SC_MAXIMIZE
                            };
                            PostMessageW(parent, WM_SYSCOMMAND, command as usize, 0);
                        }
                        return 0;
                    }
                    _ => {}
                }
            }
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }))
    .unwrap_or_else(|_| unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) })
}

unsafe extern "system" fn parent_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _id: usize,
    _data: usize,
) -> LRESULT {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        match msg {
            WM_SIZE | WM_DPICHANGED => invalidate(true),
            WM_NCDESTROY => {
                // Windows already owns child destruction at this point.
                FRAME.with(|f| {
                    f.borrow_mut().take();
                });
                RemoveWindowSubclass(hwnd, Some(parent_proc), SUBCLASS_ID);
            }
            _ => {}
        }
        // Keep Tauri/tao/wry's handlers for nonclient sizing, DWM, taskbar and
        // CloseRequested. In particular don't replace them with DefWindowProc.
        DefSubclassProc(hwnd, msg, wparam, lparam)
    }))
    .unwrap_or_else(|_| unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) })
}

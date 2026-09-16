//! Restricted window IPC. No arbitrary HWND or native message is accepted.
use crate::{
    error::{AppError, AppResult},
    platform::{
        geometry::Regions,
        windows::{self, ShellState},
    },
};
use tauri::WebviewWindow;

async fn on_window_thread<T: Send + 'static>(
    window: WebviewWindow,
    action: impl FnOnce(&WebviewWindow) -> Result<T, String> + Send + 'static,
) -> AppResult<T> {
    if window.label() != "main" {
        return Err(AppError::Internal(
            "desktop shell is restricted to main".into(),
        ));
    }
    let (sender, mut receiver) = tauri::async_runtime::channel(1);
    let target = window.clone();
    window
        .run_on_main_thread(move || {
            let _ = sender.try_send(action(&target));
        })
        .map_err(|e| AppError::Internal(e.to_string()))?;
    receiver
        .recv()
        .await
        .ok_or_else(|| AppError::Internal("window thread unavailable".into()))?
        .map_err(AppError::Internal)
}

#[tauri::command]
pub async fn desktop_shell_state(window: WebviewWindow) -> AppResult<ShellState> {
    on_window_thread(window, windows::state).await
}

#[tauri::command]
pub async fn desktop_shell_ready(window: WebviewWindow, regions: Regions) -> AppResult<ShellState> {
    on_window_thread(window, move |window| windows::ready(window, regions)).await
}

#[tauri::command]
pub async fn desktop_shell_regions(
    window: WebviewWindow,
    regions: Regions,
) -> AppResult<ShellState> {
    on_window_thread(window, move |window| windows::update(window, regions)).await
}

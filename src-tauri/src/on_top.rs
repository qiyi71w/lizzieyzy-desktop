//! 窗口置顶 switched on an open tool window: 黑白胜率大字, the floating boards, 超级鹰眼 and the
//! 独立选点列表 (Java `setAlwaysOnTop` on their frames). Every platform makes the runtime's switch;
//! on Windows the window is then restacked, because the runtime's first switch of a window created
//! not on top can mark it topmost without moving it out of the ordinary windows, or leave it
//! unmarked although the runtime reports it on top.
use lizzie_core::api::{ApiError, ErrorKind};
use tauri::{AppHandle, Manager, WebviewWindow};

/// 窗口置顶 of the open window `label` (nothing when it is not open). Synchronous, so it runs on the
/// thread that owns the window.
#[tauri::command]
pub(super) fn set_window_on_top(app: AppHandle, label: String, on_top: bool) -> Result<(), ApiError> {
    match app.get_webview_window(&label) {
        Some(window) => switch(&window, on_top),
        None => Ok(()),
    }
}

/// The switch itself, also for a window the shell retitles while it is open.
pub(super) fn switch(window: &WebviewWindow, on_top: bool) -> Result<(), ApiError> {
    #[cfg(windows)]
    let changed = window.is_always_on_top().map_err(failed)? != on_top;
    window.set_always_on_top(on_top).map_err(failed)?;
    // Only a change is restacked: the same setting again must not lift the window over others.
    #[cfg(windows)]
    if changed {
        restack(window, on_top)?;
    }
    Ok(())
}

fn failed(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(ErrorKind::Unavailable, format!("窗口置顶 failed: {e}"))
}

/// Restacks the window the way Windows defines the switch: above every ordinary window, or,
/// switched off, at the top of the ordinary windows where the next one raised covers it.
#[cfg(windows)]
fn restack(window: &WebviewWindow, on_top: bool) -> Result<(), ApiError> {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, HWND_NOTOPMOST, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE};
    let hwnd = window.hwnd().map_err(failed)?;
    let after = if on_top { HWND_TOPMOST } else { HWND_NOTOPMOST };
    // SAFETY: `hwnd` is this live window's handle; the call only reorders it.
    unsafe { SetWindowPos(hwnd, Some(after), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE) }.map_err(failed)
}

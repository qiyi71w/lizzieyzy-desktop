//! 棋盘同步工具's float board window (Java `FloatBoard`, a JDialog): undecorated, transparent,
//! always on top and not resizable, placed over the client's board in physical pixels. It is built
//! the first time the helper shows it and afterwards only hidden and shown again, like the Java
//! dialog that is never disposed; the application's exit destroys it.
use lizzie_core::api::{ApiError, ErrorKind};
use serde::Serialize;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub(super) const LABEL: &str = "readboard-float";

/// Where the window went: the edge round the helper's rectangle (Java `zoomIn(20)`) and the
/// window's physical size (`posWidth` x `posHeight`); `created` when the page is new.
#[derive(Serialize)]
pub(super) struct Placed {
    pad: i32,
    width: u32,
    height: u32,
    created: bool,
}

fn unavailable(e: tauri::Error) -> ApiError {
    ApiError::new(ErrorKind::Unavailable, format!("placing the float board failed: {e}"))
}

fn set_bounds(window: &WebviewWindow, position: PhysicalPosition<i32>, size: PhysicalSize<u32>) -> Result<(), ApiError> {
    window.set_size(size).map_err(unavailable)?;
    window.set_position(position).map_err(unavailable)
}

/// `new FloatBoard` / `setPos` (FloatBoard.java:97-114, 844-886) for the helper's rectangle in
/// physical pixels: the window reaches `zoomIn(20)` beyond it on each side, `zoomIn` dividing by
/// the default screen's scale when it is above 1 (Lizzie.java:396-406). Async: building a window
/// inside a synchronous command deadlocks on Windows.
#[tauri::command]
pub(super) async fn show_float_board(app: AppHandle, x: i32, y: i32, width: i32, height: i32) -> Result<Placed, ApiError> {
    let scale = app.primary_monitor().ok().flatten().map_or(1.0, |m| m.scale_factor());
    let pad = if scale > 1.0 { (20.0 / scale).round() as i32 } else { 20 };
    let position = PhysicalPosition::new(x - pad, y - pad);
    let size = PhysicalSize::new((width + 2 * pad).max(1) as u32, (height + 2 * pad).max(1) as u32);
    let placed = |created| Placed { pad, width: size.width, height: size.height, created };
    if let Some(window) = app.get_webview_window(LABEL) {
        set_bounds(&window, position, size)?;
        window.show().map_err(unavailable)?;
        return Ok(placed(false));
    }
    let builder = WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::App("floatboard.html".into()))
        .title("FloatBoard")
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .visible(false)
        .inner_size(f64::from(size.width) / scale, f64::from(size.height) / scale)
        .position(f64::from(position.x) / scale, f64::from(position.y) / scale)
        .disable_drag_drop_handler();
    // Only the overlay is painted; the client's board shows through (setBackground(0,0,0,0)).
    // Transparent windows need the private API on macOS, where the helper does not run.
    #[cfg(not(target_os = "macos"))]
    let builder = builder.transparent(true);
    let window = builder.build().map_err(unavailable)?;
    set_bounds(&window, position, size)?;
    window.show().map_err(unavailable)?;
    Ok(placed(true))
}

/// `setVisible(false)`: `noinboard`, `notinboard`, the end of a sync, the helper's exit, or a
/// placement in progress.
#[tauri::command]
pub(super) async fn hide_float_board(app: AppHandle) -> Result<(), ApiError> {
    if let Some(window) = app.get_webview_window(LABEL) {
        window.hide().map_err(unavailable)?;
    }
    Ok(())
}

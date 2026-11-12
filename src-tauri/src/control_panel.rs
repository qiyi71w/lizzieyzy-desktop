//! 独立棋局控制面板 (Java `Controller`, a JDialog owned by the main frame): a decorated, resizable
//! window that is not always on top. It is built at its first opening and afterwards hidden and
//! shown again (`LizzieFrame.openController`); its close box hides it. Its bounds are kept for the
//! next opening and, with whether it is shown, written at exit for the next start.
use lizzie_core::api::{ApiError, ErrorKind};
use lizzie_core::workbench::Workbench;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use super::{Shared, State};

pub(super) const LABEL: &str = "control-panel";
/// The panel's close box hid it; the main window's menu check follows.
const CLOSED_EVENT: &str = "control-panel-closed";
/// `Lizzie.setFrameSize(500, 460)` at scale 1 (Lizzie.java:898-907): a 510 x 470 window.
const DEFAULT_OUTER: (f64, f64) = (510.0, 470.0);

fn unavailable(e: tauri::Error) -> ApiError {
    ApiError::new(ErrorKind::Unavailable, format!("opening the control panel failed: {e}"))
}

/// `openController`: builds the panel, or hides the shown one and shows the hidden one. Returns
/// whether it is shown. Async: building a window inside a synchronous command deadlocks on Windows.
#[tauri::command]
pub(super) async fn toggle_control_panel(app: AppHandle, state: tauri::State<'_, State>, title: String) -> Result<bool, ApiError> {
    let Some(window) = app.get_webview_window(LABEL) else {
        build(&app, &state, title)?;
        return Ok(true);
    };
    if window.is_visible().map_err(unavailable)? {
        hide(&state, &window);
        return Ok(false);
    }
    window.show().map_err(unavailable)?;
    let _ = window.set_focus();
    Ok(true)
}

/// Lizzie.java:1026-1029: the panel shown when the application last closed opens again at start.
/// Returns whether it is shown.
#[tauri::command]
pub(super) async fn reopen_control_panel(app: AppHandle, state: tauri::State<'_, State>, title: String) -> Result<bool, ApiError> {
    if let Some(window) = app.get_webview_window(LABEL) {
        return window.is_visible().map_err(unavailable);
    }
    if !state.workbench.lock().unwrap().persisted().control_panel_open {
        return Ok(false);
    }
    build(&app, &state, title)?;
    Ok(true)
}

/// `new Controller(this)`: owned by the main window, at the bounds it last had, else Java's
/// default size centred on the main window (`setLocationRelativeTo(Lizzie.frame)`). The position
/// goes to the builder: a window manager may place a mapped window again over a later move.
fn build(app: &AppHandle, shared: &Shared, title: String) -> Result<(), ApiError> {
    let saved = shared.workbench.lock().unwrap().persisted().control_panel;
    let main = app.get_webview_window("main");
    let scale = main.as_ref().and_then(|m| m.scale_factor().ok()).unwrap_or(1.0);
    let outer = PhysicalSize::new((DEFAULT_OUTER.0 * scale) as u32, (DEFAULT_OUTER.1 * scale) as u32);
    let (position, size) = match saved {
        // The placement's size is the content's, as `super::window_placement` reads it.
        Some(p) => (Some(PhysicalPosition::new(p.x, p.y)), PhysicalSize::new(p.width.max(1), p.height.max(1))),
        None => {
            let centred = main.as_ref().and_then(|m| Some((m.outer_position().ok()?, m.outer_size().ok()?))).map(|(at, size)| {
                PhysicalPosition::new(at.x + (size.width as i32 - outer.width as i32) / 2, at.y + (size.height as i32 - outer.height as i32) / 2)
            });
            (centred, outer)
        }
    };
    let mut builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("control.html".into()))
        .title(title)
        .inner_size(f64::from(size.width) / scale, f64::from(size.height) / scale)
        .decorations(true)
        .resizable(true)
        .always_on_top(false)
        .disable_drag_drop_handler();
    if let Some(at) = position {
        builder = builder.position(f64::from(at.x) / scale, f64::from(at.y) / scale);
    }
    if let Some(main) = &main {
        builder = builder.parent(main).map_err(unavailable)?;
    }
    let window = builder.build().map_err(unavailable)?;
    if saved.is_none() {
        // Java's size is the whole window's: the frame comes off the content.
        let (o, i) = (window.outer_size().map_err(unavailable)?, window.inner_size().map_err(unavailable)?);
        let (dw, dh) = (o.width.saturating_sub(i.width), o.height.saturating_sub(i.height));
        if dw > 0 || dh > 0 {
            window.set_size(PhysicalSize::new(outer.width.saturating_sub(dw).max(1), outer.height.saturating_sub(dh).max(1))).map_err(unavailable)?;
        }
    }
    let _ = window.set_focus();
    Ok(())
}

/// Hides the panel (its close box, or the menu / toolbar while it is shown), keeping its bounds
/// for the next opening.
fn hide(shared: &Shared, window: &WebviewWindow) {
    let mut wb = shared.workbench.lock().unwrap();
    let placement = super::window_placement(window, wb.persisted().control_panel);
    if let Err(error) = wb.record_control_panel(placement, false) {
        shared.log.warn(&error.message);
    }
    drop(wb);
    let _ = window.hide();
}

/// The close box hides the panel (a JDialog's HIDE_ON_CLOSE) and the main window is told.
pub(super) fn closing(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        hide(&app.state::<State>(), &window);
        let _ = app.emit_to("main", CLOSED_EVENT, ());
    }
}

/// At exit (Config.persist, Lizzie.shutdown): the shown panel's bounds and whether it is shown.
/// A hidden panel's bounds were kept when it was hidden.
pub(super) fn capture(app: &AppHandle, shared: &Shared, wb: &mut Workbench) {
    let window = app.get_webview_window(LABEL).filter(|w| w.is_visible().unwrap_or(false));
    let placement = window.as_ref().and_then(|w| super::window_placement(w, wb.persisted().control_panel));
    if let Err(error) = wb.record_control_panel(placement, window.is_some()) {
        shared.log.warn(&error.message);
    }
}

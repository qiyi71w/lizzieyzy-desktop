//! Native floating board lifetime and Java outer-window placement.
use lizzie_core::api::{ApiError, ErrorKind};
use lizzie_core::settings::{IndependentBoard, WindowPlacement};
use lizzie_core::workbench::Workbench;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use super::{Shared, State};

pub(super) const MAIN: &str = "independent-main";
pub(super) const SUB: &str = "independent-sub";

pub(super) fn board(label: &str) -> Option<IndependentBoard> {
    match label { MAIN => Some(IndependentBoard::Main), SUB => Some(IndependentBoard::Sub), _ => None }
}
fn label(board: IndependentBoard) -> &'static str {
    match board { IndependentBoard::Main => MAIN, IndependentBoard::Sub => SUB }
}
fn placement(window: &WebviewWindow) -> Option<WindowPlacement> {
    let pos = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    Some(WindowPlacement { x: pos.x, y: pos.y, width: size.width, height: size.height, maximized: false })
}
pub(super) fn capture(app: &AppHandle, wb: &mut Workbench) -> Result<(), ApiError> {
    for board in [IndependentBoard::Main, IndependentBoard::Sub] {
        if let Some(window) = app.get_webview_window(label(board)) {
            wb.record_independent_board(board, placement(&window))?;
        }
    }
    Ok(())
}
pub(super) fn closing(shared: &Shared, window: &WebviewWindow, board: IndependentBoard) {
    let mut wb = shared.workbench.lock().unwrap();
    if let Err(error) = wb.record_independent_board(board, placement(window)) { shared.log.warn(&error.message); }
    if let Err(error) = wb.close_independent_board(board) { shared.log.warn(&error.message); }
}

fn set_outer(window: &WebviewWindow, p: WindowPlacement) -> Result<(), ApiError> {
    let fail = |e: tauri::Error| ApiError::new(ErrorKind::Unavailable, format!("placing a layout window failed: {e}"));
    if p.maximized { return window.maximize().map_err(fail); }
    window.unmaximize().map_err(fail)?;
    let outer = window.outer_size().map_err(fail)?;
    let inner = window.inner_size().map_err(fail)?;
    let width = p.width.saturating_sub(outer.width.saturating_sub(inner.width)).max(1);
    let height = p.height.saturating_sub(outer.height.saturating_sub(inner.height)).max(1);
    window.set_size(PhysicalSize::new(width, height)).map_err(fail)?;
    window.set_position(PhysicalPosition::new(p.x, p.y)).map_err(fail)
}

/// Java's independent board frames are undecorated, resizable and use screen-relative defaults.
#[tauri::command]
pub(super) async fn open_independent_board(app: AppHandle, state: tauri::State<'_, State>, board: IndependentBoard, title: String) -> Result<(), ApiError> {
    if let Some(window) = app.get_webview_window(label(board)) {
        window.set_title(&title).map_err(|e| ApiError::new(ErrorKind::Unavailable, e.to_string()))?;
        return Ok(());
    }
    let (saved, top) = {
        let wb = state.workbench.lock().unwrap();
        match board {
            IndependentBoard::Main => (wb.persisted().independent_main_board, wb.config().panels.independent_main_top),
            IndependentBoard::Sub => (wb.persisted().independent_sub_board, wb.config().panels.independent_sub_top),
        }
    };
    let monitor = app.primary_monitor().map_err(|e| ApiError::new(ErrorKind::Unavailable, e.to_string()))?
        .ok_or_else(|| ApiError::new(ErrorKind::Unavailable, "no primary monitor is available for the floating board"))?;
    let (screen_width, screen_height, scale) = (monitor.size().width, monitor.size().height, monitor.scale_factor());
    let size = if board == IndependentBoard::Main { 600.0 } else { 300.0 };
    let bounds = saved.unwrap_or(WindowPlacement {
        x: if board == IndependentBoard::Main { screen_width as i32 - (size * scale) as i32 } else { 0 },
        y: screen_height as i32 / 2 - (size * scale / 2.0) as i32,
        width: (size * scale) as u32, height: (size * scale) as u32, maximized: false,
    });
    let window = WebviewWindowBuilder::new(&app, label(board), WebviewUrl::App("board.html".into()))
        .title(title).decorations(false).resizable(true).always_on_top(top)
        .inner_size(f64::from(bounds.width) / scale, f64::from(bounds.height) / scale)
        .position(f64::from(bounds.x) / scale, f64::from(bounds.y) / scale)
        .disable_drag_drop_handler().build()
        .map_err(|e| ApiError::new(ErrorKind::Unavailable, format!("opening the floating board failed: {e}")))?;
    set_outer(&window, bounds)
}

/// Closing from a panel flag records placement but does not invoke the native-close mode policy.
#[tauri::command]
pub(super) fn close_independent_board_window(app: AppHandle, state: tauri::State<'_, State>, board: IndependentBoard) -> Result<(), ApiError> {
    if let Some(window) = app.get_webview_window(label(board)) {
        state.workbench.lock().unwrap().record_independent_board(board, placement(&window))?;
        window.destroy().map_err(|e| ApiError::new(ErrorKind::Unavailable, e.to_string()))?;
    }
    Ok(())
}

/// Apply saved native bounds after the core has committed the snapshot's flags.
pub(super) async fn apply(app: &AppHandle, shared: &Shared, slot: u8) -> Result<(), ApiError> {
    let layout = shared.workbench.lock().unwrap().config().custom_layouts[usize::from(slot - 1)].clone();
    let Some(layout) = layout else { return Ok(()); };
    if let (Some(window), Some(bounds)) = (app.get_webview_window("main"), layout.window) { set_outer(&window, bounds)?; }
    for (board, shown, bounds) in [
        (IndependentBoard::Main, layout.panels.showing_independent_main, layout.independent_main),
        (IndependentBoard::Sub, layout.panels.showing_independent_sub, layout.independent_sub),
    ] {
        if shown {
            open_independent_board(app.clone(), app.state::<State>(), board, String::new()).await?;
            if let (Some(window), Some(bounds)) = (app.get_webview_window(label(board)), bounds) { set_outer(&window, bounds)?; }
        }
    }
    Ok(())
}

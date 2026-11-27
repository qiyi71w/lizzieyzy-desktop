//! 黑白胜率大字 (Java `BigWinrateWindow`, a JDialog owned by the main frame): a decorated, resizable
//! window showing Black's and White's win rates in large figures. The main window opens and closes
//! it from 显示 > 黑白胜率大字, feeds it what to show and applies its 窗口置顶. Its bounds are kept
//! with its settings when it closes and when the application exits; closing it ends its page, so
//! nothing it listened to or timed outlives it.
use std::sync::Mutex;

use lizzie_core::api::{ApiError, ErrorKind};
use lizzie_core::settings::{BigWinrateFit, BigWinrateSettings};
use lizzie_core::workbench::Workbench;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use super::{Shared, State};

pub(super) const LABEL: &str = "big-winrate";
/// The window's close box closed it; the main window stops feeding it.
const CLOSED_EVENT: &str = "big-winrate-closed";
/// Java's first size, the whole window's (`setSize(520, 260)`), in logical pixels.
const DEFAULT_OUTER: [i32; 2] = [520, 260];
/// The smallest content: Java reads smaller bounds back as too small to use (`BigWinrateSettings`).
const MIN_INNER: (f64, f64) = (120.0, 60.0);
/// How much of saved bounds must lie on a screen that is still there (Java `onScreen`).
const VISIBLE: (f64, f64) = (80.0, 40.0);
/// The open window's fit to its kept whole size: on Linux, while it was opened with a kept frame.
static FIT: Mutex<Option<BigWinrateFit>> = Mutex::new(None);

fn unavailable(e: tauri::Error) -> ApiError {
    ApiError::new(ErrorKind::Unavailable, format!("opening 黑白胜率大字 failed: {e}"))
}

/// `toggleBigWinrateWindow`: opens the window, or closes the open one. Returns whether it is
/// open. Async: building a window inside a synchronous command deadlocks on Windows.
#[tauri::command]
pub(super) async fn toggle_big_winrate(app: AppHandle, state: tauri::State<'_, State>, title: String) -> Result<bool, ApiError> {
    if let Some(window) = app.get_webview_window(LABEL) {
        record(&state, &window);
        let _ = window.destroy();
        return Ok(false);
    }
    build(&app, &state, title)?;
    Ok(true)
}

/// `new BigWinrateWindow(owner, …)`: owned by the main window, at the bounds it last had while
/// enough of them lies on a screen; saved bounds off every screen come back to the primary
/// screen's corner, as the program's other tool windows do; without saved bounds, Java's size
/// centred on the main window (`setLocationRelativeTo(owner)`).
fn build(app: &AppHandle, shared: &Shared, title: String) -> Result<(), ApiError> {
    let settings = shared.workbench.lock().unwrap().config().big_winrate;
    let main = app.get_webview_window("main");
    let (at, outer) = match settings.bounds.filter(|b| BigWinrateSettings::usable(*b)) {
        Some([x, y, w, h]) => {
            let size = (f64::from(w), f64::from(h));
            let at = (f64::from(x), f64::from(y));
            if on_screen(app, at, size) { (Some(at), [w, h]) } else { (primary_corner(app), [w, h]) }
        }
        None => (main.as_ref().and_then(|m| centred(m, DEFAULT_OUTER)), DEFAULT_OUTER),
    };
    // Java's bounds are the whole window's: the frame comes off the content. GTK tells a window's
    // frame only once the window is shown (tao's sizes right after building are not the window's
    // yet), so on Linux the window opens with the frame kept with the bounds taken off, and is
    // refitted as GTK reports it (`resized`): the decorations may have changed since.
    #[cfg(target_os = "linux")]
    let fit = settings.frame.map(|frame| BigWinrateFit::new(outer, frame));
    #[cfg(not(target_os = "linux"))]
    let fit: Option<BigWinrateFit> = None;
    *FIT.lock().unwrap() = fit;
    let content = fit.map_or(outer, |fit| fit.content());
    let mut builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("bigwinrate.html".into()))
        .title(title)
        .inner_size(f64::from(content[0]), f64::from(content[1]))
        .min_inner_size(MIN_INNER.0, MIN_INNER.1)
        .decorations(true)
        .resizable(true)
        .always_on_top(settings.on_top)
        .disable_drag_drop_handler();
    if let Some((x, y)) = at {
        builder = builder.position(x, y);
    }
    if let Some(main) = &main {
        builder = builder.parent(main).map_err(unavailable)?;
    }
    let window = builder.build().map_err(unavailable)?;
    // On macOS the runtime sets the floating level before it makes the window a child of the main
    // window, and AppKit gives a child its parent's level: a window opened with 窗口置顶 would be an
    // ordinary one. The saved setting goes through the same switch as the right-click menu's, now
    // that the window is a child. Elsewhere the window was created on top already.
    #[cfg(target_os = "macos")]
    if settings.on_top {
        super::on_top::switch(&window, true)?;
    }
    if fit.is_none() {
        let scale = window.scale_factor().map_err(unavailable)?;
        let (o, i) = (window.outer_size().map_err(unavailable)?, window.inner_size().map_err(unavailable)?);
        let (dw, dh) = (f64::from(o.width.saturating_sub(i.width)) / scale, f64::from(o.height.saturating_sub(i.height)) / scale);
        if dw > 0.0 || dh > 0.0 {
            let size = LogicalSize::new((f64::from(outer[0]) - dw).max(MIN_INNER.0), (f64::from(outer[1]) - dh).max(MIN_INNER.1));
            window.set_size(size).map_err(unavailable)?;
        }
    }
    let _ = window.set_focus();
    Ok(())
}

/// The top-left corner that centres a window of `outer` (logical) on `main`.
fn centred(main: &WebviewWindow, outer: [i32; 2]) -> Option<(f64, f64)> {
    let scale = main.scale_factor().ok()?;
    let at = main.outer_position().ok()?.to_logical::<f64>(scale);
    let size = main.outer_size().ok()?.to_logical::<f64>(scale);
    Some((at.x + (size.width - f64::from(outer[0])) / 2.0, at.y + (size.height - f64::from(outer[1])) / 2.0))
}

/// Whether at least 80 x 40 of the bounds (logical) lies on one of the screens.
fn on_screen(app: &AppHandle, at: (f64, f64), size: (f64, f64)) -> bool {
    let Ok(monitors) = app.available_monitors() else { return true };
    monitors.iter().any(|m| {
        let scale = m.scale_factor();
        let p = m.position().to_logical::<f64>(scale);
        let s = m.size().to_logical::<f64>(scale);
        let w = (at.0 + size.0).min(p.x + s.width) - at.0.max(p.x);
        let h = (at.1 + size.1).min(p.y + s.height) - at.1.max(p.y);
        w >= VISIBLE.0 && h >= VISIBLE.1
    })
}

/// The primary screen's top-left corner (logical), where saved bounds off every screen go.
fn primary_corner(app: &AppHandle) -> Option<(f64, f64)> {
    let m = app.primary_monitor().ok().flatten()?;
    let p = m.position().to_logical::<f64>(m.scale_factor());
    Some((p.x, p.y))
}

/// The window's position, whole size and content in logical pixels.
fn measure(window: &WebviewWindow) -> Option<([i32; 2], [i32; 2], [i32; 2])> {
    let scale = window.scale_factor().ok()?;
    let at = window.outer_position().ok()?.to_logical::<i32>(scale);
    let size = window.outer_size().ok()?.to_logical::<i32>(scale);
    // The content: on Linux the page's allocation, as for the main window's placement (GTK's own
    // size there includes its header bar and shadow on Wayland).
    #[cfg(target_os = "linux")]
    let content = window.as_ref().bounds().ok()?.size.to_logical::<i32>(scale);
    #[cfg(not(target_os = "linux"))]
    let content = window.inner_size().ok()?.to_logical::<i32>(scale);
    Some(([at.x, at.y], [size.width, size.height], [content.width, content.height]))
}

/// The whole window's bounds in logical pixels, as Java keeps them, and its frame. A refitted
/// window the user has not resized keeps the whole size it was opened at.
fn bounds(window: &WebviewWindow) -> Option<([i32; 4], [i32; 2])> {
    let (at, outer, content) = measure(window)?;
    let frame = BigWinrateSettings::frame_of(outer, content);
    let [w, h] = FIT.lock().unwrap().map_or(outer, |fit| fit.kept_outer(outer, content));
    Some(([at[0], at[1], w, h], frame))
}

/// GTK reported the window's size: it is refitted while its frame is not the one it was opened
/// with and its size is still the one asked for. The content is GTK's own size of the window
/// less its client-side decorations, the size the content is asked for by, which is current as
/// GTK reports the window; the page is given its allocation only afterwards.
#[cfg(target_os = "linux")]
pub(super) fn resized(app: &AppHandle) {
    use gtk::prelude::GtkWindowExt;
    if FIT.lock().unwrap().is_none() {
        return;
    }
    let Some(window) = app.get_webview_window(LABEL) else { return };
    let (Ok(scale), Ok(size), Ok(gtk)) = (window.scale_factor(), window.outer_size(), window.gtk_window()) else { return };
    let outer = size.to_logical::<i32>(scale);
    let content = gtk.size();
    let asked = FIT.lock().unwrap().as_mut().and_then(|fit| fit.measured([outer.width, outer.height], [content.0, content.1]));
    if let Some([w, h]) = asked {
        let _ = window.set_size(LogicalSize::new(f64::from(w), f64::from(h)));
    }
}

/// The window lost the focus: it is the user's now and is not refitted any more.
#[cfg(target_os = "linux")]
pub(super) fn settled() {
    if let Some(fit) = FIT.lock().unwrap().as_mut() {
        fit.settle();
    }
}

/// Keeps the window's bounds as it closes; its fit ends with it.
fn record(shared: &Shared, window: &WebviewWindow) {
    let Some((b, frame)) = bounds(window) else { return };
    *FIT.lock().unwrap() = None;
    let result = shared.workbench.lock().unwrap().record_big_winrate_bounds(b, frame);
    if let Err(error) = result {
        shared.log.warn(&error.message);
    }
}

/// The close box (before the window goes): its bounds are kept and the main window is told.
pub(super) fn closing(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(LABEL) {
        record(&app.state::<State>(), &window);
    }
    let _ = app.emit_to("main", CLOSED_EVENT, ());
}

/// At exit: the open window's bounds.
pub(super) fn capture(app: &AppHandle, shared: &Shared, wb: &mut Workbench) {
    let Some((b, frame)) = app.get_webview_window(LABEL).as_ref().and_then(bounds) else { return };
    if let Err(error) = wb.record_big_winrate_bounds(b, frame) {
        shared.log.warn(&error.message);
    }
}

//! 抓取死活题 (Java `LizzieFrame.startCaptureTsumeGo` and its helper process): the main window is
//! minimized, the primary screen is captured, and a borderless full-screen window shows that
//! picture (dimmed as Java's translucent overlay dims the desktop) for the three clicks. The
//! overlay asks for the stones read in its marked area as the pointer moves; finishing reports the
//! captured problem to the main window, which loads it like any edit and opens 死活题. Escape, a
//! zero grid spacing or closing the overlay cancel: the main window comes back with the 抓取死活题
//! instructions, as after Java's `esc`.
//!
//! Java took a fresh screen capture on every pointer move over a translucent window; here the
//! picture is taken once, so the overlay itself is never part of what is read.

use std::sync::Mutex;
use std::time::Duration;

use lizzie_core::api::{ApiError, ErrorKind};
use lizzie_core::tsumego::capture::{self, CapturePreview, CapturedProblem, Picture, Selection};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::screen::{self, Screenshot};
use crate::State;

pub(crate) const WINDOW: &str = "tsumego-capture";
/// Payload: `CapturedProblem | null` (null: the area was never marked, nothing to load).
pub const DONE_EVENT: &str = "tsumego-capture-done";
pub const CANCELLED_EVENT: &str = "tsumego-capture-cancelled";

/// The picture of the capture in progress.
#[derive(Default)]
pub struct Capture(Mutex<Option<Screenshot>>);

fn unavailable(message: impl Into<String>) -> ApiError {
    ApiError::new(ErrorKind::Unavailable, message)
}

fn restore_main(app: &AppHandle) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.unminimize();
        let _ = main.show();
        let _ = main.set_focus();
    }
}

/// Ends the capture: the overlay goes, the main window comes back and hears `event`.
fn finish(app: &AppHandle, event: &str, problem: Option<CapturedProblem>) {
    app.state::<Capture>().0.lock().unwrap().take();
    if let Some(overlay) = app.get_webview_window(WINDOW) {
        let _ = overlay.destroy();
    }
    restore_main(app);
    if event == DONE_EVENT {
        let _ = app.emit_to("main", DONE_EVENT, problem);
    } else {
        let _ = app.emit_to("main", CANCELLED_EVENT, ());
    }
}

/// 开始抓取 / Shift+T. Async: building a window inside a synchronous command deadlocks on Windows.
#[tauri::command]
pub async fn start_tsumego_capture(app: AppHandle) -> Result<(), ApiError> {
    if let Some(overlay) = app.get_webview_window(WINDOW) {
        let _ = overlay.set_focus();
        return Ok(());
    }
    let monitor = app
        .primary_monitor()
        .ok()
        .flatten()
        .ok_or_else(|| unavailable("no primary screen"))?;
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.minimize();
    }
    let (pos, size, scale) = (*monitor.position(), *monitor.size(), monitor.scale_factor());
    let shot = tauri::async_runtime::spawn_blocking(move || {
        // Let the minimize animation uncover the screen.
        std::thread::sleep(Duration::from_millis(600));
        screen::capture(pos.x, pos.y, size.width, size.height, scale)
    })
    .await
    .map_err(|e| unavailable(e.to_string()))
    .and_then(|r| r.map_err(unavailable));
    let shot = match shot {
        Ok(shot) => shot,
        Err(e) => {
            restore_main(&app);
            return Err(e);
        }
    };
    *app.state::<Capture>().0.lock().unwrap() = Some(shot);
    let built = WebviewWindowBuilder::new(&app, WINDOW, WebviewUrl::App("capture.html".into()))
        .title("")
        .decorations(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .position(f64::from(pos.x) / scale, f64::from(pos.y) / scale)
        .inner_size(f64::from(size.width) / scale, f64::from(size.height) / scale)
        .fullscreen(true)
        .focused(true)
        .disable_drag_drop_handler()
        .build();
    if let Err(e) = built {
        app.state::<Capture>().0.lock().unwrap().take();
        restore_main(&app);
        return Err(unavailable(format!("opening the capture window failed: {e}")));
    }
    Ok(())
}

/// The captured picture: width and height (little-endian `u32`), then RGBA pixels.
#[tauri::command]
pub fn tsumego_capture_picture(capture: tauri::State<'_, Capture>) -> Result<tauri::ipc::Response, ApiError> {
    let guard = capture.0.lock().unwrap();
    let shot = guard.as_ref().ok_or_else(|| unavailable("no capture in progress"))?;
    let mut bytes = Vec::with_capacity(8 + shot.rgba.len());
    bytes.extend_from_slice(&shot.width.to_le_bytes());
    bytes.extend_from_slice(&shot.height.to_le_bytes());
    bytes.extend_from_slice(&shot.rgba);
    Ok(tauri::ipc::Response::new(bytes))
}

fn read<T>(app: &AppHandle, f: impl FnOnce(&Picture) -> T) -> Option<T> {
    let capture = app.state::<Capture>();
    let guard = capture.0.lock().unwrap();
    let shot = guard.as_ref()?;
    Some(f(&Picture {
        width: shot.width,
        height: shot.height,
        rgba: &shot.rgba,
    }))
}

fn settings(app: &AppHandle) -> lizzie_core::settings::CaptureSettings {
    app.state::<State>().workbench.lock().unwrap().capture_settings()
}

/// Step 3's marked area from the origin to `cursor` (picture pixels) with the stones read in it;
/// `None` while it is narrower than half a square.
#[tauri::command]
pub fn tsumego_capture_preview(
    app: AppHandle,
    origin: (i32, i32),
    gap: (i32, i32),
    cursor: (i32, i32),
) -> Option<CapturePreview> {
    let selection = Selection::new(origin, gap, cursor)?;
    let settings = settings(&app);
    read(&app, |picture| selection.preview(picture, &settings))
}

/// Step 2's click: a zero spacing cancels like Java's `quit`.
#[tauri::command]
pub async fn tsumego_capture_spacing(app: AppHandle, origin: (i32, i32), at: (i32, i32)) -> Option<(i32, i32)> {
    let gap = capture::spacing(origin, at);
    if gap.is_none() {
        finish(&app, CANCELLED_EVENT, None);
    }
    gap
}

/// Step 3's click (`close`): the main window loads what was read; an area never marked loads
/// nothing.
#[tauri::command]
pub async fn tsumego_capture_finish(app: AppHandle, origin: (i32, i32), first: (i32, i32), cursor: (i32, i32)) {
    let problem = capture::spacing(origin, first).and_then(|gap| {
        let selection = Selection::new(origin, gap, cursor)?;
        let settings = settings(&app);
        read(&app, |picture| {
            selection.problem(capture::direction(origin, first), picture, &settings)
        })
    });
    finish(&app, DONE_EVENT, problem);
}

/// Escape.
#[tauri::command]
pub async fn tsumego_capture_cancel(app: AppHandle) {
    finish(&app, CANCELLED_EVENT, None);
}

/// The overlay's own close request cancels the capture.
pub fn overlay_closing(app: &AppHandle) {
    finish(app, CANCELLED_EVENT, None);
}

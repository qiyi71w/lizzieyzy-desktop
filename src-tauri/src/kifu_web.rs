//! What the 野狐棋谱 and 腾讯棋谱 windows share on the platform side: Java's HTTP request over the
//! configured proxy, and how a kifu window is shown and stepped aside after a record opens.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use lizzie_core::api::{ApiError, ErrorKind};
use lizzie_core::kifu_web::{form_encode, Http};
use lizzie_core::settings::NetworkSettings;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// `HTTP_CONNECT_TIMEOUT_MS`. Java's 25 s read timeout bounds each read; a blocking client has
/// only a whole-request limit, the sum of both.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
/// `HTTP_MAX_RETRIES`, with a 350 ms × attempt pause after a failed attempt.
const ATTEMPTS: u32 = 3;

/// Java's `httpRequest` / `httpGet` over the configured proxy: the body of any status as UTF-8
/// (lossy), up to three attempts on transport failures. `cancelled` stops further attempts once
/// the search is replaced.
pub struct Transport {
    client: reqwest::blocking::Client,
    cancelled: Arc<AtomicBool>,
}

impl Transport {
    pub fn new(network: &NetworkSettings, cancelled: Arc<AtomicBool>) -> Result<Transport, String> {
        let builder = reqwest::blocking::Client::builder()
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT);
        let client = crate::resources::with_proxy(builder, network)?
            .build()
            .map_err(|e| e.without_url().to_string())?;
        Ok(Transport { client, cancelled })
    }

    fn send(&self, request: impl Fn() -> reqwest::blocking::RequestBuilder) -> Result<String, String> {
        let mut last = String::new();
        for attempt in 1..=ATTEMPTS {
            if self.cancelled.load(Ordering::Acquire) {
                return Err("cancelled".into());
            }
            let outcome = request()
                .header(reqwest::header::ACCEPT, "application/json,text/plain,*/*")
                .header(reqwest::header::CONNECTION, "close")
                .send()
                .and_then(|reply| Ok((reply.status(), reply.bytes()?)));
            match outcome {
                Ok((status, body)) if body.is_empty() && !(status.is_success() || status.is_redirection()) => {
                    last = format!("HTTP {} with empty body", status.as_u16());
                }
                Ok((_, body)) => return Ok(String::from_utf8_lossy(&body).into_owned()),
                Err(e) => last = e.without_url().to_string(),
            }
            if attempt < ATTEMPTS {
                std::thread::sleep(Duration::from_millis(350 * u64::from(attempt)));
            }
        }
        Err(last)
    }
}

impl Http for Transport {
    fn get(&mut self, url: &str, user_agent: &str) -> Result<String, String> {
        self.send(|| self.client.get(url).header(reqwest::header::USER_AGENT, user_agent))
    }

    fn post_form(&mut self, url: &str, form: &[(&str, &str)], user_agent: &str) -> Result<String, String> {
        let body = form
            .iter()
            .map(|(k, v)| format!("{}={}", form_encode(k), form_encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        self.send(|| {
            self.client
                .post(url)
                .header(reqwest::header::USER_AGENT, user_agent)
                .header(reqwest::header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(body.clone())
        })
    }
}

/// `presentWindow`: the one window `label` (page `page`), 980 x 680 centred on the main window and
/// kept on the screen, shown again (also from minimized) when it exists. Async callers only:
/// building a window inside a synchronous command deadlocks on Windows.
pub fn present(app: &AppHandle, label: &str, page: &str, title: String, on_top: bool) -> Result<(), ApiError> {
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.set_title(&title);
        let _ = window.set_always_on_top(on_top);
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    let (mut width, mut height) = (980.0_f64, 680.0_f64);
    let main = app.get_webview_window("main");
    let mut position = None;
    let mut bounds = None;
    if let Some(main) = &main {
        if let (Ok(scale), Ok(pos), Ok(size)) = (main.scale_factor(), main.outer_position(), main.outer_size()) {
            let (pos, size) = (pos.to_logical::<f64>(scale), size.to_logical::<f64>(scale));
            position = Some((pos.x, pos.y, size.width, size.height));
        }
        if let Ok(Some(monitor)) = main.current_monitor() {
            let work = monitor.work_area();
            let scale = monitor.scale_factor();
            let (origin, size) = (
                work.position.to_logical::<f64>(scale),
                work.size.to_logical::<f64>(scale),
            );
            width = width.min(size.width.max(1.0));
            height = height.min(size.height.max(1.0));
            bounds = Some((origin.x, origin.y, size.width, size.height));
        }
    }
    let mut builder = WebviewWindowBuilder::new(app, label, WebviewUrl::App(page.into()))
        .title(title)
        .inner_size(width, height)
        .resizable(true)
        .always_on_top(on_top)
        .disable_drag_drop_handler();
    if let Some((x, y, w, h)) = position.or(bounds) {
        let (mut x, mut y) = (x + (w - width) / 2.0, y + (h - height) / 2.0);
        if let Some((bx, by, bw, bh)) = bounds {
            x = x.clamp(bx, (bx + bw - width).max(bx));
            y = y.clamp(by, (by + bh - height).max(by));
        }
        builder = builder.position(x, y);
    }
    builder.build().map_err(|e| {
        ApiError::new(
            ErrorKind::Unavailable,
            format!("opening the {label} window failed: {e}"),
        )
    })?;
    Ok(())
}

/// 打开棋谱后此窗口 after a record opened: 最小化 or 关闭 (hidden: the window keeps its search, as
/// Java's frame does), then the main window comes forward.
pub fn after_open(app: &AppHandle, label: &str, minimize: bool, close: bool) {
    if let Some(window) = app.get_webview_window(label) {
        if minimize {
            let _ = window.minimize();
        } else if close {
            let _ = window.hide();
        }
    }
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.unminimize();
        let _ = main.show();
        let _ = main.set_focus();
    }
}

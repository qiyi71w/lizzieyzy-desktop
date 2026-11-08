//! 野狐棋谱 (`FoxKifuDownload`, `GetFoxRequest`): the window and the HTTP worker. Each search has
//! one serial worker (Java's single-thread executor); a new search cancels the previous one's
//! queued requests, and the core drops whatever an older search still delivers.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lizzie_core::api::{ApiError, ErrorKind};
use lizzie_core::fox_kifu::{self, Http};
use lizzie_core::settings::NetworkSettings;
use lizzie_core::workbench::FoxKifuJob;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::{publish, State};

/// The window label (`fox-kifu`, foxkifu.html).
pub const WINDOW: &str = "fox-kifu";

/// `HTTP_CONNECT_TIMEOUT_MS`. Java's 25 s read timeout bounds each read; a blocking client has
/// only a whole-request limit, the sum of both.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(45);
/// `HTTP_MAX_RETRIES`, with a 350 ms × attempt pause after a failed attempt.
const ATTEMPTS: u32 = 3;

#[derive(Default)]
pub struct Adapter {
    worker: Mutex<Option<Worker>>,
}

struct Worker {
    session: u64,
    jobs: Sender<FoxKifuJob>,
    cancelled: Arc<AtomicBool>,
}

/// Hands the requests to the worker of their search, replacing an older search's worker. Two
/// publications may hand theirs over in either order: a late job of an older search is dropped
/// (the core ignores that search's replies) and never replaces a newer search's worker.
pub fn spawn_jobs(app: &AppHandle, jobs: Vec<FoxKifuJob>) {
    if jobs.is_empty() {
        return;
    }
    let adapter = app.state::<Adapter>();
    let mut worker = adapter.worker.lock().unwrap();
    for job in jobs {
        match worker.as_ref().map(|w| w.session) {
            Some(current) if job.session < current => continue,
            Some(current) if job.session == current => {}
            _ => {
                if let Some(old) = worker.take() {
                    old.cancelled.store(true, Ordering::Release);
                }
                *worker = Some(start(app.clone(), job.session));
            }
        }
        let _ = worker.as_ref().unwrap().jobs.send(job);
    }
}

fn start(app: AppHandle, session: u64) -> Worker {
    let (tx, rx) = mpsc::channel::<FoxKifuJob>();
    let cancelled = Arc::new(AtomicBool::new(false));
    let flag = cancelled.clone();
    std::thread::spawn(move || {
        let shared = app.state::<State>().inner().clone();
        while let Ok(job) = rx.recv() {
            if flag.load(Ordering::Acquire) {
                return;
            }
            let network = shared.workbench.lock().unwrap().config().network.clone();
            let reply = match Transport::new(&network, flag.clone()) {
                Ok(mut http) => fox_kifu::fetch(&job.request, &mut http, job.no_capture),
                Err(message) => fox_kifu::Reply::Failed {
                    message,
                    user_not_found: false,
                },
            };
            // The searched account and Fox's reply text stay out of the log.
            if matches!(reply, fox_kifu::Reply::Failed { .. }) {
                let kind = match job.request {
                    fox_kifu::Request::Nickname { .. } | fox_kifu::Request::Account { .. } => "search",
                    fox_kifu::Request::Page { .. } => "page",
                    fox_kifu::Request::Record { .. } => "record",
                };
                shared.log.warn(&format!("fox kifu {kind} request failed"));
            }
            if flag.load(Ordering::Acquire) {
                return;
            }
            if shared.workbench.lock().unwrap().fox_kifu_reply(&job, reply) {
                publish(&app, &shared);
            }
        }
    });
    Worker {
        session,
        jobs: tx,
        cancelled,
    }
}

/// Java's `httpRequest` over the configured proxy: the body of any status as UTF-8 (lossy), up to
/// three attempts on transport failures.
struct Transport {
    client: reqwest::blocking::Client,
    cancelled: Arc<AtomicBool>,
}

impl Transport {
    fn new(network: &NetworkSettings, cancelled: Arc<AtomicBool>) -> Result<Transport, String> {
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
            .map(|(k, v)| format!("{}={}", fox_kifu::form_encode(k), fox_kifu::form_encode(v)))
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

/// `openFoxReq` → `presentWindow`: the one 野狐棋谱 window, 980 x 680 centred on the main window
/// and kept on the screen, shown again (also from minimized) when it exists. Async: building a
/// window inside a synchronous command deadlocks on Windows.
#[tauri::command]
pub async fn open_fox_kifu(app: AppHandle, title: String, on_top: bool) -> Result<(), ApiError> {
    if let Some(window) = app.get_webview_window(WINDOW) {
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
    let mut builder = WebviewWindowBuilder::new(&app, WINDOW, WebviewUrl::App("foxkifu.html".into()))
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
            format!("opening the Fox kifu window failed: {e}"),
        )
    })?;
    Ok(())
}

/// 打开棋谱后此窗口 after a record opened: 最小化 or 关闭 (hidden: the window keeps its search, as
/// Java's frame does), then the main window comes forward.
#[tauri::command]
pub fn fox_kifu_opened(app: AppHandle, minimize: bool, close: bool) {
    if let Some(window) = app.get_webview_window(WINDOW) {
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

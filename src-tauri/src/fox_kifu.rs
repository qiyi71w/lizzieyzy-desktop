//! 野狐棋谱 (`FoxKifuDownload`, `GetFoxRequest`): the window and the HTTP worker. Each search has
//! one serial worker (Java's single-thread executor); a new search cancels the previous one's
//! queued requests, and the core drops whatever an older search still delivers.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

use lizzie_core::api::ApiError;
use lizzie_core::fox_kifu;
use lizzie_core::workbench::FoxKifuJob;
use tauri::{AppHandle, Manager};

use crate::kifu_web::{self, Transport};
use crate::{publish, State};

/// The window label (`fox-kifu`, foxkifu.html).
pub const WINDOW: &str = "fox-kifu";

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

/// `openFoxReq` → `presentWindow`.
#[tauri::command]
pub async fn open_fox_kifu(app: AppHandle, title: String, on_top: bool) -> Result<(), ApiError> {
    kifu_web::present(&app, WINDOW, "foxkifu.html", title, on_top)
}

/// 打开棋谱后此窗口 after a record opened (`completeFoxKifuLoad`).
#[tauri::command]
pub fn fox_kifu_opened(app: AppHandle, minimize: bool, close: bool) {
    kifu_web::after_open(&app, WINDOW, minimize, close);
}

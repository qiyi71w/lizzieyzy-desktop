//! OGS 棋谱 (Java `OgsKifuDownload`, `OgsSgfClient.readResponse`): the window and the HTTP worker.
//! Each request runs on its own thread with its own cancel flag; a request the workbench no longer
//! wants when its thread starts sends nothing (its `Cancel` may have come before it was
//! registered), a cancelled request sends no further GET, and the core drops whatever it still
//! delivers. Addresses carry the searched username, so only the host reaches the log.
use std::collections::HashMap;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use lizzie_core::api::{ApiError, OgsKifuCommand};
use lizzie_core::ogs_kifu::{self, Http, Reply, Response};
use lizzie_core::settings::NetworkSettings;
use lizzie_core::workbench::{OgsKifuJob, OgsKifuTask, Workbench};
use tauri::{AppHandle, Manager};

use crate::{kifu_web, publish, State};

/// The window label (`ogs-kifu`, ogskifu.html).
pub const WINDOW: &str = "ogs-kifu";

#[derive(Default)]
pub struct Adapter {
    running: Arc<Mutex<HashMap<u64, Arc<AtomicBool>>>>,
}

/// What a request's thread works with: the workbench it asks and answers, the log, and the view
/// to publish when a reply changed something.
trait Host: Clone + Send + 'static {
    fn workbench(&self) -> &Mutex<Workbench>;
    fn warn(&self, message: &str);
    fn changed(&self);
}

#[derive(Clone)]
struct AppHost {
    app: AppHandle,
    shared: State,
}

impl Host for AppHost {
    fn workbench(&self) -> &Mutex<Workbench> {
        &self.shared.workbench
    }
    fn warn(&self, message: &str) {
        self.shared.log.warn(message);
    }
    fn changed(&self) {
        publish(&self.app, &self.shared);
    }
}

impl Adapter {
    /// A request that has not started yet has no flag; it then finds itself unwanted.
    fn cancel(&self, id: u64) {
        if let Some(flag) = self.running.lock().unwrap().remove(&id) {
            flag.store(true, Ordering::Release);
        }
    }

    fn register(&self, id: u64) -> Arc<AtomicBool> {
        let cancelled = Arc::new(AtomicBool::new(false));
        self.running.lock().unwrap().insert(id, cancelled.clone());
        cancelled
    }

    /// Carries out the workbench's tasks in order against the API root `base`: a `Cancel` sets
    /// its request's flag, a `Fetch` runs on its own thread (returned, for whoever waits for it).
    fn dispatch<H: Host>(&self, tasks: Vec<OgsKifuTask>, host: &H, base: &str) -> Vec<JoinHandle<()>> {
        let mut threads = Vec::new();
        for task in tasks {
            match task {
                OgsKifuTask::Cancel(id) => self.cancel(id),
                OgsKifuTask::Fetch(job) => {
                    let cancelled = self.register(job.id);
                    let running = self.running.clone();
                    let host = host.clone();
                    let base = base.to_string();
                    threads.push(std::thread::spawn(move || {
                        let reply = run(host.workbench(), &job, &cancelled, &base);
                        running.lock().unwrap().remove(&job.id);
                        let Some(reply) = reply else {
                            return;
                        };
                        if let Reply::Failed(failure) = &reply {
                            host.warn(&format!("ogs kifu request failed: {failure:?}"));
                        }
                        if cancelled.load(Ordering::Acquire) {
                            return;
                        }
                        if host.workbench().lock().unwrap().ogs_kifu_reply(&job, reply) {
                            host.changed();
                        }
                    }));
                }
            }
        }
        threads
    }
}

/// The API root: OGS, or in acceptance builds the local stand-in `LIZZIEYZY_TAURI_OGS_BASE`.
fn base() -> String {
    #[cfg(feature = "acceptance")]
    if let Ok(base) = std::env::var("LIZZIEYZY_TAURI_OGS_BASE") {
        return base;
    }
    ogs_kifu::API.to_string()
}

pub fn run_tasks(app: &AppHandle, tasks: Vec<OgsKifuTask>) {
    if tasks.is_empty() {
        return;
    }
    let host = AppHost {
        app: app.clone(),
        shared: app.state::<State>().inner().clone(),
    };
    // The request threads run on their own.
    drop(app.state::<Adapter>().dispatch(tasks, &host, &base()));
}

/// Runs `job` against the API root `base`, or sends nothing (`None`) when the workbench no longer
/// wants it. The check and the network settings are read under one lock: the core drops a request
/// and queues its `Cancel` under that lock, so whatever order the two task lists reach the
/// adapter in, a dropped request is either seen here or has its flag set by the `Cancel`.
fn run(workbench: &Mutex<Workbench>, job: &OgsKifuJob, cancelled: &Arc<AtomicBool>, base: &str) -> Option<Reply> {
    let network = {
        let wb = workbench.lock().unwrap();
        if !wb.ogs_kifu_wanted(job.id) {
            return None;
        }
        wb.config().network.clone()
    };
    Some(match Transport::new(&network, cancelled.clone()) {
        Ok(mut http) => ogs_kifu::fetch(&job.request, base, &mut http, job.no_capture),
        Err(_) => Reply::Failed(ogs_kifu::OgsFailure::Network),
    })
}

/// `readResponse`: one GET over the configured proxy, redirects not followed, 10 s to connect and
/// 10 s more to read (a blocking client has only a whole-request limit, the sum of both). The
/// status decides first: only a 200 reply's body is read, at most `MAX_BYTES` + 1 bytes.
struct Transport {
    client: reqwest::blocking::Client,
    cancelled: Arc<AtomicBool>,
}

impl Transport {
    fn new(network: &NetworkSettings, cancelled: Arc<AtomicBool>) -> Result<Transport, String> {
        let builder = reqwest::blocking::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(ogs_kifu::CONNECT_TIMEOUT)
            .timeout(ogs_kifu::CONNECT_TIMEOUT + ogs_kifu::READ_TIMEOUT);
        let client = crate::resources::with_proxy(builder, network)?
            .build()
            .map_err(|e| e.without_url().to_string())?;
        Ok(Transport { client, cancelled })
    }
}

impl Http for Transport {
    fn get(&mut self, url: &str) -> Result<Response, String> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err("cancelled".into());
        }
        let started = Instant::now();
        let observe = |status: Option<u16>| {
            crate::logging::network(&lizzie_core::diagnostics::NetworkExchange {
                remote: false,
                method: "GET",
                host: url,
                category: lizzie_core::diagnostics::EndpointCategory::Other,
                status,
                latency_ms: started.elapsed().as_millis() as u64,
                outcome: if status == Some(200) { "ok" } else { "failed" },
            })
        };
        let reply = self
            .client
            .get(url)
            .header(reqwest::header::USER_AGENT, ogs_kifu::USER_AGENT)
            .header(reqwest::header::ACCEPT, ogs_kifu::ACCEPT)
            .send();
        let reply = match reply {
            Ok(reply) => reply,
            Err(e) => {
                observe(None);
                return Err(e.without_url().to_string());
            }
        };
        let status = reply.status().as_u16();
        if status != 200 {
            // `getResponseCode() != 200` throws before the body: a 429 or 404 whose body breaks
            // off or never comes is still that status.
            observe(Some(status));
            return Ok(Response {
                status,
                body: Vec::new(),
            });
        }
        let mut body = Vec::new();
        if let Err(e) = reply.take(ogs_kifu::MAX_BYTES as u64 + 1).read_to_end(&mut body) {
            observe(None);
            return Err(e.to_string());
        }
        observe(Some(status));
        Ok(Response { status, body })
    }
}

/// `openKifu(true)` → `presentWindow`.
#[tauri::command]
pub async fn open_ogs_kifu(app: AppHandle, title: String, on_top: bool) -> Result<(), ApiError> {
    kifu_web::present(&app, WINDOW, "ogskifu.html", title, on_top)
}

/// The window was closed (its close box, Escape, or 打开棋谱后此窗口 关闭): it hides and its
/// request is cancelled; the next opening starts empty (Java disposes the window).
pub fn closing(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(WINDOW) {
        let _ = window.hide();
    }
    let shared = app.state::<State>().inner().clone();
    let changed = shared.workbench.lock().unwrap().ogs_kifu(OgsKifuCommand::Close);
    if changed.is_ok() {
        publish(app, &shared);
    }
}

/// 打开棋谱后此窗口 after a record opened.
#[tauri::command]
pub fn ogs_kifu_opened(app: AppHandle, minimize: bool, close: bool) {
    if close {
        closing(&app);
    }
    kifu_web::after_open(&app, WINDOW, minimize, false);
}

#[cfg(test)]
mod tests {
    use super::*;
    use lizzie_core::engine::gtp::LaunchSpec;
    use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
    use lizzie_core::ogs_kifu::{OgsFailure, Request};
    use lizzie_core::settings::{ProxyMode, Storage};
    use std::io::Write;
    use std::net::{Shutdown, TcpListener};
    use std::sync::atomic::AtomicUsize;
    use std::sync::mpsc;

    /// How the stand-in ends a reply after its head.
    #[derive(Clone, Copy, Debug)]
    enum Body {
        /// The connection closes right after the head.
        Closed,
        /// Nothing more is sent until the test releases the stand-in (drops it).
        Held,
    }

    /// A local stand-in for OGS. Each request line is reported before anything is written back,
    /// and a held reply is reported before its head goes out, so once the client has returned
    /// both reports are there to read.
    struct StandIn {
        base: String,
        requests: mpsc::Receiver<String>,
        holding: mpsc::Receiver<()>,
        _release: mpsc::Sender<()>,
    }

    fn stand_in(head: &'static [u8], body: Body) -> StandIn {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}/api/v1/", listener.local_addr().unwrap());
        let (asked, requests) = mpsc::channel();
        let (held, holding) = mpsc::channel();
        let (release, released) = mpsc::channel::<()>();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut request = Vec::new();
                let mut buffer = [0; 4096];
                while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                    match stream.read(&mut buffer) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => request.extend_from_slice(&buffer[..n]),
                    }
                }
                let line = String::from_utf8_lossy(&request)
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_string();
                let _ = asked.send(line);
                if let Body::Held = body {
                    let _ = held.send(());
                }
                let _ = stream.write_all(head);
                if let Body::Held = body {
                    // Until the test drops its end.
                    let _ = released.recv();
                }
                let _ = stream.shutdown(Shutdown::Both);
            }
        });
        StandIn {
            base,
            requests,
            holding,
            _release: release,
        }
    }

    fn direct() -> NetworkSettings {
        NetworkSettings {
            proxy_mode: ProxyMode::Direct,
            ..NetworkSettings::default()
        }
    }

    /// The status is told as soon as it arrives: a 429 or a player ID's 404 whose announced body
    /// breaks off, or is still held back when the request returns, is not a network error, and
    /// the request is sent once. A request that gets no status at all still is one.
    #[test]
    fn a_status_is_told_even_when_its_body_never_comes() {
        const LIMITED: &[u8] =
            b"HTTP/1.1 429 Too Many Requests\r\nContent-Type: application/json\r\nContent-Length: 64\r\nRetry-After: 30\r\n\r\n";
        const NOT_FOUND: &[u8] =
            b"HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: 64\r\n\r\n";
        let cases: [(&'static [u8], Body, &str, fn(&Reply) -> bool); 5] = [
            (LIMITED, Body::Closed, "limited", |r| {
                matches!(r, Reply::Failed(OgsFailure::RateLimited))
            }),
            (LIMITED, Body::Held, "limited", |r| {
                matches!(r, Reply::Failed(OgsFailure::RateLimited))
            }),
            (NOT_FOUND, Body::Closed, "987654321", |r| matches!(r, Reply::NoPlayer)),
            (NOT_FOUND, Body::Held, "987654321", |r| matches!(r, Reply::NoPlayer)),
            (b"", Body::Closed, "offline", |r| {
                matches!(r, Reply::Failed(OgsFailure::Network))
            }),
        ];
        for (head, body, query, expected) in cases {
            let server = stand_in(head, body);
            let mut http = Transport::new(&direct(), Arc::new(AtomicBool::new(false))).unwrap();
            let got = ogs_kifu::fetch(&Request::Search { query: query.into() }, &server.base, &mut http, false);
            assert!(expected(&got), "{query} {body:?}: {got:?}");
            if let Body::Held = body {
                // The body is released only after this, when `server` is dropped.
                assert_eq!(server.holding.try_recv(), Ok(()), "{query}: the body was held back");
            }
            assert_eq!(server.requests.try_iter().count(), 1, "{query} {body:?}");
        }
    }

    struct NoEngines;
    impl Launcher for NoEngines {
        fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
            Err(std::io::Error::other("no engines"))
        }
    }

    /// The workbench, and how many replies changed its view.
    #[derive(Clone)]
    struct TestHost {
        workbench: Arc<Mutex<Workbench>>,
        changed: Arc<AtomicUsize>,
    }

    impl Host for TestHost {
        fn workbench(&self) -> &Mutex<Workbench> {
            &self.workbench
        }
        fn warn(&self, _: &str) {}
        fn changed(&self) {
            self.changed.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn join(threads: Vec<JoinHandle<()>>) {
        for thread in threads {
            thread.join().unwrap();
        }
    }

    /// Two publishes race: the one carrying the window's `Cancel` reaches the adapter before the
    /// one carrying the `Fetch` it cancels. The request is still never sent and nothing is
    /// published; a request still wanted is sent once and its reply published.
    #[test]
    fn a_request_cancelled_before_it_is_registered_sends_nothing() {
        let server = stand_in(
            b"HTTP/1.1 200 OK\r\nContent-Length: 14\r\n\r\n{\"results\":[]}",
            Body::Closed,
        );
        let dir = tempfile::tempdir().unwrap();
        let host = TestHost {
            workbench: Arc::new(Mutex::new(Workbench::new(
                Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data")),
                None,
                Box::new(NoEngines),
                Vec::new(),
            ))),
            changed: Arc::default(),
        };
        let adapter = Adapter::default();
        let ogs = |op: OgsKifuCommand| {
            let mut wb = host.workbench.lock().unwrap();
            wb.ogs_kifu(op).unwrap();
            wb.take_ogs_kifu_tasks()
        };

        let search = ogs(OgsKifuCommand::Search { text: "alice".into() });
        let close = ogs(OgsKifuCommand::Close);
        let [OgsKifuTask::Fetch(job)] = search.as_slice() else {
            panic!("one request")
        };
        assert_eq!(close, vec![OgsKifuTask::Cancel(job.id)]);
        join(adapter.dispatch(close, &host, &server.base));
        join(adapter.dispatch(search, &host, &server.base));
        assert_eq!(server.requests.try_iter().count(), 0, "no request reached OGS");
        assert_eq!(host.changed.load(Ordering::SeqCst), 0, "nothing published");

        join(adapter.dispatch(ogs(OgsKifuCommand::Search { text: "bob".into() }), &host, &server.base));
        assert_eq!(
            server.requests.try_iter().collect::<Vec<_>>(),
            ["GET /api/v1/players/?username=bob HTTP/1.1"]
        );
        assert_eq!(host.changed.load(Ordering::SeqCst), 1);
    }
}

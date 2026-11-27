//! A `Workbench` whose engines launch through the product `CloudConnector` (the custom WebSocket
//! transport and its GTP adapter), talking to a loopback WebSocket peer scripted by the test.
//! Engine events arrive on the connector's threads and are fed to `Workbench::engine_input` on
//! the test thread, as the desktop shell does.

use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use lizzie_core::api::OpenSource;
use lizzie_core::cloud::{CloudSettings, Provider};
use lizzie_core::engine::cloud::CloudConnector;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats::sgf;
use lizzie_core::record::RecordFormat;
use lizzie_core::settings::Storage;
use lizzie_core::workbench::{OpenOrigin, Workbench};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::{accept, Message};

/// Launches only cloud profiles, through the product connector, counting the lines the workbench
/// writes to them.
struct WsLauncher {
    connector: CloudConnector,
    tx: Mutex<mpsc::Sender<(InstanceId, EngineInput)>>,
    endings: Arc<Mutex<Vec<JoinHandle<()>>>>,
    written: Arc<AtomicUsize>,
}

impl Launcher for WsLauncher {
    fn launch(&self, id: InstanceId, spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        let Some(cloud) = &spec.cloud else {
            return Err(std::io::Error::other("no local engines in this test"));
        };
        let tx = Mutex::new(self.tx.lock().unwrap().clone());
        let io = self.connector.launch(
            id,
            cloud,
            Arc::new(move |i, e| {
                let _ = tx.lock().unwrap().send((i, e));
            }),
            self.endings.clone(),
        )?;
        Ok(Box::new(Counted {
            io,
            written: self.written.clone(),
        }))
    }
}

/// A transport's input, counting every line written to it.
struct Counted {
    io: Box<dyn EngineIo>,
    written: Arc<AtomicUsize>,
}

impl EngineIo for Counted {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        self.io.send(line)?;
        self.written.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn analysis_progress_accepted(&mut self, total: u64) {
        self.io.analysis_progress_accepted(total);
    }

    fn terminate(self: Box<Self>) {
        self.io.terminate();
    }
}

/// A loopback WebSocket peer: every text frame received is recorded with its connection number
/// and passed to `respond`, whose frames are written back. Each transport (a restarted engine)
/// is a new connection.
pub struct Peer {
    pub url: String,
    received: Arc<Mutex<Vec<(usize, Value)>>>,
    connections: Arc<AtomicUsize>,
}

type Respond = Arc<Mutex<Box<dyn FnMut(&Value) -> Vec<Value> + Send>>>;

pub fn peer(respond: impl FnMut(&Value) -> Vec<Value> + Send + 'static) -> Peer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("ws://{}/analysis", listener.local_addr().unwrap());
    let received = Arc::new(Mutex::new(Vec::new()));
    let connections = Arc::new(AtomicUsize::new(0));
    let respond: Respond = Arc::new(Mutex::new(Box::new(respond)));
    let (record, count) = (received.clone(), connections.clone());
    thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            let connection = count.fetch_add(1, Ordering::SeqCst) + 1;
            let (record, respond) = (record.clone(), respond.clone());
            thread::spawn(move || serve(stream, connection, &record, &respond));
        }
    });
    Peer {
        url,
        received,
        connections,
    }
}

fn serve(stream: TcpStream, connection: usize, record: &Mutex<Vec<(usize, Value)>>, respond: &Respond) {
    let Ok(mut ws) = accept(stream) else { return };
    while let Ok(message) = ws.read() {
        let Message::Text(text) = message else { continue };
        let value: Value = serde_json::from_str(&text).unwrap();
        record.lock().unwrap().push((connection, value.clone()));
        for out in (respond.lock().unwrap())(&value) {
            if ws.send(Message::Text(out.to_string().into())).is_err() {
                return;
            }
        }
    }
}

impl Peer {
    /// Analysis queries received so far, in order.
    pub fn queries(&self) -> Vec<Value> {
        self.received
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, v)| v.get("action").is_none())
            .map(|(_, v)| v.clone())
            .collect()
    }

    /// Analysis queries received on the latest connection.
    pub fn latest_queries(&self) -> Vec<Value> {
        let latest = self.connections.load(Ordering::SeqCst);
        self.received
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, v)| *c == latest && v.get("action").is_none())
            .map(|(_, v)| v.clone())
            .collect()
    }

    /// Query ids the peer was asked to terminate.
    pub fn terminated(&self) -> Vec<Value> {
        self.received
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, v)| v["action"] == "terminate")
            .map(|(_, v)| v["terminateId"].clone())
            .collect()
    }
}

/// Answers each `terminate` with its ack and the terminated query's final result; queries
/// themselves get nothing until terminated.
pub fn terminate_only(v: &Value) -> Vec<Value> {
    if v["action"] != "terminate" {
        return Vec::new();
    }
    let query = v["terminateId"].clone();
    vec![
        json!({"id": v["id"], "action": "terminate", "terminateId": query}),
        json!({"id": query, "isDuringSearch": false, "moveInfos": [], "rootInfo": {"visits": 0}}),
    ]
}

pub struct Bench {
    pub wb: Workbench,
    events: mpsc::Receiver<(InstanceId, EngineInput)>,
    endings: Arc<Mutex<Vec<JoinHandle<()>>>>,
    /// Lines the workbench wrote to its transports.
    written: Arc<AtomicUsize>,
    /// Output lines fed to the workbench.
    read: usize,
    _dir: tempfile::TempDir,
}

pub fn bench() -> Bench {
    bench_with(CloudConnector::new(Arc::new(|_, _| {
        panic!("custom transport must not resolve an account token")
    })))
}

/// A bench whose cloud engines connect through `connector` (a 智子 one resolves its socket there).
pub fn bench_with(connector: CloudConnector) -> Bench {
    let dir = tempfile::tempdir().unwrap();
    let (tx, events) = mpsc::channel();
    let endings = Arc::new(Mutex::new(Vec::new()));
    let written = Arc::new(AtomicUsize::new(0));
    let launcher = WsLauncher {
        connector,
        tx: Mutex::new(tx),
        endings: endings.clone(),
        written: written.clone(),
    };
    let mut wb = Workbench::new(
        Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data")),
        None,
        Box::new(launcher),
        Vec::new(),
    );
    wb.set_auto_quick_analyze(false).unwrap();
    Bench {
        wb,
        events,
        endings,
        written,
        read: 0,
        _dir: dir,
    }
}

impl Bench {
    pub fn open(&mut self, text: &str) {
        let t = self.wb.begin_open(OpenOrigin::File {
            path: "/x.sgf".into(),
            source: OpenSource::Drop,
        });
        assert!(self
            .wb
            .complete_open(t, Ok((RecordFormat::Sgf, sgf::read(text).unwrap()))));
    }

    pub fn activate_custom(&mut self, url: &str) {
        let draft = CloudSettings {
            provider: Provider::Custom,
            custom_url: url.into(),
            ..CloudSettings::default()
        };
        self.wb.activate_cloud(self.wb.cloud_ticket(), draft, true).unwrap();
    }

    /// Feeds engine events to the workbench until `done` holds (then `true`) or `limit` passes.
    pub fn pump_until(&mut self, limit: Duration, mut done: impl FnMut(&mut Workbench) -> bool) -> bool {
        let end = Instant::now() + limit;
        loop {
            if done(&mut self.wb) {
                return true;
            }
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            if let Ok((id, e)) = self.events.recv_timeout(left.min(Duration::from_millis(20))) {
                self.feed(id, e);
            }
        }
    }

    /// Feeds engine events until nothing is on its way in either direction: `peer()` gives the
    /// lines the peer has read, which must be every line the workbench wrote, and the output
    /// lines it has written, which must all have been fed to the workbench. The bench runs no
    /// timers, so from there the workbench writes nothing more until the peer writes again.
    /// `false` if that does not happen within `limit`.
    #[allow(dead_code)] // cloud_overview.rs only
    pub fn pump_until_quiet(&mut self, limit: Duration, peer: impl Fn() -> (usize, usize)) -> bool {
        let end = Instant::now() + limit;
        loop {
            let (read, written) = peer();
            if read == self.written.load(Ordering::SeqCst) && written == self.read {
                return true;
            }
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            if let Ok((id, e)) = self.events.recv_timeout(left.min(Duration::from_millis(20))) {
                self.feed(id, e);
            }
        }
    }

    fn feed(&mut self, id: InstanceId, e: EngineInput) {
        if matches!(e, EngineInput::Stdout(_)) {
            self.read += 1;
        }
        self.wb.engine_input(id, e);
    }

    /// Waits for the threads of the transports that ended, then feeds every event left: what
    /// they reported before ending, nothing later.
    pub fn settle(&mut self) {
        for handle in std::mem::take(&mut *self.endings.lock().unwrap()) {
            handle.join().unwrap();
        }
        while let Ok((id, e)) = self.events.try_recv() {
            self.feed(id, e);
        }
    }

    pub fn finish(self) {
        let Bench {
            mut wb,
            events,
            endings,
            _dir,
            ..
        } = self;
        wb.end_engines();
        while let Ok((id, e)) = events.recv_timeout(Duration::from_millis(50)) {
            wb.engine_input(id, e);
        }
        for handle in std::mem::take(&mut *endings.lock().unwrap()) {
            let _ = handle.join();
        }
    }
}

/// Whether the foreground engine is ready.
pub fn ready(wb: &mut Workbench) -> bool {
    wb.view(false).engine.foreground.as_ref().is_some_and(|f| f.ready)
}

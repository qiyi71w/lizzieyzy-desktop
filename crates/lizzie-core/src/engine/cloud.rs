//! Cloud engine transports. Launch data is immutable for an incarnation; credentials are resolved
//! on owned background threads and are never part of diagnostics or a launch's debug output.

use std::fmt;
use std::io;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::Instant;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};
use ts_rs::TS;
use url::Url;

use super::process::Sink;
use super::runtime::{EngineInput, EngineIo, InstanceId};

use crate::diagnostics::{Diagnostics, EndpointCategory, NetworkExchange};
use crate::settings::NetworkSettings;
mod custom;
mod network;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum CloudProvider {
    Zhizi,
    Custom,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CloudLaunch {
    pub provider: CloudProvider,
    pub identifier: String,
    pub args: String,
    pub url: String,
}

impl fmt::Debug for CloudLaunch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CloudLaunch")
            .field("provider", &self.provider)
            .field("identifier", &"[redacted]")
            .field("args", &"[redacted]")
            .field("url", &"[redacted]")
            .finish()
    }
}

/// Implementations must supply a sanitized message, never a response body or request URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloudConnectError {
    pub message: String,
    pub fatal: bool,
}

pub struct SocketCredentials {
    pub socket_url: String,
    pub token: String,
}

impl fmt::Debug for SocketCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SocketCredentials([redacted])")
    }
}

pub type SocketTokenSource = Arc<
    dyn Fn(&str, &str) -> Result<SocketCredentials, CloudConnectError> + Send + Sync,
>;

/// Read once for each launch, including recovery launches; never captured from startup settings.
pub type CloudNetworkSource = Arc<dyn Fn() -> Result<NetworkSettings, CloudConnectError> + Send + Sync>;

#[derive(Clone)]
pub struct CloudConnector {
    tokens: SocketTokenSource,
    network: CloudNetworkSource,
    /// 网络/远程 records and the 网络/WebSocket full log of the custom transport.
    diagnostics: Diagnostics,
}

enum Write {
    Line(String),
    Progress(u64),
}

struct CloudIo {
    tx: mpsc::UnboundedSender<Write>,
    cancel: watch::Sender<bool>,
    output: Output,
    thread: Option<JoinHandle<()>>,
    endings: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

/// Never hold a transport lock across the sink: GTP replies synchronously send the next
/// command or retire this IO. The runtime rejects a racing event from a retired instance.
#[derive(Clone)]
struct Output {
    instance: InstanceId,
    sink: Sink,
    active: Arc<AtomicBool>,
}

impl Output {
    fn emit(&self, input: EngineInput) {
        if self.active.load(Ordering::Acquire) {
            (self.sink)(self.instance, input);
        }
    }

    fn retire(&self) {
        self.active.store(false, Ordering::Release);
    }

    fn failure(&self, established: bool, message: &str) {
        if !self.active.swap(false, Ordering::AcqRel) {
            return;
        }
        if established {
            (self.sink)(self.instance, EngineInput::CloudRecovery { message: message.into() });
        } else {
            (self.sink)(self.instance, EngineInput::Stderr(message.into()));
            (self.sink)(self.instance, EngineInput::Exited(None));
        }
    }
}

impl CloudConnector {
    pub fn new(tokens: SocketTokenSource) -> Self {
        Self { tokens, network: Arc::new(|| Ok(NetworkSettings::default())), diagnostics: Diagnostics::default() }
    }

    /// Supply current application networking without coupling the core to its settings owner.
    /// Without a source, standalone connectors use the product's default Direct mode.
    pub fn with_network(mut self, network: CloudNetworkSource) -> Self {
        self.network = network;
        self
    }

    /// Records the custom WebSocket transport through the application's recorder.
    pub fn with_diagnostics(mut self, diagnostics: Diagnostics) -> Self {
        self.diagnostics = diagnostics;
        self
    }

    pub fn launch(
        &self,
        instance: InstanceId,
        launch: &CloudLaunch,
        sink: Sink,
        endings: Arc<Mutex<Vec<JoinHandle<()>>>>,
    ) -> io::Result<Box<dyn EngineIo>> {
        let (tx, rx) = mpsc::unbounded_channel();
        let (cancel, cancelled) = watch::channel(false);
        let output = Output { instance, sink, active: Arc::new(AtomicBool::new(true)) };
        let worker_output = output.clone();
        let launch = launch.clone();
        let connector = self.clone();
        let worker_endings = endings.clone();
        let thread = std::thread::Builder::new()
            .name(format!("engine-{instance}-cloud"))
            .spawn(move || {
                match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                    Ok(runtime) => runtime.block_on(run(
                        connector, launch, worker_output, rx, cancelled, worker_endings,
                    )),
                    Err(_) => worker_output.failure(false, "Cloud transport runtime could not start"),
                }
            })?;
        Ok(Box::new(CloudIo { tx, cancel, output, thread: Some(thread), endings }))
    }
}

impl CloudIo {
    fn end(&mut self) {
        self.output.retire();
        let _ = self.cancel.send(true);
        if let Some(thread) = self.thread.take() {
            let mut endings = self.endings.lock().unwrap();
            endings.retain(|thread| !thread.is_finished());
            endings.push(thread);
        }
    }
}

impl EngineIo for CloudIo {
    fn send(&mut self, line: &str) -> io::Result<()> {
        if !self.output.active.load(Ordering::Acquire) {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "cloud engine input closed"));
        }
        self.tx.send(Write::Line(line.into()))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "cloud engine input closed"))
    }

    fn analysis_progress_accepted(&mut self, total: u64) {
        let _ = self.tx.send(Write::Progress(total));
    }

    fn terminate(mut self: Box<Self>) {
        self.end();
    }
}

impl Drop for CloudIo {
    fn drop(&mut self) {
        self.end();
    }
}

type Socket = WebSocketStream<reqwest::Upgraded>;
const READY: Duration = Duration::from_secs(60);
const WATCHDOG: Duration = Duration::from_secs(20);

async fn run(
    connector: CloudConnector,
    launch: CloudLaunch,
    output: Output,
    mut rx: mpsc::UnboundedReceiver<Write>,
    mut cancel: watch::Receiver<bool>,
    endings: Arc<Mutex<Vec<JoinHandle<()>>>>,
) {
    if *cancel.borrow() { return; }
    let client = match (connector.network)().map_err(|_| StartupError { fatal: true })
        .and_then(|settings| network::client(&settings)) {
        Ok(client) => client,
        Err(_) => {
            output.failure(false, "Cloud network configuration failed");
            return;
        }
    };
    let zhizi = launch.provider == CloudProvider::Zhizi;
    let result = match launch.provider {
        CloudProvider::Zhizi => {
            run_zhizi(connector, &client, launch, &output, &mut rx, &mut cancel, endings).await
        }
        CloudProvider::Custom => {
            run_custom(&client, launch, &output, &mut rx, &mut cancel, &connector.diagnostics).await
        }
    };
    if let Some((established, message)) = result {
        output.failure(established && zhizi, message);
    }
}

/// Cancellation drops any pending connect/send future and the exact socket. Token resolution is
/// deliberately not a Tokio blocking task: dropping a runtime must not wait for a blocking resolver.
async fn credentials(
    connector: &CloudConnector,
    launch: &CloudLaunch,
    cancel: &mut watch::Receiver<bool>,
    endings: &Arc<Mutex<Vec<JoinHandle<()>>>>,
) -> Option<Result<SocketCredentials, CloudConnectError>> {
    let source = connector.tokens.clone();
    let identifier = launch.identifier.clone();
    let args = launch.args.clone();
    let (tx, rx) = oneshot::channel();
    let thread = std::thread::Builder::new().name("cloud-socket-token".into()).spawn(move || {
        let _ = tx.send(source(&identifier, &args));
    });
    let Ok(thread) = thread else {
        return Some(Err(CloudConnectError { message: "Credential worker unavailable".into(), fatal: true }));
    };
    {
        let mut handles = endings.lock().unwrap();
        handles.retain(|handle| !handle.is_finished());
        handles.push(thread);
    }
    tokio::select! {
        biased;
        _ = cancel.changed() => None,
        result = rx => Some(result.unwrap_or_else(|_| Err(CloudConnectError {
            message: "Credential worker failed".into(), fatal: true,
        }))),
    }
}

fn socket_url(credentials: &SocketCredentials) -> Option<String> {
    let mut url = Url::parse(&credentials.socket_url).ok()?;
    let scheme = match url.scheme() { "http" | "ws" => "ws", "https" | "wss" => "wss", _ => return None };
    url.set_scheme(scheme).ok()?;
    url.set_path("/socket.io.v4/");
    url.set_query(None);
    url.set_fragment(None);
    url.query_pairs_mut().append_pair("EIO", "4").append_pair("transport", "websocket")
        .append_pair("zz-socketio-token", &credentials.token);
    Some(url.into())
}

fn fatal_startup(text: &str) -> bool {
    let text = text.to_lowercase();
    let worker_capacity = text.contains("worker") && ["no worker", "unavailable", "busy", "暂时", "没有可用", "无可用"].iter().any(|s| text.contains(s));
    let permission = ["permission", "forbidden", "unauthorized", "未开通", "无权限"].iter().any(|s| text.contains(s));
    if worker_capacity && !permission { return false; }
    ["401", "403", "unauthorized", "forbidden", "permission", "invalid token", "token invalid", "expired token", "token expired", "quota", "balance", "vip", "请先登录", "登录失效", "登录已过期", "账号未", "账号异常", "权限", "额度", "余额", "套餐"]
        .iter().any(|s| text.contains(s))
}

struct StartupError { fatal: bool }

enum Packet {
    Open,
    Connected,
    Ping(String),
    Ready,
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Failure(bool),
    Other,
}

#[derive(Default)]
struct SocketIo {
    binary_event: Option<String>,
    stdout: Lines,
    stderr: Lines,
    secrets: Vec<String>,
}

impl SocketIo {
    fn packet(&mut self, message: Message) -> Result<Packet, StartupError> {
        let text = match message {
            Message::Text(text) => text,
            Message::Binary(bytes) => {
                return match self.binary_event.take().as_deref() {
                    Some("stdout") => Ok(Packet::Stdout(bytes.to_vec())),
                    Some("stderr") => Ok(Packet::Stderr(bytes.to_vec())),
                    Some("ready") => Ok(Packet::Ready),
                    _ => Err(StartupError { fatal: false }),
                };
            }
            Message::Close(_) => return Ok(Packet::Failure(false)),
            _ => return Ok(Packet::Other),
        };
        if text.starts_with('0') { return Ok(Packet::Open); }
        if let Some(payload) = text.strip_prefix('2') { return Ok(Packet::Ping(payload.into())); }
        if text == "1" || text.starts_with("41") { return Ok(Packet::Failure(false)); }
        if text.starts_with("40") { return Ok(Packet::Connected); }
        if text.starts_with("44") {
            return Ok(Packet::Failure(fatal_startup(&text)));
        }
        let binary = text.starts_with("45");
        let body = if binary {
            let Some((count, body)) = text[2..].split_once('-') else { return Err(StartupError { fatal: false }); };
            if count != "1" || self.binary_event.is_some() { return Err(StartupError { fatal: false }); }
            body
        } else if let Some(body) = text.strip_prefix("42") { body } else { return Ok(Packet::Other); };
        let value: Value = serde_json::from_str(body).map_err(|_| StartupError { fatal: false })?;
        let event = value.get(0).and_then(Value::as_str).ok_or(StartupError { fatal: false })?;
        if binary {
            if value[1]["_placeholder"] != true || value[1]["num"] != 0 {
                return Err(StartupError { fatal: false });
            }
            self.binary_event = Some(event.into());
            return Ok(Packet::Other);
        }
        let payload = value.get(1).map(|v| v.as_str().map_or_else(|| v.to_string(), str::to_owned)).unwrap_or_default();
        Ok(match event {
            "ready" => Packet::Ready,
            "stdout" => Packet::Stdout(payload.into_bytes()),
            "stderr" => Packet::Stderr(payload.into_bytes()),
            "connect_error" | "error" => Packet::Failure(fatal_startup(&payload)),
            _ => Packet::Other,
        })
    }
}

#[derive(Default)]
struct Lines { bytes: Vec<u8> }
impl Lines {
    fn feed(&mut self, bytes: &[u8], output: &Output, stderr: bool, secrets: &[String]) {
        self.bytes.extend_from_slice(bytes);
        let mut start = 0;
        for (end, _) in self.bytes.iter().enumerate().filter(|(_, byte)| **byte == b'\n') {
            let mut line = String::from_utf8_lossy(&self.bytes[start..end]).trim_end_matches('\r').to_string();
            for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
                line = line.replace(secret, "[redacted]");
            }
            let lower = line.to_lowercase();
            if ["token", "authorization", "ws://", "wss://", "http://", "https://"].iter().any(|word| lower.contains(word)) {
                line = "[cloud diagnostic redacted]".into();
            }
            start = end + 1;
            output.emit(if stderr { EngineInput::Stderr(line) } else { EngineInput::Stdout(line) });
        }
        self.bytes.drain(..start);
    }
}

async fn send(socket: &mut Socket, message: Message, cancel: &mut watch::Receiver<bool>) -> Result<(), ()> {
    if *cancel.borrow() { return Err(()); }
    tokio::select! {
        biased;
        _ = cancel.changed() => Err(()),
        result = socket.send(message) => result.map_err(|_| ()),
    }
}

async fn zhizi_ready(client: &reqwest::Client, url: String, secrets: Vec<String>, output: &Output, cancel: &mut watch::Receiver<bool>) -> Result<(Socket, SocketIo), StartupError> {
    let deadline = Instant::now() + READY;
    let socket = tokio::select! {
        biased;
        _ = cancel.changed() => return Err(StartupError { fatal: true }),
        result = tokio::time::timeout_at(deadline, network::connect(client, &url)) => result,
    };
    let mut socket = match socket {
        Ok(Ok(socket)) => socket,
        Ok(Err(error)) => return Err(error),
        _ => return Err(StartupError { fatal: false }),
    };
    let mut protocol = SocketIo { secrets, ..SocketIo::default() };
    loop {
        let message = tokio::select! {
            biased;
            _ = cancel.changed() => return Err(StartupError { fatal: true }),
            _ = tokio::time::sleep_until(deadline) => return Err(StartupError { fatal: false }),
            message = socket.next() => message,
        };
        let packet = protocol.packet(message.and_then(Result::ok).ok_or(StartupError { fatal: false })?)?;
        match packet {
            Packet::Open => send(&mut socket, Message::Text("40".into()), cancel).await.map_err(|_| StartupError { fatal: false })?,
            Packet::Ping(payload) => send(&mut socket, Message::Text(format!("3{payload}").into()), cancel).await.map_err(|_| StartupError { fatal: false })?,
            Packet::Ready => return Ok((socket, protocol)),
            // Startup diagnostics are intentionally not forwarded: remote errors may echo tokens.
            Packet::Stderr(_) => output.emit(EngineInput::Stderr("Cloud engine is preparing".into())),
            Packet::Failure(fatal) => return Err(StartupError { fatal }),
            _ => {},
        }
    }
}

#[derive(Default)]
struct Watchdog { deadline: Option<Instant>, last: Option<u64> }
impl Watchdog {
    fn command(&mut self, line: &str) {
        let (_, command) = custom::command_parts(line);
        let name = command.split_whitespace().next().unwrap_or("").to_ascii_lowercase();
        if matches!(name.as_str(), "stop" | "stop-ponder" | "quit") {
            self.deadline = None;
            self.last = None;
        } else if matches!(name.as_str(), "kata-analyze" | "kata-analyze_interval" | "lz-analyze" | "analyze") {
            self.last = None;
            self.deadline = Some(Instant::now() + WATCHDOG);
        }
    }
    fn progress(&mut self, total: u64) {
        if self.deadline.is_some() && self.last != Some(total) {
            self.last = Some(total);
            self.deadline = Some(Instant::now() + WATCHDOG);
        }
    }
}

async fn run_zhizi(
    connector: CloudConnector,
    client: &reqwest::Client,
    launch: CloudLaunch,
    output: &Output,
    rx: &mut mpsc::UnboundedReceiver<Write>,
    cancel: &mut watch::Receiver<bool>,
    endings: Arc<Mutex<Vec<JoinHandle<()>>>>,
) -> Option<(bool, &'static str)> {
    let mut ready = None;
    for attempt in 0..3 {
        if *cancel.borrow() { return None; }
        let credentials = credentials(&connector, &launch, cancel, &endings).await?;
        let result = match credentials {
            Ok(credentials) => match socket_url(&credentials) {
                Some(url) => {
                    let mut secrets = vec![credentials.socket_url.clone(), credentials.token, launch.identifier.clone()];
                    if let Ok(parsed) = Url::parse(&credentials.socket_url) {
                        secrets.push(parsed.username().to_string());
                        if let Some(password) = parsed.password() { secrets.push(password.to_string()); }
                    }
                    zhizi_ready(client, url, secrets, output, cancel).await
                }
                None => Err(StartupError { fatal: true }),
            },
            // Resolver messages are not trusted to omit arbitrary service response bodies.
            Err(error) => Err(StartupError { fatal: error.fatal || fatal_startup(&error.message) }),
        };
        if *cancel.borrow() { return None; }
        match result {
            Ok(session) => { ready = Some(session); break; }
            Err(error) if error.fatal => return Some((false, "Cloud authentication, resource authorization or configuration failed")),
            Err(_) if attempt == 2 => return Some((false, "Cloud engine did not become ready after three attempts")),
            Err(_) => {
                output.emit(EngineInput::Stderr("Cloud engine not ready; retrying startup".into()));
                tokio::select! {
                    biased;
                    _ = cancel.changed() => return None,
                    _ = tokio::time::sleep(Duration::from_millis(if attempt == 0 { 1500 } else { 4000 })) => {},
                }
            }
        }
    }
    let (mut socket, mut protocol) = ready?;
    let mut watchdog = Watchdog::default();
    loop {
        let deadline = watchdog.deadline.unwrap_or_else(|| Instant::now() + Duration::from_secs(86400));
        tokio::select! {
            biased;
            _ = cancel.changed() => return None,
            command = rx.recv() => match command {
                Some(Write::Line(line)) => {
                    watchdog.command(&line);
                    let payload = format!("42{}", json!(["stdin", format!("{}\n", line.trim_end_matches(['\r', '\n']))]));
                    if send(&mut socket, Message::Text(payload.into()), cancel).await.is_err() {
                        if *cancel.borrow() { return None; }
                        return Some((true, "Cloud engine connection failed; rebuilding the current position"));
                    }
                }
                Some(Write::Progress(total)) => watchdog.progress(total),
                None => return None,
            },
            _ = tokio::time::sleep_until(deadline), if watchdog.deadline.is_some() => {
                return Some((true, "Cloud analysis made no accepted progress for 20 seconds; rebuilding the current position"));
            }
            message = socket.next() => {
                let packet = message.and_then(Result::ok).and_then(|message| protocol.packet(message).ok());
                match packet {
                    Some(Packet::Ping(payload)) => {
                        if send(&mut socket, Message::Text(format!("3{payload}").into()), cancel).await.is_err() {
                            if *cancel.borrow() { return None; }
                            return Some((true, "Cloud engine heartbeat connection failed"));
                        }
                    }
                    Some(Packet::Stdout(bytes)) => protocol.stdout.feed(&bytes, output, false, &protocol.secrets),
                    Some(Packet::Stderr(bytes)) => protocol.stderr.feed(&bytes, output, true, &protocol.secrets),
                    Some(Packet::Failure(_)) | None => return Some((true, "Cloud engine disconnected; rebuilding the current position")),
                    _ => {},
                }
            }
        }
    }
}

async fn run_custom(
    client: &reqwest::Client,
    launch: CloudLaunch,
    output: &Output,
    rx: &mut mpsc::UnboundedReceiver<Write>,
    cancel: &mut watch::Receiver<bool>,
    diagnostics: &Diagnostics,
) -> Option<(bool, &'static str)> {
    let Ok(url) = Url::parse(launch.url.trim()) else { return Some((false, "Invalid custom cloud WebSocket URL")); };
    if !matches!(url.scheme(), "ws" | "wss") { return Some((false, "Custom cloud URL requires ws or wss")); }
    let started = Instant::now();
    let connection = tokio::select! {
        biased;
        _ = cancel.changed() => return None,
        result = tokio::time::timeout(Duration::from_secs(20), network::connect(client, url.as_str())) => result,
    };
    // Java `KataGoAnalysisWebSocketTransport.recordConnect`.
    let connected = matches!(connection, Ok(Ok(_)));
    diagnostics.network(&NetworkExchange {
        remote: false,
        method: "WS",
        host: url.host_str().unwrap_or_default(),
        category: EndpointCategory::Protocol,
        status: None,
        latency_ms: started.elapsed().as_millis() as u64,
        outcome: if connected { "connected" } else { "failed" },
    });
    let mut socket = match connection {
        Ok(Ok(socket)) => socket,
        _ => return Some((false, "Custom cloud WebSocket connection failed")),
    };
    let mut adapter = custom::Adapter::default();
    loop {
        tokio::select! {
            biased;
            _ = cancel.changed() => return None,
            command = rx.recv() => match command {
                Some(Write::Line(line)) => match adapter.command(&line, output) {
                    Ok(custom::Action::Send(payload)) => {
                        diagnostics.trace_payload(EndpointCategory::Protocol, "send", &payload);
                        if send(&mut socket, Message::Text(payload.into()), cancel).await.is_err() {
                            if *cancel.borrow() { return None; }
                            return Some((true, "Custom cloud request failed; rebuilding the current position"));
                        }
                        // Do not publish an analysis stream until the complete query was written.
                        adapter.sent(output);
                    }
                    Ok(custom::Action::Quit) => return None,
                    Ok(custom::Action::None) => {},
                    Err(message) => return Some((true, message)),
                },
                Some(Write::Progress(_)) => {},
                None => return None,
            },
            message = socket.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    diagnostics.trace_payload(EndpointCategory::Protocol, "recv", &text);
                    for line in text.lines().filter(|line| !line.trim().is_empty()) {
                        if adapter.response(line, output).is_err() {
                            return Some((true, "Custom cloud returned an invalid analysis response"));
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return Some((true, "Custom cloud disconnected; rebuilding the current position")),
                Some(Ok(Message::Binary(_))) => return Some((true, "Custom cloud returned an unsupported binary response")),
                _ => {},
            }
        }
    }
}

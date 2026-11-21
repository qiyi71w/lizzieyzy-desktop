//! Cloud engine transports. Launch data is immutable for an incarnation; credentials are resolved
//! on owned background threads and are never part of diagnostics or a launch's debug output.

use std::fmt;
use std::io;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use percent_encoding::percent_decode_str;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::Instant;
use tokio_tungstenite::{tungstenite::Message, WebSocketStream};
use ts_rs::TS;
use url::{form_urlencoded, Url};

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
    /// The service's failure key (`CloudFailure.key`) when it named one, else empty. Shown only
    /// through a fixed key-to-text table.
    pub key: String,
}

/// Why a 智子 engine never became ready: the last startup attempt's failure, like Java's
/// `ZhiziGtpTransport.start`, which throws the last attempt's exception.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum CloudStartFailure {
    /// The socket token request failed: the service's failure key, empty when it named none.
    Token { key: String },
    /// No WebSocket connection to the service.
    Unreachable,
    /// The service refused the session or reported an error (Socket.IO `CONNECT_ERROR`, or an
    /// `error` / `connect_error` event): its reason with credentials removed, empty when it gave
    /// none or gave something that is not a plain message.
    Refused { detail: String },
    /// The connection ended before the engine was ready.
    Disconnected,
    /// Connected, but the engine did not report `ready` within 60 seconds.
    NotReady,
}

impl fmt::Display for CloudStartFailure {
    /// The log and console line: no service text.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Token { .. } => "Cloud socket token request failed",
            Self::Unreachable => "Cloud engine connection failed",
            Self::Refused { .. } => "Cloud service refused the engine session",
            Self::Disconnected => "Cloud engine disconnected before it was ready",
            Self::NotReady => "Cloud engine did not become ready in time",
        })
    }
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

    /// A 智子 engine that never became ready: its reason reaches the failure dialog.
    fn not_started(&self, reason: CloudStartFailure) {
        if !self.active.swap(false, Ordering::AcqRel) {
            return;
        }
        (self.sink)(self.instance, EngineInput::Stderr(reason.to_string()));
        (self.sink)(self.instance, EngineInput::CloudStartFailed(reason));
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
    let client = match (connector.network)().map_err(|_| StartupError::FATAL)
        .and_then(|settings| network::client(&settings)) {
        Ok(client) => client,
        Err(_) => {
            output.failure(false, "Cloud network configuration failed");
            return;
        }
    };
    let ending = match launch.provider {
        CloudProvider::Zhizi => {
            run_zhizi(connector, &client, launch, &output, &mut rx, &mut cancel, endings).await
        }
        CloudProvider::Custom => {
            run_custom(&client, launch, &output, &mut rx, &mut cancel, &connector.diagnostics).await.map(Ending::Failed)
        }
    };
    match ending {
        Some(Ending::Failed(message)) => output.failure(false, message),
        Some(Ending::Lost(message)) => output.failure(true, message),
        Some(Ending::NotStarted(reason)) => output.not_started(reason),
        None => {},
    }
}

/// How a cloud transport ended by itself.
enum Ending {
    /// A custom transport's failure.
    Failed(&'static str),
    /// A 智子 engine that never became ready.
    NotStarted(CloudStartFailure),
    /// An established 智子 session: rebuild it and replay the current board.
    Lost(&'static str),
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
        return Some(Err(CloudConnectError { message: "Credential worker unavailable".into(), fatal: true, key: String::new() }));
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
            message: "Credential worker failed".into(), fatal: true, key: String::new(),
        }))),
    }
}

/// The Engine.IO address and the Socket.IO namespace of a socket token's URL, as Java's
/// `IO.socket(socketIOURL, options)` reads it: the URL's decoded path names the namespace
/// (socket.io-client-java 2.1.2 hands `URI.getPath()` to `Manager.socket`), Engine.IO is at
/// `/socket.io.v4/` on the same host, and a query of the URL's own takes the place of the token's
/// (engine.io-client 2.1.0 `Options.fromURI` puts `getRawQuery()` over `options.query`).
fn socket_endpoint(credentials: &SocketCredentials) -> Option<(String, String)> {
    let mut url = Url::parse(&credentials.socket_url).ok()?;
    let scheme = match url.scheme() { "http" | "ws" => "ws", "https" | "wss" => "wss", _ => return None };
    let namespace = match url.path() { "" => "/".to_string(), path => percent_decode_str(path).decode_utf8().ok()?.into_owned() };
    let query = url.query().map(str::to_owned);
    url.set_scheme(scheme).ok()?;
    url.set_path("/socket.io.v4/");
    url.set_fragment(None);
    url.set_query(query.as_deref());
    if query.is_none() { url.query_pairs_mut().append_pair("zz-socketio-token", &credentials.token); }
    url.query_pairs_mut().append_pair("EIO", "4").append_pair("transport", "websocket");
    Some((url.into(), namespace))
}

fn fatal_startup(text: &str) -> bool {
    let text = text.to_lowercase();
    let worker_capacity = text.contains("worker") && ["no worker", "unavailable", "busy", "暂时", "没有可用", "无可用"].iter().any(|s| text.contains(s));
    let permission = ["permission", "forbidden", "unauthorized", "未开通", "无权限"].iter().any(|s| text.contains(s));
    if worker_capacity && !permission { return false; }
    ["401", "403", "unauthorized", "forbidden", "permission", "invalid token", "token invalid", "expired token", "token expired", "quota", "balance", "vip", "请先登录", "登录失效", "登录已过期", "账号未", "账号异常", "权限", "额度", "余额", "套餐"]
        .iter().any(|s| text.contains(s))
}

struct StartupError { fatal: bool, refused: bool }

impl StartupError {
    /// Not worth another attempt: a proxy or an address this connection cannot use.
    const FATAL: Self = Self { fatal: true, refused: false };
    const RETRY: Self = Self { fatal: false, refused: false };
    /// The service turned the session away (HTTP 401 / 403 to the upgrade).
    const REFUSED: Self = Self { fatal: true, refused: true };
}

enum Packet {
    Open,
    Connected,
    Ping(String),
    Ready,
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    /// The connection or the namespace session ended.
    Closed,
    /// The service refused the session or reported an error; `fatal` ends the startup.
    Refused { fatal: bool, detail: String },
    Other,
}

/// One Socket.IO session on its namespace. A text packet is (socket.io-parser) its type, for a
/// binary event the attachment count and `-`, the namespace and `,` unless it is the main
/// namespace `/`, an acknowledgement id, then the JSON data.
struct SocketIo {
    namespace: String,
    /// What leads a packet's data: `<namespace>,`, or nothing for the main namespace.
    prefix: String,
    binary_event: Option<String>,
    /// Binary frames still to come for another namespace's event.
    skip: usize,
    stdout: Lines,
    stderr: Lines,
    secrets: Vec<String>,
}

impl SocketIo {
    fn new(namespace: String, secrets: Vec<String>) -> Self {
        let prefix = if namespace == "/" { String::new() } else { format!("{namespace},") };
        Self { namespace, prefix, binary_event: None, skip: 0, stdout: Lines::default(), stderr: Lines::default(), secrets }
    }

    /// The namespace's CONNECT packet.
    fn connect(&self) -> String {
        format!("40{}", self.prefix)
    }

    /// An event to the namespace.
    fn event(&self, data: &Value) -> String {
        format!("42{}{data}", self.prefix)
    }

    /// `None` for a frame that breaks the protocol.
    fn packet(&mut self, message: Message) -> Option<Packet> {
        let text = match message {
            Message::Text(text) => text,
            Message::Binary(bytes) => {
                if self.skip > 0 {
                    self.skip -= 1;
                    return Some(Packet::Other);
                }
                return match self.binary_event.take().as_deref() {
                    Some("stdout") => Some(Packet::Stdout(bytes.into())),
                    Some("stderr") => Some(Packet::Stderr(bytes.into())),
                    Some("ready") => Some(Packet::Ready),
                    _ => None,
                };
            }
            Message::Close(_) => return Some(Packet::Closed),
            _ => return Some(Packet::Other),
        };
        if text.starts_with('0') { return Some(Packet::Open); }
        if let Some(payload) = text.strip_prefix('2') { return Some(Packet::Ping(payload.into())); }
        if text == "1" { return Some(Packet::Closed); }
        // An Engine.IO message carries one Socket.IO packet.
        let Some(packet) = text.strip_prefix('4') else { return Some(Packet::Other); };
        let Some(&kind) = packet.as_bytes().first().filter(|kind| kind.is_ascii_digit()) else { return Some(Packet::Other); };
        let mut rest = &packet[1..];
        let binary = kind == b'5';
        let mut attachments = 0;
        if binary {
            let (count, body) = rest.split_once('-')?;
            attachments = count.parse::<usize>().ok()?;
            rest = body;
        }
        let (namespace, rest) = if rest.starts_with('/') { rest.split_once(',').unwrap_or((rest, "")) } else { ("/", rest) };
        // Another namespace's packet, such as the main namespace's own CONNECT answer; the binary
        // frames of its event follow it and are not this session's either.
        if namespace != self.namespace {
            self.skip += attachments;
            return Some(Packet::Other);
        }
        if binary && (attachments != 1 || self.binary_event.is_some()) { return None; }
        let data = rest.trim_start_matches(|c: char| c.is_ascii_digit());
        match kind {
            b'0' => return Some(Packet::Connected),
            b'1' => return Some(Packet::Closed),
            b'4' => return Some(self.refused(data, serde_json::from_str::<Value>(data).ok().as_ref())),
            b'2' | b'5' => {},
            _ => return Some(Packet::Other),
        }
        let Ok(Value::Array(items)) = serde_json::from_str(data) else { return None; };
        let mut items = items.into_iter();
        let Some(Value::String(event)) = items.next() else { return None; };
        let payload = items.next();
        if binary {
            if payload.is_none_or(|placeholder| placeholder["_placeholder"] != true || placeholder["num"] != 0) { return None; }
            self.binary_event = Some(event);
            return Some(Packet::Other);
        }
        // The event decides before its data is taken; output text moves out of the parsed packet.
        let text = |payload: Option<Value>| match payload {
            Some(Value::String(text)) => text,
            Some(other) => other.to_string(),
            None => String::new(),
        };
        Some(match event.as_str() {
            "ready" => Packet::Ready,
            "stdout" => Packet::Stdout(text(payload).into_bytes()),
            "stderr" => Packet::Stderr(text(payload).into_bytes()),
            "connect_error" | "error" => {
                let raw = payload.as_ref().map(|value| value.as_str().map_or_else(|| value.to_string(), str::to_owned)).unwrap_or_default();
                self.refused(&raw, payload.as_ref())
            }
            _ => Packet::Other,
        })
    }

    /// The service's refusal: whether it ends the startup, and its reason with credentials
    /// removed. A reason that is not a plain message (JSON), or names a credential, an address,
    /// an e-mail or a long number such as a phone number, however formatted, is left out (Java
    /// `friendlyZhiziErrorMessage` shows no JSON).
    fn refused(&self, raw: &str, data: Option<&Value>) -> Packet {
        let message = match data {
            Some(Value::String(text)) => text.as_str(),
            Some(Value::Object(object)) => object.get("message").and_then(Value::as_str).unwrap_or(""),
            _ => "",
        };
        let first = message.lines().next().unwrap_or("").trim();
        let detail = redact(first.to_string(), &self.secrets).filter(|detail| plain_reason(detail))
            .map(|detail| detail.chars().take(160).collect()).unwrap_or_default();
        Packet::Refused { fatal: fatal_startup(raw), detail }
    }
}

/// Words that mark a remote line as carrying a credential or an address.
const SENSITIVE: [&str; 7] = ["token", "authorization", "bearer", "ws://", "wss://", "http://", "https://"];

/// A remote line with the session's secrets replaced, or `None` when it still names a credential
/// or an address. Engine diagnostics and startup refusals share it.
fn redact(mut line: String, secrets: &[String]) -> Option<String> {
    for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
        if line.contains(secret.as_str()) { line = line.replace(secret.as_str(), "[redacted]"); }
    }
    let bytes = line.as_bytes();
    let names = |word: &str| bytes.windows(word.len()).any(|window| window.eq_ignore_ascii_case(word.as_bytes()));
    let sensitive = SENSITIVE.iter().any(|word| names(word));
    (!sensitive).then_some(line)
}

/// A refusal reason fit to show: not JSON or a status payload (Java `friendlyZhiziErrorMessage`),
/// no e-mail and no run of seven or more digits (spaces, `-`, `+` and brackets between them count
/// as one run).
fn plain_reason(text: &str) -> bool {
    if text.starts_with(['{', '[']) || text.contains("\"key\"") || text.contains('@') || text.to_ascii_lowercase().contains("statuscode") { return false; }
    let mut digits = 0;
    for c in text.chars() {
        if c.is_ascii_digit() {
            digits += 1;
            if digits >= 7 { return false; }
        } else if !matches!(c, ' ' | '-' | '+' | '(' | ')') {
            digits = 0;
        }
    }
    true
}

#[derive(Default)]
struct Lines { bytes: Vec<u8> }
impl Lines {
    fn feed(&mut self, bytes: &[u8], output: &Output, stderr: bool, secrets: &[String]) {
        self.bytes.extend_from_slice(bytes);
        let mut start = 0;
        for (end, _) in self.bytes.iter().enumerate().filter(|(_, byte)| **byte == b'\n') {
            let line = String::from_utf8_lossy(&self.bytes[start..end]).trim_end_matches('\r').to_string();
            let line = redact(line, secrets).unwrap_or_else(|| "[cloud diagnostic redacted]".into());
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

/// A failed startup attempt; `fatal` ends the startup without another attempt.
struct Attempt { fatal: bool, reason: CloudStartFailure }

impl Attempt {
    fn retry(reason: CloudStartFailure) -> Self {
        Self { fatal: false, reason }
    }
}

/// A cancellation also ends in `Err`; the caller checks for it first.
async fn zhizi_ready(client: &reqwest::Client, url: String, namespace: String, secrets: Vec<String>, output: &Output, cancel: &mut watch::Receiver<bool>) -> Result<(Socket, SocketIo), Attempt> {
    let deadline = Instant::now() + READY;
    let socket = tokio::select! {
        biased;
        _ = cancel.changed() => return Err(Attempt::retry(CloudStartFailure::Disconnected)),
        result = tokio::time::timeout_at(deadline, network::connect(client, &url)) => result,
    };
    let mut socket = match socket {
        Ok(Ok(socket)) => socket,
        // Turned away by the service (401, 403 to the upgrade).
        Ok(Err(error)) if error.refused => return Err(Attempt { fatal: true, reason: CloudStartFailure::Refused { detail: String::new() } }),
        // A proxy that refuses (407) or an address this connection cannot use: not 智子's answer.
        Ok(Err(error)) if error.fatal => return Err(Attempt { fatal: true, reason: CloudStartFailure::Unreachable }),
        _ => return Err(Attempt::retry(CloudStartFailure::Unreachable)),
    };
    let mut protocol = SocketIo::new(namespace, secrets);
    // socket.io-client holds a namespace's events until its CONNECT is answered: `ready` counts
    // once both have come, in either order.
    let (mut connected, mut ready) = (false, false);
    loop {
        let message = tokio::select! {
            biased;
            _ = cancel.changed() => return Err(Attempt::retry(CloudStartFailure::Disconnected)),
            _ = tokio::time::sleep_until(deadline) => return Err(Attempt::retry(CloudStartFailure::NotReady)),
            message = socket.next() => message,
        };
        let Some(packet) = message.and_then(Result::ok).and_then(|message| protocol.packet(message)) else {
            return Err(Attempt::retry(CloudStartFailure::Disconnected));
        };
        let answer = match packet {
            Packet::Open => protocol.connect(),
            Packet::Ping(payload) => format!("3{payload}"),
            Packet::Connected => {
                connected = true;
                if ready { return Ok((socket, protocol)); }
                continue;
            }
            Packet::Ready => {
                ready = true;
                if connected { return Ok((socket, protocol)); }
                continue;
            }
            // Startup diagnostics are intentionally not forwarded: remote errors may echo tokens.
            Packet::Stderr(_) => {
                output.emit(EngineInput::Stderr("Cloud engine is preparing".into()));
                continue;
            }
            Packet::Closed => return Err(Attempt::retry(CloudStartFailure::Disconnected)),
            Packet::Refused { fatal, detail } => return Err(Attempt { fatal, reason: CloudStartFailure::Refused { detail } }),
            _ => continue,
        };
        if send(&mut socket, Message::Text(answer.into()), cancel).await.is_err() {
            return Err(Attempt::retry(CloudStartFailure::Disconnected));
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
) -> Option<Ending> {
    let mut ready = None;
    for attempt in 0..3 {
        if *cancel.borrow() { return None; }
        let credentials = credentials(&connector, &launch, cancel, &endings).await?;
        let result = match credentials {
            Ok(credentials) => match socket_endpoint(&credentials) {
                Some((url, namespace)) => {
                    let mut secrets = vec![credentials.socket_url.clone(), credentials.token, launch.identifier.clone()];
                    if let Ok(parsed) = Url::parse(&credentials.socket_url) {
                        secrets.push(parsed.username().to_string());
                        if let Some(password) = parsed.password() { secrets.push(password.to_string()); }
                    }
                    // A service may echo them URL-encoded.
                    let encoded: Vec<String> = secrets.iter().map(|secret| form_urlencoded::byte_serialize(secret.as_bytes()).collect::<String>())
                        .filter(|encoded| !secrets.contains(encoded)).collect();
                    secrets.extend(encoded);
                    zhizi_ready(client, url, namespace, secrets, output, cancel).await
                }
                // The service gave an address this transport cannot use.
                None => Err(Attempt { fatal: true, reason: CloudStartFailure::Token { key: "invalid_response".into() } }),
            },
            // Resolver messages are not trusted to omit arbitrary service response bodies.
            Err(error) => Err(Attempt {
                fatal: error.fatal || fatal_startup(&error.message),
                reason: CloudStartFailure::Token { key: error.key },
            }),
        };
        if *cancel.borrow() { return None; }
        match result {
            Ok(session) => { ready = Some(session); break; }
            Err(failure) if failure.fatal || attempt == 2 => return Some(Ending::NotStarted(failure.reason)),
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
                    let payload = protocol.event(&json!(["stdin", format!("{}\n", line.trim_end_matches(['\r', '\n']))]));
                    if send(&mut socket, Message::Text(payload.into()), cancel).await.is_err() {
                        if *cancel.borrow() { return None; }
                        return Some(Ending::Lost("Cloud engine connection failed; rebuilding the current position"));
                    }
                }
                Some(Write::Progress(total)) => watchdog.progress(total),
                None => return None,
            },
            _ = tokio::time::sleep_until(deadline), if watchdog.deadline.is_some() => {
                return Some(Ending::Lost("Cloud analysis made no accepted progress for 20 seconds; rebuilding the current position"));
            }
            message = socket.next() => {
                let packet = message.and_then(Result::ok).and_then(|message| protocol.packet(message));
                match packet {
                    Some(Packet::Ping(payload)) => {
                        if send(&mut socket, Message::Text(format!("3{payload}").into()), cancel).await.is_err() {
                            if *cancel.borrow() { return None; }
                            return Some(Ending::Lost("Cloud engine heartbeat connection failed"));
                        }
                    }
                    Some(Packet::Stdout(bytes)) => protocol.stdout.feed(&bytes, output, false, &protocol.secrets),
                    Some(Packet::Stderr(bytes)) => protocol.stderr.feed(&bytes, output, true, &protocol.secrets),
                    Some(Packet::Closed | Packet::Refused { .. }) | None => return Some(Ending::Lost("Cloud engine disconnected; rebuilding the current position")),
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
) -> Option<&'static str> {
    let Ok(url) = Url::parse(launch.url.trim()) else { return Some("Invalid custom cloud WebSocket URL"); };
    if !matches!(url.scheme(), "ws" | "wss") { return Some("Custom cloud URL requires ws or wss"); }
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
        _ => return Some("Custom cloud WebSocket connection failed"),
    };
    let mut adapter = custom::Adapter::default();
    loop {
        tokio::select! {
            biased;
            _ = cancel.changed() => return None,
            command = rx.recv() => match command {
                Some(Write::Line(line)) => match adapter.command(&line, output) {
                    Ok(custom::Action::Send(payloads)) => {
                        for payload in payloads {
                            diagnostics.trace_payload(EndpointCategory::Protocol, "send", &payload);
                            if send(&mut socket, Message::Text(payload.into()), cancel).await.is_err() {
                                if *cancel.borrow() { return None; }
                                return Some("Custom cloud request failed; rebuilding the current position");
                            }
                        }
                        // Do not publish an analysis stream until the complete query was written.
                        adapter.sent(output);
                    }
                    Ok(custom::Action::Quit) => return None,
                    Ok(custom::Action::None) => {},
                    Err(message) => return Some(message),
                },
                Some(Write::Progress(_)) => {},
                None => return None,
            },
            message = socket.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    diagnostics.trace_payload(EndpointCategory::Protocol, "recv", &text);
                    for line in text.lines().filter(|line| !line.trim().is_empty()) {
                        if adapter.response(line, output).is_err() {
                            return Some("Custom cloud returned an invalid analysis response");
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return Some("Custom cloud disconnected; rebuilding the current position"),
                Some(Ok(Message::Binary(_))) => return Some("Custom cloud returned an unsupported binary response"),
                _ => {},
            }
        }
    }
}

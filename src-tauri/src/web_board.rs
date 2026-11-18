//! Web 旁观's servers (`WebBoardManager`, `WebBoardHttpServer`, `WebBoardServer`): Java's viewer
//! page over HTTP (9998) and the board stream over WebSocket (9999), on every network interface
//! of this machine, each on the first free port of ten. The core decides what viewers see and
//! runs their 试下; this adapter only moves bytes.
//!
//! Each start is a session. Its tasks stop when it stops, and a viewer message is handed to the
//! core with its session, so nothing of a stopped share reaches a later one. Every viewer has one
//! slot per kind of state (`WebBoardClientUpdates`): a slow browser receives the latest board,
//! never a backlog, and a new board drops the unsent analysis of the old one.

use std::collections::VecDeque;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener as StdListener, UdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use lizzie_core::api::CommandResult;
use lizzie_core::web_board::parse_client_message;
use tauri::AppHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{watch, Notify};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;

use crate::{publish, Shared, State};

const INDEX_HTML: &str = include_str!("../web/index.html");
const BOARD_JS: &str = include_str!("../web/board.js");
const BOARD_CSS: &str = include_str!("../web/board.css");

/// Ports tried after the configured one (`for (int i = 0; i < 10; i++)`).
const PORT_TRIES: u16 = 10;
/// At most ten analysis updates a second (`MIN_BROADCAST_INTERVAL_MS`).
const ANALYSIS_INTERVAL: Duration = Duration::from_millis(100);
/// A page request's head; anything longer is not a request this server answers.
const MAX_REQUEST_HEAD: usize = 8 * 1024;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// A viewer's message: a few short JSON fields (`trial_move` and the like).
const MAX_VIEWER_MESSAGE: usize = 4 * 1024;

#[derive(Default)]
pub struct WebBoard {
    running: Mutex<Option<Running>>,
    next_session: AtomicU64,
}

struct Running {
    session: u64,
    stop: watch::Sender<bool>,
    /// The board may have changed: the broadcaster reads a new frame.
    wake: Arc<Notify>,
}

impl WebBoard {
    /// After a published view: the viewers get what changed.
    pub fn changed(&self) {
        if let Some(r) = self.running.lock().unwrap().as_ref() {
            r.wake.notify_one();
        }
    }
}

/// 启动 Web 旁观. Without a free port pair it stays off (Java's start returns `false`); the
/// reason goes to the log.
pub fn start(app: &AppHandle, shared: &State) -> CommandResult {
    let web = &shared.web_board;
    let mut running = web.running.lock().unwrap();
    if running.is_some() {
        return CommandResult::Unchanged;
    }
    let settings = shared.workbench.lock().unwrap().config().web_board.clone();
    let Some((http, http_port)) = bind(settings.http_port) else {
        shared
            .log
            .warn(&format!("web board: no free page port from {}", settings.http_port));
        return CommandResult::Unchanged;
    };
    let Some((ws, ws_port)) = bind(settings.ws_port) else {
        shared
            .log
            .warn(&format!("web board: no free stream port from {}", settings.ws_port));
        return CommandResult::Unchanged;
    };
    let session = web.next_session.fetch_add(1, Ordering::Relaxed) + 1;
    let url = format!("http://{}:{http_port}", lan_ip());
    let (stop, stopped) = watch::channel(false);
    let wake = Arc::new(Notify::new());
    let hub = Arc::new(Hub {
        max: settings.max_connections as usize,
        ..Hub::default()
    });
    shared.workbench.lock().unwrap().web_board_started(session, url.clone());
    shared.log.info(&format!(
        "web board started on page port {http_port}, stream port {ws_port}"
    ));
    tauri::async_runtime::spawn(serve_pages(http, ws_port, stopped.clone()));
    tauri::async_runtime::spawn(serve_stream(
        app.clone(),
        shared.clone(),
        session,
        ws,
        hub.clone(),
        stopped.clone(),
    ));
    tauri::async_runtime::spawn(broadcast(shared.clone(), session, hub, wake.clone(), stopped));
    wake.notify_one();
    *running = Some(Running { session, stop, wake });
    drop(running);
    publish(app, shared);
    CommandResult::Done
}

/// 停止 Web 旁观: viewers are disconnected and a running 试下 ends.
pub fn stop(app: &AppHandle, shared: &Shared) -> CommandResult {
    let Some(running) = shared.web_board.running.lock().unwrap().take() else {
        return CommandResult::Unchanged;
    };
    let _ = running.stop.send(true);
    shared.workbench.lock().unwrap().web_board_stopped(running.session);
    shared.log.info("web board stopped");
    publish(app, shared);
    CommandResult::Done
}

/// The first of ten ports from `first` that this machine's interfaces accept.
fn bind(first: u16) -> Option<(StdListener, u16)> {
    (0..PORT_TRIES).filter_map(|i| first.checked_add(i)).find_map(|port| {
        let listener = StdListener::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port))).ok()?;
        listener.set_nonblocking(true).ok()?;
        Some((listener, port))
    })
}

/// The address viewers on the network reach this machine at: that of the interface the system
/// routes outward through (no packet is sent); without such a route, `getLanIp`'s first IPv4
/// of an interface that is up and not loopback; else loopback.
fn lan_ip() -> IpAddr {
    let routed = || -> std::io::Result<IpAddr> {
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        socket.connect((Ipv4Addr::new(8, 8, 8, 8), 80))?;
        Ok(socket.local_addr()?.ip())
    };
    routed()
        .ok()
        .filter(|ip| !ip.is_loopback() && !ip.is_unspecified())
        .or_else(|| first_lan_ipv4(&interface_addresses()).map(IpAddr::V4))
        .unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST))
}

/// One address of a network interface, in the order the system lists them.
#[derive(Clone, Copy, Debug)]
struct InterfaceAddress {
    up: bool,
    loopback: bool,
    ip: IpAddr,
}

/// `WebBoardManager.getLanIp`: the first IPv4 address, not itself loopback, of an interface that
/// is up and not loopback.
fn first_lan_ipv4(addresses: &[InterfaceAddress]) -> Option<Ipv4Addr> {
    addresses.iter().find_map(|a| match a.ip {
        IpAddr::V4(ip) if a.up && !a.loopback && !ip.is_loopback() => Some(ip),
        _ => None,
    })
}

/// This machine's interface addresses (`getifaddrs`); empty when they cannot be read.
#[cfg(unix)]
fn interface_addresses() -> Vec<InterfaceAddress> {
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: `getifaddrs` fills `list` with a linked list that stays valid until `freeifaddrs`;
    // every entry is read before that, and a null `ifa_addr` is skipped.
    unsafe {
        if libc::getifaddrs(&mut list) != 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut entry = list;
        while let Some(e) = entry.as_ref() {
            entry = e.ifa_next;
            let Some(addr) = e.ifa_addr.as_ref() else { continue };
            let ip = match i32::from(addr.sa_family) {
                libc::AF_INET => {
                    let v4 = &*(e.ifa_addr as *const libc::sockaddr_in);
                    IpAddr::V4(Ipv4Addr::from(u32::from_be(v4.sin_addr.s_addr)))
                }
                libc::AF_INET6 => {
                    let v6 = &*(e.ifa_addr as *const libc::sockaddr_in6);
                    IpAddr::V6(std::net::Ipv6Addr::from(v6.sin6_addr.s6_addr))
                }
                _ => continue,
            };
            let flags = e.ifa_flags as libc::c_int;
            out.push(InterfaceAddress {
                up: flags & libc::IFF_UP != 0,
                loopback: flags & libc::IFF_LOOPBACK != 0,
                ip,
            });
        }
        libc::freeifaddrs(list);
        out
    }
}

/// This machine's interface unicast IPv4 addresses (`GetAdaptersAddresses`); empty when they
/// cannot be read.
#[cfg(windows)]
fn interface_addresses() -> Vec<InterfaceAddress> {
    use windows::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST,
        IF_TYPE_SOFTWARE_LOOPBACK, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    use windows::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    // The list grows between the size query and the read only rarely; try a few times.
    let mut size = 16 * 1024u32;
    for _ in 0..3 {
        // u64 elements keep the buffer aligned for IP_ADAPTER_ADDRESSES_LH.
        let mut buffer = vec![0u64; (size as usize).div_ceil(8)];
        let first = buffer.as_mut_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
        // SAFETY: `buffer` holds `size` bytes; on success the adapters and their unicast lists
        // live inside it and are read before it is dropped.
        let status = unsafe { GetAdaptersAddresses(u32::from(AF_INET.0), flags, None, Some(first), &mut size) };
        if status == 111 {
            continue; // ERROR_BUFFER_OVERFLOW: `size` now holds what is needed.
        }
        if status != 0 {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut adapter = first.cast_const();
        // SAFETY: as above; every pointer is null or into `buffer`.
        unsafe {
            while let Some(a) = adapter.as_ref() {
                adapter = a.Next;
                let mut unicast = a.FirstUnicastAddress.cast_const();
                while let Some(u) = unicast.as_ref() {
                    unicast = u.Next;
                    let Some(addr) = u.Address.lpSockaddr.as_ref() else {
                        continue;
                    };
                    if addr.sa_family != AF_INET {
                        continue;
                    }
                    let v4 = &*(u.Address.lpSockaddr as *const SOCKADDR_IN);
                    out.push(InterfaceAddress {
                        up: a.OperStatus == IfOperStatusUp,
                        loopback: a.IfType == IF_TYPE_SOFTWARE_LOOPBACK,
                        ip: IpAddr::V4(Ipv4Addr::from(u32::from_be(v4.sin_addr.S_un.S_addr))),
                    });
                }
            }
        }
        return out;
    }
    Vec::new()
}

// ------------------------------------------------------------------------ page

async fn serve_pages(listener: StdListener, ws_port: u16, mut stopped: watch::Receiver<bool>) {
    let Ok(listener) = TcpListener::from_std(listener) else {
        return;
    };
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                if let Ok((stream, _)) = accepted {
                    tauri::async_runtime::spawn(async move {
                        let _ = tokio::time::timeout(REQUEST_TIMEOUT, answer_page(stream, ws_port)).await;
                    });
                }
            }
            _ = stopped.changed() => return,
        }
    }
}

/// `WebBoardHttpServer.handleClient`: GET of the page's three files; the page learns the stream
/// port from `__WS_PORT__`.
async fn answer_page(mut stream: TcpStream, ws_port: u16) -> std::io::Result<()> {
    let mut head = Vec::new();
    let mut buf = [0u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") && !head.windows(2).any(|w| w == b"\n\n") {
        let n = stream.read(&mut buf).await?;
        if n == 0 || head.len() + n > MAX_REQUEST_HEAD {
            break;
        }
        head.extend_from_slice(&buf[..n]);
    }
    let text = String::from_utf8_lossy(&head);
    let mut parts = text.lines().next().unwrap_or("").split(' ');
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        return Ok(());
    };
    let (status, kind, body) = page(method, target, ws_port);
    let reply = match status {
        200 => format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
            body.len()
        ),
        code => format!(
            "HTTP/1.1 {code} {kind}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        ),
    };
    stream.write_all(reply.as_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    stream.shutdown().await
}

/// Status, content type (or reason) and body for a request.
fn page(method: &str, target: &str, ws_port: u16) -> (u16, &'static str, String) {
    let error = |code: u16, reason: &'static str| (code, reason, format!("<h1>{code} {reason}</h1>"));
    if !method.eq_ignore_ascii_case("GET") {
        return error(405, "Method Not Allowed");
    }
    let path = percent_decode(target);
    let path = path.split(['?', '#']).next().unwrap_or("");
    if path.contains("..") || path.contains('\\') {
        return error(403, "Forbidden");
    }
    match path {
        "/" | "/index.html" => (
            200,
            "text/html; charset=utf-8",
            INDEX_HTML.replace("__WS_PORT__", &ws_port.to_string()),
        ),
        "/board.js" => (200, "application/javascript; charset=utf-8", BOARD_JS.to_string()),
        "/board.css" => (200, "text/css; charset=utf-8", BOARD_CSS.to_string()),
        _ => error(404, "Not Found"),
    }
}

/// `URLDecoder.decode`: `%XX` and `+`.
fn percent_decode(s: &str) -> String {
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let decoded = match bytes[i..] {
            [b'%', hi, lo, ..] => hex(hi).zip(hex(lo)).map(|(hi, lo)| hi << 4 | lo),
            _ => None,
        };
        match (decoded, bytes[i]) {
            (Some(b), _) => {
                out.push(b);
                i += 3;
                continue;
            }
            (None, b'+') => out.push(b' '),
            (None, b) => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ------------------------------------------------------------------------ stream

/// The latest state of each kind for one viewer, sent in this order.
#[derive(Default)]
struct Slots {
    /// Answers for this viewer alone (`trial_denied`).
    direct: VecDeque<String>,
    full_state: Option<String>,
    history: Option<String>,
    trial: Option<String>,
    analysis: Option<String>,
}

impl Slots {
    fn next(&mut self) -> Option<String> {
        self.direct
            .pop_front()
            .or_else(|| self.full_state.take())
            .or_else(|| self.history.take())
            .or_else(|| self.trial.take())
            .or_else(|| self.analysis.take())
    }
}

struct Viewer {
    id: u64,
    slots: Mutex<Slots>,
    ready: Notify,
}

impl Viewer {
    fn put(&self, f: impl FnOnce(&mut Slots)) {
        f(&mut self.slots.lock().unwrap());
        self.ready.notify_one();
    }
}

/// The connected viewers and the latest state a new one starts with.
#[derive(Default)]
struct Hub {
    max: usize,
    viewers: Mutex<Vec<Arc<Viewer>>>,
    latest: Mutex<Slots>,
    next: AtomicU64,
}

impl Hub {
    fn each(&self, f: impl Fn(&mut Slots)) {
        f(&mut self.latest.lock().unwrap());
        for v in self.viewers.lock().unwrap().iter() {
            v.put(&f);
        }
    }

    /// A new board supersedes all unsent analysis and graph of the old one.
    fn board(&self, full_state: String, history: String) {
        self.each(|s| {
            s.full_state = Some(full_state.clone());
            s.history = Some(history.clone());
            s.analysis = None;
        });
    }

    /// A viewer joins with the latest board, graph, analysis and 试下 state (a page that
    /// reconnects learns a 试下 ended meanwhile); `None` once the limit is reached.
    fn join(self: &Arc<Self>) -> Option<Joined> {
        let mut viewers = self.viewers.lock().unwrap();
        if viewers.len() >= self.max {
            return None;
        }
        let latest = self.latest.lock().unwrap();
        let viewer = Arc::new(Viewer {
            id: self.next.fetch_add(1, Ordering::Relaxed),
            slots: Mutex::new(Slots {
                direct: VecDeque::new(),
                full_state: latest.full_state.clone(),
                history: latest.history.clone(),
                trial: latest.trial.clone(),
                analysis: latest.analysis.clone(),
            }),
            ready: Notify::new(),
        });
        viewer.ready.notify_one();
        viewers.push(viewer.clone());
        Some(Joined {
            hub: self.clone(),
            viewer,
        })
    }
}

/// A connected viewer; it leaves the hub however its connection ends.
struct Joined {
    hub: Arc<Hub>,
    viewer: Arc<Viewer>,
}

impl Drop for Joined {
    fn drop(&mut self) {
        self.hub.viewers.lock().unwrap().retain(|v| v.id != self.viewer.id);
    }
}

/// How long a stopped share waits for a viewer's close before dropping the connection (a viewer
/// that stopped reading cannot hold the share open).
const CLOSE_GRACE: Duration = Duration::from_secs(1);

async fn serve_stream(
    app: AppHandle,
    shared: State,
    session: u64,
    listener: StdListener,
    hub: Arc<Hub>,
    mut stopped: watch::Receiver<bool>,
) {
    let Ok(listener) = TcpListener::from_std(listener) else {
        return;
    };
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                if let Ok((stream, _)) = accepted {
                    let (app, shared, hub, stopped) = (app.clone(), shared.clone(), hub.clone(), stopped.clone());
                    tauri::async_runtime::spawn(viewer(app, shared, session, stream, hub, stopped));
                }
            }
            _ = stopped.changed() => return,
        }
    }
}

/// One viewer's connection (`WebBoardServer.onOpen` … `onClose`). Stopping the share closes it
/// politely, and drops it after [`CLOSE_GRACE`] whatever it is waiting for.
async fn viewer(
    app: AppHandle,
    shared: State,
    session: u64,
    stream: TcpStream,
    hub: Arc<Hub>,
    stopped: watch::Receiver<bool>,
) {
    let mut stop = stopped.clone();
    tokio::select! {
        _ = connection(app, shared, session, stream, hub, stopped) => {}
        _ = async { let _ = stop.changed().await; tokio::time::sleep(CLOSE_GRACE).await } => {}
    }
}

async fn connection(
    app: AppHandle,
    shared: State,
    session: u64,
    stream: TcpStream,
    hub: Arc<Hub>,
    mut stopped: watch::Receiver<bool>,
) {
    let config = WebSocketConfig::default()
        .max_message_size(Some(MAX_VIEWER_MESSAGE))
        .max_frame_size(Some(MAX_VIEWER_MESSAGE));
    let handshake = tokio_tungstenite::accept_async_with_config(stream, Some(config));
    let Ok(Ok(socket)) = tokio::time::timeout(REQUEST_TIMEOUT, handshake).await else {
        return;
    };
    let (mut sink, mut source) = socket.split();
    let Some(me) = hub.join() else {
        let close = CloseFrame {
            code: CloseCode::Again,
            reason: "Max connections reached".into(),
        };
        let _ = sink.send(Message::Close(Some(close))).await;
        return;
    };
    loop {
        tokio::select! {
            _ = me.viewer.ready.notified() => {
                loop {
                    let next = me.viewer.slots.lock().unwrap().next();
                    let Some(text) = next else { break };
                    if sink.send(Message::text(text)).await.is_err() {
                        return;
                    }
                }
            }
            incoming = source.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    if let Some(message) = parse_client_message(text.as_str()) {
                        let reply = shared.workbench.lock().unwrap().web_board_message(session, message, Instant::now());
                        if let Some(reply) = reply {
                            me.viewer.put(|s| s.direct.push_back(reply));
                        }
                        publish(&app, &shared);
                    }
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => return,
            },
            _ = stopped.changed() => {
                let _ = sink.send(Message::Close(None)).await;
                return;
            }
        }
    }
}

/// Sends what changed: the whole board when the shown position changes, else its analysis at
/// most every [`ANALYSIS_INTERVAL`], and the 试下 state.
async fn broadcast(shared: State, session: u64, hub: Arc<Hub>, wake: Arc<Notify>, mut stopped: watch::Receiver<bool>) {
    let mut board = None;
    let mut analysis: Option<String> = None;
    let mut trial = String::new();
    let mut sent = Instant::now() - ANALYSIS_INTERVAL;
    let mut due: Option<tokio::time::Instant> = None;
    loop {
        let deadline = due.unwrap_or_else(|| tokio::time::Instant::now() + Duration::from_secs(3600));
        tokio::select! {
            _ = wake.notified() => {}
            _ = tokio::time::sleep_until(deadline), if due.is_some() => due = None,
            _ = stopped.changed() => return,
        }
        let frame = {
            let wb = shared.workbench.lock().unwrap();
            if wb.web_board_session() != Some(session) {
                return;
            }
            wb.web_board_frame()
        };
        if board != Some(frame.board) {
            board = Some(frame.board);
            hub.board(frame.full_state, frame.history);
            analysis = frame.analysis;
            sent = Instant::now();
        } else if frame.analysis.is_some() && frame.analysis != analysis {
            if sent.elapsed() >= ANALYSIS_INTERVAL {
                let text = frame.analysis.clone().unwrap_or_default();
                hub.each(|s| s.analysis = Some(text.clone()));
                analysis = frame.analysis;
                sent = Instant::now();
            } else if due.is_none() {
                due = Some(tokio::time::Instant::from_std(sent + ANALYSIS_INTERVAL));
            }
        }
        if frame.trial != trial {
            trial = frame.trial;
            hub.each(|s| s.trial = Some(trial.clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{first_lan_ipv4, interface_addresses, page, InterfaceAddress};
    use std::net::{IpAddr, Ipv4Addr};

    fn addr(up: bool, loopback: bool, ip: &str) -> InterfaceAddress {
        InterfaceAddress {
            up,
            loopback,
            ip: ip.parse().unwrap(),
        }
    }

    #[test]
    fn the_lan_address_is_the_first_ipv4_of_an_interface_that_is_up_and_not_loopback() {
        let lan = |list: &[InterfaceAddress]| first_lan_ipv4(list).map(|ip| ip.to_string());
        assert_eq!(lan(&[]), None);
        // Loopback interfaces and loopback addresses are skipped.
        assert_eq!(
            lan(&[addr(true, true, "127.0.0.1"), addr(true, false, "127.0.1.1")]),
            None
        );
        // A down interface is skipped.
        assert_eq!(
            lan(&[addr(false, false, "10.0.0.5"), addr(true, false, "192.168.77.2")]).as_deref(),
            Some("192.168.77.2")
        );
        // Only IPv6: none.
        assert_eq!(
            lan(&[addr(true, false, "fe80::1"), addr(true, false, "2001:db8::2")]),
            None
        );
        // Several interfaces: the first one listed wins, past its IPv6 address.
        let several = [
            addr(true, true, "127.0.0.1"),
            addr(true, false, "fe80::1"),
            addr(true, false, "192.168.77.2"),
            addr(true, false, "10.1.2.3"),
        ];
        assert_eq!(lan(&several).as_deref(), Some("192.168.77.2"));
    }

    #[test]
    fn this_machines_interface_list_includes_its_loopback_interface() {
        let all = interface_addresses();
        assert!(
            all.iter()
                .any(|a| a.up && a.loopback && a.ip == IpAddr::V4(Ipv4Addr::LOCALHOST)),
            "{all:?}"
        );
    }

    #[test]
    fn only_the_page_files_are_served() {
        let (status, _, body) = page("GET", "/", 10001);
        assert_eq!(status, 200);
        assert!(body.contains("window.WS_PORT = 10001;"));
        assert_eq!(page("GET", "/board.js?v=1", 1).0, 200);
        assert_eq!(page("GET", "/%2e%2e/Cargo.toml", 1).0, 403);
        assert_eq!(page("GET", "/config.json", 1).0, 404);
        assert_eq!(page("POST", "/", 1).0, 405);
    }
}

//! Runs the follow flow's transport work (`Workbench::take_online_jobs`): GETs through the
//! configured network proxy and one WebSocket task per connection. Replies go back to the
//! workbench; the 弈客 page jobs go to [`crate::yike_browser`].
//!
//! URLs (their queries carry room tokens) and bodies are never logged.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use lizzie_core::api::OnlineJob;
use lizzie_core::online::{invalid_proxy_message, Frame, HttpReply, SocketEvent};
use lizzie_core::settings::{NetworkSettings, ProxyMode};
use tauri::{AppHandle, Url};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use crate::{publish, Shared, State};

/// Outgoing frames of the open connections. Removing a connection's sender ends its task.
static SOCKETS: LazyLock<Mutex<HashMap<u64, mpsc::UnboundedSender<Frame>>>> = LazyLock::new(Default::default);
/// The GET client for the proxy settings it was built with.
static CLIENT: Mutex<Option<(NetworkSettings, reqwest::Client)>> = Mutex::new(None);
/// Bound on a WebSocket's connect, proxy tunnel and handshake (Java's clients would wait for
/// the operating system's TCP timeout).
const SOCKET_OPEN_TIMEOUT: Duration = Duration::from_secs(30);

pub fn run_jobs(app: &AppHandle, shared: &Shared, jobs: Vec<OnlineJob>) {
    if jobs.is_empty() {
        return;
    }
    let mut cached = None;
    let mut network = || cached.get_or_insert_with(|| shared.workbench.lock().unwrap().config().network.clone()).clone();
    for job in jobs {
        match job {
            OnlineJob::Get { request, url, headers, timeout_ms } => {
                let network = network();
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    let reply = get(&network, &url, &headers, Duration::from_millis(timeout_ms)).await;
                    deliver(app, move |shared| shared.workbench.lock().unwrap().online_http(request, reply)).await;
                });
            }
            OnlineJob::Connect { conn, url, proxied } => {
                let (sender, receiver) = mpsc::unbounded_channel();
                SOCKETS.lock().unwrap().insert(conn, sender);
                let network = if proxied { network() } else { NetworkSettings::default() };
                tauri::async_runtime::spawn(run_socket(app.clone(), network, conn, url, receiver));
            }
            OnlineJob::Send { conn, frame } => {
                if let Some(sender) = SOCKETS.lock().unwrap().get(&conn) {
                    let _ = sender.send(frame);
                }
            }
            OnlineJob::Close { conn } => {
                SOCKETS.lock().unwrap().remove(&conn);
            }
            page @ (OnlineJob::UnitePoller { .. } | OnlineJob::StopUnitePoller | OnlineJob::ReloadPage { .. }) => {
                crate::yike_browser::page_job(app, shared, page);
            }
        }
    }
}

/// Ends every connection (application exit).
pub fn shutdown() {
    SOCKETS.lock().unwrap().clear();
}

/// Hands a result to the workbench off the async workers, publishing when the view changed;
/// awaited so a connection's events arrive in order.
async fn deliver(app: AppHandle, apply: impl FnOnce(&Shared) -> bool + Send + 'static) {
    let _ = tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let shared = app.state::<State>().inner().clone();
        if apply(&shared) {
            publish(&app, &shared);
        }
    })
    .await;
}

/// The reqwest client for `settings`, built the way `commentary::http` builds its client
/// (Java `NetworkProxy.openConnection`: direct, the system's proxy, or the manual HTTP proxy).
/// Callers have refused unusable manual settings ([`invalid_proxy_message`]).
fn client(settings: &NetworkSettings) -> Option<reqwest::Client> {
    let mut cached = CLIENT.lock().unwrap();
    if let Some((built, client)) = cached.as_ref() {
        if built == settings {
            return Some(client.clone());
        }
    }
    let mut builder = reqwest::Client::builder().retry(reqwest::retry::never());
    match settings.proxy_mode {
        ProxyMode::Direct => builder = builder.no_proxy(),
        ProxyMode::System => {}
        ProxyMode::Manual => {
            let host = manual_host(settings)?;
            let proxy = reqwest::Proxy::all(format!("http://{host}:{}", settings.proxy_port)).ok()?;
            builder = builder.no_proxy().proxy(proxy);
        }
    }
    let client = builder.build().ok()?;
    *cached = Some((settings.clone(), client.clone()));
    Some(client)
}

/// The manual proxy host as an authority part (IPv6 bracketed).
fn manual_host(settings: &NetworkSettings) -> Option<String> {
    let host = settings.proxy_host.trim();
    if host.is_empty() || settings.proxy_port == 0 {
        return None;
    }
    Some(if host.contains(':') && !host.starts_with('[') { format!("[{host}]") } else { host.to_owned() })
}

/// A GET whose body is read as UTF-8 text (lossy), like Java's `InputStreamReader(…, "utf-8")`.
/// Unusable manual proxy settings fail before any request (Java
/// `InvalidNetworkProxyConfigException` → `showNetworkProxyConfigWarning`).
async fn get(settings: &NetworkSettings, url: &str, headers: &[(String, String)], timeout: Duration) -> HttpReply {
    if let Some(message) = invalid_proxy_message(settings) {
        return HttpReply::Failed { proxy: true, message };
    }
    let Some(client) = client(settings) else {
        return HttpReply::Failed { proxy: false, message: "could not create the HTTP client".into() };
    };
    let mut request = client.get(url).timeout(timeout);
    for (name, value) in headers {
        request = request.header(name, value);
    }
    let started = std::time::Instant::now();
    // Java `AjaxHttpRequest.observeAjax`: a status of 400 or more is a failure.
    let observe = |status: Option<u16>| {
        crate::logging::network(&lizzie_core::diagnostics::NetworkExchange {
            remote: false,
            method: "GET",
            host: url,
            category: lizzie_core::diagnostics::EndpointCategory::Other,
            status,
            latency_ms: started.elapsed().as_millis() as u64,
            outcome: if status.is_some_and(|s| s < 400) { "ok" } else { "failed" },
        })
    };
    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            observe(None);
            return failed(error);
        }
    };
    let status = response.status().as_u16();
    match response.bytes().await {
        Ok(body) => {
            observe(Some(status));
            HttpReply::Ok { status, body: String::from_utf8_lossy(&body).into_owned() }
        }
        Err(error) => {
            observe(None);
            failed(error)
        }
    }
}

/// The error text and its causes, without the URL reqwest attaches (its query carries tokens).
fn failed(error: reqwest::Error) -> HttpReply {
    let error = error.without_url();
    let mut message = error.to_string();
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
        message.push_str(": ");
        message.push_str(&cause.to_string());
        source = cause.source();
    }
    HttpReply::Failed { proxy: false, message }
}

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>;

/// One connection: opens it, reports its events, writes the frames the flow sends, and ends
/// silently when the flow closes it (its sender is removed).
async fn run_socket(
    app: AppHandle,
    network: NetworkSettings,
    conn: u64,
    url: String,
    mut outgoing: mpsc::UnboundedReceiver<Frame>,
) {
    if let Some(message) = invalid_proxy_message(&network) {
        return ended(app, conn, SocketEvent::Refused { message }).await;
    }
    let opened = tokio::time::timeout(SOCKET_OPEN_TIMEOUT, open_socket(&network, &url)).await;
    let socket = match opened {
        Ok(Ok(socket)) => socket,
        Ok(Err(message)) => return ended(app, conn, SocketEvent::Closed { error: Some(message) }).await,
        Err(_) => return ended(app, conn, SocketEvent::Closed { error: Some("timed out".into()) }).await,
    };
    if !SOCKETS.lock().unwrap().contains_key(&conn) {
        return; // closed by the flow while opening
    }
    deliver(app.clone(), move |shared| shared.workbench.lock().unwrap().online_socket(conn, SocketEvent::Opened)).await;
    let (mut sink, mut stream) = socket.split();
    let error = loop {
        tokio::select! {
            frame = outgoing.recv() => {
                let Some(frame) = frame else {
                    let _ = sink.close().await;
                    return;
                };
                let message = match frame {
                    Frame::Text(text) => Message::text(text),
                    Frame::Binary(bytes) => Message::binary(bytes),
                };
                if let Err(error) = sink.send(message).await {
                    break Some(error.to_string());
                }
            }
            message = stream.next() => {
                let event = match message {
                    Some(Ok(Message::Text(text))) => SocketEvent::Text(text.as_str().to_owned()),
                    Some(Ok(Message::Binary(bytes))) => SocketEvent::Binary(bytes.to_vec()),
                    Some(Ok(Message::Close(_))) | None => break None,
                    Some(Ok(_)) => continue,
                    Some(Err(error)) => break Some(error.to_string()),
                };
                deliver(app.clone(), move |shared| shared.workbench.lock().unwrap().online_socket(conn, event)).await;
            }
        }
    };
    ended(app, conn, SocketEvent::Closed { error }).await;
}

/// The connection ended by itself or was never opened: it is forgotten and the flow hears
/// `event` (unless the flow already closed it).
async fn ended(app: AppHandle, conn: u64, event: SocketEvent) {
    if SOCKETS.lock().unwrap().remove(&conn).is_none() {
        return;
    }
    deliver(app, move |shared| shared.workbench.lock().unwrap().online_socket(conn, event)).await;
}

/// Opens `url` (ws or wss) through the configured proxy the way Java's `NetworkProxy`
/// configures its WebSocket client (`proxyForWebSocket`): direct, the manual HTTP proxy, or the
/// first HTTP system proxy for the matching http(s) address; proxies are CONNECT tunnels.
/// Callers have refused unusable manual settings ([`invalid_proxy_message`]).
async fn open_socket(settings: &NetworkSettings, url: &str) -> Result<Socket, String> {
    let parsed = Url::parse(url).map_err(|_| "invalid address".to_string())?;
    let secure = match parsed.scheme() {
        "wss" => true,
        "ws" => false,
        _ => return Err("invalid address".into()),
    };
    let host = parsed.host_str().ok_or("invalid address")?.to_owned();
    let port = parsed.port_or_known_default().ok_or("invalid address")?;
    let proxy = match settings.proxy_mode {
        ProxyMode::Direct => None,
        ProxyMode::Manual => {
            let proxy_host = manual_host(settings).ok_or("invalid network proxy settings")?;
            Some((proxy_host, settings.proxy_port, None))
        }
        ProxyMode::System => system_proxy(secure, &host, port),
    };
    let stream = match proxy {
        None => TcpStream::connect((host.trim_start_matches('[').trim_end_matches(']'), port))
            .await
            .map_err(|e| e.to_string())?,
        Some((proxy_host, proxy_port, auth)) => tunnel(&proxy_host, proxy_port, auth, &host, port).await?,
    };
    let _ = stream.set_nodelay(true);
    let (socket, _) = tokio_tungstenite::client_async_tls_with_config(url, stream, None, None)
        .await
        .map_err(|e| e.to_string())?;
    Ok(socket)
}

/// Java `firstHttpSystemProxy(systemLookupUriForWebSocket(uri))`: the system's HTTP proxy for
/// the http(s) form of the address, if any (the lookup reqwest's `system-proxy` uses).
fn system_proxy(secure: bool, host: &str, port: u16) -> Option<(String, u16, Option<http::HeaderValue>)> {
    let lookup: http::Uri = format!("{}://{host}:{port}/", if secure { "https" } else { "http" }).parse().ok()?;
    let intercept = hyper_util::client::proxy::matcher::Matcher::from_system().intercept(&lookup)?;
    let uri = intercept.uri();
    if uri.scheme_str() != Some("http") {
        return None; // Java takes only `Proxy.Type.HTTP` entries
    }
    let proxy_host = uri.host()?.to_owned();
    Some((proxy_host, uri.port_u16().unwrap_or(80), intercept.basic_auth().cloned()))
}

/// An HTTP CONNECT tunnel to `host:port` through the proxy.
async fn tunnel(
    proxy_host: &str,
    proxy_port: u16,
    auth: Option<http::HeaderValue>,
    host: &str,
    port: u16,
) -> Result<TcpStream, String> {
    let address = proxy_host.trim_start_matches('[').trim_end_matches(']');
    let mut stream = TcpStream::connect((address, proxy_port)).await.map_err(|e| e.to_string())?;
    let mut request = format!("CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n");
    if let Some(auth) = auth.as_ref().and_then(|a| a.to_str().ok()) {
        request.push_str(&format!("Proxy-Authorization: {auth}\r\n"));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes()).await.map_err(|e| e.to_string())?;
    let mut head = Vec::with_capacity(256);
    let mut byte = [0u8; 1];
    // Byte by byte so nothing after the reply head is consumed from the tunnel.
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() >= 8192 || stream.read(&mut byte).await.map_err(|e| e.to_string())? == 0 {
            return Err("proxy tunnel failed".into());
        }
        head.push(byte[0]);
    }
    let status = head.split(|b| *b == b' ').nth(1).unwrap_or_default();
    if status != b"200" {
        return Err(format!("proxy tunnel refused ({})", String::from_utf8_lossy(status)));
    }
    Ok(stream)
}

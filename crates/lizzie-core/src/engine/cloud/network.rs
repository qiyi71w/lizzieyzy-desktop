//! HTTP routing and TLS belong to reqwest, using the same system-proxy convention as cloud HTTP.
//! A successful HTTP upgrade is not sufficient: validate RFC 6455 before admitting any frames.

use std::time::Duration;

use reqwest::header::{CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_EXTENSIONS, SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_PROTOCOL, UPGRADE};
use tokio_tungstenite::{tungstenite::{client::IntoClientRequest, handshake::derive_accept_key, protocol::Role}, WebSocketStream};
use url::Url;

use crate::settings::{NetworkSettings, ProxyMode};
use super::{Socket, StartupError};

pub(super) fn client(network: &NetworkSettings) -> Result<reqwest::Client, StartupError> {
    let mut builder = reqwest::Client::builder()
        .http1_only()
        .connect_timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never());
    match network.proxy_mode {
        ProxyMode::Direct => builder = builder.no_proxy(),
        ProxyMode::System => {},
        ProxyMode::Manual => {
            let host = network.proxy_host.trim();
            if host.is_empty() || network.proxy_port == 0 {
                return Err(StartupError { fatal: true });
            }
            let host = if host.contains(':') && !host.starts_with('[') { format!("[{host}]") } else { host.into() };
            let proxy_url = Url::parse(&format!("http://{host}:{}", network.proxy_port))
                .map_err(|_| StartupError { fatal: true })?;
            if proxy_url.host_str().is_none() || !proxy_url.username().is_empty() || proxy_url.password().is_some()
                || proxy_url.path() != "/" || proxy_url.query().is_some() || proxy_url.fragment().is_some()
            {
                return Err(StartupError { fatal: true });
            }
            let proxy = reqwest::Proxy::all(proxy_url).map_err(|_| StartupError { fatal: true })?;
            builder = builder.no_proxy().proxy(proxy);
        }
    }
    builder.build().map_err(|_| StartupError { fatal: true })
}

/// The caller bounds this entire future (proxy negotiation, TLS, upgrade) and drops it on cancel.
/// Only a fatality bit escapes: reqwest errors and HTTP bodies can contain credentials or URLs.
pub(super) async fn connect(client: &reqwest::Client, url: &str) -> Result<Socket, StartupError> {
    let request = url.into_client_request().map_err(|_| StartupError { fatal: true })?;
    let expected = derive_accept_key(request.headers()[SEC_WEBSOCKET_KEY].as_bytes());
    let mut target = Url::parse(url).map_err(|_| StartupError { fatal: true })?;
    let scheme = match target.scheme() {
        "ws" => "http",
        "wss" => "https",
        _ => return Err(StartupError { fatal: true }),
    };
    target.set_scheme(scheme).map_err(|_| StartupError { fatal: true })?;
    let response = client.get(target).headers(request.headers().clone()).send().await
        .map_err(|_| StartupError { fatal: false })?;
    if response.status() != reqwest::StatusCode::SWITCHING_PROTOCOLS {
        return Err(StartupError { fatal: matches!(response.status().as_u16(), 401 | 403 | 407) });
    }
    let headers = response.headers();
    let upgrade = headers.get(UPGRADE).and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("websocket"));
    let connection = headers.get_all(CONNECTION).iter().filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(',')).any(|token| token.trim().eq_ignore_ascii_case("upgrade"));
    if !upgrade || !connection || headers.get(SEC_WEBSOCKET_ACCEPT).is_none_or(|value| value != expected.as_str())
        || headers.contains_key(SEC_WEBSOCKET_PROTOCOL) || headers.contains_key(SEC_WEBSOCKET_EXTENSIONS)
    {
        return Err(StartupError { fatal: false });
    }
    let stream = response.upgrade().await.map_err(|_| StartupError { fatal: false })?;
    Ok(WebSocketStream::from_raw_socket(stream, Role::Client, None).await)
}

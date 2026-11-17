use std::sync::{atomic::{AtomicBool, AtomicU64, Ordering}, mpsc, Arc};
use std::time::Duration;
use lizzie_core::{cloud::CloudFailure, settings::{NetworkSettings, ProxyMode}};
use serde_json::Value;

#[derive(Clone)]
pub(super) struct Http {
    client: reqwest::Client,
    pub generation: Arc<AtomicU64>,
    pub expected: u64,
    cancel: Arc<AtomicBool>,
    write_started: Arc<AtomicBool>,
}
pub(super) struct Response { pub body: Value, pub retry_after: u32 }
impl Http {
    pub fn new(network: &NetworkSettings, generation: Arc<AtomicU64>, expected: u64, cancel: Arc<AtomicBool>) -> Result<Self, CloudFailure> {
        let mut builder = reqwest::Client::builder().connect_timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none()).retry(reqwest::retry::never());
        match network.proxy_mode {
            ProxyMode::Direct => builder = builder.no_proxy(),
            ProxyMode::System => {},
            ProxyMode::Manual => {
                let host = network.proxy_host.trim();
                if host.is_empty() || network.proxy_port == 0 { return Err(CloudFailure::new("invalid_proxy")); }
                let host = if host.contains(':') && !host.starts_with('[') { format!("[{host}]") } else { host.into() };
                let proxy = reqwest::Proxy::all(format!("http://{host}:{}", network.proxy_port))
                    .map_err(|_| CloudFailure::new("invalid_proxy"))?;
                builder = builder.no_proxy().proxy(proxy);
            }
        }
        Ok(Self { client: builder.build().map_err(|_| CloudFailure::new("network_error"))?, generation, expected, cancel, write_started: Arc::new(AtomicBool::new(false)) })
    }
    pub fn current(&self) -> Result<(), CloudFailure> {
        if self.generation.load(Ordering::Acquire) == self.expected && !self.cancel.load(Ordering::Acquire) { Ok(()) }
        else { Err(CloudFailure { uncertain: self.write_started.load(Ordering::Acquire), ..CloudFailure::cancelled() }) }
    }
    pub async fn wait_cancelled(&self) {
        while self.current().is_ok() { tokio::time::sleep(Duration::from_millis(25)).await; }
    }
    pub fn get(&self, url: &str, token: &str) -> Result<Value, CloudFailure> { self.request(url, token, None, false).map(|r| r.body) }
    pub fn post(&self, url: &str, token: &str, body: Value, allow_empty: bool) -> Result<Response, CloudFailure> {
        self.request(url, token, Some(body), allow_empty)
    }
    fn request(&self, url: &str, token: &str, body: Option<Value>, allow_empty: bool) -> Result<Response, CloudFailure> {
        self.current()?;
        let parsed = reqwest::Url::parse(url).map_err(|_| CloudFailure::new("invalid_endpoint"))?;
        // HTTP is permitted only for deterministic loopback peers, never remote credentials.
        if parsed.scheme() != "https" && !(parsed.scheme() == "http" && matches!(parsed.host_str(), Some("127.0.0.1" | "::1" | "localhost"))) {
            return Err(CloudFailure::new("invalid_endpoint"));
        }
        if !parsed.username().is_empty() || parsed.password().is_some() { return Err(CloudFailure::new("invalid_endpoint")); }
        let write = body.is_some();
        for attempt in 0..=usize::from(!write) {
            self.current()?;
            let mut request = if let Some(body) = &body {
                self.client.post(parsed.clone()).header("Content-Type", "application/json").body(body.to_string())
            } else { self.client.get(parsed.clone()) }.header("Accept", "application/json").timeout(Duration::from_secs(20));
            if !token.is_empty() { request = request.bearer_auth(token); }
            let cancellation = self.clone();
            let (tx, rx) = mpsc::sync_channel(1);
            tauri::async_runtime::spawn(async move {
                let exchange = async {
                    if write { cancellation.write_started.store(true, Ordering::Release); }
                    let mut response = request.send().await.map_err(|_| network_failure(write))?;
                    let status = response.status().as_u16();
                    let retry_after = response.headers().get("Retry-After").and_then(|v| v.to_str().ok()).map(parse_retry_after).unwrap_or(0);
                    let mut bytes = Vec::new();
                    while let Some(chunk) = response.chunk().await.map_err(|_| network_failure(write))? {
                        if bytes.len() + chunk.len() > 2 * 1024 * 1024 { return Err(CloudFailure::new("invalid_response")); }
                        bytes.extend_from_slice(&chunk);
                    }
                    if !(200..300).contains(&status) {
                        let key = serde_json::from_slice::<Value>(&bytes).ok().and_then(|v| v.get("key").and_then(Value::as_str).map(safe_key));
                        let key = key.unwrap_or(if status == 401 { "unauthorized" } else { "unknown_error" });
                        return Err(CloudFailure { status, retry_after_seconds: retry_after,
                            retryable: !write && (matches!(status, 408 | 425 | 429) || status >= 500), ..CloudFailure::new(key) });
                    }
                    let value = if allow_empty && bytes.iter().all(u8::is_ascii_whitespace) { serde_json::json!({}) }
                        else { serde_json::from_slice(&bytes).map_err(|_| CloudFailure::new("invalid_response"))? };
                    if !value.is_object() && !value.is_array() { return Err(CloudFailure::new("invalid_response")); }
                    Ok(Response { body: value, retry_after })
                };
                let result = tokio::select! {
                    result = exchange => result,
                    _ = cancellation.wait_cancelled() => cancellation.current().and_then(|_| Err(CloudFailure::cancelled())),
                };
                let _ = tx.send(result);
            });
            let result = loop {
                self.current()?;
                match rx.recv_timeout(Duration::from_millis(25)) {
                    Ok(result) => break result,
                    Err(mpsc::RecvTimeoutError::Timeout) => {},
                    Err(_) => break Err(network_failure(write)),
                }
            };
            if attempt == 0 && !write && result.as_ref().is_err_and(|e| e.key == "network_error") {
                for _ in 0..26 { self.current()?; std::thread::sleep(Duration::from_millis(25)); }
                continue;
            }
            return result;
        }
        unreachable!()
    }
}
fn network_failure(write: bool) -> CloudFailure {
    CloudFailure { retryable: !write, uncertain: write, ..CloudFailure::new("network_error") }
}
fn parse_retry_after(value: &str) -> u32 {
    value.trim().parse::<u32>().map(|v| v.min(600)).unwrap_or_else(|_| {
        chrono::DateTime::parse_from_rfc2822(value).ok().map(|v| (v.timestamp() - chrono::Utc::now().timestamp()).clamp(0, 600) as u32).unwrap_or(0)
    })
}
fn safe_key(key: &str) -> &'static str {
    match key {
        "invalid_phone" => "invalid_phone", "invalid_email" => "invalid_email",
        "invalid_password" => "invalid_password", "invalid_credentials" => "invalid_credentials",
        "invalid_verification_code" => "invalid_verification_code", "fast_login_too_frequent" => "fast_login_too_frequent",
        "password_too_short" => "password_too_short", "duplicate_phone" => "duplicate_phone", "duplicate_email" => "duplicate_email",
        "not_found" => "not_found", "unauthorized" => "unauthorized", "send_code_error" => "send_code_error",
        "failed_to_insert_socket_io_token" => "failed_to_insert_socket_io_token", "failed_aggerate" => "failed_aggerate",
        "count_usage_error" => "count_usage_error", "list_usages_error" => "list_usages_error",
        "count_credits_error" => "count_credits_error", "list_credits_error" => "list_credits_error",
        "invalid_pay_type" => "invalid_pay_type", "invalid_amount" => "invalid_amount", "invalid_product" => "invalid_product",
        "missing_product_name" => "missing_product_name", "invalid_product_name" => "invalid_product_name",
        "failed_create_prepay_id" => "failed_create_prepay_id", "failed_insert_order" => "failed_insert_order",
        "order_not_found" => "order_not_found", _ => "unknown_error",
    }
}

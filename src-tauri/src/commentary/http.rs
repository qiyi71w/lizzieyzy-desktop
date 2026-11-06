//! Synchronous core seam backed by a cancellable reqwest response task. Dropping the async
//! request/response closes the stream, including when the server withholds all body bytes.
use std::io::{self, BufReader, Read};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::{Duration, Instant};

use lizzie_core::commentary::api_key::{HttpExchange, HttpFailure, HttpReply};
use lizzie_core::settings::{NetworkSettings, ProxyMode};

pub struct Http {
    client: reqwest::Client,
    cancel: Arc<AtomicBool>,
}
impl Http {
    pub fn new(settings: &NetworkSettings, cancel: Arc<AtomicBool>, chatgpt: bool) -> Result<Self, HttpFailure> {
        let mut builder = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(if chatgpt { 20 } else { 15 }))
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never());
        match settings.proxy_mode {
            ProxyMode::Direct => builder = builder.no_proxy(),
            ProxyMode::System => {}
            ProxyMode::Manual => {
                let host = settings.proxy_host.trim();
                if host.is_empty() || settings.proxy_port == 0 {
                    return Err(failed(false));
                }
                let host = if host.contains(':') && !host.starts_with('[') {
                    format!("[{host}]")
                } else {
                    host.to_owned()
                };
                let proxy =
                    reqwest::Proxy::all(format!("http://{host}:{}", settings.proxy_port)).map_err(|_| failed(false))?;
                builder = builder.no_proxy().proxy(proxy);
            }
        }
        Ok(Self {
            client: builder.build().map_err(|_| failed(false))?,
            cancel,
        })
    }
    pub fn form(&mut self, url: &str, values: &[(&str, &str)]) -> Result<HttpReply, HttpFailure> {
        let mut encoded = reqwest::Url::parse("http://localhost/").map_err(|_| failed(false))?;
        encoded.query_pairs_mut().extend_pairs(values.iter().copied());
        self.post(
            url,
            &[("Content-Type".into(), "application/x-www-form-urlencoded".into())],
            encoded.query().unwrap_or("").to_owned(),
            Duration::from_secs(30),
        )
    }
    fn send(&self, request: reqwest::RequestBuilder, timeout: Duration) -> Result<HttpReply, HttpFailure> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(failed(true));
        }
        let (headers_tx, headers_rx) = mpsc::sync_channel(1);
        // Bounded backpressure; chunks retain reqwest's Bytes allocation without copying.
        let (body_tx, body_rx) = tokio::sync::mpsc::channel(2);
        let cancel = self.cancel.clone();
        tauri::async_runtime::spawn(async move {
            let result = tokio::select! {
                _ = wait_cancel(&cancel) => return,
                _ = body_tx.closed() => return,
                result = request.timeout(timeout).send() => result,
            };
            let mut response = match result {
                Ok(response) => response,
                Err(_) => {
                    let _ = headers_tx.send(Err(failed(cancel.load(Ordering::Acquire))));
                    return;
                }
            };
            if headers_tx.send(Ok(response.status().as_u16())).is_err() {
                return;
            }
            loop {
                let chunk = tokio::select! {
                    _ = wait_cancel(&cancel) => return,
                    _ = body_tx.closed() => return,
                    result = response.chunk() => result,
                };
                let chunk = match chunk {
                    Ok(Some(bytes)) => Ok(bytes),
                    Ok(None) => return,
                    Err(_) => Err(()),
                };
                let failed = chunk.is_err();
                tokio::select! {
                    _ = wait_cancel(&cancel) => return,
                    result = body_tx.send(chunk) => if result.is_err() { return; },
                }
                if failed {
                    return;
                }
            }
        });
        let deadline = Instant::now() + timeout;
        let status = loop {
            if self.cancel.load(Ordering::Acquire) {
                return Err(failed(true));
            }
            if Instant::now() >= deadline {
                return Err(failed(false));
            }
            match headers_rx.recv_timeout(Duration::from_millis(25)) {
                Ok(result) => break result?,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(failed(self.cancel.load(Ordering::Acquire))),
            }
        };
        let body = Body {
            receiver: body_rx,
            cancel: self.cancel.clone(),
            bytes: bytes::Bytes::new(),
            offset: 0,
        };
        Ok(HttpReply {
            status,
            body: Box::new(BufReader::new(body)),
        })
    }
}
async fn wait_cancel(cancel: &AtomicBool) {
    while !cancel.load(Ordering::Acquire) {
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}
fn failed(cancelled: bool) -> HttpFailure {
    // reqwest errors may contain URLs or credentials; never retain their text.
    HttpFailure {
        message: if cancelled {
            "AI commentary request was cancelled."
        } else {
            "Could not connect to the AI service."
        }
        .into(),
        cancelled,
    }
}
impl HttpExchange for Http {
    fn get(&mut self, url: &str, headers: &[(String, String)], timeout: Duration) -> Result<HttpReply, HttpFailure> {
        let mut request = self.client.get(url);
        for (name, value) in headers {
            request = request.header(name, value);
        }
        self.send(request, timeout)
    }
    fn post(
        &mut self,
        url: &str,
        headers: &[(String, String)],
        body: String,
        timeout: Duration,
    ) -> Result<HttpReply, HttpFailure> {
        let mut request = self.client.post(url).body(body);
        for (name, value) in headers {
            request = request.header(name, value);
        }
        self.send(request, timeout)
    }
}
struct Body {
    receiver: tokio::sync::mpsc::Receiver<Result<bytes::Bytes, ()>>,
    cancel: Arc<AtomicBool>,
    bytes: bytes::Bytes,
    offset: usize,
}
impl Read for Body {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        if output.is_empty() {
            return Ok(0);
        }
        loop {
            if self.cancel.load(Ordering::Acquire) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "AI commentary request was cancelled.",
                ));
            }
            if self.offset < self.bytes.len() {
                let n = output.len().min(self.bytes.len() - self.offset);
                output[..n].copy_from_slice(&self.bytes[self.offset..self.offset + n]);
                self.offset += n;
                return Ok(n);
            }
            match self.receiver.blocking_recv() {
                Some(Ok(bytes)) => {
                    self.bytes = bytes;
                    self.offset = 0;
                }
                Some(Err(())) => return Err(io::Error::other("AI service response was interrupted.")),
                None if self.cancel.load(Ordering::Acquire) => {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "AI commentary request was cancelled.",
                    ))
                }
                None => return Ok(0),
            }
        }
    }
}

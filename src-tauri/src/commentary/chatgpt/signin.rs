use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use lizzie_core::commentary::{chatgpt::failure, CommentaryFailure};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

use super::sessions::{self, SignInContext, AUTH, RESOURCE};
use crate::commentary::{http::Http, Adapter};

pub struct SignIn {
    listener: TcpListener,
    context: SignInContext,
    state: String,
    nonce: String,
    verifier: String,
    callback: String,
    pub authorization: String,
    cancel: Arc<AtomicBool>,
    deadline: Instant,
}
impl SignIn {
    pub fn new(context: SignInContext, cancel: Arc<AtomicBool>) -> Result<Self, CommentaryFailure> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|_| failure("network"))?;
        listener.set_nonblocking(true).map_err(|_| failure("network"))?;
        let callback = format!(
            "http://127.0.0.1:{}/auth/callback",
            listener.local_addr().map_err(|_| failure("network"))?.port()
        );
        let state = sessions::random()?;
        let nonce = sessions::random()?;
        let verifier = sessions::random()?;
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let mut url =
            reqwest::Url::parse(&format!("{AUTH}/api/accounts/authorize")).map_err(|_| failure("protocol"))?;
        {
            let mut params = url.query_pairs_mut();
            params
                .append_pair("client_id", &context.client)
                .append_pair("ext_agent_host_id", &context.host);
            if context.client == "dynamic_agent_client" {
                params.append_pair("agent_name_hint", "LizzieYzy Tauri");
            }
            params
                .append_pair("response_type", "code")
                .append_pair("redirect_uri", &callback)
                .append_pair(
                    "scope",
                    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct",
                )
                .append_pair("resource", RESOURCE)
                .append_pair("state", &state)
                .append_pair("nonce", &nonce)
                .append_pair("code_challenge_method", "S256")
                .append_pair("code_challenge", &challenge);
            if context.reconsent {
                params.append_pair("prompt", "consent");
            }
        }
        Ok(Self {
            listener,
            context,
            state,
            nonce,
            verifier,
            callback,
            authorization: url.into(),
            cancel,
            deadline: Instant::now() + Duration::from_secs(300),
        })
    }
    pub fn open_browser(&self, app: &AppHandle) -> Result<(), CommentaryFailure> {
        app.opener()
            .open_url(&self.authorization, None::<&str>)
            .map_err(|_| failure("browser"))
    }
    pub fn run(&self, app: &AppHandle, http: &mut Http) -> Result<(), CommentaryFailure> {
        loop {
            sessions::cancelled(&self.cancel)?;
            if Instant::now() >= self.deadline {
                self.cancel.store(true, Ordering::Release);
                return Err(failure("timeout"));
            }
            match self.listener.accept() {
                Ok((mut stream, _)) => {
                    let values = match self.validate(&mut stream) {
                        Ok(values) => values,
                        Err(()) => {
                            reply(&mut stream, 400);
                            continue;
                        }
                    };
                    if self.cancel.load(Ordering::Acquire) || Instant::now() >= self.deadline {
                        reply(&mut stream, 409);
                        return Err(CommentaryFailure::cancelled());
                    }
                    reply(&mut stream, 200);
                    // Invalid callbacks remain 400 even after a valid one was consumed.
                    let finished = AtomicBool::new(false);
                    return std::thread::scope(|scope| {
                        scope.spawn(|| {
                            while !finished.load(Ordering::Acquire) && !self.cancel.load(Ordering::Acquire) {
                                if let Ok((mut stream, _)) = self.listener.accept() {
                                    let status = if self.validate(&mut stream).is_ok() { 409 } else { 400 };
                                    reply(&mut stream, status);
                                }
                                std::thread::sleep(Duration::from_millis(20));
                            }
                        });
                        let result = self.complete(app, http, values);
                        finished.store(true, Ordering::Release);
                        result
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(25)),
                Err(_) => return Err(failure("network")),
            }
        }
    }
    fn validate(&self, stream: &mut TcpStream) -> Result<BTreeMap<String, String>, ()> {
        stream
            .set_read_timeout(Some(Duration::from_millis(250)))
            .map_err(|_| ())?;
        stream
            .set_write_timeout(Some(Duration::from_millis(250)))
            .map_err(|_| ())?;
        let mut line = Vec::new();
        let mut byte = [0];
        while line.len() <= 16_410 {
            if self.cancel.load(Ordering::Acquire) || Instant::now() >= self.deadline {
                return Err(());
            }
            if stream.read(&mut byte).map_err(|_| ())? != 1 {
                return Err(());
            }
            if byte[0] == b'\n' {
                break;
            }
            line.push(byte[0]);
        }
        let line = std::str::from_utf8(&line).map_err(|_| ())?;
        let mut parts = line.trim_end_matches('\r').split(' ');
        if parts.next() != Some("GET") {
            return Err(());
        }
        let uri = parts.next().ok_or(())?;
        if uri.len() > 16_384 || !matches!(parts.next(), Some("HTTP/1.0" | "HTTP/1.1")) || parts.next().is_some() {
            return Err(());
        }
        if uri.split('?').next() != Some("/auth/callback") || uri.contains('#') {
            return Err(());
        }
        let bytes = uri.as_bytes();
        for (index, byte) in bytes.iter().enumerate() {
            if *byte == b'%'
                && (index + 2 >= bytes.len()
                    || !bytes[index + 1].is_ascii_hexdigit()
                    || !bytes[index + 2].is_ascii_hexdigit())
            {
                return Err(());
            }
        }
        let url = reqwest::Url::parse(&format!("http://127.0.0.1{uri}")).map_err(|_| ())?;
        let mut values = BTreeMap::new();
        for (key, value) in url.query_pairs() {
            if values.insert(key.into_owned(), value.into_owned()).is_some() {
                return Err(());
            }
        }
        let state = values.get("state").map(String::as_bytes).unwrap_or_default();
        if !bool::from(self.state.as_bytes().ct_eq(state)) {
            return Err(());
        }
        Ok(values)
    }
    fn complete(
        &self,
        app: &AppHandle,
        http: &mut Http,
        values: BTreeMap<String, String>,
    ) -> Result<(), CommentaryFailure> {
        if let Some(error) = values.get("error") {
            return Err(failure(if error == "access_denied" { "denied" } else { "network" }));
        }
        let client =
            values
                .get("client_id")
                .map(String::as_str)
                .unwrap_or(if self.context.client == "dynamic_agent_client" {
                    ""
                } else {
                    &self.context.client
                });
        if client.trim().is_empty()
            || client == "dynamic_agent_client"
            || (self.context.client != "dynamic_agent_client" && client != self.context.client)
        {
            return Err(failure("identity"));
        }
        let code = values
            .get("code")
            .filter(|code| !code.trim().is_empty())
            .ok_or_else(|| failure("protocol"))?;
        sessions::cancelled(&self.cancel)?;
        let adapter = app.state::<Adapter>();
        if self.context.profile.is_none() {
            adapter.sessions.lock().unwrap().retain_registration(client)?;
        }
        let reply = http
            .form(
                &format!("{AUTH}/api/accounts/oauth/token"),
                &[
                    ("grant_type", "authorization_code"),
                    ("client_id", client),
                    ("code", code),
                    ("code_verifier", &self.verifier),
                    ("redirect_uri", &self.callback),
                    ("resource", RESOURCE),
                ],
            )
            .map_err(|error| {
                if error.cancelled {
                    CommentaryFailure::cancelled()
                } else {
                    failure("network")
                }
            })?;
        let tokens = sessions::json_reply(reply).map_err(|e| e.failure())?;
        let identity = sessions::verify_identity(
            http,
            tokens["id_token"].as_str().ok_or_else(|| failure("identity"))?,
            client,
            Some(&self.nonce),
            self.context.subject.as_deref(),
        )?;
        // Cancel and persistence serialize on the same mutex. Once persistence starts, its result
        // wins, matching Java's synchronized accept/close boundary.
        let pending = adapter.sign_in.lock().unwrap();
        sessions::cancelled(&self.cancel)?;
        if Instant::now() >= self.deadline {
            return Err(failure("timeout"));
        }
        if !pending.as_ref().is_some_and(|flag| Arc::ptr_eq(flag, &self.cancel)) {
            return Err(CommentaryFailure::cancelled());
        }
        let mut sessions = adapter.sessions.lock().unwrap();
        sessions.accept(&self.context, client, identity, &tokens)
    }
}
fn reply(stream: &mut TcpStream, status: u16) {
    let body = if status == 200 {
        "Return to LizzieYzy Tauri to finish connecting ChatGPT."
    } else {
        "This sign-in callback is invalid or has expired."
    };
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Conflict",
    };
    let _ = write!(stream, "HTTP/1.1 {status} {reason}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}", body.len());
}

//! OpenAI-compatible API-key protocol over an injected, non-redirecting HTTP exchange.
use std::{
    collections::HashSet,
    io::{BufRead, Read},
    sync::atomic::AtomicBool,
    time::Duration,
};

use serde_json::{json, Value};

use super::{
    chatgpt::iso_control,
    settings,
    stream::{self, blank, check_cancel, json_string, short_detail},
    ChatTransport, CommentaryFailure, Message,
};

pub struct HttpReply {
    pub status: u16,
    pub body: Box<dyn BufRead + Send>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpFailure {
    pub message: String,
    pub cancelled: bool,
}

/// Implementations MUST NOT follow redirects or log credentials/payloads. The adapter owns
/// connection timeouts, proxy choice, and closing a blocked body when its request is cancelled.
pub trait HttpExchange {
    fn get(&mut self, url: &str, headers: &[(String, String)], timeout: Duration) -> Result<HttpReply, HttpFailure>;
    fn post(
        &mut self,
        url: &str,
        headers: &[(String, String)],
        body: String,
        timeout: Duration,
    ) -> Result<HttpReply, HttpFailure>;
}

pub struct ApiKeyClient<H = ()> {
    base_url: String,
    api_key: String,
    model: String,
    http: H,
}

impl ApiKeyClient {
    pub fn new(base_url: &str, api_key: &str, model: &str) -> Result<Self, CommentaryFailure> {
        Ok(Self {
            base_url: normalize_api_base(base_url)?,
            api_key: require_api_key(api_key)?,
            model: settings::validate_model(model)?,
            http: (),
        })
    }

    /// Model discovery does not require choosing a model or making an inference request.
    pub fn for_models(base_url: &str, api_key: &str) -> Result<Self, CommentaryFailure> {
        Ok(Self {
            base_url: normalize_api_base(base_url)?,
            api_key: require_api_key(api_key)?,
            model: String::new(),
            http: (),
        })
    }
}

impl<H> ApiKeyClient<H> {
    pub fn with_exchange<T: HttpExchange>(self, http: T) -> ApiKeyClient<T> {
        ApiKeyClient {
            base_url: self.base_url,
            api_key: self.api_key,
            model: self.model,
            http,
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn endpoint(&self, suffix: &str) -> String {
        format!("{}/{suffix}", self.base_url.trim_end_matches('/'))
    }

    fn headers(&self, streaming: bool) -> Vec<(String, String)> {
        let mut headers = vec![
            ("Authorization".into(), format!("Bearer {}", self.api_key)),
            ("Content-Type".into(), "application/json; charset=utf-8".into()),
            ("User-Agent".into(), "LizzieYzy-Next-AI-Commentary".into()),
        ];
        if streaming {
            headers.push(("Accept".into(), "text/event-stream".into()));
        }
        headers
    }

    fn exchange_failure(&self, failure: HttpFailure) -> CommentaryFailure {
        if failure.cancelled {
            CommentaryFailure {
                message_key: "Teacher.status.cancelled".into(),
                message: "AI commentary request was cancelled.".into(),
                cancelled: true,
            }
        } else {
            CommentaryFailure::new(stream::sanitize_error(&failure.message, &self.api_key))
        }
    }

    fn http_failure(&self, mut reply: HttpReply) -> CommentaryFailure {
        let mut bytes = Vec::new();
        if let Err(error) = reply.body.by_ref().take(4097).read_to_end(&mut bytes) {
            return CommentaryFailure::new(stream::sanitize_error(&error.to_string(), &self.api_key));
        }
        bytes.truncate(4096);
        let detail = short_detail(&String::from_utf8_lossy(&bytes), &self.api_key);
        CommentaryFailure::new(if detail.is_empty() {
            format!("AI service returned HTTP {}.", reply.status)
        } else {
            format!("AI service returned HTTP {}: {detail}", reply.status)
        })
    }
}

impl<H: HttpExchange> ApiKeyClient<H> {
    pub fn list_models(&mut self) -> Result<Vec<String>, CommentaryFailure> {
        let url = self.endpoint("models");
        let headers = self.headers(false);
        let mut reply = self
            .http
            .get(&url, &headers, Duration::from_secs(30))
            .map_err(|failure| self.exchange_failure(failure))?;
        if reply.status / 100 != 2 {
            return Err(self.http_failure(reply));
        }
        let mut body = String::new();
        reply
            .body
            .read_to_string(&mut body)
            .map_err(|e| CommentaryFailure::new(stream::sanitize_error(&e.to_string(), &self.api_key)))?;
        parse_models(&body)
    }
}

impl<H: HttpExchange> ChatTransport for ApiKeyClient<H> {
    fn complete(&mut self, messages: &[Message], cancel: &AtomicBool) -> Result<String, CommentaryFailure> {
        settings::validate_model(&self.model)?;
        let messages = validate_messages(messages)?;
        check_cancel(cancel)?;
        let headers = self.headers(true);
        let body = json!({"model": self.model, "stream": true, "messages": messages}).to_string();
        let url = self.endpoint("chat/completions");
        let reply = self.http.post(&url, &headers, body, Duration::from_secs(300));
        check_cancel(cancel)?;
        let mut reply = reply.map_err(|failure| self.exchange_failure(failure))?;
        if reply.status / 100 == 2 {
            return stream::parse_chat_completions(reply.body.as_mut(), cancel, &self.api_key);
        }
        if !matches!(reply.status, 404 | 405) {
            return Err(self.http_failure(reply));
        }
        drop(reply);
        check_cancel(cancel)?;
        let body = json!({"model": self.model, "stream": true, "input": messages}).to_string();
        let url = self.endpoint("responses");
        let reply = self.http.post(&url, &headers, body, Duration::from_secs(300));
        check_cancel(cancel)?;
        let mut reply = reply.map_err(|failure| self.exchange_failure(failure))?;
        if reply.status / 100 != 2 {
            return Err(self.http_failure(reply));
        }
        stream::parse_api_responses(reply.body.as_mut(), cancel, &self.api_key)
    }
}

pub fn normalize_api_base(value: &str) -> Result<String, CommentaryFailure> {
    let mut validated = settings::validate_base_url(value)?;
    let rest = validated.split_once("://").map(|(_, rest)| rest).unwrap_or("");
    if !rest.contains('/') {
        validated.push_str("/v1");
    }
    Ok(validated)
}

fn require_api_key(value: &str) -> Result<String, CommentaryFailure> {
    let key = value.trim_matches(|c: char| c <= '\u{20}');
    if key.is_empty() || key.chars().any(iso_control) {
        return Err(CommentaryFailure::new("A valid API key is required."));
    }
    Ok(key.into())
}

fn validate_messages(messages: &[Message]) -> Result<Vec<Value>, CommentaryFailure> {
    if messages.is_empty() {
        return Err(CommentaryFailure::new("At least one message is required."));
    }
    let mut result = Vec::new();
    let mut characters = 0;
    for message in messages {
        if blank(&message.content) {
            continue;
        }
        let role = message.role.trim_matches(|c: char| c <= '\u{20}');
        if !matches!(role, "system" | "user" | "assistant") {
            return Err(CommentaryFailure::new("Unsupported message role."));
        }
        characters += message.content.encode_utf16().count();
        if characters > 250_000 {
            return Err(CommentaryFailure::new("AI commentary context is too large."));
        }
        result.push(json!({"role": role, "content": message.content}));
    }
    if result.is_empty() {
        return Err(CommentaryFailure::new("At least one non-empty message is required."));
    }
    Ok(result)
}

/// Java Comparator.naturalOrder is lexicographic UTF-16 order (not numeric-aware sorting).
pub fn parse_models(body: &str) -> Result<Vec<String>, CommentaryFailure> {
    let root: Value =
        serde_json::from_str(body).map_err(|_| CommentaryFailure::new("AI service returned an invalid model list."))?;
    if !root.is_object() {
        return Err(CommentaryFailure::new("AI service returned an invalid model list."));
    }
    let mut models = HashSet::new();
    if let Some(data) = root["data"].as_array() {
        for item in data {
            let id = json_string(&item["id"]);
            let id = id.trim_matches(|c: char| c <= '\u{20}');
            if !id.is_empty() {
                models.insert(id.to_owned());
            }
        }
    }
    let mut models: Vec<String> = models.into_iter().collect();
    models.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    Ok(models)
}

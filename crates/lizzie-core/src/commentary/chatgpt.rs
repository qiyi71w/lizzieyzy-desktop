//! ChatGPT model capabilities, request formatting, and allowlisted error metadata.
use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{
    stream::{blank, json_string},
    CommentaryFailure, Message,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Model {
    pub slug: String,
    pub display_name: String,
    pub efforts: Vec<String>,
    pub default_effort: String,
}

pub fn valid_effort(effort: &str) -> bool {
    let bytes = effort.as_bytes();
    (1..=32).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(*b, b'_' | b'-'))
}

pub fn parse_models(body: &str) -> Result<Vec<Model>, CommentaryFailure> {
    let root: Value = serde_json::from_str(body).map_err(|_| failure("protocol"))?;
    let entries = root["models"].as_array().ok_or_else(|| failure("protocol"))?;
    let mut seen = HashSet::new();
    let mut models = Vec::new();
    for item in entries {
        if item["visibility"] != "list" {
            continue;
        }
        let slug = json_string(&item["slug"]);
        if blank(&slug)
            || slug.encode_utf16().count() > 160
            || slug.chars().any(iso_control)
            || !seen.insert(slug.clone())
        {
            continue;
        }
        let mut efforts = Vec::new();
        if let Some(levels) = item["supported_reasoning_levels"].as_array() {
            for level in levels {
                let effort = json_string(&level["effort"]);
                if valid_effort(&effort) && !efforts.iter().any(|value| value == effort.as_ref()) {
                    efforts.push(effort.into_owned());
                }
            }
        }
        let advertised = item["default_reasoning_level"].as_str().unwrap_or("");
        let default_effort = if efforts.iter().any(|value| value == advertised) {
            advertised.into()
        } else {
            String::new()
        };
        let display_name = if item["display_name"].is_null() {
            slug.clone()
        } else {
            json_string(&item["display_name"])
        }
        .into_owned();
        models.push(Model {
            slug: slug.into_owned(),
            display_name,
            efforts,
            default_effort,
        });
    }
    if models.is_empty() {
        return Err(failure("models"));
    }
    Ok(models)
}

pub fn request_body(model: &str, messages: &[Message], effort: Option<&str>) -> Result<Value, CommentaryFailure> {
    let effort = effort.unwrap_or("");
    if !effort.is_empty() && !valid_effort(effort) {
        return Err(failure("reasoning"));
    }
    if blank(model) || messages.is_empty() {
        return Err(failure("models"));
    }
    let mut input = Vec::new();
    let mut characters = 0;
    for message in messages {
        if blank(&message.content) {
            continue;
        }
        let role = if message.role == "system" {
            "developer"
        } else {
            message.role.as_str()
        };
        if !matches!(role, "developer" | "user" | "assistant") {
            return Err(failure("protocol"));
        }
        characters += message.content.encode_utf16().count();
        if characters > 300_000 {
            return Err(failure("protocol"));
        }
        input.push(json!({"role": role, "content": message.content}));
    }
    if input.is_empty() {
        return Err(failure("protocol"));
    }
    let mut body = json!({"model": model, "input": input, "store": false, "stream": true});
    if !effort.is_empty() {
        body["reasoning"] = json!({"effort": effort});
    }
    Ok(body)
}

pub fn responses_headers(token: &str) -> Vec<(String, String)> {
    vec![
        ("Authorization".into(), format!("Bearer {token}")),
        ("Content-Type".into(), "application/json".into()),
        ("Accept".into(), "text/event-stream".into()),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorCategory {
    Limit,
    Ineligible,
    Permission,
    Protocol,
    LoginRequired,
    Network,
}

impl ErrorCategory {
    pub fn code(self) -> &'static str {
        match self {
            Self::Limit => "limit",
            Self::Ineligible => "ineligible",
            Self::Permission => "permission",
            Self::Protocol => "protocol",
            Self::LoginRequired => "loginRequired",
            Self::Network => "network",
        }
    }
    pub fn message_key(self) -> &'static str {
        match self {
            Self::Limit => "Teacher.chatgpt.error.limit",
            Self::Ineligible => "Teacher.chatgpt.error.ineligible",
            Self::Permission => "Teacher.chatgpt.error.permission",
            Self::Protocol => "Teacher.chatgpt.error.protocol",
            Self::LoginRequired => "Teacher.chatgpt.error.loginRequired",
            Self::Network => "Teacher.chatgpt.error.network",
        }
    }
}

/// Contains no response text or URLs. All diagnostic strings use Java's metadata allowlist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApiError {
    pub category: ErrorCategory,
    pub status: u16,
    pub code: String,
    pub parameter: String,
    pub request_id: String,
}

impl ApiError {
    pub fn failure(&self) -> CommentaryFailure {
        failure(self.category.code())
    }
    pub fn refresh_revoked(&self) -> bool {
        matches!(
            self.code.as_str(),
            "invalid_grant"
                | "invalid_refresh_token"
                | "token_expired"
                | "refresh_token_expired"
                | "refresh_token_invalidated"
                | "refresh_token_reused"
        )
    }
}

pub fn classify_error(status: u16, payload: &Value, request_id: &str) -> ApiError {
    let error = payload.get("error").filter(|v| v.is_object());
    let code = safe_metadata(&json_string(error.map(|e| &e["code"]).unwrap_or(&payload["error"])));
    let parameter = safe_metadata(&json_string(error.map(|e| &e["param"]).unwrap_or(&Value::Null)));
    use ErrorCategory::*;
    let category = match code.as_str() {
        "subscription_sharing_usage_limit_exceeded" => Limit,
        "subscription_sharing_user_not_eligible" => Ineligible,
        "chatpass_v2_scope_not_authorized" | "chatpass_v2_invalid_authorization_context" => Permission,
        "subscription_sharing_unsupported_capability"
        | "subscription_sharing_route_not_supported"
        | "invalid_client" => Protocol,
        "invalid_grant"
        | "invalid_refresh_token"
        | "token_expired"
        | "refresh_token_expired"
        | "refresh_token_invalidated"
        | "refresh_token_reused"
        | "subscription_sharing_invalid_user" => LoginRequired,
        "subscription_sharing_usage_unavailable" | "subscription_sharing_user_unavailable" => Network,
        _ => match status {
            429 => Limit,
            401 => LoginRequired,
            403 => Permission,
            400 => Protocol,
            _ => Network,
        },
    };
    ApiError {
        category,
        status,
        code,
        parameter,
        request_id: safe_metadata(request_id),
    }
}

pub fn response_error(status: u16, body: &str, request_id: &str) -> ApiError {
    let payload = serde_json::from_str(body).unwrap_or(Value::Null);
    classify_error(status, &payload, request_id)
}

fn safe_metadata(value: &str) -> String {
    if (1..=160).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b':' | b'-'))
    {
        value.into()
    } else {
        String::new()
    }
}

pub(crate) fn iso_control(c: char) -> bool {
    c <= '\u{1f}' || ('\u{7f}'..='\u{9f}').contains(&c)
}

/// Default bundle texts, not provider-supplied messages. Adapters localize the message_key.
pub fn failure(code: &str) -> CommentaryFailure {
    let message = match code {
        "network" => "Could not connect to ChatGPT. Check your network and retry.",
        "permission" => "Authorize ChatGPT plan usage or check account eligibility.",
        "loginRequired" => "Your ChatGPT session expired. Sign in again.",
        "identity" => "ChatGPT identity verification failed. Sign in again.",
        "protocol" => "ChatGPT returned an unsupported response or request format. Refresh models or update the application.",
        "ineligible" => "This account, workspace or region is not eligible for ChatGPT plan commentary. Check eligibility with OpenAI. API billing will not be used automatically.",
        "timeout" => "ChatGPT sign-in timed out. Please try again.",
        "denied" => "ChatGPT authorization was cancelled or declined.",
        "models" => "No available ChatGPT model. Refresh models or check your account.",
        "incomplete" => "Commentary was interrupted and was not saved. Please retry.",
        "busy" => "Another app instance is updating the ChatGPT session. Please retry.",
        "storage" => "ChatGPT account storage could not be read. Check access permissions.",
        "limit" => "ChatGPT usage limit reached. Open Manage usage to review your allowance.",
        "browser" => "Could not open your browser. Check the system default browser.",
        "runtime" => "ChatGPT sign-in needs a Java component missing from this installation. Install the latest complete package, not only the core update.",
        "reasoning" => "This thinking depth is no longer supported. Refresh models and choose again.",
        "credentialsUnavailable" => "Saved login could not be read. Check system secure-storage lock and access permissions, then retry. This does not mean you have been signed out.",
        "modelRemoved" => "The selected model is no longer available. Choose a model from the updated list.",
        _ => return CommentaryFailure::keyed(&format!("Teacher.chatgpt.error.{code}"), format!("ChatGPT request could not be completed ({code}).")),
    };
    CommentaryFailure::keyed(&format!("Teacher.chatgpt.error.{code}"), message)
}

pub fn is_known_error(message: &str) -> bool {
    [
        "network",
        "permission",
        "ineligible",
        "loginRequired",
        "identity",
        "protocol",
        "timeout",
        "denied",
        "models",
        "reasoning",
        "incomplete",
        "busy",
        "storage",
        "credentialsUnavailable",
        "modelRemoved",
        "limit",
        "runtime",
        "browser",
    ]
    .iter()
    .any(|code| failure(code).message == message)
}

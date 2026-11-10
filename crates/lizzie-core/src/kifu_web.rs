//! What the public kifu services (野狐棋谱, 腾讯棋谱) share: the HTTP transport the platform adapter
//! supplies and the `org.json` reading rules Java applies to their replies.

use serde_json::{Map, Value};

/// The transport, as Java's `httpRequest` / `httpGet`: the body of any HTTP status, decoded as
/// UTF-8; `Err` with the failure text once its retries are spent.
pub trait Http {
    fn get(&mut self, url: &str, user_agent: &str) -> Result<String, String>;
    fn post_form(&mut self, url: &str, form: &[(&str, &str)], user_agent: &str) -> Result<String, String>;
}

/// `URLEncoder.encode(text, UTF_8)`.
pub fn form_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'.' | b'-' | b'*' | b'_' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// `new JSONObject(text)`.
pub(crate) fn object(text: &str) -> Result<Map<String, Value>, String> {
    match serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err("A JSONObject text must begin with '{'".into()),
        Err(e) => Err(e.to_string()),
    }
}

/// `optString(key, whole)`: the reply's own explanation, or the reply itself.
pub(crate) fn result_text(json: &Map<String, Value>, whole: &str) -> String {
    match json.get("resultstr") {
        None | Some(Value::Null) => whole.to_string(),
        Some(_) => opt_string(json, "resultstr"),
    }
}

/// `JSONObject.optString(key, "")`: strings as they are, other values in their JSON form.
pub(crate) fn opt_string(json: &Map<String, Value>, key: &str) -> String {
    match json.get(key) {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

/// `JSONObject.optInt` / `optLong`: numbers (fractions truncated) and numeric strings.
pub(crate) fn opt_int(json: &Map<String, Value>, key: &str, default: i64) -> i64 {
    json.get(key).and_then(as_int).unwrap_or(default)
}

pub(crate) fn as_int(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_f64().filter(|f| f.is_finite()).map(|f| f.trunc() as i64)),
        Value::String(s) => {
            let s = s.trim();
            s.parse::<i64>().ok().or_else(|| {
                s.parse::<f64>()
                    .ok()
                    .filter(|f| f.is_finite())
                    .map(|f| f.trunc() as i64)
            })
        }
        _ => None,
    }
}

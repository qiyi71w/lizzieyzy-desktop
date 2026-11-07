//! Pure SSE readers. The adapter owns stream closure/deadlines; cancellation is checked per line.
use std::{
    borrow::Cow,
    io::BufRead,
    sync::{
        atomic::{AtomicBool, Ordering},
        LazyLock,
    },
};

use fancy_regex::Regex;
use serde_json::Value;

use super::{chatgpt, CommentaryFailure};

pub(crate) fn json_string(value: &Value) -> Cow<'_, str> {
    match value {
        Value::Null => Cow::Borrowed(""),
        Value::String(value) => Cow::Borrowed(value),
        other => Cow::Owned(other.to_string()),
    }
}

pub(crate) fn check_cancel(cancel: &AtomicBool) -> Result<(), CommentaryFailure> {
    if cancel.load(Ordering::Acquire) {
        Err(CommentaryFailure {
            message_key: "Teacher.status.cancelled".into(),
            message: "AI commentary request was cancelled.".into(),
            cancelled: true,
        })
    } else {
        Ok(())
    }
}

fn incomplete() -> CommentaryFailure {
    CommentaryFailure {
        message_key: "Teacher.error.incomplete".into(),
        message: "The reply was interrupted and was not saved. Please try again.".into(),
        cancelled: false,
    }
}

/// Java TeacherLlmClient.sanitizeError: whitespace, exact secret, Bearer, then JSON fields.
/// Truncation belongs to the caller, after redaction.
pub fn sanitize_error(value: &str, secret: &str) -> String {
    static SPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t\n\x0B\f\r]+").expect("static whitespace regex"));
    static BEARER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)Bearer[ \t\n\x0B\f\r]+[A-Za-z0-9._~+\-/=]+").expect("static Bearer regex"));
    static FIELDS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?i)("(?:api[_-]?key|authorization|token)"[ \t\n\x0B\f\r]*:[ \t\n\x0B\f\r]*")[^"]+(")"#)
            .expect("static token regex")
    });
    let collapsed = SPACE.replace_all(value, " ");
    let mut detail = collapsed.trim_matches(|c: char| c <= '\u{20}').to_owned();
    if !secret.is_empty() {
        detail = detail.replace(secret, "[redacted]");
    }
    let detail = BEARER.replace_all(&detail, "Bearer [redacted]");
    FIELDS.replace_all(&detail, "${1}[redacted]${2}").into_owned()
}

pub(crate) fn short_detail(value: &str, secret: &str) -> String {
    let detail = sanitize_error(value, secret);
    if detail.encode_utf16().count() <= 240 {
        return detail;
    }
    let mut units = 0;
    let end = detail
        .char_indices()
        .find_map(|(i, c)| {
            units += c.len_utf16();
            (units > 240).then_some(i)
        })
        .unwrap_or(detail.len());
    format!("{}...", &detail[..end])
}

fn stream_failure(event: &Value, secret: &str) -> Option<CommentaryFailure> {
    let error = event.get("error").filter(|v| v.is_object()).or_else(|| {
        (event["type"] == "response.failed")
            .then(|| event.get("response")?.get("error").filter(|v| v.is_object()))
            .flatten()
    })?;
    let detail = short_detail(&json_string(&error["message"]), secret);
    Some(CommentaryFailure::new(if detail.is_empty() {
        "AI service stream failed.".into()
    } else {
        format!("AI service stream failed: {detail}")
    }))
}

pub fn parse_chat_completions(
    reader: &mut dyn BufRead,
    cancel: &AtomicBool,
    secret: &str,
) -> Result<String, CommentaryFailure> {
    parse_api_stream(reader, cancel, secret, false)
}

pub fn parse_api_responses(
    reader: &mut dyn BufRead,
    cancel: &AtomicBool,
    secret: &str,
) -> Result<String, CommentaryFailure> {
    parse_api_stream(reader, cancel, secret, true)
}

fn parse_api_stream(
    reader: &mut dyn BufRead,
    cancel: &AtomicBool,
    secret: &str,
    responses: bool,
) -> Result<String, CommentaryFailure> {
    let mut output = String::new();
    let mut line = String::new();
    let mut finished = false;
    check_cancel(cancel)?;
    loop {
        line.clear();
        let read = reader.read_line(&mut line);
        check_cancel(cancel)?;
        let size = read.map_err(|e| CommentaryFailure::new(e.to_string()))?;
        if size == 0 {
            break;
        }
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim_matches(|c: char| c <= '\u{20}');
        if data.is_empty() {
            continue;
        }
        if data == "[DONE]" {
            finished = !responses;
            break;
        }
        let event: Value = serde_json::from_str(data)
            .ok()
            .filter(Value::is_object)
            .ok_or_else(incomplete)?;
        if let Some(failure) = stream_failure(&event, secret) {
            return Err(failure);
        }
        let text;
        if responses {
            match event["type"].as_str().unwrap_or("") {
                "response.incomplete" | "response.failed" | "error" => return Err(incomplete()),
                "response.completed" => {
                    if event["response"]["status"] != "completed" {
                        return Err(incomplete());
                    }
                    finished = true;
                    break;
                }
                "response.output_text.delta" | "output_text_delta" => {
                    let delta = json_string(&event["delta"]);
                    text = if delta.is_empty() {
                        json_string(&event["content"])
                    } else {
                        delta
                    };
                }
                _ => text = Cow::Borrowed(""),
            }
        } else {
            let choice = &event["choices"][0];
            let reason = json_string(&choice["finish_reason"]);
            if !reason.is_empty() {
                if reason != "stop" {
                    return Err(incomplete());
                }
                finished = true;
            }
            text = json_string(&choice["delta"]["content"]);
        }
        output.push_str(&text);
        if finished {
            break;
        }
    }
    check_cancel(cancel)?;
    if !finished {
        return Err(incomplete());
    }
    Ok(output)
}

/// ChatGPT Responses SSE differs from the API-key protocol: events may span data lines,
/// only the canonical delta type is accepted, and explicit completion needs nonempty output.
pub fn parse_responses_stream(reader: &mut dyn BufRead, cancel: &AtomicBool) -> Result<String, CommentaryFailure> {
    const MAX_CHARS: usize = 1024 * 1024;
    let mut output = String::new();
    let mut output_chars = 0;
    let mut data = String::new();
    let mut event_chars = 0;
    let mut line = String::new();
    check_cancel(cancel)?;
    loop {
        line.clear();
        let read = reader.read_line(&mut line);
        check_cancel(cancel)?;
        let size = read.map_err(|_| chatgpt::failure("network"))?;
        if size == 0 {
            break;
        }
        let line = line.strip_suffix('\n').unwrap_or(&line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(part) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
                event_chars += 1;
            }
            let part = part.trim_start_matches(java_whitespace);
            event_chars += part.encode_utf16().count();
            if event_chars > MAX_CHARS {
                return Err(chatgpt::failure("protocol"));
            }
            data.push_str(part);
        } else if line.is_empty() && !data.is_empty() {
            let event: Value = serde_json::from_str(&data)
                .ok()
                .filter(Value::is_object)
                .ok_or_else(|| chatgpt::failure("incomplete"))?;
            data.clear();
            event_chars = 0;
            match event["type"].as_str().unwrap_or("") {
                "response.completed" => {
                    if event["response"]["status"] != "completed" || output.is_empty() {
                        return Err(chatgpt::failure("incomplete"));
                    }
                    check_cancel(cancel)?;
                    return Ok(output);
                }
                "response.failed" | "response.incomplete" | "error" => {
                    let payload = event.get("response").filter(|v| v.is_object()).unwrap_or(&event);
                    if payload.get("error").is_some_and(Value::is_object) {
                        return Err(chatgpt::classify_error(0, payload, "").failure());
                    }
                    return Err(chatgpt::failure("incomplete"));
                }
                "response.output_text.delta" => {
                    let delta = json_string(&event["delta"]);
                    output_chars += delta.encode_utf16().count();
                    if output_chars > MAX_CHARS {
                        return Err(chatgpt::failure("protocol"));
                    }
                    check_cancel(cancel)?;
                    output.push_str(&delta);
                }
                _ => {}
            }
        }
    }
    check_cancel(cancel)?;
    Err(chatgpt::failure("incomplete"))
}

pub(crate) fn java_whitespace(c: char) -> bool {
    matches!(c, '\u{9}'..='\u{d}' | '\u{1c}'..='\u{20}' | '\u{1680}' | '\u{2000}'..='\u{2006}' | '\u{2008}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{205f}' | '\u{3000}')
}

pub(crate) fn blank(value: &str) -> bool {
    value.chars().all(java_whitespace)
}

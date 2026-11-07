//! Frozen evidence, grounded generation and replaceable SGF commentary.
pub mod api_key;
pub mod chatgpt;
pub mod codec;
pub mod evidence;
pub mod grounded;
pub mod knowledge;
pub mod prompt;
pub mod settings;
pub mod stream;
pub mod verify;

use serde::{Deserialize, Serialize};
use std::sync::{atomic::AtomicBool, Arc};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}
impl Message {
    pub fn new(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentaryFailure {
    pub message_key: String,
    pub message: String,
    pub cancelled: bool,
}
impl CommentaryFailure {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message_key: String::new(),
            message: message.into(),
            cancelled: false,
        }
    }
    pub fn keyed(key: &str, message: impl Into<String>) -> Self {
        Self {
            message_key: key.into(),
            message: message.into(),
            cancelled: false,
        }
    }
    pub fn cancelled() -> Self {
        Self {
            message_key: "Teacher.status.cancelled".into(),
            message: "Cancelled".into(),
            cancelled: true,
        }
    }
}
impl std::fmt::Display for CommentaryFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for CommentaryFailure {}

pub trait ChatTransport {
    fn complete(&mut self, messages: &[Message], cancel: &AtomicBool) -> Result<String, CommentaryFailure>;
}
#[derive(Clone, Debug)]
pub enum JobConnection {
    ApiKey {
        base_url: String,
        model: String,
    },
    ChatGpt {
        account: String,
        model: String,
        effort: Option<String>,
    },
}
#[derive(Clone, Debug)]
pub struct CommentaryJob {
    pub id: u64,
    pub messages: Vec<Message>,
    pub positions: Vec<evidence::Position>,
    pub connection: JobConnection,
    pub cancel: Arc<AtomicBool>,
}

pub(crate) fn java_trim(value: &str) -> &str {
    value.trim_matches(|c: char| c <= '\u{20}')
}

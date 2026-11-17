//! 在线棋谱 / 弈客直播: following a live game from an online source (fixed Java `cb53acc`
//! `gui/OnlineDialog`, `YikeApiClient`, `YikeLiveDialog`, `BrowserFrame`).
//!
//! Protocols are sans-IO: a session turns transport events into [`Step`]s (frames to send, HTTP
//! reads it needs, and [`SourceUpdate`]s). The platform performs the IO; the workbench's follow
//! flow owns the session, its identity and every document write.

pub mod huanle;
pub mod rtgame;
pub mod url;
pub mod yike;

use std::time::Duration;

use crate::formats::{sgf, sgf_write};
use crate::record::{DocumentEdit, GameRecord, NodeId, NodeKind, SourceAction, ROOT};

/// A frame for the session's connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    Text(String),
    Binary(Vec<u8>),
}

/// A GET the session needs; its reply comes back through the session's `on_http` with `purpose`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpNeed {
    pub purpose: u32,
    pub url: String,
    pub headers: Vec<(String, String)>,
}

/// A text the follow status shows: a Java resource key (or a product key for Java's hard-coded
/// texts) with `%s`/`{0}` arguments in order.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
pub struct OnlineText {
    pub key: String,
    pub args: Vec<String>,
}

impl OnlineText {
    pub fn new(key: &str) -> Self {
        OnlineText { key: key.into(), args: Vec::new() }
    }

    pub fn with(key: &str, args: impl IntoIterator<Item = String>) -> Self {
        OnlineText { key: key.into(), args: args.into_iter().collect() }
    }
}

/// Facts a followed source reports. The follow flow applies them to the document.
#[derive(Clone, Debug)]
pub enum SourceUpdate {
    /// The source's game as it stands now: its main line is followed. `names` override the
    /// record's `PB`/`PW` when given; `komi` / `handicap` likewise (Java `live.optDouble("komi")`).
    Game {
        record: Box<GameRecord>,
        names: Option<(String, String)>,
        komi: Option<f64>,
        handicap: Option<u32>,
    },
    /// The game ended: `result` goes to the top of the live end's comment (when not blank) and,
    /// with `set_result`, into the game information (`update_game` does, the detail poll does not).
    Finished { result: String, set_result: bool },
    /// The room has no readable game and its source closed; `finished` when the room has a
    /// result, which the old detail API still serves (`initData` without history → `refresh`).
    NoGame { finished: bool },
    /// A comment for the source node at `move_number` of the main line, appended on a new line
    /// (`procComments`: `LiveMember：content`).
    Comment { move_number: u32, text: String },
    /// A comment reported before as `old` now reads `text`: its line in the node's comment is
    /// replaced (appended when the line is gone).
    CommentUpdate { move_number: u32, old: String, text: String },
    /// A commentator variation from the main line node at `move_number`: its moves and passes
    /// with their colours and comments, added as a new branch there.
    Branch { move_number: u32, actions: Vec<SourceAction> },
    /// A status for the follow status lines.
    Status(OnlineText),
    /// The follow stops with this status (a fatal protocol error, the game is over and the
    /// source closes).
    Stopped(OnlineText),
}

/// What a session asks of the platform after an event.
#[derive(Debug, Default)]
pub struct Step {
    pub send: Vec<Frame>,
    pub fetch: Vec<HttpNeed>,
    pub updates: Vec<SourceUpdate>,
    /// Close the current connection (before any `connect`).
    pub close: bool,
    /// Open a connection to `url` after `after` (the first connection, or a reconnect).
    pub connect: Option<Connect>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connect {
    pub url: String,
    pub after: Duration,
}

impl Step {
    pub fn update(update: SourceUpdate) -> Self {
        Step { updates: vec![update], ..Step::default() }
    }
}

/// The reply to an [`HttpNeed`] or a flow request: the status and body text, or a transport
/// failure (connection, timeout, a proxy that cannot be used). `proxy` failures carry
/// [`invalid_proxy_message`] as `message`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpReply {
    Ok { status: u16, body: String },
    Failed { proxy: bool, message: String },
}

/// Java `NetworkProxy.invalidSettings` for settings no request can use: manual mode without a
/// host or a port (only a hand-edited configuration holds them; saving refuses them).
pub fn invalid_proxy_message(settings: &crate::settings::NetworkSettings) -> Option<String> {
    if settings.proxy_mode != crate::settings::ProxyMode::Manual {
        return None;
    }
    let host = settings.proxy_host.trim();
    let invalid = if host.is_empty() {
        format!("Invalid network-proxy-host: {host}")
    } else if settings.proxy_port == 0 {
        "Invalid network-proxy-port: 0".to_string()
    } else {
        return None;
    };
    Some(format!(
        "Network proxy settings are invalid. Open Settings > Advanced > Network Proxy and fix {invalid}"
    ))
}

/// A connection event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SocketEvent {
    Opened,
    Text(String),
    Binary(Vec<u8>),
    /// The connection ended; `error` when it failed rather than closed.
    Closed { error: Option<String> },
    /// The connection was not opened: the network proxy settings cannot be used
    /// ([`invalid_proxy_message`]).
    Refused { message: String },
}

/// Java `SGFParser.parseBranch` of a branch's SGF text from the node `at` of the source's own
/// game: each move or pass with its colour (an empty or off-board point is a pass) and comment,
/// along the text's first line, played on the position at `at` (captures and legality are those
/// of that position). Java reads properties inside a nested game tree only in the MultiGo form
/// (the text ends with `))`); otherwise text from the first nested `(` on is not read.
pub fn branch_actions(text: &str, source: &GameRecord, at: NodeId) -> Vec<SourceAction> {
    let text = text.trim();
    let multigo = text.strip_suffix(')').is_some_and(|t| t.trim_end().ends_with(')'));
    let text = if multigo { text } else { &text[..top_level_open(text).unwrap_or(text.len())] };
    if text.is_empty() {
        return Vec::new();
    }
    // The source's line to `at` only, as SGF, with the branch text continuing it.
    let mut line = source.clone();
    let path = source.path_to(at);
    for (i, &n) in path.iter().enumerate() {
        for &child in &source.node(n).children {
            if path.get(i + 1) != Some(&child) {
                let _ = line.apply(&DocumentEdit::Remove { node: child });
            }
        }
    }
    let prefix = sgf_write::write(&line, "");
    let Some(close) = prefix.rfind(')') else { return Vec::new() };
    let (Ok(base), Ok(record)) =
        (sgf::read(&prefix), sgf::read(&format!("{}{text}{}", &prefix[..close], &prefix[close..])))
    else {
        return Vec::new();
    };
    let skip = base.path_to(base.line_end(ROOT)).len();
    record
        .path_to(record.line_end(ROOT))
        .into_iter()
        .skip(skip)
        .filter(|&n| matches!(record.node(n).kind, NodeKind::Move { .. } | NodeKind::Pass { .. }))
        .map(|n| SourceAction { kind: record.node(n).kind, comment: record.node(n).comment.clone() })
        .collect()
}

/// The first `(` outside a property value.
fn top_level_open(text: &str) -> Option<usize> {
    let (mut in_value, mut escaped) = (false, false);
    for (i, c) in text.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' if in_value => escaped = true,
            '[' if !in_value => in_value = true,
            ']' if in_value => in_value = false,
            '(' if !in_value => return Some(i),
            _ => {}
        }
    }
    None
}

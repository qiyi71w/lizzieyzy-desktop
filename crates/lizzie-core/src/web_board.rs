//! Web 旁观 protocol (Java `gui/web`, cb53acc): the messages a viewer's browser sends, and the
//! JSON the board stream sends to it (`WebBoardDataCollector`, `WebBoardManager`). The page
//! itself is Java's `web/index.html`, `board.js` and `board.css`, served by the platform.
//!
//! Positions use Java's layout: `stones` and `estimateArray` list the board column by column
//! (`Board.getIndex(x, y) = x * height + y`), 0 empty, 1 Black, 2 White.

use std::time::Duration;

use serde_json::{json, Map, Value};

use crate::api::{CandidateView, NodeAnalysisView};
use crate::engine::gtp::vertex;
use crate::record::{BoardSize, Point, Stone};

/// A 试下 ends after five minutes without an action of its owner (`IDLE_TIMEOUT_MS`).
pub const TRIAL_IDLE: Duration = Duration::from_secs(5 * 60);

/// One message from a viewer (`WebBoardManager.handleClientMessage`). Every message names the
/// viewer by the `clientId` its page keeps in local storage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientMessage {
    EnterTrial {
        client: String,
    },
    ExitTrial {
        client: String,
    },
    /// A stone on the shown position, column `x` from the left, row `y` from the top.
    TrialMove {
        client: String,
        x: u32,
        y: u32,
    },
    TrialBack {
        client: String,
    },
    /// The first continuation that is not a placeholder.
    TrialForward {
        client: String,
    },
    /// A continuation by its index among all of the shown node's children.
    TrialChild {
        client: String,
        index: u32,
    },
    TrialReset {
        client: String,
    },
}

/// `JSONObject.optInt`: a number or a numeric string.
fn opt_int(msg: &Map<String, Value>, key: &str) -> Option<i64> {
    match msg.get(key)? {
        Value::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// Reads a viewer's message; anything Java ignores (not JSON, an unknown type, a negative
/// coordinate or child index, an unknown direction) is `None`.
pub fn parse_client_message(text: &str) -> Option<ClientMessage> {
    let value: Value = serde_json::from_str(text).ok()?;
    let msg = value.as_object()?;
    let client = match msg.get("clientId") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    };
    let non_negative = |key: &str| {
        opt_int(msg, key)
            .filter(|&n| n >= 0)
            .and_then(|n| u32::try_from(n).ok())
    };
    match msg.get("type").and_then(Value::as_str)? {
        "enter_trial" => Some(ClientMessage::EnterTrial { client }),
        "exit_trial" => Some(ClientMessage::ExitTrial { client }),
        "trial_move" => Some(ClientMessage::TrialMove {
            client,
            x: non_negative("x")?,
            y: non_negative("y")?,
        }),
        "trial_navigate" if msg.contains_key("childIndex") => Some(ClientMessage::TrialChild {
            client,
            index: non_negative("childIndex")?,
        }),
        "trial_navigate" => match msg.get("direction").and_then(Value::as_str) {
            Some("back") => Some(ClientMessage::TrialBack { client }),
            Some("forward") => Some(ClientMessage::TrialForward { client }),
            _ => None,
        },
        "trial_reset" => Some(ClientMessage::TrialReset { client }),
        _ => None,
    }
}

/// The shown position and its stored analysis.
pub struct BoardState<'a> {
    pub size: BoardSize,
    /// Row by row from the top-left, as the record keeps it.
    pub stones: &'a [Option<Stone>],
    pub last_move: Option<Point>,
    pub move_number: u32,
    pub to_play: Stone,
    pub analysis: Option<&'a NodeAnalysisView>,
    /// Kata形势 of this position, Black-positive, row by row from the top-left.
    pub ownership: Option<&'a [f64]>,
}

/// Row-major board values listed column by column (Java's board index).
fn by_column<T: Copy>(size: BoardSize, values: &[T]) -> Vec<T> {
    let (w, h) = (size.width as usize, size.height as usize);
    (0..w).flat_map(|x| (0..h).map(move |y| values[y * w + x])).collect()
}

fn gtp(size: BoardSize, p: Option<Point>) -> String {
    p.map_or_else(|| "pass".to_string(), |p| vertex(size, p))
}

/// `WebBoardDataCollector.buildMoveDataJson`: numbers from the side to move.
fn candidate(size: BoardSize, c: &CandidateView) -> Value {
    let mut o = json!({
        "coordinate": gtp(size, c.point),
        "winrate": c.winrate,
        "playouts": c.visits,
        "scoreMean": c.score_lead,
        "scoreStdev": c.score_stdev,
        "policy": c.prior,
        "lcb": c.lcb,
        "order": c.order,
        "variation": c.pv.iter().map(|&p| gtp(size, p)).collect::<Vec<_>>(),
    });
    if let Some(p) = c.point {
        o["x"] = p.x.into();
        o["y"] = p.y.into();
    }
    o
}

/// The position's win rate and score lead for Black (`blackToPlay ? winrate : 100 - winrate`),
/// and its visits; no values for a position without analysis.
fn black_values(a: Option<&NodeAnalysisView>) -> (Option<f64>, Option<f64>, u64) {
    let Some(a) = a else { return (None, None, 0) };
    let black = a.to_play == Stone::Black;
    let winrate = if black { a.winrate } else { 100.0 - a.winrate };
    let lead = a.score_lead.map(|l| if black { l } else { -l });
    (Some(winrate), lead, a.visits)
}

fn analysis_fields(o: &mut Map<String, Value>, s: &BoardState) {
    let (winrate, lead, visits) = black_values(s.analysis);
    let moves: Vec<Value> = s.analysis.map_or_else(Vec::new, |a| {
        a.candidates.iter().map(|c| candidate(s.size, c)).collect()
    });
    o.insert("bestMoves".into(), moves.into());
    o.insert("winrate".into(), winrate.into());
    o.insert("scoreMean".into(), lead.into());
    o.insert("playouts".into(), visits.into());
    o.insert(
        "estimateArray".into(),
        s.ownership.map(|own| by_column(s.size, own)).into(),
    );
}

/// `full_state`: the shown position with its analysis.
pub fn full_state(s: &BoardState) -> String {
    let stones: Vec<u8> = by_column(s.size, s.stones)
        .into_iter()
        .map(|st| match st {
            None => 0,
            Some(Stone::Black) => 1,
            Some(Stone::White) => 2,
        })
        .collect();
    let mut o = Map::new();
    o.insert("type".into(), "full_state".into());
    o.insert("boardWidth".into(), s.size.width.into());
    o.insert("boardHeight".into(), s.size.height.into());
    o.insert("stones".into(), stones.into());
    o.insert("lastMove".into(), s.last_move.map(|p| vec![p.x, p.y]).into());
    o.insert("moveNumber".into(), s.move_number.into());
    o.insert(
        "currentPlayer".into(),
        if s.to_play == Stone::Black { "B" } else { "W" }.into(),
    );
    analysis_fields(&mut o, s);
    Value::Object(o).to_string()
}

/// `analysis_update`: new analysis of the same position; none while it has no candidates.
pub fn analysis_update(s: &BoardState) -> Option<String> {
    s.analysis.filter(|a| !a.candidates.is_empty())?;
    let mut o = Map::new();
    o.insert("type".into(), "analysis_update".into());
    analysis_fields(&mut o, s);
    Some(Value::Object(o).to_string())
}

/// One position from the root to the shown one, for the graphs and the 问题手 list.
pub struct HistoryEntry<'a> {
    pub move_number: u32,
    pub to_play: Stone,
    pub analysis: Option<&'a NodeAnalysisView>,
}

/// `winrate_history`: Black's win rate and score lead of each analysed position (null for the
/// others, so the graphs show no false values).
pub fn winrate_history(path: &[HistoryEntry]) -> String {
    let data: Vec<Value> = path
        .iter()
        .map(|e| {
            let (winrate, lead, _) = black_values(e.analysis.filter(|a| a.visits > 0));
            json!({
                "moveNumber": e.move_number,
                "blackToPlay": e.to_play == Stone::Black,
                "winrate": winrate,
                "scoreMean": lead,
            })
        })
        .collect();
    json!({ "type": "winrate_history", "data": data }).to_string()
}

/// A shown continuation other than the main one, numbered in child order.
pub struct SiblingMarker {
    pub point: Point,
    pub child_index: u32,
}

/// A running 试下 as the viewers see it.
pub struct TrialState {
    pub owner: String,
    pub anchor_move_number: u32,
    pub display_move_number: u32,
    pub can_back: bool,
    pub can_forward: bool,
    pub markers: Vec<SiblingMarker>,
}

/// `trial_state`; `None` once no 试下 runs.
pub fn trial_state(state: Option<&TrialState>) -> String {
    let Some(s) = state else {
        return json!({ "type": "trial_state", "active": false }).to_string();
    };
    let markers: Vec<Value> = s
        .markers
        .iter()
        .enumerate()
        .map(|(i, m)| json!({ "x": m.point.x, "y": m.point.y, "label": (i + 1).to_string(), "childIndex": m.child_index }))
        .collect();
    json!({
        "type": "trial_state",
        "active": true,
        "ownerClientId": s.owner,
        "anchorMoveNumber": s.anchor_move_number,
        "displayMoveNumber": s.display_move_number,
        "canBack": s.can_back,
        "canForward": s.can_forward,
        "siblingMarkers": markers,
    })
    .to_string()
}

/// Why a viewer may not start a 试下.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrialDenied {
    /// Another viewer's 试下 runs; its owner is named.
    InUse { owner: String },
    /// The foreground engine is missing or held by a game, a borrowed search or another mode.
    EngineBusy,
}

/// `trial_denied`, sent to the asking viewer only.
pub fn trial_denied(reason: &TrialDenied) -> String {
    let (reason, owner) = match reason {
        TrialDenied::InUse { owner } => ("in_use", owner.as_str()),
        TrialDenied::EngineBusy => ("engine_busy", ""),
    };
    json!({ "type": "trial_denied", "reason": reason, "ownerClientId": owner }).to_string()
}

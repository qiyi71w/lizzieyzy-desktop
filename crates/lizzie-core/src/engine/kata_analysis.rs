//! KataGo analysis engine protocol (`katago analysis`, Java `AnalysisEngine`): one JSON query per
//! position on stdin and one JSON answer per query on stdout, as automatic quick analysis and
//! flash analysis send them.
//!
//! A query names the position by its static anchor (`initialStones`, `initialPlayer`) and the real
//! actions after it (`moves`), like the GTP sync of [`super::runtime`]; win rates are reported for
//! the side to move (`reportAnalysisWinratesAs: SIDETOMOVE`), the unit of every stored result.

use serde_json::{json, Map, Value};

use super::gtp::{self, CandidateLine, InfoLine};
use crate::record::{GameRecord, NodeId, NodeKind, Point, Stone, ROOT};

/// What a query asks besides the position (`AnalysisEngine.sendRequest`).
#[derive(Clone, Debug, PartialEq)]
pub struct QueryOptions {
    pub max_visits: u32,
    pub komi: f64,
    /// `"tromp-taylor"` or a KataGo rules object (`resolveLocalAnalysisRules`).
    pub rules: Value,
    pub pv_visits: bool,
    pub ownership: bool,
    pub moves_ownership: bool,
}

fn color(c: Stone) -> &'static str {
    match c {
        Stone::Black => "B",
        Stone::White => "W",
    }
}

/// The query for the position at `node`, as one JSON line.
pub fn query(id: &str, rec: &GameRecord, node: NodeId, options: &QueryOptions) -> String {
    let path = rec.path_to(node);
    let anchor_at = path
        .iter()
        .rposition(|&n| n == ROOT || !rec.node(n).setup.is_empty())
        .unwrap_or(0);
    let anchor = path[anchor_at];
    let size = rec.size;
    let vertex = |p: Point| gtp::vertex(size, p);
    let moves: Vec<Value> = path[anchor_at + 1..]
        .iter()
        .filter_map(|&n| match rec.node(n).kind {
            NodeKind::Move { color: c, point } => Some(json!([color(c), vertex(point)])),
            NodeKind::Pass { color: c } => Some(json!([color(c), "pass"])),
            _ => None,
        })
        .collect();
    let pos = rec.position(anchor);
    let stones: Vec<Value> = pos
        .stones
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let p = Point {
                x: (i % size.width as usize) as u16,
                y: (i / size.width as usize) as u16,
            };
            s.map(|c| json!([color(c), vertex(p)]))
        })
        .collect();
    let mut q = Map::new();
    q.insert("id".into(), json!(id));
    q.insert("maxVisits".into(), json!(options.max_visits));
    q.insert("includePVVisits".into(), json!(options.pv_visits));
    q.insert("includeOwnership".into(), json!(options.ownership));
    q.insert(
        "includeMovesOwnership".into(),
        json!(options.ownership && options.moves_ownership),
    );
    if !stones.is_empty() {
        q.insert("initialStones".into(), Value::Array(stones));
    }
    q.insert("initialPlayer".into(), json!(color(rec.node(anchor).to_play)));
    q.insert("rules".into(), options.rules.clone());
    q.insert("komi".into(), json!(options.komi));
    q.insert("boardXSize".into(), json!(size.width));
    q.insert("boardYSize".into(), json!(size.height));
    let turns = moves.len();
    q.insert("moves".into(), Value::Array(moves));
    q.insert("analyzeTurns".into(), json!([turns]));
    q.insert(
        "overrideSettings".into(),
        json!({ "reportAnalysisWinratesAs": "SIDETOMOVE" }),
    );
    Value::Object(q).to_string()
}

/// One line the analysis engine printed.
#[derive(Clone, Debug, PartialEq)]
pub enum Response {
    /// The final answer to query `id`.
    Result { id: String, line: InfoLine },
    /// The engine rejected query `id` (or a line it could not attribute, `id: None`).
    Error { id: Option<String>, message: String },
    /// Anything else: warnings, partial results, text that is not JSON.
    Other,
}

fn percent(v: &Value) -> Option<f64> {
    v.as_f64().map(|x| x * 100.0)
}

fn doubles(v: Option<&Value>) -> Option<Vec<f64>> {
    v?.as_array()?.iter().map(Value::as_f64).collect()
}

/// Reads one stdout line (`AnalysisEngine.parseResult`, `Utils.getBestMovesFromJsonArray`).
pub fn parse(text: &str, size: crate::record::BoardSize) -> Response {
    let Ok(Value::Object(o)) = serde_json::from_str::<Value>(text.trim()) else {
        return Response::Other;
    };
    let id = o.get("id").and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    });
    if let Some(e) = o.get("error") {
        return Response::Error {
            id,
            message: e.as_str().map_or_else(|| e.to_string(), str::to_string),
        };
    }
    if o.get("isDuringSearch").and_then(Value::as_bool) == Some(true) {
        return Response::Other;
    }
    let (Some(id), Some(infos)) = (id, o.get("moveInfos").and_then(Value::as_array)) else {
        return Response::Other;
    };
    let mut candidates: Vec<CandidateLine> = infos
        .iter()
        .filter_map(|m| {
            let point = gtp::parse_vertex(size, m.get("move")?.as_str()?).ok()?;
            let visits = m.get("visits").and_then(Value::as_u64).unwrap_or(0);
            let pv = m
                .get("pv")
                .and_then(Value::as_array)
                .map(|pv| {
                    pv.iter()
                        .map_while(|v| gtp::parse_vertex(size, v.as_str()?).ok())
                        .collect()
                })
                .unwrap_or_default();
            Some(CandidateLine {
                point,
                visits,
                allocation_visits: m.get("edgeVisits").and_then(Value::as_u64).unwrap_or(visits),
                winrate: m.get("winrate").and_then(percent).unwrap_or(0.0),
                score_lead: m
                    .get("scoreLead")
                    .or_else(|| m.get("scoreMean"))
                    .and_then(Value::as_f64),
                score_stdev: m.get("scoreStdev").and_then(Value::as_f64),
                prior: m.get("prior").and_then(percent).unwrap_or(0.0),
                lcb: m.get("lcb").and_then(percent),
                order: m.get("order").and_then(Value::as_u64).unwrap_or(0) as u32,
                is_symmetry: m.get("isSymmetryOf").is_some(),
                pv,
                pv_visits: m
                    .get("pvVisits")
                    .and_then(Value::as_array)
                    .map(|v| v.iter().filter_map(Value::as_u64).collect())
                    .unwrap_or_default(),
                ownership: doubles(m.get("ownership")),
            })
        })
        .collect();
    candidates.sort_by_key(|c| c.order);
    let root = o.get("rootInfo");
    let root_visits = root.and_then(|r| r.get("visits")).and_then(Value::as_u64);
    Response::Result {
        id,
        line: InfoLine {
            candidates,
            root_visits,
            root_winrate: root.and_then(|r| r.get("winrate")).and_then(percent),
            root_score_lead: root.and_then(|r| r.get("scoreLead")).and_then(Value::as_f64),
            ownership: doubles(o.get("ownership")),
        },
    }
}

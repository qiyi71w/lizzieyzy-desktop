//! When the AI passes (Java `analysis/HumanSlAnalysisRunner`: `mayPass`, `SearchView.passEnds`,
//! `HUMAN_PASS_SHARE`): KataGo's own search decides, once the AI may pass at all.
use super::Candidate;
use crate::record::{GameRecord, NodeId, NodeKind};
use serde_json::Value;

/// KataGo's human configs consider passing only once 40% of the board was played, unless the
/// opponent just passed.
const BOARD_SHARE: f64 = 0.4;
/// After the opponent passed, passing too ends the game if it gives up at most this many points.
const TOLERANCE_POINTS: f64 = 1.0;
/// When the human policy is mostly pass, the game is over for players of that rank.
const HUMAN_SHARE: f64 = 0.5;

/// Whether the AI may pass at `node`: the opponent just passed, or 40% of the board was played.
pub fn allowed(record: &GameRecord, node: NodeId) -> bool {
    let n = record.node(node);
    matches!(n.kind, NodeKind::Pass { .. }) || f64::from(n.move_number) >= BOARD_SHARE * record.size.area() as f64
}

/// Whether the AI passes at `node`, where it may, after KataGo's search in `response`: its top
/// move is a pass, or the opponent just passed and passing too gives up at most a point, which
/// ends the game. Between equal moves, such as filling one's own territory under area scoring,
/// KataGo would otherwise play on forever.
pub fn ends(response: &Value, record: &GameRecord, node: NodeId) -> bool {
    let infos = response["moveInfos"].as_array().map(Vec::as_slice).unwrap_or_default();
    if top(infos).is_some_and(is_pass) {
        return true;
    }
    if !matches!(record.node(node).kind, NodeKind::Pass { .. }) {
        return false;
    }
    let lead = |info: &Value| info["scoreLead"].as_f64().unwrap_or(f64::NAN);
    let best = infos
        .iter()
        .enumerate()
        .find(|(i, info)| info["order"].as_u64().unwrap_or(*i as u64) == 0)
        .map_or(f64::NAN, |(_, info)| lead(info));
    let pass = infos.iter().rfind(|info| is_pass(info)).map_or(f64::NAN, lead);
    best - pass <= TOLERANCE_POINTS
}

/// KataGo's top move in `response` when it is not a pass.
pub fn search_move(response: &Value) -> Option<&str> {
    let infos = response["moveInfos"].as_array()?;
    top(infos).filter(|info| !is_pass(info))?["move"]
        .as_str()
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

/// Whether players of a policy level would mostly pass: `moves`, the pass included, hold at least
/// half of the policy on it.
pub fn human_would_pass(moves: &[Candidate]) -> bool {
    let (pass, sum) = moves
        .iter()
        .filter(|m| m.probability.is_finite() && m.probability > 0.0)
        .fold((0.0, 0.0), |(pass, sum), m| {
            let p = m.probability;
            (
                if m.vertex.eq_ignore_ascii_case("pass") {
                    pass + p
                } else {
                    pass
                },
                sum + p,
            )
        });
    sum > 0.0 && pass / sum >= HUMAN_SHARE
}

/// The search's top move: the one of order 0, else the first.
fn top(infos: &[Value]) -> Option<&Value> {
    infos
        .iter()
        .enumerate()
        .find(|(i, info)| info["order"].as_u64().unwrap_or(*i as u64) == 0)
        .map(|(_, info)| info)
        .or_else(|| infos.first())
}

fn is_pass(info: &Value) -> bool {
    info["move"]
        .as_str()
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("pass"))
}

//! The HumanSL companion: a KataGo analysis engine with the human model, and the queries AI 陪练
//! sends it (Java `analysis/HumanSlAnalysisRunner`).
use super::{Candidate, PlaySettings};
use crate::engine::{gtp, kata_analysis};
use crate::record::{GameRecord, NodeId, Point};
use serde_json::{json, Value};
use std::path::Path;

/// Both families sample the root from two symmetries (`rootNumSymmetriesToSample`).
pub const ROOT_SYMMETRIES: u32 = 2;
/// KataGo's `gtp_human5k_example.cfg` keeps 40 visits only to judge passing and resigning.
pub const PASS_CHECK_VISITS: u32 = 40;
const SEARCH_THREADS: u32 = 8;
/// Time kept back from a request's budget for the engine's last batch, its JSON and delivery.
const RETURN_RESERVE_SECONDS: f64 = 0.450;
const TIME_USE_RATIO: f64 = 0.85;

fn elite(profile: &str) -> bool {
    let profile = profile.trim().to_ascii_lowercase();
    profile.starts_with("proyear_") || matches!(profile.as_str(), "rank_7d" | "rank_8d" | "rank_9d")
}

pub fn command(base: &str, model: &Path) -> Result<String, &'static str> {
    let mut parts = gtp::split_command(base);
    if parts.first().is_none_or(String::is_empty) {
        return Err("HumanSlGame.error.noEngine");
    }
    if let Some(mode) = parts.iter_mut().find(|p| p.eq_ignore_ascii_case("gtp")) {
        *mode = "analysis".into();
    }
    if !parts.iter().any(|p| p == "analysis") {
        parts.insert(1, "analysis".into());
    }
    let path = model.to_string_lossy().into_owned();
    if let Some(at) = parts.iter().position(|p| p == "-human-model" || p == "--human-model") {
        if at + 1 < parts.len() {
            parts[at + 1] = path;
        } else {
            parts.push(path);
        }
    } else {
        parts.extend(["-human-model".into(), path]);
    }
    parts.extend([
        "-override-config".into(),
        "numAnalysisThreads=1,numSearchThreads=".into(),
    ]);
    gtp::join_command(&parts).ok_or("HumanSlGame.error.noEngine")
}

/// The position a query searches, under the session's komi and rules, and the seconds left of
/// the request's budget (`maxVisits` alone is not a clock, especially with HumanSL's weightless
/// exploration).
pub struct Position<'a> {
    pub record: &'a GameRecord,
    pub node: NodeId,
    pub komi: f64,
    pub rules: Value,
    pub seconds: f64,
}

fn base(id: &str, at: Position<'_>, max_visits: u32) -> Value {
    let options = kata_analysis::QueryOptions {
        max_visits,
        komi: at.komi,
        rules: at.rules,
        pv_visits: false,
        ownership: false,
        moves_ownership: false,
    };
    let mut q: Value =
        serde_json::from_str(&kata_analysis::query(id, at.record, at.node, &options)).expect("query JSON");
    let reserve = RETURN_RESERVE_SECONDS.min(at.seconds.max(0.0) / 2.0);
    q["overrideSettings"]["maxTime"] = json!((at.seconds.max(0.0) - reserve) * TIME_USE_RATIO);
    q
}

/// The human policy of `profile` from one visit: the readiness check (one symmetry) and a policy
/// level's move (two).
pub fn policy_query(id: &str, at: Position<'_>, profile: &str, symmetries: u32) -> String {
    let mut q = base(id, at, 1);
    q["includePolicy"] = json!(true);
    let settings = &mut q["overrideSettings"];
    settings["humanSLProfile"] = json!(profile);
    settings["ignorePreRootHistory"] = json!(false);
    settings["humanSLRootExploreProbWeightless"] = json!(if elite(profile) { 0.8 } else { 0.5 });
    settings["rootNumSymmetriesToSample"] = json!(symmetries.clamp(1, 8));
    q.to_string()
}

/// A search level's move: one search with every setting of the level. The chosen-move keys only
/// order the analysis engine's moveInfos; the move is chosen from them as genmove would
/// ([`super::choose::searched_move`]). The configs leave `wideRootNoise` at genmove's 0, which
/// the analysis engine would raise.
pub fn play_query(id: &str, at: Position<'_>, play: &PlaySettings) -> String {
    let mut q = base(id, at, play.max_visits);
    q["includePolicy"] = json!(true);
    let settings = &mut q["overrideSettings"];
    for (key, value) in [
        ("maxVisits", json!(play.max_visits)),
        ("numSearchThreads", json!(SEARCH_THREADS)),
        ("humanSLProfile", json!(play.profile)),
        ("humanSLChosenMoveProp", json!(1.0)),
        ("humanSLChosenMoveIgnorePass", json!(play.ignore_pass())),
        ("humanSLChosenMovePiklLambda", json!(play.pikl_lambda)),
        ("humanSLRootExploreProbWeightless", json!(play.root_explore_weightless)),
        ("humanSLRootExploreProbWeightful", json!(0.1)),
        ("humanSLPlaExploreProbWeightless", json!(0.0)),
        ("humanSLPlaExploreProbWeightful", json!(0.1)),
        ("humanSLOppExploreProbWeightless", json!(0.0)),
        ("humanSLOppExploreProbWeightful", json!(0.1)),
        ("humanSLCpuctExploration", json!(0.5)),
        ("humanSLCpuctPermanent", json!(play.cpuct_permanent)),
        ("chosenMoveTemperatureEarly", json!(play.temperature_early())),
        ("chosenMoveTemperature", json!(play.temperature())),
        ("chosenMoveTemperatureHalflife", json!(play.temperature_halflife())),
        (
            "chosenMoveTemperatureOnlyBelowProb",
            json!(play.temperature_only_below_prob),
        ),
        ("chosenMoveSubtract", json!(0.0)),
        ("chosenMovePrune", json!(0.0)),
        ("ignorePreRootHistory", json!(false)),
        ("rootNumSymmetriesToSample", json!(ROOT_SYMMETRIES)),
        ("useLcbForSelection", json!(false)),
        ("winLossUtilityFactor", json!(1.0)),
        ("staticScoreUtilityFactor", json!(play.static_score_utility)),
        ("dynamicScoreUtilityFactor", json!(play.dynamic_score_utility)),
        ("useUncertainty", json!(false)),
        ("subtreeValueBiasFactor", json!(0.0)),
        ("useNoisePruning", json!(false)),
        ("wideRootNoise", json!(0.0)),
    ] {
        settings[key] = value;
    }
    q.to_string()
}

/// KataGo's own search, without the human profile, on whether passing is best here: a policy
/// level's pass check.
pub fn pass_check_query(id: &str, at: Position<'_>) -> String {
    base(id, at, PASS_CHECK_VISITS).to_string()
}

/// The response's human policy, at the top level or in `rootInfo`.
pub fn human_policy(response: &Value) -> Option<&Value> {
    response
        .get("humanPolicy")
        .or_else(|| response.get("rootInfo").and_then(|v| v.get("humanPolicy")))
}

/// The legal moves of the response's human policy with their odds, in the policy's order; the pass
/// only when `allow_pass`.
pub fn policy(response: &Value, record: &GameRecord, node: NodeId, allow_pass: bool) -> Vec<Candidate> {
    let Some(policy) = human_policy(response) else {
        return Vec::new();
    };
    let size = record.size;
    let stones = &record.position(node).stones;
    let mut moves = Vec::new();
    let mut add = |vertex: String, probability: f64| {
        let legal = if vertex.eq_ignore_ascii_case("pass") {
            allow_pass
        } else {
            gtp::parse_vertex(size, &vertex)
                .ok()
                .flatten()
                .is_some_and(|p| stones[p.y as usize * size.width as usize + p.x as usize].is_none())
        };
        if legal && probability.is_finite() && probability > 0.0 {
            moves.push(Candidate { vertex, probability });
        }
    };
    if let Some(array) = policy.as_array() {
        for (i, v) in array.iter().enumerate() {
            if let Some(p) = v.as_f64() {
                if i == usize::from(size.width) * usize::from(size.height) {
                    add("pass".into(), p);
                } else if i < stones.len() {
                    add(
                        gtp::vertex(
                            size,
                            Point {
                                x: (i % usize::from(size.width)) as u16,
                                y: (i / usize::from(size.width)) as u16,
                            },
                        ),
                        p,
                    );
                }
            } else if let (Some(vertex), Some(p)) = (v[0].as_str(), v[1].as_f64()) {
                add(vertex.into(), p);
            }
        }
    } else if let Some(object) = policy.as_object() {
        for (vertex, p) in object {
            if let Some(p) = p.as_f64() {
                add(vertex.clone(), p);
            }
        }
    }
    moves
}

/// Whether the search's top move is a pass.
pub fn prefers_pass(response: &Value) -> bool {
    response["moveInfos"]
        .as_array()
        .and_then(|infos| infos.iter().min_by_key(|m| m["order"].as_u64().unwrap_or(u64::MAX)))
        .and_then(|m| m["move"].as_str())
        .is_some_and(|v| v.trim().eq_ignore_ascii_case("pass"))
}

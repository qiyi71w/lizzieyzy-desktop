use super::Candidate;
use crate::engine::{gtp, kata_analysis};
use crate::record::{GameRecord, NodeId, Point, Stone};
use crate::settings::{HumanSlTrainingSettings, OpponentPreset};
use serde_json::{json, Value};
use std::path::Path;

pub fn profile(s: &HumanSlTrainingSettings) -> String {
    match s.opponent {
        OpponentPreset::ModernPro => "proyear_2023".into(),
        OpponentPreset::Online9d => "rank_9d".into(),
        OpponentPreset::Rank => format!("rank_{}{}", s.rank, if s.dan_rank { "d" } else { "k" }),
    }
}
pub fn elite(profile: &str) -> bool {
    profile == "rank_9d" || profile.starts_with("proyear_")
}
pub fn visits(s: &HumanSlTrainingSettings) -> u32 {
    if s.opponent == OpponentPreset::ModernPro {
        128
    } else {
        64
    }
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

pub struct QueryBudget<'a> {
    pub visits: u32,
    pub allowed: &'a [String],
    pub seconds: f64,
    pub readiness: bool,
}
pub fn query(
    id: &str,
    record: &GameRecord,
    node: NodeId,
    settings: &HumanSlTrainingSettings,
    komi: f64,
    rules: Value,
    budget: QueryBudget<'_>,
) -> String {
    let QueryBudget {
        visits: max_visits,
        allowed,
        seconds,
        readiness,
    } = budget;
    let options = kata_analysis::QueryOptions {
        max_visits,
        komi,
        rules,
        pv_visits: false,
        ownership: false,
        moves_ownership: false,
    };
    let mut q: Value = serde_json::from_str(&kata_analysis::query(id, record, node, &options)).expect("query JSON");
    let profile = profile(settings);
    q["includePolicy"] = json!(true);
    q["overrideSettings"]["humanSLProfile"] = json!(profile);
    q["overrideSettings"]["ignorePreRootHistory"] = json!(false);
    q["overrideSettings"]["humanSLRootExploreProbWeightless"] = json!(if elite(&profile) { 0.8 } else { 0.5 });
    q["overrideSettings"]["rootNumSymmetriesToSample"] =
        json!(if !readiness && settings.opponent == OpponentPreset::ModernPro {
            2
        } else {
            1
        });
    q["overrideSettings"]["maxTime"] = json!((seconds - (seconds / 2.0).min(0.450)).max(0.001) * 0.85);
    if !allowed.is_empty() {
        q["overrideSettings"]["humanSLCpuctPermanent"] = json!(2.0);
        q["allowMoves"] = json!([{"player":if record.node(node).to_play == Stone::Black {"B"} else {"W"},"moves":allowed,"untilDepth":1}]);
    }
    q.to_string()
}

pub fn policy(response: &Value, record: &GameRecord, node: NodeId, allow_pass: bool) -> Vec<Candidate> {
    let Some(policy) = response
        .get("humanPolicy")
        .or_else(|| response.get("rootInfo").and_then(|v| v.get("humanPolicy")))
    else {
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

pub fn top(response: &Value) -> Option<&str> {
    response["moveInfos"]
        .as_array()?
        .iter()
        .min_by_key(|m| m["order"].as_u64().unwrap_or(u64::MAX))?["move"]
        .as_str()
}

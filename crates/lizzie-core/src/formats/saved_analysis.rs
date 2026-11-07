//! Analysis saved in a record: the primary engine's (`LZ` on a move, `LZOP` on the root or a
//! setup node) and 双引擎模式's secondary engine's (`LZ2`, `LZOP2`), read with the fixed Java
//! meaning (`SGFParser.applyAnalysisTagOnDetachedHistory`, `applyAnalysisHeader`,
//! `parseAnalysisPlayouts`, `applyParsedAnalysisInfo`). Both use the same payload
//! (`formatAnalysisPayload`).
//!
//! The payload's first line is the header `<engine> <winrate> <playouts> [<scoreMean>
//! [<scoreStdev> [<pda>]]] [rootVisits=<n>]`, the win rate being that of the side that just moved;
//! the second line holds the candidate moves and optional Black-positive ownership.

use crate::api::{CandidateView, NodeAnalysisView};
use crate::engine::gtp::{parse_vertex, vertex};
use crate::record::Property;
use crate::record::{BoardSize, NodeId, Stone};
use crate::viewpoint::ShownValues;

/// The analysis summary of one node as Java holds it in `BoardData`.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedAnalysis {
    pub engine: String,
    /// Percent, side to move (`BoardData.winrate`).
    pub winrate: f64,
    pub visits: u64,
    /// Present for KataGo data (`isKataData`), side to move.
    pub score_lead: Option<f64>,
    /// The fifth header field; absent in older files (unknown, not 0).
    pub score_stdev: Option<f64>,
    /// `pda`, the sixth header field (0 when absent).
    pub pda: f64,
}

/// The primary engine's tags: on a move node, and on the root or a setup node.
pub const PRIMARY: [&str; 2] = ["LZ", "LZOP"];
/// The 双引擎模式 secondary engine's tags (`isSecondaryEngineAnalysisTag`).
pub const SECONDARY: [&str; 2] = ["LZ2", "LZOP2"];

/// The node's saved analysis under `tags`; the last valid payload wins, as Java applies them in
/// order.
pub fn saved(properties: &[Property], tags: [&str; 2]) -> Option<SavedAnalysis> {
    properties
        .iter()
        .filter(|p| tags.contains(&p.id.as_str()))
        .rev()
        .find_map(|p| p.values.first().and_then(|v| read(v)))
}

/// The node's saved primary analysis.
pub fn primary(properties: &[Property]) -> Option<SavedAnalysis> {
    saved(properties, PRIMARY)
}

fn number(text: &str, default: f64) -> f64 {
    text.parse().unwrap_or(default)
}

/// `parseAnalysisPlayouts`: `12.3k`, `1m`, `1,234`.
fn playouts(text: &str) -> u64 {
    let lower = text.trim().to_ascii_lowercase().replace(',', "");
    let (digits, multiplier) = if let Some(n) = lower.strip_suffix('m') {
        (n, 1_000_000.0)
    } else if let Some(n) = lower.strip_suffix('k') {
        (n, 1_000.0)
    } else {
        (lower.as_str(), 1.0)
    };
    let kept: String = digits
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
        .collect();
    match kept.parse::<f64>() {
        Ok(v) if v <= 0.0 => 0,
        Ok(v) => (v * multiplier).round() as u64,
        Err(_) => lower
            .chars()
            .filter(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .unwrap_or(0),
    }
}

fn read(payload: &str) -> Option<SavedAnalysis> {
    let (line1, line2) = payload.split_once('\n').unwrap_or((payload, ""));
    let header: Vec<&str> = line1.split_whitespace().collect();
    if header.len() < 3 {
        return None;
    }
    // The last `rootVisits=` counts; an unreadable one falls back to the header's playouts.
    let root_visits = header
        .iter()
        .skip(6)
        .rev()
        .find_map(|t| t.strip_prefix("rootVisits="))
        .and_then(|v| v.parse::<u64>().ok());
    let mut saved = SavedAnalysis {
        engine: header[0].to_string(),
        winrate: 100.0 - number(header[1], 50.0),
        visits: root_visits.unwrap_or_else(|| playouts(header[2])),
        score_lead: header.get(3).map(|s| number(s, 0.0)),
        score_stdev: header.get(4).map(|s| number(s, 0.0)),
        pda: header.get(5).map_or(0.0, |s| number(s, 0.0)),
    };
    // A header without a score takes the best move's (`applyParsedAnalysisInfo`); its
    // uncertainty is the header's (none, so unknown).
    if saved.visits > 0 && saved.score_lead.is_none() {
        let best = line2.split("ownership").next().unwrap_or("");
        if let Some(first) = best.split(" info ").find(|v| !v.trim().is_empty()) {
            let tokens: Vec<&str> = first.split_whitespace().collect();
            let mut kata = false;
            let mut score = 0.0;
            let mut i = 0;
            while i + 1 < tokens.len() && tokens[i] != "pv" {
                match tokens[i] {
                    "scoreMean" => {
                        score = number(tokens[i + 1], 0.0);
                        kata = true;
                    }
                    "scoreStdev" => kata = true,
                    _ => {}
                }
                i += 2;
            }
            if kata {
                saved.score_lead = Some(score);
            }
        }
    }
    Some(saved)
}

/// Header information not exposed in the analysis display. Live results use the Java default
/// header layout; imported results retain the explicit score/PDA slots.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Metadata {
    pub pda: f64,
    pub header_slots: usize,
}

pub(crate) struct Payload {
    pub view: NodeAnalysisView,
    pub ownership: Option<Vec<f64>>,
    pub metadata: Metadata,
}

/// Detached-history parsing of the payload under `tags`: the header owns the node summary, not
/// the first candidate.
#[allow(clippy::too_many_arguments)]
pub(crate) fn load(
    properties: &[Property],
    tags: [&str; 2],
    node: NodeId,
    size: BoardSize,
    to_play: Stone,
    komi: f64,
    max_suggestions: usize,
    branch_length: u32,
) -> Option<Payload> {
    let (payload, summary) = properties
        .iter()
        .rev()
        .filter(|p| tags.contains(&p.id.as_str()))
        .filter_map(|p| p.values.first())
        .find_map(|p| read(p).map(|s| (p, s)))?;
    let (header, detail) = payload.split_once('\n').unwrap_or((payload, ""));
    let slots: Vec<_> = header.split_whitespace().collect();
    let root_visits = slots
        .iter()
        .skip(6)
        .filter_map(|t| t.strip_prefix("rootVisits="))
        .next_back()
        .and_then(|v| v.parse::<u64>().ok());
    let mut candidates = Vec::new();
    let mut ownership = None;
    if summary.visits > 0 && !detail.trim().is_empty() {
        let (moves, map) = detail
            .split_once("ownership")
            .map_or((detail, None), |(m, o)| (m, Some(o)));
        for variation in moves
            .split(" info ")
            .filter(|s| !s.trim().is_empty())
            .take(max_suggestions)
        {
            let tokens: Vec<_> = variation.split_whitespace().collect();
            let mut c = CandidateView {
                point: None,
                visits: 0,
                allocation_visits: 0,
                winrate: 0.0,
                score_lead: None,
                score_stdev: None,
                prior: 0.0,
                lcb: None,
                order: candidates.len() as u32,
                pv: Vec::new(),
                pv_visits: Vec::new(),
                pv_captures: Vec::new(),
                shown: ShownValues::default(),
                ownership: None,
                side_to_move: ShownValues::default(),
                kept: false,
            };
            let mut i = 0;
            let mut edge = None;
            let mut coordinate = None;
            while i + 1 < tokens.len() {
                let (key, value) = (tokens[i], tokens[i + 1]);
                if key == "pv" {
                    i += 1;
                    while i < tokens.len() && tokens[i] != "pvVisits" {
                        if let Ok(p) = parse_vertex(size, tokens[i]) {
                            c.pv.push(p);
                        }
                        i += 1;
                    }
                    if i < tokens.len() {
                        c.pv_visits = tokens[i + 1..].iter().filter_map(|v| v.parse().ok()).collect();
                    }
                    break;
                }
                match key {
                    "move" => coordinate = parse_vertex(size, value).ok(),
                    "visits" => c.visits = playouts(value),
                    "edgeVisits" => edge = value.parse().ok(),
                    "winrate" => {
                        c.winrate = playouts(value) as f64 / 100.0;
                        c.lcb = Some(c.winrate);
                    }
                    "prior" => {
                        c.prior = value
                            .parse::<i64>()
                            .map_or_else(|_| number(value, 0.0), |v| v as f64 / 100.0)
                    }
                    "scoreMean" => c.score_lead = Some(number(value, 0.0)),
                    "scoreStdev" => {
                        c.score_stdev = Some(number(value, 0.0));
                        c.score_lead.get_or_insert(0.0);
                    }
                    "order" => c.order = value.parse().unwrap_or(c.order),
                    _ => {}
                }
                i += 2;
            }
            let Some(point) = coordinate else { continue };
            c.point = point;
            c.allocation_visits = edge.unwrap_or(c.visits);
            if c.pv.is_empty() {
                c.pv.push(point);
            }
            if candidates.is_empty() {
                if let Some(stdev) = summary.score_stdev {
                    c.score_stdev = Some(stdev);
                }
            }
            if branch_length > 0 {
                c.pv.truncate(branch_length as usize);
            }
            candidates.push(c);
        }
        if !candidates.is_empty() {
            ownership = map.map(|o| {
                o.split_whitespace()
                    .map_while(|v| v.parse::<f64>().ok())
                    .map(|v| (v * 1000.0).round() / 1000.0 + 0.0)
                    .collect()
            });
        }
    }
    Some(Payload {
        view: NodeAnalysisView {
            node,
            engine: summary.engine,
            to_play,
            visits: summary.visits,
            root_visits,
            winrate: summary.winrate,
            score_lead: summary.score_lead,
            score_stdev: summary.score_lead.and(summary.score_stdev),
            komi,
            candidates,
            black: ShownValues::default(),
        },
        ownership,
        metadata: Metadata {
            pda: summary.pda,
            header_slots: slots.len().min(6),
        },
    })
}

/// `formatNodeData` / `bestMovesToString`: the same payload fixed Java reads.
pub(crate) fn write(a: &NodeAnalysisView, size: BoardSize, metadata: Metadata, ownership: Option<&Vec<f64>>) -> String {
    use std::fmt::Write as _;
    let mut out = format!(
        "{} {:.1} {}",
        a.engine,
        100.0 - a.winrate,
        crate::formats::winrate_comment::playouts_text(a.visits)
    );
    if a.score_lead.is_some() || a.root_visits.is_some() {
        let _ = write!(out, " {}", a.score_lead.unwrap_or(0.0));
        if a.root_visits.is_some() || metadata.header_slots == 0 || metadata.header_slots >= 5 {
            let _ = write!(out, " {}", a.score_stdev.unwrap_or(0.0));
        }
        if a.root_visits.is_some() || metadata.header_slots >= 6 || (metadata.header_slots == 0 && metadata.pda != 0.0)
        {
            let _ = write!(out, " {}", metadata.pda);
        }
    }
    if let Some(n) = a.root_visits {
        let _ = write!(out, " rootVisits={n}");
    }
    let coordinate = |p: Option<crate::record::Point>| p.map_or_else(|| "pass".to_string(), |p| vertex(size, p));
    for (i, c) in a.candidates.iter().enumerate() {
        out.push_str(if i == 0 { "\n" } else { " info " });
        let _ = write!(
            out,
            "move {} visits {} winrate {} prior {}",
            coordinate(c.point),
            c.visits,
            (c.winrate * 100.0) as i64,
            (c.prior * 100.0) as i64
        );
        if a.score_lead.is_some() {
            let _ = write!(out, " scoreMean {:.2}", c.score_lead.unwrap_or(0.0));
        }
        let _ = write!(out, " order {}", c.order);
        if c.allocation_visits != c.visits {
            let _ = write!(out, " edgeVisits {}", c.allocation_visits);
        }
        out.push_str(" pv");
        for &p in &c.pv {
            let _ = write!(out, " {}", coordinate(p));
        }
        if a.score_lead.is_some() && !c.pv_visits.is_empty() {
            out.push_str(" pvVisits");
            for n in &c.pv_visits {
                let _ = write!(out, " {n}");
            }
        }
    }
    if let Some(map) = ownership.filter(|v| !v.is_empty()) {
        if a.candidates.is_empty() {
            out.push('\n');
        }
        out.push_str(" ownership");
        for v in map {
            let _ = write!(out, " {v}");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prop(id: &str, value: &str) -> Property {
        Property {
            id: id.into(),
            values: vec![value.into()],
        }
    }

    #[test]
    fn the_header_gives_the_side_to_moves_win_rate_and_root_visits() {
        let a = primary(&[prop(
            "LZ",
            "b11 44.0 2400 1.2 0.7 0.5 rootVisits=2500\nmove D4 visits 1500 winrate 5600 scoreMean 9 pv D4",
        )])
        .unwrap();
        assert_eq!(
            a,
            SavedAnalysis {
                engine: "b11".into(),
                winrate: 56.0,
                visits: 2500,
                score_lead: Some(1.2),
                score_stdev: Some(0.7),
                pda: 0.5,
            }
        );
    }

    #[test]
    fn playouts_read_suffixes_and_a_scoreless_header_takes_the_best_moves_score() {
        let a = primary(&[prop(
            "LZOP",
            "b11 99.8 102k\nmove D4 visits 2638 winrate 20 scoreMean -18.1 pv D4 C4",
        )])
        .unwrap();
        assert_eq!((a.visits, a.score_lead, a.score_stdev), (102_000, Some(-18.1), None));
        let leela = primary(&[prop("LZ", "LeelaZero 45.0 1.3m\nmove D4 visits 9 winrate 4500 pv D4")]).unwrap();
        assert_eq!((leela.visits, leela.score_lead), (1_300_000, None));
    }

    #[test]
    fn short_or_missing_payloads_give_nothing_and_the_last_valid_one_wins() {
        assert_eq!(primary(&[prop("LZ", "b11 44.0")]), None);
        assert_eq!(primary(&[prop("LZ2", "b11 44.0 100 1 1")]), None);
        let a = primary(&[prop("LZ", "a 40 10 1 1"), prop("LZ", "b 30 20 2 2"), prop("LZ", "bad")]).unwrap();
        assert_eq!((a.engine.as_str(), a.visits), ("b", 20));
    }
}

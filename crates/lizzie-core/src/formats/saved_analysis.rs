//! The primary analysis saved in a record (`LZ` on a move, `LZOP` on the root or a setup node),
//! read with the fixed Java meaning (`SGFParser.applyAnalysisTagOnDetachedHistory`,
//! `applyAnalysisHeader`, `parseAnalysisPlayouts`, `applyParsedAnalysisInfo`).
//!
//! The payload's first line is the header `<engine> <winrate> <playouts> [<scoreMean>
//! [<scoreStdev> [<pda>]]] [rootVisits=<n>]`, the win rate being that of the side that just moved;
//! the second line holds the candidate moves. Only the summary values Java keeps on the node are
//! read here: the candidates themselves stay in the property as written.

use crate::record::Property;

/// The analysis summary of one node as Java holds it in `BoardData`.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedAnalysis {
    pub engine: String,
    /// Percent, side to move (`BoardData.winrate`).
    pub winrate: f64,
    pub visits: u64,
    /// Present for KataGo data (`isKataData`), side to move.
    pub score_lead: Option<f64>,
    pub score_stdev: f64,
    /// `pda`, the sixth header field (0 when absent).
    pub pda: f64,
}

/// The node's saved primary analysis; the last valid `LZ`/`LZOP` payload wins, as Java applies
/// them in order.
pub fn primary(properties: &[Property]) -> Option<SavedAnalysis> {
    properties
        .iter()
        .filter(|p| p.id == "LZ" || p.id == "LZOP")
        .rev()
        .find_map(|p| p.values.first().and_then(|v| read(v)))
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
        score_stdev: header.get(4).map_or(0.0, |s| number(s, 0.0)),
        pda: header.get(5).map_or(0.0, |s| number(s, 0.0)),
    };
    // A header without a score takes the best move's (`applyParsedAnalysisInfo`); its
    // uncertainty is the header's (none, so 0).
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
                score_stdev: 0.7,
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
        assert_eq!((a.visits, a.score_lead, a.score_stdev), (102_000, Some(-18.1), 0.0));
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

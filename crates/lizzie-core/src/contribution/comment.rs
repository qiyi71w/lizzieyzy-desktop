//! `SGFParser.formatCommentPk` (`SGFParser.java:2546-2690`) for a watched contribution game: the
//! KataGo summary written into the analysed node's comment (the position before the move). The
//! win rate compares with the same player's previous move (two plies back); the side shown is
//! the side to move.

use crate::api::CommentLabels;
use crate::formats::winrate_comment::playouts_text;
use crate::record::Stone;

/// One analysed position (`BoardData`), side to move.
#[derive(Clone, Debug, PartialEq)]
pub struct PkEval {
    pub engine: String,
    pub visits: u64,
    /// Percent, side to move.
    pub winrate: f64,
    pub score_lead: Option<f64>,
    pub score_stdev: Option<f64>,
    pub komi: f64,
    pub to_play: Stone,
}

/// The summary. `previous` is the analysis of the node two plies back (`node.previous().previous()`),
/// as Java compares win rates of the same player. `root` reproduces the early return for a node
/// without a parent: Java writes an empty comment there. A non-empty `comment` keeps its text
/// with the summary after a blank line.
pub fn format(
    labels: &CommentLabels,
    lead_with_komi: bool,
    current: &PkEval,
    previous: Option<&PkEval>,
    comment: &str,
    root: bool,
) -> String {
    if root {
        return String::new();
    }
    let engine: String = current.engine.chars().filter(|&c| c != ' ').collect();
    let playouts = playouts_text(current.visits);
    let last = previous.map(|p| {
        (
            p.winrate,
            if p.visits > 0 { p.score_lead } else { None },
        )
    });
    let last_move_diff = match last {
        Some((winrate, _)) => {
            let diff = current.winrate - winrate;
            format!("({}{:.1}%)", if diff > 0.0 { "+" } else { "-" }, diff.abs())
        }
        None => String::new(),
    };
    let who = if current.to_play == Stone::Black {
        &labels.black
    } else {
        &labels.white
    };
    let Some(lead) = current.score_lead else {
        let summary = format!(
            "{who} {} {:.1}% {last_move_diff}\n({engine} / {playouts} {})\n{} {:.1}",
            labels.winrate, current.winrate, labels.playouts, labels.komi, current.komi
        );
        return merge(comment, summary);
    };
    let score = if lead_with_komi {
        if current.to_play == Stone::Black {
            lead + current.komi
        } else {
            lead - current.komi
        }
    } else {
        lead
    };
    let diff_score = match last {
        Some((_, Some(previous_lead))) if previous.is_some_and(|p| p.visits > 0) => {
            let diff = lead - previous_lead;
            format!("({}{:.1})", if diff > 0.0 { "+" } else { "-" }, diff.abs())
        }
        _ => String::new(),
    };
    let lead_label = if lead_with_komi {
        &labels.lead_with_komi
    } else {
        &labels.lead
    };
    let stdev = current.score_stdev.unwrap_or(0.0);
    let summary = format!(
        "{who} {} {:.1}% {last_move_diff}\n{lead_label} {score:.1} {diff_score} {} {stdev:.1}\n({engine} / {playouts} {})\n{} {:.1}",
        labels.winrate, current.winrate, labels.stdev, labels.playouts, labels.komi, current.komi
    );
    merge(comment, summary)
}

fn merge(comment: &str, summary: String) -> String {
    if comment.is_empty() {
        summary
    } else {
        format!("{comment}\n\n{summary}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels() -> CommentLabels {
        CommentLabels {
            black: "黑棋".into(),
            white: "白棋".into(),
            winrate: "胜率:".into(),
            lead: "领先:".into(),
            lead_with_komi: "盘面:".into(),
            stdev: "不确定度:".into(),
            playouts: "计算量".into(),
            komi: "贴目:".into(),
            pda: "PDA:".into(),
            wrn: "广度:".into(),
        }
    }

    fn eval(winrate: f64, lead: f64, to_play: Stone) -> PkEval {
        PkEval {
            engine: "Kata Go".into(),
            visits: 201,
            winrate,
            score_lead: Some(lead),
            score_stdev: Some(11.96),
            komi: 7.5,
            to_play,
        }
    }

    #[test]
    fn the_summary_compares_with_the_same_player_two_plies_back() {
        let previous = eval(30.0, -1.0, Stone::Black);
        let current = eval(40.0, 0.5, Stone::Black);
        let text = format(&labels(), false, &current, Some(&previous), "", false);
        assert_eq!(
            text,
            "黑棋 胜率: 40.0% (+10.0%)\n领先: 0.5 (+1.5) 不确定度: 12.0\n(KataGo / 201 计算量)\n贴目: 7.5"
        );
    }

    #[test]
    fn the_root_position_has_no_summary() {
        assert_eq!(format(&labels(), false, &eval(50.0, 0.0, Stone::Black), None, "规则", true), "");
    }

    #[test]
    fn a_existing_comment_keeps_its_text() {
        let text = format(&labels(), true, &eval(50.0, 1.0, Stone::White), None, "user note", false);
        assert!(text.starts_with("user note\n\n白棋 胜率: 50.0% "));
        assert!(text.contains("盘面: -6.5 "));
    }
}

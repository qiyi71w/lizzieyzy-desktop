//! The win-rate summary Java writes into an analysed node's comment when exporting
//! 保存纯净棋谱(带评论) (`SGFParser.formatComment`, `winrateCommentRegex`, `Utils.getPlayoutsString`).
//!
//! Java's display options that change this text (`win-rate-always-black`,
//! `show-katago-score-lead-with-komi`) are not settings of this product yet; their Java defaults
//! (both off) apply. The words come from the UI language ([`CommentLabels`]).

use crate::api::CommentLabels;
use crate::record::Stone;

/// The stored analysis of one node. Win rate and score lead are from the side to move, like
/// Java `BoardData.winrate` / `scoreMean`.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeEval {
    pub engine: String,
    pub visits: u64,
    /// Percent, side to move.
    pub winrate: f64,
    /// KataGo / SAI score lead, side to move.
    pub score_lead: Option<f64>,
    /// KataGo's score uncertainty; SAI reports none.
    pub score_stdev: Option<f64>,
    pub komi: f64,
    pub to_play: Stone,
}

/// The comment of an analysed node (`formatComment`). `previous` is the parent's analysis when the
/// node has a parent (`None` inside means the parent was not analysed: Java's unanalysed 50 %);
/// `comment` is the node's own comment, which keeps its text: an earlier summary in it is replaced,
/// otherwise the summary follows after a blank line.
pub fn format(labels: &CommentLabels, eval: &NodeEval, previous: Option<Option<&NodeEval>>, comment: &str) -> String {
    let engine: String = eval.engine.chars().filter(|&c| c != ' ').collect();
    let playouts = playouts_text(eval.visits);
    let has_previous = previous.is_some();
    let last_wr = previous.flatten().map_or(50.0, |p| p.winrate);
    let cur_wr = eval.winrate;
    let last_move_diff = if has_previous {
        let diff = 100.0 - last_wr - cur_wr;
        format!("({}{:.1}%)", if diff > 0.0 { "+" } else { "-" }, diff.abs())
    } else {
        String::new()
    };
    // The side that just moved: Black when White is to play.
    let mover = if eval.to_play == Stone::White {
        &labels.black
    } else {
        &labels.white
    };
    let winrate = format!("{:.1}%", 100.0 - cur_wr);
    let summary = match eval.score_lead {
        Some(lead) => {
            let diff_score = match previous.flatten() {
                Some(p) if p.visits > 0 => {
                    let diff = -lead - p.score_lead.unwrap_or(0.0);
                    format!("({}{:.1})", if diff > 0.0 { "+" } else { "-" }, diff.abs())
                }
                _ => String::new(),
            };
            let score = format!("{:.1}", -lead);
            match eval.score_stdev {
                Some(stdev) => format!(
                    "{mover} {} {winrate} {last_move_diff}\n{} {score} {diff_score} {} {:.1}\n({engine} / {playouts} {})\n{} {:.1}",
                    labels.winrate, labels.lead, labels.stdev, stdev, labels.playouts, labels.komi, eval.komi
                ),
                None => format!(
                    "{mover} {} {winrate} {last_move_diff}\n{} {score} {diff_score}\n({engine} / {playouts} {})\n{} {:.1}",
                    labels.winrate, labels.lead, labels.playouts, labels.komi, eval.komi
                ),
            }
        }
        None => format!(
            "{mover} {} {winrate} {last_move_diff}\n({engine} / {playouts} {})\n{} {:.1}",
            labels.winrate, labels.playouts, labels.komi, eval.komi
        ),
    };
    if comment.is_empty() {
        return summary;
    }
    let kind = match (eval.score_lead.is_some(), eval.score_stdev.is_some()) {
        (false, _) => Kind::Plain,
        (true, true) => Kind::Kata,
        (true, false) => Kind::Sai,
    };
    let pattern = summary_pattern(labels, kind);
    // Java tests with `(?s)` over the whole comment but replaces with the bare pattern.
    let found = fancy_regex::Regex::new(&format!("(?s).*{pattern}(?s).*"))
        .ok()
        .and_then(|r| r.is_match(comment).ok())
        .unwrap_or(false);
    if found {
        if let Ok(re) = fancy_regex::Regex::new(&pattern) {
            return re.replace_all(comment, fancy_regex::NoExpand(&summary)).into_owned();
        }
    }
    format!("{comment}\n\n{summary}")
}

#[derive(Clone, Copy)]
enum Kind {
    Plain,
    Kata,
    Sai,
}

/// `winrateCommentRegex(isKataData, isSaiData)`.
fn summary_pattern(labels: &CommentLabels, kind: Kind) -> String {
    let e = fancy_regex::escape;
    let head = format!(
        "({} |{} ){} [0-9\\.\\-]+%* \\(*[0-9.\\-+]*%*\\)*\\n",
        e(&labels.black),
        e(&labels.white),
        e(&labels.winrate)
    );
    let tail = format!(
        "\\(.* / [0-9\\.]*[kmKM]* {}\\)\\n{}.*",
        e(&labels.playouts),
        e(&labels.komi)
    );
    match kind {
        Kind::Plain => format!("{head}{tail}"),
        Kind::Sai => format!("{head}{} [0-9\\.\\-+]* \\(*[0-9.\\-+]*\\)*\\n{tail}", e(&labels.lead)),
        Kind::Kata => format!(
            "{head}{} [0-9\\.\\-+]* \\(*[0-9.\\-+]*\\)* {} [0-9\\.\\-+]*\\n{tail}",
            e(&labels.lead),
            e(&labels.stdev)
        ),
    }
}

/// `Utils.getPlayoutsString`: 345, 1.3k, 45k, 1235k, 12.3m.
pub fn playouts_text(visits: u64) -> String {
    let tenths = |v: f64| {
        let n = v.round() as u64;
        format!("{}.{}", n / 10, n % 10)
    };
    if visits >= 10_000_000 {
        format!("{}m", tenths(visits as f64 / 100_000.0))
    } else if visits >= 9950 {
        format!("{}k", (visits as f64 / 1000.0).round() as u64)
    } else if visits >= 1000 {
        format!("{}k", tenths(visits as f64 / 100.0))
    } else {
        visits.to_string()
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
            stdev: "不确定度:".into(),
            playouts: "计算量".into(),
            komi: "贴目:".into(),
        }
    }

    fn eval(winrate: f64, lead: Option<f64>, to_play: Stone) -> NodeEval {
        NodeEval {
            engine: "Kata Go".into(),
            visits: 201,
            winrate,
            score_lead: lead,
            score_stdev: lead.map(|_| 11.96),
            komi: 7.5,
            to_play,
        }
    }

    #[test]
    fn playouts_are_shortened_like_java() {
        assert_eq!(playouts_text(345), "345");
        assert_eq!(playouts_text(1000), "1.0k");
        assert_eq!(playouts_text(1265), "1.3k");
        assert_eq!(playouts_text(9949), "9.9k");
        assert_eq!(playouts_text(44556), "45k");
        assert_eq!(playouts_text(1_234_567), "1235k");
        assert_eq!(playouts_text(12_345_678), "12.3m");
    }

    #[test]
    fn the_root_summary_matches_the_java_text() {
        // Java autoGame1.sgf root: Black to play, Black 32.8 %, lead -0.876 for Black.
        let text = format(&labels(), &eval(32.8, Some(-0.876249), Stone::Black), None, "");
        assert_eq!(
            text,
            "白棋 胜率: 67.2% \n领先: 0.9  不确定度: 12.0\n(KataGo / 201 计算量)\n贴目: 7.5"
        );
    }

    #[test]
    fn a_move_summary_shows_the_change_and_replaces_an_earlier_one() {
        let prev = eval(32.8, Some(-0.9), Stone::Black);
        let cur = eval(70.0, Some(1.2), Stone::White);
        let first = format(&labels(), &cur, Some(Some(&prev)), "好手");
        assert_eq!(
            first,
            "好手\n\n黑棋 胜率: 30.0% (-2.8%)\n领先: -1.2 (-0.3) 不确定度: 12.0\n(KataGo / 201 计算量)\n贴目: 7.5"
        );
        let again = format(&labels(), &cur, Some(Some(&prev)), &first);
        assert_eq!(again, first);
    }
}

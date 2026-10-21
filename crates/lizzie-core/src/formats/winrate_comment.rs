//! The win-rate summary Java writes into an analysed node's comment when exporting
//! 保存纯净棋谱(带评论) (`SGFParser.formatComment`, `winrateCommentRegex`, `Utils.getPlayoutsString`).
//!
//! The side and lead follow the display settings through [`Viewpoint`], like every display: the
//! side that just moved, or Black with 总是显示黑胜率; the lead after komi (目差) or the board
//! difference (盘面). The words come from the UI language ([`CommentLabels`]).
//!
//! Objective correction against the fixed Java build: Java turned the lead's change to Black's
//! side by the turn of the position shown on the board when exporting
//! (`Lizzie.board.getHistory().isBlacksTurn()`), so the same file got different signs depending
//! on where the user stood. Here it turns exactly when the shown side is not the side that moved,
//! like the win-rate change next to it.

use crate::api::CommentLabels;
use crate::record::Stone;
use crate::viewpoint::Viewpoint;

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
    /// KataGo's PDA (`playoutDoublingAdvantage`) saved with the analysis; 0 when none.
    pub pda: f64,
    pub to_play: Stone,
}

/// The comment of an analysed node (`formatComment`). `previous` is the parent's analysis when the
/// node has a parent (`None` inside means the parent was not analysed: Java's unanalysed 50 %);
/// `comment` is the node's own comment, which keeps its text: an earlier summary in it is replaced,
/// otherwise the summary follows after a blank line.
pub fn format(
    labels: &CommentLabels,
    view: Viewpoint,
    eval: &NodeEval,
    previous: Option<Option<&NodeEval>>,
    comment: &str,
) -> String {
    let engine: String = eval.engine.chars().filter(|&c| c != ' ').collect();
    let playouts = playouts_text(eval.visits);
    let mover = eval.to_play.opposite();
    let side = view.side(mover);
    let shown = Viewpoint::winrate(eval.winrate, eval.to_play, side);
    let last_move_diff = match previous {
        // Java compares with the parent's own number (its side to move, the side that moved
        // here), or Black's with 总是显示黑胜率; an unanalysed parent counts as 50 %.
        Some(p) => {
            let last = p.map_or(50.0, |p| {
                if view.always_black {
                    Viewpoint::winrate(p.winrate, p.to_play, Stone::Black)
                } else {
                    p.winrate
                }
            });
            let diff = shown - last;
            format!("({}{:.1}%)", if diff > 0.0 { "+" } else { "-" }, diff.abs())
        }
        None => String::new(),
    };
    let who = if side == Stone::Black {
        &labels.black
    } else {
        &labels.white
    };
    let lead_label = lead_label(labels, view);
    let winrate = format!("{shown:.1}%");
    let summary = match eval.score_lead {
        Some(lead) => {
            let diff_score = match previous.flatten() {
                Some(p) if p.visits > 0 => {
                    // The mover's change (komi cancels), turned to the shown side.
                    let change = -lead - p.score_lead.unwrap_or(0.0);
                    let diff = if side == mover { change } else { -change };
                    format!("({}{:.1})", if diff > 0.0 { "+" } else { "-" }, diff.abs())
                }
                _ => String::new(),
            };
            let score = format!("{:.1}", view.lead(lead, eval.to_play, side, eval.komi));
            match eval.score_stdev {
                Some(stdev) => format!(
                    "{who} {} {winrate} {last_move_diff}\n{lead_label} {score} {diff_score} {} {:.1}\n({engine} / {playouts} {})\n{} {:.1}{}",
                    labels.winrate,
                    labels.stdev,
                    stdev,
                    labels.playouts,
                    labels.komi,
                    eval.komi,
                    // Java appends a non-zero PDA as `Double.toString` writes it.
                    if eval.pda != 0.0 {
                        format!(" {}{}", labels.pda, java_double(eval.pda))
                    } else {
                        String::new()
                    }
                ),
                None => format!(
                    "{who} {} {winrate} {last_move_diff}\n{lead_label} {score} {diff_score}\n({engine} / {playouts} {})\n{} {:.1}",
                    labels.winrate, labels.playouts, labels.komi, eval.komi
                ),
            }
        }
        None => format!(
            "{who} {} {winrate} {last_move_diff}\n({engine} / {playouts} {})\n{} {:.1}",
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
    let pattern = summary_pattern(labels, view, kind);
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

/// `Double.toString` for the values a PDA takes (a whole number keeps `.0`).
fn java_double(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e7 {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

#[derive(Clone, Copy)]
enum Kind {
    Plain,
    Kata,
    Sai,
}

/// `SGFParse.leadWithKomi` under 盘面, else `SGFParse.leadJustScore`.
fn lead_label(labels: &CommentLabels, view: Viewpoint) -> &str {
    if view.lead_with_komi {
        &labels.lead_with_komi
    } else {
        &labels.lead
    }
}

/// `winrateCommentRegex(isKataData, isSaiData)`.
fn summary_pattern(labels: &CommentLabels, view: Viewpoint, kind: Kind) -> String {
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
    let lead = e(lead_label(labels, view));
    match kind {
        Kind::Plain => format!("{head}{tail}"),
        Kind::Sai => format!("{head}{lead} [0-9\\.\\-+]* \\(*[0-9.\\-+]*\\)*\\n{tail}"),
        Kind::Kata => format!(
            "{head}{lead} [0-9\\.\\-+]* \\(*[0-9.\\-+]*\\)* {} [0-9\\.\\-+]*\\n{tail}",
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
            lead_with_komi: "盘面:".into(),
            stdev: "不确定度:".into(),
            playouts: "计算量".into(),
            komi: "贴目:".into(),
            pda: "PDA:".into(),
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
            pda: 0.0,
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
        let text = format(
            &labels(),
            Viewpoint::default(),
            &eval(32.8, Some(-0.876249), Stone::Black),
            None,
            "",
        );
        assert_eq!(
            text,
            "白棋 胜率: 67.2% \n领先: 0.9  不确定度: 12.0\n(KataGo / 201 计算量)\n贴目: 7.5"
        );
    }

    #[test]
    fn a_move_summary_shows_the_change_and_replaces_an_earlier_one() {
        let prev = eval(32.8, Some(-0.9), Stone::Black);
        let cur = eval(70.0, Some(1.2), Stone::White);
        let first = format(&labels(), Viewpoint::default(), &cur, Some(Some(&prev)), "好手");
        assert_eq!(
            first,
            "好手\n\n黑棋 胜率: 30.0% (-2.8%)\n领先: -1.2 (-0.3) 不确定度: 12.0\n(KataGo / 201 计算量)\n贴目: 7.5"
        );
        let again = format(&labels(), Viewpoint::default(), &cur, Some(Some(&prev)), &first);
        assert_eq!(again, first);
    }

    #[test]
    fn a_saved_pda_follows_the_komi() {
        let cur = NodeEval {
            pda: 1.0,
            ..eval(70.0, Some(1.2), Stone::White)
        };
        let text = format(&labels(), Viewpoint::default(), &cur, None, "");
        assert!(text.ends_with("贴目: 7.5 PDA:1.0"), "{text}");
    }
}

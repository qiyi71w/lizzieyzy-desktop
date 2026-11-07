//! Whose win rate and score lead are shown (Java `win-rate-always-black`,
//! `show-katago-score-lead-with-komi`). Every display and the 带评论 export convert the engine's
//! numbers (win rate and KataGo score lead of the side to move) through this one rule set.
//!
//! The fixed Java surfaces choose the side differently but convert alike:
//! - board candidates and the candidate table: the side to move, Black with 总是显示黑胜率
//!   (`BoardRenderer.drawLeelazSuggestions`, `Utils.convertScoreToString`, `LizzieFrame` table);
//! - win-rate graph, statistics panel and title: always Black (`WinrateGraph`,
//!   `drawMoveStatistics`, `updateTitle`);
//! - the export comment: the side that just moved, Black with 总是显示黑胜率
//!   (`SGFParser.formatComment`).
//!
//! With 盘面 a lead becomes the board difference: plus the komi for Black, minus it for White.

use serde::Serialize;
use ts_rs::TS;

use crate::record::Stone;
use crate::settings::DisplaySettings;

/// A candidate's numbers as shown on the board and in the candidate table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, TS)]
pub struct ShownValues {
    /// Percent, for the shown side.
    pub winrate: f64,
    /// For the shown side, with the komi when 盘面 is chosen.
    pub score_lead: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Viewpoint {
    pub always_black: bool,
    pub lead_with_komi: bool,
}

impl From<&DisplaySettings> for Viewpoint {
    fn from(s: &DisplaySettings) -> Self {
        Viewpoint {
            always_black: s.winrate_always_black,
            lead_with_komi: s.score_lead_with_komi,
        }
    }
}

impl Viewpoint {
    /// The side shown where `natural` is shown by default.
    pub fn side(self, natural: Stone) -> Stone {
        if self.always_black {
            Stone::Black
        } else {
            natural
        }
    }

    /// `winrate` (percent, for `to_play`) for `side`.
    pub fn winrate(winrate: f64, to_play: Stone, side: Stone) -> f64 {
        if side == to_play {
            winrate
        } else {
            100.0 - winrate
        }
    }

    /// KataGo's `lead` (for `to_play`) for `side`, as a board difference with 盘面.
    pub fn lead(self, lead: f64, to_play: Stone, side: Stone, komi: f64) -> f64 {
        let lead = if side == to_play { lead } else { -lead };
        match (self.lead_with_komi, side) {
            (false, _) => lead,
            (true, Stone::Black) => lead + komi,
            (true, Stone::White) => lead - komi,
        }
    }

    fn values(self, winrate: f64, lead: Option<f64>, to_play: Stone, side: Stone, komi: f64) -> ShownValues {
        ShownValues {
            winrate: Self::winrate(winrate, to_play, side),
            score_lead: lead.map(|l| self.lead(l, to_play, side, komi)),
        }
    }

    /// A candidate of a position with `to_play` to move, for the board and the table.
    pub fn candidate(self, winrate: f64, lead: Option<f64>, to_play: Stone, komi: f64) -> ShownValues {
        self.values(winrate, lead, to_play, self.side(to_play), komi)
    }

    /// The side to move's own numbers with 盘面 applied, ignoring 总是显示黑胜率: the 独立选点列表
    /// (`AnalysisFrame.getValueAt`).
    pub fn side_to_move(self, winrate: f64, lead: Option<f64>, to_play: Stone, komi: f64) -> ShownValues {
        self.values(winrate, lead, to_play, to_play, komi)
    }

    /// Black's numbers: graph, statistics panel and title.
    pub fn black(self, winrate: f64, lead: Option<f64>, to_play: Stone, komi: f64) -> ShownValues {
        self.values(winrate, lead, to_play, Stone::Black, komi)
    }

    /// A position's result (for `to_play`) as a candidate of the previous position, whose side
    /// to move made the move (the candidate table's actual-move row).
    pub fn as_candidate(self, winrate: f64, lead: Option<f64>, to_play: Stone, komi: f64) -> AsCandidate {
        let mover = to_play.opposite();
        let (winrate, score_lead) = (Self::winrate(winrate, to_play, mover), lead.map(|l| -l));
        AsCandidate {
            winrate,
            score_lead,
            shown: self.candidate(winrate, score_lead, mover, komi),
            side_to_move: self.side_to_move(winrate, score_lead, mover, komi),
        }
    }
}

/// A position's result seen as a candidate of the previous position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, TS)]
pub struct AsCandidate {
    /// Percent, for the side that moved (the table compares it with the best candidate's).
    pub winrate: f64,
    /// For the side that moved.
    pub score_lead: Option<f64>,
    pub shown: ShownValues,
    /// For the side that moved, 盘面 applied, 总是显示黑胜率 ignored (独立选点列表).
    pub side_to_move: ShownValues,
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOTH: [Viewpoint; 4] = [
        Viewpoint {
            always_black: false,
            lead_with_komi: false,
        },
        Viewpoint {
            always_black: true,
            lead_with_komi: false,
        },
        Viewpoint {
            always_black: false,
            lead_with_komi: true,
        },
        Viewpoint {
            always_black: true,
            lead_with_komi: true,
        },
    ];

    /// `Utils.convertScoreToString` and the docked table, transcribed: Black to move adds the
    /// komi with 盘面; White to move subtracts it, then turns to Black with 总是显示黑胜率.
    fn java_board(v: Viewpoint, score: f64, black_to_play: bool, komi: f64) -> f64 {
        let mut s = score;
        if black_to_play {
            if v.lead_with_komi {
                s += komi;
            }
        } else {
            if v.lead_with_komi {
                s -= komi;
            }
            if v.always_black {
                s = -s;
            }
        }
        s
    }

    /// `drawMoveStatistics` / `updateTitle` / `WinrateGraph` (Black, plus komi with 盘面).
    fn java_black(v: Viewpoint, score: f64, black_to_play: bool, komi: f64) -> f64 {
        let black = if black_to_play { score } else { -score };
        if v.lead_with_komi {
            black + komi
        } else {
            black
        }
    }

    #[test]
    fn board_table_and_black_values_follow_the_java_formulas() {
        for v in BOTH {
            for (to_play, black) in [(Stone::Black, true), (Stone::White, false)] {
                let shown = v.candidate(62.0, Some(3.5), to_play, 7.5);
                assert_eq!(
                    shown.score_lead,
                    Some(java_board(v, 3.5, black, 7.5)),
                    "{v:?} {to_play:?}"
                );
                let wr = if v.always_black && !black { 38.0 } else { 62.0 };
                assert_eq!(shown.winrate, wr, "{v:?} {to_play:?}");
                let b = v.black(62.0, Some(3.5), to_play, 7.5);
                assert_eq!(b.winrate, if black { 62.0 } else { 38.0 });
                assert_eq!(b.score_lead, Some(java_black(v, 3.5, black, 7.5)));
                // The same position as the previous position's candidate (the table's actual
                // move): the mover's raw numbers.
                let row = v.as_candidate(62.0, Some(3.5), to_play, 7.5);
                assert_eq!((row.winrate, row.score_lead), (38.0, Some(-3.5)));
            }
        }
    }
}

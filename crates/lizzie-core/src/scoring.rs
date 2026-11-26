//! 终局点目: counting a final position with the user's dead-stone and 不计入 marks (Java
//! `Board.getGroupInfo` / `toggleDeadStoneOrEmptyPoint` / `showGroupResult` and
//! `ScoreResult.setScore`).
//!
//! An empty region counts for a side when every stone bordering it is that side's living stone
//! or the other side's dead stone; any other region is neutral. Dead stones count as points of
//! the side whose area they lie in and as its prisoners.

use crate::api::{ScorePoint, ScoreVerdict, ScoreView};
use crate::record::{BoardSize, GameRecord, NodeId, NodeKind, Point, Position, Stone};

/// The player who passed first on the way to `node`, if anyone did (`FirstPassDraw.firstPasser`).
/// A `DD[true]` placeholder is no player's pass.
pub fn first_passer(record: &GameRecord, node: NodeId) -> Option<Stone> {
    let mut first = None;
    let mut at = Some(node);
    while let Some(n) = at {
        if let NodeKind::Pass { color } = record.node(n).kind {
            first = Some(color);
        }
        at = record.parent(n);
    }
    first
}

/// The count of a finished game from KataGo's final score lead for Black (`GameResult.byScore`),
/// rounded to what the komi allows: with a half-point komi every result is a half point, with a
/// whole-point komi a draw is possible. Under rules that give a drawn count to the player who
/// passed first, `first_passer` is that player; KataGo counts the rule as half a point for them
/// (its button), which comes off again here, so a draw goes to them by no margin and every other
/// result keeps its whole-point margin. `None` when the lead or the komi is not a number.
pub fn verdict_of_lead(black_lead: f64, komi: f64, first_passer: Option<Stone>) -> Option<ScoreVerdict> {
    if !black_lead.is_finite() || !komi.is_finite() {
        return None;
    }
    let lead = match first_passer {
        Some(Stone::Black) => black_lead - 0.5,
        Some(Stone::White) => black_lead + 0.5,
        None => black_lead,
    };
    let fraction = komi - komi.floor();
    // Java `Math.round`: half up.
    let rounded = (lead - fraction + 0.5).floor() + fraction;
    Some(verdict(rounded, 0.0, first_passer))
}

/// The verdict of the totals (`CountVerdict`): equal totals are a draw, or `draw_winner`'s when
/// the rules give a drawn count to the first player to pass.
fn verdict(black_total: f64, white_total: f64, draw_winner: Option<Stone>) -> ScoreVerdict {
    let margin = black_total - white_total;
    if margin.abs() < 1e-9 {
        return match draw_winner {
            Some(winner) => ScoreVerdict::DrawToFirstPasser { winner },
            None => ScoreVerdict::Draw,
        };
    }
    let winner = if margin > 0.0 { Stone::Black } else { Stone::White };
    let margin = margin.abs();
    // Java: a whole margin as a long, else `%.1f` (half up).
    let margin = if margin == margin.trunc() {
        format!("{}", margin as i64)
    } else {
        format!("{:.1}", (margin * 10.0).round() / 10.0)
    };
    ScoreVerdict::Win { winner, margin }
}

/// The SGF `RE` of a verdict (`GameResult.sgf`): `B+3.5`, `0` for a draw, `B+` for a draw won by
/// passing first.
pub fn sgf_result(verdict: &ScoreVerdict) -> String {
    let side = |s: Stone| if s == Stone::Black { "B+" } else { "W+" };
    match verdict {
        ScoreVerdict::Draw => "0".into(),
        ScoreVerdict::DrawToFirstPasser { winner } => side(*winner).into(),
        ScoreVerdict::Win { winner, margin } => format!("{}{margin}", side(*winner)),
    }
}

/// The marks of one scoring session, on the stones it started from.
#[derive(Clone, Debug, PartialEq)]
pub struct Marks {
    pub size: BoardSize,
    pub stones: Vec<Option<Stone>>,
    pub dead: Vec<bool>,
    pub excluded: Vec<bool>,
}

impl Marks {
    pub fn new(position: &Position) -> Marks {
        let n = position.stones.len();
        Marks {
            size: position.size,
            stones: position.stones.clone(),
            dead: vec![false; n],
            excluded: vec![false; n],
        }
    }

    fn neighbours(&self, k: usize) -> impl Iterator<Item = usize> {
        let (w, h) = (self.size.width as usize, self.size.height as usize);
        let (x, y) = (k % w, k / w);
        [
            (x > 0).then(|| k - 1),
            (y > 0).then(|| k - w),
            (x + 1 < w).then(|| k + 1),
            (y + 1 < h).then(|| k + w),
        ]
        .into_iter()
        .flatten()
    }

    /// A left click: an empty point toggles 不计入; a stone toggles, as dead or alive, every
    /// stone of its colour reachable through that colour's stones and empty points
    /// (`toggleDeadStone`).
    pub fn toggle(&mut self, p: Point) {
        let k = self.size.index(p);
        let Some(color) = self.stones.get(k).copied() else {
            return;
        };
        let Some(color) = color else {
            self.excluded[k] = !self.excluded[k];
            return;
        };
        let mut seen = vec![false; self.stones.len()];
        let mut stack = vec![k];
        while let Some(k) = stack.pop() {
            if seen[k] {
                continue;
            }
            match self.stones[k] {
                Some(c) if c == color => self.dead[k] = !self.dead[k],
                Some(_) => continue,
                None => {}
            }
            seen[k] = true;
            stack.extend(self.neighbours(k));
        }
    }

    /// Whose area each point is (`groupStatus.value`).
    fn owners(&self) -> Vec<Option<Stone>> {
        let mut owner: Vec<Option<Stone>> = self
            .stones
            .iter()
            .zip(&self.dead)
            .map(|(s, &dead)| s.map(|c| if dead { c.opposite() } else { c }))
            .collect();
        let mut seen = vec![false; self.stones.len()];
        for start in 0..self.stones.len() {
            if seen[start] || self.stones[start].is_some() {
                continue;
            }
            let (mut region, mut borders) = (Vec::new(), (false, false));
            let mut stack = vec![start];
            while let Some(k) = stack.pop() {
                if seen[k] || self.stones[k].is_some() {
                    continue;
                }
                seen[k] = true;
                region.push(k);
                for n in self.neighbours(k) {
                    if let Some(c) = self.stones[n] {
                        let side = if self.dead[n] { c.opposite() } else { c };
                        if side == Stone::Black {
                            borders.0 = true
                        } else {
                            borders.1 = true
                        }
                    } else {
                        stack.push(n);
                    }
                }
            }
            let side = match borders {
                (true, false) => Some(Stone::Black),
                (false, true) => Some(Stone::White),
                _ => None,
            };
            for k in region {
                owner[k] = side;
            }
        }
        owner
    }

    /// The count with `komi` and the captures of the scored position (`showGroupResult`,
    /// `ScoreResult.setScore`). `handicap_bonus` is White's handicap compensation under area
    /// scoring (KataGo `whiteHandicapBonus` N or N-1 applied to the handicap stones);
    /// `draw_winner` wins equal totals (KataGo's button rules: the first player to pass).
    pub fn score(
        &self,
        node: NodeId,
        position: &Position,
        komi: f64,
        territory: bool,
        handicap_bonus: u32,
        draw_winner: Option<Stone>,
    ) -> ScoreView {
        let owners = self.owners();
        let (mut black_alive, mut white_alive, mut black_points, mut white_points) = (0u32, 0u32, 0u32, 0u32);
        let (mut black_captures, mut white_captures) = (position.black_captures, position.white_captures);
        let mut points = Vec::with_capacity(owners.len());
        for (k, &owner) in owners.iter().enumerate() {
            let stone = self.stones[k];
            points.push(ScorePoint {
                owner,
                dead: stone.is_some() && self.dead[k],
                excluded: stone.is_none() && self.excluded[k],
            });
            if self.excluded[k] && stone.is_none() {
                continue;
            }
            match owner {
                Some(Stone::Black) if stone == Some(Stone::Black) => black_alive += 1,
                Some(Stone::Black) => {
                    if stone == Some(Stone::White) {
                        black_captures += 1;
                    }
                    black_points += 1;
                }
                Some(Stone::White) if stone == Some(Stone::White) => white_alive += 1,
                Some(Stone::White) => {
                    if stone == Some(Stone::Black) {
                        white_captures += 1;
                    }
                    white_points += 1;
                }
                None => {}
            }
        }
        let black_alive = black_alive.saturating_sub(handicap_bonus);
        let (black_total, white_total) = if territory {
            (
                black_points as f64 - white_captures as f64,
                white_points as f64 - black_captures as f64 + komi,
            )
        } else {
            (
                (black_alive + black_points) as f64,
                (white_alive + white_points) as f64 + komi,
            )
        };
        let verdict = verdict(black_total, white_total, draw_winner);
        let result = sgf_result(&verdict);
        ScoreView {
            node,
            points,
            territory,
            black_alive,
            white_alive,
            black_points,
            white_points,
            black_captures,
            white_captures,
            komi,
            black_total,
            white_total,
            verdict,
            result,
        }
    }
}

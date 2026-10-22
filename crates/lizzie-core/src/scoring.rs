//! 终局点目: counting a final position with the user's dead-stone and 不计入 marks (Java
//! `Board.getGroupInfo` / `toggleDeadStoneOrEmptyPoint` / `showGroupResult` and
//! `ScoreResult.setScore`).
//!
//! An empty region counts for a side when every stone bordering it is that side's living stone
//! or the other side's dead stone; any other region is neutral. Dead stones count as points of
//! the side whose area they lie in and as its prisoners.

use crate::api::{ScorePoint, ScoreView};
use crate::record::{BoardSize, NodeId, Point, Position, Stone};

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
    /// scoring (KataGo `whiteHandicapBonus` N or N-1 applied to the handicap stones).
    pub fn score(
        &self,
        node: NodeId,
        position: &Position,
        komi: f64,
        territory: bool,
        handicap_bonus: u32,
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
        let diff = (black_total - white_total).abs();
        let result = format!("{}+{diff:.1}", if black_total >= white_total { "B" } else { "W" });
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
            result,
        }
    }
}

//! 死活题 (Java `rules/Tsumego`, `gui/TsumeGoFrame`): surround a local life-and-death problem with
//! an attacker's wall and fill the rest of the board so an engine reads only that problem.
//!
//! [`build_frame`] is the whole construction on a plain position; the workbench turns its result
//! into the new root position of the open record. Java facts kept here:
//!
//! - The cover box is the bounding box of all stones widened by 墙面间距 + 1 on every side; a side
//!   that would reach the board edge (or beyond) is left open. A box open on all four sides is no
//!   problem to frame: nothing changes.
//! - The attacker (`side`) builds the wall around the box and owns the part of the board nearest
//!   the problem, roughly half the board plus four points of komi; the defender owns the rest. The
//!   territories are a checkerboard of stones with a solid front line.
//! - Ko threats are small fixed shapes in board corners; a shape that has no room is reported
//!   ("没有布置 [x方] 劫材的空间") and left out.
//! - The komi is 7.5 for the attacker, or twice the attacker's area beyond half the board when the
//!   problem is too big for half the board, negated when White attacks and kept within ±150.
//! - The Java loops are kept statement for statement: on one point the first stone placed wins
//!   (`Utils.addStone`) and a ko shape overwrites (`removeAndAddStone`).

pub mod capture;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::record::{BoardSize, Stone};

/// 先手方 (`tsume-go-to-play`: 1 继承, 2 黑先, 3 白先).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum TsumegoToPlay {
    /// The side to play of the position stays.
    #[default]
    Keep,
    Black,
    White,
}

/// 进攻方 (`tsume-go-attaker`: 1 自动检测, 2 黑方, 3 白方).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum TsumegoAttacker {
    #[default]
    Auto,
    Black,
    White,
}

/// 添加劫材 (`tsume-go-ko-threat`: 1 双方, 2 进攻方, 3 防守方, 4 无).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum TsumegoKoThreat {
    #[default]
    Both,
    Attacker,
    Defender,
    None,
}

impl TsumegoKoThreat {
    fn attacker(self) -> bool {
        matches!(self, TsumegoKoThreat::Both | TsumegoKoThreat::Attacker)
    }

    fn defender(self) -> bool {
        matches!(self, TsumegoKoThreat::Both | TsumegoKoThreat::Defender)
    }
}

/// The choices of the 死活题 dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameOptions {
    pub to_play: TsumegoToPlay,
    pub attacker: TsumegoAttacker,
    pub ko_threat: TsumegoKoThreat,
    /// 墙面间距: empty lines between the problem's stones and the wall.
    pub wall_distance: i32,
}

/// A framed position.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// Row-major like [`crate::record::Position::stones`].
    pub stones: Vec<Option<Stone>>,
    pub black_to_play: bool,
    pub komi: f64,
    /// The colours named in the "no room for ko threats" messages, in Java's order.
    pub no_room: Vec<Stone>,
}

/// The attacker chosen (also when nothing is framed) and the framed position, if any.
#[derive(Clone, Debug, PartialEq)]
pub struct FrameOutcome {
    pub attacker: Stone,
    pub frame: Option<Frame>,
}

/// `Tsumego.getCoverSideAndIndex` + `buildCoverWall` on `stones` (row-major for `size`).
pub fn build_frame(
    size: BoardSize,
    stones: &[Option<Stone>],
    black_to_play: bool,
    options: &FrameOptions,
) -> FrameOutcome {
    let mut grid = Grid {
        w: i64::from(size.width),
        h: i64::from(size.height),
        stones: stones.to_vec(),
    };
    let side = attacker(&grid, options.attacker);
    let gap = i64::from(options.wall_distance) + 1;
    let Some(bounds) = grid.cover_box(gap) else {
        return FrameOutcome {
            attacker: side,
            frame: None,
        };
    };
    let komi = grid.cover(side, bounds, options.ko_threat.defender(), options.ko_threat.attacker());
    let to_play = match options.to_play {
        TsumegoToPlay::Keep => black_to_play,
        TsumegoToPlay::Black => true,
        TsumegoToPlay::White => false,
    };
    FrameOutcome {
        attacker: side,
        frame: Some(Frame {
            stones: grid.stones,
            black_to_play: to_play,
            komi: komi.0,
            no_room: komi.1,
        }),
    }
}

/// The attacker. Java compares per-colour bounding boxes, but those start as the whole board and
/// only ever grow (`if (i < blackLeft)` with `blackLeft = 0`), so every comparison is a tie and
/// automatic detection is the stone count: the colour with more stones, Black on equal counts.
fn attacker(grid: &Grid, choice: TsumegoAttacker) -> Stone {
    match choice {
        TsumegoAttacker::Black => Stone::Black,
        TsumegoAttacker::White => Stone::White,
        TsumegoAttacker::Auto => {
            let black = grid.stones.iter().filter(|s| **s == Some(Stone::Black)).count();
            let white = grid.stones.iter().filter(|s| **s == Some(Stone::White)).count();
            if black >= white {
                Stone::Black
            } else {
                Stone::White
            }
        }
    }
}

/// The cover box: `-1` / width / height stand for a side left open at the board edge.
#[derive(Clone, Copy)]
struct Bounds {
    left: i64,
    right: i64,
    top: i64,
    bottom: i64,
}

struct Grid {
    w: i64,
    h: i64,
    stones: Vec<Option<Stone>>,
}

/// The ko-threat shape's two rows, from the corner inward: `[empty, p, q, empty]` on the edge and
/// `[p, p, p, empty]` next to it.
#[derive(Clone, Copy)]
enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Grid {
    fn index(&self, x: i64, y: i64) -> usize {
        (y * self.w + x) as usize
    }

    /// `Utils.addStone`: only on an empty point.
    fn add(&mut self, x: i64, y: i64, color: Stone) {
        let i = self.index(x, y);
        if self.stones[i].is_none() {
            self.stones[i] = Some(color);
        }
    }

    /// `Tsumego.removeAndAddStone`: the point becomes `color` (or empty).
    fn put(&mut self, x: i64, y: i64, color: Option<Stone>) {
        if (0..self.w).contains(&x) && (0..self.h).contains(&y) {
            let i = self.index(x, y);
            self.stones[i] = color;
        }
    }

    /// A ko-threat shape in `corner`, along the horizontal edge or (`vertical`) the vertical one.
    fn ko(&mut self, corner: Corner, vertical: bool, p: Stone, q: Stone) {
        let (right, bottom) = match corner {
            Corner::TopLeft => (false, false),
            Corner::TopRight => (true, false),
            Corner::BottomLeft => (false, true),
            Corner::BottomRight => (true, true),
        };
        let edge = [None, Some(p), Some(q), None];
        let inner = [Some(p), Some(p), Some(p), None];
        for along in 0..4 {
            for (across, row) in [edge, inner].iter().enumerate() {
                let (a, c) = (along as i64, across as i64);
                let (dx, dy) = if vertical { (c, a) } else { (a, c) };
                let x = if right { self.w - 1 - dx } else { dx };
                let y = if bottom { self.h - 1 - dy } else { dy };
                self.put(x, y, row[along]);
            }
        }
    }

    fn cover_box(&self, gap: i64) -> Option<Bounds> {
        let (w, h) = (self.w, self.h);
        let (mut left, mut right, mut top, mut bottom) = (w - 1, 0, h - 1, 0);
        for x in 0..w {
            for y in 0..h {
                if self.stones[self.index(x, y)].is_some() {
                    left = left.min(x);
                    right = right.max(x);
                    top = top.min(y);
                    bottom = bottom.max(y);
                }
            }
        }
        let b = Bounds {
            left: if left - gap > 0 { left - gap } else { -1 },
            right: if right + gap < w - 1 { right + gap } else { w },
            top: if top - gap > 0 { top - gap } else { -1 },
            bottom: if bottom + gap < h - 1 { bottom + gap } else { h },
        };
        let open = b.right == w && b.left == -1 && b.top == -1 && b.bottom == h;
        (!open).then_some(b)
    }

    /// `buildCoverWall`: the wall, both territories and the ko threats; returns the komi and the
    /// colours of the "no room" messages.
    fn cover(&mut self, side: Stone, b: Bounds, ko_side: bool, ko_other: bool) -> (f64, Vec<Stone>) {
        let (w, h) = (self.w, self.h);
        let other = side.opposite();
        let Bounds {
            left,
            right,
            top,
            bottom,
        } = b;
        let even = |x: i64, y: i64| (x + y) % 2 == 0;

        for i in 0..w {
            for j in 0..h {
                let mut placed = false;
                if (i == left || i == right) && j >= top && j <= bottom {
                    self.add(i, j, side);
                    placed = true;
                }
                if !placed && (j == top || j == bottom) && i >= left && i <= right {
                    self.add(i, j, side);
                }
            }
        }

        let areas = [(right + 1) * h, (w - left) * h, (bottom + 1) * w, (h - top) * w];
        let mut min_area = 99999;
        let mut min_index = 0;
        for (k, area) in areas.into_iter().enumerate() {
            if area < min_area {
                min_area = area;
                min_index = k + 1;
            }
        }
        let half_area = (w * h) as f64 / 2.0;
        let half_with_komi = (half_area + 4.0).ceil() as i64;
        let komi_of = |area: i64| (2.0 * (area as f64 - half_area.ceil())).abs();
        let mut no_side = false;
        let mut no_other = false;
        let mut total = None;

        match min_index {
            1 if min_area <= half_with_komi => {
                let s = half_with_komi / h - 1;
                let remain = half_with_komi - (s + 1) * h;
                for x in 0..w {
                    for y in 0..h {
                        if x < s {
                            if x <= right && x >= left {
                                if (y < top || y > bottom) && even(x, y) {
                                    self.add(x, y, side);
                                }
                            } else if even(x, y) {
                                self.add(x, y, side);
                            }
                        }
                        if x == s {
                            self.add(x, y, side);
                        }
                        if remain > 0 {
                            if x == s + 1 {
                                self.add(x, y, if y < remain { side } else { other });
                            }
                            if x == s + 2 {
                                self.add(x, y, other);
                            }
                            if x > s + 2 && even(x, y) {
                                self.add(x, y, other);
                            }
                        } else {
                            if x == s + 1 {
                                self.add(x, y, other);
                            }
                            if x > s + 1 && even(x, y) {
                                self.add(x, y, other);
                            }
                        }
                    }
                }
                if ko_side {
                    if s < 4 {
                        no_side = true;
                    } else if top >= 2 {
                        self.ko(Corner::TopLeft, false, other, side);
                    } else if h - bottom > 2 {
                        self.ko(Corner::BottomLeft, false, other, side);
                    } else {
                        no_side = true;
                    }
                }
                if ko_other {
                    if w - s - 2 < 5 {
                        no_other = true;
                    } else {
                        self.ko(Corner::TopRight, false, side, other);
                    }
                }
            }
            1 => {
                if ko_side && right < 4 {
                    no_side = true;
                }
                let a1 = w.min(right + 1) * h.min(bottom + 1);
                let a2 = w.min(right + 1) * (h - top.max(0));
                if komi_of(a1) < komi_of(a2) {
                    total = Some(a1);
                    for x in 0..w {
                        for y in 0..h {
                            if x < right && (y < top || (x < left && y <= bottom)) && (y == bottom || even(x, y)) {
                                self.add(x, y, side);
                            }
                            if x == right && y < top {
                                self.add(x, y, side);
                            }
                            if x <= right {
                                if y == bottom + 1 {
                                    self.add(x, y, other);
                                }
                                if y > bottom + 1 && even(x, y) {
                                    self.add(x, y, other);
                                }
                            }
                            if x == right + 1 {
                                self.add(x, y, other);
                            }
                            if x > right + 1 && even(x, y) {
                                self.add(x, y, other);
                            }
                        }
                    }
                    if ko_side {
                        if right < 4 {
                            no_side = true;
                        } else if top >= 2 {
                            self.ko(Corner::TopLeft, false, other, side);
                        } else {
                            no_side = true;
                        }
                    }
                } else {
                    total = Some(a2);
                    for x in 0..w {
                        for y in 0..h {
                            if x < right && (y > bottom || (x < left && y >= top)) && (y == top || even(x, y)) {
                                self.add(x, y, side);
                            }
                            if x == right && y > bottom {
                                self.add(x, y, side);
                            }
                            if x <= right {
                                if y == top - 1 {
                                    self.add(x, y, other);
                                }
                                if y < top - 1 && even(x, y) {
                                    self.add(x, y, other);
                                }
                            }
                            if x == right + 1 {
                                self.add(x, y, other);
                            }
                            if x > right + 1 && even(x, y) {
                                self.add(x, y, other);
                            }
                        }
                    }
                    if ko_side {
                        if right < 4 {
                            no_side = true;
                        } else if h - bottom > 2 {
                            self.ko(Corner::BottomLeft, false, other, side);
                        } else {
                            no_side = true;
                        }
                    }
                }
                if ko_other {
                    if w - right - 1 < 5 {
                        no_other = true;
                    } else {
                        self.ko(Corner::TopRight, false, side, other);
                    }
                }
            }
            2 if min_area <= half_with_komi => {
                let s = half_with_komi / h + 1;
                let remain = half_with_komi - (w - s) * h;
                for x in 0..w {
                    for y in 0..h {
                        if x > s {
                            if x >= left && x <= right {
                                if (y < top || y > bottom) && even(x, y) {
                                    self.add(x, y, side);
                                }
                            } else if even(x, y) {
                                self.add(x, y, side);
                            }
                        }
                        if x == s {
                            self.add(x, y, side);
                        }
                        if remain > 0 {
                            if x == s - 1 {
                                self.add(x, y, if y < remain { side } else { other });
                            }
                            if x == s - 2 {
                                self.add(x, y, other);
                            }
                            if x < s && even(x, y) {
                                self.add(x, y, other);
                            }
                        } else {
                            if x == s - 1 {
                                self.add(x, y, other);
                            }
                            // Java: `x > rightSideIndex - 1`, the attacker's side of the board.
                            if x > s - 1 && even(x, y) {
                                self.add(x, y, other);
                            }
                        }
                    }
                }
                if ko_side {
                    if w - s - 1 < 4 {
                        no_side = true;
                    } else if top >= 2 {
                        self.ko(Corner::TopRight, false, other, side);
                    } else if h - bottom > 2 {
                        self.ko(Corner::BottomRight, false, other, side);
                    } else {
                        no_side = true;
                    }
                }
                if ko_other {
                    if s - 1 < 5 {
                        no_other = true;
                    } else {
                        self.ko(Corner::TopLeft, false, side, other);
                    }
                }
            }
            2 => {
                if ko_side && w - left - 1 < 4 {
                    no_side = true;
                }
                let a1 = (w - left.max(0)) * h.min(bottom + 1);
                let a2 = (w - left.max(0)) * (h - top.max(0));
                if komi_of(a1) < komi_of(a2) {
                    total = Some(a1);
                    for x in 0..w {
                        for y in 0..h {
                            if x > left && (y < top || (x > right && y <= bottom)) && (y == bottom || even(x, y)) {
                                self.add(x, y, side);
                            }
                            if x == left && y < top {
                                self.add(x, y, side);
                            }
                            if x >= left {
                                if y == bottom + 1 {
                                    self.add(x, y, other);
                                }
                                if y > bottom + 1 && even(x, y) {
                                    self.add(x, y, other);
                                }
                            }
                            if x == left - 1 {
                                self.add(x, y, other);
                            }
                            if x < left - 1 && even(x, y) {
                                self.add(x, y, other);
                            }
                        }
                    }
                    if ko_side {
                        if w - left - 1 < 4 {
                            no_side = true;
                        } else if top >= 2 {
                            self.ko(Corner::TopRight, false, other, side);
                        } else {
                            no_side = true;
                        }
                    }
                } else {
                    total = Some(a2);
                    for x in 0..w {
                        for y in 0..h {
                            if x > left && (y > bottom || (x > right && y >= top)) && (y == top || even(x, y)) {
                                self.add(x, y, side);
                            }
                            if x == left && y > bottom {
                                self.add(x, y, side);
                            }
                            if x >= left {
                                if y == top - 1 {
                                    self.add(x, y, other);
                                }
                                if y < top - 1 && even(x, y) {
                                    self.add(x, y, other);
                                }
                            }
                            if x == left - 1 {
                                self.add(x, y, other);
                            }
                            if x < left - 1 && even(x, y) {
                                self.add(x, y, other);
                            }
                        }
                    }
                    if ko_side {
                        if w - left - 1 < 4 {
                            no_side = true;
                        } else if h - bottom > 2 {
                            self.ko(Corner::BottomRight, false, other, side);
                        } else {
                            no_side = true;
                        }
                    }
                }
                if ko_other {
                    if left < 5 {
                        no_other = true;
                    } else {
                        self.ko(Corner::TopLeft, false, side, other);
                    }
                }
            }
            3 if min_area <= half_with_komi => {
                let s = half_with_komi / w - 1;
                // Java multiplies by the board height here (equal on square boards).
                let remain = half_with_komi - (s + 1) * h;
                for x in 0..w {
                    for y in 0..h {
                        if y < s {
                            if y <= bottom {
                                if (x < left || x > right || y < top) && even(x, y) {
                                    self.add(x, y, side);
                                }
                            } else if even(x, y) {
                                self.add(x, y, side);
                            }
                        }
                        if y == s {
                            self.add(x, y, side);
                        }
                        if remain > 0 {
                            if y == s + 1 {
                                self.add(x, y, if x < remain { side } else { other });
                            }
                            if y == s + 2 {
                                self.add(x, y, other);
                            }
                            if y > s + 2 && even(x, y) {
                                self.add(x, y, other);
                            }
                        } else {
                            if y == s + 1 {
                                self.add(x, y, other);
                            }
                            if y > s + 1 && even(x, y) {
                                self.add(x, y, other);
                            }
                        }
                    }
                }
                if ko_side {
                    if s < 4 {
                        no_side = true;
                    } else if left >= 2 {
                        self.ko(Corner::TopLeft, true, other, side);
                    } else if w - right > 2 {
                        self.ko(Corner::TopRight, true, other, side);
                    } else {
                        no_side = true;
                    }
                }
                if ko_other {
                    if h - s - 2 < 5 {
                        no_other = true;
                    } else {
                        self.ko(Corner::BottomLeft, true, side, other);
                    }
                }
            }
            3 => {
                if ko_side && bottom < 4 {
                    no_side = true;
                }
                let a1 = w.min(right + 1) * h.min(bottom + 1);
                let a2 = (w - left.max(0)) * h.min(bottom + 1);
                if komi_of(a1) < komi_of(a2) {
                    total = Some(a1);
                    for x in 0..w {
                        for y in 0..h {
                            if y < bottom && (x < left || (x <= right && y < top)) && (x == right || even(x, y)) {
                                self.add(x, y, side);
                            }
                            if y == bottom && x < left {
                                self.add(x, y, side);
                            }
                            if y <= bottom {
                                if x == right + 1 {
                                    self.add(x, y, other);
                                }
                                if x > right + 1 && even(x, y) {
                                    self.add(x, y, other);
                                }
                            }
                            if y == bottom + 1 {
                                self.add(x, y, other);
                            }
                            if y > bottom + 1 && even(x, y) {
                                self.add(x, y, other);
                            }
                        }
                    }
                    if ko_side {
                        if bottom < 4 {
                            no_side = true;
                        } else if left >= 2 {
                            self.ko(Corner::TopLeft, true, other, side);
                        } else {
                            no_side = true;
                        }
                    }
                } else {
                    total = Some(a2);
                    for x in 0..w {
                        for y in 0..h {
                            if ((x > right && y < bottom) || (x >= left && y < top)) && (x == left || even(x, y)) {
                                self.add(x, y, side);
                            }
                            if y == bottom && x > right {
                                self.add(x, y, side);
                            }
                            if y <= bottom {
                                if x == left - 1 {
                                    self.add(x, y, other);
                                }
                                if x < left - 1 && even(x, y) {
                                    self.add(x, y, other);
                                }
                            }
                            if y == bottom + 1 {
                                self.add(x, y, other);
                            }
                            if y > bottom + 1 && even(x, y) {
                                self.add(x, y, other);
                            }
                        }
                    }
                    if ko_side {
                        if bottom < 4 {
                            no_side = true;
                        } else if w - right > 2 {
                            self.ko(Corner::TopRight, true, other, side);
                        } else {
                            no_side = true;
                        }
                    }
                }
                if ko_other {
                    if h - bottom - 1 < 5 {
                        no_other = true;
                    } else {
                        self.ko(Corner::BottomLeft, true, side, other);
                    }
                }
            }
            4 if min_area <= half_with_komi => {
                let s = half_with_komi / w + 1;
                // Java multiplies by the board height here (equal on square boards).
                let remain = half_with_komi - (h - s) * h;
                for x in 0..w {
                    for y in 0..h {
                        if y > s {
                            if y >= top && y <= bottom {
                                if (x < left || x > right) && even(x, y) {
                                    self.add(x, y, side);
                                }
                            } else if even(x, y) {
                                self.add(x, y, side);
                            }
                        }
                        if y == s {
                            self.add(x, y, side);
                        }
                        if remain > 0 {
                            if y == s - 1 {
                                self.add(x, y, if x < remain { side } else { other });
                            }
                            if y == s - 2 {
                                self.add(x, y, other);
                            }
                            if y < s - 2 && even(x, y) {
                                self.add(x, y, other);
                            }
                        } else {
                            if y == s - 1 {
                                self.add(x, y, other);
                            }
                            if y < s - 1 && even(x, y) {
                                self.add(x, y, other);
                            }
                        }
                    }
                }
                if ko_side {
                    if h - s - 1 < 4 {
                        no_side = true;
                    } else if left >= 2 {
                        self.ko(Corner::BottomLeft, true, other, side);
                    } else if w - right > 2 {
                        self.ko(Corner::BottomRight, true, other, side);
                    } else {
                        no_side = true;
                    }
                }
                if ko_other {
                    if s - 1 < 5 {
                        no_other = true;
                    } else {
                        self.ko(Corner::TopLeft, true, side, other);
                    }
                }
            }
            4 => {
                // Java checks the bottom of the box here, as for a frame at the top.
                if ko_side && bottom < 4 {
                    no_side = true;
                }
                let a1 = w.min(right + 1) * (h - top.max(0));
                let a2 = (w - left.max(0)) * (h - top.max(0));
                if komi_of(a1) < komi_of(a2) {
                    total = Some(a1);
                    for x in 0..w {
                        for y in 0..h {
                            if y > top && (x < left || (y >= bottom && x <= right)) && (x == right || even(x, y)) {
                                self.add(x, y, side);
                            }
                            if y == top && x < left {
                                self.add(x, y, side);
                            }
                            if y >= top {
                                if x == right + 1 {
                                    self.add(x, y, other);
                                }
                                if x > right + 1 && even(x, y) {
                                    self.add(x, y, other);
                                }
                            }
                            if y == top - 1 {
                                self.add(x, y, other);
                            }
                            if y < top - 1 && even(x, y) {
                                self.add(x, y, other);
                            }
                        }
                    }
                    if ko_side {
                        if h - top - 1 < 4 {
                            no_side = true;
                        } else if left >= 2 {
                            self.ko(Corner::BottomLeft, true, other, side);
                        } else {
                            no_side = true;
                        }
                    }
                } else {
                    total = Some(a2);
                    for x in 0..w {
                        for y in 0..h {
                            if y > top && (x > right || (y >= bottom && x >= left)) && (x == left || even(x, y)) {
                                self.add(x, y, side);
                            }
                            if y == top && x > right {
                                self.add(x, y, side);
                            }
                            if y >= top {
                                if x == left - 1 {
                                    self.add(x, y, other);
                                }
                                if x < left - 1 && even(x, y) {
                                    self.add(x, y, other);
                                }
                            }
                            if y == top - 1 {
                                self.add(x, y, other);
                            }
                            if y < top - 1 && even(x, y) {
                                self.add(x, y, other);
                            }
                        }
                    }
                    if ko_side {
                        if h - top - 1 < 4 {
                            no_side = true;
                        } else if w - right > 2 {
                            self.ko(Corner::BottomRight, true, other, side);
                        } else {
                            no_side = true;
                        }
                    }
                }
                if ko_other {
                    if top < 5 {
                        no_other = true;
                    } else {
                        self.ko(Corner::TopLeft, true, side, other);
                    }
                }
            }
            _ => {}
        }

        let mut komi = match total {
            Some(area) => 2.0 * (area as f64 - half_area.ceil()),
            None => 7.5,
        };
        if side == Stone::White {
            komi = -komi;
        }
        let komi = komi.clamp(-150.0, 150.0);
        // The messages name `side` for the missing defender-side shape first, then the other.
        let mut no_room = Vec::new();
        if no_other {
            no_room.push(side);
        }
        if no_side {
            no_room.push(other);
        }
        (komi, no_room)
    }
}

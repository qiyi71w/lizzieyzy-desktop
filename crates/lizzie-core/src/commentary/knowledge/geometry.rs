use super::{data::Problem, features, KnowledgeQuery};
use std::collections::{HashMap, HashSet};

type Point = (i32, i32);
type Board = HashMap<Point, char>;
type Relative = (i32, i32, char);
const TRANSFORMS: [&str; 8] = [
    "identity",
    "rot90",
    "rot180",
    "rot270",
    "flipX",
    "flipY",
    "transpose",
    "anti-transpose",
];

pub(super) struct Geometry {
    pub score: i32,
    pub ratio: f64,
    pub matched: usize,
    pub expected: usize,
    pub liberty_score: i32,
    pub transform: &'static str,
    pub color_mode: &'static str,
}
struct Anchor {
    relative: HashSet<Relative>,
    profile: Profile,
}
pub(super) struct Context {
    size: u32,
    anchors: Vec<Anchor>,
}

fn point(gtp: &str, size: u32) -> Option<Point> {
    if gtp.trim_matches(|c: char| c <= '\u{0020}') != gtp {
        return None;
    }
    let (row, col) = features::point(gtp, size, false)?;
    if col >= size as i32 {
        return None;
    }
    Some((size as i32 - 1 - row, col))
}
fn neighbors((row, col): Point, size: u32) -> impl Iterator<Item = Point> {
    [(row - 1, col), (row + 1, col), (row, col - 1), (row, col + 1)]
        .into_iter()
        .filter(move |&(r, c)| r >= 0 && c >= 0 && r < size as i32 && c < size as i32)
}
fn relative(stones: &[(Point, char)], anchor: Point) -> HashSet<Relative> {
    stones
        .iter()
        .filter_map(|&((row, col), color)| {
            let (dx, dy) = (col - anchor.1, row - anchor.0);
            ((dx != 0 || dy != 0) && dx.abs().max(dy.abs()) <= 4).then_some((dx, dy, color))
        })
        .collect()
}
#[derive(Default)]
struct Profile {
    adjacent: [i32; 4],
    groups: [i32; 2],
    liberties: [Option<i32>; 2],
}
fn profile(board: &Board, anchor: Point, size: u32) -> Option<Profile> {
    if board.contains_key(&anchor) {
        return None;
    }
    let mut profile = Profile::default();
    let mut visited = HashSet::new();
    let mut count = 0;
    for n in neighbors(anchor, size) {
        count += 1;
        let Some(&color) = board.get(&n) else {
            profile.adjacent[2] += 1;
            continue;
        };
        let ci = if color == 'B' { 0 } else { 1 };
        profile.adjacent[ci] += 1;
        if visited.contains(&n) {
            continue;
        }
        let mut stack = vec![n];
        let mut liberties = HashSet::new();
        while let Some(p) = stack.pop() {
            if !visited.insert(p) {
                continue;
            }
            for next in neighbors(p, size) {
                match board.get(&next) {
                    None => {
                        liberties.insert(next);
                    }
                    Some(&other) if other == color && !visited.contains(&next) => stack.push(next),
                    _ => (),
                }
            }
        }
        profile.groups[ci] += 1;
        let count = liberties.len() as i32;
        profile.liberties[ci] = Some(profile.liberties[ci].map_or(count, |previous| previous.min(count)));
    }
    profile.adjacent[3] = 4 - count;
    Some(profile)
}
fn close(q: i32, p: i32, exact: i32, near: i32) -> i32 {
    if q == p {
        exact
    } else if (q - p).abs() <= 1 {
        near
    } else {
        0
    }
}
fn profile_score(query: &Profile, problem: &Profile, swap: bool) -> i32 {
    let mut score =
        close(query.adjacent[2], problem.adjacent[2], 2, 1) + close(query.adjacent[3], problem.adjacent[3], 2, 1);
    for color in 0..2 {
        let other = if swap { 1 - color } else { color };
        score += close(query.adjacent[color], problem.adjacent[other], 2, 1);
        score += close(query.groups[color], problem.groups[other], 1, 0);
        // Java profileValue2 boxes profileValue's default zero, including when
        // both minimum-liberty values are missing. Those two points are observable.
        score += close(
            query.liberties[color].unwrap_or(0),
            problem.liberties[other].unwrap_or(0),
            2,
            1,
        );
    }
    score
}
fn transform((dx, dy, color): Relative, t: usize, swap: bool) -> Relative {
    let (dx, dy) = match t {
        0 => (dx, dy),
        1 => (-dy, dx),
        2 => (-dx, -dy),
        3 => (dy, -dx),
        4 => (-dx, dy),
        5 => (dx, -dy),
        6 => (dy, dx),
        _ => (-dy, -dx),
    };
    (
        dx,
        dy,
        if swap {
            if color == 'B' {
                'W'
            } else {
                'B'
            }
        } else {
            color
        },
    )
}
impl Context {
    pub fn new(query: &KnowledgeQuery) -> Self {
        let size = query.board_size;
        let stones: Vec<_> = query
            .board_snapshot
            .iter()
            .filter_map(|stone| point(&stone.point, size).map(|p| (p, stone.color)))
            .collect();
        let board: Board = stones.iter().copied().collect();
        let mut seen = HashSet::new();
        let anchors = std::iter::once(&query.played_move)
            .chain(query.candidate_moves.iter().take(6))
            .chain(query.principal_variation.iter().take(4))
            .filter_map(|gtp| {
                let p = point(gtp, size)?;
                if !seen.insert(p) {
                    return None;
                }
                let profile = profile(&board, p, size)?;
                let relative = relative(&stones, p);
                (relative.len() >= 3).then_some(Anchor { profile, relative })
            })
            .collect();
        Self { size, anchors }
    }
    pub fn match_problem(&self, problem: &Problem) -> Option<Geometry> {
        if self.anchors.is_empty() {
            return None;
        }
        let stones: Vec<_> = problem
            .initial_stones
            .iter()
            .filter_map(|stone| {
                stone
                    .point()
                    .and_then(|p| point(p, self.size))
                    .map(|p| (p, stone.color.chars().next().unwrap_or('B')))
            })
            .collect();
        let board: Board = stones.iter().copied().collect();
        let mut seen = HashSet::new();
        let anchors: Vec<_> = problem
            .correct_moves
            .iter()
            .take(2)
            .filter_map(|mv| {
                let p = point(mv.point()?, self.size)?;
                if !seen.insert(p) {
                    return None;
                }
                let profile = profile(&board, p, self.size)?;
                let relative = relative(&stones, p);
                (relative.len() >= 3).then_some(Anchor { profile, relative })
            })
            .collect();
        let mut best: Option<Geometry> = None;
        for qa in &self.anchors {
            for pa in &anchors {
                for (t, &name) in TRANSFORMS.iter().enumerate() {
                    for swap in [false, true] {
                        let matched = pa
                            .relative
                            .iter()
                            .filter(|&&s| qa.relative.contains(&transform(s, t, swap)))
                            .count();
                        let expected = pa.relative.len();
                        let raw_ratio = matched as f64 / expected as f64;
                        let liberty_score = profile_score(&qa.profile, &pa.profile, swap);
                        let profile_ratio = (liberty_score as f64 / 12.0).min(1.0);
                        let ratio = (if swap { 0.92 } else { 1.0 }) * (raw_ratio * 0.76 + profile_ratio * 0.24);
                        if matched < 3 || ratio < 0.55 || liberty_score < 3 {
                            continue;
                        }
                        let score = (ratio * 24.0 + 0.5).floor() as i32 + matched.min(8) as i32 + liberty_score;
                        if best
                            .as_ref()
                            .is_none_or(|b| score > b.score || (score == b.score && ratio > b.ratio))
                        {
                            best = Some(Geometry {
                                score,
                                ratio,
                                matched,
                                expected,
                                liberty_score,
                                transform: name,
                                color_mode: if swap { "color-swapped" } else { "same-color" },
                            });
                        }
                    }
                }
            }
        }
        best
    }
}

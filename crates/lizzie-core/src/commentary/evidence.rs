//! Java TeacherEvidence / TeacherBoardContext. Candidate numbers are side-to-play, 0–100
//! winrate and points (not display-projected numbers); playouts is the stored analysis
//! `visits`, the counterpart of BoardData.getPlayouts() (an SGF header's rootVisits= when saved).
//! Selection/replay follows TeacherEvidence.java:68–225 and TeacherBoardContext.java:47–125.
use super::knowledge::{self, KnowledgeMove, KnowledgeQuery, SnapshotStone};
use crate::api::NodeAnalysisView;
use crate::record::{GameRecord, NodeId, NodeKind, PlayRules, Point, Position as BoardPosition, Stone, ROOT};
use std::collections::VecDeque;
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub rank: usize,
    pub coordinate: String,
    pub winrate: f64,
    pub score_lead: f64,
    pub visits: u64,
    pub variation: Vec<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Position {
    pub move_number: u32,
    pub to_play: String,
    pub playouts: u64,
    pub actual_move: String,
    pub actual_winrate_loss: Option<f64>,
    pub candidates: Vec<Candidate>,
    pub played_continuation: Vec<String>,
    pub board: Option<BoardContext>,
}
impl Position {
    pub fn actual_score_loss(&self) -> Option<f64> {
        let best = self.candidates.first()?.score_lead;
        let actual = self
            .candidates
            .iter()
            .find(|c| c.coordinate == self.actual_move)?
            .score_lead;
        (best.is_finite() && actual.is_finite()).then(|| (best - actual).max(0.0))
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Range {
    pub positions: Vec<Position>,
    pub analyzed_positions: usize,
    pub omitted_positions: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub color: String,
    pub stones: Vec<String>,
    pub liberties: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub stones: BoardPosition,
    pub r#move: String,
    pub color: String,
    pub captures: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub label: String,
    pub moves: Vec<String>,
    pub frames: Vec<Frame>,
    pub truncated: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardContext {
    pub width: u16,
    pub height: u16,
    pub stones: BoardPosition,
    pub groups: Vec<Group>,
    pub lines: Vec<Line>,
    pub knowledge: String,
}
pub fn coordinate(p: Point, height: u16) -> String {
    format!(
        "{}{}",
        char::from_u32('A' as u32 + u32::from(p.x) + u32::from(p.x >= 8)).unwrap_or('?'),
        height - p.y
    )
}
pub fn parse_coordinate(s: &str, size: crate::record::BoardSize) -> Option<Point> {
    let s = s.trim().to_ascii_uppercase();
    let b = *s.as_bytes().first()?;
    if !b.is_ascii_uppercase() || b == b'I' {
        return None;
    }
    let x = u16::from(b - b'A' - u8::from(b > b'I'));
    let row: u16 = s[1..].parse().ok()?;
    let p = Point {
        x,
        y: size.height.checked_sub(row)?,
    };
    (row > 0 && size.contains(p)).then_some(p)
}
fn actual(rec: &GameRecord, node: NodeId) -> String {
    match rec.node(node).kind {
        NodeKind::Move { point, .. } => coordinate(point, rec.size.height),
        NodeKind::Pass { .. } => "pass".into(),
        _ => String::new(),
    }
}
fn next(rec: &GameRecord, node: NodeId) -> Option<NodeId> {
    rec.node(node)
        .children
        .first()
        .copied()
        .filter(|n| rec.node(*n).kind != NodeKind::DummyPass)
}
fn raw(rec: &GameRecord, node: NodeId, analysis: &NodeAnalysisView) -> Option<Position> {
    let child = next(rec, node);
    let actual_move = child.map_or_else(String::new, |n| actual(rec, n));
    let convert = |(i, c): (usize, &crate::api::CandidateView)| Candidate {
        rank: i + 1,
        coordinate: c
            .point
            .map_or_else(|| "PASS".into(), |p| coordinate(p, rec.size.height)),
        winrate: if c.winrate.is_finite() { c.winrate } else { f64::NAN },
        score_lead: c.score_lead.filter(|v| v.is_finite()).unwrap_or(f64::NAN),
        visits: c.visits,
        variation: c
            .pv
            .iter()
            .take(12)
            .map(|p| p.map_or_else(|| "PASS".into(), |p| coordinate(p, rec.size.height)))
            .collect(),
    };
    let mut candidates: Vec<_> = analysis.candidates.iter().enumerate().take(3).map(convert).collect();
    if candidates.is_empty() {
        return None;
    }
    // Java compares the fourth candidate case-sensitively (actual pass stays lowercase).
    if !actual_move.is_empty() && !candidates.iter().any(|c| c.coordinate == actual_move) {
        if let Some(c) = analysis
            .candidates
            .iter()
            .enumerate()
            .map(convert)
            .find(|c| c.coordinate == actual_move)
        {
            candidates.push(c);
        }
    }
    let actual_winrate_loss = analysis
        .candidates
        .iter()
        .enumerate()
        .map(convert)
        .find(|c| c.coordinate.eq_ignore_ascii_case(&actual_move))
        .and_then(|c| {
            (c.winrate.is_finite() && candidates[0].winrate.is_finite())
                .then(|| (candidates[0].winrate - c.winrate).max(0.0))
        });
    let mut played_continuation = Vec::new();
    let mut walk = child;
    let mut color = rec.node(node).to_play;
    while let Some(n) = walk {
        let m = actual(rec, n);
        if m.is_empty() || rec.node(n).kind.action_color() != Some(color) || played_continuation.len() == 5 {
            break;
        }
        played_continuation.push(m);
        color = color.opposite();
        walk = next(rec, n);
    }
    Some(Position {
        move_number: rec.node(node).move_number,
        to_play: color_name(rec.node(node).to_play).into(),
        playouts: analysis.visits,
        actual_move,
        actual_winrate_loss,
        candidates,
        played_continuation,
        board: None,
    })
}
pub fn current(rec: &GameRecord, node: NodeId, analysis: Option<&NodeAnalysisView>) -> Option<Position> {
    let mut p = raw(rec, node, analysis?)?;
    p.board = Some(capture(rec, node, &p, analysis?));
    Some(p)
}
pub fn main_line<'a>(
    rec: &GameRecord,
    first: u32,
    last: u32,
    analysis: impl Fn(NodeId) -> Option<&'a NodeAnalysisView>,
) -> Range {
    let first = first.max(1);
    let last = last.max(first);
    let mut available = Vec::new();
    let mut parent = ROOT;
    while let Some(child) = next(rec, parent) {
        let n = rec.node(child).move_number;
        if n >= first && n <= last {
            if let Some(p) = analysis(parent).and_then(|a| raw(rec, parent, a)) {
                available.push((parent, p));
            }
        }
        if n > last {
            break;
        }
        parent = child;
    }
    let count = available.len();
    if count > 5 {
        available.sort_by(|a, b| {
            importance(&b.1)
                .total_cmp(&importance(&a.1))
                .then(a.1.move_number.cmp(&b.1.move_number))
        });
        let mut selected: Vec<(NodeId, Position)> = Vec::new();
        for p in available {
            if selected.iter().any(|s| s.1.move_number.abs_diff(p.1.move_number) < 3) {
                continue;
            }
            selected.push(p);
            if selected.len() == 5 {
                break;
            }
        }
        available = selected;
        available.sort_by_key(|p| p.1.move_number);
    }
    let positions = available
        .into_iter()
        .map(|(n, mut p)| {
            p.board = Some(capture(rec, n, &p, analysis(n).expect("selected analysis")));
            p
        })
        .collect::<Vec<_>>();
    Range {
        analyzed_positions: count,
        omitted_positions: count - positions.len(),
        positions,
    }
}
pub fn whole_game<'a>(rec: &GameRecord, analysis: impl Fn(NodeId) -> Option<&'a NodeAnalysisView>) -> Range {
    main_line(rec, 1, u32::MAX, analysis)
}
fn importance(p: &Position) -> f64 {
    p.actual_score_loss()
        .or(p.actual_winrate_loss.map(|v| v / 10.0))
        .unwrap_or_else(|| p.candidates.first().map_or(0.0, |c| c.visits as f64 / 1_000_000.0))
}
fn color_name(c: Stone) -> &'static str {
    if c == Stone::Black {
        "B"
    } else {
        "W"
    }
}
pub fn groups(board: &BoardPosition) -> Vec<Group> {
    let mut seen = vec![false; board.size.area()];
    let mut result = Vec::new();
    // Java's stone array and traversal are column-major; product storage is row-major.
    for x in 0..board.size.width {
        for y in 0..board.size.height {
            let origin = Point { x, y };
            let Some(color) = board.at(origin) else { continue };
            if seen[board.size.index(origin)] {
                continue;
            }
            let mut queue = VecDeque::from([origin]);
            seen[board.size.index(origin)] = true;
            let mut stones = Vec::new();
            let mut liberties = Vec::new();
            while let Some(p) = queue.pop_front() {
                stones.push(coordinate(p, board.size.height));
                for (x, y) in [
                    (i32::from(p.x) - 1, i32::from(p.y)),
                    (i32::from(p.x) + 1, i32::from(p.y)),
                    (i32::from(p.x), i32::from(p.y) - 1),
                    (i32::from(p.x), i32::from(p.y) + 1),
                ] {
                    if x < 0 || y < 0 {
                        continue;
                    }
                    let q = Point {
                        x: x as u16,
                        y: y as u16,
                    };
                    if !board.size.contains(q) {
                        continue;
                    }
                    match board.at(q) {
                        None => {
                            let s = coordinate(q, board.size.height);
                            if !liberties.contains(&s) {
                                liberties.push(s);
                            }
                        }
                        Some(c) if c == color && !seen[board.size.index(q)] => {
                            seen[board.size.index(q)] = true;
                            queue.push_back(q);
                        }
                        _ => {}
                    }
                }
            }
            result.push(Group {
                color: color_name(color).into(),
                stones,
                liberties,
            });
        }
    }
    result
}
fn replay(rec: &GameRecord, node: NodeId, label: String, moves: &[String]) -> Line {
    let mut board = rec.position(node);
    let mut previous = rec.node(node).parent.map(|n| rec.position(n));
    let mut color = rec.node(node).to_play;
    let mut frames = vec![Frame {
        stones: board.clone(),
        r#move: String::new(),
        color: String::new(),
        captures: 0,
    }];
    let mut accepted = Vec::new();
    for m in moves {
        let after = if m.eq_ignore_ascii_case("pass") {
            Some(board.clone())
        } else {
            parse_coordinate(m, rec.size)
                .and_then(|p| {
                    board.try_play(
                        color,
                        p,
                        PlayRules {
                            allow_suicide: false,
                            no_capture: false,
                        },
                    )
                })
                .filter(|b| previous.as_ref().is_none_or(|p| p.stones != b.stones))
        };
        let Some(after) = after else { break };
        let captures = if color == Stone::Black {
            after.black_captures - board.black_captures
        } else {
            after.white_captures - board.white_captures
        };
        previous = Some(board);
        board = after;
        accepted.push(m.clone());
        frames.push(Frame {
            stones: board.clone(),
            r#move: m.clone(),
            color: color_name(color).into(),
            captures,
        });
        color = color.opposite();
    }
    Line {
        label,
        truncated: accepted.len() != moves.len(),
        moves: accepted,
        frames,
    }
}
fn capture(rec: &GameRecord, node: NodeId, p: &Position, analysis: &NodeAnalysisView) -> BoardContext {
    let stones = rec.position(node);
    let mut lines = Vec::new();
    if !p.played_continuation.is_empty() {
        lines.push(replay(rec, node, "recorded".into(), &p.played_continuation));
    }
    for c in &p.candidates {
        let moves = if c
            .variation
            .first()
            .is_some_and(|m| m.eq_ignore_ascii_case(&c.coordinate))
        {
            c.variation.clone()
        } else {
            vec![c.coordinate.clone()]
        };
        lines.push(replay(rec, node, c.coordinate.clone(), &moves));
    }
    let path = rec.path_to(node);
    let recent_moves = path
        .iter()
        .skip(path.len().saturating_sub(40))
        .filter_map(|n| match rec.node(*n).kind {
            NodeKind::Move { point, .. } => Some(KnowledgeMove {
                col: i32::from(point.x),
                row: i32::from(point.y),
                gtp: coordinate(point, rec.size.height),
            }),
            _ => None,
        })
        .collect();
    let candidate_moves = analysis
        .candidates
        .iter()
        .take(5)
        .map(|c| {
            c.point
                .map_or_else(|| "PASS".into(), |p| coordinate(p, rec.size.height))
        })
        .collect();
    let principal_variation = analysis.candidates.first().map_or_else(Vec::new, |c| {
        c.pv.iter()
            .take(12)
            .map(|p| p.map_or_else(|| "PASS".into(), |p| coordinate(p, rec.size.height)))
            .collect()
    });
    let mut board_snapshot = Vec::new();
    for x in 0..rec.size.width {
        for y in 0..rec.size.height {
            let point = Point { x, y };
            if let Some(c) = stones.at(point) {
                board_snapshot.push(SnapshotStone {
                    point: coordinate(point, rec.size.height),
                    color: if c == Stone::Black { 'B' } else { 'W' },
                });
            }
        }
    }
    let knowledge = knowledge::match_text(&KnowledgeQuery {
        board_size: u32::from(rec.size.width),
        move_number: p.move_number,
        played_move: p.actual_move.clone(),
        text: String::new(),
        recent_moves,
        candidate_moves,
        principal_variation,
        board_snapshot,
        loss_score: p.actual_score_loss(),
    });
    BoardContext {
        width: rec.size.width,
        height: rec.size.height,
        groups: groups(&stones),
        stones,
        lines,
        knowledge,
    }
}
fn list(v: &[String]) -> String {
    format!("[{}]", v.join(", "))
}
impl BoardContext {
    pub fn facts(&self) -> String {
        let mut s = format!(
            "Board: {}x{}\nConnected groups (liberties are geometric facts, NOT proof of life/death or sente):\n",
            self.width, self.height
        );
        for g in &self.groups {
            s.push_str(&format!(
                "{} stones={} liberties={}\n",
                g.color,
                list(&g.stones),
                list(&g.liberties)
            ));
        }
        if self.groups.is_empty() {
            s.push_str("Empty board.\n");
        }
        s.push_str(
            "Locally replayed lines (simple ko and no suicide; NOT proof of best replies, superko or forced play):\n",
        );
        for line in &self.lines {
            s.push_str(&if line.label == "recorded" {
                "Recorded continuation: ".into()
            } else {
                format!("Candidate {}: ", line.label)
            });
            for (i, f) in line.frames.iter().enumerate().skip(1) {
                s.push_str(&format!("{i}.{} {}", f.color, f.r#move));
                if f.captures > 0 {
                    s.push_str(&format!(" captures={}", f.captures));
                }
                s.push_str("; ");
            }
            if line.truncated {
                s.push_str("STOPPED: remaining moves could not be replayed. Do not explain the omitted suffix.");
            }
            s.push('\n');
            if line.frames.len() > 1 {
                for g in groups(&line.frames.last().expect("initial frame").stones) {
                    if g.liberties.len() <= 2 {
                        s.push_str(&format!(
                            "End of line: {} {} has {} liberties {} (not a death verdict).\n",
                            g.color,
                            list(&g.stones),
                            g.liberties.len(),
                            list(&g.liberties)
                        ));
                    }
                }
            }
        }
        if !self.knowledge.trim().is_empty() {
            s.push_str("Position-specific knowledge:\n");
            s.push_str(&self.knowledge);
        }
        s
    }
}

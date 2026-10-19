//! Tygem GIB reading with the fixed Java semantics (`rules/GIBParser.java`).
//!
//! Kept from Java: players from `\[GAMEBLACKNAME=` / `\[GAMEWHITENAME=` (defaults "Player 2" /
//! "Player 1"), komi from `GONGJE:` in tenths (default 7.5), handicap from `INI`, the Tygem
//! handicap placement table, moves from `STO` (`fields[3] == "1"` is Black, column `fields[4]`,
//! row `fields[5]`), passes from `SKI`, and illegal moves skipped.
//!
//! Objective corrections against the fixed Java build, recorded in the ticket:
//! - GIB records are always 19x19; Java kept whatever size the previous record had.
//! - Handicap stones are root setup stones (`SNAPSHOT`), not numbered black moves, so the first
//!   recorded move is move 1 and White plays first after a completed handicap.
//! - Text without any GIB line is rejected instead of loading as an empty game.
//! - Name lines are accepted with LF as well as CRLF line ends.

use super::resolve_turns;
use crate::record::{BoardSize, GameRecord, NodeKind, Point, Position, Stone, ROOT};

/// `handicapPlacement` (`GIBParser.java:13-15`): column, row.
const HANDICAP: [(u16, u16); 9] = [
    (3, 15),
    (15, 3),
    (15, 15),
    (3, 3),
    (3, 9),
    (15, 9),
    (9, 3),
    (9, 15),
    (9, 9),
];

fn handicap_points(count: u32) -> Vec<Point> {
    let mut n = count.min(9) as usize;
    let mut points = Vec::new();
    if n == 5 || n == 7 {
        points.push(Point { x: 9, y: 9 });
        n -= 1;
    }
    points.extend(HANDICAP[..n].iter().map(|&(x, y)| Point { x, y }));
    points
}

fn field<'a>(fields: &[&'a str], i: usize, line: &str) -> Result<&'a str, String> {
    fields
        .get(i)
        .copied()
        .ok_or_else(|| format!("missing field {i} in `{line}`"))
}

fn number<T: std::str::FromStr>(s: &str, line: &str) -> Result<T, String> {
    s.trim().parse().map_err(|_| format!("bad number `{s}` in `{line}`"))
}

fn name(line: &str, prefix: &str) -> String {
    let rest = &line[prefix.len()..];
    rest.strip_suffix("\\]").unwrap_or(rest).to_string()
}

pub fn read(text: &str) -> Result<GameRecord, String> {
    let size = BoardSize::STANDARD;
    let mut rec = GameRecord::empty(size, 7.5);
    let mut black = "Player 2".to_string();
    let mut white = "Player 1".to_string();
    let mut komi = 7.5;
    let mut recognised = false;
    let mut current = ROOT;
    let mut pos = Position::new(size);
    let mut prev: Option<Position> = None;

    for raw in text.trim().split('\n') {
        let line = raw.trim_end_matches('\r');
        if line.starts_with("\\[GAMEINFOMAIN=") {
            recognised = true;
            if let Some(i) = line.find("GONGJE:") {
                let start = i + "GONGJE:".len();
                let end = line[start..]
                    .find(',')
                    .map(|e| start + e)
                    .ok_or_else(|| format!("unterminated komi in `{line}`"))?;
                komi = number::<i64>(&line[start..end], line)? as f64 / 10.0;
            }
        }
        if line.starts_with("\\[GAMEBLACKNAME=") {
            recognised = true;
            black = name(line, "\\[GAMEBLACKNAME=");
        }
        if line.starts_with("\\[GAMEWHITENAME=") {
            recognised = true;
            white = name(line, "\\[GAMEWHITENAME=");
        }
        if line.starts_with("INI") {
            recognised = true;
            let fields: Vec<&str> = line.split(' ').collect();
            let handicap: i64 = number(field(&fields, 3, line)?, line)?;
            if handicap >= 2 {
                let points = handicap_points(handicap as u32);
                rec.info.handicap = handicap.min(9) as u32;
                let root = rec.node_mut(ROOT);
                root.setup.add_black.extend(points.iter().copied());
                pos.apply_setup(&root.setup);
            }
        }
        if line.starts_with("STO") {
            recognised = true;
            let fields: Vec<&str> = line.split(' ').collect();
            let x: u16 = number(field(&fields, 4, line)?, line)?;
            let y: u16 = number(field(&fields, 5, line)?, line)?;
            let color = if field(&fields, 3, line)? == "1" {
                Stone::Black
            } else {
                Stone::White
            };
            let p = Point { x, y };
            if !size.contains(p) || pos.at(p).is_some() || pos.is_suicide(color, p) {
                continue;
            }
            let mut next = pos.clone();
            next.play(color, p);
            if prev.as_ref().is_some_and(|q| q.stones == next.stones) {
                continue;
            }
            prev = Some(std::mem::replace(&mut pos, next));
            current = rec.push_child(current, NodeKind::Move { color, point: p });
        }
        if line.starts_with("SKI") {
            recognised = true;
            // Java `Board.pass()`: the side to play passes; after a completed handicap that is White.
            let color = match rec.node(current).kind.action_color() {
                Some(c) => c.opposite(),
                None if rec.info.handicap >= 2 => Stone::White,
                None => Stone::Black,
            };
            prev = Some(pos.clone());
            current = rec.push_child(current, NodeKind::Pass { color });
        }
    }
    if !recognised {
        return Err("no GIB content".into());
    }
    rec.info.black = black;
    rec.info.white = white;
    rec.info.komi = komi;
    rec.komi_declared = true;
    // Java applies a GIB komi only when automatic komi loading is on.
    rec.has_setup_or_handicap = false;
    resolve_turns(&mut rec);
    Ok(rec)
}

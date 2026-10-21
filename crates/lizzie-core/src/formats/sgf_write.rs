//! SGF writing with the fixed Java save semantics (`SGFParser.saveToStream`, `generateNode`,
//! `materializedRootSnapshotProperties`, `materializedSetupProperties`; `SNAPSHOT_NODE_KIND.md`).
//!
//! The output is compared with Java by structure and meaning, not bytes:
//!
//! - The root carries `CA[UTF-8]` (files are always written as UTF-8), the game information
//!   (`HA` when non-zero, `KM`, players, ranks, result, date, rules), `SZ`, this product's `AP`
//!   and its setup materialised from the root position as `AB`/`AW`, followed by `PL`.
//! - Every snapshot writes the side to play it was given at import or by the user as an explicit
//!   `PL`; a later snapshot writes its setup as the change from its parent like Java
//!   `appendSetupStoneDelta`: `AE` for every changed point that held a stone, then `AB`/`AW` for
//!   points that now hold one (a replaced stone is `AE[p]` + `AW[p]`, which both readers apply in
//!   order).
//! - Moves are `B[xy]`/`W[xy]`, real passes `B[]`/`W[]`, placeholders `DD[true]`.
//! - Marks and comments come from the record; properties the record does not interpret (`MN`,
//!   analysis `LZ`/`LZ2`/`LZOP`/`LZOP2`, `GM`, `FF`, `GN`, ...) are written back as read.
//! - Variations follow the child order; parentheses only where a node has several children.
//!
//! Objective correction against the fixed Java build: every text value escapes `\` and `]`
//! (SGF FF[4] Text/SimpleText). Java escaped only comments, so a `]` in a player name, label or
//! other property ended the value early and corrupted the rest of the file.

use std::fmt::Write as _;

use crate::record::{BoardSize, GameRecord, MarkKind, NodeId, NodeKind, Point, Property, Stone, ROOT};

/// Properties the record holds in its own fields; they are written from those fields.
const MODELLED: &[&str] = &[
    "B", "W", "AB", "AW", "AE", "PL", "C", "DD", "LB", "TR", "SQ", "CR", "MA",
];
/// Root properties written from the game information or replaced on save (Java replaces `AP`
/// and `CA` with its own values).
const ROOT_MODELLED: &[&str] = &[
    "SZ", "KM", "KO", "HA", "PB", "PW", "BR", "WR", "RE", "DT", "RU", "CA", "AP",
];

/// Serialises a record. `application` is the `AP` value naming the writing program.
pub fn write(rec: &GameRecord, application: &str) -> String {
    let mut out = String::with_capacity(64 + rec.node_count() * 12);
    out.push_str("(;");
    write_root(&mut out, rec, application);
    write_children(&mut out, rec, ROOT);
    out.push(')');
    out
}

fn write_children(out: &mut String, rec: &GameRecord, mut id: NodeId) {
    // A single child continues the sequence; several open one parenthesised variation each.
    loop {
        match rec.node(id).children.as_slice() {
            [] => return,
            [only] => {
                id = *only;
                write_node(out, rec, id);
            }
            children => {
                for &child in children {
                    out.push('(');
                    write_node(out, rec, child);
                    write_children(out, rec, child);
                    out.push(')');
                }
                return;
            }
        }
    }
}

fn write_root(out: &mut String, rec: &GameRecord, application: &str) {
    let info = &rec.info;
    prop(out, "CA", "UTF-8");
    if info.handicap != 0 {
        prop(out, "HA", &info.handicap.to_string());
    }
    prop(out, "KM", &komi_text(info.komi));
    for (id, value) in [
        ("PW", &info.white),
        ("PB", &info.black),
        ("WR", &info.white_rank),
        ("BR", &info.black_rank),
        ("DT", &info.date),
        ("RE", &info.result),
    ] {
        if !value.is_empty() {
            prop(out, id, value);
        }
    }
    if let Some(rules) = info.rules.as_deref().filter(|r| !r.is_empty()) {
        prop(out, "RU", rules);
    }
    let size = rec.size;
    let sz = if size.width == size.height {
        size.width.to_string()
    } else {
        format!("{}:{}", size.width, size.height)
    };
    prop(out, "SZ", &sz);
    prop(out, "AP", application);
    let root = rec.node(ROOT);
    write_retained(out, &root.properties, ROOT_MODELLED);
    // Root setup materialised from the position (`collectCurrentRootSetup`).
    let pos = rec.position(ROOT);
    let (mut black, mut white) = (Vec::new(), Vec::new());
    for y in 0..size.height {
        for x in 0..size.width {
            match pos.at(Point { x, y }) {
                Some(Stone::Black) => black.push(Point { x, y }),
                Some(Stone::White) => white.push(Point { x, y }),
                None => {}
            }
        }
    }
    points(out, "AB", &black, size);
    points(out, "AW", &white, size);
    prop(out, "PL", side(root.to_play));
    write_marks_and_comment(out, rec, ROOT);
}

fn write_node(out: &mut String, rec: &GameRecord, id: NodeId) {
    let node = rec.node(id);
    let size = rec.size;
    out.push(';');
    match node.kind {
        NodeKind::Snapshot => {
            prop(out, "PL", side(node.to_play));
            let parent = node.parent.expect("only the root has no parent");
            let before = rec.position(parent);
            let mut after = before.clone();
            after.apply_setup(&node.setup);
            let (mut clear, mut black, mut white) = (Vec::new(), Vec::new(), Vec::new());
            for y in 0..size.height {
                for x in 0..size.width {
                    let p = Point { x, y };
                    let (was, now) = (before.at(p), after.at(p));
                    if was == now {
                        continue;
                    }
                    if was.is_some() {
                        clear.push(p);
                    }
                    match now {
                        None => {}
                        Some(Stone::Black) => black.push(p),
                        Some(Stone::White) => white.push(p),
                    }
                }
            }
            points(out, "AE", &clear, size);
            points(out, "AB", &black, size);
            points(out, "AW", &white, size);
        }
        NodeKind::Move { color, point } => prop(out, side(color), &coord(point, size)),
        NodeKind::Pass { color } => prop(out, side(color), ""),
        NodeKind::DummyPass => prop(out, "DD", "true"),
    }
    write_retained(out, &node.properties, &[]);
    write_marks_and_comment(out, rec, id);
}

fn write_retained(out: &mut String, properties: &[Property], also_modelled: &[&str]) {
    for p in properties {
        let id = p.id.as_str();
        if MODELLED.contains(&id) || also_modelled.contains(&id) || p.values.is_empty() {
            continue;
        }
        out.push_str(id);
        for v in &p.values {
            value(out, v);
        }
    }
}

fn write_marks_and_comment(out: &mut String, rec: &GameRecord, id: NodeId) {
    let node = rec.node(id);
    let size = rec.size;
    // One property per mark kind, kinds in order of first appearance.
    let mut kinds: Vec<&'static str> = Vec::new();
    for m in &node.marks {
        let k = mark_id(&m.mark);
        if !kinds.contains(&k) {
            kinds.push(k);
        }
    }
    for k in kinds {
        out.push_str(k);
        for m in node.marks.iter().filter(|m| mark_id(&m.mark) == k) {
            match &m.mark {
                MarkKind::Label(text) => value(out, &format!("{}:{}", coord(m.point, size), text)),
                _ => value(out, &coord(m.point, size)),
            }
        }
    }
    if !node.comment.is_empty() {
        prop(out, "C", &node.comment);
    }
}

fn mark_id(mark: &MarkKind) -> &'static str {
    match mark {
        MarkKind::Triangle => "TR",
        MarkKind::Square => "SQ",
        MarkKind::Circle => "CR",
        MarkKind::Cross => "MA",
        MarkKind::Label(_) => "LB",
    }
}

fn side(color: Stone) -> &'static str {
    match color {
        Stone::Black => "B",
        Stone::White => "W",
    }
}

/// Java writes the komi with `String.format("%s", Double)`: `7.5`, `6.0`.
fn komi_text(komi: f64) -> String {
    if komi.fract() == 0.0 && komi.abs() < 1e15 {
        format!("{komi:.1}")
    } else {
        komi.to_string()
    }
}

/// `asCoord`: letters `a`–`z`, `A`–`Z`; boards of 52 or more lines use `x_y`.
fn coord(p: Point, size: BoardSize) -> String {
    if size.width >= 52 || size.height >= 52 {
        return format!("{}_{}", p.x, p.y);
    }
    let letter = |n: u16| -> char {
        if n < 26 {
            (b'a' + n as u8) as char
        } else {
            (b'A' + (n - 26) as u8) as char
        }
    };
    [letter(p.x), letter(p.y)].into_iter().collect()
}

fn points(out: &mut String, id: &str, list: &[Point], size: BoardSize) {
    if list.is_empty() {
        return;
    }
    out.push_str(id);
    for &p in list {
        let _ = write!(out, "[{}]", coord(p, size));
    }
}

fn prop(out: &mut String, id: &str, text: &str) {
    out.push_str(id);
    value(out, text);
}

fn value(out: &mut String, text: &str) {
    out.push('[');
    for c in text.chars() {
        if c == '\\' || c == ']' {
            out.push('\\');
        }
        out.push(c);
    }
    out.push(']');
}

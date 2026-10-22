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
//! - Marks and comments come from the record; unknown properties (`MN`, `GM`, `FF`, `GN`, ...)
//!   are retained. Workbench saves supply `LZ`/`LZOP` from the per-node analysis store, replacing
//!   read-in primary tags; secondary-engine tags stay as read. Plain [`write`] retains all tags.
//! - Variations follow the child order; parentheses only where a node has several children.
//!
//! The 更多保存 exports ([`write_export`], `SGFParser.saveSnapshot(raw, comments, branch)`)
//! write the same tree without the analysis `LZ`/`LZ2` and without this product's `AP` (an `AP`
//! or `DZ` read with the record stays, as Java writes the root's properties as read). 纯净棋谱
//! drops move comments (the root comment and root `LZOP`/`LZOP2` stay, as
//! Java writes them from the root data); 带评论 keeps every comment and gives analysed nodes their
//! win-rate summary; 纯净分支 writes one line (root to the current node, then first children)
//! without any comment, root comment or root analysis.
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
/// The same for exports: they name no writing program, so an `AP` read with the record stays
/// (Java's raw header adds none, and the root's own properties are written as read).
const EXPORT_ROOT_MODELLED: &[&str] = &["SZ", "KM", "KO", "HA", "PB", "PW", "BR", "WR", "RE", "DT", "RU", "CA"];

/// What a record export leaves out.
pub enum Export<'a> {
    /// 保存纯净棋谱.
    Raw,
    /// 保存纯净棋谱(带评论): `comment` gives an analysed node's comment with its summary.
    RawWithComments {
        comment: &'a dyn Fn(NodeId) -> Option<String>,
    },
    /// 保存纯净分支 through `current`.
    Branch { current: NodeId },
}

enum Comments<'a> {
    All,
    RootOnly,
    Formatted(&'a dyn Fn(NodeId) -> Option<String>),
    None,
}

struct Opts<'a> {
    /// `AP`; `None` for exports.
    application: Option<&'a str>,
    comments: Comments<'a>,
    /// The single line of a branch export, root first.
    line: Option<Vec<NodeId>>,
    /// When supplied, primary analysis comes only from the per-node store.
    analysis: Option<&'a dyn Fn(NodeId) -> Option<String>>,
}

/// Serialises a record. `application` is the `AP` value naming the writing program.
pub fn write(rec: &GameRecord, application: &str) -> String {
    write_with(
        rec,
        &Opts {
            application: Some(application),
            comments: Comments::All,
            line: None,
            analysis: None,
        },
    )
}

pub(crate) fn write_analyzed(
    rec: &GameRecord,
    application: &str,
    analysis: &dyn Fn(NodeId) -> Option<String>,
    comment: &dyn Fn(NodeId) -> Option<String>,
) -> String {
    write_with(
        rec,
        &Opts {
            application: Some(application),
            comments: Comments::Formatted(comment),
            line: None,
            analysis: Some(analysis),
        },
    )
}

/// Serialises a 更多保存 export of the record (see the module notes).
pub fn write_export(rec: &GameRecord, export: &Export) -> String {
    write_export_analyzed(rec, export, None)
}

pub(crate) fn write_export_analyzed(
    rec: &GameRecord,
    export: &Export,
    analysis: Option<&dyn Fn(NodeId) -> Option<String>>,
) -> String {
    let opts = match export {
        Export::Raw => Opts {
            application: None,
            comments: Comments::RootOnly,
            line: None,
            analysis,
        },
        Export::RawWithComments { comment } => Opts {
            application: None,
            comments: Comments::Formatted(*comment),
            line: None,
            analysis,
        },
        Export::Branch { current } => {
            let mut line = rec.path_to(*current);
            let mut id = *current;
            while let Some(&first) = rec.node(id).children.first() {
                line.push(first);
                id = first;
            }
            Opts {
                application: None,
                comments: Comments::None,
                line: Some(line),
                analysis: None,
            }
        }
    };
    write_with(rec, &opts)
}

fn write_with(rec: &GameRecord, opts: &Opts) -> String {
    let mut out = String::with_capacity(64 + rec.node_count() * 12);
    out.push_str("(;");
    write_root(&mut out, rec, opts);
    match &opts.line {
        Some(line) => {
            for &id in line.iter().skip(1) {
                write_node(&mut out, rec, id, opts);
            }
        }
        None => write_children(&mut out, rec, ROOT, opts),
    }
    out.push(')');
    out
}

fn write_children(out: &mut String, rec: &GameRecord, mut id: NodeId, opts: &Opts) {
    // A single child continues the sequence; several open one parenthesised variation each.
    loop {
        match rec.node(id).children.as_slice() {
            [] => return,
            [only] => {
                id = *only;
                write_node(out, rec, id, opts);
            }
            children => {
                for &child in children {
                    out.push('(');
                    write_node(out, rec, child, opts);
                    write_children(out, rec, child, opts);
                    out.push(')');
                }
                return;
            }
        }
    }
}

fn write_root(out: &mut String, rec: &GameRecord, opts: &Opts) {
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
    let root = rec.node(ROOT);
    let root_analysis = opts.analysis.and_then(|a| a(ROOT));
    match opts.application {
        Some(application) => {
            prop(out, "AP", application);
            write_retained(
                out,
                root.properties
                    .iter()
                    .filter(|p| opts.analysis.is_none() || !matches!(p.id.as_str(), "LZ" | "LZOP")),
                ROOT_MODELLED,
            );
        }
        None => {
            // Exports: no analysis and no own application marker; what the record read (`AP`,
            // `DZ`) stays. A branch also drops the root's own analysis (`stripRootMetadata`).
            let branch = opts.line.is_some();
            write_retained(
                out,
                root.properties.iter().filter(|p| match p.id.as_str() {
                    "LZ" | "LZ2" => false,
                    // An uninterpretable root property is opaque metadata in Java's raw export.
                    "LZOP" => {
                        !branch
                            && (opts.analysis.is_none()
                                || (root_analysis.is_none()
                                    && crate::formats::saved_analysis::primary(&root.properties).is_none()))
                    }
                    "LZOP2" => !branch,
                    _ => true,
                }),
                EXPORT_ROOT_MODELLED,
            );
        }
    }
    if let Some(payload) = root_analysis {
        prop(out, "LZOP", &payload);
    }
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
    let comment = match opts.comments {
        Comments::None => None,
        _ => Some(root.comment.as_str()),
    };
    write_marks_and_comment(out, rec, ROOT, comment);
}

fn write_node(out: &mut String, rec: &GameRecord, id: NodeId, opts: &Opts) {
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
    if opts.application.is_some() {
        write_retained(
            out,
            &node.properties,
            if opts.analysis.is_some() { &["LZ", "LZOP"] } else { &[] },
        );
        if let Some(payload) = opts.analysis.and_then(|a| a(id)) {
            prop(
                out,
                if node.kind == NodeKind::Snapshot { "LZOP" } else { "LZ" },
                &payload,
            );
        }
    } else {
        write_retained(out, node.properties.iter().filter(|p| !p.id.starts_with("LZ")), &[]);
    }
    let formatted;
    let comment = match opts.comments {
        Comments::All => Some(node.comment.as_str()),
        Comments::RootOnly | Comments::None => None,
        Comments::Formatted(f) => {
            formatted = f(id);
            Some(formatted.as_deref().unwrap_or(&node.comment))
        }
    };
    write_marks_and_comment(out, rec, id, comment);
}

fn write_retained<'a>(out: &mut String, properties: impl IntoIterator<Item = &'a Property>, also_modelled: &[&str]) {
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

fn write_marks_and_comment(out: &mut String, rec: &GameRecord, id: NodeId, comment: Option<&str>) {
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
    if let Some(comment) = comment.filter(|c| !c.is_empty()) {
        prop(out, "C", comment);
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

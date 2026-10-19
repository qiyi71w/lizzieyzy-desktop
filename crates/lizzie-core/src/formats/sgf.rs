//! SGF reading with the fixed Java semantics (`rules/SGFParser.java`, `docs/SNAPSHOT_NODE_KIND.md`).
//!
//! Reading produces a candidate [`GameRecord`]; nothing is adopted here. Behaviour kept from Java:
//!
//! - Only `B`/`W` actions, setup (`AB`/`AW`/`AE`/`PL`) and `DD[true]` create history nodes.
//!   Comments, marks, `MN` and other properties in a node without such content attach to the
//!   previous history node. `MN` is retained as a display property, not a real move number.
//! - Root setup stays on the root snapshot; setup in later nodes creates a separate snapshot that
//!   does not consume a move number. Metadata stays on that snapshot even when the node has a move.
//! - Setup applies in property order; `AB`/`AW` on an occupied point are ignored and `AE` empties a
//!   point, so Java's own `AE[p]AW[p]` replacement reads as a white stone.
//! - Illegal moves (off board, occupied, immediate ko repetition, suicide) are skipped without
//!   failing the read.
//! - `KM` is normalised like Java (Fox scale, legacy x100 and quarter "zi" values).
//! - The initial side to play is resolved at import (explicit `PL`, first main-line action,
//!   completed handicap, otherwise Black).
//! - Digits continue a property identifier (`LZ2`, `LZOP2`: secondary-engine analysis).
//!
//! Objective corrections against the fixed Java build, recorded in the ticket:
//! - `B[tt]`/`W[tt]` is a pass on boards up to 19x19 (FF[4]); Java dropped the move.
//! - Compressed point lists (`AB[aa:cc]`) are expanded; Java kept only the first point.
//! - Escapes are removed from every text value, not only from `C`; label text is everything after
//!   the first colon.
//! - A comment from a comment-only SGF node is appended to an existing comment of the node it
//!   attaches to instead of replacing it.
//! - Trailing content after the first game tree is ignored instead of disabling variations.

use crate::record::{BoardSize, GameRecord, Mark, MarkKind, NodeId, NodeKind, Point, Position, Property, Stone, ROOT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SgfError(pub String);

impl std::fmt::Display for SgfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug)]
enum Event {
    Open,
    Close,
    Node,
    Prop(String, Vec<String>),
}

/// Java validates adjacent collection trees too, even though only the first tree is imported.
/// Without structural escapes, keep the reader's existing incomplete-tree tolerance.
fn validate_structural_newlines(chars: &[char], start: usize) -> Result<(), SgfError> {
    let mut i = start;
    let mut depth = 0i64;
    let mut in_value = false;
    let mut escaped = false;
    let mut normalized = false;
    let mut previous = '\0';
    while let Some(&c) = chars.get(i) {
        if depth == 0 && c != '(' && c != '\\' && !c.is_whitespace() {
            break;
        }
        if in_value {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == ']' {
                in_value = false;
                previous = c;
            }
            i += 1;
            continue;
        }
        if c == '\\' {
            let end = if chars[i..].starts_with(&['\\', 'r', '\\', 'n']) {
                i + 4
            } else if chars[i..].starts_with(&['\\', 'n']) {
                i + 2
            } else {
                return Err(SgfError(format!("unknown structural escape at {i}")));
            };
            let next = chars[end..].iter().find(|c| !c.is_whitespace());
            if previous.is_alphabetic() && next.is_some_and(|c| c.is_alphabetic()) {
                return Err(SgfError(format!("structural escape inside property name at {i}")));
            }
            normalized = true;
            i = end;
            continue;
        }
        match c {
            '[' => in_value = true,
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if !c.is_whitespace() {
            previous = c;
        }
        i += 1;
    }
    if normalized && (depth != 0 || in_value) {
        return Err(SgfError("unterminated tree with structural newline escapes".into()));
    }
    Ok(())
}

/// Splits SGF text into structural events. Literal `\n` / `\r\n` sequences in structural
/// whitespace are accepted like Java `normalizeStructuralNewlines`; other escapes outside property
/// values fail the read.
fn tokenize(text: &str) -> Result<Vec<Event>, SgfError> {
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    // Skip anything before the first game tree.
    while i < chars.len() && chars[i] != '(' {
        i += 1;
    }
    if i == chars.len() {
        return Err(SgfError("no game tree".into()));
    }
    validate_structural_newlines(&chars, i)?;
    let mut events = Vec::new();
    let mut depth = 0usize;
    let mut ident = String::new();
    let mut values: Vec<String> = Vec::new();
    let flush_prop = |ident: &mut String, values: &mut Vec<String>, events: &mut Vec<Event>| {
        if !ident.is_empty() {
            events.push(Event::Prop(std::mem::take(ident), std::mem::take(values)));
        }
    };
    while i < chars.len() {
        let c = chars[i];
        match c {
            '(' => {
                flush_prop(&mut ident, &mut values, &mut events);
                depth += 1;
                events.push(Event::Open);
                i += 1;
            }
            ')' => {
                flush_prop(&mut ident, &mut values, &mut events);
                if depth == 0 {
                    break;
                }
                depth -= 1;
                events.push(Event::Close);
                i += 1;
                if depth == 0 {
                    // Only the first game tree is read.
                    return Ok(events);
                }
            }
            ';' => {
                flush_prop(&mut ident, &mut values, &mut events);
                if depth == 0 {
                    return Err(SgfError("node outside a game tree".into()));
                }
                events.push(Event::Node);
                i += 1;
            }
            '[' => {
                if ident.is_empty() && values.is_empty() {
                    return Err(SgfError(format!("value without property at {i}")));
                }
                i += 1;
                let mut value = String::new();
                loop {
                    let Some(&v) = chars.get(i) else {
                        return Err(SgfError("unterminated property value".into()));
                    };
                    i += 1;
                    match v {
                        '\\' => {
                            let Some(&n) = chars.get(i) else {
                                return Err(SgfError("unterminated property value".into()));
                            };
                            value.push(n);
                            i += 1;
                        }
                        ']' => break,
                        _ => value.push(v),
                    }
                }
                values.push(value);
            }
            '\\' => {
                // Structural literal newline escapes.
                if chars.get(i + 1) == Some(&'n') {
                    i += 2;
                } else if chars.get(i + 1) == Some(&'r')
                    && chars.get(i + 2) == Some(&'\\')
                    && chars.get(i + 3) == Some(&'n')
                {
                    i += 4;
                } else {
                    return Err(SgfError(format!("unknown structural escape at {i}")));
                }
            }
            c if c.is_ascii_uppercase() => {
                if !values.is_empty() {
                    flush_prop(&mut ident, &mut values, &mut events);
                }
                ident.push(c);
                i += 1;
            }
            // Digits continue an identifier like Java's tag reader (`LZ2`, `LZOP2`, the
            // secondary-engine analysis). FF[3] long identifiers (`AddBlack`) keep only their
            // capitals; whitespace and other stray characters between properties are ignored.
            c if c.is_ascii_digit() && !ident.is_empty() && values.is_empty() => {
                ident.push(c);
                i += 1;
            }
            _ => i += 1,
        }
    }
    if depth > 0 && !ident.is_empty() && values.is_empty() {
        return Err(SgfError("property without value at end of input".into()));
    }
    flush_prop(&mut ident, &mut values, &mut events);
    // An unclosed final tree is accepted (Java reads up to the last bracket).
    for _ in 0..depth {
        events.push(Event::Close);
    }
    Ok(events)
}

/// `SZ[19]` or `SZ[w:h]`; missing, malformed or smaller than 2 falls back to 19
/// (`SGFParser.java:3867-3934`, `Board.reopen`).
fn board_size(events: &[Event]) -> BoardSize {
    let mut seen_node = false;
    for e in events {
        match e {
            Event::Node if seen_node => break,
            Event::Node => seen_node = true,
            Event::Prop(id, values) if id == "SZ" => {
                let v = values.first().map(|s| s.trim()).unwrap_or("");
                let parse = |s: &str| s.trim().parse::<u16>().ok().filter(|&n| n >= 2);
                let (w, h) = match v.split_once(':') {
                    Some((w, h)) => (parse(w), parse(h)),
                    None => (parse(v), parse(v)),
                };
                return BoardSize {
                    width: w.unwrap_or(19),
                    height: h.unwrap_or(19),
                };
            }
            Event::Open | Event::Close if seen_node => break,
            _ => {}
        }
    }
    BoardSize::STANDARD
}

fn coord_char(c: char) -> Option<u16> {
    match c {
        'a'..='z' => Some(c as u16 - 'a' as u16),
        'A'..='Z' => Some(c as u16 - 'A' as u16 + 26),
        _ => None,
    }
}

/// One SGF point; large boards use `x_y` (`SGFParser.java:405-432`).
fn parse_point(s: &str, size: BoardSize) -> Option<Point> {
    let s = s.trim();
    let p = if size.width >= 52 || size.height >= 52 {
        let (x, y) = s.split_once('_')?;
        Point {
            x: x.trim().parse().ok()?,
            y: y.trim().parse().ok()?,
        }
    } else {
        let mut it = s.chars();
        let (x, y) = (coord_char(it.next()?)?, coord_char(it.next()?)?);
        if it.next().is_some() {
            return None;
        }
        Point { x, y }
    };
    size.contains(p).then_some(p)
}

/// Point list value, expanding a compressed `aa:cc` rectangle.
fn parse_points(value: &str, size: BoardSize, out: &mut Vec<Point>) {
    match value.split_once(':') {
        Some((a, b)) => {
            if let (Some(a), Some(b)) = (parse_point(a, size), parse_point(b, size)) {
                for y in a.y.min(b.y)..=a.y.max(b.y) {
                    for x in a.x.min(b.x)..=a.x.max(b.x) {
                        out.push(Point { x, y });
                    }
                }
            }
        }
        None => {
            if let Some(p) = parse_point(value, size) {
                out.push(p);
            }
        }
    }
}

enum ActionValue {
    Pass,
    Point(Point),
    Invalid,
}

fn action_value(value: &str, size: BoardSize) -> ActionValue {
    let v = value.trim();
    if v.is_empty() || (v == "tt" && size.width <= 19 && size.height <= 19) {
        return ActionValue::Pass;
    }
    parse_point(v, size).map_or(ActionValue::Invalid, ActionValue::Point)
}

/// `normalizeSgfKomi` (`SGFParser.java:320-356`).
fn normalize_komi(raw: f64, fox: bool, chinese: bool, size: BoardSize) -> Option<f64> {
    let mut komi = raw;
    if fox {
        if komi.abs() >= 50.0 {
            komi /= 100.0;
            if chinese && (-4.0..=4.0).contains(&komi) {
                komi *= 2.0;
            }
        }
    } else {
        if komi >= 200.0 {
            komi /= 100.0;
            if (-4.0..=4.0).contains(&komi) {
                komi *= 2.0;
            }
        }
        let frac = (komi.fract().abs() * 100.0).round() as i64;
        if frac == 75 || frac == 25 {
            komi *= 2.0;
        }
    }
    (komi.abs() < size.area() as f64).then_some(komi)
}

#[derive(Default)]
struct Pending {
    comment: Option<String>,
    marks: Vec<Mark>,
    name: Option<String>,
    props: Vec<Property>,
}

struct Frame {
    current: NodeId,
    pos: Position,
    prev: Option<Position>,
}

struct Builder {
    rec: GameRecord,
    current: NodeId,
    pos: Position,
    /// Position before the current node's move, for the immediate ko check.
    prev: Option<Position>,
    stack: Vec<Frame>,
    /// History node created by the SGF node being read, which receives its metadata.
    target: Option<NodeId>,
    has_action: bool,
    snapshot: Option<NodeId>,
    in_root_node: bool,
    pending: Pending,
    root_props: Vec<Property>,
    komi_raw: Option<String>,
}

impl Builder {
    fn new(size: BoardSize) -> Builder {
        Builder {
            rec: GameRecord::empty(size, 0.0),
            current: ROOT,
            pos: Position::new(size),
            prev: None,
            stack: Vec::new(),
            target: None,
            has_action: false,
            snapshot: None,
            in_root_node: false,
            pending: Pending::default(),
            root_props: Vec::new(),
            komi_raw: None,
        }
    }

    /// Ends the SGF node being read: its metadata goes to the node it created, or else to the
    /// node that is current now.
    fn end_node(&mut self) {
        let target = self.snapshot.or(self.target).unwrap_or(self.current);
        let p = std::mem::take(&mut self.pending);
        let node = self.rec.node_mut(target);
        if let Some(c) = p.comment.filter(|c| !c.trim().is_empty()) {
            if node.comment.trim().is_empty() {
                node.comment = c;
            } else {
                node.comment.push('\n');
                node.comment.push_str(&c);
            }
        }
        if let Some(name) = p.name {
            if node.comment.is_empty() {
                node.comment = name;
            }
        }
        node.marks.extend(p.marks);
        node.properties.extend(p.props);
        self.target = None;
        self.has_action = false;
        self.snapshot = None;
        self.in_root_node = false;
    }

    /// Setup in a non-root SGF node creates one snapshot for that node; metadata collected in the
    /// node so far moves with it.
    fn snapshot_target(&mut self) -> NodeId {
        if self.in_root_node && !self.has_action {
            return ROOT;
        }
        if let Some(id) = self.snapshot {
            return id;
        }
        let id = self.rec.push_child(self.current, NodeKind::Snapshot);
        self.prev = Some(self.pos.clone());
        self.current = id;
        self.snapshot = Some(id);
        self.target = Some(id);
        id
    }

    /// Setup properties apply in file order like Java: `AB`/`AW` only place stones on empty points
    /// (`BoardHistoryList.addExtraStone`), `AE` empties a point. Java writes a replaced stone as
    /// `AE[p]AW[p]`. The node keeps the net change, each point in one list at most.
    fn setup(&mut self, id: String, values: Vec<String>) {
        let size = self.rec.size;
        let mut points = Vec::new();
        for v in &values {
            parse_points(v, size, &mut points);
        }
        let target = self.snapshot_target();
        for p in points {
            let now = match id.as_str() {
                "AB" | "AW" if self.pos.at(p).is_some() => continue,
                "AB" => Some(Stone::Black),
                "AW" => Some(Stone::White),
                _ => None,
            };
            let setup = &mut self.rec.node_mut(target).setup;
            setup.add_black.retain(|&q| q != p);
            setup.add_white.retain(|&q| q != p);
            setup.clear.retain(|&q| q != p);
            let mut one = crate::record::Setup::default();
            for s in [setup, &mut one] {
                match now {
                    Some(Stone::Black) => s.add_black.push(p),
                    Some(Stone::White) => s.add_white.push(p),
                    None => s.clear.push(p),
                }
            }
            self.pos.apply_setup(&one);
        }
        self.rec.node_mut(target).properties.push(Property { id, values });
        if target == ROOT {
            self.prev = None;
        }
    }

    fn action(&mut self, color: Stone, value: &str) {
        let kind = match action_value(value, self.rec.size) {
            ActionValue::Pass => NodeKind::Pass { color },
            ActionValue::Invalid => return,
            ActionValue::Point(p) => {
                if self.pos.at(p).is_some() || self.pos.is_suicide(color, p) {
                    return;
                }
                let mut next = self.pos.clone();
                next.play(color, p);
                if self.prev.as_ref().is_some_and(|prev| prev.stones == next.stones) {
                    return;
                }
                NodeKind::Move { color, point: p }
            }
        };
        let id = self.rec.push_child(self.current, kind);
        let before = self.pos.clone();
        if let NodeKind::Move { color, point } = kind {
            self.pos.play(color, point);
        }
        self.prev = Some(before);
        self.current = id;
        self.has_action = true;
        self.target = Some(id);
    }

    fn property(&mut self, id: String, values: Vec<String>) {
        let first = values.first().cloned().unwrap_or_default();
        match id.as_str() {
            "B" | "W" => {
                let color = if id == "B" { Stone::Black } else { Stone::White };
                self.action(color, &first);
            }
            "AB" | "AW" | "AE" => self.setup(id, values),
            "DD" if first.trim() == "true" => {
                let dummy = self.rec.push_child(self.current, NodeKind::DummyPass);
                if !self.has_action {
                    self.target = Some(dummy);
                }
            }
            "C" => {
                if !first.is_empty() {
                    match &mut self.pending.comment {
                        Some(c) => *c = first,
                        None => self.pending.comment = Some(first),
                    }
                }
                self.pending.props.push(Property { id, values });
            }
            "N" => {
                self.pending.name = Some(first.clone());
                self.pending.props.push(Property { id, values });
            }
            "LB" => {
                for v in &values {
                    if let Some((p, text)) = v.split_once(':') {
                        if let Some(point) = parse_point(p, self.rec.size) {
                            self.pending.marks.push(Mark {
                                point,
                                mark: MarkKind::Label(text.to_string()),
                            });
                        }
                    }
                }
                self.pending.props.push(Property { id, values });
            }
            "TR" | "SQ" | "CR" | "MA" => {
                let kind = match id.as_str() {
                    "TR" => MarkKind::Triangle,
                    "SQ" => MarkKind::Square,
                    "CR" => MarkKind::Circle,
                    _ => MarkKind::Cross,
                };
                let mut points = Vec::new();
                for v in &values {
                    parse_points(v, self.rec.size, &mut points);
                }
                self.pending.marks.extend(points.into_iter().map(|point| Mark {
                    point,
                    mark: kind.clone(),
                }));
                self.pending.props.push(Property { id, values });
            }
            "PL" => {
                let target = self.snapshot_target();
                let v = first.trim();
                if v.eq_ignore_ascii_case("W") {
                    self.rec.node_mut(target).explicit_to_play = Some(Stone::White);
                } else if v.eq_ignore_ascii_case("B") {
                    self.rec.node_mut(target).explicit_to_play = Some(Stone::Black);
                }
                self.rec.node_mut(target).properties.push(Property { id, values });
            }
            "PB" | "PW" | "BR" | "WR" | "RE" | "DT" | "RU" | "HA" | "KM" | "KO" | "SZ" if self.in_root_node => {
                if id == "KM" || (id == "KO" && self.komi_raw.is_none()) {
                    self.komi_raw = Some(first.clone());
                }
                self.root_props.push(Property { id, values });
            }
            _ => self.pending.props.push(Property { id, values }),
        }
    }

    fn run(mut self, events: Vec<Event>) -> Result<GameRecord, SgfError> {
        let mut started = false;
        for e in events {
            match e {
                Event::Open => {
                    if started {
                        self.end_node();
                        self.stack.push(Frame {
                            current: self.current,
                            pos: self.pos.clone(),
                            prev: self.prev.clone(),
                        });
                    }
                }
                Event::Close => {
                    self.end_node();
                    if let Some(f) = self.stack.pop() {
                        self.current = f.current;
                        self.pos = f.pos;
                        self.prev = f.prev;
                    }
                }
                Event::Node => {
                    if started {
                        self.end_node();
                    } else {
                        started = true;
                        self.in_root_node = true;
                    }
                    self.in_root_node = self.in_root_node && self.rec.node_count() == 1;
                }
                Event::Prop(id, values) => {
                    if !started {
                        return Err(SgfError("property before the first node".into()));
                    }
                    self.property(id, values);
                }
            }
        }
        if !started {
            return Err(SgfError("empty game tree".into()));
        }
        self.end_node();
        self.finish()
    }

    fn finish(mut self) -> Result<GameRecord, SgfError> {
        let get = |props: &[Property], id: &str| {
            props
                .iter()
                .find(|p| p.id == id)
                .and_then(|p| p.values.first())
                .cloned()
        };
        let root_props = std::mem::take(&mut self.root_props);
        let mut rec = self.rec;
        let all_root: Vec<Property> = root_props
            .iter()
            .chain(rec.node(ROOT).properties.iter())
            .cloned()
            .collect();
        let contains = |id: &str, needle: &str| get(&all_root, id).is_some_and(|v| v.to_lowercase().contains(needle));
        rec.info.black = get(&root_props, "PB").unwrap_or_default();
        rec.info.white = get(&root_props, "PW").unwrap_or_default();
        rec.info.black_rank = get(&root_props, "BR").unwrap_or_default();
        rec.info.white_rank = get(&root_props, "WR").unwrap_or_default();
        rec.info.result = get(&root_props, "RE").unwrap_or_default();
        rec.info.date = get(&root_props, "DT").unwrap_or_default();
        rec.info.rules = get(&root_props, "RU");
        rec.info.handicap = get(&root_props, "HA")
            .and_then(|v| v.trim().parse::<i64>().ok())
            .map_or(0, |h| h.max(0) as u32);
        if let Some(raw) = self.komi_raw.as_deref().and_then(|v| v.trim().parse::<f64>().ok()) {
            if let Some(k) = normalize_komi(raw, contains("AP", "foxwq"), contains("RU", "chinese"), rec.size) {
                rec.info.komi = k;
                rec.komi_declared = true;
            }
        }
        rec.has_setup_or_handicap = rec.info.handicap > 0 || !rec.node(ROOT).setup.is_empty();
        let mut root_all = root_props;
        root_all.append(&mut rec.node_mut(ROOT).properties);
        rec.node_mut(ROOT).properties = root_all;
        super::resolve_turns(&mut rec);
        Ok(rec)
    }
}

/// Reads one SGF game tree into a candidate record.
pub fn read(text: &str) -> Result<GameRecord, SgfError> {
    let text = text.trim_start_matches('\u{feff}');
    let events = tokenize(text)?;
    let size = board_size(&events);
    Builder::new(size).run(events)
}

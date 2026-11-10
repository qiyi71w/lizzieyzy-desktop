//! Main-line normalisation of a downloaded Fox record (`GetFoxRequest.normalizeFoxSgfPayload`).
//!
//! Fox replies carry their game as SGF text in which commentary variations, windowed fragments
//! and leading handicap runs obscure the played game. [`normalize`] rewrites that text into a
//! single main line with clean root properties before the ordinary SGF parser reads it.
//!
//! Java behaviour that is intentionally not reproduced:
//! - Java runs the merge recovery of windowed records only when the record's board size equals
//!   the board open in the window (its history helpers use the global board size). Here merge
//!   recovery always runs with the record's own `SZ`, so the result no longer depends on whatever
//!   board happens to be open.
//! - Java's merge recovery reads `canSuicidal` from the running engine; here suicide is always
//!   rejected, as Java does with no engine or an engine that forbids suicide.
//! - Variation nesting deep enough to overflow Java's recursive parser (a Java `StackOverflowError`
//!   that loses the reply) is normalised like any other record.
//! - Merge recovery is skipped, as after a Java runtime failure, for boards above 2^20 points
//!   (`MAX_MERGE_POINTS`), where Java's per-node arrays exhaust memory or overflow `int`.
//! - On boards of 52 lines or more, Java parses `x_y` coordinates with `Integer.parseInt`, which
//!   also accepts non-ASCII decimal digits; only ASCII digits are accepted here.

use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::HashMap;

/// `GetFoxRequest.sanitizeFoxSgf` then `retainMainLineOnly` (the `chess` text of a Fox record
/// reply). `no_capture` is the "no capture" setting that Java's merge recovery plays moves with.
pub fn normalize(sgf: &str, no_capture: bool) -> String {
    if java_trim(sgf).is_empty() {
        return sgf.to_owned();
    }
    let sanitized = sanitize(sgf);
    if java_trim(&sanitized).is_empty() {
        return sanitized;
    }
    retain_main_line(&sanitized, no_capture).unwrap_or(sanitized)
}

/// Strips U+FEFF, trims, and drops backslashes outside property values; escapes inside values
/// are kept verbatim.
fn sanitize(sgf: &str) -> String {
    let without_bom: Cow<'_, str> = if sgf.contains('\u{FEFF}') {
        Cow::Owned(sgf.replace('\u{FEFF}', ""))
    } else {
        Cow::Borrowed(sgf)
    };
    let text = java_trim(&without_bom);
    let mut out = String::with_capacity(text.len());
    let mut inside_value = false;
    let mut chars = text.chars();
    while let Some(current) = chars.next() {
        if inside_value {
            out.push(current);
            if current == '\\' {
                if let Some(escaped) = chars.next() {
                    out.push(escaped);
                }
            } else if current == ']' {
                inside_value = false;
            }
            continue;
        }
        if current == '\\' {
            continue;
        }
        out.push(current);
        if current == '[' {
            inside_value = true;
        }
    }
    out
}

/// `None` means "keep the sanitized text": malformed SGF, trailing text, or nothing to rebuild.
fn retain_main_line(input: &str, no_capture: bool) -> Option<String> {
    let mut parser = Parser::new(input);
    parser.skip_whitespace();
    if parser.peek() != Some(b'(') {
        return None;
    }
    let tree = parser.parse_tree()?;
    parser.skip_whitespace();
    if parser.pos < input.len() {
        return None;
    }
    rebuild(&tree, input.len(), no_capture)
}

// ---------------------------------------------------------------------------------------------
// SGF text model and parser (`SgfMainLineNormalizer.parseTree`)

struct Property<'a> {
    name: &'a str,
    /// Raw value text between the brackets, escapes included.
    values: Vec<&'a str>,
}

struct Node<'a> {
    properties: Vec<Property<'a>>,
}

struct Tree<'a> {
    nodes: Vec<Node<'a>>,
    children: Vec<Tree<'a>>,
}

impl Drop for Tree<'_> {
    /// Unnests children first so dropping a deeply nested record cannot overflow the stack.
    fn drop(&mut self) {
        let mut pending = std::mem::take(&mut self.children);
        while let Some(mut tree) = pending.pop() {
            pending.append(&mut tree.children);
        }
    }
}

struct Parser<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Self { text, pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.pos).copied()
    }

    fn expect(&mut self, expected: u8) -> Option<()> {
        (self.peek() == Some(expected)).then(|| self.pos += 1)
    }

    fn skip_whitespace(&mut self) {
        while let Some(current) = self.text[self.pos..].chars().next() {
            if !is_java_whitespace(current) {
                break;
            }
            self.pos += current.len_utf8();
        }
    }

    /// Iterative form of the recursive Java `parseTree`.
    fn parse_tree(&mut self) -> Option<Tree<'a>> {
        let mut open: Vec<Tree<'a>> = Vec::new();
        self.open_tree(&mut open)?;
        loop {
            if self.peek() == Some(b'(') {
                self.open_tree(&mut open)?;
                continue;
            }
            self.expect(b')')?;
            let closed = open.pop()?;
            self.skip_whitespace();
            match open.last_mut() {
                Some(parent) => parent.children.push(closed),
                None => return Some(closed),
            }
        }
    }

    fn open_tree(&mut self, open: &mut Vec<Tree<'a>>) -> Option<()> {
        self.expect(b'(')?;
        self.skip_whitespace();
        let mut nodes = Vec::new();
        while self.peek() == Some(b';') {
            nodes.push(self.parse_node()?);
            self.skip_whitespace();
        }
        open.push(Tree {
            nodes,
            children: Vec::new(),
        });
        Some(())
    }

    fn parse_node(&mut self) -> Option<Node<'a>> {
        self.expect(b';')?;
        let mut properties = Vec::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                None | Some(b';' | b'(' | b')') => break,
                Some(_) => properties.push(self.parse_property()?),
            }
        }
        Some(Node { properties })
    }

    fn parse_property(&mut self) -> Option<Property<'a>> {
        let start = self.pos;
        while self.peek().is_some_and(|byte| byte.is_ascii_alphabetic()) {
            self.pos += 1;
        }
        if start == self.pos {
            return None;
        }
        let name = &self.text[start..self.pos];
        self.skip_whitespace();
        let mut values = Vec::new();
        while self.peek() == Some(b'[') {
            values.push(self.parse_value()?);
            self.skip_whitespace();
        }
        (!values.is_empty()).then_some(Property { name, values })
    }

    fn parse_value(&mut self) -> Option<&'a str> {
        self.pos += 1; // '['
        let start = self.pos;
        let bytes = self.text.as_bytes();
        while let Some(&byte) = bytes.get(self.pos) {
            self.pos += 1;
            match byte {
                // The escaped character never ends the value; a multi-byte one continues with
                // bytes that cannot be `]` or `\`.
                b'\\' if self.pos < bytes.len() => self.pos += 1,
                b']' => return Some(&self.text[start..self.pos - 1]),
                _ => {}
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------------------------
// Rebuilding the main line (`SgfMainLineNormalizer.rebuild`)

#[derive(Clone, Copy, PartialEq, Eq)]
enum Color {
    Black,
    White,
}

impl Color {
    fn from_property(name: &str) -> Option<Self> {
        match name {
            "B" => Some(Self::Black),
            "W" => Some(Self::White),
            _ => None,
        }
    }

    fn letter(self) -> &'static str {
        match self {
            Self::Black => "B",
            Self::White => "W",
        }
    }
}

/// A move as Java's `SgfMove`: colour plus the raw coordinate text.
#[derive(Clone, PartialEq, Eq)]
struct Move<'a> {
    color: Color,
    coordinate: Cow<'a, str>,
}

struct RootProperty<'a> {
    name: &'a str,
    values: Vec<Cow<'a, str>>,
}

const ROOT_PROPERTY_ORDER: [&str; 37] = [
    "GM", "FF", "CA", "AP", "ST", "RU", "SZ", "KM", "HA", "TM", "TC", "TT", "OT", "EV", "RO", "PC", "DT", "GN", "GC",
    "PB", "BR", "PW", "WR", "RE", "US", "SO", "CP", "AN", "ON", "BT", "WT", "PL", "AB", "AW", "AE", "RN", "RL",
];

fn rebuild(tree: &Tree<'_>, input_len: usize, no_capture: bool) -> Option<String> {
    let root = tree.nodes.first()?;
    let mut root_properties = build_root_properties(&tree.nodes);
    let root_moves = extract_moves(&root.properties);
    let main_line = recover_preferred_moves(tree, &root_moves, no_capture);
    let next_color = main_line.first().map(|m| m.color);
    let mut ordered = choose_compatible_root_prefix(&root_moves, next_color);
    ordered.extend(main_line);

    let handicap = leading_handicap_count(&ordered);
    if handicap >= 2 {
        let stones: Vec<Cow<'_, str>> = ordered.drain(..handicap).map(|m| m.coordinate).collect();
        apply_handicap_stones(&mut root_properties, stones);
    }
    let moves = alternating(ordered);
    if root_properties.is_empty() && moves.is_empty() {
        return None;
    }
    Some(build_sgf(&root_properties, &moves, input_len))
}

/// Fox writes handicap stones as a leading run of `B` moves; two or more placed stones form one.
fn leading_handicap_count(moves: &[Move<'_>]) -> usize {
    moves
        .iter()
        .take_while(|m| m.color == Color::Black && is_placed_stone(&m.coordinate))
        .count()
}

fn is_placed_stone(coordinate: &str) -> bool {
    utf16_len_is(coordinate, 2) && coordinate != "tt"
}

fn apply_handicap_stones<'a>(properties: &mut Vec<RootProperty<'a>>, stones: Vec<Cow<'a, str>>) {
    properties.retain(|p| p.name != "HA" && p.name != "AB");
    properties.push(RootProperty {
        name: "HA",
        values: vec![Cow::Owned(stones.len().to_string())],
    });
    properties.push(RootProperty {
        name: "AB",
        values: stones,
    });
    let komi = vec![Cow::Borrowed("0")];
    match properties.iter_mut().find(|p| p.name == "KM") {
        Some(property) => property.values = komi,
        None => properties.push(RootProperty {
            name: "KM",
            values: komi,
        }),
    }
}

/// Keeps each move whose colour differs from the previously kept one.
fn alternating(moves: Vec<Move<'_>>) -> Vec<Move<'_>> {
    let mut last: Option<Color> = None;
    let mut kept = Vec::with_capacity(moves.len());
    for m in moves {
        if last != Some(m.color) {
            last = Some(m.color);
            kept.push(m);
        }
    }
    kept
}

fn build_sgf(properties: &[RootProperty<'_>], moves: &[Move<'_>], input_len: usize) -> String {
    let mut out = String::with_capacity((input_len / 8).max(128));
    out.push_str("(;");
    for property in properties {
        out.push_str(property.name);
        for value in &property.values {
            out.push('[');
            out.push_str(value);
            out.push(']');
        }
    }
    for m in moves {
        out.push(';');
        out.push_str(m.color.letter());
        out.push('[');
        out.push_str(&m.coordinate);
        out.push(']');
    }
    out.push(')');
    out
}

/// Root properties come from the root and the following move-less nodes, in a fixed order:
/// AB/AW/AE values are merged, other properties keep their last non-blank occurrence (the first
/// one when all are blank), and GM/FF/CA/SZ get defaults.
fn build_root_properties<'a>(nodes: &[Node<'a>]) -> Vec<RootProperty<'a>> {
    let collected: Vec<&Property<'a>> = nodes
        .iter()
        .enumerate()
        .take_while(|(i, node)| *i == 0 || !node_has_moves(node))
        .flat_map(|(_, node)| node.properties.iter())
        .collect();
    let mut ordered = Vec::new();
    for name in ROOT_PROPERTY_ORDER {
        let mut matching = collected.iter().filter(|p| p.name == name);
        if matches!(name, "AB" | "AW" | "AE") {
            let values: Vec<Cow<'a, str>> = matching
                .flat_map(|p| p.values.iter().map(|v| Cow::Borrowed(*v)))
                .collect();
            if !values.is_empty() {
                ordered.push(RootProperty { name, values });
            }
            continue;
        }
        let mut chosen = matching.next();
        for later in matching {
            if later.values.iter().any(|v| !java_trim(v).is_empty()) {
                chosen = Some(later);
            }
        }
        let values = match (chosen, default_root_value(name)) {
            (Some(property), _) => property.values.iter().map(|v| Cow::Borrowed(*v)).collect(),
            (None, Some(default)) => vec![Cow::Borrowed(default)],
            (None, None) => continue,
        };
        ordered.push(RootProperty { name, values });
    }
    ordered
}

fn default_root_value(name: &str) -> Option<&'static str> {
    match name {
        "GM" => Some("1"),
        "FF" => Some("4"),
        "CA" => Some("UTF-8"),
        "SZ" => Some("19"),
        _ => None,
    }
}

fn node_has_moves(node: &Node<'_>) -> bool {
    node.properties.iter().any(|p| Color::from_property(p.name).is_some())
}

fn extract_moves<'a>(properties: &[Property<'a>]) -> Vec<Move<'a>> {
    let mut moves = Vec::new();
    for property in properties {
        if let Some(color) = Color::from_property(property.name) {
            moves.extend(property.values.iter().map(|v| Move {
                color,
                coordinate: Cow::Borrowed(*v),
            }));
        }
    }
    moves
}

fn is_alternating(moves: &[Move<'_>]) -> bool {
    moves.windows(2).all(|pair| pair[0].color != pair[1].color)
}

/// The longest alternating prefix of the root's own moves whose last colour differs from the
/// main line's first move; failing that, the longest alternating prefix.
fn choose_compatible_root_prefix<'a>(root_moves: &[Move<'a>], next_color: Option<Color>) -> Vec<Move<'a>> {
    let alternating_len = (1..=root_moves.len())
        .take_while(|&len| is_alternating(&root_moves[..len]))
        .last()
        .unwrap_or(0);
    let compatible = (1..=alternating_len)
        .rev()
        .find(|&len| next_color != Some(root_moves[len - 1].color))
        .unwrap_or(alternating_len);
    root_moves[..compatible].to_vec()
}

// ---------------------------------------------------------------------------------------------
// Choosing the main line (`recoverPreferredMoves`)

fn recover_preferred_moves<'a>(tree: &Tree<'a>, root_moves: &[Move<'a>], no_capture: bool) -> Vec<Move<'a>> {
    let default_moves = extract_main_line_moves(tree);
    if !looks_like_windowed_fox(tree) {
        return default_moves;
    }
    let merged = recover_merged_fox_branch(tree, no_capture);
    if merged.len() >= default_moves.len().max(12) {
        return merged;
    }
    let windows = extract_fox_windows(&tree.children);
    if windows.len() < 5 {
        return default_moves;
    }
    let recovered = recover_windowed_moves(&windows, root_moves);
    if recovered.len() >= (default_moves.len() + 4).max(15) {
        return recovered;
    }
    default_moves
}

/// Moves after the root, following at each fork the first child that leads to any move.
fn extract_main_line_moves<'a>(tree: &Tree<'a>) -> Vec<Move<'a>> {
    let mut moves = Vec::new();
    let mut current = tree;
    let mut skip = 1;
    loop {
        for node in current.nodes.iter().skip(skip) {
            moves.extend(extract_moves(&node.properties));
        }
        skip = 0;
        match current.children.iter().find(|child| tree_has_moves(child)) {
            Some(child) => current = child,
            None => return moves,
        }
    }
}

fn tree_has_moves(tree: &Tree<'_>) -> bool {
    let mut pending = vec![tree];
    while let Some(current) = pending.pop() {
        if current.nodes.iter().any(node_has_moves) {
            return true;
        }
        pending.extend(current.children.iter());
    }
    false
}

/// Fox "windowed" records: many leaf variations of 7..10 move nodes under the main sequence.
fn looks_like_windowed_fox(tree: &Tree<'_>) -> bool {
    let children = tree.children.len();
    if children < 20 {
        return false;
    }
    let candidates = tree
        .children
        .iter()
        .filter(|child| {
            let move_nodes = child.nodes.iter().filter(|node| node_has_moves(node)).count();
            child.children.is_empty() && (7..=10).contains(&move_nodes)
        })
        .count();
    candidates >= (children / 3).max(10)
}

// ---------------------------------------------------------------------------------------------
// Merge recovery (`recoverMergedFoxBranch`): replays every variation into one game history the
// way Java's `BoardHistoryList` does, then reads that history's first-child line.

/// Largest board (in points) merge recovery replays; see the module documentation.
const MAX_MERGE_POINTS: i64 = 1 << 20;

fn recover_merged_fox_branch<'a>(tree: &Tree<'_>, no_capture: bool) -> Vec<Move<'a>> {
    let (width, height) = detect_board_size(tree.nodes.first().map(|n| &n.properties[..]));
    let Some(mut history) = MergeHistory::new(width, height, no_capture) else {
        return Vec::new();
    };
    // Pre-order walk of Java's recursive `mergeTreeIntoHistory`: each variation starts at the
    // history node where its parent sequence ended.
    let mut pending = vec![(tree, MergeHistory::ROOT)];
    while let Some((current, start)) = pending.pop() {
        history.head = start;
        for node in &current.nodes {
            history.apply_node(node);
        }
        let next_start = history.head;
        pending.extend(current.children.iter().rev().map(|child| (child, next_start)));
    }
    history.main_branch()
}

/// `SZ` of the root node (the last non-blank one wins), `W:H` for rectangles, at least 2.
fn detect_board_size(properties: Option<&[Property<'_>]>) -> (i32, i32) {
    let (mut width, mut height) = (19, 19);
    for property in properties.unwrap_or_default().iter().filter(|p| p.name == "SZ") {
        let Some(text) = property.values.last() else { continue };
        let text = java_trim(text);
        if text.is_empty() {
            continue;
        }
        if let Some((w, h)) = text.split_once(':') {
            width = parse_java_int(java_trim(w)).unwrap_or(19);
            height = parse_java_int(java_trim(h)).unwrap_or(width);
        } else {
            width = parse_java_int(text).unwrap_or(19);
            height = width;
        }
    }
    (width.max(2), height.max(2))
}

const EMPTY: u8 = 0;
const BLACK: u8 = 1;
const WHITE: u8 = 2;
/// Marks stones already reached by the liberty search.
const VISITED: u8 = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum NodeKind {
    Snapshot,
    Move,
    Pass,
}

/// The parts of Java's `BoardData` that merge recovery reads.
struct HistoryNode {
    kind: NodeKind,
    stones: Box<[u8]>,
    /// Java's Zobrist hash as the set of toggled (point, colour) entries it XORs together.
    zobrist: Box<[u64]>,
    last_move: Option<(i32, i32)>,
    last_color: u8,
    black_to_play: bool,
    move_number: i32,
    previous: Option<usize>,
    children: Vec<usize>,
}

impl HistoryNode {
    /// `BoardHistoryNode.matchesChildState`; comments are always empty here.
    fn same_child_state(&self, other: &Self) -> bool {
        self.stones == other.stones
            && self.last_move == other.last_move
            && self.kind == other.kind
            && self.move_number == other.move_number
            && self.black_to_play == other.black_to_play
            && self.last_color == other.last_color
    }
}

#[derive(Clone, Copy)]
struct Geometry {
    width: i32,
    height: i32,
}

impl Geometry {
    fn index(self, x: i32, y: i32) -> Option<usize> {
        let valid = (0..self.width).contains(&x) && (0..self.height).contains(&y);
        valid.then(|| (x * self.height + y) as usize)
    }

    /// Java's `x_y` coordinates and plain numbers start at this size.
    fn uses_numeric_coordinates(self) -> bool {
        self.width >= 52 || self.height >= 52
    }
}

struct MergeHistory {
    geometry: Geometry,
    no_capture: bool,
    nodes: Vec<HistoryNode>,
    head: usize,
    flood: Vec<usize>,
}

impl MergeHistory {
    const ROOT: usize = 0;

    fn new(width: i32, height: i32, no_capture: bool) -> Option<Self> {
        let points = i64::from(width) * i64::from(height);
        if points > MAX_MERGE_POINTS {
            return None;
        }
        let points = points as usize;
        let root = HistoryNode {
            kind: NodeKind::Snapshot,
            stones: vec![EMPTY; points].into_boxed_slice(),
            zobrist: vec![0; (points * 2).div_ceil(64)].into_boxed_slice(),
            last_move: None,
            last_color: EMPTY,
            black_to_play: true,
            move_number: 0,
            previous: None,
            children: Vec::new(),
        };
        Some(Self {
            geometry: Geometry { width, height },
            no_capture,
            nodes: vec![root],
            head: Self::ROOT,
            flood: Vec::new(),
        })
    }

    /// `applyNodeToHistory`: AB/AW/AE edit the current node, B/W play or pass.
    fn apply_node(&mut self, node: &Node<'_>) {
        for property in &node.properties {
            let setup = match property.name {
                "AB" => Some(BLACK),
                "AW" => Some(WHITE),
                "AE" => Some(EMPTY),
                _ => None,
            };
            if let Some(stone) = setup {
                for value in &property.values {
                    if let Some((x, y)) = self.sgf_point(value) {
                        self.set_stone(x, y, stone);
                    }
                }
                continue;
            }
            let Some(color) = Color::from_property(property.name) else {
                continue;
            };
            let color = if color == Color::Black { BLACK } else { WHITE };
            for value in &property.values {
                match self.sgf_point(value) {
                    Some((x, y)) => self.place(x, y, color),
                    None => self.pass(color),
                }
            }
        }
    }

    /// `SGFParser.convertSgfPosToCoord`: `None` is a pass. Letters outside the alphabet give -1,
    /// which (like `tt` on boards up to 19) is an off-board point that is then ignored.
    fn sgf_point(&self, value: &str) -> Option<(i32, i32)> {
        value.encode_utf16().nth(1)?;
        if self.geometry.uses_numeric_coordinates() {
            let trimmed = java_trim(value);
            let mut parts: Vec<&str> = trimmed.split('_').collect();
            if parts.len() > 1 {
                while parts.last() == Some(&"") {
                    parts.pop();
                }
            }
            return match parts[..] {
                [x, y] => Some((parse_java_int(x)?, parse_java_int(y)?)),
                _ => None,
            };
        }
        let mut units = value.encode_utf16();
        Some((sgf_letter_index(units.next()), sgf_letter_index(units.next())))
    }

    /// `BoardHistoryList.setStone`: edits the current node in place.
    fn set_stone(&mut self, x: i32, y: i32, stone: u8) {
        let Some(index) = self.geometry.index(x, y) else { return };
        let head = &mut self.nodes[self.head];
        head.stones[index] = stone;
        toggle(&mut head.zobrist, index, stone);
    }

    /// `BoardHistoryList.place(x, y, color, false)`.
    fn place(&mut self, x: i32, y: i32, color: u8) {
        let Some(index) = self.geometry.index(x, y) else { return };
        let current = &self.nodes[self.head];
        if current.stones[index] != EMPTY {
            return;
        }
        // Replaying the next move of the current line only advances.
        if let Some(&next) = current.children.first() {
            if self.nodes[next].last_move == Some((x, y)) {
                self.head = next;
                return;
            }
        }
        let mut stones = current.stones.clone();
        let mut zobrist = current.zobrist.clone();
        let move_number = current.move_number + 1;
        let previous = current.previous;
        stones[index] = color;
        toggle(&mut zobrist, index, color);
        let mut suicide = 0;
        if !self.no_capture {
            let opponent = BLACK + WHITE - color;
            for (nx, ny) in [(x + 1, y), (x, y + 1), (x - 1, y), (x, y - 1)] {
                self.remove_dead_chain(nx, ny, opponent, &mut stones, &mut zobrist);
            }
            suicide = self.remove_dead_chain(x, y, color, &mut stones, &mut zobrist);
        }
        // Simple ko: the position may not repeat the one before the current move.
        if previous.is_some_and(|p| self.nodes[p].zobrist == zobrist) || suicide > 0 {
            return;
        }
        self.add_or_goto(HistoryNode {
            kind: NodeKind::Move,
            stones,
            zobrist,
            last_move: Some((x, y)),
            last_color: color,
            black_to_play: color == WHITE,
            move_number,
            previous: None,
            children: Vec::new(),
        });
    }

    /// `BoardHistoryList.pass(color, false)`.
    fn pass(&mut self, color: u8) {
        let current = &self.nodes[self.head];
        if let Some(&next) = current.children.first() {
            if self.nodes[next].kind == NodeKind::Pass {
                self.head = next;
                return;
            }
        }
        let node = HistoryNode {
            kind: NodeKind::Pass,
            stones: current.stones.clone(),
            zobrist: current.zobrist.clone(),
            last_move: None,
            last_color: color,
            black_to_play: color == WHITE,
            move_number: current.move_number + 1,
            previous: None,
            children: Vec::new(),
        };
        self.add_or_goto(node);
    }

    /// `BoardHistoryNode.addOrGoto`: go to an equal child, else append a new one.
    fn add_or_goto(&mut self, mut node: HistoryNode) {
        let head = self.head;
        let existing = self.nodes[head]
            .children
            .iter()
            .copied()
            .find(|&child| self.nodes[child].same_child_state(&node));
        if let Some(child) = existing {
            self.head = child;
            return;
        }
        let id = self.nodes.len();
        let parent = &mut self.nodes[head];
        // `alignParentTurnStateWithFirstActionChild`: a move node's turn follows its first child.
        if parent.children.is_empty() && parent.kind != NodeKind::Snapshot {
            parent.black_to_play = node.last_color == BLACK;
        }
        parent.children.push(id);
        node.previous = Some(head);
        self.nodes.push(node);
        self.head = id;
    }

    /// `Board.removeDeadChain`: removes the chain at (x, y) when it has no liberty and returns the
    /// number of removed stones.
    fn remove_dead_chain(&mut self, x: i32, y: i32, color: u8, stones: &mut [u8], zobrist: &mut [u64]) -> usize {
        let geometry = self.geometry;
        let Some(start) = geometry.index(x, y) else { return 0 };
        if stones[start] != color {
            return 0;
        }
        let chain = &mut self.flood;
        chain.clear();
        chain.push(start);
        stones[start] = color | VISITED;
        let mut has_liberty = false;
        let mut next = 0;
        while next < chain.len() {
            let point = chain[next];
            next += 1;
            let (px, py) = ((point as i32) / geometry.height, (point as i32) % geometry.height);
            for (nx, ny) in [(px + 1, py), (px, py + 1), (px - 1, py), (px, py - 1)] {
                let Some(neighbor) = geometry.index(nx, ny) else {
                    continue;
                };
                if stones[neighbor] == EMPTY {
                    has_liberty = true;
                } else if stones[neighbor] == color {
                    stones[neighbor] = color | VISITED;
                    chain.push(neighbor);
                }
            }
        }
        if has_liberty {
            for &point in chain.iter() {
                stones[point] = color;
            }
            return 0;
        }
        for &point in chain.iter() {
            stones[point] = EMPTY;
            toggle(zobrist, point, color);
        }
        chain.len()
    }

    /// `extractMainBranch`: the first-child line from the root, passes as empty coordinates.
    fn main_branch<'a>(&self) -> Vec<Move<'a>> {
        let mut moves = Vec::new();
        let mut node = &self.nodes[Self::ROOT];
        while let Some(&next) = node.children.first() {
            node = &self.nodes[next];
            let color = match node.last_color {
                BLACK => Color::Black,
                WHITE => Color::White,
                _ => continue,
            };
            let coordinate = match (node.kind, node.last_move) {
                (NodeKind::Pass, _) => String::new(),
                (NodeKind::Move, Some((x, y))) => self.coordinate_text(x, y),
                _ => continue,
            };
            moves.push(Move {
                color,
                coordinate: Cow::Owned(coordinate),
            });
        }
        moves
    }

    /// `SGFParser.asCoord(...).toLowerCase()`.
    fn coordinate_text(&self, x: i32, y: i32) -> String {
        if self.geometry.uses_numeric_coordinates() {
            return format!("{x}_{y}");
        }
        let letter = |i: i32| char::from(SGF_ALPHABET[i as usize].to_ascii_lowercase());
        [letter(x), letter(y)].into_iter().collect()
    }
}

/// `SGFParser.alphabet`.
const SGF_ALPHABET: &[u8; 52] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

fn sgf_letter_index(unit: Option<u16>) -> i32 {
    unit.and_then(|u| SGF_ALPHABET.iter().position(|&letter| u16::from(letter) == u))
        .map_or(-1, |i| i as i32)
}

fn toggle(zobrist: &mut [u64], point: usize, stone: u8) {
    let entry = match stone {
        BLACK => point * 2,
        WHITE => point * 2 + 1,
        _ => return,
    };
    zobrist[entry / 64] ^= 1u64 << (entry % 64);
}

// ---------------------------------------------------------------------------------------------
// Windowed recovery (`recoverWindowedMoves`): chains overlapping move windows into one game.

struct FoxWindow<'a> {
    index: usize,
    /// Second move of a window's first node, which Fox uses to repeat the preceding move.
    context: Vec<Move<'a>>,
    sequence: Vec<Move<'a>>,
    /// `context` followed by `sequence`.
    augmented: Vec<Move<'a>>,
}

impl<'a> FoxWindow<'a> {
    fn skip_sequence(&self) -> &[Move<'a>] {
        if self.sequence.len() > 1 {
            &self.sequence[1..]
        } else {
            &[]
        }
    }
}

struct Candidate<'w, 'a> {
    index: usize,
    target: &'w [Move<'a>],
    overlap: usize,
    uses_context: bool,
    append_size: usize,
    variant_rank: u8,
}

fn extract_fox_windows<'a>(children: &[Tree<'a>]) -> Vec<FoxWindow<'a>> {
    let mut windows = Vec::new();
    for (index, child) in children.iter().enumerate() {
        let mut context = Vec::new();
        let mut sequence = Vec::new();
        for node in &child.nodes {
            let mut moves = extract_moves(&node.properties).into_iter();
            let Some(first) = moves.next() else { continue };
            if sequence.is_empty() {
                context.extend(moves.next());
            }
            sequence.push(first);
        }
        if !sequence.is_empty() {
            let augmented = context.iter().chain(&sequence).cloned().collect();
            windows.push(FoxWindow {
                index,
                context,
                sequence,
                augmented,
            });
        }
    }
    windows
}

fn recover_windowed_moves<'a>(windows: &[FoxWindow<'a>], root_moves: &[Move<'a>]) -> Vec<Move<'a>> {
    let seeds = pick_seed_windows(windows, root_moves);
    let mut best: Vec<Move<'a>> = Vec::new();
    let mut best_score = i64::MIN;
    let mut best_priority = i32::MIN;
    for root_prefix in root_prefix_candidates(root_moves) {
        for seed in &seeds {
            let priority = seed_priority(seed, root_moves);
            let attempt = build_windowed_sequence(seed, windows, &root_prefix);
            if attempt.is_empty() {
                continue;
            }
            let score = score_recovered_moves(&attempt, &root_prefix, root_moves);
            let better = score > best_score
                || (score == best_score && priority > best_priority)
                || (score == best_score && priority == best_priority && attempt.len() > best.len());
            if better {
                best = attempt;
                best_score = score;
                best_priority = priority;
            }
        }
    }
    best
}

fn root_prefix_candidates<'a>(root_moves: &[Move<'a>]) -> Vec<Vec<Move<'a>>> {
    let mut candidates: Vec<Vec<Move<'a>>> = Vec::new();
    let mut add = |prefix: Vec<Move<'a>>| {
        if (prefix.is_empty() || is_alternating(&prefix)) && !candidates.contains(&prefix) {
            candidates.push(prefix);
        }
    };
    add(root_moves.to_vec());
    if let Some(last) = root_moves.last() {
        add(vec![last.clone()]);
    }
    if root_moves.len() > 1 {
        add(root_moves.iter().rev().cloned().collect());
    }
    add(Vec::new());
    candidates
}

/// Up to 16 windows, highest seed priority first, then in record order.
fn pick_seed_windows<'w, 'a>(windows: &'w [FoxWindow<'a>], root_moves: &[Move<'a>]) -> Vec<&'w FoxWindow<'a>> {
    let mut ordered: Vec<(i32, &FoxWindow<'a>)> = windows.iter().map(|w| (seed_priority(w, root_moves), w)).collect();
    ordered.sort_by(|(lp, l), (rp, r)| rp.cmp(lp).then(l.index.cmp(&r.index)));
    ordered.into_iter().take(16).map(|(_, w)| w).collect()
}

fn seed_priority<'a>(window: &FoxWindow<'a>, root_moves: &[Move<'a>]) -> i32 {
    let last_root = root_moves.last();
    if last_root.is_some() && window.context.first() == last_root {
        return 4;
    }
    let differs_from_root = matches!(
        (last_root, window.sequence.first()),
        (Some(root), Some(first)) if root.color != first.color
    );
    if window.context.is_empty() && differs_from_root {
        return 3;
    }
    if differs_from_root {
        return 2;
    }
    if window.context.is_empty() {
        return 1;
    }
    0
}

fn build_windowed_sequence<'a>(
    seed: &FoxWindow<'a>,
    windows: &[FoxWindow<'a>],
    root_prefix: &[Move<'a>],
) -> Vec<Move<'a>> {
    let mut current = root_prefix.to_vec();
    let seed_overlap = if root_prefix.is_empty() {
        0
    } else {
        let overlap = compute_overlap(root_prefix, &seed.sequence);
        if !can_append(root_prefix, &seed.sequence, overlap) {
            return Vec::new();
        }
        overlap
    };
    current.extend_from_slice(&seed.sequence[seed_overlap..]);
    if !is_alternating(&current) {
        return Vec::new();
    }
    let mut used = vec![seed.index];
    while current.len() < 600 {
        let mut best: Option<Candidate<'_, 'a>> = None;
        for window in windows.iter().filter(|w| !used.contains(&w.index)) {
            let index = window.index;
            best = better_candidate(best, build_candidate(&current, &window.sequence, index, false, 3));
            let skip = window.skip_sequence();
            if !skip.is_empty() {
                best = better_candidate(best, build_candidate(&current, skip, index, false, 2));
            }
            if !window.context.is_empty() {
                best = better_candidate(best, build_candidate(&current, &window.augmented, index, true, 1));
            }
        }
        let Some(chosen) = best.filter(|c| c.overlap >= 2) else {
            break;
        };
        current.extend_from_slice(&chosen.target[chosen.overlap..]);
        used.push(chosen.index);
    }
    if current.len() <= root_prefix.len() {
        return Vec::new();
    }
    current.split_off(root_prefix.len())
}

fn build_candidate<'w, 'a>(
    current: &[Move<'a>],
    target: &'w [Move<'a>],
    index: usize,
    uses_context: bool,
    variant_rank: u8,
) -> Option<Candidate<'w, 'a>> {
    if target.is_empty() {
        return None;
    }
    let overlap = compute_overlap(current, target);
    if overlap == 0 || !can_append(current, target, overlap) {
        return None;
    }
    Some(Candidate {
        index,
        target,
        overlap,
        uses_context,
        append_size: target.len() - overlap,
        variant_rank,
    })
}

/// Prefers longer overlap, then more appended moves, then the plain/skip/context variant order,
/// then no context, then the earlier window.
fn better_candidate<'w, 'a>(
    current: Option<Candidate<'w, 'a>>,
    next: Option<Candidate<'w, 'a>>,
) -> Option<Candidate<'w, 'a>> {
    let (current, next) = match (current, next) {
        (current, None) => return current,
        (None, next) => return next,
        (Some(current), Some(next)) => (current, next),
    };
    let next_wins = if next.overlap != current.overlap {
        next.overlap > current.overlap
    } else if next.append_size != current.append_size {
        next.append_size > current.append_size
    } else if next.variant_rank != current.variant_rank {
        next.variant_rank > current.variant_rank
    } else if next.uses_context != current.uses_context {
        !next.uses_context
    } else {
        next.index < current.index
    };
    Some(if next_wins { next } else { current })
}

/// Longest suffix of `current` that is also a prefix of `target`.
fn compute_overlap<'a>(current: &[Move<'a>], target: &[Move<'a>]) -> usize {
    let max = current.len().min(target.len());
    (1..=max)
        .rev()
        .find(|&size| current[current.len() - size..] == target[..size])
        .unwrap_or(0)
}

fn can_append(current: &[Move<'_>], target: &[Move<'_>], overlap: usize) -> bool {
    let Some(appended) = target.get(overlap..).filter(|rest| !rest.is_empty()) else {
        return false;
    };
    if current.last().is_some_and(|last| last.color == appended[0].color) {
        return false;
    }
    is_alternating(appended)
}

fn score_recovered_moves<'a>(recovered: &[Move<'a>], root_prefix: &[Move<'a>], root_moves: &[Move<'a>]) -> i64 {
    let mut score = recovered.len() as i64 * 100 + root_prefix.len() as i64 * 8;
    if let Some(last_prefix) = root_prefix.last() {
        if recovered.len() > root_prefix.len() && recovered[0].color != last_prefix.color {
            score += 24;
        }
    }
    score -= repeat_penalty(recovered);
    if !root_moves.is_empty() && recovered.starts_with(root_moves) {
        score += 20;
    }
    score
}

fn repeat_penalty(moves: &[Move<'_>]) -> i64 {
    let mut penalty = 0;
    for i in 0..moves.len() {
        for j in i.saturating_sub(12)..i {
            if moves[i] == moves[j] {
                penalty += (120 - (i - j) as i64 * 6).max(12);
            }
        }
    }
    (2..=4).map(|block| repeated_block_penalty(moves, block)).sum::<i64>() + penalty
}

/// Blocks are compared through Java's `color:coordinate|` key text, so coordinates containing
/// those separators compare exactly as they do in Java.
fn repeated_block_penalty(moves: &[Move<'_>], block: usize) -> i64 {
    if moves.len() < block * 2 {
        return 0;
    }
    let mut first_seen: HashMap<String, usize> = HashMap::new();
    let mut penalty = 0;
    for i in 0..=moves.len() - block {
        let mut key = String::with_capacity(block * 8);
        for m in &moves[i..i + block] {
            key.push_str(m.color.letter());
            key.push(':');
            key.push_str(&m.coordinate);
            key.push('|');
        }
        match first_seen.entry(key) {
            Entry::Vacant(vacant) => {
                vacant.insert(i);
            }
            Entry::Occupied(seen) if i - *seen.get() <= 16 => penalty += block as i64 * 180,
            Entry::Occupied(_) => {}
        }
    }
    penalty
}

// ---------------------------------------------------------------------------------------------
// Java string semantics

/// `String.trim()`: strips leading and trailing characters up to U+0020.
fn java_trim(text: &str) -> &str {
    text.trim_matches(|c: char| c <= ' ')
}

/// `Character.isWhitespace` (Unicode separators except no-break spaces, plus ASCII controls).
fn is_java_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t'..='\r'
            | '\u{1C}'..='\u{20}'
            | '\u{1680}'
            | '\u{2000}'..='\u{2006}'
            | '\u{2008}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{205F}'
            | '\u{3000}'
    )
}

/// Whether the text is exactly `len` UTF-16 units long (Java `String.length()`).
fn utf16_len_is(text: &str, len: usize) -> bool {
    text.encode_utf16().take(len + 1).count() == len
}

/// `Integer.parseInt(text)` for ASCII digits with an optional sign (no trimming).
fn parse_java_int(text: &str) -> Option<i32> {
    text.parse().ok()
}

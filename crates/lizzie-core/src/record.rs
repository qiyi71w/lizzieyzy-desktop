//! Game record (棋局文档): the record tree and what each node means for the game.
//!
//! Semantics follow the fixed Java behaviour (`wimi321/lizzieyzy-next`
//! `cb53acc55eda42c6c671b5f668cd3b597456bf07`, `rules/BoardHistoryList.java`, `BoardData.java`,
//! `docs/SNAPSHOT_NODE_KIND.md`):
//!
//! - `MOVE` and `PASS` are real history actions and consume one move number.
//! - `SNAPSHOT` is a static position anchor (the root and SGF setup nodes). Setup stones are not
//!   moves, do not consume a move number and do not go through capture rules.
//! - A dummy PASS (`DD[true]`) is a placeholder, never a real action.
//! - The main line is the chain of first children from the root.
//!
//! Content changes go through [`DocumentEdit`] and [`GameRecord::apply`] only; browsing never
//! changes a record. Editing slices add their operations to [`DocumentEdit`].

use serde::Serialize;
use ts_rs::TS;

/// Index of a node inside one record. Stable for the lifetime of that record.
pub type NodeId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, TS)]
pub enum Stone {
    Black,
    White,
}

impl Stone {
    pub fn opposite(self) -> Stone {
        match self {
            Stone::Black => Stone::White,
            Stone::White => Stone::Black,
        }
    }
}

/// Board intersection; `x` is the column from the left, `y` the row from the top.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, TS)]
pub struct Point {
    pub x: u16,
    pub y: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub struct BoardSize {
    pub width: u16,
    pub height: u16,
}

impl BoardSize {
    pub const STANDARD: BoardSize = BoardSize { width: 19, height: 19 };

    pub fn contains(self, p: Point) -> bool {
        p.x < self.width && p.y < self.height
    }

    pub fn area(self) -> usize {
        self.width as usize * self.height as usize
    }

    pub fn index(self, p: Point) -> usize {
        p.y as usize * self.width as usize + p.x as usize
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind")]
pub enum NodeKind {
    /// Static position anchor: the root, or an SGF node carrying `AB`/`AW`/`AE`/`PL`.
    Snapshot,
    Move {
        color: Stone,
        point: Point,
    },
    Pass {
        color: Stone,
    },
    /// `DD[true]` placeholder; not a history action.
    DummyPass,
}

impl NodeKind {
    pub fn is_action(self) -> bool {
        matches!(self, NodeKind::Move { .. } | NodeKind::Pass { .. })
    }

    pub fn action_color(self) -> Option<Stone> {
        match self {
            NodeKind::Move { color, .. } | NodeKind::Pass { color } => Some(color),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", content = "text")]
pub enum MarkKind {
    Triangle,
    Square,
    Circle,
    Cross,
    Label(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
pub struct Mark {
    pub point: Point,
    pub mark: MarkKind,
}

/// Static setup carried by a snapshot node: the net change of its `AB`, `AW`, `AE`, each point in
/// one list at most, so the order of application does not matter.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Setup {
    pub add_black: Vec<Point>,
    pub add_white: Vec<Point>,
    pub clear: Vec<Point>,
}

impl Setup {
    pub fn is_empty(&self) -> bool {
        self.add_black.is_empty() && self.add_white.is_empty() && self.clear.is_empty()
    }
}

/// An SGF property kept as read (identifier and unescaped values). Properties the record does
/// not interpret are retained so later save/export slices do not drop them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Property {
    pub id: String,
    pub values: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub kind: NodeKind,
    pub setup: Setup,
    /// Explicit `PL` on this node.
    pub explicit_to_play: Option<Stone>,
    /// Side to play at this node as determined at import (Java `blackToPlay`).
    pub to_play: Stone,
    /// Java `BoardData.moveNumber`: actions count one, snapshots keep their parent's number.
    /// The display-only SGF `MN` is retained in `properties` without changing this count.
    pub move_number: u32,
    pub comment: String,
    pub marks: Vec<Mark>,
    pub properties: Vec<Property>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
pub struct GameInfo {
    pub black: String,
    pub white: String,
    pub black_rank: String,
    pub white_rank: String,
    pub komi: f64,
    pub handicap: u32,
    pub result: String,
    /// Raw `RU` value, if any.
    pub rules: Option<String>,
    pub date: String,
}

/// Record format a document was read from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum RecordFormat {
    Sgf,
    Gib,
}

#[derive(Clone, Debug)]
pub struct GameRecord {
    pub size: BoardSize,
    pub info: GameInfo,
    /// Whether the record itself declared a komi (`KM`/`KO`, or GIB `GONGJE`).
    pub komi_declared: bool,
    /// Whether the record starts from setup stones or declares a handicap; Java applies the
    /// record komi to such games even when automatic komi loading is off.
    pub has_setup_or_handicap: bool,
    nodes: Vec<Node>,
}

pub const ROOT: NodeId = 0;

impl GameRecord {
    /// Empty record: one root snapshot, Black to play (Java `BoardData.empty`).
    pub fn empty(size: BoardSize, komi: f64) -> GameRecord {
        GameRecord {
            size,
            info: GameInfo {
                komi,
                ..GameInfo::default()
            },
            komi_declared: false,
            has_setup_or_handicap: false,
            nodes: vec![Node {
                parent: None,
                children: Vec::new(),
                kind: NodeKind::Snapshot,
                setup: Setup::default(),
                explicit_to_play: None,
                to_play: Stone::Black,
                move_number: 0,
                comment: String::new(),
                marks: Vec::new(),
                properties: Vec::new(),
            }],
        }
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id as usize]
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn contains(&self, id: NodeId) -> bool {
        (id as usize) < self.nodes.len()
    }

    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.nodes.iter().enumerate().map(|(i, n)| (i as NodeId, n))
    }

    pub(crate) fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id as usize]
    }

    /// Appends a child after the existing children of `parent`.
    pub(crate) fn push_child(&mut self, parent: NodeId, kind: NodeKind) -> NodeId {
        let id = self.nodes.len() as NodeId;
        let parent_node = &self.nodes[parent as usize];
        let move_number = parent_node.move_number + u32::from(kind.is_action());
        let to_play = match kind.action_color() {
            Some(c) => c.opposite(),
            None => parent_node.to_play,
        };
        self.nodes.push(Node {
            parent: Some(parent),
            children: Vec::new(),
            kind,
            setup: Setup::default(),
            explicit_to_play: None,
            to_play,
            move_number,
            comment: String::new(),
            marks: Vec::new(),
            properties: Vec::new(),
        });
        self.nodes[parent as usize].children.push(id);
        id
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).parent
    }

    /// First child (Java `next()`).
    pub fn main_child(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).children.first().copied()
    }

    pub fn depth(&self, mut id: NodeId) -> usize {
        let mut d = 0;
        while let Some(p) = self.parent(id) {
            d += 1;
            id = p;
        }
        d
    }

    /// Whether every ancestor reaches this node through its first child.
    pub fn is_main_trunk(&self, mut id: NodeId) -> bool {
        while let Some(p) = self.parent(id) {
            if self.main_child(p) != Some(id) {
                return false;
            }
            id = p;
        }
        true
    }

    /// Last node of the first-child chain from `id`.
    pub fn line_end(&self, mut id: NodeId) -> NodeId {
        while let Some(c) = self.main_child(id) {
            id = c;
        }
        id
    }

    /// Nodes from the root to `id`, inclusive.
    pub fn path_to(&self, id: NodeId) -> Vec<NodeId> {
        let mut path = vec![id];
        let mut cur = id;
        while let Some(p) = self.parent(cur) {
            path.push(p);
            cur = p;
        }
        path.reverse();
        path
    }

    /// Replays the static setup and real moves from the root to `id`.
    pub fn position(&self, id: NodeId) -> Position {
        let mut pos = Position::new(self.size);
        for n in self.path_to(id) {
            let node = self.node(n);
            pos.apply_setup(&node.setup);
            if let NodeKind::Move { color, point } = node.kind {
                pos.play(color, point);
            }
        }
        pos
    }
}

/// A content change to a record: the one write path of the document module.
#[derive(Clone, Debug, PartialEq)]
pub enum DocumentEdit {
    /// Replaces the comment of a node (Java `BoardData.comment`).
    SetComment { node: NodeId, comment: String },
    /// The game komi (`GameInfo.setKomi`), saved as `KM`.
    SetKomi { komi: f64 },
}

impl GameRecord {
    /// Applies `edit`. Returns whether the content changed; an edit naming a node that is not in
    /// this record is refused without changing anything.
    pub fn apply(&mut self, edit: &DocumentEdit) -> Result<bool, String> {
        match edit {
            DocumentEdit::SetComment { node, comment } => {
                if !self.contains(*node) {
                    return Err(format!("no node {node} in this document"));
                }
                let target = &mut self.node_mut(*node).comment;
                if target == comment {
                    return Ok(false);
                }
                comment.clone_into(target);
                Ok(true)
            }
            DocumentEdit::SetKomi { komi } => {
                if self.info.komi == *komi {
                    return Ok(false);
                }
                self.info.komi = *komi;
                Ok(true)
            }
        }
    }
}

/// Stones on the board after replaying a path, with capture counts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Position {
    pub size: BoardSize,
    pub stones: Vec<Option<Stone>>,
    /// Stones captured by Black / by White.
    pub black_captures: u32,
    pub white_captures: u32,
}

impl Position {
    pub fn new(size: BoardSize) -> Position {
        Position {
            size,
            stones: vec![None; size.area()],
            black_captures: 0,
            white_captures: 0,
        }
    }

    pub fn at(&self, p: Point) -> Option<Stone> {
        self.stones[self.size.index(p)]
    }

    fn set(&mut self, p: Point, s: Option<Stone>) {
        let i = self.size.index(p);
        self.stones[i] = s;
    }

    pub fn apply_setup(&mut self, setup: &Setup) {
        for &p in &setup.add_black {
            self.set(p, Some(Stone::Black));
        }
        for &p in &setup.add_white {
            self.set(p, Some(Stone::White));
        }
        for &p in &setup.clear {
            self.set(p, None);
        }
    }

    fn neighbours(size: BoardSize, p: Point) -> impl Iterator<Item = Point> {
        [
            (p.x.checked_sub(1), Some(p.y)),
            (p.x.checked_add(1), Some(p.y)),
            (Some(p.x), p.y.checked_sub(1)),
            (Some(p.x), p.y.checked_add(1)),
        ]
        .into_iter()
        .filter_map(move |c| match c {
            (Some(x), Some(y)) if size.contains(Point { x, y }) => Some(Point { x, y }),
            _ => None,
        })
    }

    /// The chain containing `p` if it has no liberty.
    fn dead_chain(&self, p: Point) -> Option<Vec<Point>> {
        let color = self.at(p)?;
        let mut seen = vec![false; self.size.area()];
        let mut stack = vec![p];
        let mut chain = Vec::new();
        seen[self.size.index(p)] = true;
        while let Some(q) = stack.pop() {
            chain.push(q);
            for n in Self::neighbours(self.size, q) {
                match self.at(n) {
                    None => return None,
                    Some(c) if c == color && !seen[self.size.index(n)] => {
                        seen[self.size.index(n)] = true;
                        stack.push(n);
                    }
                    _ => {}
                }
            }
        }
        Some(chain)
    }

    /// Whether playing `color` at the empty point `p` would leave its own chain without a
    /// liberty after removing captured opponent chains.
    pub fn is_suicide(&self, color: Stone, p: Point) -> bool {
        let mut probe = self.clone();
        probe.set(p, Some(color));
        let neighbours: Vec<Point> = Self::neighbours(self.size, p).collect();
        for n in neighbours {
            if probe.at(n) == Some(color.opposite()) && probe.dead_chain(n).is_some() {
                return false;
            }
        }
        probe.dead_chain(p).is_some()
    }

    /// Places a stone and removes the opponent chains it captures (Java
    /// `BoardHistoryList.place`, `Board.removeDeadChain` on the four neighbours).
    pub fn play(&mut self, color: Stone, p: Point) {
        let mut captured = Vec::new();
        self.place(color, p, &mut captured);
        let count = captured.len() as u32;
        match color {
            Stone::Black => self.black_captures += count,
            Stone::White => self.white_captures += count,
        }
    }

    /// Puts `color` at `p` (whatever was there) and removes the opponent chains left without a
    /// liberty, appending their points to `captured` (`Branch`, `removeDeadChain`).
    pub fn place(&mut self, color: Stone, p: Point, captured: &mut Vec<Point>) {
        self.set(p, Some(color));
        let neighbours: Vec<Point> = Self::neighbours(self.size, p).collect();
        for n in neighbours {
            if self.at(n) == Some(color.opposite()) {
                if let Some(chain) = self.dead_chain(n) {
                    for &q in &chain {
                        self.set(q, None);
                    }
                    captured.extend(chain);
                }
            }
        }
    }
}

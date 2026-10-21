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
//! Content changes go through [`DocumentEdit`] and [`GameRecord::apply`] only (operations in
//! [`edit`]); browsing never changes a record.
//!
//! Node identity: a [`NodeId`] indexes the node arena and stays valid for the record's lifetime.
//! Removed nodes stay in the arena, detached and no longer live. Every node also carries a
//! [`Node::stamp`], unique within the record and replaced whenever what the node's position means
//! changes in place (its setup or side to play), so engine positions and stored analysis can tell
//! a changed node from the one they were made for.

mod edit;

pub use edit::{Applied, EditError, MarkTool, PlayRules, StoneChange, Transform};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Index of a node inside one record. Stable for the lifetime of that record.
pub type NodeId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
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
    /// Position identity: unique in the record, replaced when the node's setup or side to play
    /// changes in place.
    pub stamp: u64,
    /// Still part of the record (not removed).
    pub live: bool,
    /// The move was placed without removing any stone (Java `no-capture`, 五子棋无提子规则, in
    /// force when it was placed or read): its position keeps every stone.
    pub no_capture: bool,
    /// 分支内从1开始显示 was on when the node was created (read or played): as a later sibling
    /// it starts its own move numbers at 1 (`BoardHistoryNode.addOrGoto`,
    /// `Menu.moveNumberInBranchTips`). Display only; not saved.
    pub branch_numbers_from_one: bool,
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
    /// Next [`Node::stamp`].
    next_stamp: u64,
    /// [`Node::branch_numbers_from_one`] of the nodes created from now on.
    branch_numbers_from_one: bool,
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
                stamp: 0,
                live: true,
                no_capture: false,
                branch_numbers_from_one: true,
            }],
            next_stamp: 1,
            branch_numbers_from_one: true,
        }
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id as usize]
    }

    /// Size of the node arena, removed nodes included.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Whether `id` is a node of this record that has not been removed.
    pub fn contains(&self, id: NodeId) -> bool {
        self.nodes.get(id as usize).is_some_and(|n| n.live)
    }

    /// The nodes still in the record, by id.
    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &Node)> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.live)
            .map(|(i, n)| (i as NodeId, n))
    }

    /// Whether the record holds anything after its root (Java "has previous or next").
    pub fn has_children_of_root(&self) -> bool {
        !self.node(ROOT).children.is_empty()
    }

    /// `Board.hasRealMoveOrPassHistory`: any real move or pass anywhere in the tree.
    pub fn has_actions(&self) -> bool {
        self.nodes().any(|(_, n)| n.kind.is_action())
    }

    pub(crate) fn node_mut(&mut self, id: NodeId) -> &mut Node {
        &mut self.nodes[id as usize]
    }

    fn new_stamp(&mut self) -> u64 {
        let stamp = self.next_stamp;
        self.next_stamp += 1;
        stamp
    }

    /// Gives `id` a new [`Node::stamp`]: what its position means has changed in place.
    pub(crate) fn restamp(&mut self, id: NodeId) {
        let stamp = self.new_stamp();
        self.nodes[id as usize].stamp = stamp;
    }

    /// The 分支内从1开始显示 setting nodes created from now on are numbered with.
    pub(crate) fn set_branch_numbering(&mut self, from_one: bool) {
        self.branch_numbers_from_one = from_one;
    }

    /// A record just read takes the setting in force while it was read, like Java's reader,
    /// which numbers each branch as it creates it.
    pub(crate) fn number_branches_as_read(&mut self, from_one: bool) {
        self.branch_numbers_from_one = from_one;
        for n in &mut self.nodes {
            n.branch_numbers_from_one = from_one;
        }
    }

    /// Appends a child after the existing children of `parent`.
    pub(crate) fn push_child(&mut self, parent: NodeId, kind: NodeKind) -> NodeId {
        let id = self.nodes.len() as NodeId;
        let stamp = self.new_stamp();
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
            stamp,
            live: true,
            no_capture: false,
            branch_numbers_from_one: self.branch_numbers_from_one,
        });
        self.nodes[parent as usize].children.push(id);
        id
    }

    /// Makes `self` (an earlier state of the same document) safe to resume after `newer`: node
    /// ids and stamps handed out by `newer` are never handed out again, so nothing made for a
    /// node of `newer` can be mistaken for a node of `self`.
    pub(crate) fn resume_after(&mut self, newer: &GameRecord) {
        while self.nodes.len() < newer.nodes.len() {
            self.nodes.push(Node {
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
                stamp: u64::MAX,
                live: false,
                no_capture: false,
                branch_numbers_from_one: false,
            });
        }
        self.next_stamp = self.next_stamp.max(newer.next_stamp);
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
            pos.enter(self.node(n));
        }
        pos
    }
}

/// A content change to a record: the one write path of the document module. The operations are
/// in [`edit`]; [`GameRecord::apply`] returns [`edit::Applied`].
#[derive(Clone, Debug, PartialEq)]
pub enum DocumentEdit {
    /// Replaces the comment of a node (Java `BoardData.comment`).
    SetComment {
        node: NodeId,
        comment: String,
    },
    /// The game komi (`GameInfo.setKomi`), saved as `KM`.
    SetKomi {
        komi: f64,
    },
    /// Player names (`GameInfoDialog`: `PB`, `PW`).
    SetPlayers {
        black: String,
        white: String,
    },
    /// A stone on `at`'s position (`Board.place`). `alternate`: the colour is the side to play
    /// (交替落子), so a click on the main continuation's point follows it.
    Play {
        at: NodeId,
        color: Stone,
        point: Point,
        alternate: bool,
        rules: PlayRules,
    },
    /// A real pass (`Board.pass`).
    Pass {
        at: NodeId,
        color: Stone,
    },
    /// An engine variation added as a new branch of `at` without moving there
    /// (`BoardRenderer.addSuggestionAsBranch`): a node without continuation first gets an end
    /// placeholder so the variation stays a side branch; the first move always starts a new
    /// variation, the others follow equal existing moves. Colours alternate from `at`'s side to
    /// play; an illegal move ends the variation.
    AddBranch {
        at: NodeId,
        moves: Vec<Point>,
        rules: PlayRules,
    },
    /// A move between `at` and its continuation, which is played again after it
    /// (`LizzieFrame.insertMove`).
    Insert {
        at: NodeId,
        color: Stone,
        point: Point,
        rules: PlayRules,
    },
    /// Changes the stone at `point` of `at`'s position where it was placed, and plays the rest
    /// again (`Board.editMove`, `LizzieFrame.DraggedReleased`).
    EditStone {
        at: NodeId,
        point: Point,
        change: StoneChange,
        rules: PlayRules,
    },
    /// Removes `node` and everything after it (`deleteMove`); not the root.
    Remove {
        node: NodeId,
    },
    /// The branch holding `node` swaps with its previous (`up`) or next variation.
    MoveBranch {
        node: NodeId,
        up: bool,
    },
    /// The line through `node` becomes the main line (`LizzieFrame.setAsMain`).
    SetAsMain {
        node: NodeId,
    },
    /// Side to play at `node` (`Board.changeNextTurn`).
    SetToPlay {
        node: NodeId,
        color: Stone,
    },
    /// Starting-position setup on a root without moves: `Some` places or replaces a stone,
    /// `None` erases it.
    SetupStone {
        point: Point,
        stone: Option<Stone>,
    },
    /// `setupClearAll`.
    SetupClearAll,
    /// 黑先 / 白先 of the starting position.
    SetupSideToPlay {
        color: Stone,
    },
    /// The position at `node` becomes the root position; all moves are discarded.
    ConvertToStartingPosition {
        node: NodeId,
    },
    Transform(Transform),
    /// Markup tool click on `point` of `node`.
    ToggleMark {
        node: NodeId,
        point: Point,
        tool: MarkTool,
    },
    /// 清除标记: the marks of `node`.
    ClearMarks {
        node: NodeId,
    },
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

    /// Puts `color` at `p` without removing anything (`no-capture`).
    pub fn put(&mut self, color: Stone, p: Point) {
        self.set(p, Some(color));
    }

    /// The step into `node` from its parent's position: its setup, then its move (placed without
    /// removing anything when it was placed under `no-capture`).
    pub(crate) fn enter(&mut self, node: &Node) {
        self.apply_setup(&node.setup);
        if let NodeKind::Move { color, point } = node.kind {
            if node.no_capture {
                self.set(point, Some(color));
            } else {
                self.play(color, point);
            }
        }
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

    /// Places a stone and removes the opponent chains it captures, then its own chain if that has
    /// no liberty left (Java `BoardHistoryList.place`, `Board.removeDeadChain` on the four
    /// neighbours and on the point). Returns the number of own stones removed (a suicide).
    pub fn play(&mut self, color: Stone, p: Point) -> usize {
        let mut captured = Vec::new();
        self.place(color, p, &mut captured);
        let count = captured.len() as u32;
        match color {
            Stone::Black => self.black_captures += count,
            Stone::White => self.white_captures += count,
        }
        match self.dead_chain(p) {
            Some(own) => {
                for &q in &own {
                    self.set(q, None);
                }
                own.len()
            }
            None => 0,
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

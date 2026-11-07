//! The operations behind [`DocumentEdit`]: moves and passes, insertion and stone edits that
//! replay the following moves, removal, branch order, side to play, starting-position setup,
//! whole-record transforms, comments, markup and player names.
//!
//! Behaviour follows the fixed Java build (`rules/Board.java` `place`/`pass`/`deleteMove`/
//! `moveBranchUp`/`setAsMainBranch`/`editMove`/`setup*`/`convertCurrentPositionToStartingPosition`/
//! `getAllMovelist`, `BoardHistoryNode.addOrGoto`, `LizzieFrame.insertMove`/`DraggedReleased`/
//! `tryToMarkup`). Where Java rebuilds moves through `MoveLinkedList` it keeps only coordinates and
//! colours; here the rebuilt nodes keep their comments, marks, setup snapshots and non-analysis
//! properties (Java silently dropped them), while analysis data (`LZ*`) of a node whose position
//! changed is dropped.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{
    DocumentEdit, GameRecord, Mark, MarkKind, NodeId, NodeKind, Point, Position, Property, Setup, Stone, ROOT,
};

/// What the rules allow when a stone is placed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayRules {
    /// The foreground engine's rules allow suicide of more than one stone (`Leelaz.canSuicidal`,
    /// KataGo `suicide`); a single-stone suicide is never played.
    pub allow_suicide: bool,
    /// 五子棋无提子规则 (`no-capture`): a stone removes nothing, so only an occupied point (or a
    /// return to the previous position) refuses it.
    pub no_capture: bool,
}

/// A change to a stone already on the board (RightClickMenu2 反色 / 删除 / 移动, dragging).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind")]
pub enum StoneChange {
    /// The stone changes colour.
    Switch,
    /// The stone is taken out of the record.
    Delete,
    /// The stone moves to an empty point.
    MoveTo { point: Point },
}

/// Markup tool of the top toolbar (`LizzieFrame.markupType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum MarkTool {
    Letter,
    Number,
    Circle,
    Cross,
    Square,
    Triangle,
    Eraser,
}

/// Whole-record transforms of the Edit menu (`Board.exchangeBlackWhite`, `SpinAndMirror`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum Transform {
    /// 交换黑白: every stone and pass changes colour, and so does the side to play.
    ExchangeColors,
    /// 向右旋转 (type 1): `(x, y) → (w-1-y, x)`.
    RotateRight,
    /// 向左旋转 (type 2): `(x, y) → (y, h-1-x)`.
    RotateLeft,
    /// 水平翻转 (type 3): `(x, y) → (w-1-x, y)`.
    MirrorVertical,
    /// 垂直翻转 (type 4): `(x, y) → (x, h-1-y)`.
    MirrorHorizontal,
}

/// Why an edit was refused; the record is unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditError {
    UnknownNode(NodeId),
    /// Starting-position setup edits only a root that has nothing after it
    /// (`LizzieFrame.setupEditRequiresRootOnly`).
    SetupRequiresRootOnly,
    /// Rotation needs a square board (`SpinAndMirror.noneSquareError`).
    NotSquare,
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditError::UnknownNode(n) => write!(f, "no node {n} in this document"),
            EditError::SetupRequiresRootOnly => f.write_str("setup edits only a root without moves"),
            EditError::NotSquare => f.write_str("only a square board can be rotated"),
        }
    }
}

/// Result of an applied edit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Applied {
    /// Something in the record changed (its tree, a position or node content).
    pub changed: bool,
    /// Content a saved file holds changed; the document revision moves on.
    pub content: bool,
    /// The node the edit leads to: the played or followed move, the inserted move, the parent of a
    /// removed node, the current node after a replay.
    pub node: Option<NodeId>,
}

impl Applied {
    fn content(node: Option<NodeId>) -> Applied {
        Applied {
            changed: true,
            content: true,
            node,
        }
    }

    fn unchanged(node: Option<NodeId>) -> Applied {
        Applied {
            changed: false,
            content: false,
            node,
        }
    }
}

/// A node taken out of the tree to be played again (`MoveLinkedList`, keeping node content).
struct Carry {
    old: NodeId,
    kind: NodeKind,
    setup: Setup,
    explicit_to_play: Option<Stone>,
    to_play: Stone,
    comment: String,
    marks: Vec<Mark>,
    properties: Vec<Property>,
    children: Vec<Carry>,
}

/// Analysis results stored in the file (`LZ`, `LZ2`, `LZOP`, `LZOP2`) belong to one position.
fn without_analysis(properties: Vec<Property>) -> Vec<Property> {
    properties.into_iter().filter(|p| !p.id.starts_with("LZ")).collect()
}

impl GameRecord {
    /// Applies `edit`. An edit naming a node that is not in this record, or one the rules refuse
    /// to apply, leaves the record unchanged; an illegal move is ignored like Java (`Ok` with
    /// nothing changed and no node).
    pub fn apply(&mut self, edit: &DocumentEdit) -> Result<Applied, EditError> {
        let known = |rec: &GameRecord, n: NodeId| {
            if rec.contains(n) {
                Ok(())
            } else {
                Err(EditError::UnknownNode(n))
            }
        };
        match edit {
            DocumentEdit::SetComment { node, comment } => {
                known(self, *node)?;
                let target = &mut self.node_mut(*node).comment;
                if target == comment {
                    return Ok(Applied::unchanged(None));
                }
                comment.clone_into(target);
                Ok(Applied::content(None))
            }
            DocumentEdit::SetKomi { komi } => {
                if self.info.komi == *komi {
                    return Ok(Applied::unchanged(None));
                }
                self.info.komi = *komi;
                Ok(Applied::content(None))
            }
            DocumentEdit::SetPlayers { black, white } => {
                if &self.info.black == black && &self.info.white == white {
                    return Ok(Applied::unchanged(None));
                }
                black.clone_into(&mut self.info.black);
                white.clone_into(&mut self.info.white);
                Ok(Applied::content(None))
            }
            DocumentEdit::SetResult { result } => {
                if &self.info.result == result {
                    return Ok(Applied::unchanged(None));
                }
                result.clone_into(&mut self.info.result);
                Ok(Applied::content(None))
            }
            DocumentEdit::Play {
                at,
                color,
                point,
                alternate,
                rules,
            } => {
                known(self, *at)?;
                let kind = NodeKind::Move {
                    color: *color,
                    point: *point,
                };
                Ok(match self.place(*at, kind, *rules, *alternate) {
                    Some((node, created)) if created => Applied::content(Some(node)),
                    Some((node, _)) => Applied::unchanged(Some(node)),
                    None => Applied::unchanged(None),
                })
            }
            DocumentEdit::Pass { at, color } => {
                known(self, *at)?;
                let kind = NodeKind::Pass { color: *color };
                Ok(match self.place(*at, kind, PlayRules::default(), true) {
                    Some((node, true)) => Applied::content(Some(node)),
                    Some((node, false)) => Applied::unchanged(Some(node)),
                    None => Applied::unchanged(None),
                })
            }
            DocumentEdit::AddBranch { at, moves, rules } => {
                known(self, *at)?;
                Ok(self.add_branch(*at, moves, *rules))
            }
            DocumentEdit::Insert {
                at,
                color,
                point,
                rules,
            } => {
                known(self, *at)?;
                Ok(self.insert(*at, *color, *point, *rules))
            }
            DocumentEdit::EditStone {
                at,
                point,
                change,
                rules,
            } => {
                known(self, *at)?;
                Ok(self.edit_stone(*at, *point, *change, *rules))
            }
            DocumentEdit::Remove { node } => {
                known(self, *node)?;
                let Some(parent) = self.parent(*node) else {
                    return Ok(Applied::unchanged(None));
                };
                self.detach(*node);
                Ok(Applied::content(Some(parent)))
            }
            DocumentEdit::MoveBranch { node, up } => {
                known(self, *node)?;
                Ok(self.move_branch(*node, *up))
            }
            DocumentEdit::SetAsMain { node } => {
                known(self, *node)?;
                let mut changed = false;
                while self.promote_once(*node) {
                    changed = true;
                }
                Ok(if changed {
                    Applied::content(None)
                } else {
                    Applied::unchanged(None)
                })
            }
            DocumentEdit::SetToPlay { node, color } => {
                known(self, *node)?;
                Ok(self.set_to_play(*node, *color))
            }
            DocumentEdit::SetupStone { point, stone } => {
                self.setup_root()?;
                Ok(self.setup_stone(*point, *stone))
            }
            DocumentEdit::SetupClearAll => {
                self.setup_root()?;
                let root = self.node_mut(ROOT);
                if root.setup.is_empty() {
                    return Ok(Applied::unchanged(None));
                }
                root.setup = Setup::default();
                self.restamp_position(ROOT);
                Ok(Applied::content(None))
            }
            DocumentEdit::SetupSideToPlay { color } => {
                self.setup_root()?;
                Ok(self.set_to_play(ROOT, *color))
            }
            DocumentEdit::ConvertToStartingPosition { node } => {
                known(self, *node)?;
                Ok(self.convert_to_start(*node))
            }
            DocumentEdit::Transform(t) => self.transform(*t),
            DocumentEdit::ToggleMark { node, point, tool } => {
                known(self, *node)?;
                Ok(self.toggle_mark(*node, *point, *tool))
            }
            DocumentEdit::ClearMarks { node } => {
                known(self, *node)?;
                let marks = &mut self.node_mut(*node).marks;
                if marks.is_empty() {
                    return Ok(Applied::unchanged(None));
                }
                marks.clear();
                Ok(Applied::content(None))
            }
        }
    }

    // ------------------------------------------------------------------------ placing

    /// `Board.place` / `Board.pass` from `at`: the move a click repeats is followed, an illegal
    /// stone is refused, an equal existing variation is followed, else a new last variation is
    /// added. Returns the node and whether it was created.
    ///
    /// `follow_main_by_point`: the alternate-colour fast path (`blackorwhite == 0`) follows the
    /// main continuation at the same point whatever its colour; a pass always follows a real pass
    /// that continues the line.
    fn place(
        &mut self,
        at: NodeId,
        kind: NodeKind,
        rules: PlayRules,
        follow_main_by_point: bool,
    ) -> Option<(NodeId, bool)> {
        if let Some(next) = self.visible_main_child(at) {
            let repeats = match (kind, self.node(next).kind) {
                (NodeKind::Move { point, .. }, NodeKind::Move { point: p, .. }) => follow_main_by_point && p == point,
                (NodeKind::Pass { .. }, NodeKind::Pass { .. }) => true,
                _ => false,
            };
            if repeats {
                return Some((next, false));
            }
        }
        if let NodeKind::Move { color, point } = kind {
            if !self.is_legal(at, color, point, rules) {
                return None;
            }
        }
        if let Some(&same) = self.node(at).children.iter().find(|&&c| self.node(c).kind == kind) {
            return Some((same, false));
        }
        Some((self.add_action(at, kind, rules), true))
    }

    /// First child unless it is an end placeholder (`BoardHistoryNode.next(false)`).
    fn visible_main_child(&self, id: NodeId) -> Option<NodeId> {
        self.main_child(id).filter(|&c| !self.is_end_dummy(c))
    }

    fn is_end_dummy(&self, id: NodeId) -> bool {
        let n = self.node(id);
        n.kind == NodeKind::DummyPass && n.children.is_empty()
    }

    /// Occupied points, single-stone suicide (and any suicide the rules forbid) and a return to
    /// the position before the last move (`violatesKoRule`) are illegal.
    fn is_legal(&self, at: NodeId, color: Stone, point: Point, rules: PlayRules) -> bool {
        if !self.size.contains(point) {
            return false;
        }
        let Some(next) = self.position(at).try_play(color, point, rules) else {
            return false;
        };
        match self.parent(at) {
            Some(before) => self.position(before).stones != next.stones,
            None => true,
        }
    }

    /// `BoardHistoryNode.addOrGoto` adding a node: an end placeholder is replaced, and an action
    /// node without `PL` that gets its first continuation takes that move's colour as its side
    /// to play (`alignParentTurnStateWithFirstActionChild`). A move is placed under `rules`.
    fn add_action(&mut self, at: NodeId, kind: NodeKind, rules: PlayRules) -> NodeId {
        let dummy = self.main_child(at).filter(|&c| self.is_end_dummy(c));
        let first = self.node(at).children.is_empty();
        let id = self.push_move(at, kind, rules);
        if let Some(d) = dummy {
            self.node_mut(at).children.retain(|&c| c != id);
            let slot = self
                .node(at)
                .children
                .iter()
                .position(|&c| c == d)
                .expect("placeholder child");
            self.node_mut(at).children[slot] = id;
            self.mark_removed(d);
        } else if first {
            let parent = self.node(at);
            if let Some(color) = kind.action_color() {
                if parent.kind.is_action() && parent.explicit_to_play.is_none() && parent.to_play != color {
                    self.node_mut(at).to_play = color;
                    self.restamp_position(at);
                }
            }
        }
        id
    }

    /// A new last child; a move records whether it removes stones.
    fn push_move(&mut self, at: NodeId, kind: NodeKind, rules: PlayRules) -> NodeId {
        let id = self.push_child(at, kind);
        self.node_mut(id).no_capture = rules.no_capture && matches!(kind, NodeKind::Move { .. });
        id
    }

    fn add_branch(&mut self, at: NodeId, moves: &[Point], rules: PlayRules) -> Applied {
        let Some((&first, rest)) = moves.split_first() else {
            return Applied::unchanged(None);
        };
        let color = self.node(at).to_play;
        if !self.is_legal(at, color, first, rules) {
            return Applied::unchanged(None);
        }
        if self.node(at).children.is_empty() {
            self.push_child(at, NodeKind::DummyPass);
        }
        let mut cur = self.push_move(at, NodeKind::Move { color, point: first }, rules);
        for &point in rest {
            let color = self.node(cur).to_play;
            match self.place(cur, NodeKind::Move { color, point }, rules, false) {
                Some((next, _)) => cur = next,
                None => break,
            }
        }
        Applied::content(None)
    }

    // ------------------------------------------------------------------------ removal

    /// Takes `id` and everything after it out of the tree.
    fn detach(&mut self, id: NodeId) {
        if let Some(parent) = self.parent(id) {
            self.node_mut(parent).children.retain(|&c| c != id);
        }
        self.mark_removed(id);
    }

    fn mark_removed(&mut self, id: NodeId) {
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            let node = self.node_mut(n);
            node.live = false;
            node.parent = None;
            stack.extend(std::mem::take(&mut node.children));
        }
    }

    // ------------------------------------------------------------------------ replaying

    fn take(&self, id: NodeId) -> Carry {
        let n = self.node(id);
        Carry {
            old: id,
            kind: n.kind,
            setup: n.setup.clone(),
            explicit_to_play: n.explicit_to_play,
            to_play: n.to_play,
            comment: n.comment.clone(),
            marks: n.marks.clone(),
            properties: n.properties.clone(),
            children: n.children.iter().map(|&c| self.take(c)).collect(),
        }
    }

    /// Plays a taken node again under `parent` (`placeLinkedList`): a move that is no longer
    /// legal there is skipped and what followed it continues from `parent`; a move equal to an
    /// existing variation joins it. `map` records where every old node ended up.
    fn replay(
        &mut self,
        parent: NodeId,
        carry: Carry,
        rules: PlayRules,
        map: &mut HashMap<NodeId, NodeId>,
    ) -> Option<NodeId> {
        let landed = match carry.kind {
            NodeKind::Move { .. } | NodeKind::Pass { .. } => match self.place(parent, carry.kind, rules, false) {
                Some((id, created)) => {
                    let node = self.node_mut(id);
                    if created {
                        node.comment = carry.comment;
                        node.marks = carry.marks;
                        node.properties = without_analysis(carry.properties);
                    } else {
                        if node.comment.is_empty() {
                            node.comment = carry.comment;
                        }
                        if node.marks.is_empty() {
                            node.marks = carry.marks;
                        }
                    }
                    Some(id)
                }
                None => None,
            },
            NodeKind::Snapshot | NodeKind::DummyPass => {
                let id = self.push_child(parent, carry.kind);
                let node = self.node_mut(id);
                node.setup = carry.setup;
                node.explicit_to_play = carry.explicit_to_play;
                if carry.kind == NodeKind::Snapshot {
                    node.to_play = carry.explicit_to_play.unwrap_or(carry.to_play);
                }
                node.comment = carry.comment;
                node.marks = carry.marks;
                node.properties = without_analysis(carry.properties);
                Some(id)
            }
        };
        let next_parent = landed.unwrap_or(parent);
        map.insert(carry.old, next_parent);
        for child in carry.children {
            self.replay(next_parent, child, rules, map);
        }
        landed
    }

    /// Moves `child` of `parent` to position `index` among the children.
    fn reorder(&mut self, parent: NodeId, child: NodeId, index: usize) {
        let children = &mut self.node_mut(parent).children;
        if let Some(at) = children.iter().position(|&c| c == child) {
            children.remove(at);
            let index = index.min(children.len());
            children.insert(index, child);
        }
    }

    /// `LizzieFrame.insertMove`: a move between `at` and everything that followed it, which is
    /// played again after the new move. The new move is the current node afterwards.
    fn insert(&mut self, at: NodeId, color: Stone, point: Point, rules: PlayRules) -> Applied {
        if !self.is_legal(at, color, point, rules) {
            return Applied::unchanged(None);
        }
        let children = std::mem::take(&mut self.node_mut(at).children);
        let carries: Vec<Carry> = children.iter().map(|&c| self.take(c)).collect();
        for c in children {
            self.mark_removed(c);
        }
        let inserted = self.add_action(at, NodeKind::Move { color, point }, rules);
        let mut map = HashMap::new();
        for c in carries {
            self.replay(inserted, c, rules, &mut map);
        }
        Applied::content(Some(inserted))
    }

    /// `Board.editMove` / `LizzieFrame.DraggedReleased` for the stone at `point` of `at`'s
    /// position. A stone played in the record is changed where it was played and everything after
    /// it is played again; a setup stone is changed in the setup that placed it and everything
    /// after that setup is played again. Returns where the current node `at` ended up.
    fn edit_stone(&mut self, at: NodeId, point: Point, change: StoneChange, rules: PlayRules) -> Applied {
        let here = self.position(at);
        let Some(stone) = here
            .stones
            .get(self.size.index(point))
            .copied()
            .flatten()
            .filter(|_| self.size.contains(point))
        else {
            return Applied::unchanged(None);
        };
        if let StoneChange::MoveTo { point: to } = change {
            if to == point || !self.size.contains(to) || here.at(to).is_some() {
                return Applied::unchanged(None);
            }
        }
        let played = self
            .path_to(at)
            .into_iter()
            .rev()
            .find(|&n| self.node(n).kind == NodeKind::Move { color: stone, point });
        let mut map = HashMap::new();
        if let Some(m) = played.filter(|_| self.played_stone_stands(at, point)) {
            let parent = self.parent(m).expect("a move has a parent");
            let index = self.node(parent).children.iter().position(|&c| c == m).expect("child");
            let mut carry = self.take(m);
            self.detach(m);
            match change {
                StoneChange::Delete => {
                    map.insert(carry.old, parent);
                    for child in std::mem::take(&mut carry.children) {
                        self.replay(parent, child, rules, &mut map);
                    }
                }
                StoneChange::Switch | StoneChange::MoveTo { .. } => {
                    carry.kind = match change {
                        StoneChange::Switch => NodeKind::Move {
                            color: stone.opposite(),
                            point,
                        },
                        StoneChange::MoveTo { point: to } => NodeKind::Move {
                            color: stone,
                            point: to,
                        },
                        StoneChange::Delete => unreachable!(),
                    };
                    if let Some(head) = self.replay(parent, carry, rules, &mut map) {
                        self.reorder(parent, head, index);
                    }
                }
            }
        } else {
            let Some(anchor) = self.path_to(at).into_iter().rev().find(|&n| {
                self.node(n).setup.add_black.contains(&point) || self.node(n).setup.add_white.contains(&point)
            }) else {
                return Applied::unchanged(None);
            };
            let had_stone_before = self
                .parent(anchor)
                .is_some_and(|p| self.position(p).at(point).is_some());
            let setup = &mut self.node_mut(anchor).setup;
            setup.add_black.retain(|&q| q != point);
            setup.add_white.retain(|&q| q != point);
            let (stays_at, color) = match change {
                StoneChange::Switch => (point, stone.opposite()),
                StoneChange::Delete | StoneChange::MoveTo { .. } => {
                    if had_stone_before && !setup.clear.contains(&point) {
                        setup.clear.push(point);
                    }
                    match change {
                        StoneChange::MoveTo { point: to } => (to, stone),
                        _ => (point, stone),
                    }
                }
            };
            if change != StoneChange::Delete {
                setup.clear.retain(|&q| q != stays_at);
                match color {
                    Stone::Black => setup.add_black.push(stays_at),
                    Stone::White => setup.add_white.push(stays_at),
                }
            }
            self.restamp_position(anchor);
            map.insert(anchor, anchor);
            let children = std::mem::take(&mut self.node_mut(anchor).children);
            let carries: Vec<Carry> = children.iter().map(|&c| self.take(c)).collect();
            for c in children {
                self.mark_removed(c);
            }
            for c in carries {
                self.replay(anchor, c, rules, &mut map);
            }
        }
        Applied::content(map.get(&at).copied())
    }

    /// Whether the stone `point` holds at `at` is the one its latest move played there (a later
    /// setup on the path may have replaced it).
    fn played_stone_stands(&self, at: NodeId, point: Point) -> bool {
        for n in self.path_to(at).into_iter().rev() {
            let node = self.node(n);
            if matches!(node.kind, NodeKind::Move { point: p, .. } if p == point) {
                return true;
            }
            let s = &node.setup;
            if s.add_black.contains(&point) || s.add_white.contains(&point) || s.clear.contains(&point) {
                return false;
            }
        }
        false
    }

    // ------------------------------------------------------------------------ branches

    /// `Board.moveBranchUp/Down`: the branch holding `node` (from its top, `topOfBranch`) swaps
    /// places with the previous or next variation.
    fn move_branch(&mut self, node: NodeId, up: bool) -> Applied {
        let mut top = node;
        while let Some(p) = self.parent(top) {
            if self.node(p).children.len() != 1 {
                break;
            }
            top = p;
        }
        let Some(parent) = self.parent(top) else {
            return Applied::unchanged(None);
        };
        let children = &mut self.node_mut(parent).children;
        let index = children.iter().position(|&c| c == top).expect("child");
        let other = if up {
            index.checked_sub(1)
        } else {
            Some(index + 1).filter(|&i| i < children.len())
        };
        match other {
            Some(o) => {
                children.swap(index, o);
                Applied::content(None)
            }
            None => Applied::unchanged(None),
        }
    }

    /// One step of `Board.setAsMainBranch`: the branch that joins the main trunk below `node`
    /// becomes the first variation there.
    fn promote_once(&mut self, node: NodeId) -> bool {
        if self.is_main_trunk(node) {
            return false;
        }
        let mut top = node;
        while let Some(p) = self.parent(top) {
            if self.is_main_trunk(p) {
                break;
            }
            top = p;
        }
        let parent = self.parent(top).expect("a branch joins the main trunk");
        let children = &mut self.node_mut(parent).children;
        let index = children.iter().position(|&c| c == top).expect("child");
        children.swap(0, index);
        true
    }

    // ------------------------------------------------------------------------ side to play

    /// `Board.changeNextTurn` / `setupSetSideToPlay`. A snapshot records it as its `PL`; the
    /// node's analysis no longer applies (`clearbestmoves`, `invalidateSetupAnalysis`). Choosing
    /// the side that already plays changes nothing, as in Java.
    fn set_to_play(&mut self, node: NodeId, color: Stone) -> Applied {
        let n = self.node_mut(node);
        if n.to_play == color {
            return Applied::unchanged(None);
        }
        n.to_play = color;
        let snapshot = n.kind == NodeKind::Snapshot;
        if snapshot {
            n.explicit_to_play = Some(color);
        }
        let dropped_analysis = self.restamp_position(node);
        Applied {
            changed: true,
            content: snapshot || dropped_analysis,
            node: None,
        }
    }

    /// What `id`'s position means changed in place: it gets a new stamp, and the analysis saved
    /// with it (`LZ*`) is dropped. Returns whether saved analysis was dropped.
    fn restamp_position(&mut self, id: NodeId) -> bool {
        self.restamp(id);
        let node = self.node_mut(id);
        let before = node.properties.len();
        node.properties = without_analysis(std::mem::take(&mut node.properties));
        node.properties.len() != before
    }

    /// Every live node's position and side to play, from one walk of the tree.
    fn positions(&self) -> HashMap<NodeId, (Position, Stone)> {
        let mut out = HashMap::new();
        let mut stack = vec![(ROOT, Position::new(self.size))];
        while let Some((id, mut pos)) = stack.pop() {
            let node = self.node(id);
            pos.enter(node);
            stack.extend(node.children.iter().map(|&c| (c, pos.clone())));
            out.insert(id, (pos, node.to_play));
        }
        out
    }

    // ------------------------------------------------------------------------ setup

    /// `Board.rootSetupNode`: setup edits change the root only while nothing follows it.
    fn setup_root(&self) -> Result<(), EditError> {
        let root = self.node(ROOT);
        if root.kind != NodeKind::Snapshot || !root.children.is_empty() {
            return Err(EditError::SetupRequiresRootOnly);
        }
        Ok(())
    }

    /// `setupPlaceStone` (`Some`) / `setupEraseStone` (`None`): no capture, ko or suicide rules,
    /// no move number.
    fn setup_stone(&mut self, point: Point, stone: Option<Stone>) -> Applied {
        if !self.size.contains(point) || self.position(ROOT).at(point) == stone {
            return Applied::unchanged(None);
        }
        let setup = &mut self.node_mut(ROOT).setup;
        setup.add_black.retain(|&q| q != point);
        setup.add_white.retain(|&q| q != point);
        setup.clear.retain(|&q| q != point);
        match stone {
            Some(Stone::Black) => setup.add_black.push(point),
            Some(Stone::White) => setup.add_white.push(point),
            None => {}
        }
        self.restamp_position(ROOT);
        Applied::content(None)
    }

    /// `convertCurrentPositionToStartingPosition`: the stones and side to play of `node` become
    /// the root setup; every move and variation is discarded. Game information, the root comment,
    /// marks and other root properties stay.
    fn convert_to_start(&mut self, node: NodeId) -> Applied {
        let pos = self.position(node);
        let to_play = self.node(node).to_play;
        let (mut black, mut white) = (Vec::new(), Vec::new());
        for y in 0..self.size.height {
            for x in 0..self.size.width {
                let p = Point { x, y };
                match pos.at(p) {
                    Some(Stone::Black) => black.push(p),
                    Some(Stone::White) => white.push(p),
                    None => {}
                }
            }
        }
        for c in std::mem::take(&mut self.node_mut(ROOT).children) {
            self.mark_removed(c);
        }
        let root = self.node_mut(ROOT);
        root.setup = Setup {
            add_black: black,
            add_white: white,
            clear: Vec::new(),
        };
        root.to_play = to_play;
        root.explicit_to_play = Some(to_play);
        root.properties = without_analysis(std::mem::take(&mut root.properties));
        self.has_setup_or_handicap = !self.node(ROOT).setup.is_empty();
        self.restamp(ROOT);
        Applied::content(Some(ROOT))
    }

    // ------------------------------------------------------------------------ transforms

    /// Exchange colours, rotate or mirror the whole record, keeping its structure, comments and
    /// game information; marks and setup move with the stones. Analysis stays with the nodes whose
    /// position and side to play come out the same (the empty board, stones on the centre).
    fn transform(&mut self, t: Transform) -> Result<Applied, EditError> {
        let (w, h) = (self.size.width, self.size.height);
        if matches!(t, Transform::RotateLeft | Transform::RotateRight) && w != h {
            return Err(EditError::NotSquare);
        }
        let map = |p: Point| match t {
            Transform::ExchangeColors => p,
            Transform::RotateRight => Point { x: w - 1 - p.y, y: p.x },
            Transform::RotateLeft => Point { x: p.y, y: h - 1 - p.x },
            Transform::MirrorVertical => Point { x: w - 1 - p.x, y: p.y },
            Transform::MirrorHorizontal => Point { x: p.x, y: h - 1 - p.y },
        };
        let swap = |c: Stone| {
            if t == Transform::ExchangeColors {
                c.opposite()
            } else {
                c
            }
        };
        let before = self.positions();
        let live: Vec<NodeId> = before.keys().copied().collect();
        for &id in &live {
            let n = self.node_mut(id);
            n.kind = match n.kind {
                NodeKind::Move { color, point } => NodeKind::Move {
                    color: swap(color),
                    point: map(point),
                },
                NodeKind::Pass { color } => NodeKind::Pass { color: swap(color) },
                other => other,
            };
            let s = &mut n.setup;
            for p in s
                .add_black
                .iter_mut()
                .chain(s.add_white.iter_mut())
                .chain(s.clear.iter_mut())
            {
                *p = map(*p);
            }
            if t == Transform::ExchangeColors {
                std::mem::swap(&mut s.add_black, &mut s.add_white);
            }
            n.to_play = swap(n.to_play);
            n.explicit_to_play = n.explicit_to_play.map(swap);
            for m in &mut n.marks {
                m.point = map(m.point);
            }
        }
        let after = self.positions();
        for id in live {
            if before[&id] != after[&id] {
                self.restamp_position(id);
            }
        }
        Ok(Applied::content(None))
    }

    // ------------------------------------------------------------------------ markup

    /// `LizzieFrame.tryToMarkup`: a point that carries any mark loses its marks; otherwise the
    /// tool adds its mark (letters continue after the highest letter label, skipping the
    /// characters between `Z` and `a`; numbers after the highest number label).
    fn toggle_mark(&mut self, node: NodeId, point: Point, tool: MarkTool) -> Applied {
        if !self.size.contains(point) {
            return Applied::unchanged(None);
        }
        let marks = &mut self.node_mut(node).marks;
        if marks.iter().any(|m| m.point == point) {
            marks.retain(|m| m.point != point);
            return Applied::content(None);
        }
        let mark = match tool {
            MarkTool::Eraser => return Applied::unchanged(None),
            MarkTool::Circle => MarkKind::Circle,
            MarkTool::Cross => MarkKind::Cross,
            MarkTool::Square => MarkKind::Square,
            MarkTool::Triangle => MarkKind::Triangle,
            MarkTool::Letter => {
                // Java compares the first character of every label (`charAt(0)`).
                let last = marks
                    .iter()
                    .filter_map(|m| match &m.mark {
                        MarkKind::Label(t) => t.chars().next(),
                        _ => None,
                    })
                    .map(u32::from)
                    .fold(u32::from('A') - 1, u32::max);
                let mut next = last + 1;
                if (91..=96).contains(&next) {
                    next = 97;
                }
                MarkKind::Label(char::from_u32(next).map_or_else(String::new, String::from))
            }
            MarkTool::Number => {
                let last = marks
                    .iter()
                    .filter_map(|m| match &m.mark {
                        MarkKind::Label(t) => t.parse::<i32>().ok(),
                        _ => None,
                    })
                    .fold(0, i32::max);
                MarkKind::Label(last.wrapping_add(1).to_string())
            }
        };
        marks.push(Mark { point, mark });
        Applied::content(None)
    }
}

impl Position {
    /// The position after `color` plays at `p` (`Board.place`): `None` when the point is taken or
    /// the move is a suicide the rules do not allow (a single stone never; more stones only with
    /// `allow_suicide`, removing them). Without capture nothing is removed.
    pub fn try_play(&self, color: Stone, p: Point, rules: PlayRules) -> Option<Position> {
        if self.at(p).is_some() {
            return None;
        }
        let mut next = self.clone();
        if rules.no_capture {
            next.set(p, Some(color));
            return Some(next);
        }
        let suicided = next.play(color, p);
        if suicided == 1 || (suicided > 1 && !rules.allow_suicide) {
            return None;
        }
        Some(next)
    }
}

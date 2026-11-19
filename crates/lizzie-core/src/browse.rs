//! Browsing operations over one record: where each navigation command lands.
//!
//! Every function maps the current node to the target node without changing the record.
//! Behaviour follows the fixed Java build (`rules/Board.java`, `BoardHistoryNode.java`,
//! `gui/Input.java`, `LizzieFrame.java`); dummy PASS placeholders are never landed on
//! (`BoardHistoryNode.next(false)` hides an end dummy, the variation tree skips them).

use serde::Deserialize;
use ts_rs::TS;

use crate::record::{GameRecord, NodeId, NodeKind, ROOT};

/// A browsing command. UI menus, toolbar buttons, shortcuts and acceptance automation all send
/// these same values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, serde::Serialize, TS)]
#[serde(tag = "op", content = "count")]
pub enum Navigation {
    /// Down / wheel down / `>` (`navigateHistory(+n)`), `>>` and Ctrl+Down / PageDown use 10.
    Forward(u32),
    /// Up / wheel up / `<`, `<<` and Ctrl+Up / PageUp use 10.
    Back(u32),
    /// Home / `|<` (`firstMove`).
    First,
    /// End / `>|` (`lastMove`): end of the current line through first children.
    Last,
    /// Right (`Board.nextBranch`): next node at the same depth.
    NextBranch,
    /// Left (`Board.previousBranch`): previous node at the same depth, main line end first.
    PreviousBranch,
    /// Ctrl+Left (`Input.undoToFirstParentWithVariations`).
    BranchPoint,
    /// Shift+Up (`Input.undoToChildOfPreviousWithVariation`).
    BranchStart,
    /// Ctrl+Shift+Down (`goCommentNode(true)`).
    NextComment,
    /// Ctrl+Shift+Up (`goCommentNode(false)`).
    PreviousComment,
    /// B / "返回主分支" (`LizzieFrame.moveToMainTrunk`).
    MainTrunk,
}

/// Next node along the line, hiding a dummy placeholder (`BoardHistoryNode.next(false)`).
fn next(rec: &GameRecord, id: NodeId) -> Option<NodeId> {
    rec.main_child(id).filter(|&c| rec.node(c).kind != NodeKind::DummyPass)
}

fn visible_children(rec: &GameRecord, id: NodeId) -> impl Iterator<Item = NodeId> + '_ {
    rec.node(id)
        .children
        .iter()
        .copied()
        .filter(|&c| rec.node(c).kind != NodeKind::DummyPass)
}

fn has_variations(rec: &GameRecord, id: NodeId) -> bool {
    rec.node(id).children.len() > 1
}

fn forward(rec: &GameRecord, mut id: NodeId, n: u32) -> NodeId {
    for _ in 0..n {
        match next(rec, id) {
            Some(c) => id = c,
            None => break,
        }
    }
    id
}

fn back(rec: &GameRecord, mut id: NodeId, n: u32) -> NodeId {
    for _ in 0..n {
        match rec.parent(id) {
            Some(p) => id = p,
            None => break,
        }
    }
    id
}

/// Move number at the end of the line below `id`.
fn line_end_move_number(rec: &GameRecord, id: NodeId) -> u32 {
    let mut end = id;
    while let Some(c) = next(rec, end) {
        end = c;
    }
    rec.node(end).move_number
}

/// `Board.branchCandidates`: all nodes at the current depth in tree order, with the end of the
/// main trunk in front when the first of them is not on the main trunk.
fn branch_candidates(rec: &GameRecord, id: NodeId) -> Vec<NodeId> {
    let depth = rec.depth(id);
    let mut level = vec![ROOT];
    for _ in 0..depth {
        level = level.iter().flat_map(|&n| visible_children(rec, n)).collect();
    }
    if level.first().is_none_or(|&f| !rec.is_main_trunk(f)) {
        let mut end = ROOT;
        while let Some(c) = next(rec, end) {
            end = c;
        }
        level.insert(0, end);
    }
    level
}

/// `BoardHistoryNode.findChildOfPreviousWithVariation`.
fn child_of_previous_with_variation(rec: &GameRecord, id: NodeId) -> Option<NodeId> {
    let mut node = id;
    while let Some(p) = rec.parent(node) {
        if has_variations(rec, p) {
            return Some(node);
        }
        node = p;
    }
    None
}

/// `Board.undoToChildOfPreviousWithVariation`: the junction child, unless already there.
fn undo_to_child_of_previous_with_variation(rec: &GameRecord, id: NodeId) -> Option<NodeId> {
    child_of_previous_with_variation(rec, id).filter(|&g| g != id)
}

/// Target of a browsing command from `current`.
pub fn navigate(rec: &GameRecord, current: NodeId, nav: Navigation) -> NodeId {
    match nav {
        Navigation::Forward(n) => forward(rec, current, n),
        Navigation::Back(n) => back(rec, current, n),
        Navigation::First => ROOT,
        Navigation::Last => forward(rec, current, u32::MAX),
        Navigation::NextBranch => {
            let c = branch_candidates(rec, current);
            c.iter()
                .position(|&n| n == current)
                .and_then(|i| c.get(i + 1).copied())
                .unwrap_or(current)
        }
        Navigation::PreviousBranch => {
            let mut target = current;
            for n in branch_candidates(rec, current) {
                if n == current {
                    break;
                }
                target = n;
            }
            target
        }
        Navigation::BranchPoint => match undo_to_child_of_previous_with_variation(rec, current) {
            Some(child) => back(rec, child, 1),
            None => current,
        },
        Navigation::BranchStart => match undo_to_child_of_previous_with_variation(rec, current) {
            Some(child) => child,
            None => back(rec, current, 1),
        },
        Navigation::NextComment => {
            // `goToNextNodeWithComment` counts to the end when no later node has a comment.
            let mut node = current;
            let mut moves = 0;
            while let Some(c) = next(rec, node) {
                moves += 1;
                if !rec.node(c).comment.is_empty() {
                    break;
                }
                node = c;
            }
            forward(rec, current, moves)
        }
        Navigation::PreviousComment => {
            let mut node = current;
            let mut moves = 0;
            while let Some(p) = rec.parent(node) {
                moves += 1;
                if !rec.node(p).comment.is_empty() {
                    break;
                }
                node = p;
            }
            back(rec, current, moves)
        }
        Navigation::MainTrunk => {
            let mut node = current;
            while !rec.is_main_trunk(node) {
                node = back(rec, node, 1);
            }
            node
        }
    }
}

/// Move-number jump (bottom bar field + "跳转", `Board.goToMoveNumberBeyondBranch`): when the
/// current branch is shorter than the number but the main trunk is long enough, start from the
/// root; then walk the line until the node shows that move number. Setup snapshots are nodes
/// but not moves, so the walk counts move numbers rather than nodes (Java steps one node per
/// move and lands on the wrong move once a snapshot lies in between).
pub fn goto_move_number(rec: &GameRecord, current: NodeId, number: u32) -> NodeId {
    let mut node = current;
    if number > line_end_move_number(rec, current) && number <= line_end_move_number(rec, ROOT) {
        node = ROOT;
    }
    while rec.node(node).move_number < number {
        match next(rec, node) {
            Some(c) => node = c,
            None => break,
        }
    }
    while rec.node(node).move_number > number {
        match rec.parent(node) {
            Some(p) => node = p,
            None => break,
        }
    }
    node
}

/// Whether `id` is a node the user can land on.
pub fn is_browsable(rec: &GameRecord, id: NodeId) -> bool {
    rec.contains(id) && rec.node(id).kind != NodeKind::DummyPass
}

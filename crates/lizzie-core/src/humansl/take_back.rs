//! 悔棋 in an AI 陪练 game (Java `HumanSlGameController.takeBack`, `GameTreePosition.takeBackMoves`,
//! `CoachTakenBackLine`): how many moves a take-back withdraws, and the game's memory of the
//! continuations it withdrew. A take-back only steps the game's position back; the withdrawn
//! moves stay in the record, a move played again follows its withdrawn node, and a different move
//! takes the slot the game's line had, the older line staying beside it.
use crate::record::{GameRecord, NodeId, NodeKind};
use std::collections::HashMap;

/// How many moves a take-back withdraws: on the player's turn the player's last move and the AI's
/// reply, while the AI has not answered only the player's move, and never a move from before the
/// game started (`GameTreePosition.takeBackMoves`).
pub fn moves(human_to_play: bool, moves_since_start: usize) -> usize {
    match (human_to_play, moves_since_start) {
        (true, n) if n >= 2 => 2,
        (false, n) if n >= 1 => 1,
        _ => 0,
    }
}

/// The continuations withdrawn in this game, by the node they continue (`CoachTakenBackLine`).
#[derive(Default)]
pub struct TakenBack {
    retained: HashMap<NodeId, Continuations>,
}

#[derive(Default)]
struct Continuations {
    moves: Vec<NodeId>,
    /// The slot the game's line had among the node's continuations.
    active: usize,
}

impl TakenBack {
    /// Remembers the `moves` nodes from `live` up as withdrawn (`capture`).
    pub fn capture(&mut self, record: &GameRecord, live: NodeId, moves: usize) {
        let mut current = live;
        for _ in 0..moves {
            let Some(parent) = record.parent(current) else { return };
            self.remember(record, parent, current);
            current = parent;
        }
    }

    /// The withdrawn continuation of `at` that is the move `kind`, while it is still there
    /// (`replayMatching`). Other equal continuations, such as a record's own, are not replayed.
    pub fn replay(&self, record: &GameRecord, at: NodeId, kind: NodeKind) -> Option<NodeId> {
        let children = &record.node(at).children;
        self.retained
            .get(&at)?
            .moves
            .iter()
            .copied()
            .find(|node| children.contains(node) && record.node(*node).kind == kind)
    }

    /// The game went on from `before` to `after` (`resumeWith`): the slot `after` belongs in when
    /// a continuation of `before` was withdrawn and `after` is not in the game line's slot. The
    /// next move of a replayed older attempt is remembered too, so a different reply takes its
    /// slot while the older continuation stays.
    pub fn resume(&mut self, record: &GameRecord, before: NodeId, after: NodeId) -> Option<usize> {
        if record.parent(after) != Some(before) {
            return None;
        }
        let children = &record.node(before).children;
        let slot = self.retained.get(&before).and_then(|choices| {
            let current = children.iter().position(|&c| c == after)?;
            let active = choices.active.min(children.len() - 1);
            (current != active).then_some(active)
        });
        if let Some(&next) = record.node(after).children.first() {
            self.remember(record, after, next);
        }
        slot
    }

    fn remember(&mut self, record: &GameRecord, parent: NodeId, node: NodeId) {
        let choices = self.retained.entry(parent).or_default();
        if let Some(index) = record.node(parent).children.iter().position(|&c| c == node) {
            choices.active = index;
        }
        if !choices.moves.contains(&node) {
            choices.moves.push(node);
        }
    }
}

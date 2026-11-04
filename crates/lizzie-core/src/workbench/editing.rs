//! Application flow for document editing: which node an edit applies to and where browsing goes
//! afterwards, confirmations, the delete/edit undo cache (`Board.saveListForEdit` / `cleanedit` /
//! `reedit`), starting-position setup mode, new boards, and the effects of an edit on stored
//! analysis and the foreground engine. The document content itself changes only through
//! [`Workbench::apply_edit`] → [`GameRecord::apply`].

use std::sync::Arc;

use super::Workbench;
use crate::api::{ApiError, CommandResult, EditCommand, EditTarget, EditingView, ErrorKind};
use crate::browse::{self, Navigation};
use crate::record::{
    Applied, BoardSize, DocumentEdit, EditError, GameRecord, NodeId, NodeKind, PlayRules, Point, Stone, ROOT,
};
use crate::settings::EditSettings;

/// `LizzieFrame.setupEditRequiresRootOnly`.
pub const SETUP_ROOT_ONLY: &str = "LizzieFrame.setupEditRequiresRootOnly";
/// `LizzieFrame.setupModeRequiresRootOnly`.
pub const SETUP_MODE_ROOT_ONLY: &str = "LizzieFrame.setupModeRequiresRootOnly";
/// `LizzieFrame.setupModeEngineNotReady`: setup mode needs the foreground engine closed.
pub const SETUP_ENGINE_RUNNING: &str = "LizzieFrame.setupModeEngineNotReady";
/// `SpinAndMirror.noneSquareError`.
pub const NOT_SQUARE: &str = "SpinAndMirror.noneSquareError";

/// A record and browsing position kept for 撤销删除 / 恢复删除.
#[derive(Clone)]
pub(super) struct EditState {
    record: Arc<GameRecord>,
    cursor: NodeId,
}

/// Delete/edit undo cache, setup mode and 试下; reset when another record is adopted.
#[derive(Clone, Default)]
pub(super) struct Editing {
    setup_mode: bool,
    before: Option<EditState>,
    after: Option<EditState>,
    /// The record and position kept while 试下 is on.
    trial: Option<EditState>,
}

fn refused(key: &str) -> Result<CommandResult, ApiError> {
    Ok(CommandResult::Refused {
        message_key: key.to_string(),
    })
}

fn done(changed: bool) -> Result<CommandResult, ApiError> {
    Ok(if changed {
        CommandResult::Done
    } else {
        CommandResult::Unchanged
    })
}

impl Workbench {
    // ------------------------------------------------------------------------ write path

    /// Applies `edit` to the open document: the one content write path. The revision moves on
    /// when saved content changed; stored analysis of removed or edited nodes is dropped.
    pub(super) fn apply_edit(&mut self, edit: DocumentEdit) -> Result<Applied, EditError> {
        let record = Arc::make_mut(&mut self.doc.record);
        record.set_branch_numbering(self.config.display.branch_numbers_from_one);
        let applied = record.apply(&edit)?;
        if applied.content {
            self.doc.revision += 1;
        }
        if applied.changed {
            self.retain_valid_analysis();
        }
        Ok(applied)
    }

    /// The content-write entry for callers outside editing (komi, tests): returns whether the
    /// record changed.
    pub fn edit_document(&mut self, edit: DocumentEdit) -> Result<bool, ApiError> {
        self.apply_edit(edit)
            .map(|a| a.changed)
            .map_err(|e| ApiError::new(ErrorKind::InvalidArgument, e.to_string()))
    }

    /// Another record was adopted or a new board started: setup mode ends and the undo cache is
    /// emptied (`setHistory`, `clearBoardState` → `cleanedittemp`).
    pub(super) fn reset_editing(&mut self) {
        self.editing = Editing::default();
    }

    pub fn setup_mode(&self) -> bool {
        self.editing.setup_mode
    }

    /// The question a confirmation answers: this document, revision and current node.
    fn edit_target(&self) -> EditTarget {
        EditTarget {
            document: self.doc.id,
            revision: self.doc.revision,
            node: self.cursor,
        }
    }

    /// The answer to give instead of running a destructive edit: `None` runs it. Without a
    /// confirmation it asks first when `needed`; a confirmation for another document, revision
    /// or node is stale and does nothing.
    fn confirmation(&self, needed: bool, confirmed: Option<EditTarget>) -> Option<CommandResult> {
        match confirmed {
            Some(target) if target != self.edit_target() => Some(CommandResult::Unchanged),
            None if needed => Some(CommandResult::NeedsEditConfirmation {
                target: self.edit_target(),
            }),
            _ => None,
        }
    }

    pub(super) fn editing_view(&self) -> EditingView {
        EditingView {
            setup_mode: self.editing.setup_mode,
            can_undo: self.editing.before.is_some(),
            can_redo: self.editing.after.is_some(),
            trying: self.editing.trial.is_some(),
        }
    }

    /// The editing settings. Turning `no-capture` on or off leaves existing positions as they
    /// were placed (Java keeps each node's board); only later moves, reads and the variation
    /// previews follow it.
    pub fn set_edit_settings(&mut self, settings: EditSettings) -> Result<bool, ApiError> {
        if self.config.edit == settings {
            return Ok(false);
        }
        let rules_changed = self.config.edit.no_capture != settings.no_capture;
        self.commit_config(|c| c.edit = settings)?;
        if rules_changed {
            self.refresh_pv_captures();
        }
        Ok(true)
    }

    /// `saveListForEdit`: the record before a delete, insert or stone edit; redo is forgotten.
    fn remember_for_undo(&mut self) {
        self.editing.before = Some(EditState {
            record: self.doc.record.clone(),
            cursor: self.cursor,
        });
        self.editing.after = None;
    }

    pub(super) fn play_rules(&self) -> PlayRules {
        PlayRules {
            allow_suicide: self
                .engines
                .foreground()
                .is_some_and(|fg| self.engines.allows_suicide(fg)),
            no_capture: self.config.edit.no_capture,
        }
    }

    /// Browsing moves to `node` after an edit, silently (bulk edits mute Java's stone sound).
    fn land(&mut self, node: NodeId) {
        self.cursor = node;
        self.sounds.clear();
        self.sync_engine();
    }

    // ------------------------------------------------------------------------ commands

    /// Runs one editing command; during a game only the human's moves and annotations go
    /// through, and a human move asks the engine for its answer.
    pub fn edit(&mut self, op: EditCommand) -> Result<CommandResult, ApiError> {
        if let Some(refusal) = self.play_edit_refusal(&op) {
            return Ok(refusal);
        }
        let human = self.game_active() && matches!(op, EditCommand::Play { .. } | EditCommand::Pass);
        let before = self.cursor;
        let result = self.run_edit(op)?;
        if human && self.cursor != before {
            self.play_after_human();
        }
        Ok(result)
    }

    fn run_edit(&mut self, op: EditCommand) -> Result<CommandResult, ApiError> {
        match op {
            EditCommand::Play { point, color } => {
                if self.editing.setup_mode {
                    return done(false);
                }
                done(self.play(point, color))
            }
            EditCommand::Pass => {
                if self.editing.setup_mode {
                    return done(false);
                }
                let color = self.doc.record.node(self.cursor).to_play;
                let applied = self.edit_op(DocumentEdit::Pass { at: self.cursor, color })?;
                Ok(self.follow(applied))
            }
            EditCommand::Insert { point, color } => {
                let before = self.doc.record.clone();
                let rules = self.play_rules();
                let applied = self.edit_op(DocumentEdit::Insert {
                    at: self.cursor,
                    color,
                    point,
                    rules,
                })?;
                self.after_rebuild(before, applied)
            }
            EditCommand::EditStone { point, change } => {
                let before = self.doc.record.clone();
                let rules = self.play_rules();
                let applied = self.edit_op(DocumentEdit::EditStone {
                    at: self.cursor,
                    point,
                    change,
                    rules,
                })?;
                self.after_rebuild(before, applied)
            }
            EditCommand::DeleteMove { confirmed } => self.delete(self.cursor, confirmed),
            EditCommand::DeleteMoveNoHint => {
                let target = self.edit_target();
                self.delete(self.cursor, Some(target))
            }
            EditCommand::DeleteBranch { confirmed } => {
                // `undoToChildOfPreviousWithVariation`, then `deleteMove` there.
                let target = browse::navigate(&self.doc.record, self.cursor, Navigation::BranchStart);
                self.delete(target, confirmed)
            }
            EditCommand::MoveBranch { up } => {
                let applied = self.edit_op(DocumentEdit::MoveBranch { node: self.cursor, up })?;
                done(applied.changed)
            }
            EditCommand::SetAsMain => {
                let applied = self.edit_op(DocumentEdit::SetAsMain { node: self.cursor })?;
                done(applied.changed)
            }
            EditCommand::UndoEdit => done(self.restore(true)),
            EditCommand::RedoEdit => done(self.restore(false)),
            EditCommand::ClearEditCache => {
                let had = self.editing.before.is_some() || self.editing.after.is_some();
                self.editing.before = None;
                self.editing.after = None;
                done(had)
            }
            EditCommand::ChangeTurn => {
                let can_add_player = self
                    .engines
                    .foreground()
                    .is_some_and(|fg| self.engines.can_add_player(fg));
                if can_add_player {
                    let color = self.doc.record.node(self.cursor).to_play.opposite();
                    let applied = self.edit_op(DocumentEdit::SetToPlay {
                        node: self.cursor,
                        color,
                    })?;
                    if applied.changed {
                        self.sync_engine();
                    }
                    done(applied.changed)
                } else {
                    self.edit(EditCommand::Pass)
                }
            }
            EditCommand::PlayBestMove => {
                let best = self
                    .current_analysis()
                    .and_then(|a| a.candidates.into_iter().min_by_key(|c| c.order))
                    .and_then(|c| c.point);
                match best {
                    Some(point) => self.edit(EditCommand::Play { point, color: None }),
                    None => done(false),
                }
            }
            EditCommand::PlayVariation { moves } => {
                let mut sounds = Vec::new();
                let mut changed = false;
                for point in moves {
                    changed |= self.play(point, None);
                    sounds.append(&mut self.sounds);
                }
                self.sounds = sounds;
                done(changed)
            }
            EditCommand::AddVariationAsBranch { moves } => {
                let rec = &self.doc.record;
                if !rec.is_main_trunk(self.cursor) && rec.node(self.cursor).children.is_empty() {
                    return self.edit(EditCommand::PlayVariation { moves });
                }
                let rules = self.play_rules();
                let applied = self.edit_op(DocumentEdit::AddBranch {
                    at: self.cursor,
                    moves,
                    rules,
                })?;
                done(applied.changed)
            }
            EditCommand::TryPlay => self.try_play(),
            EditCommand::ContinueLadder => {
                let mut sounds = Vec::new();
                let mut changed = false;
                while let Some(point) = self.ladder_continuation() {
                    if !self.play(point, None) {
                        break;
                    }
                    changed = true;
                    sounds.append(&mut self.sounds);
                }
                self.sounds = sounds;
                done(changed)
            }
            EditCommand::FindMove { point, any_branch } => match self.find_move(point, any_branch) {
                Some(node) if node != self.cursor => {
                    self.cursor = node;
                    self.sync_engine();
                    done(true)
                }
                _ => done(false),
            },
            EditCommand::SetComment { node, comment } => {
                let applied = self.edit_op(DocumentEdit::SetComment { node, comment })?;
                done(applied.changed)
            }
            EditCommand::ToggleMark { point, tool } => {
                let applied = self.edit_op(DocumentEdit::ToggleMark {
                    node: self.cursor,
                    point,
                    tool,
                })?;
                done(applied.changed)
            }
            EditCommand::ClearMarks => {
                let applied = self.edit_op(DocumentEdit::ClearMarks { node: self.cursor })?;
                done(applied.changed)
            }
            EditCommand::SetGameInfo { black, white, komi } => {
                if self.whole_game_active() {
                    return Ok(CommandResult::Refused { message_key: "WholeGameAnalysis.conflict.analysis".into() });
                }
                if !komi.is_finite() {
                    return Err(ApiError::new(ErrorKind::InvalidArgument, "komi"));
                }
                let names = self.edit_op(DocumentEdit::SetPlayers { black, white })?.changed;
                let komi = if komi != self.komi { self.set_komi(komi)? } else { false };
                done(names || komi)
            }
            EditCommand::NewBoard { confirmed } => {
                let needed = self.config.edit.show_new_board_hint && self.doc.record.has_children_of_root();
                if let Some(answer) = self.confirmation(needed, confirmed) {
                    return Ok(answer);
                }
                self.new_board(self.doc.record.size);
                done(true)
            }
            EditCommand::ClearBoard => {
                self.new_board(self.doc.record.size);
                done(true)
            }
            EditCommand::SetBoardSize { width, height } => {
                if !(2..=52).contains(&width) || !(2..=52).contains(&height) {
                    return Err(ApiError::new(ErrorKind::InvalidArgument, "board size"));
                }
                let size = BoardSize { width, height };
                let standard = width == height && [19, 15, 13, 9].contains(&width);
                if !standard {
                    // `Config.saveOtherBoardSize`.
                    self.commit_config(|c| {
                        c.edit.other_size_width = width;
                        c.edit.other_size_height = height;
                    })?;
                }
                if size == self.doc.record.size {
                    return done(!standard);
                }
                self.new_board(size);
                done(true)
            }
            EditCommand::Transform { transform } => match self.apply_edit(DocumentEdit::Transform(transform)) {
                Ok(applied) => {
                    self.sync_engine();
                    done(applied.changed)
                }
                Err(EditError::NotSquare) => refused(NOT_SQUARE),
                Err(e) => Err(ApiError::new(ErrorKind::InvalidArgument, e.to_string())),
            },
            EditCommand::SetSetupMode { enabled, confirmed } => self.set_setup_mode(enabled, confirmed),
            EditCommand::SetupStone { point, stone } => {
                if !self.editing.setup_mode {
                    return done(false);
                }
                self.setup_edit(DocumentEdit::SetupStone { point, stone })
            }
            EditCommand::SetupClearAll => self.setup_edit(DocumentEdit::SetupClearAll),
            EditCommand::SetupSideToPlay { color } => self.setup_edit(DocumentEdit::SetupSideToPlay { color }),
            EditCommand::ConvertToStartingPosition { confirmed } => {
                if let Some(answer) = self.confirmation(self.doc.record.has_actions(), confirmed) {
                    return Ok(answer);
                }
                self.convert_to_start();
                done(true)
            }
        }
    }

    fn edit_op(&mut self, edit: DocumentEdit) -> Result<Applied, ApiError> {
        self.apply_edit(edit)
            .map_err(|e| ApiError::new(ErrorKind::InvalidArgument, e.to_string()))
    }

    /// A placed or followed move becomes the current node with its stone sound.
    fn follow(&mut self, applied: Applied) -> CommandResult {
        match applied.node {
            Some(node) => {
                self.move_to(node);
                CommandResult::Done
            }
            None => CommandResult::Unchanged,
        }
    }

    /// `Board.place` at the current node; returns whether the browsing position or the record
    /// changed.
    fn play(&mut self, point: Point, color: Option<Stone>) -> bool {
        let to_play = self.doc.record.node(self.cursor).to_play;
        let rules = self.play_rules();
        let edit = DocumentEdit::Play {
            at: self.cursor,
            color: color.unwrap_or(to_play),
            point,
            alternate: color.is_none(),
            rules,
        };
        match self.apply_edit(edit) {
            Ok(applied) => self.follow(applied) == CommandResult::Done,
            Err(_) => false,
        }
    }

    /// After an insert or stone edit: the undo cache holds the record before it.
    fn after_rebuild(&mut self, before: Arc<GameRecord>, applied: Applied) -> Result<CommandResult, ApiError> {
        if !applied.changed {
            return done(false);
        }
        self.editing.before = Some(EditState {
            record: before,
            cursor: self.cursor,
        });
        self.editing.after = None;
        let node = applied.node.filter(|&n| self.doc.record.contains(n)).unwrap_or(ROOT);
        self.land(node);
        done(true)
    }

    /// `Board.deleteMove` of `node`: asks while something follows it; the root clears the board.
    fn delete(&mut self, node: NodeId, confirmed: Option<EditTarget>) -> Result<CommandResult, ApiError> {
        if let Some(answer) = self.confirmation(!self.doc.record.node(node).children.is_empty(), confirmed) {
            return Ok(answer);
        }
        if node == ROOT {
            self.new_board(self.doc.record.size);
            return done(true);
        }
        self.remember_for_undo();
        let applied = self.edit_op(DocumentEdit::Remove { node })?;
        if !self.doc.record.contains(self.cursor) {
            self.land(applied.node.unwrap_or(ROOT));
        } else {
            self.sync_engine();
        }
        done(true)
    }

    /// `cleanedit` (`undo`) / `reedit`: swaps the open record with the kept one; the record being
    /// left becomes the other direction's.
    fn restore(&mut self, undo: bool) -> bool {
        let slot = if undo {
            self.editing.before.take()
        } else {
            self.editing.after.take()
        };
        let Some(kept) = slot else { return false };
        let leaving = EditState {
            record: self.doc.record.clone(),
            cursor: self.cursor,
        };
        if undo {
            self.editing.after = Some(leaving);
        } else {
            self.editing.before = Some(leaving);
        }
        self.reinstate(kept);
        true
    }

    /// Opens a kept record of this document again at its kept position. Ids and stamps stay unique
    /// across the switch, so analysis and engine state of the record being left cannot attach to
    /// it. Its komi becomes the game komi again, as Java's reload of the kept SGF sets it.
    fn reinstate(&mut self, kept: EditState) {
        let mut record = (*kept.record).clone();
        record.resume_after(&self.doc.record);
        self.doc.record = Arc::new(record);
        self.doc.revision += 1;
        let komi = self.doc.record.info.komi;
        if komi != self.komi {
            self.komi = komi;
            self.reset_analysis();
            self.limit_stop = None;
        }
        self.retain_valid_analysis();
        let cursor = Some(kept.cursor)
            .filter(|&n| self.doc.record.contains(n))
            .unwrap_or(ROOT);
        self.land(cursor);
    }

    /// `LizzieFrame.tryPlay`: starting keeps the record and position and removes what follows the
    /// current node (`deleteMoveNoHintAfter`); ending restores both. When nothing changed the
    /// record meanwhile it is still the kept one (every edit copies a shared record), so only the
    /// position comes back and the document stays at its revision.
    fn try_play(&mut self) -> Result<CommandResult, ApiError> {
        if let Some(kept) = self.editing.trial.take() {
            if Arc::ptr_eq(&kept.record, &self.doc.record) {
                self.cursor = kept.cursor;
                self.sync_engine();
            } else {
                self.reinstate(kept);
            }
            return done(true);
        }
        self.editing.trial = Some(EditState {
            record: self.doc.record.clone(),
            cursor: self.cursor,
        });
        let children = self.doc.record.node(self.cursor).children.clone();
        for node in children {
            self.edit_op(DocumentEdit::Remove { node })?;
        }
        self.sync_engine();
        done(true)
    }

    /// `Board.clear` / `reopen`: a new empty board of `size`. The komi is the foreground engine's
    /// own (`Leelaz.orikomi`) when one runs, else the current one.
    fn new_board(&mut self, size: BoardSize) {
        if let Some(komi) = self
            .engines
            .foreground()
            .and_then(|fg| self.engines.profile_of(fg))
            .map(|p| p.komi)
        {
            self.komi = komi;
        }
        self.komi_changed = false;
        self.start_komi = None;
        self.limit_stop = None;
        self.reopen_empty_board(size);
        self.sync_engine();
    }

    fn convert_to_start(&mut self) {
        let applied = self.apply_edit(DocumentEdit::ConvertToStartingPosition { node: self.cursor });
        self.editing.setup_mode = false;
        if applied.is_ok() {
            self.land(ROOT);
        }
    }

    fn set_setup_mode(&mut self, enabled: bool, confirmed: Option<EditTarget>) -> Result<CommandResult, ApiError> {
        if !enabled {
            let was = std::mem::replace(&mut self.editing.setup_mode, false);
            if was {
                self.sync_engine();
            }
            return done(was);
        }
        if self.editing.setup_mode {
            return done(false);
        }
        if self.engines.foreground().is_some() {
            return refused(SETUP_ENGINE_RUNNING);
        }
        if let Some(answer) = self.confirmation(self.doc.record.has_actions(), confirmed) {
            return Ok(answer);
        }
        if self.doc.record.has_actions() {
            self.convert_to_start();
        }
        if self.doc.record.has_children_of_root() {
            return refused(SETUP_MODE_ROOT_ONLY);
        }
        self.cursor = ROOT;
        self.editing.setup_mode = true;
        done(true)
    }

    fn setup_edit(&mut self, edit: DocumentEdit) -> Result<CommandResult, ApiError> {
        match self.apply_edit(edit) {
            Ok(applied) => {
                if applied.changed {
                    self.sync_engine();
                }
                done(applied.changed)
            }
            Err(EditError::SetupRequiresRootOnly) => refused(SETUP_ROOT_ONLY),
            Err(e) => Err(ApiError::new(ErrorKind::InvalidArgument, e.to_string())),
        }
    }

    // ------------------------------------------------------------------------ queries

    /// `Board.continueLadderByOne`: the last five moves on the line form a ladder (the fifth back
    /// is one diagonal step from the current one); the next ladder move is free and so are the two
    /// points that keep it running.
    fn ladder_continuation(&self) -> Option<Point> {
        let rec = &self.doc.record;
        let mut moves = Vec::with_capacity(5);
        let mut node = self.cursor;
        for _ in 0..5 {
            let NodeKind::Move { point, .. } = rec.node(node).kind else {
                return None;
            };
            moves.push((i32::from(point.x), i32::from(point.y)));
            node = rec.parent(node)?;
        }
        let (dx, dy) = (moves[0].0 - moves[4].0, moves[0].1 - moves[4].1);
        if dx.abs() != 1 || dy.abs() != 1 {
            return None;
        }
        let (x, y) = (moves[3].0 + dx, moves[3].1 + dy);
        let pos = rec.position(self.cursor);
        let empty = |x: i32, y: i32| {
            let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) else {
                return false;
            };
            let p = Point { x, y };
            rec.size.contains(p) && pos.at(p).is_none()
        };
        (empty(x, y) && empty(x + dx, y) && empty(x, y + dy)).then_some(Point {
            x: x as u16,
            y: y as u16,
        })
    }

    /// `Board.gotoAnyMoveByCoords` / `findMove`: the current node, its ancestors, the main
    /// continuation below it, then (`any_branch`) every branch from the root.
    fn find_move(&self, point: Point, any_branch: bool) -> Option<NodeId> {
        let rec = &self.doc.record;
        let plays = |n: NodeId| matches!(rec.node(n).kind, NodeKind::Move { point: p, .. } if p == point);
        if let Some(n) = rec.path_to(self.cursor).into_iter().rev().find(|&n| plays(n)) {
            return Some(n);
        }
        let mut n = self.cursor;
        while let Some(c) = rec.main_child(n) {
            if plays(c) {
                return Some(c);
            }
            n = c;
        }
        if !any_branch {
            return None;
        }
        let mut stack = vec![ROOT];
        while let Some(n) = stack.pop() {
            if plays(n) {
                return Some(n);
            }
            stack.extend(rec.node(n).children.iter().rev());
        }
        None
    }
}

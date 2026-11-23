//! Web 旁观 (`WebBoardManager`): which share session runs, what its viewers see, and a viewer's
//! 试下. The platform runs the page and board stream; it reports the session it started and
//! hands each viewer message over with that session, so a message from a stopped share never
//! reaches a later one.
//!
//! A 试下 lets one viewer play variations from the position shown when it started (the anchor).
//! Its moves are added to the open record as side branches of the anchor, so they are kept and
//! saved like any other variation; a position without continuation first gets an end
//! placeholder, taken out again when the 试下 ends without moves (`cleanupMainlineDummy`). While
//! it runs, the desktop and every viewer show the viewer's position and the foreground engine
//! analyses it (`EngineFollowController`); desktop edits and browsing are held off and modes
//! that would take the engine are refused (`isWebTrialEngineBusy`). It ends at the owner's
//! request, after five idle minutes, by 强制结束试下, when Web 旁观 stops, or when another record
//! is opened; browsing then returns to the anchor.

use std::time::Instant;

use super::Workbench;
use crate::api::{
    ApiError, Command, CommandResult, EngineScope, GameCommand, HawkeyeCommand, HumanSlCommand, MatchCommand,
    TuningCommand, WebBoardCommand, WebBoardView,
};
use crate::engine::runtime::LeaseAvailability;
use crate::record::{DocumentEdit, NodeId, NodeKind, Point, Position};
use crate::web_board::{
    self as protocol, BoardState, ClientMessage, HistoryEntry, SiblingMarker, TrialDenied, TrialState, TRIAL_IDLE,
};

/// `LizzieFrame.webTrialDesktopBlocked`: a desktop edit during a viewer's 试下.
pub const WEB_TRIAL_BLOCKED: &str = "LizzieFrame.webTrialDesktopBlocked";
/// `AnalysisSettings.reuseStatus.application_exclusive_mode`: a mode that needs the engine.
const ENGINE_EXCLUSIVE: &str = "AnalysisSettings.reuseStatus.application_exclusive_mode";
/// `AnalysisSettings.reuseStatus.existing_lease`: Java's notice when an engine switch or restart
/// cannot take its lifecycle reservation (`EngineManager.showForegroundEngineLeaseInUse`).
pub(super) const ENGINE_LEASED: &str = "AnalysisSettings.reuseStatus.existing_lease";

#[derive(Default)]
pub(super) struct WebBoardFlow {
    /// The running share: the platform's session number and the access address.
    session: Option<(u64, String)>,
    trial: Option<Trial>,
}

struct Trial {
    owner: String,
    /// The document and position it started from.
    doc: u64,
    anchor: NodeId,
    /// Positions of the record that got an end placeholder, so a 试下 move under a position
    /// without continuation stays a branch (`mainlineDummy`, for every such position).
    placeholders: Vec<NodeId>,
    /// Positions this 试下 added; their continuations are its own.
    created: Vec<NodeId>,
    /// When it ends unless its owner acts.
    deadline: Instant,
}

/// What the viewers are sent: the board (resent whole when `board` changes), its analysis and
/// graph, and the 试下 state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WebBoardFrame {
    /// Document, revision, shown node and its position stamp.
    pub board: (u64, u64, NodeId, u64),
    pub full_state: String,
    pub history: String,
    /// The shown position's analysis while it has candidates.
    pub analysis: Option<String>,
    pub trial: String,
}

impl Workbench {
    // -------------------------------------------------------------------- session

    /// The platform started Web 旁观 as `session`, reachable at `url`.
    pub fn web_board_started(&mut self, session: u64, url: String) {
        self.end_web_trial();
        self.web_board.session = Some((session, url));
    }

    /// Web 旁观 `session` stopped (`WebBoardManager.stop`): its 试下 ends. Returns whether it
    /// was the running session.
    pub fn web_board_stopped(&mut self, session: u64) -> bool {
        if self.web_board.session.as_ref().is_none_or(|(s, _)| *s != session) {
            return false;
        }
        self.web_board.session = None;
        self.end_web_trial();
        true
    }

    /// The running share's session.
    pub fn web_board_session(&self) -> Option<u64> {
        self.web_board.session.as_ref().map(|(s, _)| *s)
    }

    pub(super) fn web_board_view(&self) -> WebBoardView {
        WebBoardView {
            url: self.web_board.session.as_ref().map(|(_, url)| url.clone()),
            trial_active: self.web_board.trial.is_some(),
        }
    }

    /// The commands the core runs itself; starting and stopping belong to the platform.
    pub fn web_board(&mut self, op: WebBoardCommand) -> Result<CommandResult, ApiError> {
        Ok(match op {
            WebBoardCommand::ForceExitTrial if self.web_board.session.is_some() && self.end_web_trial() => {
                CommandResult::Done
            }
            _ => CommandResult::Unchanged,
        })
    }

    // -------------------------------------------------------------------- viewers

    /// Runs a viewer's message of `session`; the answer for that viewer alone, if any
    /// (`trial_denied`). Messages of a session that no longer runs are ignored.
    pub fn web_board_message(&mut self, session: u64, message: ClientMessage, now: Instant) -> Option<String> {
        if self.web_board_session() != Some(session) {
            return None;
        }
        match message {
            ClientMessage::EnterTrial { client } => {
                self.enter_web_trial(client, now).map(|d| protocol::trial_denied(&d))
            }
            ClientMessage::ExitTrial { client } => {
                if self.owns_web_trial(&client) {
                    self.end_web_trial();
                }
                None
            }
            ClientMessage::TrialMove { client, x, y } => {
                if self.owns_web_trial(&client) {
                    self.web_trial_move(x, y);
                    self.touch_web_trial(now);
                }
                None
            }
            ClientMessage::TrialBack { client } => self.web_trial_step(&client, now, |wb, t| {
                (wb.cursor != t.anchor)
                    .then(|| wb.doc.record.parent(wb.cursor))
                    .flatten()
            }),
            ClientMessage::TrialForward { client } => self.web_trial_step(&client, now, |wb, _| {
                let rec = &wb.doc.record;
                rec.node(wb.cursor)
                    .children
                    .iter()
                    .copied()
                    .find(|&c| rec.node(c).kind != NodeKind::DummyPass)
            }),
            ClientMessage::TrialChild { client, index } => self.web_trial_step(&client, now, |wb, _| {
                let rec = &wb.doc.record;
                rec.node(wb.cursor)
                    .children
                    .get(index as usize)
                    .copied()
                    .filter(|&c| rec.node(c).kind != NodeKind::DummyPass)
            }),
            ClientMessage::TrialReset { client } => {
                if self.owns_web_trial(&client) {
                    let anchor = self.web_board.trial.as_ref().map(|t| t.anchor).unwrap_or(self.cursor);
                    self.web_trial_land(anchor);
                    self.touch_web_trial(now);
                }
                None
            }
        }
    }

    fn owns_web_trial(&self, client: &str) -> bool {
        self.web_board
            .trial
            .as_ref()
            .is_some_and(|t| t.owner == client && t.doc == self.doc.id)
    }

    fn touch_web_trial(&mut self, now: Instant) {
        if let Some(t) = self.web_board.trial.as_mut() {
            t.deadline = now + TRIAL_IDLE;
        }
    }

    /// `enterTrialWithResult`: the same viewer again is accepted as is; another viewer is refused
    /// while one runs, and so is any viewer while the foreground engine is missing or held.
    fn enter_web_trial(&mut self, client: String, now: Instant) -> Option<TrialDenied> {
        if let Some(t) = &self.web_board.trial {
            return (t.owner != client).then(|| TrialDenied::InUse { owner: t.owner.clone() });
        }
        if !self.web_trial_engine_free() {
            return Some(TrialDenied::EngineBusy);
        }
        let anchor = self.cursor;
        self.web_board.trial = Some(Trial {
            owner: client,
            doc: self.doc.id,
            anchor,
            placeholders: Vec::new(),
            created: Vec::new(),
            deadline: now + TRIAL_IDLE,
        });
        self.guard_main_line(anchor);
        self.engines.set_application_exclusive(true);
        None
    }

    /// The foreground engine is there, done starting, and not held by a game, a borrowed
    /// search, an unrestored state or a mode that moves it (`beginEngineModeReservation`,
    /// `desktopPlayingProbe`). An engine still starting is a switch or restart that has not
    /// completed: Java's lifecycle reservation refuses the 试下 until it has. That holds while GMA
    /// is armed as well, whose availability (`ReadBoardGma`) answers before the engine's start-up.
    /// A secondary engine still starting holds it too: its restart or replacement reserves the
    /// foreground engine until the board sync (`reStartEngine2`, EngineManager.java:4723-4742;
    /// `reserveEngineLifecycle`, 14393-14432).
    /// A Fox session owns the board from the moment it listens, before it takes the engine; the
    /// desktop gate (`fox_gate`) refuses changes then, and so does this entry (Java admits the 试下
    /// there).
    fn web_trial_engine_free(&self) -> bool {
        // Missing or still starting: `foreground_complete` (availability answers `ReadBoardGma`
        // before `EngineNotReady`).
        let held = matches!(
            self.engines.lease_availability(),
            LeaseAvailability::ExistingLease
                | LeaseAvailability::EngineStateUnrestored
                | LeaseAvailability::PlayMode
                | LeaseAvailability::HumanSlGame
                | LeaseAvailability::ApplicationExclusive
        );
        // A GMA hand or its restore holds `readBoardGmaReservation` (Leelaz.java:15201-15205).
        !held
            && self.engines.foreground_complete()
            && self.engines.secondary().is_none_or(|id| self.engines.complete(id))
            && !self.readboard.gma.reserved()
            && !self.fox_owns_board()
            && !self.fox_holds_engine()
            && !self.game_active()
            && !self.match_active()
            && !self.human_sl_active()
            && !self.auto_analysis_active()
    }

    /// A viewer's 试下 holds the foreground engine (`isWebTrialEngineBusy`).
    pub(super) fn web_trial_active(&self) -> bool {
        self.web_board.trial.is_some()
    }

    /// Gives `at` an end placeholder when it has no continuation and was there before the 试下,
    /// so a move played there becomes a branch, never the record's continuation.
    fn guard_main_line(&mut self, at: NodeId) {
        let Some(trial) = self.web_board.trial.as_ref() else {
            return;
        };
        if trial.created.contains(&at) || !self.doc.record.node(at).children.is_empty() {
            return;
        }
        let added = self
            .apply_edit(DocumentEdit::EndPlaceholder { at, present: true })
            .is_ok_and(|a| a.changed);
        if let (true, Some(trial)) = (added, self.web_board.trial.as_mut()) {
            trial.placeholders.push(at);
        }
    }

    /// `doApplyMove`: a stone on the shown position, of the side to play. An equal continuation
    /// is followed; otherwise a legal stone starts a new last variation.
    fn web_trial_move(&mut self, x: u32, y: u32) {
        let (Ok(x), Ok(y)) = (u16::try_from(x), u16::try_from(y)) else {
            return;
        };
        let point = Point { x, y };
        if !self.doc.record.size.contains(point) {
            return;
        }
        let at = self.cursor;
        let rec = &self.doc.record;
        let same = rec
            .node(at)
            .children
            .iter()
            .copied()
            .find(|&c| matches!(rec.node(c).kind, NodeKind::Move { point: p, .. } if p == point));
        let node = match same {
            Some(node) => Some(node),
            None => {
                self.guard_main_line(at);
                let edit = DocumentEdit::PlayBranch {
                    at,
                    color: self.doc.record.node(at).to_play,
                    point,
                    rules: self.play_rules(),
                };
                let node = self.apply_edit(edit).ok().and_then(|a| a.node);
                if let (Some(node), Some(trial)) = (node, self.web_board.trial.as_mut()) {
                    trial.created.push(node);
                }
                node
            }
        };
        if let Some(node) = node {
            self.web_trial_land(node);
        }
    }

    /// A browsing step of the owner's 试下 (`trialNavigate*`); the idle time restarts only when
    /// it moves.
    fn web_trial_step(
        &mut self,
        client: &str,
        now: Instant,
        target: impl FnOnce(&Workbench, &Trial) -> Option<NodeId>,
    ) -> Option<String> {
        if !self.owns_web_trial(client) {
            return None;
        }
        let trial = self.web_board.trial.as_ref()?;
        if let Some(node) = target(self, trial) {
            self.web_trial_land(node);
            self.touch_web_trial(now);
        }
        None
    }

    /// The 试下 shows `node`; the foreground engine follows it.
    fn web_trial_land(&mut self, node: NodeId) {
        if node == self.cursor {
            return;
        }
        self.cursor = node;
        self.sounds.clear();
        self.sync_engine();
        self.quick_navigated();
    }

    /// Ends a running 试下 (`exitTrial` / `forceExitTrial`): a placeholder it added goes where
    /// nothing was played beside it (`cleanupMainlineDummy`), browsing returns to the anchor and
    /// the engine follows; GMA parameters that could not be given back during the 试下 go back
    /// now. Returns whether one ran.
    fn end_web_trial(&mut self) -> bool {
        let Some(t) = self.web_board.trial.take() else {
            return false;
        };
        self.engines.set_application_exclusive(false);
        if t.doc == self.doc.id && self.doc.record.contains(t.anchor) {
            for at in t.placeholders.into_iter().rev() {
                if self.doc.record.contains(at) {
                    let _ = self.apply_edit(DocumentEdit::EndPlaceholder { at, present: false });
                }
            }
            self.web_trial_land(t.anchor);
        }
        self.gma_restore_deferred();
        true
    }

    /// Another record was adopted or a new board started: a 试下 of the old record ends.
    pub(super) fn web_board_document_replaced(&mut self) {
        if self.web_board.trial.as_ref().is_some_and(|t| t.doc != self.doc.id) {
            self.end_web_trial();
        }
    }

    // -------------------------------------------------------------------- desktop

    /// What a desktop command does instead while a viewer's 试下 runs: edits are refused with
    /// Java's hint (`showTrialBlockedHint`), browsing is held off, modes that would take the
    /// foreground engine are refused as application-exclusive, and restarting or replacing the
    /// secondary engine as already in exclusive use (closing it runs). `None` lets it run.
    pub(super) fn web_trial_gate(&self, command: &Command) -> Option<CommandResult> {
        self.web_board.trial.as_ref()?;
        let refused = |key: &str| {
            Some(CommandResult::Refused {
                message_key: key.into(),
            })
        };
        match command {
            // A desktop move or record change; 设置贴目 writes the record's komi.
            Command::Edit { .. } | Command::SetKomi { .. } => refused(WEB_TRIAL_BLOCKED),
            Command::Navigate { .. }
            | Command::GotoNode { .. }
            | Command::GotoMoveNumber { .. }
            | Command::JumpToProblemMove { .. }
            | Command::Hawkeye {
                op:
                    HawkeyeCommand::GotoRow { .. } | HawkeyeCommand::GotoMove { .. } | HawkeyeCommand::GotoMoveNumber { .. },
            } => Some(CommandResult::Unchanged),
            Command::Game {
                op: GameCommand::Start { .. } | GameCommand::Continue { .. },
            }
            | Command::Match {
                op: MatchCommand::Start { .. } | MatchCommand::StartToolbar,
            }
            | Command::HumanSl {
                op: HumanSlCommand::Start { .. },
            }
            | Command::StartFlashAnalysis { .. }
            | Command::RedoFlashAnalysis
            | Command::StartAutoAnalysis { .. }
            | Command::StartWholeGameAnalysis { .. }
            | Command::ResumeWholeGameAnalysis
            | Command::Tracking { .. }
            | Command::Batch { .. }
            | Command::Tuning {
                op: TuningCommand::StartBenchmark { .. },
            }
            // The console reaches the foreground engine and the record (undo, komi, play);
            // closing, restarting or replacing the foreground engine would end its following
            // (Java refuses the lifecycle reservation while a 试下 runs).
            | Command::SendGtpCommand { .. }
            | Command::SwitchEngine { .. }
            | Command::RestartEngine
            | Command::ShutdownEngines {
                scope: EngineScope::Current | EngineScope::All,
            }
            | Command::SaveEngineSettings { .. } => refused(ENGINE_EXCLUSIVE),
            // The secondary engine's restart and replacement need Java's lifecycle reservation as
            // well (`reStartEngine2`, `switchEngine(index, false)`) and get its notice; the
            // operations check again for requests that do not pass here
            // (`secondary_lifecycle_held`). Closing it takes none (`killThisEngines2`).
            Command::SwitchSecondaryEngine { .. } | Command::RestartSecondaryEngine => refused(ENGINE_LEASED),
            _ => None,
        }
    }

    // -------------------------------------------------------------------- timer

    pub(super) fn web_trial_deadline(&self) -> Option<Instant> {
        self.web_board.trial.as_ref().map(|t| t.deadline)
    }

    /// The idle time ran out (`scheduleIdleTimeout` → `forceExitTrial`).
    pub(super) fn web_trial_tick(&mut self, now: Instant) -> bool {
        self.web_board.trial.as_ref().is_some_and(|t| now >= t.deadline) && self.end_web_trial()
    }

    // -------------------------------------------------------------------- board stream

    /// What the viewers see now: the shown position (the 试下's while one runs), its analysis,
    /// Black's values from the root to it, and the 试下 state.
    pub fn web_board_frame(&self) -> WebBoardFrame {
        let rec = &self.doc.record;
        let node = self.cursor;
        let n = rec.node(node);
        let position: Position = rec.position(node);
        let state = BoardState {
            size: rec.size,
            stones: &position.stones,
            last_move: match n.kind {
                NodeKind::Move { point, .. } => Some(point),
                _ => None,
            },
            move_number: n.move_number,
            to_play: n.to_play,
            analysis: self.store.get(rec, node),
            ownership: self.store.ownership(rec, node).map(Vec::as_slice),
        };
        let path: Vec<HistoryEntry> = rec
            .path_to(node)
            .into_iter()
            .map(|id| {
                let n = rec.node(id);
                HistoryEntry {
                    move_number: n.move_number,
                    to_play: n.to_play,
                    analysis: self.store.get(rec, id),
                }
            })
            .collect();
        let trial = self.web_board.trial.as_ref().filter(|t| t.doc == self.doc.id).map(|t| {
            let children = &n.children;
            let dummy = |c: NodeId| rec.node(c).kind == NodeKind::DummyPass;
            // The main continuation is not marked; under a placeholder every move is a branch.
            let first = usize::from(children.first().is_some_and(|&c| !dummy(c)));
            let markers = children
                .iter()
                .enumerate()
                .skip(first)
                .filter_map(|(i, &c)| match rec.node(c).kind {
                    NodeKind::Move { point, .. } => Some(SiblingMarker {
                        point,
                        child_index: i as u32,
                    }),
                    _ => None,
                })
                .collect();
            TrialState {
                owner: t.owner.clone(),
                anchor_move_number: rec.node(t.anchor).move_number,
                display_move_number: n.move_number,
                can_back: node != t.anchor,
                can_forward: children.iter().any(|&c| !dummy(c)),
                markers,
            }
        });
        WebBoardFrame {
            board: (self.doc.id, self.doc.revision, node, n.stamp),
            full_state: protocol::full_state(&state),
            history: protocol::winrate_history(&path),
            analysis: protocol::analysis_update(&state),
            trial: protocol::trial_state(trial.as_ref()),
        }
    }
}

//! 棋盘同步工具 (ReadBoard) in the application flow: the helper process of the current launch,
//! its session ([`crate::readboard::session`]) applied to the open record, the float board,
//! hosted helper updates, the helper's logging and GMA ([`gma`]).
//!
//! Every helper launch has a new [`HelperId`]; output of an earlier helper (a late line, its end)
//! is dropped, so a replaced or stopped helper can never change the record or the engine of its
//! successor; the successor starts only once the replaced helper's process has exited, and an
//! update being installed owns the restart. The session reaches the record only through
//! [`SyncTarget`], whose writes are ordinary record edits; while it runs, those edits are not
//! reported back to it as local moves.

mod gma;

use std::path::PathBuf;
use std::time::{Instant, SystemTime};

use crate::api::{
    Command, CommandResult, EditCommand, FloatBoardView, FloatSceneView, GameCommand, HelperLoggingView, HumanSlCommand,
    MatchCommand, ReadBoardCommand, ReadBoardNotice, ReadBoardView, TuningCommand,
};
use crate::diagnostics::{DiagnosticModule, TraceScope};
use crate::engine::runtime::{InstanceId, Phase};
use crate::readboard::diagnostics::{self, ReadBoardDiagnostics, SyncDiagnostics, SyncState};
use crate::readboard::launch::{self, LaunchParams};
use crate::readboard::logging::{self, LoggingControl, LoggingLaunch, LoggingSnapshot};
use crate::readboard::protocol::FloatBoardGeometry;
use crate::readboard::session::{
    next_excluding_end_placeholder, AutoPlayParam, FloatBoardEvent, HostContext, PlaceGate, ReadBoardSession,
    RebuiltSnapshot, SessionEvent, SessionSettings, SyncBoard,
};
use crate::readboard::update::{self, UpdateRequest};
use crate::readboard::{HelperHost, HelperId, HelperInput};
use crate::record::{Applied, BoardSize, DocumentEdit, GameRecord, NodeId, NodeKind, Point, Stone, ROOT};
use crate::settings::Language;
use crate::workbench::Workbench;

pub(super) use gma::GmaFlow;
use gma::{GmaFacts, GmaNotice};

/// One helper process and what belongs to it alone.
struct Helper {
    id: HelperId,
    session: ReadBoardSession,
    logging: LoggingControl,
}

/// The float board window's state (`FloatBoard`; the helper places it, the user toggles the rest).
#[derive(Default)]
struct FloatBoard {
    geometry: Option<FloatBoardGeometry>,
    visible: bool,
    edit_mode: bool,
    suggestions_hidden: bool,
    /// The window was hidden for a `place` that waits until the client confirms it is off the
    /// screen (`FloatBoardHidden` with this number).
    hide_request: Option<u64>,
}

/// A helper update the user is asked about.
struct UpdateOffer {
    helper: HelperId,
    request: UpdateRequest,
}

/// A confirmed helper update: the platform installs it off the UI thread once the helper it
/// replaces has exited ([`Workbench::take_readboard_install`]), then reports the result
/// ([`Workbench::finish_readboard_install`]).
pub struct ReadBoardInstall {
    request: UpdateRequest,
    dir: PathBuf,
}

impl ReadBoardInstall {
    /// Installs the package into the helper directory (blocking file work).
    pub fn run(&self) -> Result<(), String> {
        update::install(&self.request, &self.dir).map_err(|e| e.message().to_string())
    }
}

/// A helper start that waits for something to finish first (Java `queueReadBoardRestart`); an
/// `Open` meanwhile is this same start.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PendingStart {
    /// The replaced helper's process has not exited yet: Java starts the successor only after
    /// the old helper's `shutdown` finished, so the two never click the target at once.
    AfterExit(HelperId),
    /// An accepted update is being installed: its restart is the only start
    /// (`restartReadBoardAfterHostedUpdate`).
    AfterInstall,
}

#[derive(Default)]
pub(super) struct ReadBoardState {
    host: Option<Box<dyn HelperHost>>,
    base_dirs: Vec<PathBuf>,
    next_helper: HelperId,
    helper: Option<Helper>,
    pending: Option<PendingStart>,
    /// A helper whose output ended by itself while its process was still exiting: an `Open`
    /// before [`Workbench::readboard_exited`] reports it waits for it like a replaced helper.
    exiting: Option<HelperId>,
    /// The session is changing the record or the view: those changes are its own.
    applying: bool,
    /// While a session call runs (its helper is taken out): whether that session synced the
    /// board when the call began, for [`Workbench::readboard_syncing`].
    session_syncing: bool,
    /// The application log session id given to helpers (Java `LoggingRuntime` session).
    host_session_id: String,
    pub(super) gma: GmaFlow,
    float: FloatBoard,
    /// The last [`FloatBoard::hide_request`] number; never reused, so a confirmation meant for an
    /// earlier hide or an earlier helper's float board matches nothing.
    float_hide_seq: u64,
    update: Option<UpdateOffer>,
    install: Option<ReadBoardInstall>,
    /// A normal (non-GMA) auto-play line waits until GMA gave the engine back
    /// (`normalAutoPlayIntent`).
    normal_intent: bool,
    /// The analysis was stopped because the helper asked for it (not the user's own pause).
    stopped_analysis: bool,
    notices: Vec<ReadBoardNotice>,
    /// Recent protocol lines and sync decisions of this run's helpers, for the diagnostics package.
    diagnostics: SyncDiagnostics,
}

fn refused(key: &str) -> CommandResult {
    CommandResult::Refused { message_key: key.into() }
}

fn message(key: &str, args: Vec<String>) -> ReadBoardNotice {
    ReadBoardNotice::Message { key: key.into(), args }
}

/// Java `ReadBoard.language`: the helper knows `cn`, `en`, `jp` and `kr`. (Java's Thai resource
/// sends a mistranslated word the helper does not know; English is what it then shows.)
fn helper_language(language: Language) -> &'static str {
    match language {
        Language::ZhCn | Language::ZhTw | Language::System => "cn",
        Language::JaJp => "jp",
        Language::KoKr => "kr",
        Language::EnUs | Language::ThTh => "en",
    }
}

/// The record as the session sees it: reads of the open document, writes as record edits. While
/// 野狐 AI 直连 owns the board every write is dropped, as Java's `Board` drops the helper thread's
/// `place` / `addStone` / `reopen` / `clear` / `setHistory` (`FoxLiveSession.rejectOrdinaryMutation`).
struct SyncTarget<'a> {
    wb: &'a mut Workbench,
}

impl SyncBoard for SyncTarget<'_> {
    fn record(&self) -> &GameRecord {
        &self.wb.doc.record
    }

    fn cursor(&self) -> NodeId {
        self.wb.cursor
    }

    fn place_for_sync(&mut self, color: Stone, point: Point) -> Option<NodeId> {
        if self.wb.fox_owns_board() {
            return None;
        }
        let at = self.main_end();
        let rules = self.wb.play_rules();
        let node = self.wb.apply_edit(DocumentEdit::Play { at, color, point, alternate: false, rules }).ok()?.node?;
        if self.wb.doc.record.main_child(at) != Some(node) {
            let _ = self.wb.apply_edit(DocumentEdit::SetAsMain { node });
        }
        self.wb.move_to(node);
        Some(node)
    }

    fn rebuild(&mut self, snapshot: RebuiltSnapshot) -> NodeId {
        if self.wb.fox_owns_board() {
            return ROOT;
        }
        let (comment, properties) =
            snapshot.metadata.map(|m| (m.comment, m.properties)).unwrap_or_default();
        let edit = DocumentEdit::SyncSnapshot {
            stones: snapshot.stones,
            to_play: snapshot.to_play,
            explicit_to_play: snapshot.explicit_to_play,
            move_number: snapshot.move_number,
            black_captures: snapshot.black_captures,
            white_captures: snapshot.white_captures,
            move_numbers: snapshot.move_numbers,
            comment,
            properties,
        };
        if self.wb.apply_edit(edit).is_ok() {
            self.wb.land(ROOT);
        }
        ROOT
    }

    fn move_to(&mut self, node: NodeId) {
        self.wb.move_to(node);
    }

    fn discard_failed_move(&mut self, node: NodeId) -> Option<NodeId> {
        if self.wb.fox_owns_board() || self.main_end() != node {
            return None;
        }
        let parent = self.wb.doc.record.parent(node)?;
        self.wb.apply_edit(DocumentEdit::Remove { node }).ok()?;
        if !self.wb.doc.record.contains(self.wb.cursor) || self.wb.cursor == node {
            self.wb.land(parent);
        } else {
            self.wb.move_to(parent);
        }
        Some(parent)
    }

    fn clear_board(&mut self) {
        if !self.wb.fox_owns_board() {
            let size = self.wb.doc.record.size;
            self.wb.new_board(size);
        }
    }

    fn reopen(&mut self, width: u8, height: u8) {
        if !self.wb.fox_owns_board() {
            self.wb.new_board(BoardSize { width: width.into(), height: height.into() });
        }
    }

    fn change_next_turn(&mut self) {
        if self.wb.fox_owns_board() {
            return;
        }
        if let Err(e) = self.wb.run_edit(EditCommand::ChangeTurn) {
            self.wb.log(true, format!("readboard: changing the side to play failed: {}", e.message));
        }
    }
}

impl Workbench {
    // -------------------------------------------------------------------- platform

    /// Gives the core the platform's helper processes and the directories `readboard/` may be in
    /// (absolute, in order of preference: working directory, program directory, resources).
    /// Without a host (other systems) 棋盘同步工具 is unavailable.
    pub fn set_readboard_host(&mut self, host: Box<dyn HelperHost>, base_dirs: Vec<PathBuf>) {
        self.readboard.host = Some(host);
        self.readboard.base_dirs = base_dirs;
    }

    /// Output of helper `helper`. Returns whether the view changed. Output of a helper that is
    /// no longer the current one is dropped (a stopped helper's exit arrives through
    /// [`Workbench::readboard_exited`] instead).
    pub fn readboard_input(&mut self, helper: HelperId, input: HelperInput) -> bool {
        if self.readboard.helper.as_ref().is_none_or(|h| h.id != helper) {
            return false;
        }
        let ended = matches!(input, HelperInput::Ended { .. });
        if let HelperInput::Line(line) = &input {
            let summary = self.readboard.diagnostics.record_line(line, SystemTime::now());
            // Java `SyncDiagnosticsRecorder.recordProtocolLine` → `traceProtocol`.
            self.diagnostics.trace(TraceScope::ReadBoardYike, || format!("readboard raw protocol={summary}"));
            if logging::is_control_line(line) {
                if let Some(h) = self.readboard.helper.as_mut() {
                    h.logging.on_line(line, SystemTime::now());
                }
                return true;
            }
        }
        if let HelperInput::Ended { error: Some(e) } = &input {
            self.log(true, format!("readboard: reading the helper failed: {e}"));
        }
        let now = Instant::now();
        self.with_session(|session, board, ctx| session.input(board, &input, ctx, now));
        if ended {
            // The process ended by itself: it is reaped (and killed if its output closed early).
            // A start asked for before it is gone waits for it.
            if self.readboard.host.as_mut().is_some_and(|host| host.stop(helper)) {
                self.readboard.exiting = Some(helper);
            }
            self.readboard_ended(helper);
        }
        true
    }

    /// The exit sequence of stopped helper `helper` finished: its process exited or was killed
    /// ([`HelperHost::stop`]). A start that waited for it happens now, once, as a new helper
    /// (Java `finishReadBoardRestart`). Returns whether the view changed.
    pub fn readboard_exited(&mut self, helper: HelperId) -> bool {
        self.readboard.exiting.take_if(|&mut id| id == helper);
        if self.readboard.pending != Some(PendingStart::AfterExit(helper)) {
            return false;
        }
        self.readboard.pending = None;
        self.readboard_launch();
        true
    }

    /// A confirmed helper update to install, once (the platform waits for the old helper's exit,
    /// runs it and reports back).
    pub fn take_readboard_install(&mut self) -> Option<ReadBoardInstall> {
        self.readboard.install.take()
    }

    /// The installer finished: the helper starts again (`ReadBoard.updateInstall*`), unless the
    /// app is exiting ([`Workbench::readboard_shutdown`] dropped the restart).
    pub fn finish_readboard_install(&mut self, install: ReadBoardInstall, result: Result<(), String>) -> bool {
        let version = install.request.version_tag().to_string();
        if let Err(detail) = &result {
            self.log(true, format!("readboard: installing {version} failed: {detail}"));
        }
        if self.readboard.pending != Some(PendingStart::AfterInstall) {
            return false;
        }
        self.readboard.pending = None;
        match result {
            Ok(()) => {
                if self.readboard_launch() {
                    self.readboard.notices.push(message("ReadBoard.updateInstallSucceeded", vec![version]));
                } else {
                    self.readboard.notices.push(message("ReadBoard.updateInstallRestartFailed", vec![version]));
                }
            }
            Err(detail) => {
                self.readboard.notices.push(message("ReadBoard.updateInstallFailed", vec![version, detail]));
                self.readboard_launch();
            }
        }
        true
    }

    /// Application exit: the current helper is asked to quit and stopped, and no start waits any
    /// more; nothing else is touched (the target client is not restored or changed).
    pub fn readboard_shutdown(&mut self) {
        self.readboard.pending = None;
        self.readboard_stop();
    }

    // -------------------------------------------------------------------- commands

    pub fn readboard(&mut self, op: ReadBoardCommand) -> Result<CommandResult, crate::api::ApiError> {
        match op {
            ReadBoardCommand::Open => {
                if self.readboard_helper_dir().is_none() {
                    self.readboard.notices.push(message("FunctionSearch.unavailable.readBoard", Vec::new()));
                    return Ok(CommandResult::Done);
                }
                // A start already waits (the old helper's exit or an update's install): this is it.
                if self.readboard.pending.is_some() {
                    return Ok(CommandResult::Done);
                }
                match self.readboard_stop().or(self.readboard.exiting) {
                    Some(old) => self.readboard.pending = Some(PendingStart::AfterExit(old)),
                    None => {
                        self.readboard_launch();
                    }
                }
                Ok(CommandResult::Done)
            }
            ReadBoardCommand::SetAlwaysSyncBoardStat { enabled } => {
                self.commit_config(|c| c.read_board.always_sync_board_stat = enabled)?;
                self.readboard_settings_changed();
                Ok(CommandResult::Done)
            }
            ReadBoardCommand::SetGetFocus { enabled } => {
                self.commit_config(|c| c.read_board.get_focus = enabled)?;
                self.readboard_settings_changed();
                Ok(CommandResult::Done)
            }
            ReadBoardCommand::SetNoSoundInSync { enabled } => {
                self.commit_config(|c| c.read_board.no_sound_in_sync = enabled)?;
                Ok(CommandResult::Done)
            }
            ReadBoardCommand::AnswerUpdate { accept } => Ok(self.readboard_answer_update(accept)),
            ReadBoardCommand::AnswerPonderingNotice { generation, decision } => {
                self.gma_pondering_notice_answered(generation, decision)?;
                Ok(CommandResult::Done)
            }
            ReadBoardCommand::ReturnFocus => {
                let events = self.readboard.helper.as_ref().map(|h| h.session.return_focus()).unwrap_or_default();
                self.readboard_effects(events);
                Ok(CommandResult::Done)
            }
            ReadBoardCommand::SetFloatEditMode { enabled } => {
                if self.readboard.float.edit_mode == enabled {
                    return Ok(CommandResult::Unchanged);
                }
                self.readboard_set_float_edit_mode(enabled);
                Ok(CommandResult::Done)
            }
            ReadBoardCommand::FloatBoardHidden { request } => {
                if self.readboard.float.hide_request != Some(request) {
                    return Ok(CommandResult::Unchanged);
                }
                self.readboard.float.hide_request = None;
                let now = Instant::now();
                self.with_session(|session, board, ctx| session.float_board_hidden(board, ctx, now));
                Ok(CommandResult::Done)
            }
            ReadBoardCommand::SetFloatSuggestionsHidden { hidden } => {
                if self.readboard.float.suggestions_hidden == hidden {
                    return Ok(CommandResult::Unchanged);
                }
                self.readboard.float.suggestions_hidden = hidden;
                if !hidden {
                    self.readboard_show_main_end();
                }
                Ok(CommandResult::Done)
            }
            ReadBoardCommand::SetHelperLogging { diagnostics, capture, trace } => {
                Ok(if self.readboard_request_logging(diagnostics, capture, trace) {
                    CommandResult::Done
                } else {
                    CommandResult::Unchanged
                })
            }
        }
    }

    /// What a command does instead while GMA holds the engine. Another mode cannot start on it
    /// (`AnalysisSettings.reuseStatus.readboard_gma`, as the analysis lease reports); while a
    /// hand or its parameter restore holds the reservation, switching, restarting, closing or
    /// reconfiguring the engine and raw GTP commands are refused as Java's
    /// `beginEngineModeReservation` / `beginExclusiveGtpLifecycleTransition` refuse them
    /// (`existing_lease`). An engine left unrestored stays switchable and restartable: that is
    /// the only way out of the quarantine.
    pub(super) fn readboard_gate(&self, command: &Command) -> Option<CommandResult> {
        let gma = &self.readboard.gma;
        if !gma.busy() {
            return None;
        }
        match command {
            Command::Game { op: GameCommand::Start { .. } | GameCommand::Continue { .. } }
            | Command::Match { op: MatchCommand::Start { .. } | MatchCommand::StartToolbar }
            | Command::HumanSl { op: HumanSlCommand::Start { .. } } => {
                Some(refused("AnalysisSettings.reuseStatus.readboard_gma"))
            }
            Command::SwitchEngine { .. }
            | Command::RestartEngine
            | Command::ShutdownEngines { .. }
            | Command::SaveEngineSettings { .. }
            | Command::SendGtpCommand { .. }
            | Command::Tuning { op: TuningCommand::StartBenchmark { .. } }
                if gma.reserved() =>
            {
                Some(refused("AnalysisSettings.reuseStatus.existing_lease"))
            }
            _ => None,
        }
    }

    // -------------------------------------------------------------------- record hooks

    /// GMA holds the foreground engine (armed, a hand in flight or its parameters being given
    /// back): the engine follows the position without analysing.
    pub(super) fn readboard_gma_busy(&self) -> bool {
        self.readboard.gma.busy()
    }

    /// The analysis may not make a 自动落子 move (`notifyAutoPlay`, `Leelaz.java:8488-8491`): GMA
    /// is armed, asking or giving the engine back (`isReadBoardGmaEngineBusy`), or the helper's
    /// switch to normal 自动落子 waits for GMA to let go (`isNormalAutoPlayTransitionPending`).
    pub(super) fn readboard_holds_autoplay(&self) -> bool {
        self.readboard.gma.busy() || self.readboard.normal_intent
    }

    /// The helper syncs a board (`LizzieFrame.syncBoard`), also while its session is running.
    pub(super) fn readboard_syncing(&self) -> bool {
        match &self.readboard.helper {
            Some(h) => h.session.sync_board(),
            None => self.readboard.session_syncing,
        }
    }

    /// Before a stone is played at `at` by anything but the session (a click, an engine's move):
    /// Java `Board.place`'s ReadBoard checks. `Some` replaces the edit's result: the move is
    /// dropped while a `place` is unresolved, or it became a variation behind the synced line
    /// (one-way sync at the main end).
    pub(super) fn readboard_before_play(&mut self, at: NodeId, color: Stone, point: Point) -> Option<Applied> {
        if self.readboard.applying {
            return None;
        }
        let mut helper = self.readboard.helper.take()?;
        self.readboard.session_syncing = helper.session.sync_board();
        let gate = helper.session.place_gate(&SyncTarget { wb: self }, point, color, Instant::now());
        self.readboard.session_syncing = false;
        self.readboard.helper = Some(helper);
        match gate {
            PlaceGate::Proceed => None,
            PlaceGate::Suppressed => Some(Applied::default()),
            PlaceGate::Variation => {
                let rules = self.play_rules();
                let before = self.doc.record.node(at).children.clone();
                self.readboard.applying = true;
                let applied = self.apply_edit(DocumentEdit::AddBranch { at, moves: vec![point], rules });
                self.readboard.applying = false;
                let applied = applied.ok()?;
                let rec = &self.doc.record;
                let node = rec.node(at).children.iter().copied().find(|c| !before.contains(c) && rec.node(*c).kind.is_action());
                Some(Applied { node, ..applied })
            }
        }
    }

    /// A stone played at the main end by anything but the session: the helper is asked to click
    /// it (`place x y`) when both boards are synced.
    pub(super) fn readboard_after_play(&mut self, point: Point) {
        if self.readboard.applying {
            return;
        }
        let now = Instant::now();
        self.with_session(|session, board, ctx| session.local_move(board, point, ctx, now));
    }

    /// The user moved the view (Java `onLocalHistoryNavigation`).
    pub(super) fn readboard_navigated(&mut self) {
        if self.readboard.applying {
            return;
        }
        let events = self.readboard.helper.as_mut().map(|h| h.session.local_navigation()).unwrap_or_default();
        self.readboard_effects(events);
    }

    /// The record was replaced by something other than the session (Java `onHistoryOverwritten`).
    pub(super) fn readboard_history_overwritten(&mut self) {
        if self.readboard.applying {
            return;
        }
        let events = self.readboard.helper.as_mut().map(|h| h.session.history_overwritten()).unwrap_or_default();
        self.readboard_effects(events);
    }

    /// The user's Space: a pause of the user's own (not one the helper asked for).
    pub(super) fn readboard_user_toggled_analysis(&mut self) {
        self.readboard.stopped_analysis = false;
    }

    // -------------------------------------------------------------------- engine and timers

    pub(super) fn readboard_deadline(&self) -> Option<Instant> {
        let helper = self.readboard.helper.as_ref();
        [
            self.readboard.gma.deadline(),
            helper.and_then(|h| h.session.next_deadline()),
            helper.and_then(|h| h.logging.next_deadline()),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    pub(super) fn readboard_tick(&mut self, now: Instant) -> bool {
        let mut changed = self.gma_tick(now);
        if let Some(h) = self.readboard.helper.as_mut() {
            changed |= h.logging.tick(now);
        }
        if self.readboard.helper.as_ref().and_then(|h| h.session.next_deadline()).is_some_and(|d| d <= now) {
            changed |= self.with_session(|session, board, ctx| session.tick(board, ctx, now));
        }
        changed | self.readboard_after_engine()
    }

    /// The foreground engine finished starting (`onReadBoardGmaCapabilityReady`).
    pub(super) fn readboard_engine_ready(&mut self, instance: InstanceId) {
        let facts = self.gma_facts();
        self.gma_engine_ready(instance, facts);
    }

    /// After engine events: a settling GMA hand may end, GMA's notices are shown, and a normal
    /// auto-play line that waited for GMA to give the engine back resumes the analysis.
    pub(super) fn readboard_after_engine(&mut self) -> bool {
        let mut changed = self.gma_settle();
        for notice in self.readboard.gma.take_notices() {
            changed = true;
            self.readboard.notices.push(match notice {
                GmaNotice::Unsupported => ReadBoardNotice::GmaUnsupported,
                GmaNotice::Message(key) => message(&key, Vec::new()),
            });
        }
        if self.readboard.normal_intent && !self.readboard.gma.reserved() {
            self.readboard.normal_intent = false;
            self.readboard_resume_analysis();
            changed = true;
        }
        changed
    }

    // -------------------------------------------------------------------- view

    /// The 棋盘同步 part of the diagnostics package (导出诊断包): the sync snapshots, the current
    /// helper's logging state and the version it reported.
    pub fn readboard_diagnostics(&self, app_version: &str) -> ReadBoardDiagnostics {
        let helper = self.readboard.helper.as_ref();
        let session = helper.map(|h| &h.session);
        let state = SyncState {
            attached: helper.is_some(),
            syncing: session.is_some_and(|s| s.sync_board()),
            awaiting_first_sync_frame: session.is_some_and(|s| s.awaiting_first_sync_frame()),
            has_resolved_node: session.is_some_and(|s| s.resolved_node().is_some()),
        };
        let d = &self.readboard.diagnostics;
        ReadBoardDiagnostics {
            helper_version: d.helper_version().map(str::to_string),
            snapshots: d.snapshot_entries(state, app_version, SystemTime::now()),
            observed: diagnostics::observed_json(&helper.map_or_else(LoggingSnapshot::detached, |h| h.logging.snapshot())),
        }
    }

    pub(super) fn readboard_view(&self) -> ReadBoardView {
        let helper = self.readboard.helper.as_ref();
        let both_sync = helper.is_some_and(|h| h.session.both_sync());
        let f = &self.readboard.float;
        let float_board = f.geometry.filter(|_| f.visible && helper.is_some()).map(|g| FloatBoardView {
            x: g.x,
            y: g.y,
            width: g.width,
            height: g.height,
            board_type: g.board_type,
            scale: g.scale,
            edit_mode: f.edit_mode,
            suggestions_hidden: f.suggestions_hidden,
            edit_button: !both_sync,
            clicks_play: both_sync || f.edit_mode,
            // `getCurOrMainEnd(editMode)`: outside the edit mode the overlay is the main line's end.
            scene: (!f.edit_mode).then(|| self.readboard_float_scene()),
        });
        ReadBoardView {
            available: self.readboard_helper_dir().is_some(),
            running: helper.is_some(),
            syncing: helper.is_some_and(|h| h.session.sync_board()),
            both_sync,
            gma: self.readboard.gma.color(),
            float_board,
            float_hide_request: f.hide_request.filter(|_| helper.is_some()),
            update_offer: self.readboard.update.as_ref().map(|u| u.request.version_tag().to_string()),
            pondering_notice: self.readboard.gma.pondering_notice(),
            logging: helper.map(|h| logging_view(h.logging.snapshot())),
            always_sync_board_stat: self.config.read_board.always_sync_board_stat,
            get_focus: self.config.read_board.get_focus,
            no_sound_in_sync: self.config.read_board.no_sound_in_sync,
            notices: self.readboard.notices.clone(),
        }
    }

    pub(super) fn take_readboard_notices(&mut self) -> Vec<ReadBoardNotice> {
        std::mem::take(&mut self.readboard.notices)
    }

    // -------------------------------------------------------------------- the helper

    /// The directory holding a readable `readboard.exe`, when a host exists.
    fn readboard_helper_dir(&self) -> Option<PathBuf> {
        self.readboard.host.as_ref()?;
        let dir = launch::helper_dir(&launch::candidates(&self.readboard.base_dirs));
        launch::available(&dir).then_some(dir)
    }

    /// Java `startEngine`: a new helper with the toolbar's auto-play values. Returns whether it
    /// started (`ReadBoard.loadFailed` otherwise).
    fn readboard_launch(&mut self) -> bool {
        let Some(dir) = self.readboard_helper_dir() else {
            self.readboard.notices.push(message("FunctionSearch.unavailable.readBoard", Vec::new()));
            return false;
        };
        let a = self.persisted.autoplay;
        let field = |enabled: bool, value: i32| (enabled && value > 0).then(|| value.to_string());
        if self.readboard.host_session_id.is_empty() {
            let millis = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_millis());
            self.readboard.host_session_id = format!("lizzieyzy-{millis}-{}", std::process::id());
        }
        let logging = LoggingLaunch {
            log_dir: logging::readboard_log_directory(&self.storage.logs_dir),
            host_session_id: self.readboard.host_session_id.clone(),
            // Java `ReadBoard` launch: the helper follows 诊断记录.
            diagnostics: self.config.logging.diagnostics_enabled,
        };
        let params = LaunchParams {
            time: field(a.time_enabled, a.time_seconds),
            playouts: field(a.playouts_enabled, a.playouts),
            first_playouts: field(a.first_playouts_enabled, a.first_playouts),
            language: helper_language(self.active_language).to_string(),
            logging: Some(logging),
        };
        let spec = launch::launch_spec(&dir, &params);
        self.readboard.next_helper += 1;
        let id = self.readboard.next_helper;
        let Some(host) = self.readboard.host.as_mut() else { return false };
        match host.launch(id, &spec) {
            Ok(()) => {
                self.log(false, format!("readboard: helper {id} started from {}", dir.display()));
                self.readboard.float = FloatBoard::default();
                self.readboard.helper = Some(Helper {
                    id,
                    session: ReadBoardSession::new(self.readboard_settings()),
                    logging: LoggingControl::for_launch(params.logging.as_ref()),
                });
                true
            }
            Err(e) => {
                self.log(true, format!("readboard: starting {} failed: {e}", spec.program.display()));
                self.readboard.notices.push(message("ReadBoard.loadFailed", Vec::new()));
                false
            }
        }
    }

    /// Java `shutdown` + `releaseHostedResources`: `quit`, then the process is stopped. GMA
    /// stops as when the helper leaves; a hand in flight still converges on its own. Returns the
    /// stopped helper when its process is still exiting (its exit reaches
    /// [`Workbench::readboard_exited`]).
    fn readboard_stop(&mut self) -> Option<HelperId> {
        let id = self.readboard.helper.as_ref().map(|h| h.id)?;
        let now = Instant::now();
        self.with_session(|session, board, ctx| session.shutdown(board, ctx, now));
        let exiting = self.readboard.host.as_mut().is_some_and(|host| host.stop(id));
        self.readboard_ended(id);
        exiting.then_some(id)
    }

    /// Helper `id` is gone: nothing of it is used any more.
    fn readboard_ended(&mut self, id: HelperId) {
        let Some(mut helper) = self.readboard.helper.take_if(|h| h.id == id) else { return };
        helper.logging.disconnect();
        self.log(false, format!("readboard: helper {id} ended"));
        self.readboard.float.visible = false;
        self.readboard.float.hide_request = None;
        self.readboard.update.take_if(|u| u.helper == id);
        self.readboard.normal_intent = false;
        self.gma_helper_ended();
    }

    fn readboard_send(&mut self, line: &str) {
        let Some(id) = self.readboard.helper.as_ref().map(|h| h.id) else { return };
        let Some(host) = self.readboard.host.as_mut() else { return };
        if let Err(e) = host.send(id, line) {
            self.log(true, format!("readboard: sending to the helper failed: {e}"));
        }
    }

    fn readboard_settings(&self) -> SessionSettings {
        SessionSettings {
            always_sync_board_stat: self.config.read_board.always_sync_board_stat,
            read_board_get_focus: self.config.read_board.get_focus,
        }
    }

    /// The running helper session follows the 棋盘同步工具 settings now (a menu change or a Java
    /// settings import).
    pub(super) fn readboard_settings_changed(&mut self) {
        let settings = self.readboard_settings();
        if let Some(h) = self.readboard.helper.as_mut() {
            h.session.set_settings(settings);
        }
    }

    /// Runs `f` on the current helper's session with the open record and applies its effects.
    /// Returns whether a helper was there.
    fn with_session(
        &mut self,
        f: impl FnOnce(&mut ReadBoardSession, &mut dyn SyncBoard, &HostContext) -> Vec<SessionEvent>,
    ) -> bool {
        let Some(mut helper) = self.readboard.helper.take() else { return false };
        self.readboard.session_syncing = helper.session.sync_board();
        let ctx = self.readboard_context();
        self.readboard.applying = true;
        let events = f(&mut helper.session, &mut SyncTarget { wb: self }, &ctx);
        self.readboard.session_syncing = false;
        self.readboard.applying = false;
        self.readboard.helper = Some(helper);
        self.readboard_effects(events);
        true
    }

    /// The host facts Java's `ReadBoard` read from globals.
    fn readboard_context(&self) -> HostContext {
        let foreground = self.engines.foreground();
        let (playing_against_engine, player_is_black, analysis_game) = self.play_readboard_facts();
        let user_paused = self.paused && !self.readboard.stopped_analysis;
        let can_resume_analysis = !user_paused
            && foreground.is_some()
            && !self.whole_game_holds_foreground()
            && !self.match_active()
            && !playing_against_engine;
        let gma = &self.readboard.gma;
        HostContext {
            engine_available: foreground.is_some_and(|f| self.engines.phase(f) == Some(Phase::Ready)),
            pondering: foreground.is_some() && !self.paused,
            playing_against_engine,
            player_is_black,
            analysis_game,
            can_resume_analysis,
            can_resume_auto_play_analysis: can_resume_analysis && analysis_game && self.play_autoplay_enabled(),
            normal_auto_play_intent: self.readboard.normal_intent,
            normal_auto_play_draining: self.readboard.normal_intent && gma.reserved(),
            gma_busy: gma.reserved(),
            gma_in_flight: gma.in_flight() || gma.restoring(),
            gma_restore_pending: gma.restoring(),
            can_arm_gma: self.gma_can_arm(),
            no_capture: self.config.edit.no_capture,
            allow_suicide: self.play_rules().allow_suicide,
            float_board_shown: self.readboard.float.visible && self.readboard.float.geometry.is_some(),
            float_edit_mode: self.readboard.float.edit_mode,
        }
    }

    fn gma_facts(&self) -> GmaFacts {
        let session = self.readboard.helper.as_ref().map(|h| &h.session);
        GmaFacts {
            both_sync: session.is_some_and(|s| s.both_sync()),
            turn_trusted: session.is_some_and(|s| s.turn_trusted()),
            failed_move_waits: session.is_some_and(|s| s.failed_local_move_awaiting_observation()),
            ponder: session.is_some_and(|s| s.read_board_ponder()),
        }
    }

    /// Analysis runs (Java `ponder()` / `togglePonder` when idle): the session asks only when
    /// Java would, which already respects a pause of the user's own.
    fn readboard_resume_analysis(&mut self) {
        self.readboard.stopped_analysis = false;
        // Java's `ponder()` also ends a wait for 分析 黑 / 白 (`underPonder`).
        self.side_hold = None;
        if self.paused && self.engines.foreground().is_some() {
            self.paused = false;
            self.limit_stop = None;
            self.secondary_limit_stop = None;
        }
        self.sync_engine();
    }

    /// Java `getHistory().getMainEnd()`.
    fn readboard_main_end(&self) -> NodeId {
        let rec = &self.doc.record;
        let mut node = ROOT;
        while let Some(child) = next_excluding_end_placeholder(rec, node) {
            node = child;
        }
        node
    }

    fn readboard_show_main_end(&mut self) {
        let node = self.readboard_main_end();
        self.move_to(node);
    }

    /// `FloatBoard.changeEetEditMode`: leaving the edit mode shows the main line's end.
    fn readboard_set_float_edit_mode(&mut self, enabled: bool) {
        self.readboard.float.edit_mode = enabled;
        if !enabled {
            self.readboard_show_main_end();
        }
    }

    /// `LizzieFrame.tryPlay`: a shown float board leaves its edit mode first.
    pub(super) fn readboard_before_try_play(&mut self) {
        let f = &self.readboard.float;
        if f.edit_mode && f.visible && f.geometry.is_some() && self.readboard.helper.is_some() {
            self.readboard_set_float_edit_mode(false);
        }
    }

    /// What the float board paints outside its edit mode (`FloatBoardRenderer.prepareSelection`
    /// with `getCurOrMainEnd(false)`): the main line's end, its analysis, marks and Kata形势.
    fn readboard_float_scene(&self) -> FloatSceneView {
        let rec = &self.doc.record;
        let node = self.readboard_main_end();
        let n = rec.node(node);
        // The previous position's estimate until this one has its own (`preEstimateArray`).
        let ownership = self.estimate.state.shown.then(|| {
            self.store.ownership(rec, node).or_else(|| rec.parent(node).and_then(|p| self.store.ownership(rec, p))).cloned()
        });
        FloatSceneView {
            node,
            stones: rec.position(node).stones,
            last_action: super::last_action(n.kind),
            to_play: n.to_play,
            snapshot: n.kind == NodeKind::Snapshot,
            next_moves: super::next_moves(rec, node),
            analysis: self.shown_analysis_at(&self.store, node),
            board: self.overlay_at(&self.store, node),
            ownership: ownership.flatten(),
        }
    }

    /// What the session asked for, in its order.
    fn readboard_effects(&mut self, events: Vec<SessionEvent>) {
        for event in events {
            match event {
                SessionEvent::Send(line) => self.readboard_send(&line),
                SessionEvent::StartAnalysis | SessionEvent::ResumeAnalysis { .. } => self.readboard_resume_analysis(),
                SessionEvent::StopAnalysis => {
                    if !self.paused {
                        self.paused = true;
                        self.readboard.stopped_analysis = true;
                        self.sync_engine();
                    }
                }
                SessionEvent::ToggleAnalysisManual => {
                    if let Err(e) = self.toggle_analysis() {
                        self.log(true, format!("readboard: resuming the analysis failed: {}", e.message));
                    }
                }
                SessionEvent::ResumeCancelled => self.readboard.normal_intent = false,
                SessionEvent::Synced { .. } => {
                    let facts = self.gma_facts();
                    self.gma_synced(facts);
                }
                SessionEvent::Genmove { .. } => self.play_readboard_genmove(),
                SessionEvent::ScheduleGma { .. } | SessionEvent::PassLine => {
                    let facts = self.gma_facts();
                    self.gma_schedule(facts);
                }
                // The engine follows the record's current position; a settling hand ends there.
                SessionEvent::GmaRestoreRequested { .. } => {
                    self.gma_settle();
                }
                SessionEvent::GmaRequestInvalidated { .. } => self.gma_retire_hand(),
                SessionEvent::GmaLeaseConflict => {
                    self.readboard.notices.push(message("AnalysisSettings.reuseStatus.existing_lease", Vec::new()));
                }
                SessionEvent::GmaCleared { .. } => self.gma_clear(),
                // Kept by the session (`read_board_ponder`); a GMA hand reads it when it starts.
                // Turned off between hands, the engine stops pondering at once.
                SessionEvent::PonderPreference { on: false } => self.gma_ponder_off(),
                SessionEvent::PonderPreference { on: true } => {}
                SessionEvent::AutoPlay { color, toolbar_side, time, playouts, first_playouts, gma } => {
                    self.play_readboard_toolbar(toolbar_side, Some(time), Some(playouts), Some(first_playouts));
                    if gma {
                        if let Some(color) = color {
                            self.gma_arm(color, time, playouts);
                        }
                    } else {
                        if self.readboard.gma.color().is_some() {
                            self.gma_clear();
                        }
                        if self.readboard.gma.reserved() {
                            self.readboard.normal_intent = true;
                        } else {
                            self.readboard_resume_analysis();
                        }
                    }
                }
                SessionEvent::AutoPlayParam(p) => match p {
                    AutoPlayParam::Time(t) => {
                        self.play_readboard_toolbar(None, Some(t), None, None);
                        self.gma_limits(Some(t), None);
                    }
                    AutoPlayParam::Playouts(v) => {
                        self.play_readboard_toolbar(None, None, Some(v), None);
                        self.gma_limits(None, Some(v));
                    }
                    AutoPlayParam::FirstPlayouts(v) => self.play_readboard_toolbar(None, None, None, Some(v)),
                },
                SessionEvent::AutoPlayStopped => self.play_readboard_autoplay_off(),
                SessionEvent::StopGame | SessionEvent::StopPlayingAgainstEngine => self.play_readboard_stop_game(),
                // The engine follows the record's position by itself.
                SessionEvent::NormalAutoPlayTargetChanged { .. } => {}
                SessionEvent::FloatBoard(e) => self.readboard_float(e),
                SessionEvent::VersionTooOld => {
                    self.readboard.notices.push(message("ReadBoard.versionCheckFaied", Vec::new()));
                }
                SessionEvent::ConsoleLine(line) => self.log(false, format!("readboard: {line}")),
                SessionEvent::UpdateReady(line) => self.readboard_update_ready(&line),
                SessionEvent::LoggingLine(line) => {
                    if let Some(h) = self.readboard.helper.as_mut() {
                        h.logging.on_line(&line, SystemTime::now());
                    }
                }
                SessionEvent::LoggingReady => {
                    if let Some(h) = self.readboard.helper.as_mut() {
                        h.logging.on_ready(Instant::now());
                    }
                }
                // The 弈客 web window's sync (`handleYikeSyncStartCommand` /
                // `handleYikeSyncStopCommand`): `yikeSyncStart` starts from the page's current
                // address, `syncPlatform yike` only makes sure it is followed.
                SessionEvent::YikeSyncStart { from_platform_line } => self.online_helper_yike_start(!from_platform_line),
                SessionEvent::YikeSyncStop => self.online_browser_stop_sync(),
                SessionEvent::LocalMoveConfirmed { .. } => {}
                SessionEvent::LocalMoveFailed { reason, .. } => {
                    // Java `ReadBoardObservation.recordLocalMove("failed", reason)`.
                    self.diagnostics.debug(DiagnosticModule::ReadBoardYike, || {
                        format!("readboard event=local-move outcome=failed reason={reason}")
                    });
                }
                SessionEvent::Decision(d) => {
                    self.readboard.diagnostics.record_decision(d, SystemTime::now());
                    // Java `ReadBoardObservation.recordDecision`.
                    let result = diagnostics::outcome_name(d.outcome);
                    self.diagnostics.debug(DiagnosticModule::ReadBoardYike, || {
                        format!("readboard event=decision result={result} reason={}", d.reason)
                    });
                    self.diagnostics.trace(TraceScope::ReadBoardYike, || {
                        format!("readboard raw decision result={result} reason={}", d.reason)
                    });
                }
            }
        }
    }

    fn readboard_float(&mut self, event: FloatBoardEvent) {
        let f = &mut self.readboard.float;
        match event {
            FloatBoardEvent::Show(g) => {
                if g.width <= 0 || g.height <= 0 {
                    f.visible = false;
                    return;
                }
                f.geometry = Some(g);
                f.visible = true;
                // `setPos` shows hidden suggestions again (`toggleHide`), at the main end.
                if std::mem::take(&mut f.suggestions_hidden) {
                    self.readboard_show_main_end();
                }
            }
            FloatBoardEvent::Hide | FloatBoardEvent::HideAndForget => f.visible = false,
            FloatBoardEvent::HideForPlace { place_waits } => {
                f.visible = false;
                if place_waits {
                    self.readboard.float_hide_seq += 1;
                    f.hide_request = Some(self.readboard.float_hide_seq);
                }
            }
            FloatBoardEvent::RestoreAfterPlace => {
                f.visible = f.geometry.is_some();
                f.hide_request = None;
            }
            FloatBoardEvent::SetEditButton => {}
        }
    }

    // -------------------------------------------------------------------- hosted update

    /// `readboardUpdateReady`: a valid package is offered to the user; anything else is
    /// refused to the helper.
    fn readboard_update_ready(&mut self, line: &str) {
        let Some(helper) = self.readboard.helper.as_ref().map(|h| h.id) else { return };
        let Some(request) = UpdateRequest::try_parse(line) else {
            self.readboard_send(&update::failed_line("invalid readboard update request"));
            return;
        };
        if let Err(e) = update::validate(&request) {
            self.readboard_send(&update::failed_line(e.message()));
            return;
        }
        self.readboard.update = Some(UpdateOffer { helper, request });
    }

    fn readboard_answer_update(&mut self, accept: bool) -> CommandResult {
        let Some(offer) = self.readboard.update.take() else { return CommandResult::Unchanged };
        // Java tells only the offering helper (`updateInstallNoLongerActive`) and shows nothing; an
        // offer lives only while its helper is current, and no restart or install is queued then.
        if self.readboard.helper.as_ref().is_none_or(|h| h.id != offer.helper) {
            return CommandResult::Done;
        }
        if !accept {
            self.readboard_send(update::CANCELLED_LINE);
            return CommandResult::Done;
        }
        let Some(dir) = self.readboard_helper_dir() else {
            self.readboard_send(&update::failed_line("readboard directory is not available"));
            return CommandResult::Done;
        };
        self.readboard_send(update::INSTALLING_LINE);
        self.readboard_stop();
        self.readboard.pending = Some(PendingStart::AfterInstall);
        self.readboard.install = Some(ReadBoardInstall { request: offer.request, dir });
        CommandResult::Done
    }

    // -------------------------------------------------------------------- logging

    fn readboard_request_logging(&mut self, diagnostics: bool, capture: bool, trace: bool) -> bool {
        let mut id = [0u8; 12];
        for (i, chunk) in id.chunks_mut(8).enumerate() {
            let n = std::hash::BuildHasher::hash_one(&std::collections::hash_map::RandomState::new(), i);
            chunk.copy_from_slice(&n.to_le_bytes()[..chunk.len()]);
        }
        let Some(h) = self.readboard.helper.as_mut() else { return false };
        let Some(line) = h.logging.request_set(diagnostics, capture, trace, id, Instant::now()) else { return false };
        let helper = h.id;
        let sent = self.readboard.host.as_mut().map(|host| host.send(helper, &line));
        if !matches!(sent, Some(Ok(()))) {
            if let Some(h) = self.readboard.helper.as_mut() {
                h.logging.send_failed();
            }
        }
        true
    }
}

fn logging_view(s: LoggingSnapshot) -> HelperLoggingView {
    HelperLoggingView {
        status: s.status,
        process_session_id: s.process_session_id,
        persistence: s.persistence,
        drop_count: s.drop_count,
        desired_diagnostics: s.desired.diagnostics,
        desired_capture: s.desired.capture,
        desired_trace: s.desired.trace,
        diagnostics: s.diagnostics_presentation,
        capture: s.capture_presentation,
        trace: s.trace_presentation,
        capture_summary: s.capture_summary,
    }
}

//! One helper's board synchronization (Java `analysis/ReadBoard.java` without its process, GMA,
//! update, logging and diagnostics parts): frames from the helper become record changes, moves
//! played here become `place` commands that the next frames must confirm, and the session tells
//! the workbench what Java would have done around it (analysis, auto-play, float board).
//!
//! The session is pure. The record is reached through [`SyncBoard`], host facts Java read from
//! globals arrive as [`HostContext`], time is passed in, and every outside effect is a
//! [`SessionEvent`] in Java's order. Timers (`PENDING_LOCAL_MOVE_ACK_TIMEOUT_MS`,
//! `SYNC_ANALYSIS_RESUME_DELAY_MS`, the first-sync last-move jump) run through [`ReadBoardSession::tick`]
//! and [`ReadBoardSession::next_deadline`].
//!
//! Frame handling follows `syncBoardStones`/`applySyncBoardStones` (`ReadBoard.java:1650-2045`):
//! one marked new stone is played on the main line; anything else goes through
//! `resolveCompleteSnapshotRecovery` (`:2227-2301`): reuse a matching node, replay a single move
//! with captures, hold one conflicting frame, or rebuild the record as one snapshot
//! (`rebuildFromSnapshot`, `:2345-2407`). Java code without callers (`rememberConfirmedLocalMove`
//! and the confirmed-local-move guard, the `LOCAL_MOVE_PLACE_RETRY_LIMIT = 0` retry, the history
//! jump tracker's `remember`/`consume`) is not ported.

mod classify;
mod remote;
#[cfg(test)]
mod tests;

use std::time::{Duration, Instant};

use self::classify::{
    code_stone, comparable, conflict_key, find_single_move_capture, frame_stones, has_diff, matches_stones,
    point_of, position_key, snapshot_marker, stones_key, summarize_delta, SnapshotDelta,
};
use self::remote::{RemoteContext, WindowKind};
use super::protocol::{self, FloatBoardGeometry, Inbound, LastMoveSource, Outbound, PlayConfig, StartSize, SyncPlatform};
use super::HelperInput;
use crate::record::{BoardSize, GameRecord, NodeId, NodeKind, Point, Position, Property, Setup, Stone, ROOT};

/// `PENDING_LOCAL_MOVE_ACK_TIMEOUT_MS`: a sent `place` that no frame confirmed is rolled back.
pub const PENDING_LOCAL_MOVE_ACK_TIMEOUT: Duration = Duration::from_millis(3000);
/// `FAILED_LOCAL_MOVE_OBSERVATION_GRACE_MS`: after a failed `place`, every local move is held
/// until a frame was seen or this much time passed.
pub const FAILED_LOCAL_MOVE_OBSERVATION_GRACE: Duration = Duration::from_millis(1000);
/// `SYNC_ANALYSIS_RESUME_DELAY_MS`.
pub const SYNC_ANALYSIS_RESUME_DELAY: Duration = Duration::from_millis(200);
/// The first frame's view stays one move back this long (`syncBoardStones` `firstSync` thread).
const FIRST_SYNC_LAST_MOVE_DELAY: Duration = Duration::from_millis(500);

/// The record the session synchronizes, implemented by the workbench on the current document.
/// Every write must be a normal record edit (undoable like other edits); the session never sees
/// the document otherwise. Only `move_to` and `cursor` concern the view.
pub trait SyncBoard {
    /// The current record, for queries only.
    fn record(&self) -> &GameRecord;
    /// The node the board shows (Java `getCurrentHistoryNode`).
    fn cursor(&self) -> NodeId;
    /// Java `BoardHistoryList.getMainEnd`: the last node of the main line, not counting an end
    /// placeholder (a [`NodeKind::DummyPass`] without children, as left by
    /// [`PlaceGate::Variation`]).
    fn main_end(&self) -> NodeId {
        let rec = self.record();
        let mut node = ROOT;
        while let Some(child) = next_excluding_end_placeholder(rec, node) {
            node = child;
        }
        node
    }
    /// Java `Board.placeForSync(x, y, color, true)` at [`SyncBoard::main_end`] (the session has
    /// already shown the main end): plays `color` at `point` with capture rules and makes the new
    /// move the main continuation, ahead of an end placeholder if there is one; the view moves to
    /// the new node. Returns the new node, or `None` when the move is illegal (nothing changes).
    fn place_for_sync(&mut self, color: Stone, point: Point) -> Option<NodeId>;
    /// Java `buildSnapshotHistory` + `Board.setHistory`: the record becomes a single root snapshot
    /// holding `snapshot` (stones as the root setup, `to_play`, `explicit_to_play` exactly as given,
    /// `move_number`, and the metadata comment/properties), keeping the game information (komi,
    /// players, rules); the view moves to that node, which is returned. See [`RebuiltSnapshot`]
    /// for the fields the record cannot express itself.
    fn rebuild(&mut self, snapshot: RebuiltSnapshot) -> NodeId;
    /// Shows `node` without changing the record (`moveToAnyPositionWithoutTracking`). Must not be
    /// reported back as a local navigation.
    fn move_to(&mut self, node: NodeId);
    /// Java `Board.discardFailedReadBoardMove`: when `node` is the main end and has a parent,
    /// removes it, shows the parent and returns it; otherwise changes nothing.
    fn discard_failed_move(&mut self, node: NodeId) -> Option<NodeId>;
    /// `clearBoard` line: Java `Board.clear(false)`, a new empty game of the same size.
    fn clear_board(&mut self);
    /// `start w h` with another size: Java `Board.reopen(w, h)`, a new empty game of that size.
    fn reopen(&mut self, width: u8, height: u8);
    /// `pass` line: Java `Board.changeNextTurn()` on the shown node (the other side plays next;
    /// an engine that cannot change the side to play makes it a pass instead).
    fn change_next_turn(&mut self);
}

/// The first child of `node` unless it is an end placeholder (Java `BoardHistoryNode.next()`).
pub fn next_excluding_end_placeholder(rec: &GameRecord, node: NodeId) -> Option<NodeId> {
    let child = rec.main_child(node)?;
    let c = rec.node(child);
    (!(c.kind == NodeKind::DummyPass && c.children.is_empty())).then_some(child)
}

/// The position a rebuild leaves: Java `buildSnapshotBoardData` + `copySnapshotSetupMetadata` +
/// the root start setup kept by `restoreRootStartSetupIfNoOrRootSnapshotAnchor`.
#[derive(Clone, Debug, PartialEq)]
pub struct RebuiltSnapshot {
    /// Row-major stones of the frame (`BoardData.stones`).
    pub stones: Vec<Option<Stone>>,
    /// Side to play (`blackToPlay`): an explicit `PL` of the metadata anchor wins; else a trusted
    /// marker's opposite colour, the Fox move-number parity of a markerless single move, White
    /// after a Fox handicap setup at move 0, Black on an empty board, or the sync start's side.
    pub to_play: Stone,
    /// The `PL` carried over from the metadata anchor. `None`: the side to play is inferred and the
    /// node must not declare one.
    pub explicit_to_play: Option<Stone>,
    /// `moveNumber`: the Fox move number when the frame had one; otherwise the sync start's number
    /// plus the added stones of a marked frame, the stone count adjusted to the marker's parity,
    /// 0 for an empty board, or the sync start's number.
    pub move_number: u32,
    /// The last-move marker of the frame (`lastMove` + `lastMoveColor`), shown on the snapshot.
    pub last_move: Option<(Point, Stone)>,
    /// `moveNumberList`, row-major: the sync start's numbers on unchanged stones (not above
    /// `move_number`), `move_number` on the marked stone, 0 elsewhere.
    pub move_numbers: Vec<u32>,
    /// Captures of the sync start when the stones are exactly the same, else 0.
    pub black_captures: u32,
    pub white_captures: u32,
    /// Copied from the nearest snapshot ancestor of the sync start (`copySnapshotSetupMetadata`).
    pub metadata: Option<SnapshotMetadata>,
    /// The old start position counted as a setup/handicap start (`Board.hasStartStone`) and the
    /// rebuild keeps it so (the metadata anchor was the root). The session keeps this fact for the
    /// rebuilt node; the record needs nothing for it.
    pub keeps_start_setup: bool,
}

/// What a rebuilt snapshot takes over from the snapshot it replaces.
#[derive(Clone, Debug, PartialEq)]
pub struct SnapshotMetadata {
    pub comment: String,
    /// All retained properties of the anchor (`MN` included; Java also applies `MN` as the shown
    /// move number).
    pub properties: Vec<Property>,
    /// The anchor's own setup lists (Java `extraStones` and the removed-stone flag). The rebuilt
    /// root's setup holds every stone anyway; this is what the anchor declared itself.
    pub setup: Setup,
}

/// Live ReadBoard options (Java `Config.alwaysSyncBoardStat`, `Config.readBoardGetFocus`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionSettings {
    /// 总是保持棋盘一致: show the newest move after a synced move.
    pub always_sync_board_stat: bool,
    /// Give the focus back to the target window after the float board took it (`loss`).
    pub read_board_get_focus: bool,
}

impl Default for SessionSettings {
    fn default() -> Self {
        SessionSettings { always_sync_board_stat: true, read_board_get_focus: true }
    }
}

/// Host facts Java's `ReadBoard` read from globals at the moment it needed them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostContext {
    /// `isReadBoardAnalysisEngineAvailable`: a foreground engine is started.
    pub engine_available: bool,
    /// `Lizzie.leelaz.isPondering()`.
    pub pondering: bool,
    /// `LizzieFrame.isPlayingAgainstLeelaz`: a human-against-engine game where the engine answers
    /// with `genmove`.
    pub playing_against_engine: bool,
    /// `LizzieFrame.playerIsBlack` of that game.
    pub player_is_black: bool,
    /// `LizzieFrame.isAnaPlayingAgainstLeelaz`: an analysis-mode game / toolbar auto-play runs.
    pub analysis_game: bool,
    /// `LizzieFrame.canResumeAnalysisAfterSync()`: the user has not paused analysis.
    pub can_resume_analysis: bool,
    /// `LizzieFrame.canResumeReadBoardAutoPlayAnalysis()`.
    pub can_resume_auto_play_analysis: bool,
    /// A normal (non-GMA) auto-play intent from a `play>` line waits (`normalAutoPlayIntent`).
    pub normal_auto_play_intent: bool,
    /// ...and the engine drain it waits for is still running (`isReadBoardGmaDraining`).
    pub normal_auto_play_draining: bool,
    /// GMA work besides the armed flag: a physical request, a restore pending or in progress, a
    /// session not yet terminal, an engine drain (`isReadBoardGmaEngineBusy` minus the flag).
    pub gma_busy: bool,
    /// `readBoardGmaPending || readBoardGmaEngineRestorePending || readBoardGmaEngineRestoreInProgress`.
    pub gma_in_flight: bool,
    /// `readBoardGmaEngineRestorePending`.
    pub gma_restore_pending: bool,
    /// `Leelaz.canArmReadBoardGma()`.
    pub can_arm_gma: bool,
    /// 五子棋无提子规则 (`Config.noCapture`).
    pub no_capture: bool,
    /// `Leelaz.canSuicidal`: multi-stone suicide is legal.
    pub allow_suicide: bool,
    /// `floatBoard.isVisible()`: the float board window is on the screen now.
    pub float_board_shown: bool,
    /// `ReadBoard.editMode`: the float board's edit mode; synced frames leave the view where the
    /// user is.
    pub float_edit_mode: bool,
}

/// How a frame that was not one new stone was resolved (`CompleteSnapshotRecoveryOutcome`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecoveryOutcome {
    NoChange,
    SingleMoveRecovery,
    Hold,
    ForceRebuild,
}

/// Diagnostics trace of one recovery decision (`SyncDecisionTrace` reason codes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncDecision {
    pub outcome: RecoveryOutcome,
    pub reason: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FloatBoardEvent {
    /// `inboard …`: create or move the float board (Java `new FloatBoard`/`setPos`).
    Show(FloatBoardGeometry),
    /// `noinboard`, `endsync`, `stopsync`, helper exit: hide it if it is visible.
    Hide,
    /// `notinboard`: hide it; it no longer counts as shown.
    HideAndForget,
    /// `foreFoxWithInBoard` is on and a `place` is about to be sent: hide it until the result.
    /// `place_waits`: the window was shown, so the `place` is written only once the window is
    /// confirmed hidden ([`ReadBoardSession::float_board_hidden`]); never without that
    /// confirmation (an unconfirmed move fails at its ack timeout). Otherwise it was written
    /// right after this event.
    HideForPlace { place_waits: bool },
    /// The `place` result arrived: show the float board hidden by `HideForPlace` again.
    RestoreAfterPlace,
    /// `bothSync`/`nobothSync`: refresh its edit button if it is visible.
    SetEditButton,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoPlayParam {
    /// `timechanged`: seconds; `<= 0` means the engine's `max-game-thinking-time-seconds`.
    Time(i32),
    /// `playoutschanged`: `<= 0` unchecks the playouts box.
    Playouts(i32),
    /// `firstchanged`: `<= 0` unchecks the first-playouts box.
    FirstPlayouts(i32),
}

/// What a local click may do (`Board.place` checks before and around the move).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaceGate {
    /// Play it normally; call [`ReadBoardSession::local_move`] afterwards.
    Proceed,
    /// Drop the click: a failed `place` is still unresolved or another `place` awaits its frame
    /// (`shouldSuppressLocalPlaceAfterFailedReadBoardSync`, `…WhileReadBoardPending`).
    Suppressed,
    /// One-way sync at the main end (`Board.java:2819-2863`): the click becomes a variation behind
    /// an end placeholder (`DocumentEdit::AddBranch` shape), is sent to the engine only and not to
    /// the helper. Java checks this after the point was found free; the workbench does the same.
    Variation,
}

/// A `place` sent and not yet seen in a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingLocalMove {
    /// The main-end move it was sent for; `None` when the main end was not a move.
    pub node: Option<NodeId>,
    pub point: Option<Point>,
    pub color: Option<Stone>,
    /// A `placeComplete` arrived for it (not for a previous `place`, see quarantine).
    pub click_completed: bool,
}

/// The auto-play state the helper's `play>` lines set (GMA arming; Java `readBoardGma*`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AutoPlayLine {
    /// `readBoardGmaAutoPlayActive`.
    pub gma_active: bool,
    /// `readBoardGmaAutoPlayColor`.
    pub color: Option<Stone>,
    /// `readBoardGmaTimeSeconds` (never negative).
    pub time_seconds: i32,
    /// `readBoardGmaMaxVisits` (never negative).
    pub max_visits: i32,
}

/// Float board facts the helper drives.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FloatBoardState {
    /// `showInBoard`: the helper says the float board covers the target board (forces following
    /// the newest move).
    pub shown: bool,
    /// `hideFromPlace`: hidden while a `place` runs.
    pub hidden_for_place: bool,
    /// `hideFloadBoardBeforePlace` (`foreFoxWithInBoard`).
    pub hide_before_place: bool,
}

/// One effect for the workbench, in the order Java performed it.
#[derive(Clone, Debug, PartialEq)]
pub enum SessionEvent {
    /// Write this line to the helper.
    Send(String),
    /// `sync` with an idle engine: start analysing (`togglePonder`).
    StartAnalysis,
    /// `stopsync` (`nameCmd`) or `noponder` while pondering: stop analysing.
    StopAnalysis,
    /// Analyse `node` now: the delayed resume after a sync (only with `playpon on`), an auto-play
    /// game's confirmed sync, or keeping the analysis when the helper ended.
    ResumeAnalysis { node: NodeId },
    /// `resumeponder` with an idle engine: `togglePonderMannul`.
    ToggleAnalysisManual,
    /// Java `invalidatePendingSyncAnalysisResume`: drop a waiting normal auto-play intent; a
    /// scheduled resume is already void.
    ResumeCancelled,
    /// A Java `continueGameAfterSyncIfNeeded(reason, node)` site: the board is synced at `node`
    /// (marks a GMA board as synced, flushes a deferred GMA restore).
    Synced { node: NodeId, reason: &'static str },
    /// Ask the engine for `color`'s move in a human-against-engine game.
    Genmove { color: Stone },
    /// Java `scheduleReadBoardGmaIfNeeded(reason)`.
    ScheduleGma { reason: String },
    /// Java `requestReadBoardGmaEngineRestore(reason, node)`.
    GmaRestoreRequested { node: NodeId, reason: &'static str },
    /// Java `invalidateReadBoardGmaPhysicalRequestIfPending(reason)`.
    GmaRequestInvalidated { reason: &'static str },
    /// A `play> … gma` line the engine cannot take (`showForegroundEngineLeaseConflict`,
    /// `AnalysisSettings.reuseStatus.existing_lease`); the line was dropped.
    GmaLeaseConflict,
    /// Java `clearReadBoardGmaAutoPlay(reason)`.
    GmaCleared { reason: &'static str },
    /// `playpon on|off` set the in-memory `readBoardPonder` (`off` also stops an idle GMA's
    /// pondering, Java `disableReadBoardGmaPonderIfIdle`).
    PonderPreference { on: bool },
    /// `play>…`: write the auto-play toolbar (time `<= 0` → the engine's
    /// `max-game-thinking-time-seconds`; playouts/first `<= 0` uncheck), select the side and start
    /// the analysis-mode game for `toolbar_side`; without `gma` restore normal auto-play (ponder,
    /// or wait for the engine drain as a normal auto-play intent).
    AutoPlay {
        color: Option<Stone>,
        toolbar_side: Option<Stone>,
        time: i32,
        playouts: i32,
        first_playouts: i32,
        gma: bool,
    },
    AutoPlayParam(AutoPlayParam),
    /// `stopAutoPlay`: uncheck auto-play.
    AutoPlayStopped,
    /// Java `stopAiPlayingAndPolicy()`.
    StopGame,
    /// `noponder` during a human-against-engine game: it ends (`isPlayingAgainstLeelaz = false`).
    StopPlayingAgainstEngine,
    /// `pass` line handled: the side to play changed (GMA `readboard-exchange-order`).
    PassLine,
    /// A frame changed the board while a normal auto-play intent waited for its engine drain:
    /// the intent must restore `node` (or a GMA session absorbs it).
    NormalAutoPlayTargetChanged { node: NodeId },
    FloatBoard(FloatBoardEvent),
    /// `ReadBoard.versionCheckFaied`; the helper keeps running.
    VersionTooOld,
    /// A line for the GTP console's ReadBoard pane.
    ConsoleLine(String),
    /// `readboardUpdateReady…` for the hosted updater.
    UpdateReady(String),
    /// A logging negotiation line.
    LoggingLine(String),
    /// The first `ready` was handled: the logging handshake may start.
    LoggingReady,
    /// Start Yike browser sync; `from_platform_line`: from `syncPlatform yike` (keep the current
    /// address) rather than `yikeSyncStart` (reload the room).
    YikeSyncStart { from_platform_line: bool },
    YikeSyncStop,
    /// The frame showed the stone of the pending `place`.
    LocalMoveConfirmed { node: NodeId },
    /// The pending `place` failed: rolled back to `node` when the move was still the main end.
    LocalMoveFailed { node: Option<NodeId>, reason: String },
    Decision(SyncDecision),
}

/// Effects and inputs of one call.
struct Call<'a> {
    board: &'a mut dyn SyncBoard,
    ctx: &'a HostContext,
    now: Instant,
    events: Vec<SessionEvent>,
}

impl Call<'_> {
    fn emit(&mut self, event: SessionEvent) {
        self.events.push(event);
    }

    fn send(&mut self, command: Outbound) {
        self.events.push(SessionEvent::Send(command.line()));
    }
}

/// A node as Java's `BoardData` saw it.
#[derive(Clone, Debug)]
struct NodeView {
    id: NodeId,
    kind: NodeKind,
    stones: Vec<Option<Stone>>,
    black_captures: u32,
    white_captures: u32,
    move_numbers: Vec<u32>,
    last_move: Option<(Point, Stone)>,
    to_play: Stone,
    move_number: u32,
}

impl NodeView {
    fn is_move(&self) -> bool {
        matches!(self.kind, NodeKind::Move { .. })
    }
}

/// What the record cannot hold about the snapshot the last rebuild made.
#[derive(Clone, Debug)]
struct RebuiltFacts {
    node: NodeId,
    stamp: u64,
    last_move: Option<(Point, Stone)>,
    black_captures: u32,
    white_captures: u32,
    move_numbers: Vec<u32>,
    explicit_to_play: Option<Stone>,
    /// Copied setup or setup properties (`hasSetupOrHandicapProperty`, extra/removed stones).
    setup_risk: bool,
    keeps_start_setup: bool,
}

#[derive(Clone, Copy, Debug)]
struct Suppression {
    point: Point,
    color: Stone,
}

#[derive(Clone, Copy, Debug)]
struct ScheduledResume {
    epoch: u64,
    node: NodeId,
    at: Instant,
}

/// Local move confirmation state (Java `pendingLocalMove*`, `failedLocalMove*`).
#[derive(Default)]
struct LocalMoves {
    /// `lastMovePlayByLizzie`.
    by_client: bool,
    /// `waitingForReadBoardLocalMoveAck`.
    waiting: bool,
    click_completed: bool,
    /// `lastPendingLocalMoveRetryTimeMs`.
    last_attempt: Option<Instant>,
    node: Option<NodeId>,
    point: Option<Point>,
    color: Option<Stone>,
    baseline_key: String,
    /// When the scheduled ack timeout fires.
    ack_deadline: Option<Instant>,
    /// `ignoreReadBoardPlaceResultsForCurrentPending` / `…ForNextPending`.
    ignore_current: bool,
    ignore_next: bool,
    suppression: Option<Suppression>,
    /// `failedLocalMoveSuppressionSnapshotKey`.
    suppression_key: String,
    /// `failedLocalMoveRecovery*`.
    recovery: Option<Suppression>,
    awaiting_observation: bool,
    waiting_our_turn: bool,
    observation_deadline: Option<Instant>,
    /// A `place` for the pending move that waits for the shown float board to be confirmed hidden.
    place_after_hide: Option<Point>,
}

/// One helper's protocol and synchronization state.
pub struct ReadBoardSession {
    settings: SessionSettings,
    /// `isLoaded`: the first `ready` was handled.
    loaded: bool,
    /// `shutdownStarted`.
    shut_down: bool,
    /// The helper's output ended; nothing more can be written.
    process_ended: bool,
    /// `LizzieFrame.syncBoard`.
    sync_board: bool,
    /// `LizzieFrame.bothSync`.
    both_sync: bool,
    /// `Config.readBoardPonder` (only `playpon` sets it; not saved).
    read_board_ponder: bool,
    first_sync: bool,
    need_genmove: bool,
    show_in_board: bool,
    float_board_created: bool,
    hide_before_place: bool,
    hide_from_place: bool,
    turn_trusted: bool,
    /// `tempcount`.
    frame: Vec<u8>,
    invalid_frame: bool,
    /// `pendingRemoteContext`.
    remote: RemoteContext,
    awaiting_first_sync_frame: bool,
    /// `SyncConflictTracker`: the conflict held once.
    conflict: Option<String>,
    /// `SyncResumeState`.
    resume: Option<(NodeId, RemoteContext)>,
    last_resolved: Option<NodeId>,
    /// `syncAnalysisEpoch`.
    epoch: u64,
    scheduled_resume: Option<ScheduledResume>,
    first_sync_jump: Option<Instant>,
    /// While a frame runs inside Java's `applyReadBoardSync`: the resume target it collected.
    collecting: Option<Option<NodeId>>,
    /// Record writes made by this session (Java's context revision for intent checks).
    writes: u64,
    auto_play: AutoPlayLine,
    local: LocalMoves,
    rebuilt: Option<RebuiltFacts>,
}

impl ReadBoardSession {
    pub fn new(settings: SessionSettings) -> ReadBoardSession {
        ReadBoardSession {
            settings,
            loaded: false,
            shut_down: false,
            process_ended: false,
            sync_board: false,
            both_sync: false,
            read_board_ponder: false,
            first_sync: true,
            need_genmove: false,
            show_in_board: false,
            float_board_created: false,
            hide_before_place: false,
            hide_from_place: false,
            turn_trusted: false,
            frame: Vec::new(),
            invalid_frame: false,
            remote: RemoteContext::generic(false),
            awaiting_first_sync_frame: true,
            conflict: None,
            resume: None,
            last_resolved: None,
            epoch: 0,
            scheduled_resume: None,
            first_sync_jump: None,
            collecting: None,
            writes: 0,
            auto_play: AutoPlayLine::default(),
            local: LocalMoves::default(),
            rebuilt: None,
        }
    }

    pub fn settings(&self) -> SessionSettings {
        self.settings
    }

    pub fn set_settings(&mut self, settings: SessionSettings) {
        self.settings = settings;
    }

    /// `LizzieFrame.syncBoard`: the helper is syncing (`sync` … `endsync`/`stopsync`).
    pub fn sync_board(&self) -> bool {
        self.sync_board
    }

    /// `LizzieFrame.bothSync`: local moves are sent to the helper.
    pub fn both_sync(&self) -> bool {
        self.both_sync
    }

    /// `readBoardTurnTrusted`: the side to play of the last accepted frame is reliable.
    pub fn turn_trusted(&self) -> bool {
        self.turn_trusted
    }

    /// The first `ready` was handled.
    pub fn loaded(&self) -> bool {
        self.loaded
    }

    pub fn shut_down(&self) -> bool {
        self.shut_down
    }

    /// `Config.readBoardPonder` as set by `playpon`.
    pub fn read_board_ponder(&self) -> bool {
        self.read_board_ponder
    }

    /// `awaitingFirstSyncFrame`.
    pub fn awaiting_first_sync_frame(&self) -> bool {
        self.awaiting_first_sync_frame
    }

    /// The node the last accepted frame resolved to (Java `SyncResumeState.node`), which the next
    /// frames are recognised against; `None` after the resume state was reset.
    pub fn resolved_node(&self) -> Option<NodeId> {
        self.last_resolved
    }

    /// `isFailedLocalMoveAwaitingRemoteObservation`.
    pub fn failed_local_move_awaiting_observation(&self) -> bool {
        self.local.recovery.is_some() && self.local.awaiting_observation
    }

    /// `isPendingLocalMoveAwaitingReadBoard` with what it was sent for.
    pub fn pending_local_move(&self) -> Option<PendingLocalMove> {
        self.is_pending().then_some(PendingLocalMove {
            node: self.local.node,
            point: self.local.point,
            color: self.local.color,
            click_completed: self.local.click_completed,
        })
    }

    pub fn auto_play(&self) -> AutoPlayLine {
        self.auto_play
    }

    pub fn float_board(&self) -> FloatBoardState {
        FloatBoardState {
            shown: self.show_in_board,
            hidden_for_place: self.hide_from_place,
            hide_before_place: self.hide_before_place,
        }
    }

    /// `hasLocalMoveConfirmationAuthority`: syncing both ways with a live helper.
    pub fn has_local_move_confirmation_authority(&self) -> bool {
        !self.shut_down && !self.process_ended && self.sync_board && self.both_sync
    }

    /// The earliest time [`ReadBoardSession::tick`] has work.
    pub fn next_deadline(&self) -> Option<Instant> {
        [
            self.local.ack_deadline,
            self.scheduled_resume.map(|r| r.at),
            self.first_sync_jump,
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// One helper input.
    pub fn input(
        &mut self,
        board: &mut dyn SyncBoard,
        input: &HelperInput,
        ctx: &HostContext,
        now: Instant,
    ) -> Vec<SessionEvent> {
        match input {
            HelperInput::Line(line) => self.line(board, line, ctx, now),
            HelperInput::Ended { error } => self.ended(board, error.is_some(), ctx, now),
        }
    }

    /// One helper line (Java `parseLine`, then `handleReady` for `ready`).
    pub fn line(&mut self, board: &mut dyn SyncBoard, line: &str, ctx: &HostContext, now: Instant) -> Vec<SessionEvent> {
        let mut c = Call { board, ctx, now, events: Vec::new() };
        let raw = line.strip_suffix('\r').unwrap_or(line);
        self.handle(&mut c, raw, protocol::parse_line(raw));
        c.events
    }

    /// The helper's output ended (Java's reader loop end) or reading failed (`read_failed`).
    pub fn ended(
        &mut self,
        board: &mut dyn SyncBoard,
        read_failed: bool,
        ctx: &HostContext,
        now: Instant,
    ) -> Vec<SessionEvent> {
        let mut c = Call { board, ctx, now, events: Vec::new() };
        if read_failed {
            self.both_sync = false;
            self.sync_board = false;
        } else {
            if ctx.pondering {
                let node = c.board.cursor();
                c.emit(SessionEvent::ResumeAnalysis { node });
            }
            self.show_in_board = false;
            if self.float_board_created {
                c.emit(SessionEvent::FloatBoard(FloatBoardEvent::Hide));
            }
        }
        self.process_ended = true;
        self.shutdown_inner(&mut c, false);
        c.events
    }

    /// Graceful stop by the client (Java `shutdown()`): resets the sync state and sends `quit`
    /// to a helper that is still running. Idempotent.
    pub fn shutdown(&mut self, board: &mut dyn SyncBoard, ctx: &HostContext, now: Instant) -> Vec<SessionEvent> {
        let mut c = Call { board, ctx, now, events: Vec::new() };
        self.shutdown_inner(&mut c, true);
        c.events
    }

    /// Runs the timers due at `now`.
    pub fn tick(&mut self, board: &mut dyn SyncBoard, ctx: &HostContext, now: Instant) -> Vec<SessionEvent> {
        let mut c = Call { board, ctx, now, events: Vec::new() };
        if self.local.ack_deadline.is_some_and(|d| d <= now) {
            self.local.ack_deadline = None;
            self.fail_if_ack_timed_out(&mut c);
        }
        if let Some(resume) = self.scheduled_resume.filter(|r| r.at <= now) {
            self.scheduled_resume = None;
            self.resume_if_still_current(&mut c, resume);
        }
        if self.first_sync_jump.is_some_and(|d| d <= now) {
            self.first_sync_jump = None;
            self.show_line_end(&mut c);
        }
        c.events
    }

    /// Before a local click is played (Java `Board.place` checks, `forSync = false`).
    pub fn place_gate(&mut self, board: &dyn SyncBoard, point: Point, color: Stone, now: Instant) -> PlaceGate {
        if self.both_sync
            && self.has_local_move_confirmation_authority()
            && (self.should_suppress_after_failed(point, color, now) || self.is_pending())
        {
            return PlaceGate::Suppressed;
        }
        if !self.both_sync && self.sync_board {
            let current = board.cursor();
            if current == board.main_end() {
                let rec = board.record();
                let followed = rec
                    .node(current)
                    .children
                    .iter()
                    .any(|&child| matches!(rec.node(child).kind, NodeKind::Move { point: p, .. } if p == point));
                if !followed {
                    return PlaceGate::Variation;
                }
            }
        }
        PlaceGate::Proceed
    }

    /// A local move was just played as a new main-end move (a user click that did not follow an
    /// existing move, or a GMA move) at `point`: Java `sendReadBoardPlaceIfAllowed` +
    /// `sendCommand("place x y")`. Sends `place x y` only while syncing both ways with no other
    /// `place` pending, and starts waiting for a frame that shows the main-end move.
    pub fn local_move(&mut self, board: &mut dyn SyncBoard, point: Point, ctx: &HostContext, now: Instant) -> Vec<SessionEvent> {
        let mut c = Call { board, ctx, now, events: Vec::new() };
        if !self.has_local_move_confirmation_authority() || self.is_pending() {
            return c.events;
        }
        if self.local.recovery.is_some_and(|r| r.point != point) {
            self.clear_failed_state();
        }
        self.start_tracking(&mut c);
        if ctx.playing_against_engine {
            self.need_genmove = true;
        }
        if !self.is_current_confirmation(&*c.board) {
            return c.events;
        }
        if self.hide_before_place && self.float_board_created {
            self.hide_from_place = true;
            let place_waits = ctx.float_board_shown;
            c.emit(SessionEvent::FloatBoard(FloatBoardEvent::HideForPlace { place_waits }));
            if place_waits {
                // Java hides the window on the EDT and waits for it before writing `place`.
                self.local.place_after_hide = Some(point);
                return c.events;
            }
        }
        c.send(Outbound::Place { x: point.x, y: point.y });
        c.events
    }

    /// The float board hidden by [`FloatBoardEvent::HideForPlace`] `{ place_waits: true }` is off
    /// the screen: the `place` that waited for it is written, unless its move is no longer the
    /// pending main-end move (Java's `isCurrentLocalMoveConfirmation` before writing).
    pub fn float_board_hidden(&mut self, board: &mut dyn SyncBoard, ctx: &HostContext, now: Instant) -> Vec<SessionEvent> {
        let mut c = Call { board, ctx, now, events: Vec::new() };
        self.send_place_after_hide(&mut c);
        c.events
    }

    /// Writes the `place` that waited for the float board, once.
    fn send_place_after_hide(&mut self, c: &mut Call<'_>) {
        let Some(point) = self.local.place_after_hide.take() else { return };
        if self.is_current_confirmation(&*c.board) {
            c.send(Outbound::Place { x: point.x, y: point.y });
        }
    }

    /// The user moved the view (Java `onLocalHistoryNavigation`; only user navigation, never the
    /// session's own `move_to`): a scheduled analysis resume no longer applies.
    pub fn local_navigation(&mut self) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        self.invalidate_resume(&mut events);
        events
    }

    /// The record was replaced by something other than this session (open, new game, external
    /// `setHistory`/`clear`; Java `onHistoryOverwritten`): pending and failed local moves and the
    /// resume anchor are dropped.
    pub fn history_overwritten(&mut self) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        self.overwritten(&mut events);
        events
    }

    /// The float board took the focus (Java `sendLossFocus`): give it back with `loss`.
    pub fn return_focus(&self) -> Vec<SessionEvent> {
        if self.settings.read_board_get_focus {
            vec![SessionEvent::Send(Outbound::Loss.line())]
        } else {
            Vec::new()
        }
    }

    /// Java `releaseFailedLocalMoveObservationIfTimedOut`, for the GMA scheduler: once the grace
    /// after a failed `place` has passed, forget the failure. Returns whether it was released.
    pub fn release_failed_local_move_observation_if_elapsed(&mut self, now: Instant) -> bool {
        if !self.local.awaiting_observation {
            return false;
        }
        if self.local.observation_deadline.is_some_and(|d| now < d) {
            return false;
        }
        self.clear_failed_state();
        true
    }

    // ------------------------------------------------------------------------------- lines

    fn handle(&mut self, c: &mut Call<'_>, line: &str, inbound: Inbound) {
        match inbound {
            Inbound::UpdateReady => c.emit(SessionEvent::UpdateReady(line.to_owned())),
            Inbound::Logging => c.emit(SessionEvent::LoggingLine(line.to_owned())),
            Inbound::YikeSyncStart => c.emit(SessionEvent::YikeSyncStart { from_platform_line: false }),
            Inbound::YikeSyncStop => c.emit(SessionEvent::YikeSyncStop),
            Inbound::PlayPonder(Some(on)) => {
                self.read_board_ponder = on;
                c.emit(SessionEvent::PonderPreference { on });
            }
            Inbound::PlayPonder(None) => {}
            Inbound::Row(cells) => self.row(c, cells),
            Inbound::FoxMoveNumber(Some(n)) => self.remote = self.remote.with_fox_move_number(Some(n)),
            Inbound::FoxMoveNumber(None) => {}
            Inbound::LastMoveSource(source) => self.remote = self.remote.with_last_move_source(source),
            Inbound::SyncPlatform { platform, yike_line } => {
                self.remote = self.remote.with_platform(platform);
                if yike_line {
                    c.emit(SessionEvent::YikeSyncStart { from_platform_line: true });
                }
            }
            Inbound::RoomToken(token) => self.remote = self.remote.with_room_token(token),
            Inbound::LiveTitleMove(n) => self.remote = self.remote.with_live_title_move(n),
            Inbound::RecordCurrentMove(n) => self.remote = self.remote.with_record_current_move(n),
            Inbound::RecordTotalMove(n) => self.remote = self.remote.with_record_total_move(n),
            Inbound::RecordAtEnd(at_end) => self.remote = self.remote.with_record_at_end(at_end),
            Inbound::RecordTitleFingerprint(f) => self.remote = self.remote.with_title_fingerprint(f),
            Inbound::ForceRebuild => self.remote = self.remote.with_force_rebuild(true),
            Inbound::Version { number } => {
                c.emit(SessionEvent::ConsoleLine(format!("Board synchronization tool {line}")));
                if number.is_some_and(|n| n < protocol::MIN_HELPER_VERSION) {
                    c.emit(SessionEvent::VersionTooOld);
                }
            }
            Inbound::PlaceFailed { echo } => {
                self.handle_failure(c, format!("readboard line placement failed line={line}"));
                if echo {
                    c.emit(SessionEvent::ConsoleLine(line.to_owned()));
                }
            }
            Inbound::Error => c.emit(SessionEvent::ConsoleLine(line.to_owned())),
            Inbound::End => {
                let yike = self.remote.platform == SyncPlatform::Yike;
                if !self.invalid_frame && !yike {
                    self.sync_board_stones(c);
                }
                self.remote = RemoteContext::generic(false);
                self.frame.clear();
                self.invalid_frame = false;
            }
            Inbound::Clear => {
                self.invalid_frame = false;
                self.reset_for_control_line();
                self.remote = RemoteContext::generic(false);
                self.frame.clear();
            }
            Inbound::ClearBoard => {
                c.board.clear_board();
                self.writes += 1;
                self.overwritten(&mut c.events);
            }
            Inbound::Start(size) => self.start(c, size),
            Inbound::Sync => {
                self.sync_board = true;
                if self.failed_local_move_awaiting_observation() {
                    self.release_failed_local_move_observation_if_elapsed(c.now);
                }
                if !self.auto_play.gma_active
                    && !c.ctx.normal_auto_play_intent
                    && c.ctx.engine_available
                    && !c.ctx.pondering
                {
                    c.emit(SessionEvent::StartAnalysis);
                }
            }
            Inbound::BothSync => {
                self.both_sync = true;
                if self.float_board_created {
                    c.emit(SessionEvent::FloatBoard(FloatBoardEvent::SetEditButton));
                }
            }
            Inbound::NoBothSync => {
                self.invalidate_resume(&mut c.events);
                self.clear_gma(c, "noboth");
                self.turn_trusted = false;
                self.both_sync = false;
                self.clear_pending_tracking();
                if self.float_board_created {
                    c.emit(SessionEvent::FloatBoard(FloatBoardEvent::SetEditButton));
                }
            }
            Inbound::StopAutoPlay => {
                self.invalidate_resume(&mut c.events);
                self.clear_gma(c, "stopAutoPlay");
                c.emit(SessionEvent::AutoPlayStopped);
            }
            Inbound::EndSync => self.end_sync(c, false),
            Inbound::StopSync => self.end_sync(c, true),
            Inbound::Play(Some(config)) => self.play(c, config),
            Inbound::Play(None) => {}
            Inbound::Pass => {
                c.board.change_next_turn();
                self.writes += 1;
                self.refresh_rebuilt_stamp(&*c.board);
                self.turn_trusted = true;
                c.emit(SessionEvent::PassLine);
            }
            Inbound::FirstChanged(Some(n)) => c.emit(SessionEvent::AutoPlayParam(AutoPlayParam::FirstPlayouts(n))),
            Inbound::PlayoutsChanged(Some(n)) => {
                if self.auto_play.gma_active {
                    self.auto_play.max_visits = n.max(0);
                }
                c.emit(SessionEvent::AutoPlayParam(AutoPlayParam::Playouts(n)));
            }
            Inbound::TimeChanged(Some(n)) => {
                if self.auto_play.gma_active {
                    self.auto_play.time_seconds = n.max(0);
                }
                c.emit(SessionEvent::AutoPlayParam(AutoPlayParam::Time(n)));
            }
            Inbound::FirstChanged(None) | Inbound::PlayoutsChanged(None) | Inbound::TimeChanged(None) => {}
            Inbound::NoPonder => {
                self.invalidate_resume(&mut c.events);
                self.clear_gma(c, "noponder");
                if c.ctx.playing_against_engine {
                    c.emit(SessionEvent::StopPlayingAgainstEngine);
                }
                if c.ctx.analysis_game {
                    c.emit(SessionEvent::StopGame);
                }
                if c.ctx.pondering {
                    c.emit(SessionEvent::StopAnalysis);
                }
                c.send(Outbound::AnalysisState { running: false });
            }
            Inbound::ResumePonder => {
                let start = !c.ctx.normal_auto_play_intent
                    && !(self.auto_play.gma_active || c.ctx.gma_busy)
                    && c.ctx.engine_available
                    && !c.ctx.pondering;
                if start {
                    c.emit(SessionEvent::ToggleAnalysisManual);
                }
                c.send(Outbound::AnalysisState { running: start || c.ctx.pondering });
            }
            Inbound::NoInBoard => {
                if self.float_board_created {
                    c.emit(SessionEvent::FloatBoard(FloatBoardEvent::Hide));
                }
            }
            Inbound::InBoard(geometry) => {
                if self.hide_from_place {
                    return;
                }
                self.show_in_board = true;
                if let Some(geometry) = geometry {
                    self.float_board_created = true;
                    c.emit(SessionEvent::FloatBoard(FloatBoardEvent::Show(geometry)));
                }
            }
            Inbound::NotInBoard => {
                self.show_in_board = false;
                if self.float_board_created {
                    c.emit(SessionEvent::FloatBoard(FloatBoardEvent::HideAndForget));
                }
            }
            Inbound::ForeFoxWithInBoard => self.hide_before_place = true,
            Inbound::NotForeFoxWithInBoard => self.hide_before_place = false,
            Inbound::PlaceComplete => {
                self.mark_click_completed();
                self.restore_float_after_place(c);
            }
            Inbound::Ready => self.handle_ready(c),
            Inbound::Other => {}
        }
    }

    /// `re=` (`ReadBoard.java:766-791`).
    fn row(&mut self, c: &mut Call<'_>, cells: Option<Vec<u8>>) {
        let size = c.board.record().size;
        match cells {
            Some(cells) if cells.len() == size.width as usize && self.frame.len() + cells.len() <= size.area() => {
                if !self.invalid_frame {
                    self.frame.extend(cells);
                }
            }
            _ => self.invalid_frame = true,
        }
    }

    /// `start` (`ReadBoard.java:897-917`).
    fn start(&mut self, c: &mut Call<'_>, size: StartSize) {
        self.invalid_frame = false;
        self.remote = RemoteContext::generic(false);
        match size {
            StartSize::Size { width, height } => {
                let current = c.board.record().size;
                if u16::from(width) != current.width || u16::from(height) != current.height {
                    self.reset_active(false, false, false);
                    self.clear_resume_state();
                    c.board.reopen(width, height);
                    self.writes += 1;
                    self.overwritten(&mut c.events);
                } else {
                    self.reset_for_control_line();
                }
            }
            StartSize::Missing => self.reset_for_control_line(),
            StartSize::Invalid => return,
        }
        self.frame.clear();
    }

    /// `endsync` / `stopsync` (`ReadBoard.java:959-992`).
    fn end_sync(&mut self, c: &mut Call<'_>, stop_engine: bool) {
        self.invalidate_resume(&mut c.events);
        self.sync_board = false;
        self.reset_active(false, false, false);
        self.restore_float_after_place(c);
        self.clear_gma(c, if stop_engine { "stopsync" } else { "endsync" });
        self.remote = RemoteContext::generic(false);
        self.frame.clear();
        if c.ctx.analysis_game {
            c.emit(SessionEvent::StopGame);
        }
        if stop_engine {
            c.emit(SessionEvent::StopAnalysis);
        }
        self.show_in_board = false;
        if self.float_board_created {
            c.emit(SessionEvent::FloatBoard(FloatBoardEvent::Hide));
        }
    }

    /// `play>…` (`ReadBoard.java:993-1097`).
    fn play(&mut self, c: &mut Call<'_>, config: PlayConfig) {
        self.invalidate_resume(&mut c.events);
        let already_armed = config.gma && self.auto_play.gma_active;
        if config.gma && !already_armed && !c.ctx.can_arm_gma {
            c.emit(SessionEvent::GmaLeaseConflict);
            return;
        }
        if already_armed && config.color.is_some() && config.color != self.auto_play.color {
            c.emit(SessionEvent::GmaRequestInvalidated { reason: "play-color-switch" });
        }
        if let Some(color) = config.color {
            let failed = self.local.recovery.map(|r| r.color).or(self.local.suppression.map(|s| s.color));
            if self.local.recovery.is_some() && failed.is_some_and(|f| f != color) {
                self.clear_failed_state();
            }
        }
        if self.local.recovery.is_none() {
            self.clear_failed_state();
        }
        self.auto_play = AutoPlayLine {
            gma_active: config.gma,
            color: config.color,
            time_seconds: config.time.max(0),
            max_visits: config.playouts.max(0),
        };
        if !config.gma {
            c.emit(SessionEvent::GmaRequestInvalidated { reason: "play-mode-switch" });
        }
        c.emit(SessionEvent::AutoPlay {
            color: config.color,
            toolbar_side: config.toolbar_side,
            time: config.time,
            playouts: config.playouts,
            first_playouts: config.first_playouts,
            gma: config.gma,
        });
    }

    /// `handleReady` (`ReadBoard.java:6781-6797`): only the first `ready` counts.
    fn handle_ready(&mut self, c: &mut Call<'_>) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        c.send(Outbound::Version);
        c.send(Outbound::UpdateSupported);
        c.send(Outbound::UpdatePackageV2Supported);
        c.send(Outbound::AnalysisState { running: c.ctx.pondering });
        c.emit(SessionEvent::LoggingReady);
    }

    /// `clearReadBoardGmaAutoPlay`.
    fn clear_gma(&mut self, c: &mut Call<'_>, reason: &'static str) {
        self.auto_play = AutoPlayLine::default();
        c.emit(SessionEvent::GmaCleared { reason });
    }

    /// `restoreFloatBoardAfterPlaceResult`.
    fn restore_float_after_place(&mut self, c: &mut Call<'_>) {
        if self.float_board_created && self.hide_from_place {
            self.hide_from_place = false;
            c.emit(SessionEvent::FloatBoard(FloatBoardEvent::RestoreAfterPlace));
        }
    }

    /// `shutdown(requestGracefulExit)` (`ReadBoard.java:6105-6126`).
    fn shutdown_inner(&mut self, c: &mut Call<'_>, graceful: bool) {
        if self.shut_down {
            return;
        }
        self.shut_down = true;
        self.auto_play = AutoPlayLine::default();
        self.reset_active(false, false, false);
        self.restore_float_after_place(c);
        self.clear_resume_state();
        self.remote = RemoteContext::generic(false);
        self.frame.clear();
        self.invalid_frame = false;
        self.sync_board = false;
        self.both_sync = false;
        self.first_sync_jump = None;
        if graceful && !self.process_ended {
            c.send(Outbound::Quit);
        }
    }

    // ----------------------------------------------------------------------- state resets

    fn bump_epoch(&mut self) {
        self.epoch += 1;
        self.scheduled_resume = None;
    }

    /// `invalidatePendingSyncAnalysisResume`.
    fn invalidate_resume(&mut self, events: &mut Vec<SessionEvent>) {
        self.bump_epoch();
        events.push(SessionEvent::ResumeCancelled);
    }

    /// `onHistoryOverwritten`.
    fn overwritten(&mut self, events: &mut Vec<SessionEvent>) {
        events.push(SessionEvent::ResumeCancelled);
        self.clear_pending_tracking();
        self.clear_failed_state();
        self.clear_resume_state();
        self.rebuilt = None;
    }

    /// `resetActiveSyncState(preserveRecovery, preserveSuppression, preservePending)`.
    fn reset_active(&mut self, keep_recovery: bool, keep_suppression: bool, keep_pending: bool) {
        self.conflict = None;
        if !(keep_pending && self.is_pending()) {
            self.clear_pending_tracking();
        }
        if !(keep_suppression && self.local.suppression.is_some()) {
            self.clear_suppression();
        }
        if !keep_recovery {
            self.clear_recovery();
        }
        self.remote = RemoteContext::generic(false);
        self.turn_trusted = false;
        self.awaiting_first_sync_frame = true;
        self.bump_epoch();
    }

    /// `resetActiveSyncStateForReadBoardControlLine`: keeps a pending or failed local move.
    fn reset_for_control_line(&mut self) {
        let failed = self.local.recovery.is_some();
        let pending = self.is_pending();
        self.reset_active(failed, failed, pending);
    }

    /// `clearResumeState`.
    fn clear_resume_state(&mut self) {
        self.bump_epoch();
        self.resume = None;
        self.last_resolved = None;
    }

    /// `rememberResolvedSnapshotNode`.
    fn remember_resolved(&mut self, node: NodeId, remote: RemoteContext) {
        self.resume = Some((node, remote));
        self.last_resolved = Some(node);
        self.awaiting_first_sync_frame = false;
    }

    fn remember_resolved_now(&mut self, node: NodeId) {
        let remote = self.remote.with_force_rebuild(false);
        self.remember_resolved(node, remote);
    }

    // ---------------------------------------------------------------------- node facts

    fn rebuilt_facts(&self, rec: &GameRecord, id: NodeId) -> Option<&RebuiltFacts> {
        self.rebuilt
            .as_ref()
            .filter(|f| f.node == id && rec.contains(id) && rec.node(id).stamp == f.stamp)
    }

    /// `changeNextTurn` may give the rebuilt node a new stamp; it is still the same snapshot.
    fn refresh_rebuilt_stamp(&mut self, board: &dyn SyncBoard) {
        let rec = board.record();
        if let Some(f) = self.rebuilt.as_mut() {
            if rec.contains(f.node) && rec.node(f.node).kind == NodeKind::Snapshot {
                f.stamp = rec.node(f.node).stamp;
            }
        }
    }

    /// The step from `prev` (the parent's view, `None` for the root) into `id`.
    fn enter(&self, rec: &GameRecord, prev: Option<&NodeView>, id: NodeId) -> NodeView {
        let size = rec.size;
        let n = rec.node(id);
        let carry = rec.root_carry();
        let (stones, black, white, mut numbers) = match prev {
            Some(p) => (p.stones.clone(), p.black_captures, p.white_captures, p.move_numbers.clone()),
            None => (vec![None; size.area()], carry.black_captures, carry.white_captures, vec![0; size.area()]),
        };
        let mut pos = Position { size, stones, black_captures: black, white_captures: white };
        let before = pos.stones.clone();
        pos.enter(n);
        for (i, number) in numbers.iter_mut().enumerate() {
            if before[i] != pos.stones[i] {
                *number = 0;
            }
        }
        // The root keeps the numbers a rebuild took over (`moveNumberList` of the snapshot).
        if prev.is_none() && carry.move_numbers.len() == numbers.len() {
            for ((number, &carried), stone) in numbers.iter_mut().zip(&carry.move_numbers).zip(&pos.stones) {
                if stone.is_some() {
                    *number = carried;
                }
            }
        }
        let mut last_move = None;
        if let NodeKind::Move { color, point } = n.kind {
            if pos.at(point).is_some() {
                numbers[size.index(point)] = n.move_number;
            }
            last_move = Some((point, color));
        }
        if let Some(f) = self.rebuilt_facts(rec, id) {
            numbers.clone_from(&f.move_numbers);
            last_move = f.last_move;
            pos.black_captures = f.black_captures;
            pos.white_captures = f.white_captures;
        }
        NodeView {
            id,
            kind: n.kind,
            stones: pos.stones,
            black_captures: pos.black_captures,
            white_captures: pos.white_captures,
            move_numbers: numbers,
            last_move,
            to_play: n.to_play,
            move_number: n.move_number,
        }
    }

    /// Views of the nodes from the root to `id`.
    fn path_views(&self, rec: &GameRecord, id: NodeId) -> Vec<NodeView> {
        let mut views: Vec<NodeView> = Vec::new();
        for n in rec.path_to(id) {
            let view = self.enter(rec, views.last(), n);
            views.push(view);
        }
        views
    }

    fn view(&self, rec: &GameRecord, id: NodeId) -> NodeView {
        self.path_views(rec, id).pop().expect("a path holds its node")
    }

    /// `explicitPlayerOverride`: the node's declared `PL`.
    fn explicit_to_play(&self, rec: &GameRecord, id: NodeId) -> Option<Stone> {
        match self.rebuilt_facts(rec, id) {
            Some(f) => f.explicit_to_play,
            None => rec.node(id).explicit_to_play,
        }
    }

    /// `Board.hasStartStone`: the game starts from setup or handicap stones.
    fn has_start_setup(&self, rec: &GameRecord) -> bool {
        match self.rebuilt_facts(rec, ROOT) {
            Some(f) => f.keeps_start_setup,
            None => !rec.node(ROOT).setup.is_empty(),
        }
    }

    /// `hasRemovedStone || extraStones || hasSetupOrHandicapProperty` of one node.
    fn node_setup_risk(&self, rec: &GameRecord, id: NodeId) -> bool {
        if let Some(f) = self.rebuilt_facts(rec, id) {
            return f.setup_risk;
        }
        let n = rec.node(id);
        !n.setup.is_empty() || n.explicit_to_play.is_some() || has_setup_property(&n.properties)
    }

    /// `hasSetupOrHandicapTurnRisk`.
    fn setup_risk(&self, rec: &GameRecord, id: NodeId, delta: &SnapshotDelta) -> bool {
        if self.has_start_setup(rec) || delta.removals() > 0 || delta.additions > 1 {
            return true;
        }
        rec.path_to(id).into_iter().any(|n| self.node_setup_risk(rec, n))
    }

    // ------------------------------------------------------------------------ frames

    /// `syncBoardStones` (`ReadBoard.java:1650-1697`): the frame runs inside
    /// `Board.applyReadBoardSync` (collecting its resume target) unless GMA or a game owns the
    /// engine; while a normal auto-play intent drains it runs locally.
    fn sync_board_stones(&mut self, c: &mut Call<'_>) {
        if self.shut_down {
            return;
        }
        if c.ctx.normal_auto_play_intent && c.ctx.normal_auto_play_draining {
            let source = c.board.cursor();
            let writes = self.writes;
            self.collecting = Some(None);
            self.apply_sync(c);
            self.collecting = None;
            let target = c.board.cursor();
            if source != target || writes != self.writes {
                c.emit(SessionEvent::NormalAutoPlayTargetChanged { node: target });
            }
            return;
        }
        if self.auto_play.gma_active || c.ctx.gma_busy || c.ctx.playing_against_engine {
            self.apply_sync(c);
            return;
        }
        self.collecting = Some(None);
        self.apply_sync(c);
        if let Some(target) = self.collecting.take().flatten() {
            self.schedule_resume_now(c, target);
        }
    }

    /// `applySyncBoardStones` (`ReadBoard.java:1730-2045`).
    fn apply_sync(&mut self, c: &mut Call<'_>) {
        let size = c.board.record().size;
        let codes = std::mem::take(&mut self.frame);
        let cursor = c.board.cursor();
        let start_id = c.board.main_end();
        let start = self.view(c.board.record(), start_id);
        let remote = self.remote.clone();
        let fox = remote.recovery_move_number();
        let delta = summarize_delta(size, &start.stones, &codes);
        if !comparable(size, &codes, &start.stones) {
            return;
        }
        self.update_failed_suppression(size, &codes);
        if let Some(acked) = self.ack_if_caught_up(size, &codes) {
            self.update_turn_trust(c.board.record(), &start, &codes, &delta, &remote, fox, false);
            self.finish_after_ack(c, acked);
            return;
        }
        if self.retry_if_missing(c, size, &codes) {
            return;
        }

        let mut played = false;
        let mut need_resync = false;
        if self.allows_incremental(&delta, &start, &remote) {
            // The delta is valid, so no point of the frame contradicts a stone of the sync start
            // (Java's clear-and-resync branches inside this loop cannot run).
            let mut held: Option<(Point, Stone)> = None;
            for (index, &code) in codes.iter().enumerate() {
                let point = point_of(size, index);
                let stone = start.stones[index];
                if ((held.is_some() && code == 3) || code == 1) && stone != Some(Stone::Black) {
                    if !played {
                        c.board.move_to(start_id);
                    }
                    self.place_for_sync(c, Stone::Black, point);
                    played = true;
                }
                if ((held.is_some() && code == 4) || code == 2) && stone != Some(Stone::White) {
                    if !played {
                        c.board.move_to(start_id);
                    }
                    self.place_for_sync(c, Stone::White, point);
                    played = true;
                }
                if held.is_none() && code == 3 && stone != Some(Stone::Black) {
                    held = Some((point, Stone::Black));
                }
                if held.is_none() && code == 4 && stone != Some(Stone::White) {
                    held = Some((point, Stone::White));
                }
            }
            if let Some((point, color)) = held {
                if !played {
                    c.board.move_to(start_id);
                }
                self.place_for_sync(c, color, point);
                played = true;
                if self.settings.always_sync_board_stat || self.show_in_board {
                    self.show_line_end(c);
                }
            }
            let end = c.board.main_end();
            let after = self.view(c.board.record(), end);
            if has_diff(size, &codes, &after.stones, self.ignore_point()) {
                need_resync = true;
            }
        } else {
            need_resync = true;
        }

        let mut single = false;
        if need_resync {
            let decision = self.resolve_recovery(c, cursor, &start, &codes, &delta);
            c.emit(SessionEvent::Decision(SyncDecision { outcome: decision.outcome, reason: decision.reason }));
            match decision.outcome {
                RecoveryOutcome::Hold => {
                    self.resume_failed_after_hold(c);
                    return;
                }
                RecoveryOutcome::ForceRebuild => {
                    self.rebuild_from_snapshot(c, &start, &codes, &delta, fox);
                    return;
                }
                RecoveryOutcome::NoChange | RecoveryOutcome::SingleMoveRecovery => {}
            }
            single = decision.outcome == RecoveryOutcome::SingleMoveRecovery;
            let trust_view = match decision.resolved {
                Some(node) => self.view(c.board.record(), node),
                None => start.clone(),
            };
            self.update_turn_trust(c.board.record(), &trust_view, &codes, &delta, &remote, fox, single);
            if let (RecoveryOutcome::NoChange, Some(node)) = (decision.outcome, decision.resolved) {
                if node != c.board.cursor() {
                    c.board.move_to(node);
                }
                if decision.resume {
                    self.remember_resolved_now(node);
                    self.schedule_resume(c, node);
                }
            }
            played = single;
        }

        let end = c.board.main_end();
        if played && !single {
            self.remember_resolved_now(end);
        }
        if single && c.board.cursor() != end {
            c.board.move_to(end);
        }
        let end_view = self.view(c.board.record(), end);
        if self.should_rebuild_for_fox_metadata(c.board.record(), &end_view, &remote, &codes, &delta) {
            self.rebuild_from_snapshot(c, &end_view, &codes, &delta, fox);
            return;
        }
        self.update_turn_trust(c.board.record(), &end_view, &codes, &delta, &remote, fox, played);
        // `applySyncViewState`: in the float board's edit mode the main line grows but the view
        // stays on the node the user was looking at.
        if c.ctx.float_edit_mode
            && played
            && !single
            && c.board.record().contains(cursor)
            && c.board.cursor() != cursor
        {
            c.board.move_to(cursor);
        }
        self.conflict = None;
        self.awaiting_first_sync_frame = false;
        if self.collecting.is_some() {
            if played && !single {
                let current = c.board.cursor();
                self.schedule_resume(c, current);
            }
            self.first_sync = false;
        }
        if self.first_sync {
            self.first_sync = false;
            if let Some(parent) = c.board.record().parent(c.board.cursor()) {
                c.board.move_to(parent);
            }
            self.first_sync_jump = Some(c.now + FIRST_SYNC_LAST_MOVE_DELAY);
        }
        let current = c.board.cursor();
        self.continue_game(c, "sync", current);
    }

    /// `placeForSync`; Java's `Board.placeForSync` also clears `lastMovePlayByLizzie`.
    fn place_for_sync(&mut self, c: &mut Call<'_>, color: Stone, point: Point) -> Option<NodeId> {
        let node = c.board.place_for_sync(color, point);
        self.writes += 1;
        self.local.by_client = false;
        node
    }

    /// `Lizzie.frame.lastMove()`: follow the shown line to its end.
    fn show_line_end(&mut self, c: &mut Call<'_>) {
        let start = c.board.cursor();
        let rec = c.board.record();
        let mut node = start;
        while let Some(child) = next_excluding_end_placeholder(rec, node) {
            node = child;
        }
        if node != start {
            c.board.move_to(node);
        }
    }

    /// `allowsIncrementalSync`: one marked stone, an unchanged frame, or a Fox markerless single
    /// step whose move number is the next one.
    fn allows_incremental(&self, delta: &SnapshotDelta, start: &NodeView, remote: &RemoteContext) -> bool {
        if delta.allows_incremental_sync() {
            return true;
        }
        match remote.recovery_move_number() {
            Some(n) => delta.has_only_additions() && delta.additions == 1 && i64::from(n) == i64::from(start.move_number) + 1,
            None => false,
        }
    }

    /// The pending local stone a frame may not show yet (`shouldIgnoreCurrentLastLocalMove`).
    fn ignore_point(&self) -> Option<Point> {
        if self.both_sync && self.is_pending() && self.has_pending_record() {
            self.local.point
        } else {
            None
        }
    }

    // --------------------------------------------------------------------- recovery

    /// `resolveCompleteSnapshotRecovery` (`ReadBoard.java:2227-2301`).
    fn resolve_recovery(
        &mut self,
        c: &mut Call<'_>,
        cursor: NodeId,
        start: &NodeView,
        codes: &[u8],
        delta: &SnapshotDelta,
    ) -> Decision {
        let remote = self.remote.clone();
        let fox = remote.recovery_move_number();
        if remote.force_rebuild {
            return Decision::rebuild("remote_force_rebuild_requested");
        }
        if fox.is_some() && self.resume.as_ref().is_some_and(|(_, r)| r.conflicts_with(&remote)) {
            return Decision::rebuild("resume_context_conflict");
        }
        if self.is_pending() && self.has_pending_record() {
            return Decision::hold();
        }
        let rec = c.board.record();
        let size = rec.size;
        // The live title can lag the board image by one move after the move was acknowledged:
        // keep that proven move instead of flattening it.
        if let Some(n) = fox {
            if !self.awaiting_first_sync_frame
                && self.resume.as_ref().is_some_and(|(node, _)| *node == cursor)
                && cursor == start.id
                && start.is_move()
                && remote.window == WindowKind::LiveRoom
                && i64::from(n) == i64::from(start.move_number) - 1
                && matches_stones(size, &start.stones, codes, true)
            {
                return Decision::no_change(cursor, false, "snapshot_matches_accepted_move_with_lagging_title");
            }
        }
        let matching = if fox.is_some() {
            self.find_in_mainline_window(rec, cursor, start.id, codes, &remote)
        } else {
            self.find_matching_history_node(rec, start.id, codes, &remote)
        };
        if let Some(node) = matching {
            return Decision::no_change(node, node != cursor || self.awaiting_first_sync_frame, "snapshot_matches_existing_node");
        }
        if let Some(node) = self.find_adjacent_to_resolved(rec, codes, &remote) {
            return Decision::no_change(
                node,
                node != cursor || self.awaiting_first_sync_frame,
                "snapshot_matches_adjacent_resolved_node",
            );
        }
        if delta.has_marker() && self.try_single_move_recovery(c, start, codes) {
            return Decision {
                outcome: RecoveryOutcome::SingleMoveRecovery,
                resolved: None,
                resume: false,
                reason: "single_move_recovery",
            };
        }
        let force_now = if self.awaiting_first_sync_frame {
            fox.is_some()
        } else {
            fox.is_some_and(|n| i64::from(n) != i64::from(start.move_number))
        };
        if force_now {
            return Decision::rebuild(if self.awaiting_first_sync_frame {
                "first_sync_force_rebuild"
            } else {
                "fallback_force_rebuild"
            });
        }
        if self.hold_conflicting(c.board.record(), start.id, codes, &remote) {
            return Decision::hold();
        }
        Decision::rebuild("fallback_force_rebuild")
    }

    /// `matchesRemoteIdentity`: same stones, and the Fox move number or the marked last move.
    fn matches_identity(&self, size: BoardSize, view: &NodeView, codes: &[u8], remote: &RemoteContext) -> bool {
        let fox = remote.recovery_move_number();
        if !matches_stones(size, &view.stones, codes, fox.is_some()) {
            return false;
        }
        if let Some(n) = fox {
            return i64::from(view.move_number) == i64::from(n);
        }
        match snapshot_marker(size, codes) {
            Err(()) => false,
            Ok(None) => true,
            Ok(Some(marker)) => view.last_move == Some(marker),
        }
    }

    /// `findMatchingHistoryNode`: Fox frames scan the ancestors, others check the node itself.
    fn find_matching_history_node(&self, rec: &GameRecord, start: NodeId, codes: &[u8], remote: &RemoteContext) -> Option<NodeId> {
        if codes.is_empty() {
            return None;
        }
        let views = self.path_views(rec, start);
        if remote.supports_fox_recovery() {
            views.iter().rev().find(|v| self.matches_identity(rec.size, v, codes, remote)).map(|v| v.id)
        } else {
            views.last().filter(|v| self.matches_identity(rec.size, v, codes, remote)).map(|v| v.id)
        }
    }

    /// `findMatchingNodeInMainlineWindow`: the shown node or an ancestor, then the main line ahead
    /// of it up to the main end.
    fn find_in_mainline_window(
        &self,
        rec: &GameRecord,
        current: NodeId,
        main_end: NodeId,
        codes: &[u8],
        remote: &RemoteContext,
    ) -> Option<NodeId> {
        if codes.is_empty() || !remote.supports_fox_recovery() {
            return None;
        }
        if let Some(node) = self.find_matching_history_node(rec, current, codes, remote) {
            return Some(node);
        }
        let mut view = self.view(rec, current);
        let mut node = current;
        while let Some(next) = next_main_line_node(rec, node) {
            view = self.enter(rec, Some(&view), next);
            if self.matches_identity(rec.size, &view, codes, remote) {
                return Some(next);
            }
            if next == main_end {
                break;
            }
            node = next;
        }
        None
    }

    /// `findAdjacentMatchFromLastResolvedNode`: the main-line node after the last resolved one.
    fn find_adjacent_to_resolved(&self, rec: &GameRecord, codes: &[u8], remote: &RemoteContext) -> Option<NodeId> {
        let (node, resolved_remote) = self.resume.as_ref()?;
        if !remote.supports_fox_recovery() || resolved_remote.conflicts_with(remote) || !rec.contains(*node) {
            return None;
        }
        let next = next_main_line_node(rec, *node)?;
        let view = self.view(rec, next);
        self.matches_identity(rec.size, &view, codes, remote).then_some(next)
    }

    /// `tryApplySingleMoveRecovery` (`ReadBoard.java:5404-5441`): one move with captures turns the
    /// main end into the frame.
    fn try_single_move_recovery(&mut self, c: &mut Call<'_>, start: &NodeView, codes: &[u8]) -> bool {
        let rec = c.board.record();
        let Some((point, color)) = find_single_move_capture(rec.size, &start.stones, codes) else {
            return false;
        };
        if !self.can_apply_single_move(rec, start, point, color, codes, c.ctx) {
            return false;
        }
        c.board.move_to(start.id);
        if self.collecting.is_none() && c.ctx.gma_in_flight {
            c.board.place_for_sync(color, point);
            self.writes += 1;
            let end = c.board.main_end();
            self.remember_resolved_now(end);
            c.emit(SessionEvent::GmaRestoreRequested { node: end, reason: "single-move-recovery" });
            return true;
        }
        self.place_for_sync(c, color, point);
        if self.settings.always_sync_board_stat || self.show_in_board {
            self.show_line_end(c);
        }
        let end = c.board.main_end();
        self.remember_resolved_now(end);
        self.schedule_resume(c, end);
        true
    }

    /// `canApplySingleMoveRecovery`: legal under the current rules (ko, suicide, no-capture) and
    /// equal to the frame.
    fn can_apply_single_move(
        &self,
        rec: &GameRecord,
        start: &NodeView,
        point: Point,
        color: Stone,
        codes: &[u8],
        ctx: &HostContext,
    ) -> bool {
        let size = rec.size;
        if start.stones[size.index(point)].is_some() {
            return false;
        }
        let mut pos = Position { size, stones: start.stones.clone(), black_captures: 0, white_captures: 0 };
        let suicide = if ctx.no_capture {
            pos.put(color, point);
            0
        } else {
            pos.play(color, point)
        };
        if let Some(parent) = rec.parent(start.id) {
            if self.view(rec, parent).stones == pos.stones {
                return false;
            }
        }
        let suicide_refused = if ctx.allow_suicide { suicide == 1 } else { suicide > 0 };
        if suicide_refused {
            return false;
        }
        !has_diff(size, codes, &pos.stones, self.ignore_point())
    }

    /// `shouldHoldConflictingSnapshot`: a conflict is held once; the same conflict again rebuilds.
    fn hold_conflicting(&mut self, rec: &GameRecord, start: NodeId, codes: &[u8], remote: &RemoteContext) -> bool {
        if rec.parent(start).is_none() {
            return false;
        }
        let key = conflict_key(codes, remote);
        if self.conflict.as_deref() == Some(key.as_str()) {
            self.conflict = None;
            false
        } else {
            self.conflict = Some(key);
            true
        }
    }

    /// `resumeFailedLocalMoveAfterRecoveryHoldIfNeeded`.
    fn resume_failed_after_hold(&mut self, c: &mut Call<'_>) {
        if self.local.recovery.is_none() {
            return;
        }
        let held = c.board.main_end();
        if self.local.awaiting_observation {
            self.continue_game(c, "sync-hold-observation", held);
            return;
        }
        self.resume_failed_after_sync(c, "sync-hold", held);
    }

    /// `shouldRebuildForFoxMetadataChange`: the stones match but the Fox move number or the side
    /// to play of a snapshot main end is wrong.
    fn should_rebuild_for_fox_metadata(
        &self,
        rec: &GameRecord,
        end: &NodeView,
        remote: &RemoteContext,
        codes: &[u8],
        delta: &SnapshotDelta,
    ) -> bool {
        let Some(n) = remote.recovery_move_number() else { return false };
        if has_diff(rec.size, codes, &end.stones, None) || end.kind.is_action() {
            return false;
        }
        let number_differs = i64::from(end.move_number) != i64::from(n);
        match self.expected_black_to_play(rec, end, remote, delta) {
            None => number_differs,
            Some(black) => number_differs || (end.to_play == Stone::Black) != black,
        }
    }

    fn expected_black_to_play(&self, rec: &GameRecord, end: &NodeView, remote: &RemoteContext, delta: &SnapshotDelta) -> Option<bool> {
        let fox = remote.recovery_move_number();
        if let Some(pl) = self.explicit_to_play(rec, end.id) {
            return Some(pl != Stone::White);
        }
        if fox_zero_move_handicap(&end.stones, delta, remote, remote.last_move_source, fox) {
            return Some(false);
        }
        if delta.has_marker() && remote.last_move_source.is_trusted_visual_marker() {
            return delta.marker().map(|(_, color)| color == Stone::White);
        }
        if self.markerless_fox_fallback(rec, end.id, delta, remote.last_move_source, fox) {
            return fox.map(|n| n % 2 == 0);
        }
        None
    }

    // ------------------------------------------------------------------------ rebuild

    /// `rebuildFromSnapshot` (`ReadBoard.java:2345-2407`).
    fn rebuild_from_snapshot(
        &mut self,
        c: &mut Call<'_>,
        start: &NodeView,
        codes: &[u8],
        delta: &SnapshotDelta,
        fox: Option<i32>,
    ) {
        let resolved = self.remote.with_force_rebuild(false);
        let engine_available = c.ctx.engine_available;
        if self.local.recovery.is_some() {
            let keep_suppression = self.local.suppression.is_some();
            self.reset_active(true, keep_suppression, false);
        } else {
            self.reset_active(true, false, false);
        }
        let rec = c.board.record();
        let had_start_setup = self.has_start_setup(rec);
        let Some((snapshot, setup_risk)) = self.build_snapshot(rec, start, codes, delta, fox, &resolved, had_start_setup) else {
            return;
        };
        let facts = RebuiltFacts {
            node: ROOT,
            stamp: 0,
            last_move: snapshot.last_move,
            black_captures: snapshot.black_captures,
            white_captures: snapshot.white_captures,
            move_numbers: snapshot.move_numbers.clone(),
            explicit_to_play: snapshot.explicit_to_play,
            setup_risk,
            keeps_start_setup: snapshot.keeps_start_setup,
        };
        let node = c.board.rebuild(snapshot);
        self.writes += 1;
        let stamp = c.board.record().node(node).stamp;
        self.rebuilt = Some(RebuiltFacts { node, stamp, ..facts });
        let view = self.view(c.board.record(), node);
        self.update_turn_trust(c.board.record(), &view, codes, delta, &resolved, fox, false);
        self.remember_resolved(node, resolved);
        if engine_available && !self.continue_game(c, "rebuild", node) {
            self.schedule_resume(c, node);
        }
        self.first_sync = false;
    }

    /// `buildSnapshotBoardData` + `copySnapshotSetupMetadata` + the start-setup rule. `None`
    /// when the anchor's `MN` is not a number (Java throws and the frame is abandoned).
    #[allow(clippy::too_many_arguments)]
    fn build_snapshot(
        &self,
        rec: &GameRecord,
        start: &NodeView,
        codes: &[u8],
        delta: &SnapshotDelta,
        fox: Option<i32>,
        remote: &RemoteContext,
        had_start_setup: bool,
    ) -> Option<(RebuiltSnapshot, bool)> {
        let source = remote.last_move_source;
        let stones = frame_stones(codes);
        let move_number = self.infer_move_number(start, &stones, delta, fox);
        let mut to_play = if self.infer_black_to_play(rec, start, &stones, delta, fox, remote, source) {
            Stone::Black
        } else {
            Stone::White
        };
        let last_move = delta.marker().filter(|_| delta.has_marker());
        let mut move_numbers = vec![0; stones.len()];
        for (i, number) in move_numbers.iter_mut().enumerate() {
            let n = start.move_numbers[i];
            if stones[i] == start.stones[i] && n > 0 && n <= move_number {
                *number = n;
            }
        }
        if let Some((point, _)) = last_move {
            move_numbers[rec.size.index(point)] = move_number;
        }
        let (black_captures, white_captures) = if stones == start.stones {
            (start.black_captures, start.white_captures)
        } else {
            (0, 0)
        };
        let anchor = rec.path_to(start.id).into_iter().rev().find(|&n| rec.node(n).kind == NodeKind::Snapshot);
        let mut explicit_to_play = None;
        let mut metadata = None;
        let mut setup_risk = false;
        if let Some(anchor) = anchor {
            let node = rec.node(anchor);
            explicit_to_play = self.explicit_to_play(rec, anchor);
            if let Some(pl) = explicit_to_play {
                to_play = pl;
            }
            if let Some(mn) = node.properties.iter().find(|p| p.id == "MN").and_then(|p| p.values.first()) {
                if !mn.is_empty() && mn.parse::<i32>().is_err() {
                    return None;
                }
            }
            let setup = match self.rebuilt_facts(rec, anchor) {
                // The rebuilt root's own setup holds the whole position; what it declared was
                // only the metadata copied into it, which `setup_risk` remembers.
                Some(_) => Setup::default(),
                None => node.setup.clone(),
            };
            setup_risk = self.node_setup_risk(rec, anchor) || has_setup_property(&node.properties);
            metadata = Some(SnapshotMetadata { comment: node.comment.clone(), properties: node.properties.clone(), setup });
        }
        let keeps_start_setup = had_start_setup && anchor.is_none_or(|a| rec.parent(a).is_none());
        Some((
            RebuiltSnapshot {
                stones,
                to_play,
                explicit_to_play,
                move_number,
                last_move,
                move_numbers,
                black_captures,
                white_captures,
                metadata,
                keeps_start_setup,
            },
            setup_risk,
        ))
    }

    /// `inferSnapshotMoveNumber`.
    fn infer_move_number(&self, start: &NodeView, stones: &[Option<Stone>], delta: &SnapshotDelta, fox: Option<i32>) -> u32 {
        if let Some(n) = fox {
            return n.max(0) as u32;
        }
        if stones.iter().all(Option::is_none) {
            return 0;
        }
        if let Some((_, marker)) = delta.marker() {
            if delta.changed_stones() == 0 {
                return start.move_number;
            }
            if delta.has_only_additions() {
                return start.move_number + delta.additions;
            }
            let occupied = stones.iter().filter(|s| s.is_some()).count() as u32;
            let black_to_play = marker == Stone::White;
            return if occupied.is_multiple_of(2) == black_to_play { occupied } else { occupied + 1 };
        }
        start.move_number
    }

    /// `inferSnapshotBlackToPlay`.
    #[allow(clippy::too_many_arguments)]
    fn infer_black_to_play(
        &self,
        rec: &GameRecord,
        start: &NodeView,
        stones: &[Option<Stone>],
        delta: &SnapshotDelta,
        fox: Option<i32>,
        remote: &RemoteContext,
        source: LastMoveSource,
    ) -> bool {
        if stones.iter().all(Option::is_none) {
            return true;
        }
        if fox_zero_move_handicap(stones, delta, remote, source, fox) {
            return false;
        }
        if delta.has_marker() && source.is_trusted_visual_marker() {
            return delta.marker().is_some_and(|(_, color)| color == Stone::White);
        }
        if self.markerless_fox_fallback(rec, start.id, delta, source, fox) {
            return fox.is_some_and(|n| n % 2 == 0);
        }
        start.to_play == Stone::Black
    }

    // ---------------------------------------------------------------------- turn trust

    /// `updateReadBoardTurnTrustFromAcceptedFrame` (`ReadBoard.java:2735-2776`).
    #[allow(clippy::too_many_arguments)]
    fn update_turn_trust(
        &mut self,
        rec: &GameRecord,
        node: &NodeView,
        codes: &[u8],
        delta: &SnapshotDelta,
        remote: &RemoteContext,
        fox: Option<i32>,
        accepted_real_move: bool,
    ) {
        let source = remote.last_move_source;
        let unchanged = !delta.has_marker() && delta.changed_stones() == 0;
        let stones = frame_stones(codes);
        self.turn_trusted = (accepted_real_move && !remote.supports_fox_recovery())
            || self.explicit_to_play(rec, node.id).is_some()
            || (delta.has_marker() && source.is_trusted_visual_marker())
            || (node.kind == NodeKind::Snapshot && unchanged && !self.setup_risk(rec, node.id, delta))
            || (unchanged && stones.iter().all(Option::is_none) && !self.setup_risk(rec, node.id, delta))
            || fox_zero_move_handicap(&stones, delta, remote, source, fox)
            || self.markerless_fox_fallback(rec, node.id, delta, source, fox);
    }

    /// `isMarkerlessOrdinaryFoxTurnFallback`: a markerless single stone whose colour fits the Fox
    /// move number, without setup stones around.
    fn markerless_fox_fallback(
        &self,
        rec: &GameRecord,
        node: NodeId,
        delta: &SnapshotDelta,
        source: LastMoveSource,
        fox: Option<i32>,
    ) -> bool {
        let Some(n) = fox else { return false };
        let expected = if n % 2 == 1 { Stone::Black } else { Stone::White };
        !delta.has_marker()
            && matches!(source, LastMoveSource::None | LastMoveSource::LegacyUnknown)
            && delta.removals() == 0
            && delta.additions == 1
            && delta.single_addition_color() == Some(expected)
            && !self.setup_risk(rec, node, delta)
    }

    // ------------------------------------------------------------- analysis resume, games

    /// `scheduleResumeAnalysisAfterSync(target)`: inside a collecting frame only the last target
    /// counts.
    fn schedule_resume(&mut self, c: &mut Call<'_>, target: NodeId) {
        if c.ctx.normal_auto_play_intent {
            return;
        }
        if let Some(slot) = self.collecting.as_mut() {
            *slot = Some(target);
            return;
        }
        self.schedule_resume_now(c, target);
    }

    /// `scheduleResumeAnalysisAfterSync(target, confirmed)`: resume 200 ms later if nothing
    /// invalidated it meanwhile.
    fn schedule_resume_now(&mut self, c: &mut Call<'_>, target: NodeId) {
        if c.ctx.normal_auto_play_intent || !c.ctx.engine_available {
            return;
        }
        self.bump_epoch();
        self.scheduled_resume = Some(ScheduledResume { epoch: self.epoch, node: target, at: c.now + SYNC_ANALYSIS_RESUME_DELAY });
    }

    /// `resumeAnalysisAfterSyncIfStillCurrent` (`ReadBoard.java:5905-5967`).
    fn resume_if_still_current(&mut self, c: &mut Call<'_>, resume: ScheduledResume) {
        if self.shut_down || resume.epoch != self.epoch {
            return;
        }
        if self.auto_play.gma_active {
            c.emit(SessionEvent::ScheduleGma { reason: "resume-analysis-after-sync".to_owned() });
            return;
        }
        if !self.read_board_ponder || !c.ctx.engine_available || c.board.cursor() != resume.node {
            return;
        }
        if c.ctx.analysis_game {
            self.resume_auto_play(c, "confirmed-sync", resume.node, true);
            return;
        }
        if c.ctx.can_resume_analysis {
            c.emit(SessionEvent::ResumeAnalysis { node: resume.node });
        }
    }

    /// `continueGameAfterSyncIfNeeded` (`ReadBoard.java:5558-5576`): whether a game or a failed
    /// local move took over after the sync.
    fn continue_game(&mut self, c: &mut Call<'_>, reason: &'static str, target: NodeId) -> bool {
        c.emit(SessionEvent::Synced { node: target, reason });
        if c.ctx.gma_restore_pending {
            return true;
        }
        if self.resume_failed_after_sync(c, reason, target) {
            return true;
        }
        if self.continue_playing_against_engine(c) {
            return true;
        }
        self.resume_auto_play(c, reason, target, false)
    }

    /// `resumeFailedLocalMoveAfterSyncIfNeeded` (`ReadBoard.java:5585-5732`).
    fn resume_failed_after_sync(&mut self, c: &mut Call<'_>, reason: &str, target: NodeId) -> bool {
        if self.local.recovery.is_none() {
            return false;
        }
        if self.local.awaiting_observation && !self.release_failed_local_move_observation_if_elapsed(c.now) {
            return false;
        }
        let Some(failed) = self.local.recovery else { return false };
        if c.board.record().node(target).to_play != failed.color {
            self.local.awaiting_observation = false;
            self.local.waiting_our_turn = true;
            return true;
        }
        self.local.awaiting_observation = false;
        self.local.waiting_our_turn = false;
        if !c.ctx.engine_available {
            return true;
        }
        if c.ctx.playing_against_engine {
            let engine_side = (failed.color == Stone::Black) != c.ctx.player_is_black;
            self.clear_failed_state();
            if !engine_side {
                return false;
            }
            c.emit(SessionEvent::Genmove { color: failed.color });
            self.need_genmove = false;
            return true;
        }
        if c.ctx.analysis_game {
            self.clear_failed_state();
            if self.auto_play.gma_active {
                c.emit(SessionEvent::ScheduleGma { reason: format!("{reason}-failed-local-recovery") });
                return true;
            }
            if !self.read_board_ponder {
                return true;
            }
            return self.resume_auto_play(c, reason, target, false);
        }
        self.clear_failed_state();
        false
    }

    /// `continuePlayingAgainstLeelazAfterSyncIfNeeded`: the engine's turn after a `place` in a
    /// human-against-engine game.
    fn continue_playing_against_engine(&mut self, c: &mut Call<'_>) -> bool {
        if !c.ctx.playing_against_engine || !self.need_genmove || !c.ctx.engine_available {
            return false;
        }
        let to_play = c.board.record().node(c.board.cursor()).to_play;
        if (to_play == Stone::Black) == c.ctx.player_is_black {
            return false;
        }
        c.emit(SessionEvent::Genmove { color: to_play });
        self.need_genmove = false;
        true
    }

    /// `resumeAutoPlayAnalysisAfterSyncIfNeeded` (`ReadBoard.java:5799-5903`).
    fn resume_auto_play(&mut self, c: &mut Call<'_>, reason: &str, target: NodeId, confirmed: bool) -> bool {
        if c.ctx.normal_auto_play_intent {
            return true;
        }
        if !c.ctx.analysis_game {
            return false;
        }
        if self.auto_play.gma_active {
            c.emit(SessionEvent::ScheduleGma { reason: format!("{reason}-resume-auto-play") });
            return true;
        }
        if self.shut_down || !c.ctx.can_resume_auto_play_analysis {
            return true;
        }
        if self.local.recovery.is_some() {
            if !self.local.awaiting_observation {
                return true;
            }
            self.release_failed_local_move_observation_if_elapsed(c.now);
            if self.local.recovery.is_some() && !self.local.awaiting_observation {
                return true;
            }
        }
        if !self.read_board_ponder {
            return true;
        }
        if !c.ctx.engine_available {
            return false;
        }
        if self.collecting.is_some() {
            if reason != "sync" {
                self.schedule_resume(c, target);
            }
            return true;
        }
        if !confirmed && c.ctx.pondering {
            return true;
        }
        if confirmed {
            c.emit(SessionEvent::ResumeAnalysis { node: target });
        } else {
            self.schedule_resume(c, target);
        }
        true
    }

    // ----------------------------------------------------------------- local moves

    /// `isPendingLocalMoveAwaitingReadBoard`.
    fn is_pending(&self) -> bool {
        self.local.by_client && self.local.waiting
    }

    /// `hasPendingLocalMoveRecord`.
    fn has_pending_record(&self) -> bool {
        self.local.node.is_some() && self.local.point.is_some() && self.local.color.is_some()
    }

    /// `isCurrentLocalMoveConfirmation`: the pending move is still the main end of a session that
    /// may confirm it.
    fn is_current_confirmation(&self, board: &dyn SyncBoard) -> bool {
        self.has_local_move_confirmation_authority()
            && self.is_pending()
            && self.local.node.is_some_and(|n| board.main_end() == n)
    }

    /// `startTrackingLocalMoveFromLizzie` + `capturePendingLocalMoveRecord`.
    fn start_tracking(&mut self, c: &mut Call<'_>) {
        let rec = c.board.record();
        let end = c.board.main_end();
        let mut node = None;
        let mut point = None;
        let mut color = None;
        let mut baseline_key = String::new();
        if let NodeKind::Move { color: col, point: p } = rec.node(end).kind {
            node = Some(end);
            point = Some(p);
            color = Some(col);
            if let Some(parent) = rec.parent(end) {
                baseline_key = stones_key(&self.view(rec, parent).stones);
            }
        }
        let local = &mut self.local;
        local.node = node;
        local.point = point;
        local.color = color;
        local.baseline_key = baseline_key;
        local.by_client = true;
        local.waiting = true;
        local.click_completed = false;
        local.ignore_current = local.ignore_next;
        local.ignore_next = false;
        local.last_attempt = Some(c.now);
        local.ack_deadline = Some(c.now + PENDING_LOCAL_MOVE_ACK_TIMEOUT);
    }

    /// `markLocalMoveCommandCompleted`: `placeComplete` only notes the click; a later `place`
    /// never takes a result meant for an earlier one (`ignoreReadBoardPlaceResultsFor…`).
    fn mark_click_completed(&mut self) {
        if !self.is_pending() {
            self.local.click_completed = false;
            return;
        }
        if self.local.ignore_current {
            return;
        }
        self.local.click_completed = true;
    }

    /// `clearPendingLocalMoveTracking`.
    fn clear_pending_tracking(&mut self) {
        let was_pending = self.is_pending();
        let local = &mut self.local;
        local.by_client = false;
        local.waiting = false;
        local.click_completed = false;
        local.last_attempt = None;
        local.node = None;
        local.point = None;
        local.color = None;
        local.baseline_key.clear();
        local.ack_deadline = None;
        local.place_after_hide = None;
        local.ignore_current = false;
        if was_pending {
            local.ignore_next = true;
        }
    }

    /// `isCurrentPendingLocalMovePresentInSnapshot`.
    fn present_in_frame(&self, size: BoardSize, codes: &[u8]) -> bool {
        match (self.has_pending_record(), self.local.point, self.local.color) {
            (true, Some(point), Some(color)) => codes.get(size.index(point)).and_then(|&c| code_stone(c)) == Some(color),
            _ => false,
        }
    }

    /// `acknowledgeLocalMoveIfSnapshotCaughtUp`: the frame shows the pending stone. Returns the
    /// confirmed move's node.
    fn ack_if_caught_up(&mut self, size: BoardSize, codes: &[u8]) -> Option<Option<NodeId>> {
        if !self.has_local_move_confirmation_authority() || !self.local.by_client || !self.has_pending_record() {
            return None;
        }
        if !self.present_in_frame(size, codes) {
            return None;
        }
        let node = self.local.node;
        self.clear_pending_tracking();
        Some(node)
    }

    /// `finishSyncAfterAcknowledgedLocalMoveSnapshot`.
    fn finish_after_ack(&mut self, c: &mut Call<'_>, acked: Option<NodeId>) {
        let end = c.board.main_end();
        self.remember_resolved_now(end);
        self.conflict = None;
        self.awaiting_first_sync_frame = false;
        self.bump_epoch();
        c.emit(SessionEvent::LocalMoveConfirmed { node: acked.unwrap_or(end) });
        if !self.continue_game(c, "ack", end) {
            self.schedule_resume(c, end);
        }
    }

    /// `retryPendingLocalMoveIfSnapshotStillMissing` without the retry Java never runs
    /// (`LOCAL_MOVE_PLACE_RETRY_LIMIT = 0`): a frame without the pending stone fails it when the
    /// rest of the board moved on or the place result is overdue. Returns whether it failed.
    fn retry_if_missing(&mut self, c: &mut Call<'_>, size: BoardSize, codes: &[u8]) -> bool {
        if !self.is_current_confirmation(&*c.board) || self.present_in_frame(size, codes) {
            return false;
        }
        let key = position_key(codes);
        let baseline = &self.local.baseline_key;
        let failure = if !baseline.is_empty() && !key.is_empty() && *baseline != key {
            Some("pending local move remote changed without target".to_owned())
        } else {
            self.local.last_attempt.and_then(|at| {
                let elapsed = c.now.saturating_duration_since(at);
                (elapsed >= PENDING_LOCAL_MOVE_ACK_TIMEOUT).then(|| {
                    format!("pending local move timed out without place result elapsedMs={}", elapsed.as_millis())
                })
            })
        };
        match failure {
            Some(reason) => self.handle_failure(c, reason),
            None => false,
        }
    }

    /// The scheduled `failPendingLocalMoveIfAckTimedOutWithoutSnapshot`.
    fn fail_if_ack_timed_out(&mut self, c: &mut Call<'_>) {
        if !self.is_current_confirmation(&*c.board) {
            return;
        }
        let Some(at) = self.local.last_attempt else { return };
        let elapsed = c.now.saturating_duration_since(at);
        if elapsed < PENDING_LOCAL_MOVE_ACK_TIMEOUT {
            return;
        }
        self.handle_failure(c, format!("pending local move timed out without sync frame elapsedMs={}", elapsed.as_millis()));
    }

    /// `handlePendingLocalMovePlacementFailure` (`ReadBoard.java:1275-1328`): roll the move back
    /// from the main end, suppress it, and hold every local move until a frame was observed or
    /// the grace passed. No message is shown.
    fn handle_failure(&mut self, c: &mut Call<'_>, reason: String) -> bool {
        if !self.is_current_confirmation(&*c.board) {
            return false;
        }
        match (self.has_pending_record(), self.local.point, self.local.color) {
            (true, Some(point), Some(color)) => {
                self.local.suppression = Some(Suppression { point, color });
                self.local.suppression_key.clear();
                self.local.recovery = Some(Suppression { point, color });
                self.local.awaiting_observation = false;
                self.local.waiting_our_turn = false;
                self.local.observation_deadline = None;
            }
            _ => self.clear_failed_state(),
        }
        let mut rollback = None;
        if let (Some(failed), Some(node)) = (self.local.recovery, self.local.node) {
            let matches = matches!(
                c.board.record().node(node).kind,
                NodeKind::Move { point, color } if point == failed.point && color == failed.color
            );
            if matches {
                rollback = c.board.discard_failed_move(node);
                if rollback.is_some() {
                    self.writes += 1;
                }
            }
        }
        self.clear_pending_tracking();
        if self.local.recovery.is_some() {
            self.local.awaiting_observation = true;
            self.local.waiting_our_turn = false;
            self.local.observation_deadline = Some(c.now + FAILED_LOCAL_MOVE_OBSERVATION_GRACE);
        }
        if let (Some(node), Some(_)) = (rollback, self.local.suppression) {
            self.local.suppression_key = stones_key(&self.view(c.board.record(), node).stones);
        }
        self.restore_float_after_place(c);
        c.emit(SessionEvent::LocalMoveFailed { node: rollback, reason });
        if rollback.is_some() {
            let current = c.board.cursor();
            if c.ctx.gma_busy || c.ctx.gma_restore_pending {
                c.emit(SessionEvent::GmaRestoreRequested { node: current, reason: "placeFailed" });
            } else if c.ctx.engine_available && self.has_local_move_confirmation_authority() {
                self.continue_game(c, "placeFailed", current);
            }
        }
        true
    }

    /// `shouldSuppressLocalPlaceAfterFailedSync` (`ReadBoard.java:3850-3929`).
    fn should_suppress_after_failed(&mut self, point: Point, color: Stone, now: Instant) -> bool {
        let Some(failed) = self.local.suppression else { return false };
        if !self.has_local_move_confirmation_authority() {
            return false;
        }
        if self.local.awaiting_observation {
            return !self.release_failed_local_move_observation_if_elapsed(now);
        }
        if self.local.waiting_our_turn {
            return true;
        }
        if failed.point == point && failed.color == color {
            return true;
        }
        self.clear_failed_state();
        false
    }

    /// `updateFailedLocalMoveSuppressionForSnapshot` + `observeFailedLocalMoveSnapshot`: a frame
    /// showing the failed stone, or the same board as after the rollback, ends the failure; a
    /// changed board waits for the failed side's turn.
    fn update_failed_suppression(&mut self, size: BoardSize, codes: &[u8]) {
        let Some(failed) = self.local.suppression else { return };
        if codes.is_empty() {
            return;
        }
        let key = position_key(codes);
        let visible = codes.get(size.index(failed.point)).and_then(|&c| code_stone(c)) == Some(failed.color);
        if self.local.awaiting_observation {
            if visible || self.local.suppression_key == key {
                self.clear_failed_state();
            } else if self.local.suppression_key.is_empty() {
                self.local.suppression_key = key;
            } else {
                self.local.awaiting_observation = false;
                self.local.waiting_our_turn = true;
                self.local.observation_deadline = None;
            }
            return;
        }
        if self.local.waiting_our_turn {
            return;
        }
        if visible {
            self.clear_failed_state();
        } else if self.local.suppression_key.is_empty() {
            self.local.suppression_key = key;
        } else if self.local.suppression_key != key {
            self.clear_failed_state();
        }
    }

    fn clear_suppression(&mut self) {
        self.local.suppression = None;
        self.local.suppression_key.clear();
    }

    /// `clearFailedLocalMoveRecovery`.
    fn clear_recovery(&mut self) {
        self.local.recovery = None;
        self.local.awaiting_observation = false;
        self.local.waiting_our_turn = false;
        self.local.observation_deadline = None;
    }

    fn clear_failed_state(&mut self) {
        self.clear_suppression();
        self.clear_recovery();
    }
}

/// `hasSetupOrHandicapProperty`.
fn has_setup_property(properties: &[Property]) -> bool {
    properties.iter().any(|p| matches!(p.id.as_str(), "AB" | "AW" | "AE" | "PL" | "HA"))
}

/// `isTrustedFoxZeroMoveHandicapSetupTurn`: Fox move 0 with only (at least two) black stones is a
/// handicap start, White to play.
fn fox_zero_move_handicap(
    stones: &[Option<Stone>],
    delta: &SnapshotDelta,
    remote: &RemoteContext,
    source: LastMoveSource,
    fox: Option<i32>,
) -> bool {
    remote.platform == SyncPlatform::Fox
        && fox == Some(0)
        && !delta.has_marker()
        && !source.is_trusted_visual_marker()
        && !stones.contains(&Some(Stone::White))
        && stones.iter().filter(|s| **s == Some(Stone::Black)).count() >= 2
}

/// `next().filter(isMainTrunk)`.
fn next_main_line_node(rec: &GameRecord, node: NodeId) -> Option<NodeId> {
    next_excluding_end_placeholder(rec, node).filter(|&n| rec.is_main_trunk(n))
}

/// A recovery decision (`CompleteSnapshotRecoveryDecision`).
struct Decision {
    outcome: RecoveryOutcome,
    resolved: Option<NodeId>,
    resume: bool,
    reason: &'static str,
}

impl Decision {
    fn rebuild(reason: &'static str) -> Decision {
        Decision { outcome: RecoveryOutcome::ForceRebuild, resolved: None, resume: false, reason }
    }

    fn hold() -> Decision {
        Decision { outcome: RecoveryOutcome::Hold, resolved: None, resume: false, reason: "conflict_hold" }
    }

    fn no_change(node: NodeId, resume: bool, reason: &'static str) -> Decision {
        Decision { outcome: RecoveryOutcome::NoChange, resolved: Some(node), resume, reason }
    }
}

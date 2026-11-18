//! ReadBoard GMA: the helper's auto-play asks this client's KataGo for each move of one colour
//! (`play>black|white>time playouts first gma`); the move is placed here and clicked on the
//! target window by the helper. Java `ReadBoard.scheduleReadBoardGmaIfNeeded`,
//! `handleReadBoardGmaEnginePlay`, `clearReadBoardGmaAutoPlay`, `Leelaz.genmoveAnalyzeForReadBoard`,
//! `ReadBoardGmaPreparation`, `ReadBoardGmaSession` and its restore barrier.
//!
//! Each hand reserves the foreground engine (a GMA seat, Java `ReadBoardGmaReservation`), sets
//! `ponderingEnabled` / `maxTime` / `maxVisits` (their original values are read once with
//! `kata-get-param`), asks `kata-genmove_analyze` and ends at a stable recovery point: a played
//! move the record took, or — for a pass, resignation, refusal or a stale answer — the engine
//! holding the record's current position again (Java's exact restore, latest position wins).
//! A hand whose GMA was cleared meanwhile also gives the overridden parameters back while it
//! still holds the seat (Java `RestoringRuntime`), then checks the position again: one changed
//! meanwhile is restored first (`RestoringExact`, latest wins). Only when the engine holds the
//! latest position is the reservation given back. Without a hand the overridden parameters are
//! given back when GMA is cleared. A parameter command refused or unanswered within 8 s, or a
//! restore that cannot be confirmed, leaves the engine unrestored (`engineStateUnrestored`):
//! nothing uses it again until the user switches or restarts it. A failed arming does not retry
//! by itself (one failure per arming, Java `readBoardGmaFailedGeneration`).

use std::time::{Duration, Instant};

use crate::api::{ApiError, PonderingNoticeDecision};
use crate::engine::runtime::{GenmoveReply, InstanceId, LeaseAvailability, LeaseFailure, RuntimeEvent};
use crate::record::{DocumentEdit, NodeId, Stone};
use crate::workbench::Workbench;

/// Java `readBoardGmaRestoreResponseTimeoutMillis`: a preparation command and the exact restore
/// each wait this long.
const RESPONSE_TIMEOUT: Duration = Duration::from_millis(8000);

/// The runtime parameters a hand overrides, in Java's preparation order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Param {
    Pondering,
    MaxTime,
    MaxVisits,
}

const PARAMS: [Param; 3] = [Param::Pondering, Param::MaxTime, Param::MaxVisits];

impl Param {
    fn name(self) -> &'static str {
        match self {
            Param::Pondering => "ponderingEnabled",
            Param::MaxTime => "maxTime",
            Param::MaxVisits => "maxVisits",
        }
    }
}

/// One parameter's original value and whether a hand left it changed (Java
/// `ReadBoardGmaRuntimeParam`).
#[derive(Clone, Debug, Default)]
struct Override {
    original: Option<String>,
    snapshot_requested: bool,
    overridden: bool,
}

/// The parameters GMA changed on one engine process; a replaced engine starts clean.
#[derive(Clone, Debug)]
struct Overrides {
    instance: InstanceId,
    params: [Override; 3],
}

impl Overrides {
    fn dirty(&self) -> bool {
        self.params.iter().any(|p| p.overridden)
    }

    /// `kata-set-param <name> <original>` for every overridden parameter.
    fn restore_commands(&self) -> Vec<String> {
        PARAMS
            .iter()
            .zip(&self.params)
            .filter(|(_, p)| p.overridden)
            .filter_map(|(param, p)| p.original.as_ref().map(|v| format!("kata-set-param {} {v}", param.name())))
            .collect()
    }
}

/// The colour and limits of the helper's GMA auto-play.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Armed {
    color: Stone,
    /// `maxTime` seconds; 0 keeps the engine's own value.
    time: i32,
    /// `maxVisits`; 0 keeps the engine's own value.
    visits: i32,
}

#[derive(Debug)]
enum Step {
    /// Setting the parameters: `index` is the parameter being prepared, `sent` the command
    /// waiting for its answer.
    Prepare { index: usize, sent: Option<Sent> },
    /// `kata-genmove_analyze` asked at this position.
    Genmove { request: u64, node: NodeId, stamp: u64 },
    /// The engine catches up with the record's current position (Java `RestoringExact`, latest
    /// wins): the answer did not become a record move, or a retired hand gave its parameters
    /// back. The reservation is given back once the engine holds the position.
    Settling { deadline: Instant },
    /// A retired hand gives the overridden parameters back (Java `RestoringRuntime`): the
    /// `kata-set-param` commands not answered yet. The seat stays; the position is settled again
    /// afterwards, since it may have changed meanwhile.
    Restoring { pending: Vec<u64>, deadline: Instant },
}

#[derive(Debug)]
struct Sent {
    command: u64,
    text: String,
    /// The value this `kata-set-param` sets and whether it is GMA's own (vs. the original), or
    /// `None` for `kata-get-param`.
    set: Option<(String, bool)>,
    deadline: Instant,
}

/// One GMA hand: the reservation, the arming it serves and its progress.
#[derive(Debug)]
struct Hand {
    game: u64,
    instance: InstanceId,
    generation: u64,
    color: Stone,
    time: i32,
    visits: i32,
    ponder: bool,
    /// GMA was cleared or re-armed meanwhile (`retire`): the hand still converges, its answer is
    /// stale and the overridden parameters are given back with it.
    retired: bool,
    step: Step,
}

/// The reservation turning the engine's pondering off outside a hand (the helper's `playpon
/// off`, Java `disableReadBoardGmaPonderIfIdle`).
#[derive(Debug)]
struct PonderOff {
    game: u64,
    /// The `kata-get-param ponderingEnabled` waiting for the engine's own value (an engine no
    /// hand has prepared yet) and its deadline.
    snapshot: Option<(u64, Instant)>,
}

/// What GMA wants shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::workbench) enum GmaNotice {
    /// `该模式仅支持 KataGo`.
    Unsupported,
    /// A Java resource key (`AnalysisSettings.reuseStatus.existing_lease` when arming is refused).
    Message(String),
}

#[derive(Debug, Default)]
pub(in crate::workbench) struct GmaFlow {
    armed: Option<Armed>,
    /// Java `readBoardGmaSessionGeneration`: every arming and clearing starts a new one, so a
    /// late answer of an earlier one is stale.
    generation: u64,
    /// The arming whose hand failed (one failure per arming).
    failed_generation: Option<u64>,
    /// After any hand the next one waits for a synced board (`readBoardGmaAwaitingSyncedBoard`).
    awaiting_synced_board: bool,
    hand: Option<Hand>,
    /// The reservation giving the overridden parameters back after GMA was cleared.
    restore: Option<u64>,
    /// Turning the engine's pondering off outside a hand.
    ponder_off: Option<PonderOff>,
    overrides: Option<Overrides>,
    /// The engine `该模式仅支持 KataGo` was shown for (once per engine).
    unsupported_shown: Option<InstanceId>,
    next_request: u64,
    notices: Vec<GmaNotice>,
    ponder_notice: PonderNotice,
}

/// Java `readBoardWebSocketPonderingNotice{Pending,Acknowledged}`: the helper asked for pondering
/// the custom WebSocket engine cannot do. Every arming and clearing starts afresh.
#[derive(Debug, Default)]
struct PonderNotice {
    /// Answered with 确定 or 不再提示 for this arming: GMA plays without pondering.
    acknowledged: bool,
    /// Asked for this arming (its generation); GMA waits. A dismissed notice still waits.
    pending: Option<u64>,
    /// The notice is on the screen.
    shown: bool,
}

/// What the ReadBoard session knows that GMA's schedule needs.
#[derive(Clone, Copy, Debug)]
pub(in crate::workbench) struct GmaFacts {
    /// `Lizzie.frame.bothSync`.
    pub both_sync: bool,
    /// `readBoardTurnTrusted`.
    pub turn_trusted: bool,
    /// A failed local move still waits for the remote board (`failedLocalMoveAwaitingRemoteObservation`
    /// after its grace).
    pub failed_move_waits: bool,
    /// `config.readBoardPonder` (the helper's `playpon on|off`).
    pub ponder: bool,
}

impl GmaFlow {
    /// Java `isReadBoardGmaEngineBusy`: armed, a hand in flight or parameters being given back.
    pub(in crate::workbench) fn busy(&self) -> bool {
        self.armed.is_some() || self.reserved()
    }

    /// The colour GMA plays, while armed.
    pub(in crate::workbench) fn color(&self) -> Option<Stone> {
        self.armed.map(|a| a.color)
    }

    /// A hand holds the engine (Java `readBoardGmaPending` or its restore in progress).
    pub(in crate::workbench) fn in_flight(&self) -> bool {
        self.hand.is_some()
    }

    /// The overridden parameters are being given back.
    pub(in crate::workbench) fn restoring(&self) -> bool {
        self.restore.is_some()
    }

    /// A hand or the parameter restore holds a reservation (Java refuses engine switches and
    /// restarts while `readBoardGmaReservation` exists).
    pub(in crate::workbench) fn reserved(&self) -> bool {
        self.hand.is_some() || self.restore.is_some() || self.ponder_off.is_some()
    }

    pub(in crate::workbench) fn take_notices(&mut self) -> Vec<GmaNotice> {
        std::mem::take(&mut self.notices)
    }

    /// The arming whose pondering notice is on the screen.
    pub(in crate::workbench) fn pondering_notice(&self) -> Option<u64> {
        self.ponder_notice.pending.filter(|_| self.ponder_notice.shown)
    }

    pub(in crate::workbench) fn deadline(&self) -> Option<Instant> {
        let snapshot = self.ponder_off.as_ref().and_then(|p| p.snapshot).map(|(_, deadline)| deadline);
        match self.hand.as_ref().map(|h| &h.step) {
            Some(Step::Prepare { sent: Some(s), .. }) => Some(s.deadline),
            Some(Step::Settling { deadline } | Step::Restoring { deadline, .. }) => Some(*deadline),
            _ => snapshot,
        }
    }

    fn owns(&self, game: u64) -> bool {
        self.hand.as_ref().is_some_and(|h| h.game == game)
            || self.restore == Some(game)
            || self.ponder_off.as_ref().is_some_and(|p| p.game == game)
    }
}

impl Workbench {
    /// Publishes GMA's hold on the foreground engine and lets the analysis follow it.
    fn gma_hold(&mut self) {
        let busy = self.readboard.gma.busy();
        self.engines.set_readboard_gma(busy);
        self.sync_engine();
    }

    /// `play>…gma` (`ReadBoard.parseLine`): arms GMA for `color` with the helper's limits.
    /// Refused while the foreground engine is unrestored, in 双引擎模式, or while another mode,
    /// lease or recovery owns it (`canArmReadBoardGma`); a colour change while armed abandons
    /// the move being asked. Returns whether GMA is armed.
    pub(in crate::workbench) fn gma_arm(&mut self, color: Stone, time: i32, visits: i32) -> bool {
        let already = self.readboard.gma.armed.is_some();
        if !already && !self.gma_can_arm() {
            self.readboard.gma.notices.push(GmaNotice::Message("AnalysisSettings.reuseStatus.existing_lease".into()));
            return false;
        }
        if already && self.readboard.gma.armed.is_some_and(|a| a.color != color) {
            self.gma_retire_hand();
        }
        let gma = &mut self.readboard.gma;
        gma.generation += 1;
        gma.ponder_notice = PonderNotice::default();
        gma.armed = Some(Armed { color, time: time.max(0), visits: visits.max(0) });
        gma.awaiting_synced_board = true;
        self.gma_hold();
        true
    }

    /// `canArmReadBoardGma`, Web 试下 included also once its engine died (Leelaz.java:15165-15177).
    pub(in crate::workbench) fn gma_can_arm(&self) -> bool {
        if self.web_trial_active() {
            return false;
        }
        let Some(fg) = self.engines.foreground() else { return true };
        if self.engines.unrestored(fg).is_some() || self.double_engine() || self.restart_restoring() {
            return false;
        }
        !matches!(
            self.engines.lease_availability(),
            LeaseAvailability::PlayMode
                | LeaseAvailability::HumanSlGame
                | LeaseAvailability::ExistingLease
                | LeaseAvailability::ReadBoardGma
        )
    }

    /// `timechanged` / `playoutschanged` while GMA is armed.
    pub(in crate::workbench) fn gma_limits(&mut self, time: Option<i32>, visits: Option<i32>) {
        if let Some(a) = self.readboard.gma.armed.as_mut() {
            if let Some(t) = time {
                a.time = t.max(0);
            }
            if let Some(v) = visits {
                a.visits = v.max(0);
            }
        }
    }

    /// `playpon off` (`disableReadBoardGmaPonderIfIdle`): with GMA armed, no hand or restore
    /// holding the engine and an engine that can do GMA, the engine's pondering is turned off
    /// now (`setReadBoardGmaPondering(false)`: its own value read first if no hand has read it,
    /// then `kata-set-param ponderingEnabled false`; the original goes back with the other
    /// parameters) and a running analysis stops (`stopPonderingIfActive`), before the first hand
    /// as between two. The custom WebSocket engine gets no parameter command.
    pub(in crate::workbench) fn gma_ponder_off(&mut self) {
        let gma = &self.readboard.gma;
        if gma.armed.is_none() || gma.reserved() {
            return;
        }
        let Some(fg) = self.engines.foreground() else { return };
        if !self.gma_supported(fg) {
            return;
        }
        if !self.engines.is_custom_websocket(fg) {
            self.gma_set_pondering_off(fg);
        }
        self.readboard_stop_analysis();
    }

    /// `setReadBoardGmaPondering(false)` on its own GMA seat.
    fn gma_set_pondering_off(&mut self, fg: InstanceId) {
        let game = match self.engines.reserve_readboard_gma() {
            Ok(game) => game,
            Err(a) => {
                self.log(false, format!("ReadBoard GMA pondering stays: {a:?}"));
                return;
            }
        };
        let gma = &mut self.readboard.gma;
        if gma.overrides.as_ref().is_some_and(|o| o.instance != fg) {
            gma.overrides = None;
        }
        let pondering = &mut gma.overrides.get_or_insert_with(|| Overrides { instance: fg, params: Default::default() }).params[0];
        let snapshot = if pondering.original.is_some() {
            None
        } else {
            pondering.snapshot_requested = true;
            let Some(command) = self.engines.game_send(game, &format!("kata-get-param {}", Param::Pondering.name())) else {
                self.readboard.gma.ponder_off = Some(PonderOff { game, snapshot: None });
                self.engines.abandon_game(game);
                self.drain_engine_events();
                return;
            };
            Some((command, Instant::now() + RESPONSE_TIMEOUT))
        };
        let waits = snapshot.is_some();
        self.readboard.gma.ponder_off = Some(PonderOff { game, snapshot });
        if !waits {
            self.gma_release_pondering_off(game);
        }
        self.gma_hold();
    }

    /// The engine's own value is known: `ponderingEnabled` is overridden and set to `false` as
    /// the seat is given back.
    fn gma_release_pondering_off(&mut self, game: u64) {
        if let Some(o) = self.readboard.gma.overrides.as_mut() {
            o.params[0].overridden = true;
        }
        self.engines.release_game(game, &[format!("kata-set-param {} false", Param::Pondering.name())]);
        self.drain_engine_events();
    }

    /// The answer to the `kata-get-param` a `playpon off` sent: the engine's own value, or a
    /// failure that leaves the engine unrestored (as a hand's failed snapshot does).
    fn gma_ponder_off_reply(&mut self, game: u64, command: u64, ok: bool, text: &str) {
        let gma = &mut self.readboard.gma;
        let Some(off) = gma.ponder_off.as_mut().filter(|p| p.game == game) else { return };
        if !matches!(off.snapshot, Some((c, _)) if c == command) {
            return;
        }
        off.snapshot = None;
        let value = text.trim();
        if !ok || value.is_empty() {
            self.log(true, format!("ReadBoard GMA invalid parameter snapshot response: {}", Param::Pondering.name()));
            self.engines.abandon_game(game);
            self.drain_engine_events();
            return;
        }
        if let Some(o) = gma.overrides.as_mut() {
            o.params[0].original = Some(value.to_string());
        }
        self.gma_release_pondering_off(game);
    }

    /// `clearReadBoardGmaAutoPlay`: GMA stops; a hand in flight still converges and gives the
    /// parameters back after its answer, otherwise they are given back now.
    pub(in crate::workbench) fn gma_clear(&mut self) {
        let gma = &mut self.readboard.gma;
        let was = gma.armed.take().is_some() || gma.hand.is_some();
        gma.generation += 1;
        gma.ponder_notice = PonderNotice::default();
        gma.awaiting_synced_board = false;
        if was {
            self.gma_retire_hand();
        }
        if self.readboard.gma.hand.is_none() {
            self.gma_restore_runtime();
        }
        self.gma_hold();
    }

    /// The hand in flight loses its authorisation; it keeps converging (`retire`).
    pub(in crate::workbench) fn gma_retire_hand(&mut self) {
        let Some(hand) = self.readboard.gma.hand.as_mut() else { return };
        hand.retired = true;
        if let Step::Genmove { .. } = hand.step {
            return;
        }
        if let Step::Prepare { sent: None, .. } = hand.step {
            // Between parameter commands nothing physical is outstanding.
            let game = hand.game;
            self.gma_settling(game);
        }
    }

    /// The board was synced (`markReadBoardGmaSyncedBoard`, every `continueGameAfterSyncIfNeeded`).
    pub(in crate::workbench) fn gma_synced(&mut self, facts: GmaFacts) {
        self.readboard.gma.awaiting_synced_board = false;
        self.gma_schedule(facts);
    }

    /// The foreground engine finished starting (`onReadBoardGmaCapabilityReady`).
    pub(in crate::workbench) fn gma_engine_ready(&mut self, instance: InstanceId, facts: GmaFacts) {
        if self.engines.foreground() == Some(instance) {
            self.gma_schedule(facts);
        }
    }

    /// `scheduleReadBoardGmaIfNeeded`: asks for this colour's move when it is its turn on a
    /// synced, trusted board and the engine can do it.
    pub(in crate::workbench) fn gma_schedule(&mut self, facts: GmaFacts) {
        let Some(armed) = self.readboard.gma.armed else { return };
        let gma = &self.readboard.gma;
        if gma.failed_generation == Some(gma.generation)
            || gma.hand.is_some()
            || gma.restore.is_some()
            || gma.ponder_off.is_some()
            || facts.failed_move_waits
            || gma.awaiting_synced_board
        {
            return;
        }
        if !facts.both_sync || self.match_active() || self.double_engine() {
            self.gma_unsupported_once();
            return;
        }
        if self.doc.record.node(self.cursor).to_play != armed.color || !facts.turn_trusted {
            return;
        }
        let Some(fg) = self.engines.foreground() else { return };
        if self.engines.phase(fg) != Some(crate::engine::runtime::Phase::Ready) {
            return;
        }
        if !self.gma_supported(fg) {
            self.gma_unsupported_once();
            return;
        }
        // `supportsReadBoardGmaPondering`: the custom WebSocket engine cannot ponder. The helper's
        // wish for it is explained once per arming (unless 不再提示 was chosen); GMA waits for
        // the answer, then plays without pondering.
        let websocket = self.engines.is_custom_websocket(fg);
        let generation = self.readboard.gma.generation;
        if facts.ponder && websocket && !self.config.read_board.suppress_websocket_pondering_notice {
            let notice = &mut self.readboard.gma.ponder_notice;
            if !notice.acknowledged {
                if notice.pending.is_none() {
                    *notice = PonderNotice { acknowledged: false, pending: Some(generation), shown: true };
                }
                return;
            }
        }
        if self.readboard.gma.overrides.as_ref().is_some_and(|o| o.instance != fg) {
            self.readboard.gma.overrides = None;
        }
        let game = match self.engines.reserve_readboard_gma() {
            Ok(game) => game,
            Err(a) => {
                // `genmoveAnalyzeForReadBoard` refused: this arming does not try again.
                self.log(true, format!("ReadBoard GMA rejected by foreground lease: {a:?}"));
                self.readboard.gma.failed_generation = Some(generation);
                return;
            }
        };
        let gma = &mut self.readboard.gma;
        gma.overrides.get_or_insert_with(|| Overrides { instance: fg, params: Default::default() });
        gma.hand = Some(Hand {
            game,
            instance: fg,
            generation,
            color: armed.color,
            time: armed.time,
            visits: armed.visits,
            ponder: facts.ponder && !websocket,
            retired: false,
            // `ReadBoardGmaPreparation.start`: the WebSocket engine has no `ponderingEnabled`, so
            // its preparation begins with `maxTime` (`PARAMS[1]`).
            step: Step::Prepare { index: usize::from(websocket), sent: None },
        });
        self.gma_prepare();
    }

    /// `supportsReadBoardGmaFixedLimits`: KataGo listing `kata-genmove_analyze`,
    /// `kata-get-param` and `kata-set-param`.
    fn gma_supported(&self, instance: InstanceId) -> bool {
        self.engines.is_katago(instance)
            && ["kata-genmove_analyze", "kata-get-param", "kata-set-param"]
                .iter()
                .all(|c| self.engines.has_command(instance, c))
    }

    /// `showReadBoardGmaUnsupportedOnce`: once per engine.
    fn gma_unsupported_once(&mut self) {
        let fg = self.engines.foreground();
        let gma = &mut self.readboard.gma;
        if fg.is_some() && gma.unsupported_shown != fg {
            gma.unsupported_shown = fg;
            gma.notices.push(GmaNotice::Unsupported);
        }
    }

    /// The answer to the pondering notice of arming `generation` (Java's decision callback):
    /// 不再提示 is saved whatever arming it belonged to; 确定 and 不再提示 let the current arming
    /// play without pondering. A closed notice leaves GMA waiting until it is armed again.
    pub(in crate::workbench) fn gma_pondering_notice_answered(
        &mut self,
        generation: u64,
        decision: PonderingNoticeDecision,
    ) -> Result<(), ApiError> {
        let notice = &mut self.readboard.gma.ponder_notice;
        if notice.pending == Some(generation) {
            notice.shown = false;
        }
        if decision == PonderingNoticeDecision::Dismiss {
            return Ok(());
        }
        let saved = match decision {
            PonderingNoticeDecision::Suppress => {
                self.commit_config(|c| c.read_board.suppress_websocket_pondering_notice = true)
            }
            _ => Ok(()),
        };
        let gma = &mut self.readboard.gma;
        if gma.armed.is_some() && gma.generation == generation && gma.ponder_notice.pending == Some(generation) {
            gma.ponder_notice = PonderNotice { acknowledged: true, pending: None, shown: false };
            let facts = self.gma_facts();
            self.gma_schedule(facts);
        }
        saved
    }

    /// `ReadBoardGmaPreparation`: the next parameter command, or the move request once all are
    /// set.
    fn gma_prepare(&mut self) {
        loop {
            let gma = &mut self.readboard.gma;
            let Some(hand) = gma.hand.as_mut() else { return };
            let Step::Prepare { index, sent } = &mut hand.step else { return };
            if sent.is_some() {
                return;
            }
            if *index >= PARAMS.len() {
                break;
            }
            let param = PARAMS[*index];
            let overrides = gma.overrides.as_mut().expect("a hand has overrides");
            let ov = &mut overrides.params[*index];
            if !ov.snapshot_requested {
                ov.snapshot_requested = true;
                let text = format!("kata-get-param {}", param.name());
                let game = hand.game;
                self.gma_send(game, text, None);
                continue;
            }
            let value = match param {
                Param::Pondering => Some(if hand.ponder { "true" } else { "false" }.to_string()),
                Param::MaxTime => (hand.time > 0).then(|| hand.time.to_string()),
                Param::MaxVisits => (hand.visits > 0).then(|| hand.visits.to_string()),
            };
            let action = match value {
                Some(v) => Some((v, true)),
                // `restoreParamForMoveIfNeeded`: no limit of its own puts the original back.
                None if ov.overridden => match ov.original.clone() {
                    Some(original) => Some((original, false)),
                    None => {
                        let game = hand.game;
                        self.gma_fail(game, format!("missing parameter snapshot: {}", param.name()));
                        return;
                    }
                },
                None => None,
            };
            *index += 1;
            if let Some((v, own)) = action {
                let game = hand.game;
                self.gma_send(game, format!("kata-set-param {} {v}", param.name()), Some((v, own)));
            }
        }
        self.gma_genmove();
    }

    fn gma_send(&mut self, game: u64, text: String, set: Option<(String, bool)>) {
        let Some(command) = self.engines.game_send(game, &text) else {
            self.gma_fail(game, format!("parameter send failed: {text}"));
            return;
        };
        if let Some(Hand { step: Step::Prepare { sent, .. }, .. }) = self.readboard.gma.hand.as_mut() {
            *sent = Some(Sent { command, text, set, deadline: Instant::now() + RESPONSE_TIMEOUT });
        }
    }

    /// `sendReadBoardGmaCommand`: `kata-genmove_analyze <colour> <interval> …` at the current
    /// position.
    fn gma_genmove(&mut self) {
        let target = self.analysis_target();
        let gma = &mut self.readboard.gma;
        let Some(hand) = gma.hand.as_mut() else { return };
        if hand.retired {
            let game = hand.game;
            self.gma_settling(game);
            return;
        }
        let request = gma.next_request;
        gma.next_request += 1;
        let node = target.node;
        let stamp = target.record.node(node).stamp;
        let (game, color) = (hand.game, hand.color);
        hand.step = Step::Genmove { request, node, stamp };
        if !self.engines.genmove(game, request, target, color, None, false) {
            self.gma_fail(game, "ReadBoard GMA session admission failed".into());
        }
    }

    /// GMA's engine events; others pass on.
    pub(in crate::workbench) fn gma_event(&mut self, ev: RuntimeEvent) -> Option<RuntimeEvent> {
        let owned = match &ev {
            RuntimeEvent::GameReply { game, .. } | RuntimeEvent::Genmove { game, .. } | RuntimeEvent::GameReleased { game, .. } => {
                self.readboard.gma.owns(*game)
            }
            _ => false,
        };
        if !owned {
            return Some(ev);
        }
        match ev {
            RuntimeEvent::GameReply { game, command, ok, text } => self.gma_reply(game, command, ok, text),
            RuntimeEvent::Genmove { game, request, doc, node, stamp, reply } => {
                self.gma_answer(game, request, doc, node, stamp, reply)
            }
            RuntimeEvent::GameReleased { game, failure } => self.gma_released(game, failure),
            _ => unreachable!(),
        }
        None
    }

    fn gma_reply(&mut self, game: u64, command: u64, ok: bool, text: String) {
        let gma = &mut self.readboard.gma;
        if gma.ponder_off.as_ref().is_some_and(|p| p.game == game) {
            self.gma_ponder_off_reply(game, command, ok, &text);
            return;
        }
        let Some(hand) = gma.hand.as_mut().filter(|h| h.game == game) else { return };
        if let Step::Restoring { pending, .. } = &mut hand.step {
            let Some(at) = pending.iter().position(|c| *c == command) else { return };
            pending.remove(at);
            let done = pending.is_empty();
            if !ok {
                self.gma_fail(game, format!("restore command failed: {}", text.trim()));
            } else if done {
                // The parameters are back; the position may have changed meanwhile.
                gma.overrides = None;
                self.gma_settling(game);
            }
            return;
        }
        let Step::Prepare { index, sent } = &mut hand.step else { return };
        let Some(s) = sent.take_if(|s| s.command == command) else { return };
        if !ok {
            self.gma_fail(game, format!("parameter command failed: {}", text.trim()));
            return;
        }
        let overrides = gma.overrides.as_mut().expect("a hand has overrides");
        match s.set {
            None => {
                let value = text.trim();
                if value.is_empty() {
                    let name = PARAMS[*index].name();
                    self.gma_fail(game, format!("invalid parameter snapshot response: {name}"));
                    return;
                }
                overrides.params[*index].original = Some(value.to_string());
            }
            Some((_, own)) => {
                // `index` already moved past the parameter this set belongs to.
                overrides.params[*index - 1].overridden = own;
            }
        }
        if hand.retired {
            self.gma_settling(game);
        } else {
            self.gma_prepare();
        }
    }

    /// `handleReadBoardGmaEnginePlay`: a move of the current arming at the position it was asked
    /// for is placed (the engine already played it) and clicked by the helper; anything else
    /// ends the hand with the engine back at the record's position.
    fn gma_answer(&mut self, game: u64, request: u64, doc: u64, node: NodeId, stamp: u64, reply: GenmoveReply) {
        let gma = &self.readboard.gma;
        let Some(hand) = gma.hand.as_ref().filter(|h| h.game == game) else {
            self.engines.game_move_dropped(game, request);
            return;
        };
        let Step::Genmove { request: asked, node: at, stamp: at_stamp } = hand.step else {
            self.engines.game_move_dropped(game, request);
            return;
        };
        let stale = request != asked || hand.retired || hand.generation != gma.generation || gma.armed.is_none();
        let rec = &self.doc.record;
        let current = doc == self.doc.id
            && node == at
            && stamp == at_stamp
            && node == self.cursor
            && rec.contains(node)
            && rec.node(node).stamp == stamp
            && rec.node(node).to_play == hand.color;
        let color = hand.color;
        if let (GenmoveReply::Move(point), false, true) = (&reply, stale, current) {
            let edit = DocumentEdit::Play { at: self.cursor, color, point: *point, alternate: false, rules: self.play_rules() };
            if let Some(n) = self.apply_edit(edit).ok().and_then(|a| a.node) {
                self.move_to(n);
                let committed = self.doc.record.node(n).stamp;
                self.engine_move_committed(game, request, committed);
                self.gma_terminal(game, true);
                return;
            }
        }
        match &reply {
            GenmoveReply::Failed(text) => self.log(true, format!("ReadBoard GMA move failed: {text}")),
            GenmoveReply::Resign => self.log(false, "ReadBoard GMA: the engine resigned".into()),
            _ => {}
        }
        self.engines.game_move_dropped(game, request);
        self.gma_terminal(game, false);
    }

    /// A hand's answer was taken (`played`) or not. After every hand the next one waits for a
    /// synced board. A played move needs no restore unless GMA was cleared meanwhile.
    fn gma_terminal(&mut self, game: u64, played: bool) {
        self.readboard.gma.awaiting_synced_board = true;
        let retired = self.readboard.gma.hand.as_ref().is_some_and(|h| h.retired);
        if played && !retired {
            self.gma_end_hand(game);
        } else {
            self.gma_settling(game);
        }
    }

    /// Exact restore: the reservation stays until the engine holds the record's current
    /// position (latest wins).
    fn gma_settling(&mut self, game: u64) {
        let Some(hand) = self.readboard.gma.hand.as_mut().filter(|h| h.game == game) else { return };
        hand.step = Step::Settling { deadline: Instant::now() + RESPONSE_TIMEOUT };
        self.gma_settle();
    }

    /// Checks a settling hand: failed when the engine refused the wished position; once the
    /// engine holds it, a retired hand with parameters still overridden gives them back first
    /// (and settles again after), any other hand ends. Returns whether the hand moved on.
    pub(in crate::workbench) fn gma_settle(&mut self) -> bool {
        let gma = &self.readboard.gma;
        let Some(hand) = gma.hand.as_ref() else { return false };
        if !matches!(hand.step, Step::Settling { .. }) {
            return false;
        }
        let (game, instance) = (hand.game, hand.instance);
        if self.engines.sync_refused(instance) {
            self.gma_fail(game, "the engine refused the restored position".into());
            return true;
        }
        if !self.engines.holds_wish(instance) {
            return false;
        }
        let restores = match gma.overrides.as_ref() {
            Some(o) if hand.retired => o.restore_commands(),
            _ => Vec::new(),
        };
        if restores.is_empty() {
            self.gma_end_hand(game);
        } else {
            self.gma_restore_params(game, &restores);
        }
        true
    }

    /// A retired hand gives the overridden parameters back on its own seat.
    fn gma_restore_params(&mut self, game: u64, restores: &[String]) {
        let mut pending = Vec::with_capacity(restores.len());
        for text in restores {
            let Some(command) = self.engines.game_send(game, text) else {
                self.gma_fail(game, format!("restore send failed: {text}"));
                return;
            };
            pending.push(command);
        }
        if let Some(hand) = self.readboard.gma.hand.as_mut().filter(|h| h.game == game) {
            hand.step = Step::Restoring { pending, deadline: Instant::now() + RESPONSE_TIMEOUT };
        }
    }

    /// The hand ends with the engine at the record's position (and, if retired, its parameters
    /// back): the reservation is given back without a command, so a native ponder goes on
    /// (`retainReadBoardGmaNativePonderAfterTerminal`).
    fn gma_end_hand(&mut self, game: u64) {
        if self.readboard.gma.hand.take_if(|h| h.game == game).is_none() {
            return;
        }
        self.engines.end_game(game);
        if self.readboard.gma.armed.is_none() {
            self.gma_restore_runtime();
        }
        self.gma_hold();
    }

    /// The parameters go back once GMA is cleared and no hand holds the engine. While another
    /// mode holds the engine (a viewer's Web 试下 between two hands) the duty is kept and runs
    /// when that mode lets go ([`Self::gma_restore_deferred`]); it lapses only with the engine
    /// it belongs to.
    fn gma_restore_runtime(&mut self) {
        let gma = &mut self.readboard.gma;
        let Some(overrides) = gma.overrides.as_ref() else { return };
        if gma.reserved() {
            return;
        }
        if !overrides.dirty() || self.engines.foreground() != Some(overrides.instance) {
            gma.overrides = None;
            return;
        }
        let restores = overrides.restore_commands();
        match self.engines.reserve_readboard_gma() {
            Ok(game) => {
                self.readboard.gma.restore = Some(game);
                self.engines.release_game(game, &restores);
                self.drain_engine_events();
            }
            Err(a @ (LeaseAvailability::NoForegroundEngine | LeaseAvailability::EngineStateUnrestored)) => {
                self.log(true, format!("ReadBoard GMA runtime restore not admitted: {a:?}"));
                self.readboard.gma.overrides = None;
            }
            Err(a) => self.log(false, format!("ReadBoard GMA runtime restore waits: {a:?}")),
        }
    }

    /// A mode that held the engine let it go: a parameter restore that had to wait runs now,
    /// unless GMA was armed again meanwhile (its hands give the parameters back themselves).
    pub(in crate::workbench) fn gma_restore_deferred(&mut self) {
        let gma = &self.readboard.gma;
        if gma.overrides.is_some() && gma.armed.is_none() && !gma.reserved() {
            self.gma_restore_runtime();
            self.gma_hold();
        }
    }

    /// A reservation ended in the runtime: the parameter restore after clearing confirmed (or
    /// failed), pondering was turned off outside a hand, or a hand's engine went away (Java
    /// `failEngineProcess`).
    fn gma_released(&mut self, game: u64, failure: Option<LeaseFailure>) {
        let gma = &mut self.readboard.gma;
        if gma.restore == Some(game) {
            gma.restore = None;
            gma.overrides = None;
            if let Some(f) = failure {
                self.gma_quarantined(format!("restore response failed: {f:?}"));
            }
            self.gma_hold();
            return;
        }
        if gma.ponder_off.take_if(|p| p.game == game).is_some() {
            if let Some(f) = failure {
                gma.overrides = None;
                self.gma_quarantined(format!("ponder response failed: {f:?}"));
                self.gma_hold();
                return;
            }
            // GMA may have been cleared, or its turn come, meanwhile.
            if gma.armed.is_none() {
                self.gma_restore_runtime();
            }
            self.gma_hold();
            let facts = self.gma_facts();
            self.gma_schedule(facts);
            return;
        }
        let Some(hand) = gma.hand.take_if(|h| h.game == game) else { return };
        gma.failed_generation = Some(hand.generation);
        gma.overrides = None;
        self.gma_quarantined(format!("{:?}", failure.unwrap_or(LeaseFailure::RestoreFailed)));
        self.gma_hold();
    }

    /// A hand failed (`ReadBoardGmaPreparation.fail` / `failReadBoardGmaEngineRestore`): the
    /// reservation is dropped and the engine left unrestored.
    fn gma_fail(&mut self, game: u64, detail: String) {
        let gma = &mut self.readboard.gma;
        let Some(hand) = gma.hand.take_if(|h| h.game == game) else { return };
        gma.failed_generation = Some(hand.generation);
        gma.overrides = None;
        self.engines.abandon_game(game);
        self.gma_quarantined(detail);
        self.drain_engine_events();
        self.gma_hold();
    }

    /// `quarantineSessionOwnedReadBoardGmaFailure`: the engine stays unrestored until the user
    /// switches or restarts it.
    fn gma_quarantined(&mut self, detail: String) {
        self.log(true, format!("ReadBoard GMA engine restore failed: {detail}"));
    }

    /// Preparation commands, the parameter restore, the exact restore and the snapshot a
    /// `playpon off` reads are bounded by 8 s each.
    pub(in crate::workbench) fn gma_tick(&mut self, now: Instant) -> bool {
        let off = self.readboard.gma.ponder_off.as_ref();
        if let Some(game) = off.filter(|p| p.snapshot.is_some_and(|(_, d)| now >= d)).map(|p| p.game) {
            self.log(true, format!("ReadBoard GMA parameter response timeout: kata-get-param {}", Param::Pondering.name()));
            self.engines.abandon_game(game);
            self.drain_engine_events();
            return true;
        }
        let Some(hand) = self.readboard.gma.hand.as_ref() else { return false };
        let game = hand.game;
        match &hand.step {
            Step::Prepare { sent: Some(s), .. } if now >= s.deadline => {
                let text = s.text.clone();
                self.gma_fail(game, format!("parameter response timeout: {text}"));
                true
            }
            Step::Settling { deadline } | Step::Restoring { deadline, .. } if now >= *deadline => {
                self.gma_fail(game, "restore response timeout".into());
                true
            }
            _ => false,
        }
    }

    /// The helper went away: GMA stops as if cleared (Java `retireTrackingEligibility`).
    pub(in crate::workbench) fn gma_helper_ended(&mut self) {
        self.gma_clear();
    }
}

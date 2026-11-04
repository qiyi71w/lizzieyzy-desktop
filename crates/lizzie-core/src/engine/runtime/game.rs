//! The foreground engine reserved by a play mode (人机对弈, Java `EngineModeReservation` and the
//! `PLAY_MODE` availability it implies). Unlike the foreground analysis lease the engine keeps
//! following its seat's wish: the game position is the wish, and the game adds its own commands
//! (setup, time, handicap) and engine moves. Answers go to the holder as events; a reservation
//! ends only when the engine confirmed it is restored (`name` after the restore commands) or when
//! the engine is gone.

use std::time::Instant;

use super::{
    color_letter, sync_commands, Engines, EngineKind, InstanceId, LeaseFailure, Purpose, Request, RuntimeEvent,
    Target, TargetKey, LEASE_RESTORE,
};
use crate::engine::gtp;
use crate::record::{Point, Stone};

/// What an engine answered to its move request.
#[derive(Clone, Debug, PartialEq)]
pub enum GenmoveReply {
    Move(Point),
    Pass,
    Resign,
    /// Refused, or an answer that names no move of this board.
    Failed(String),
}

/// A move asked for and not sent yet (the engine is busy with earlier commands).
struct Wanted {
    request: u64,
    target: Target,
    color: Stone,
    time_left: Option<String>,
    /// `nameCmdfornoponder`: stop thinking once the move is answered (`play-ponder` off).
    stop_after: bool,
}

struct InFlight {
    request: u64,
    gtp: u64,
    key: TargetKey,
    stop_after: bool,
}

pub(super) struct GameSeat {
    id: u64,
    instance: InstanceId,
    wanted: Option<Wanted>,
    genmove: Option<InFlight>,
    /// The placeholder action an answered move left on the engine board, until the record has it.
    placed: Option<(u64, u64)>,
    /// The `name` that confirms the restore, once releasing.
    releasing: Option<u64>,
    /// The restore commands not answered yet; one refused leaves the engine unrestored.
    restoring: Vec<u64>,
    restore_failed: bool,
    deadline: Option<Instant>,
}

/// A placeholder board action for the engine's own move: matches no node, so a move the record
/// never took is undone by the next sync.
fn placeholder(gtp: u64) -> u64 {
    u64::MAX - gtp
}

impl Engines {
    /// Reserves the foreground engine for a play mode (`beginEngineModeReservation`). Refused
    /// without a foreground engine, while it is lent or unrestored, or while another game holds it.
    pub fn reserve_game(&mut self) -> Result<u64, super::LeaseAvailability> {
        use super::LeaseAvailability as A;
        let Some(id) = self.foreground.filter(|&f| self.get(f).is_some()) else {
            return Err(A::NoForegroundEngine);
        };
        if self.get(id).unwrap().unrestored.is_some() {
            return Err(A::EngineStateUnrestored);
        }
        if self.lease.is_some() {
            return Err(A::ExistingLease);
        }
        if self.game.is_some() {
            return Err(A::PlayMode);
        }
        let game = self.next_lease;
        self.next_lease += 1;
        self.game = Some(GameSeat {
            id: game,
            instance: id,
            wanted: None,
            genmove: None,
            placed: None,
            releasing: None,
            restoring: Vec::new(),
            restore_failed: false,
            deadline: None,
        });
        Ok(game)
    }

    /// The instance `game` holds, while it holds one.
    pub fn game_instance(&self, game: u64) -> Option<InstanceId> {
        self.game.as_ref().filter(|g| g.id == game).map(|g| g.instance)
    }

    /// Whether instance `id` listed `command` (`list_commands`).
    pub fn has_command(&self, id: InstanceId, command: &str) -> bool {
        self.get(id).is_some_and(|i| i.has(command))
    }

    /// Whether instance `id` can analyse (Java `!noAnalyze`).
    pub fn can_analyze(&self, id: InstanceId) -> bool {
        self.get(id).is_some_and(|i| i.dialect().is_some())
    }

    /// The name a game records for instance `id` (`oriEnginename`).
    pub fn engine_name(&self, id: InstanceId) -> Option<String> {
        self.get(id).map(|i| i.display_name())
    }

    /// Whether the reservation is being given back (restore not yet confirmed).
    pub fn game_releasing(&self, game: u64) -> bool {
        self.game.as_ref().is_some_and(|g| g.id == game && g.releasing.is_some())
    }

    /// Whether `game`'s engine is thinking about a move (asked or answering).
    pub fn game_thinking(&self, game: u64) -> bool {
        self.game.as_ref().is_some_and(|g| g.id == game && (g.wanted.is_some() || g.genmove.is_some()))
    }

    /// Sends a command of `game` now; its answer comes back as [`RuntimeEvent::GameReply`].
    pub fn game_send(&mut self, game: u64, command: &str) -> Option<u64> {
        let id = self.game_instance(game).filter(|_| !self.game_releasing(game))?;
        let n = self.send(id, command, Purpose::Game)?;
        self.fence_last(id);
        Some(n)
    }

    /// Asks `game`'s engine for its move at `target` as `color` (`Leelaz.genmove`), after the
    /// board is synced; `time_left` goes first. Replaces a request not sent yet. The answer comes
    /// back as [`RuntimeEvent::Genmove`] with `request`.
    pub fn genmove(&mut self, game: u64, request: u64, target: Target, color: Stone, time_left: Option<String>, stop_after: bool) -> bool {
        let Some(seat) = self.game.as_mut().filter(|g| g.id == game && g.releasing.is_none()) else {
            return false;
        };
        seat.wanted = Some(Wanted { request, target, color, time_left, stop_after });
        let id = seat.instance;
        self.pump_seat(id);
        true
    }

    /// Forgets a move request not sent yet; one in flight still answers (and is then stale).
    pub fn cancel_genmove(&mut self, game: u64) {
        if let Some(seat) = self.game.as_mut().filter(|g| g.id == game) {
            seat.wanted = None;
        }
    }

    /// The record took the engine's answered move as the node stamped `stamp`: the engine board
    /// holds that node now instead of the placeholder.
    pub fn game_move_committed(&mut self, game: u64, request: u64, stamp: u64) {
        let Some(seat) = self.game.as_mut().filter(|g| g.id == game) else { return };
        let Some((_, mark)) = seat.placed.take_if(|(r, _)| *r == request) else { return };
        let id = seat.instance;
        if let Some(board) = self.get_mut(id).and_then(|i| i.board.as_mut()) {
            if let Some(a) = board.actions.iter_mut().find(|a| **a == mark) {
                *a = stamp;
            }
        }
        self.pump_seat(id);
    }

    /// The record did not take the engine's answered move (a stale answer, an illegal move or
    /// the end of the game): the next sync undoes it on the engine board.
    pub fn game_move_dropped(&mut self, game: u64, request: u64) {
        let Some(seat) = self.game.as_mut().filter(|g| g.id == game) else { return };
        if seat.placed.take_if(|(r, _)| *r == request).is_some() {
            let id = seat.instance;
            self.pump_seat(id);
        }
    }

    /// Gives the engine back: `restore` (commands undoing what the game changed), then `name`;
    /// [`RuntimeEvent::GameReleased`] reports the confirmation, or a restore failure when any of
    /// them is refused (the engine then stays unrestored).
    pub fn release_game(&mut self, game: u64, restore: &[String]) {
        let Some(seat) = self.game.as_mut().filter(|g| g.id == game && g.releasing.is_none()) else { return };
        seat.wanted = None;
        let id = seat.instance;
        let mut restoring = Vec::new();
        for c in restore {
            restoring.extend(self.send(id, c, Purpose::Game));
        }
        let Some(n) = self.send(id, "name", Purpose::Game) else {
            self.game = None;
            self.pending.push(RuntimeEvent::GameReleased { game, failure: Some(LeaseFailure::TransportClosed) });
            return;
        };
        self.fence_last(id);
        let seat = self.game.as_mut().unwrap();
        seat.releasing = Some(n);
        seat.restoring = restoring;
        if seat.genmove.is_none() {
            seat.deadline = Some(Instant::now() + LEASE_RESTORE);
        }
    }

    pub(super) fn game_deadline(&self) -> Option<Instant> {
        self.game.as_ref().and_then(|g| g.deadline)
    }

    /// The restore was not confirmed in time: the engine's state is unknown until it restarts.
    pub(super) fn game_timeout(&mut self, now: Instant, events: &mut Vec<RuntimeEvent>) {
        if !self.game.as_ref().and_then(|g| g.deadline).is_some_and(|d| now >= d) {
            return;
        }
        let seat = self.game.take().unwrap();
        if let Some(inst) = self.get_mut(seat.instance) {
            inst.unrestored = Some(LeaseFailure::RestoreFailed);
        }
        events.push(RuntimeEvent::GameReleased { game: seat.id, failure: Some(LeaseFailure::RestoreFailed) });
    }

    /// The reserved instance ended.
    pub(super) fn game_instance_ended(&mut self, id: InstanceId) {
        if let Some(seat) = self.game.take_if(|g| g.instance == id) {
            self.pending.push(RuntimeEvent::GameReleased { game: seat.id, failure: Some(LeaseFailure::TransportClosed) });
        }
    }

    /// The engine's move command, by kind (`Leelaz.genmove`).
    fn genmove_command(&self, id: InstanceId, color: Stone) -> String {
        let inst = self.get(id).unwrap();
        let c = color_letter(color);
        let interval = self.interval_centisec;
        match inst.kind {
            EngineKind::KataGo if inst.has("kata-genmove_analyze") => {
                let o = self.options_for(inst);
                format!(
                    "kata-genmove_analyze {c} {interval}{}{}{}{}",
                    if inst.supports_root_info() { " rootInfo true" } else { "" },
                    if o.ownership { " ownership true" } else { "" },
                    if o.pv_visits { " pvVisits true" } else { "" },
                    if o.moves_ownership { " movesOwnership true" } else { "" },
                )
            }
            EngineKind::Sayuri if inst.has("genmove_analyze") => format!("genmove_analyze {c} {interval}"),
            EngineKind::Sai | EngineKind::LeelaZero if inst.has("lz-genmove_analyze") => {
                format!("lz-genmove_analyze {c} {interval}")
            }
            _ => format!("genmove {c}"),
        }
    }

    /// `pump_seat` for a reserved instance: an answered move the record has not decided on
    /// holds the engine board as it is; a move to ask for syncs the position, then asks.
    /// Returns whether the seat's wish waits.
    pub(super) fn game_pump(&mut self, id: InstanceId) -> bool {
        if self.game.as_ref().is_some_and(|g| g.instance == id && g.placed.is_some()) {
            return true;
        }
        let Some(seat) = self.game.as_mut().filter(|g| g.instance == id && g.wanted.is_some() && g.genmove.is_none()) else {
            return false;
        };
        let w = seat.wanted.take().unwrap();
        let inst = self.get(id).unwrap();
        let (commands, board) = sync_commands(inst, &w.target);
        let key = w.target.key();
        if !commands.is_empty() {
            let i = self.get_mut(id).unwrap();
            i.board = Some(board);
            i.syncing = Some(key);
            for c in commands {
                self.send(id, &c, Purpose::Sync);
            }
        }
        if let Some(t) = &w.time_left {
            self.send(id, t, Purpose::Setup);
        }
        let command = self.genmove_command(id, w.color);
        let Some(n) = self.send(id, &command, Purpose::Genmove) else {
            self.pending.push(RuntimeEvent::Genmove {
                game: self.game.as_ref().unwrap().id,
                request: w.request,
                doc: key.doc,
                node: key.node,
                stamp: key.stamp,
                reply: GenmoveReply::Failed("engine not writable".into()),
            });
            return true;
        };
        let i = self.get_mut(id).unwrap();
        i.requests.push_back((
            n,
            Request { key, to_play: w.color, size: w.target.record.size, started: Instant::now() },
        ));
        i.fence = Some(n);
        let seat = self.game.as_mut().unwrap();
        seat.genmove = Some(InFlight { request: w.request, gtp: n, key, stop_after: w.stop_after });
        true
    }

    /// The answer to a game command or move.
    pub(super) fn game_answer(&mut self, id: InstanceId, n: u64, genmove: bool, ok: bool, text: String, events: &mut Vec<RuntimeEvent>) {
        let Some(seat) = self.game.as_mut().filter(|g| g.instance == id) else { return };
        let game = seat.id;
        if genmove {
            let Some(f) = seat.genmove.take_if(|f| f.gtp == n) else { return };
            if seat.releasing.is_some() {
                seat.deadline = Some(Instant::now() + LEASE_RESTORE);
            }
            let inst = self.get_mut(id).unwrap();
            inst.requests.retain(|(r, _)| *r != n);
            let size = inst.board.as_ref().map(|b| b.size);
            let word = text.split_whitespace().last().unwrap_or("").to_string();
            let reply = if !ok {
                GenmoveReply::Failed(text)
            } else if word.eq_ignore_ascii_case("resign") {
                GenmoveReply::Resign
            } else if word.eq_ignore_ascii_case("pass") {
                GenmoveReply::Pass
            } else {
                match size.map(|s| gtp::parse_vertex(s, &word)) {
                    Some(Ok(Some(p))) => GenmoveReply::Move(p),
                    Some(Ok(None)) => GenmoveReply::Pass,
                    _ => GenmoveReply::Failed(text),
                }
            };
            if matches!(reply, GenmoveReply::Move(_) | GenmoveReply::Pass) {
                let mark = placeholder(n);
                if let Some(board) = inst.board.as_mut() {
                    board.actions.push(mark);
                }
                self.game.as_mut().unwrap().placed = Some((f.request, mark));
            }
            if f.stop_after {
                let stop = if self.get(id).unwrap().kind == EngineKind::KataGo { "stop" } else { "name" };
                self.send(id, stop, Purpose::Setup);
            }
            events.push(RuntimeEvent::Genmove {
                game,
                request: f.request,
                doc: f.key.doc,
                node: f.key.node,
                stamp: f.key.stamp,
                reply,
            });
            return;
        }
        if let Some(at) = seat.restoring.iter().position(|r| *r == n) {
            seat.restoring.remove(at);
            seat.restore_failed |= !ok;
        }
        if seat.releasing == Some(n) {
            let failed = !ok || seat.restore_failed;
            let seat = self.game.take().unwrap();
            let failure = failed.then_some(LeaseFailure::RestoreFailed);
            if failed {
                if let Some(inst) = self.get_mut(seat.instance) {
                    inst.unrestored = failure;
                }
            }
            events.push(RuntimeEvent::GameReleased { game, failure });
            return;
        }
        events.push(RuntimeEvent::GameReply { game, command: n, ok, text });
    }
}

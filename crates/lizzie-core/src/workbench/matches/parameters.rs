//! 对局参数会话 (Java `EngineGameParameterSession`): an analysis-mode batch's breadth
//! (`analysisWideRootNoise`) and aggressiveness (`playoutDoublingAdvantage`) per participant
//! position. It keeps apart the batch's frozen opening values, the current game's confirmed and
//! requested values, and each participant process's values before the batch first changed them.
//! It is the only writer of the two parameters while a match holds the engines: every write is
//! read back with `kata-get-param`, and the batch's end gives exactly the captured processes their
//! values back. Genmove batches have no session and leave both parameters alone.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use super::Workbench;
use crate::api::*;
use crate::engine::runtime::InstanceId;
use crate::settings::MatchOpeningParameters;

/// The GTP names in the order a game writes them: breadth, then aggressiveness.
const NAMES: [&str; 2] = ["analysisWideRootNoise", "playoutDoublingAdvantage"];
const PARAMETERS: [MatchParameter; 2] = [MatchParameter::Wrn, MatchParameter::Pda];
/// Java `Leelaz.engineGameCommandResponseTimeoutMillis` (`FOREGROUND_RELEASE_STOP_TIMEOUT_MILLIS`):
/// how long one participant command may wait for its answer.
const COMMAND_TIMEOUT: Duration = Duration::from_millis(8000);
/// Java `EngineManager.engineGameStartupTimeoutMillis`, which `runUpdate` gives the game's
/// operation in flight before an in-game revision writes anything.
const TURN_WAIT: Duration = Duration::from_secs(30);
/// The notice of a game whose opening parameters were not confirmed (product text); its `{0}`
/// lists the participants and reasons.
pub(super) const OPENING_FAILED: &str = "product.matchOpeningFailed";
/// The notice of an in-game revision that was not confirmed: the game is paused.
const REVISION_FAILED: &str = "EngineGameParameters.failed";
const NOT_PLAYING: &str = "EngineGameParameters.notPlaying";

/// `[parameter][position]`: parameter 0 is breadth, position 0 is 引擎 1.
type Grid<T> = [[T; 2]; 2];

pub(super) struct ParameterSession {
    opening: Grid<f64>,
    originals: Vec<Original>,
    /// Captured values were written over since the last restore.
    dirty: bool,
    game: Option<GameParameters>,
    work: Option<Work>,
    /// `(seat, command)` of writes and read-backs that timed out: their late answers belong to
    /// nobody and change nothing.
    late: Vec<(u64, u64)>,
    /// Processes stopped because their values could not be given back.
    stopped: Vec<InstanceId>,
    /// Of those, the ones the batch's end has not stopped yet.
    to_stop: Vec<InstanceId>,
}

/// A process's values before the batch; `None` for a parameter it does not take.
struct Original {
    position: usize,
    instance: InstanceId,
    values: [Option<f64>; 2],
}

/// One game's parameters (Java `EngineGameParameterSession.Game`).
struct GameParameters {
    game: u64,
    supported: Grid<bool>,
    confirmed: Grid<Option<f64>>,
    target: Grid<Option<f64>>,
    pending: bool,
    failures: Vec<MatchParameterFailure>,
}

#[derive(Clone, Copy)]
enum Purpose {
    Capture(usize),
    Set(usize, f64),
    Read(usize, f64),
}

struct Leg {
    position: usize,
    seat: u64,
    command: u64,
    purpose: Purpose,
}

/// Writing a game's values: each parameter is set on every participant that takes a value, then
/// read back; with `capture`, the participant's values are read first, before any write. An
/// in-game revision (Java `runUpdate`) starts at the first write and writes only the values that
/// differ from what the engines confirmed.
struct Apply {
    /// Index into [`STAGES`].
    stage: usize,
    values: Grid<Option<f64>>,
    capture: [bool; 2],
    captured: Grid<Option<f64>>,
    set: Grid<bool>,
    legs: Vec<Leg>,
    deadline: Option<Instant>,
    /// This stage's failures, ordered by position when it completes.
    stage_failures: Vec<MatchParameterFailure>,
    failures: Vec<MatchParameterFailure>,
    revision: bool,
    /// The revision waits for the game's operation in flight (a komi change) before writing.
    settling: bool,
}

#[derive(Clone, Copy)]
enum Stage {
    Capture(usize),
    Set(usize),
    Read(usize),
}

const STAGES: [Stage; 6] = [
    Stage::Capture(0),
    Stage::Capture(1),
    Stage::Set(0),
    Stage::Read(0),
    Stage::Set(1),
    Stage::Read(1),
];

/// Giving the captured processes their values back, one command at a time.
struct Restore {
    queue: VecDeque<(usize, Purpose)>,
    /// The command in flight and the original (by index) it restores.
    leg: Option<(usize, Leg)>,
    deadline: Option<Instant>,
    /// Originals (by index) whose restore failed; their remaining commands are skipped.
    failed: Vec<usize>,
    failures: Vec<MatchParameterFailure>,
}

enum Work {
    Apply(Apply),
    Restore(Restore),
}

impl ParameterSession {
    pub(super) fn new(opening: &MatchOpeningParameters) -> Self {
        Self {
            opening: [
                [opening.first_wrn, opening.second_wrn],
                [opening.first_pda, opening.second_pda],
            ],
            originals: Vec::new(),
            dirty: false,
            game: None,
            work: None,
            late: Vec::new(),
            stopped: Vec::new(),
            to_stop: Vec::new(),
        }
    }

    /// Whether this batch stopped `instance` after failing to give its values back.
    pub(super) fn stopped(&self, instance: InstanceId) -> bool {
        self.stopped.contains(&instance)
    }

    /// The current game's values are pending or not confirmed: the game must not search.
    fn blocks_search(&self, game: u64) -> bool {
        self.game
            .as_ref()
            .is_some_and(|g| g.game == game && (g.pending || !g.failures.is_empty()))
    }

    fn deadline(&self) -> Option<Instant> {
        match &self.work {
            Some(Work::Apply(a)) => a.deadline,
            Some(Work::Restore(r)) => r.deadline,
            None => None,
        }
    }

    fn view(&self, game: u64) -> MatchParametersView {
        let Some(g) = self.game.as_ref().filter(|g| g.game == game) else {
            return MatchParametersView::default();
        };
        let value = |k: usize, p: usize| MatchParameterValue {
            supported: g.supported[k][p],
            confirmed: g.confirmed[k][p],
            target: g.target[k][p].filter(|t| !same(*t, g.confirmed[k][p])),
        };
        let position = |p| MatchParameterPosition {
            wrn: value(0, p),
            pda: value(1, p),
        };
        MatchParametersView {
            first: position(0),
            second: position(1),
            pending: g.pending,
            failures: g.failures.clone(),
        }
    }
}

fn failure(position: usize, reason: MatchParameterFailureReason) -> MatchParameterFailure {
    MatchParameterFailure {
        first: Some(position == 0),
        reason,
    }
}

/// A failure of the game as a whole, not of one participant (Java's `runUpdate` without a leg).
fn whole(reason: MatchParameterFailureReason) -> MatchParameterFailure {
    MatchParameterFailure { first: None, reason }
}

/// Java `EngineGameParameterSession.same`.
fn same(expected: f64, actual: Option<f64>) -> bool {
    actual.is_some_and(|a| (expected - a).abs() <= 1e-9)
}

/// Java `EngineGameParameterSession.unmet`: a requested value the engines have not confirmed.
fn unmet(target: &Grid<Option<f64>>, confirmed: &Grid<Option<f64>>) -> bool {
    (0..2).any(|k| (0..2).any(|p| target[k][p].is_some_and(|t| !same(t, confirmed[k][p]))))
}

/// Java `EngineGameParameterSession.valid`: breadth 0..=2, aggressiveness -3..=3.
fn valid(k: usize, value: f64) -> bool {
    if k == 0 {
        MatchOpeningParameters::valid_wrn(value)
    } else {
        MatchOpeningParameters::valid_pda(value)
    }
}

/// Java `EngineGameParameterSession.parse`: a finite number, else nothing.
fn parse(payload: &str) -> Option<f64> {
    payload.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}

/// `BigDecimal.valueOf(value).stripTrailingZeros().toPlainString()`: 0.5, 1, -1.5, never `-0`.
fn plain(value: f64) -> String {
    format!("{}", value + 0.0)
}

fn command_text(purpose: Purpose) -> String {
    match purpose {
        Purpose::Capture(k) | Purpose::Read(k, _) => format!("kata-get-param {}", NAMES[k]),
        Purpose::Set(k, value) => format!("kata-set-param {} {}", NAMES[k], plain(value)),
    }
}

/// The failure an answer means for `purpose`, or `None` when it confirms it.
fn check(purpose: Purpose, text: &str, ok: bool, ok_value: &mut Option<f64>) -> Option<MatchParameterFailureReason> {
    let rejected = || MatchParameterFailureReason::Rejected {
        command: command_text(purpose),
        response: text.trim().into(),
    };
    if !ok {
        return Some(rejected());
    }
    match purpose {
        Purpose::Set(..) => None,
        Purpose::Capture(_) => match parse(text) {
            Some(value) => {
                *ok_value = Some(value);
                None
            }
            None => Some(rejected()),
        },
        Purpose::Read(_, expected) => {
            if same(expected, parse(text)) {
                None
            } else {
                Some(MatchParameterFailureReason::Mismatch {
                    actual: text.trim().into(),
                    expected: plain(expected),
                })
            }
        }
    }
}

impl Workbench {
    pub(super) fn match_params_blocked(&self) -> bool {
        self.matches
            .run
            .as_ref()
            .is_some_and(|r| r.parameters.as_ref().is_some_and(|s| s.blocks_search(r.game)))
    }

    pub(super) fn match_params_view(&self) -> Option<MatchParametersView> {
        let r = self.matches.run.as_ref()?;
        Some(r.parameters.as_ref()?.view(r.game))
    }

    pub(super) fn match_params_deadline(&self) -> Option<Instant> {
        self.matches.run.as_ref()?.parameters.as_ref()?.deadline()
    }

    /// Whether `instance` was stopped by this batch's restore.
    pub(super) fn match_params_stopped(&self, instance: InstanceId) -> bool {
        self.matches
            .run
            .as_ref()
            .is_some_and(|r| r.parameters.as_ref().is_some_and(|s| s.stopped(instance)))
    }

    /// Engines of the batch may hold values of it that were not given back yet.
    pub(super) fn match_params_dirty(&self) -> bool {
        self.matches
            .run
            .as_ref()
            .is_some_and(|r| r.parameters.as_ref().is_some_and(|s| s.dirty))
    }

    /// The batch's end is giving the captured processes their values back.
    pub(super) fn match_params_restoring(&self) -> bool {
        self.matches.run.as_ref().is_some_and(|r| {
            r.parameters
                .as_ref()
                .is_some_and(|s| matches!(s.work, Some(Work::Restore(_))))
        })
    }

    /// The game's opening values (Java `applyOpening`), once both participants answered their
    /// setup: true when they are confirmed (or the batch has no session), false while they are
    /// written or after they failed. Every participant is checked before anything is sent, and
    /// a process new to the batch has its values captured before the first write. A participant
    /// that cannot take a parameter is left alone when its opening value is 0.
    pub(super) fn match_params_opening(&mut self) -> bool {
        let Some(r) = self.matches.run.as_mut() else {
            return false;
        };
        let game = r.game;
        let Some(s) = r.parameters.as_mut() else { return true };
        if let Some(g) = s.game.as_ref().filter(|g| g.game == game) {
            return !g.pending && g.failures.is_empty();
        }
        let mut supported = [[false; 2]; 2];
        let mut capture = [false; 2];
        let mut unsupported = None;
        for (p, side) in r.sides.iter().enumerate() {
            let id = side.instance;
            let katago = self.engines.is_katago(id);
            // A `KataGoPda` engine keeps its own aggressiveness strategy (Java `supports`; the
            // dynamic-PDA engines answer `getpda` and so are `KataGoPda` engines too).
            let own_pda = self.engines.is_kata_go_pda(id);
            for k in 0..2 {
                supported[k][p] = katago && (k == 0 || !own_pda);
                if !supported[k][p] && s.opening[k][p] != 0.0 && unsupported.is_none() {
                    unsupported = Some(failure(
                        p,
                        MatchParameterFailureReason::Unsupported {
                            parameter: PARAMETERS[k],
                        },
                    ));
                }
                capture[p] |= supported[k][p];
            }
            capture[p] &= !s.originals.iter().any(|o| o.instance == id);
        }
        let mut values = [[None; 2]; 2];
        for k in 0..2 {
            for p in 0..2 {
                if supported[k][p] {
                    values[k][p] = Some(s.opening[k][p]);
                }
            }
        }
        s.game = Some(GameParameters {
            game,
            supported,
            confirmed: [[None; 2]; 2],
            target: values,
            pending: unsupported.is_none(),
            failures: Vec::new(),
        });
        if let Some(unsupported) = unsupported {
            self.match_params_failed(vec![unsupported]);
            return false;
        }
        s.work = Some(Work::Apply(Apply {
            stage: 0,
            values,
            capture,
            captured: [[None; 2]; 2],
            set: [[false; 2]; 2],
            legs: Vec::new(),
            deadline: None,
            stage_failures: Vec::new(),
            failures: Vec::new(),
            revision: false,
            settling: false,
        }));
        self.match_params_send_stage();
        self.match_params_advance()
    }

    /// Sends the current stage's commands; a stage with nothing to send stays empty.
    fn match_params_send_stage(&mut self) {
        let Some(r) = self.matches.run.as_mut() else { return };
        let Some(Work::Apply(a)) = r.parameters.as_mut().and_then(|s| s.work.as_mut()) else {
            return;
        };
        for p in 0..2 {
            let purpose = match STAGES[a.stage] {
                Stage::Capture(k) if a.capture[p] && a.values[k][p].is_some() => Purpose::Capture(k),
                Stage::Set(k) => match a.values[k][p] {
                    Some(value) => Purpose::Set(k, value),
                    None => continue,
                },
                Stage::Read(k) if a.set[k][p] => Purpose::Read(k, a.values[k][p].unwrap()),
                _ => continue,
            };
            let text = command_text(purpose);
            let side = &mut r.sides[p];
            match self.engines.game_send(side.reservation, &text) {
                Some(command) => {
                    side.awaiting.push(command);
                    a.legs.push(Leg {
                        position: p,
                        seat: side.reservation,
                        command,
                        purpose,
                    });
                }
                None => a
                    .stage_failures
                    .push(failure(p, MatchParameterFailureReason::NotSent { command: text })),
            }
        }
        a.deadline = (!a.legs.is_empty()).then(|| Instant::now() + COMMAND_TIMEOUT);
    }

    /// Moves past settled stages. Returns true once the values are all confirmed; a failure ends
    /// the work and reports it.
    fn match_params_advance(&mut self) -> bool {
        loop {
            let Some(r) = self.matches.run.as_mut() else {
                return false;
            };
            let Some(s) = r.parameters.as_mut() else { return false };
            let Some(Work::Apply(a)) = s.work.as_mut() else {
                return false;
            };
            if !a.legs.is_empty() {
                return false;
            }
            a.stage_failures.sort_by_key(|f| f.first != Some(true));
            let stage_failures = std::mem::take(&mut a.stage_failures);
            let capturing = matches!(STAGES[a.stage], Stage::Capture(_));
            a.failures.extend(stage_failures);
            // Every participant is captured before the first write; a capture failure ends it.
            if capturing && !a.failures.is_empty() {
                let failures = std::mem::take(&mut a.failures);
                s.work = None;
                self.match_params_failed(failures);
                return false;
            }
            if matches!(STAGES[a.stage], Stage::Capture(1)) {
                for p in 0..2 {
                    if a.capture[p] {
                        s.originals.push(Original {
                            position: p,
                            instance: r.sides[p].instance,
                            values: [a.captured[0][p], a.captured[1][p]],
                        });
                        s.dirty = true;
                    }
                }
            }
            if matches!(STAGES[a.stage], Stage::Set(_)) {
                s.dirty = true;
            }
            a.stage += 1;
            if a.stage == STAGES.len() {
                let Some(Work::Apply(a)) = s.work.take() else {
                    unreachable!()
                };
                if a.revision {
                    self.match_params_revised(a.failures);
                    return false;
                }
                if !a.failures.is_empty() {
                    self.match_params_failed(a.failures);
                    return false;
                }
                if let Some(g) = s.game.as_mut() {
                    g.pending = false;
                }
                return true;
            }
            self.match_params_send_stage();
        }
    }

    /// The game's values were not confirmed. Java fails the game's start (`applyOpening` throws,
    /// `failEngineGameTransaction`): the batch ends and its processes get their values back. The
    /// notice names each participant and reason.
    fn match_params_failed(&mut self, failures: Vec<MatchParameterFailure>) {
        if let Some(g) = self
            .matches
            .run
            .as_mut()
            .and_then(|r| r.parameters.as_mut())
            .and_then(|s| s.game.as_mut())
        {
            g.pending = false;
            g.failures = failures.clone();
        }
        let mut notice = MatchNotice::new(OPENING_FAILED);
        notice.parameter_failures = failures;
        self.matches.notices.push(notice);
        self.match_end(MatchEnd::Retired, None);
    }

    /// 对局参数 (Java `reviseFor`): records new values for the playing analysis game `game` and
    /// writes those the engines have not confirmed. Every value is checked first: one out of range
    /// is refused with its message, one a participant cannot take with
    /// `EngineGameParameters.rejected`; nothing new to write is `Unchanged`. The game stops
    /// searching until the values are confirmed; a revision already writing takes the new values
    /// when its pass ends. A write that cannot be sent fails the revision before this returns, so
    /// the answer names the last view built before it: the later views show the outcome.
    pub(super) fn match_params_revise(&mut self, game: u64, request: Grid<Option<f64>>) -> CommandResult {
        for (k, values) in request.iter().enumerate() {
            if values.iter().flatten().any(|v| !valid(k, *v)) {
                return super::refused(if k == 0 {
                    "EngineGameParameters.invalidWrn"
                } else {
                    "EngineGameParameters.invalidPda"
                });
            }
        }
        let cursor = self.cursor;
        let Some(r) = self.matches.run.as_mut().filter(|r| {
            r.game == game
                && r.values.kind == GameKind::Analysis
                && matches!(r.phase, MatchPhase::Playing | MatchPhase::Paused)
        }) else {
            return super::refused(NOT_PLAYING);
        };
        let Some(s) = r.parameters.as_mut() else {
            return super::refused(NOT_PLAYING);
        };
        let Some(g) = s.game.as_mut().filter(|g| g.game == game) else {
            return super::refused(NOT_PLAYING);
        };
        let mut next = g.target;
        for k in 0..2 {
            for p in 0..2 {
                let Some(value) = request[k][p] else { continue };
                if !g.supported[k][p] {
                    return super::refused("EngineGameParameters.rejected");
                }
                next[k][p] = Some(value);
            }
        }
        let searching = !g.pending && g.failures.is_empty();
        if searching && !unmet(&next, &g.confirmed) {
            return CommandResult::Unchanged;
        }
        g.target = next;
        g.pending = true;
        // The revision takes the rest of the current move's time (Java keeps the move's start).
        if searching && r.move_time.0 == cursor {
            r.move_time.1 += std::mem::take(&mut r.move_time.2);
        }
        let start = s.work.is_none();
        if start {
            s.work = Some(Work::Apply(Apply {
                stage: 0,
                values: [[None; 2]; 2],
                capture: [false; 2],
                captured: [[None; 2]; 2],
                set: [[false; 2]; 2],
                legs: Vec::new(),
                deadline: None,
                stage_failures: Vec::new(),
                failures: Vec::new(),
                revision: true,
                settling: true,
            }));
        }
        self.sync_engine();
        if start {
            self.match_params_proceed();
        }
        CommandResult::ParametersRevised { after: self.view_seq() }
    }

    /// A settling revision writes once the game has no operation in flight (`runUpdate` waits for
    /// `operationsInFlight`); in an analysis game that is a komi change waiting for its answers.
    pub(super) fn match_params_proceed(&mut self) {
        let Some(r) = self.matches.run.as_mut() else { return };
        let busy = r.komi_in_flight.is_some() || !r.komi_awaiting.is_empty();
        let Some(Work::Apply(a)) = r.parameters.as_mut().and_then(|s| s.work.as_mut()) else {
            return;
        };
        if !a.settling {
            return;
        }
        if busy {
            a.deadline.get_or_insert_with(|| Instant::now() + TURN_WAIT);
            return;
        }
        self.match_params_pass();
    }

    /// One write pass of a revision: every value not confirmed yet, each unknown until its read-back
    /// confirms it (a timed-out set may still apply later).
    fn match_params_pass(&mut self) {
        let Some(s) = self.matches.run.as_mut().and_then(|r| r.parameters.as_mut()) else {
            return;
        };
        let Some(g) = s.game.as_mut() else { return };
        let mut values = [[None; 2]; 2];
        for k in 0..2 {
            for p in 0..2 {
                if let Some(target) = g.target[k][p].filter(|t| !same(*t, g.confirmed[k][p])) {
                    values[k][p] = Some(target);
                    g.confirmed[k][p] = None;
                }
            }
        }
        s.work = Some(Work::Apply(Apply {
            stage: STAGES.iter().position(|s| matches!(s, Stage::Set(_))).unwrap(),
            values,
            capture: [false; 2],
            captured: [[None; 2]; 2],
            set: [[false; 2]; 2],
            legs: Vec::new(),
            deadline: None,
            stage_failures: Vec::new(),
            failures: Vec::new(),
            revision: true,
            settling: false,
        }));
        self.match_params_send_stage();
        self.match_params_advance();
    }

    /// A revision pass ended. Any write may have changed a value, so earlier results give way
    /// (Java `invalidateAnalysis`). A failure pauses the game; values requested meanwhile get
    /// another pass; otherwise the game searches again, unless it was paused.
    fn match_params_revised(&mut self, failures: Vec<MatchParameterFailure>) {
        self.invalidate_analysis(AnalysisScope::All);
        if !failures.is_empty() {
            self.match_params_revision_failed(failures);
            return;
        }
        let Some(g) = self
            .matches
            .run
            .as_mut()
            .and_then(|r| r.parameters.as_mut())
            .and_then(|s| s.game.as_mut())
        else {
            return;
        };
        if unmet(&g.target, &g.confirmed) {
            self.match_params_pass();
            return;
        }
        g.pending = false;
        g.failures.clear();
        self.sync_engine();
    }

    /// Java `fail`: the game keeps the reasons and is paused; only a later revision that is
    /// confirmed resolves it. The notice names each participant and reason.
    fn match_params_revision_failed(&mut self, failures: Vec<MatchParameterFailure>) {
        let Some(r) = self.matches.run.as_mut() else { return };
        if let Some(g) = r.parameters.as_mut().and_then(|s| s.game.as_mut()) {
            g.pending = false;
            g.failures = failures.clone();
        }
        if r.phase == MatchPhase::Playing {
            r.phase = MatchPhase::Paused;
        }
        let mut notice = MatchNotice::new(REVISION_FAILED);
        notice.parameter_failures = failures;
        self.matches.notices.push(notice);
        self.match_dispatch();
        self.sync_engine();
    }

    /// The current game's values are not confirmed (Java `EngineGameParameterState.unresolved`):
    /// the game cannot continue until a revision is.
    pub(super) fn match_params_unresolved(&self) -> bool {
        self.matches.run.as_ref().is_some_and(|r| {
            r.parameters
                .as_ref()
                .and_then(|s| s.game.as_ref())
                .is_some_and(|g| g.game == r.game && !g.failures.is_empty())
        })
    }

    /// An answer to a session command; false when `command` is not one.
    pub(super) fn match_params_reply(&mut self, seat: u64, command: u64, ok: bool, text: &str) -> bool {
        let Some(r) = self.matches.run.as_mut() else {
            return false;
        };
        let Some(s) = r.parameters.as_mut() else { return false };
        if let Some(at) = s.late.iter().position(|l| *l == (seat, command)) {
            s.late.swap_remove(at);
            return true;
        }
        match s.work.as_mut() {
            Some(Work::Apply(a)) => {
                let Some(at) = a.legs.iter().position(|l| l.seat == seat && l.command == command) else {
                    return false;
                };
                let leg = a.legs.remove(at);
                let mut value = None;
                let p = leg.position;
                match check(leg.purpose, text, ok, &mut value) {
                    Some(reason) => {
                        a.stage_failures.push(failure(p, reason));
                        if matches!(leg.purpose, Purpose::Capture(_)) {
                            // confirmAll: the first failure ends the wait; the others answer nobody.
                            a.legs.clear();
                        }
                    }
                    None => match leg.purpose {
                        Purpose::Capture(k) => a.captured[k][p] = value,
                        Purpose::Set(k, _) => a.set[k][p] = true,
                        Purpose::Read(k, confirmed) => {
                            if let Some(g) = s.game.as_mut() {
                                g.confirmed[k][p] = Some(confirmed);
                            }
                            let side = &mut r.sides[p];
                            if k == 0 {
                                side.wrn = Some(confirmed)
                            } else {
                                side.pda = Some(confirmed)
                            }
                        }
                    },
                }
                self.match_params_advance();
                true
            }
            Some(Work::Restore(restore)) => {
                if !restore
                    .leg
                    .as_ref()
                    .is_some_and(|(_, l)| l.seat == seat && l.command == command)
                {
                    return false;
                }
                let (original, leg) = restore.leg.take().unwrap();
                let mut value = None;
                if let Some(reason) = check(leg.purpose, text, ok, &mut value) {
                    restore.failed.push(original);
                    restore.failures.push(failure(leg.position, reason));
                } else if let Purpose::Read(k, value) = leg.purpose {
                    let side = &mut r.sides[leg.position];
                    if k == 0 {
                        side.wrn = Some(value)
                    } else {
                        side.pda = Some(value)
                    }
                }
                restore.deadline = None;
                self.match_params_restore_next();
                true
            }
            None => false,
        }
    }

    /// Commands past their answer limit fail as timed out. Returns whether anything changed.
    pub(super) fn match_params_tick(&mut self, now: Instant) -> bool {
        let Some(r) = self.matches.run.as_mut() else {
            return false;
        };
        let Some(s) = r.parameters.as_mut() else { return false };
        if !s.deadline().is_some_and(|d| now >= d) {
            return false;
        }
        match s.work.as_mut() {
            Some(Work::Apply(a)) if a.settling => {
                // Java `runUpdate`: 等待当前一手超时, before anything was written.
                s.work = None;
                self.match_params_revision_failed(vec![whole(MatchParameterFailureReason::TurnTimeout)]);
            }
            Some(Work::Apply(a)) => {
                for leg in std::mem::take(&mut a.legs) {
                    r.sides[leg.position].awaiting.retain(|c| *c != leg.command);
                    s.late.push((leg.seat, leg.command));
                    a.stage_failures
                        .push(failure(leg.position, MatchParameterFailureReason::Timeout));
                }
                a.deadline = None;
                if self.match_params_advance() {
                    self.match_setup();
                }
            }
            Some(Work::Restore(restore)) => {
                if let Some((original, leg)) = restore.leg.take() {
                    r.sides[leg.position].awaiting.retain(|c| *c != leg.command);
                    s.late.push((leg.seat, leg.command));
                    restore.failed.push(original);
                    restore
                        .failures
                        .push(failure(leg.position, MatchParameterFailureReason::Timeout));
                }
                restore.deadline = None;
                self.match_params_restore_next();
                self.match_release();
            }
            None => return false,
        }
        true
    }

    /// The game ends while its values are still being written (Java: the worker is interrupted,
    /// or the transaction is gone before the answer): the write failed for every participant
    /// still owing an answer (for the game as a whole when a revision had not written yet), and
    /// late answers belong to nobody. A failed opening is reported; a revision's reasons stay
    /// with the game's parameters, as Java's panel shows them.
    pub(super) fn match_params_cut(&mut self, end: MatchEnd) {
        let Some(s) = self.matches.run.as_mut().and_then(|r| r.parameters.as_mut()) else {
            return;
        };
        if !matches!(s.work, Some(Work::Apply(_))) {
            return;
        }
        let Some(Work::Apply(a)) = s.work.take() else {
            unreachable!()
        };
        let reason = if end == MatchEnd::UserStopped {
            MatchParameterFailureReason::Interrupted
        } else {
            MatchParameterFailureReason::GameEnded
        };
        let mut failures = a.failures;
        failures.extend(a.stage_failures);
        for p in 0..2 {
            if a.legs.iter().any(|l| l.position == p) {
                failures.push(failure(p, reason.clone()));
            }
        }
        if a.settling {
            failures.push(whole(reason));
        }
        failures.sort_by_key(|f| f.first != Some(true));
        if let Some(g) = s.game.as_mut() {
            g.pending = false;
            g.failures = failures.clone();
        }
        if !a.revision {
            let mut notice = MatchNotice::new(OPENING_FAILED);
            notice.parameter_failures = failures;
            self.matches.notices.push(notice);
        }
    }

    /// The batch ends (Java `restoreOriginals`): `None` while the captured processes are being
    /// given their values back, then the processes to stop, whose values could not be restored.
    /// A process that is gone is not written (its replacement keeps its own state) and is
    /// reported; a failed restore is reported and its process is stopped.
    pub(super) fn match_params_restored(&mut self) -> Option<Vec<InstanceId>> {
        let Some(r) = self.matches.run.as_mut() else {
            return Some(Vec::new());
        };
        let Some(s) = r.parameters.as_mut() else {
            return Some(Vec::new());
        };
        if matches!(s.work, Some(Work::Restore(_))) {
            return None;
        }
        if s.dirty {
            s.dirty = false;
            let mut queue = VecDeque::new();
            for (index, original) in s.originals.iter().enumerate() {
                for k in 0..2 {
                    if let Some(value) = original.values[k] {
                        queue.push_back((index, Purpose::Set(k, value)));
                        queue.push_back((index, Purpose::Read(k, value)));
                    }
                }
            }
            s.work = Some(Work::Restore(Restore {
                queue,
                leg: None,
                deadline: None,
                failed: Vec::new(),
                failures: Vec::new(),
            }));
            self.match_params_restore_next();
        }
        let s = self.matches.run.as_mut()?.parameters.as_mut()?;
        if s.work.is_some() {
            return None;
        }
        Some(std::mem::take(&mut s.to_stop))
    }

    /// Sends the next restore command, one at a time in Java's order (each process, breadth then
    /// aggressiveness, set then read back; a process's first failure skips the rest of it), or
    /// finishes the restore.
    fn match_params_restore_next(&mut self) {
        let Some(r) = self.matches.run.as_mut() else { return };
        let Some(s) = r.parameters.as_mut() else { return };
        let Some(Work::Restore(restore)) = s.work.as_mut() else {
            return;
        };
        if restore.leg.is_some() {
            return;
        }
        while let Some((index, purpose)) = restore.queue.pop_front() {
            if restore.failed.contains(&index) {
                continue;
            }
            let original = &s.originals[index];
            let side = &mut r.sides[original.position];
            if self.engines.phase(original.instance).is_none() {
                restore.failed.push(index);
                restore
                    .failures
                    .push(failure(original.position, MatchParameterFailureReason::ProcessChanged));
                continue;
            }
            let text = command_text(purpose);
            let sent = if self.engines.game_instance(side.reservation) == Some(original.instance) {
                self.engines.game_send(side.reservation, &text)
            } else {
                None
            };
            let Some(command) = sent else {
                restore.failed.push(index);
                restore.failures.push(failure(
                    original.position,
                    MatchParameterFailureReason::NotSent { command: text },
                ));
                continue;
            };
            side.awaiting.push(command);
            let leg = Leg {
                position: original.position,
                seat: side.reservation,
                command,
                purpose,
            };
            restore.leg = Some((index, leg));
            restore.deadline = Some(Instant::now() + COMMAND_TIMEOUT);
            return;
        }
        let Some(Work::Restore(restore)) = s.work.take() else {
            unreachable!()
        };
        for index in restore.failed {
            let instance = s.originals[index].instance;
            if self.engines.phase(instance).is_some() && !s.stopped.contains(&instance) {
                s.stopped.push(instance);
                s.to_stop.push(instance);
            }
        }
        if !restore.failures.is_empty() {
            let mut notice = MatchNotice::new("EngineGameParameters.restoreFailed");
            notice.parameter_failures = restore.failures;
            self.matches.notices.push(notice);
        }
    }
}

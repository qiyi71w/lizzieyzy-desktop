//! Engine runtime (引擎运行): engine instances, their protocol streams, command order and
//! confirmation, request ownership and process exit.
//!
//! The runtime does no I/O of its own: a [`Launcher`] starts processes and hands back an
//! [`EngineIo`] for writing; whatever the process prints comes back through [`Engines::input`]
//! with the instance it belongs to. Every command carries a GTP id and waits for its own answer,
//! so the runtime knows which position an engine holds and which request an analysis line
//! answers. Output of an instance that has been retired is ignored.
//!
//! The application flow says what it wants ([`Engines::want`]): the position the foreground
//! engine should hold and whether to analyse it. The runtime brings the engine there with at
//! most one batch of commands in flight; a newer wish made meanwhile replaces the pending one,
//! so quick navigation does not queue a command batch per step.

use std::collections::{HashMap, VecDeque};
use std::io::Write;
use std::hash::BuildHasher;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use ts_rs::TS;

use super::gtp::{self, Dialect, Frame, Framer, InfoLine, LaunchSpec};
use super::profile::EngineProfile;
use super::raw_net::{self, RawNetMode, RawNetView};
use super::rules::KataRules;
use crate::record::{BoardSize, GameRecord, NodeId, NodeKind, Point, Stone, ROOT};

pub type InstanceId = u64;

/// What a process produced.
#[derive(Clone, Debug, PartialEq)]
pub enum EngineInput {
    Stdout(String),
    Stderr(String),
    /// The process ended (exit code if known).
    Exited(Option<i32>),
}

/// Writing side of a running engine process.
pub trait EngineIo: Send {
    /// Writes one line; must not block on a slow reader.
    fn send(&mut self, line: &str) -> std::io::Result<()>;
    /// Ends the process: `quit`, then a forced kill if it has not exited in time. Returns at
    /// once.
    fn terminate(self: Box<Self>);
}

/// Starts engine processes. The real launcher spawns OS processes; tests use a controlled one.
pub trait Launcher: Send {
    fn launch(&self, instance: InstanceId, spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>>;
}

/// The engine kind decided from `name` (`checkNameAndVersionForCurrentReader`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineKind {
    KataGo,
    LeelaZero,
    Sayuri,
    Sai,
    Leela011,
    Other,
}

/// The position an engine should hold: one node of one document at a komi, and the rules the
/// session requires (`None`: whatever the engine has). The node's [`stamp`](crate::record::Node::stamp)
/// is part of its identity: a node edited in place is another position.
#[derive(Clone)]
pub struct Target {
    pub doc: u64,
    pub node: NodeId,
    pub record: Arc<GameRecord>,
    pub komi: f64,
    pub rules: Option<KataRules>,
    /// Generation of the session rules this position is analysed under; results of another
    /// generation are not this target's.
    pub rules_rev: u64,
}

/// Progress of making an engine use the session rules (`SessionRulesSynchronizer`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RulesOutcome {
    /// The engine confirmed exactly these rules.
    Confirmed,
    /// `kata-set-rules` / `kata-get-rules` not answered yet (or not sent).
    Pending,
    /// Java `SessionRulesSynchronizer.Failure` name.
    Failed(&'static str),
}

impl Target {
    fn key(&self) -> TargetKey {
        TargetKey {
            doc: self.doc,
            node: self.node,
            stamp: self.record.node(self.node).stamp,
            komi_bits: self.komi.to_bits(),
            rules_rev: self.rules_rev,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TargetKey {
    doc: u64,
    node: NodeId,
    stamp: u64,
    komi_bits: u64,
    rules_rev: u64,
}

/// Why an engine failed, for the failure dialog and the log.
#[derive(Clone, Debug, PartialEq)]
pub struct EngineFailure {
    pub instance: InstanceId,
    pub profile_id: String,
    pub engine_name: String,
    pub command: String,
    /// Whether it failed before it was ready.
    pub during_startup: bool,
    pub message: String,
    /// Last lines the engine printed (stderr and non-analysis stdout).
    pub output: Vec<String>,
}

/// Something the application flow must act on.
#[derive(Clone, Debug, PartialEq)]
pub enum RuntimeEvent {
    /// The engine finished start-up (identification and initial commands answered).
    Ready { instance: InstanceId },
    /// One analysis line of a request, for the position the request was made for.
    Analysis {
        instance: InstanceId,
        doc: u64,
        node: NodeId,
        /// The node's stamp when the request was made.
        stamp: u64,
        komi: f64,
        rules_rev: u64,
        to_play: Stone,
        engine_name: String,
        line: InfoLine,
        /// Time since the request started.
        elapsed: Duration,
    },
    /// The engine answered `kata-get-rules` or refused session rules: whether analysis may run
    /// can have changed.
    Rules { instance: InstanceId },
    /// A complete pure-network answer became available for the current foreground target.
    RawNet { instance: InstanceId },
    /// The process failed to start or ended unexpectedly; the instance is gone.
    Failed(EngineFailure),
    /// The borrowed foreground engine stopped and its rules were captured: searches may start.
    LeaseReady { lease: u64 },
    /// A finite search of the borrowed engine reached its visits, for the position it was made
    /// for.
    LeaseResult {
        lease: u64,
        doc: u64,
        node: NodeId,
        stamp: u64,
        komi: f64,
        rules_rev: u64,
        to_play: Stone,
        line: InfoLine,
    },
    /// A finite search of the borrowed engine stalled, was refused or produced nothing.
    LeaseSearchFailed { lease: u64 },
    /// The lease is over: the engine was given back (`None`), or giving it back failed.
    LeaseEnded { lease: u64, failure: Option<LeaseFailure> },
    /// One stdout line of an analysis process.
    WorkerLine { instance: InstanceId, line: String },
    /// A deferred analysis worker's fingerprint check and process launch completed.
    WorkerReady { instance: InstanceId },
    WorkerLaunchFailed { instance: InstanceId, message: String },
    /// An analysis process ended by itself (or could not be written to); it is gone.
    WorkerExited {
        instance: InstanceId,
        message: String,
        output: Vec<String>,
    },
    /// The focus points an instance's analysis stream searches changed: a stream with them was
    /// accepted, or the stream that searched them ended.
    FocusConfirmed { instance: InstanceId },
}

/// Whether the foreground engine can be borrowed (Java `ExclusiveGtpLeaseAvailability`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseAvailability {
    Available,
    NoForegroundEngine,
    EngineNotReady,
    NotKatago,
    MissingCapability,
    ExistingLease,
    EngineStateUnrestored,
}

/// How borrowing or giving back the foreground engine failed (Java
/// `ForegroundAnalysisLeaseFailure`). The `Final*`, `RestoreFailed` and `TransportClosed` ones
/// leave the engine unrestored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseFailure {
    InitialStopError,
    InitialStopTimeout,
    FinalStopError,
    FinalStopTimeout,
    RestoreFailed,
    TransportClosed,
}

impl LeaseFailure {
    /// Whether the engine's state is unknown afterwards.
    pub fn unrestores(self) -> bool {
        !matches!(self, LeaseFailure::InitialStopError | LeaseFailure::InitialStopTimeout)
    }
}

/// Commands a borrowed engine needs (Java `FLASH_ANALYSIS_GTP_COMMANDS`).
const LEASE_COMMANDS: [&str; 9] = [
    "stop",
    "boardsize",
    "komi",
    "kata-get-rules",
    "kata-set-rules",
    "clear_board",
    "play",
    "set_position",
    "kata-analyze",
];
/// Java `FOREGROUND_INITIAL_STOP_TIMEOUT_MILLIS`, `REMOTE_GTP_SETUP_ACK_TIMEOUT_MILLIS`,
/// `REMOTE_GTP_ANALYZE_STALL_TIMEOUT_MILLIS`, `FOREGROUND_RELEASE_STOP_TIMEOUT_MILLIS`.
const LEASE_ACK: Duration = Duration::from_millis(8000);
/// Java `REMOTE_GTP_STOP_ACK_TIMEOUT_MILLIS`.
const LEASE_STOP_ACK: Duration = Duration::from_millis(1200);
/// Java's foreground restore watchdog.
const LEASE_RESTORE: Duration = Duration::from_millis(30_000);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LeaseStep {
    /// The `stop` that takes the engine over.
    InitialStop(u64),
    /// `kata-get-rules`, kept for giving the engine back.
    Capture(u64),
    Idle,
    /// A finite search in flight.
    Search,
    /// The `stop` that ends a search.
    SearchStop(u64),
    /// The `stop` that starts giving the engine back.
    FinalStop(u64),
    /// Rules and position restored; `name` confirms the engine follows again.
    Restore(u64),
}

struct LeaseSearch {
    analyze: u64,
    request: Request,
    target_visits: u64,
    best: Option<InfoLine>,
    failed: bool,
}

/// The foreground engine lent to a background task (Java `ForegroundAnalysisLease`): the seat's
/// wish is not followed until the engine is given back.
struct Lease {
    id: u64,
    instance: InstanceId,
    step: LeaseStep,
    deadline: Option<Instant>,
    captured_rules: Option<String>,
    rules_changed: bool,
    releasing: bool,
    restore_failed: bool,
    search: Option<LeaseSearch>,
}

/// A KataGo analysis process (`katago analysis`): JSON lines in and out, no GTP.
struct Worker {
    id: InstanceId,
    name: String,
    io: Option<Box<dyn EngineIo>>,
    tail: VecDeque<String>,
    queued: Option<Vec<String>>,
}

/// A console entry (GtpConsolePane): sent commands, engine output and errors.
#[derive(Clone, Debug, PartialEq)]
pub struct ConsoleLine {
    pub seq: u64,
    pub engine: String,
    pub kind: ConsoleKind,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsoleKind {
    Sent,
    Output,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Process running; identification / initial commands not all answered yet.
    Starting,
    Ready,
}

#[derive(Clone, Copy, Debug)]
enum Purpose {
    Name,
    Version,
    ListCommands,
    Setup,
    GetRules,
    SetRules,
    Sync,
    Analyze,
    RawNet,
    /// Leela 0.11 starts pondering with an ordinary, immediately answered GTP command.
    Ponder,
    /// Ends a pondering cycle to obtain its stderr summary, without changing its owner.
    Probe,
    Stop,
    User,
    /// A command of the foreground lease; its answer goes to the lease.
    Lease,
    /// The start-up probe `kata-analyze … focus pass 0`: its answer decides [`FocusCapability`].
    FocusProbe,
}

/// What the engine will hold once it has processed every command sent so far: the anchor
/// position and the actions after it, by node stamp (a removed or edited node never matches a
/// node of the current record).
#[derive(Clone, Debug, PartialEq)]
struct EngineBoard {
    doc: u64,
    size: BoardSize,
    komi_bits: u64,
    anchor: u64,
    actions: Vec<u64>,
}

/// An analysis request: which position it is for.
#[derive(Clone, Copy)]
struct Request {
    key: TargetKey,
    to_play: Stone,
    size: BoardSize,
    started: Instant,
}

/// Leela prints its heatmap on stderr and acknowledges on stdout. Hold the command fence
/// until both arrive: the two reader threads may deliver them in either order, but no later
/// position may claim this unnumbered table.
struct RawRequest {
    id: u64,
    request: Request,
    revision: u64,
    mode: RawNetMode,
    text: String,
    response: Option<bool>,
    complete: bool,
}

struct Instance {
    id: InstanceId,
    profile: EngineProfile,
    launch_threads: u32,
    io: Option<Box<dyn EngineIo>>,
    /// Lines written before the process exists (its launch waits for a [`LaunchCheck`]).
    queued: Option<Vec<String>>,
    phase: Phase,
    gtp_name: String,
    version: String,
    kind: EngineKind,
    no_analyze: bool,
    commands: Vec<String>,
    next_id: u64,
    waiting: HashMap<u64, Purpose>,
    framer: Framer,
    boot: Option<Bootstrap>,
    ready_fence: Option<u64>,
    /// The last command of the batch in flight; no new batch is sent until it is answered.
    fence: Option<u64>,
    board: Option<EngineBoard>,
    /// GTP id of the analysis stream the engine is producing (or about to).
    stream: Option<u64>,
    /// What the stream was asked to report besides the candidates (KataGo).
    stream_options: StreamOptions,
    /// Analysis requests by GTP id, so late lines still reach the position they were for.
    requests: VecDeque<(u64, Request)>,
    /// One summary per Leela 0.11 pondering cycle, retained across stop/navigation: stderr
    /// has no command ids, so its tables must be consumed in engine order, not reassigned.
    summaries: VecDeque<(u64, Request, InfoLine)>,
    ponder_due: Option<Instant>,
    /// A position whose sync the engine refused; not retried until the wish changes.
    rejected: Option<TargetKey>,
    /// The position the sync commands in flight are for.
    syncing: Option<TargetKey>,
    synced: bool,
    output_tail: VecDeque<String>,
    /// Rules the engine last reported (`kata-get-rules`).
    confirmed_rules: Option<KataRules>,
    /// Session rules sent to it and how that went.
    rules_request: Option<(KataRules, RulesOutcome)>,
    raw_mode: RawNetMode,
    raw_revision: u64,
    raw_requested: Option<(TargetKey, u64)>,
    raw_request: Option<RawRequest>,
    raw_data: Option<(TargetKey, RawNetView)>,
    /// Giving the engine back after a lease failed: its state is unknown and it no longer
    /// follows the seat's wish until it is restarted.
    unrestored: Option<LeaseFailure>,
    /// Whether this process accepts `focus` (its own start-up probe decided it).
    focus: FocusCapability,
    /// The probe in flight: its GTP id and the arguments sent after `kata-analyze`.
    focus_probe: Option<(u64, String)>,
    /// Region and focus arguments of the analysis stream, its region arguments alone, and the
    /// focus points it asked for.
    stream_args: String,
    stream_region: String,
    stream_focus: Vec<Point>,
    /// Focus points the engine accepted (the header of the stream that asked for them arrived)
    /// and the request that asked for them; empty once that stream ended.
    installed_focus: Vec<Point>,
    installed_from: Option<u64>,
}

const OUTPUT_TAIL: usize = 40;
const REQUEST_MEMORY: usize = 16;
const CONSOLE_LIMIT: usize = 2000;
const PONDER_INTERVAL: Duration = Duration::from_millis(1000);
/// Commands that change the engine's board (`Leelaz.analysisStateMutation`, genmove forms); the
/// console does not pass them through because the record, not the engine, owns the position.
const BOARD_COMMANDS: [&str; 16] = [
    "play",
    "genmove",
    "lz-genmove",
    "kata-genmove",
    "genmove_analyze",
    "lz-genmove_analyze",
    "kata-genmove_analyze",
    "undo",
    "clear_board",
    "boardsize",
    "rectangular_boardsize",
    "loadsgf",
    "set_position",
    "fixed_handicap",
    "place_free_handicap",
    "set_free_handicap",
];

/// Start-up settings that come from the application, not the profile.
#[derive(Clone, Debug)]
pub struct Bootstrap {
    pub komi: f64,
    /// Rules applied to KataGo at start-up (`auto-load-kata-rules`).
    pub rules: Option<KataRules>,
}

/// A start whose launch overlays depend on artifact content (measured or Apple tuning). Java
/// `MeasuredKataGoTuning.verifyFingerprint` requires a worker thread, so the caller runs
/// [`LaunchCheck::run`] off the workbench and hands the result to [`Engines::finish_launch`].
pub struct LaunchCheck {
    instance: InstanceId,
    command: String,
    policy: super::tuning::ThreadPolicy,
    roots: Vec<PathBuf>,
    spec: LaunchSpec,
    apple: Option<serde_json::Value>,
    measured: Option<(String, String, u64)>,
    /// Saved GTP command when checking a whole-game analysis worker instead of a live engine.
    whole_game: Option<String>,
}

/// The launch a [`LaunchCheck`] decided, for the instance it was made for.
pub struct CheckedLaunch {
    instance: InstanceId,
    spec: LaunchSpec,
}

impl LaunchCheck {
    /// Reads the engine, model and configuration and applies the overlays that still match.
    pub fn run(self) -> CheckedLaunch {
        let mut spec = self.spec;
        if let Some(hardware) = &self.apple {
            super::tuning::apply_apple_launch_policy(&self.command, &self.policy, &mut spec, hardware);
        }
        if let Some(hardware) = &self.measured {
            if let Some(saved) = &self.whole_game {
                super::tuning::apply_measured_whole_game(&self.command, saved, &self.policy, &self.roots,
                    &mut spec, hardware);
            } else {
                super::tuning::apply_measured_live(&self.command, &self.policy, &self.roots, &mut spec,
                    &hardware.0, &hardware.1, hardware.2);
            }
        }
        CheckedLaunch {
            instance: self.instance,
            spec,
        }
    }
}

/// The search threads a launch forces; `kata-set-param` repeats them once the engine is ready.
fn launch_threads(command: &str, spec: &LaunchSpec) -> u32 {
    if super::tuning::has_search_threads(command) {
        0
    } else {
        super::tuning::overrides(&spec.args)
            .get("numsearchthreads")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserCommandError {
    Empty,
    NoEngine,
    /// The command would change the engine's board behind the record's back.
    ChangesBoard,
}

/// Optional `kata-analyze` reports (`Leelaz.addKataTag`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct StreamOptions {
    pv_visits: bool,
    ownership: bool,
    moves_ownership: bool,
}

/// Whether an engine accepts `kata-analyze … focus` (选点重点研究). Each process is probed during
/// its own start-up, whatever the analysis pause; the capability is never inferred from a version
/// or a download source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, TS)]
pub enum FocusCapability {
    /// Not decided: still starting, or not a KataGo speaking `kata-analyze`.
    #[default]
    Unknown,
    Supported,
    /// The engine rejected the `focus` option (`unknown analyze option focus` or KataGo's
    /// argument error for exactly the probe's arguments).
    Unsupported,
    /// The probe was answered with another error: unknown until the engine starts again.
    Failed,
}

/// 强制计算区域 (`LizzieFrame.allowcoords` / `avoidcoords`): the foreground engine's analysis
/// only considers (`allow`) or never considers (`avoid`) these points for both colours, up to
/// `until_move` moves deep (Java sends at least 1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchRegion {
    pub allow: bool,
    pub points: Vec<Point>,
    pub until_move: u32,
}

/// 选点重点研究: extra search directed into `points` (`focus … 0.5`), for one instance and one
/// position (document, node and its stamp). It is sent only to that instance, only while it
/// holds that position and only when the instance supports `focus`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchFocus {
    pub instance: InstanceId,
    pub doc: u64,
    pub node: NodeId,
    pub stamp: u64,
    pub points: Vec<Point>,
}

/// The region and focus arguments of one analysis request.
#[derive(Default)]
struct StreamArgs {
    region: String,
    focus: String,
    focus_points: Vec<Point>,
}

impl StreamArgs {
    fn text(&self) -> String {
        format!("{}{}", self.region, self.focus)
    }
}

pub struct Engines {
    launcher: Box<dyn Launcher>,
    roots: Vec<PathBuf>,
    library_dirs: Vec<PathBuf>,
    measured_hardware: Option<(String, String, u64)>,
    apple_hardware: Option<serde_json::Value>,
    instances: Vec<Instance>,
    checks: Vec<LaunchCheck>,
    next_instance: InstanceId,
    foreground: Option<InstanceId>,
    wish: Option<(Target, bool)>,
    /// 双引擎模式's secondary engine (Java `Lizzie.leelaz2`): never the foreground engine. It
    /// holds the position of its own wish; results name their instance, so they are never taken
    /// for the foreground engine's.
    secondary: Option<InstanceId>,
    secondary_wish: Option<(Target, bool)>,
    interval_centisec: u32,
    /// What KataGo analysis also reports: `show-pv-visits` (each variation move's visits) and
    /// Kata形势 (`ownership`, with 按选点显示评估 `movesOwnership`).
    options: StreamOptions,
    console: VecDeque<ConsoleLine>,
    console_seq: u64,
    trace: Option<Box<dyn Write + Send>>,
    lease: Option<Lease>,
    next_lease: u64,
    workers: Vec<Worker>,
    /// Events produced outside engine output, handed out with the next events.
    pending: Vec<RuntimeEvent>,
    /// The foreground engine's 强制计算区域 and 选点重点研究.
    region: Option<SearchRegion>,
    focus: Option<SearchFocus>,
}

impl Instance {
    fn display_name(&self) -> String {
        if self.profile.name.is_empty() {
            self.gtp_name.clone()
        } else {
            self.profile.name.clone()
        }
    }

    fn has(&self, command: &str) -> bool {
        self.commands.iter().any(|c| c == command)
    }

    fn dialect(&self) -> Option<Dialect> {
        if self.no_analyze {
            return None;
        }
        match self.kind {
            EngineKind::Sayuri => Some(Dialect::Sayuri),
            EngineKind::Sai => Some(Dialect::Sai),
            EngineKind::Leela011 => Some(Dialect::Leela011),
            _ if self.has("kata-analyze") => Some(Dialect::Kata),
            _ if self.has("lz-analyze") => Some(Dialect::Leela),
            _ => None,
        }
    }

    /// `supportMovesOwnership`: KataGo after 1.10 (`Leelaz.java:6298-6304`).
    fn supports_moves_ownership(&self) -> bool {
        let v: Vec<u32> = self
            .version
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .take(2)
            .filter_map(|s| s.parse().ok())
            .collect();
        self.kind == EngineKind::KataGo && v.len() == 2 && (v[0] > 1 || v[1] > 10)
    }

    /// `supportRootInfo`: KataGo 1.16.4 and later report `rootInfo`.
    fn supports_root_info(&self) -> bool {
        let v: Vec<u32> = self
            .version
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .take(3)
            .filter_map(|s| s.parse().ok())
            .collect();
        let (major, minor, patch) = (
            v.first().copied().unwrap_or(0),
            v.get(1).copied().unwrap_or(0),
            v.get(2).copied().unwrap_or(0),
        );
        self.kind == EngineKind::KataGo && (major > 1 || (major == 1 && (minor > 16 || (minor == 16 && patch >= 4))))
    }

    fn request(&self, n: u64) -> Option<Request> {
        self.requests.iter().find(|(id, _)| *id == n).map(|(_, r)| *r)
    }

    fn failure(&self, message: String) -> EngineFailure {
        EngineFailure {
            instance: self.id,
            profile_id: self.profile.id.clone(),
            engine_name: self.display_name(),
            command: self.profile.command.clone(),
            during_startup: self.phase == Phase::Starting,
            message,
            output: self.output_tail.iter().cloned().collect(),
        }
    }
}

impl Engines {
    /// `roots`: directories relative engine paths are looked up in (Java's candidate roots).
    pub fn new(launcher: Box<dyn Launcher>, roots: Vec<PathBuf>) -> Engines {
        Engines {
            launcher,
            roots,
            library_dirs: Vec::new(),
            measured_hardware: None,
            apple_hardware: None,
            instances: Vec::new(),
            checks: Vec::new(),
            next_instance: 1,
            foreground: None,
            wish: None,
            secondary: None,
            secondary_wish: None,
            interval_centisec: 10,
            options: StreamOptions::default(),
            console: VecDeque::new(),
            console_seq: 0,
            trace: None,
            lease: None,
            next_lease: 1,
            workers: Vec::new(),
            pending: Vec::new(),
            region: None,
            focus: None,
        }
    }

    fn get(&self, id: InstanceId) -> Option<&Instance> {
        self.instances.iter().find(|i| i.id == id)
    }

    fn get_mut(&mut self, id: InstanceId) -> Option<&mut Instance> {
        self.instances.iter_mut().find(|i| i.id == id)
    }

    // -------------------------------------------------------------------- queries

    pub fn foreground(&self) -> Option<InstanceId> {
        self.foreground
    }

    pub fn secondary(&self) -> Option<InstanceId> {
        self.secondary
    }

    /// The foreground or the secondary engine: an instance the runtime drives to a wished
    /// position.
    fn seated(&self, id: InstanceId) -> bool {
        self.foreground == Some(id) || self.secondary == Some(id)
    }

    /// What the seat of `id` should do; nothing for an unseated (kept or preloaded) instance.
    fn wish_of(&self, id: InstanceId) -> Option<&(Target, bool)> {
        if self.foreground == Some(id) {
            self.wish.as_ref()
        } else if self.secondary == Some(id) {
            self.secondary_wish.as_ref()
        } else {
            None
        }
    }

    pub fn instance_of(&self, profile_id: &str) -> Option<InstanceId> {
        self.instances.iter().find(|i| i.profile.id == profile_id).map(|i| i.id)
    }

    pub fn profile_of(&self, id: InstanceId) -> Option<&EngineProfile> {
        self.get(id).map(|i| &i.profile)
    }

    /// Every running instance: profile id and phase.
    pub fn running(&self) -> Vec<(InstanceId, String, Phase)> {
        self.instances
            .iter()
            .map(|i| (i.id, i.profile.id.clone(), i.phase))
            .collect()
    }

    pub fn phase(&self, id: InstanceId) -> Option<Phase> {
        self.get(id).map(|i| i.phase)
    }

    /// GTP `name` and `version` answers and the kind decided from them.
    pub fn identity(&self, id: InstanceId) -> Option<(String, String, EngineKind)> {
        self.get(id).map(|i| (i.gtp_name.clone(), i.version.clone(), i.kind))
    }

    pub fn display_name(&self, id: InstanceId) -> Option<String> {
        self.get(id).map(Instance::display_name)
    }

    /// Foreground switch complete: identified, start-up answered and the board synced to the
    /// wished position at least once.
    pub fn foreground_complete(&self) -> bool {
        self.foreground.is_some_and(|id| self.complete(id))
    }

    /// Identified, start-up answered and synced to its wished position at least once.
    pub fn complete(&self, id: InstanceId) -> bool {
        self.get(id).is_some_and(|i| i.phase == Phase::Ready && i.synced)
    }

    /// Whether the foreground engine is producing analysis for this node.
    pub fn analysing(&self, doc: u64, node: NodeId) -> bool {
        self.foreground.is_some_and(|id| self.analysing_by(id, doc, node))
    }

    /// Whether instance `id` is producing analysis for this node.
    pub fn analysing_by(&self, id: InstanceId, doc: u64, node: NodeId) -> bool {
        self.get(id).is_some_and(|i| {
            i.stream
                .and_then(|n| i.request(n))
                .is_some_and(|r| r.key.doc == doc && r.key.node == node)
        })
    }

    /// Whether the identified engine supports local analysis.
    pub fn can_analyse(&self, id: InstanceId) -> bool {
        self.get(id).is_some_and(|i| i.dialect().is_some())
    }

    pub fn is_katago(&self, id: InstanceId) -> bool {
        self.get(id).is_some_and(|i| i.kind == EngineKind::KataGo)
    }

    pub fn supports(&self, id: InstanceId, command: &str) -> bool {
        self.get(id).is_some_and(|i| i.has(command))
    }

    /// What instance `id`'s start-up probe found about `focus`.
    pub fn focus_capability(&self, id: InstanceId) -> FocusCapability {
        self.get(id).map_or(FocusCapability::Unknown, |i| i.focus)
    }

    /// The focus points instance `id`'s current analysis stream was accepted with.
    pub fn installed_focus(&self, id: InstanceId) -> &[Point] {
        self.get(id).map_or(&[], |i| &i.installed_focus)
    }

    /// `list_commands` of the engine (GtpConsolePane 常用命令).
    pub fn known_commands(&self, id: InstanceId) -> Vec<String> {
        self.get(id).map(|i| i.commands.clone()).unwrap_or_default()
    }

    pub fn console_seq(&self) -> u64 {
        self.console_seq
    }

    pub fn console_since(&self, seq: u64) -> Vec<ConsoleLine> {
        self.console.iter().filter(|l| l.seq > seq).cloned().collect()
    }

    // -------------------------------------------------------------------- settings
    /// Java `KataGoRuntimeHelper`: Windows NVIDIA DLL lookup for subsequent starts.
    pub fn set_library_dirs(&mut self, dirs: Vec<PathBuf>) {
        self.library_dirs = dirs;
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }
    pub fn library_dirs(&self) -> &[PathBuf] {
        &self.library_dirs
    }
    /// Session GPU identity captured off-lock by the shell (resources detection can refresh it).
    pub fn set_measured_hardware(&mut self, hardware: Option<(String, String, u64)>) -> bool {
        if self.measured_hardware == hardware {
            return false;
        }
        self.measured_hardware = hardware;
        true
    }
    pub fn set_apple_hardware(&mut self, hardware: Option<serde_json::Value>) -> bool {
        if self.apple_hardware == hardware {
            return false;
        }
        self.apple_hardware = hardware;
        true
    }
    pub fn apple_hardware(&self) -> Option<&serde_json::Value> {
        self.apple_hardware.as_ref()
    }

    /// Update the running policy; launch-only changes require the caller to reload.
    pub fn apply_thread_policy(&mut self, profile: &EngineProfile) {
        let Some(id) = self.instance_of(&profile.id) else {
            return;
        };
        let threads = profile
            .thread_policy
            .as_ref()
            .map_or(0, |p| p.threads_for_launch(&profile.command));
        if let Some(inst) = self.get_mut(id) {
            inst.profile.thread_policy = profile.thread_policy.clone();
            inst.launch_threads = threads;
        }
        if threads > 0 && self.is_katago(id) {
            self.send(
                id,
                &format!("kata-set-param numSearchThreads {threads}"),
                Purpose::Setup,
            );
            self.fence_last(id);
        }
    }
    pub fn apply_gtp_configuration(&mut self, profile: &EngineProfile) {
        let Some(id) = self.instance_of(&profile.id) else {
            return;
        };
        if let Some(configuration) = &profile.gtp_configuration {
            if configuration.protocol == "zengtp-config" {
                if let Ok(json) = serde_json::to_string(&configuration.profile) {
                    self.send(id, &format!("zengtp_config_set {json}"), Purpose::Setup);
                    self.fence_last(id);
                }
            }
        }
        if let Some(inst) = self.get_mut(id) {
            inst.profile.gtp_configuration = profile.gtp_configuration.clone();
        }
    }

    /// `analyze-update-interval-centisec`; 0 means the default 10.
    pub fn set_interval(&mut self, centisec: u32) {
        self.interval_centisec = if centisec == 0 { 10 } else { centisec };
    }

    /// `show-pv-visits` (`Leelaz` adds ` pvVisits true` to `kata-analyze`); a KataGo stream asked
    /// with the other choice is requested again.
    pub fn set_pv_visits(&mut self, on: bool) {
        if self.options.pv_visits != on {
            self.options.pv_visits = on;
            self.pump();
        }
    }

    /// Kata形势 (`ownership true`) and 按选点显示评估 (`movesOwnership true`, KataGo after 1.10)
    /// (`Leelaz.java:20068-20074`); a KataGo stream asked otherwise is requested again
    /// (`Leelaz.ponder`).
    pub fn set_ownership(&mut self, ownership: bool, moves_ownership: bool) {
        let moves_ownership = ownership && moves_ownership;
        if (self.options.ownership, self.options.moves_ownership) != (ownership, moves_ownership) {
            self.options.ownership = ownership;
            self.options.moves_ownership = moves_ownership;
            self.pump();
        }
    }

    /// 强制计算区域 and 选点重点研究 of the foreground engine, taking effect with the next
    /// [`want`](Self::want): a request whose arguments differ is sent again. When only the focus
    /// changes, the same analysis request is re-sent with other `focus` points, which KataGo
    /// continues with the same search tree (no restart, no `clear_board`).
    pub fn set_search(&mut self, region: Option<SearchRegion>, focus: Option<SearchFocus>) {
        self.region = region;
        self.focus = focus;
    }

    /// The region and focus arguments for a request of instance `id` at `target`: only the
    /// foreground engine has them, a region only for engines whose analysis command takes
    /// `allow` / `avoid`, the focus only for its own instance, position and a supporting engine.
    fn search_args(&self, id: InstanceId, target: &Target, inst: &Instance) -> StreamArgs {
        let mut args = StreamArgs::default();
        if self.foreground != Some(id) {
            return args;
        }
        let size = target.record.size;
        let names = |points: &[Point]| {
            let mut names: Vec<String> = points
                .iter()
                .filter(|p| p.x < size.width && p.y < size.height)
                .map(|&p| gtp::vertex(size, p))
                .collect();
            names.dedup();
            names
        };
        if let Some(region) = &self.region {
            let list = names(&region.points);
            if !list.is_empty() && !matches!(inst.dialect(), None | Some(Dialect::Leela011)) {
                let kind = if region.allow { "allow" } else { "avoid" };
                let (list, until) = (list.join(","), region.until_move.max(1));
                args.region = format!(" {kind} b {list} {until} {kind} w {list} {until}");
            }
        }
        if let Some(focus) = self.focus.as_ref().filter(|f| {
            f.instance == id
                && inst.focus == FocusCapability::Supported
                && inst.dialect() == Some(Dialect::Kata)
                && f.doc == target.doc
                && f.node == target.node
                && target.record.node(target.node).stamp == f.stamp
        }) {
            let mut list = names(&focus.points);
            list.sort();
            list.dedup();
            if !list.is_empty() {
                args.focus = format!(" focus {} 0.5", list.join(","));
                args.focus_points = focus.points.clone();
            }
        }
        args
    }

    /// The stream options for `inst`: what it can report of what is wanted.
    fn options_for(&self, inst: &Instance) -> StreamOptions {
        StreamOptions { moves_ownership: self.options.moves_ownership && inst.supports_moves_ownership(), ..self.options }
    }

    /// Full trace (诊断与日志 > 完整日志, engine GTP scope): every line to and from engines.
    pub fn set_trace(&mut self, sink: Option<Box<dyn Write + Send>>) {
        self.trace = sink;
    }

    pub fn tracing(&self) -> bool {
        self.trace.is_some()
    }

    fn trace(&mut self, engine: &str, dir: &str, text: &str) {
        if let Some(t) = self.trace.as_mut() {
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis());
            let _ = writeln!(t, "{ms} [{engine}] {dir} {}", crate::sanitize::sanitize(text));
            let _ = t.flush();
        }
    }

    fn console_push(&mut self, engine: String, kind: ConsoleKind, text: String) {
        self.console_seq += 1;
        self.console.push_back(ConsoleLine {
            seq: self.console_seq,
            engine,
            kind,
            text,
        });
        while self.console.len() > CONSOLE_LIMIT {
            self.console.pop_front();
        }
    }

    // -------------------------------------------------------------------- lifecycle

    /// Starts an engine for `profile` and sends the identification commands. A launch failure
    /// is returned and nothing is kept.
    pub fn start(&mut self, profile: &EngineProfile, boot: Bootstrap) -> Result<InstanceId, Box<EngineFailure>> {
        let id = self.next_instance;
        self.next_instance += 1;
        let fail = |message: String| EngineFailure {
            instance: id,
            profile_id: profile.id.clone(),
            engine_name: profile.name.clone(),
            command: profile.command.clone(),
            during_startup: true,
            message,
            output: Vec::new(),
        };
        let mut spec =
            gtp::prepare_launch(&profile.command, &self.roots).ok_or_else(|| fail("empty command".into()))?;
        // `EngineThreadPolicy.invalidRecommendation`: a Benchmark source without a recommendation.
        if profile
            .thread_policy
            .as_ref()
            .is_some_and(|p| p.source == super::tuning::ThreadSource::BENCHMARK && p.threads == 0)
            && super::tuning::is_local_katago(&profile.command)
            && !super::tuning::has_search_threads(&profile.command)
        {
            return Err(Box::new(fail("EngineThreadPolicy.invalidRecommendation".into())));
        }
        spec.library_dirs = self.library_dirs.clone();
        super::tuning::apply_launch_policy(&profile.command, profile.thread_policy.as_ref(), &mut spec);
        // Overlays that need artifact fingerprints wait for an off-workbench check; commands
        // queue until the process exists.
        let check = profile.thread_policy.as_ref().and_then(|policy| {
            let apple = self.apple_hardware.clone().filter(|_| policy.tuning_profiles.is_some());
            let measured = self
                .measured_hardware
                .clone()
                .filter(|_| policy.measured_scenes.as_ref().is_some_and(|s| s.get("live").is_some()));
            (apple.is_some() || measured.is_some()).then(|| LaunchCheck {
                instance: id,
                command: profile.command.clone(),
                policy: policy.clone(),
                roots: self.roots.clone(),
                spec: spec.clone(),
                apple,
                measured,
                whole_game: None,
            })
        });
        let (io, queued) = match check {
            Some(check) => {
                self.checks.push(check);
                (None, Some(Vec::new()))
            }
            None => (
                Some(self.launcher.launch(id, &spec).map_err(|e| fail(e.to_string()))?),
                None,
            ),
        };
        self.instances.push(Instance {
            id,
            profile: profile.clone(),
            launch_threads: launch_threads(&profile.command, &spec),
            io,
            queued,
            phase: Phase::Starting,
            gtp_name: String::new(),
            version: String::new(),
            kind: EngineKind::Other,
            no_analyze: false,
            commands: Vec::new(),
            next_id: 1,
            waiting: HashMap::new(),
            framer: Framer::default(),
            boot: Some(boot),
            ready_fence: None,
            fence: None,
            board: None,
            stream: None,
            requests: VecDeque::new(),
            summaries: VecDeque::new(),
            ponder_due: None,
            rejected: None,
            syncing: None,
            synced: false,
            output_tail: VecDeque::new(),
            confirmed_rules: None,
            rules_request: None,
            raw_mode: RawNetMode::Off,
            raw_revision: 0,
            raw_requested: None,
            raw_request: None,
            raw_data: None,
            stream_options: StreamOptions::default(),
            unrestored: None,
            focus: FocusCapability::Unknown,
            focus_probe: None,
            stream_args: String::new(),
            stream_region: String::new(),
            stream_focus: Vec::new(),
            installed_focus: Vec::new(),
            installed_from: None,
        });
        let name = profile.name.clone();
        self.trace(&name, "--", &format!("start {}", profile.command));
        self.send(id, "name", Purpose::Name);
        self.send(id, "version", Purpose::Version);
        self.send(id, "list_commands", Purpose::ListCommands);
        Ok(id)
    }

    /// Hands out the pending launch checks once each; the caller runs them off the workbench.
    pub fn take_launch_checks(&mut self) -> Vec<LaunchCheck> {
        std::mem::take(&mut self.checks)
    }

    /// Starts the instance a check was made for and writes the commands queued meanwhile. A
    /// retired instance's check is ignored; a launch failure is reported like an exit during
    /// start-up.
    pub fn finish_launch(&mut self, checked: CheckedLaunch) -> Vec<RuntimeEvent> {
        let id = checked.instance;
        if let Some(index) = self.workers.iter().position(|w| w.id == id && w.queued.is_some()) {
            let launched = self.launcher.launch(id, &checked.spec);
            match launched {
                Ok(mut io) => {
                    for line in self.workers[index].queued.take().unwrap_or_default() {
                        if let Err(e) = io.send(&line) {
                            io.terminate(); self.terminate(id);
                            return vec![RuntimeEvent::WorkerExited { instance: id, message: e.to_string(), output: vec![] }];
                        }
                    }
                    self.workers[index].io = Some(io);
                    return vec![RuntimeEvent::WorkerReady { instance: id }];
                }
                Err(e) => {
                    self.terminate(id);
                    return vec![RuntimeEvent::WorkerLaunchFailed { instance: id, message: e.to_string() }];
                }
            }
        }
        let Some(name) = self.get(id).filter(|i| i.queued.is_some()).map(Instance::display_name) else {
            return Vec::new();
        };
        let launched = self.launcher.launch(id, &checked.spec);
        let Some(inst) = self.get_mut(id) else {
            return Vec::new();
        };
        match launched {
            Err(e) => {
                let failure = inst.failure(e.to_string());
                self.trace(&name, "--", &format!("launch failed: {e}"));
                self.terminate(id);
                vec![RuntimeEvent::Failed(failure)]
            }
            Ok(mut io) => {
                inst.launch_threads = launch_threads(&inst.profile.command, &checked.spec);
                for line in inst.queued.take().unwrap_or_default() {
                    if io.send(&line).is_err() {
                        break;
                    }
                }
                inst.io = Some(io);
                Vec::new()
            }
        }
    }

    /// Makes `id` (or nothing) the foreground engine. The previous foreground engine stops
    /// analysing; it keeps running when `keep_previous` (fast engine change) and is ended
    /// otherwise. An instance taken from the secondary seat leaves it.
    pub fn set_foreground(&mut self, id: Option<InstanceId>, keep_previous: bool) {
        let previous = self.foreground;
        if previous == id {
            return;
        }
        if id.is_some() && id == self.secondary {
            self.secondary = None;
            self.secondary_wish = None;
        }
        self.foreground = id;
        self.seat(previous, id, keep_previous);
    }

    /// Makes `id` (or nothing) the 双引擎模式 secondary engine; the foreground engine cannot be
    /// it. The previous secondary engine stops analysing; it keeps running when `keep_previous`
    /// and is ended otherwise. Its wish is set separately ([`want_secondary`](Self::want_secondary)).
    pub fn set_secondary(&mut self, id: Option<InstanceId>, keep_previous: bool) {
        let previous = self.secondary;
        if previous == id || (id.is_some() && id == self.foreground) {
            return;
        }
        self.secondary = id;
        self.seat(previous, id, keep_previous);
    }

    fn seat(&mut self, previous: Option<InstanceId>, id: Option<InstanceId>, keep_previous: bool) {
        if let Some(p) = previous {
            if keep_previous {
                self.stop_stream(p);
            } else {
                self.terminate(p);
            }
        }
        if let Some(i) = id.and_then(|id| self.get_mut(id)) {
            i.synced = false;
            i.raw_mode = RawNetMode::Off;
            i.raw_revision += 1;
            i.raw_data = None;
            i.rejected = None;
        }
        self.pump();
    }

    /// Ends one instance (an engine or an analysis process). Its later output is ignored; a seat
    /// it held is empty, and a lease of it ends as [`LeaseFailure::TransportClosed`] (reported by
    /// [`take_events`](Self::take_events)).
    pub fn terminate(&mut self, id: InstanceId) {
        self.checks.retain(|c| c.instance != id);
        if let Some(at) = self.workers.iter().position(|w| w.id == id) {
            let mut w = self.workers.remove(at);
            self.trace(&w.name, "--", "terminate");
            if let Some(io) = w.io.take() {
                io.terminate();
            }
            return;
        }
        let Some(at) = self.instances.iter().position(|i| i.id == id) else {
            return;
        };
        let mut inst = self.instances.remove(at);
        if self.foreground == Some(id) {
            self.foreground = None;
        }
        if self.secondary == Some(id) {
            self.secondary = None;
            self.secondary_wish = None;
        }
        if let Some(lease) = self.lease.take_if(|l| l.instance == id) {
            self.pending.push(RuntimeEvent::LeaseEnded {
                lease: lease.id,
                failure: Some(LeaseFailure::TransportClosed),
            });
        }
        self.trace(&inst.display_name(), "--", "terminate");
        if let Some(io) = inst.io.take() {
            io.terminate();
        }
    }

    pub fn terminate_all(&mut self) {
        let ids: Vec<InstanceId> = self
            .instances
            .iter()
            .map(|i| i.id)
            .chain(self.workers.iter().map(|w| w.id))
            .collect();
        for id in ids {
            self.terminate(id);
        }
    }

    /// Events produced outside engine output (a lease lost with its terminated engine).
    pub fn take_events(&mut self) -> Vec<RuntimeEvent> {
        std::mem::take(&mut self.pending)
    }

    // -------------------------------------------------------------------- wishes

    /// What the foreground engine should do: hold `target` (synced even while not analysing, so
    /// a switch completes) and analyse it when `analyse`. `None` stops and leaves the board.
    pub fn want(&mut self, target: Option<Target>, analyse: bool) {
        let wish = target.map(|t| (t, analyse));
        Self::rewish(
            self.foreground
                .and_then(|id| self.instances.iter_mut().find(|i| i.id == id)),
            &self.wish,
            &wish,
        );
        self.wish = wish;
        self.pump();
    }

    /// The same for the secondary engine. `None` stops its analysis and keeps it running
    /// (`nameCmdfornoponder` when leaving 双引擎模式).
    pub fn want_secondary(&mut self, target: Option<Target>, analyse: bool) {
        let wish = target.map(|t| (t, analyse));
        Self::rewish(
            self.secondary
                .and_then(|id| self.instances.iter_mut().find(|i| i.id == id)),
            &self.secondary_wish,
            &wish,
        );
        self.secondary_wish = wish;
        // Only the secondary seat's wish changed: the foreground engine waits for its own `want`.
        if let Some(id) = self.secondary {
            self.pump_seat(id);
        }
    }

    /// A seat's new wish: another position drops its pure-network answer; a pondering cycle
    /// continues only for the position it is analysing.
    fn rewish(inst: Option<&mut Instance>, old: &Option<(Target, bool)>, new: &Option<(Target, bool)>) {
        let Some(inst) = inst else { return };
        if old.as_ref().map(|(t, _)| t.key()) != new.as_ref().map(|(t, _)| t.key()) {
            inst.raw_revision += 1;
            inst.raw_data = None;
        }
        let keep_pondering = new.as_ref().is_some_and(|(target, analyse)| {
            *analyse
                && inst
                    .stream
                    .and_then(|n| inst.request(n))
                    .is_some_and(|r| r.key == target.key())
        });
        if !keep_pondering {
            inst.ponder_due = None;
        }
    }

    /// Pure network (H) belongs to the foreground instance, not the document or profile. With
    /// `mirror` (双引擎模式, Java `toggleHeatmapSub`) the secondary engine follows: the same state,
    /// its policy instead of an ownership it cannot report.
    pub fn toggle_raw_net(&mut self, mirror: bool) -> bool {
        let Some(inst) = self.foreground.and_then(|id| self.instances.iter_mut().find(|i| i.id == id)) else { return false };
        inst.raw_mode = match inst.raw_mode {
            RawNetMode::Off => RawNetMode::Policy,
            RawNetMode::Policy if inst.kind == EngineKind::KataGo => RawNetMode::Ownership,
            _ => RawNetMode::Off,
        };
        inst.raw_revision += 1;
        inst.raw_data = None;
        let mode = inst.raw_mode;
        if mirror {
            if let Some(sec) = self
                .secondary
                .and_then(|id| self.instances.iter_mut().find(|i| i.id == id))
            {
                sec.raw_mode = match mode {
                    RawNetMode::Ownership if sec.kind != EngineKind::KataGo => RawNetMode::Policy,
                    m => m,
                };
                sec.raw_revision += 1;
                sec.raw_data = None;
            }
        }
        true
    }

    pub fn stop_raw_net(&mut self) -> bool {
        let Some(inst) = self.foreground.and_then(|id| self.instances.iter_mut().find(|i| i.id == id)) else { return false };
        if inst.raw_mode == RawNetMode::Off { return false; }
        inst.raw_mode = RawNetMode::Off;
        inst.raw_revision += 1;
        inst.raw_data = None;
        if let Some(sec) = self
            .secondary
            .and_then(|id| self.instances.iter_mut().find(|i| i.id == id))
        {
            sec.raw_mode = RawNetMode::Off;
            sec.raw_revision += 1;
            sec.raw_data = None;
        }
        true
    }

    pub fn raw_net_active(&self) -> bool {
        self.foreground.and_then(|id| self.get(id)).is_some_and(|i| i.raw_mode != RawNetMode::Off)
    }

    /// The foreground engine's pure-network answer for `target`.
    pub fn raw_net_view(&self, target: &Target) -> Option<RawNetView> {
        self.raw_net_view_of(self.foreground?, target)
    }

    /// Instance `id`'s pure-network answer for `target` (pending until it answers that position).
    pub fn raw_net_view_of(&self, id: InstanceId, target: &Target) -> Option<RawNetView> {
        let inst = self.get(id)?;
        if inst.raw_mode == RawNetMode::Off { return None; }
        let key = target.key();
        Some(inst.raw_data.as_ref().filter(|(k, _)| *k == key).map_or_else(
            || RawNetView::pending(inst.raw_mode, target.node), |(_, view)| view.clone()))
    }

    /// A seated instance's next Leela 0.11 pondering probe.
    fn due(&self, id: Option<InstanceId>) -> Option<Instant> {
        let inst = self.get(id?)?;
        if inst.fence.is_none() && inst.stream.is_some() {
            inst.ponder_due
        } else {
            None
        }
    }

    /// When [`tick`](Self::tick) has work next (a Leela 0.11 pondering probe, a lease deadline),
    /// if ever.
    pub fn next_tick(&self) -> Option<Instant> {
        [
            self.due(self.foreground),
            self.due(self.secondary),
            self.lease.as_ref().and_then(|l| l.deadline),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    /// Advances timed engine work without owning a thread: the platform calls this at
    /// [`next_tick`](Self::next_tick); controlled-protocol tests supply a later instant directly.
    pub fn tick(&mut self, now: Instant) -> Vec<RuntimeEvent> {
        for id in [self.foreground, self.secondary].into_iter().flatten() {
            if self.due(Some(id)).is_some_and(|due| now >= due) {
                self.get_mut(id).unwrap().ponder_due = None;
                self.send(id, "name", Purpose::Probe);
                self.fence_last(id);
            }
        }
        let mut events = Vec::new();
        if self.lease.as_ref().and_then(|l| l.deadline).is_some_and(|d| now >= d) {
            self.lease_timeout(&mut events);
        }
        events.append(&mut self.pending);
        events
    }

    fn stop_stream(&mut self, id: InstanceId) -> bool {
        let Some(inst) = self.get(id) else { return false };
        if inst.stream.is_none() {
            return false;
        }
        // `nameCmdfornoponder`: KataGo stops with `stop`, others with `name`.
        let stop = if inst.kind == EngineKind::KataGo {
            "stop"
        } else {
            "name"
        };
        self.send(id, stop, Purpose::Stop);
        true
    }

    /// Sends the next batch for each seat's wish when nothing is in flight for it.
    fn pump(&mut self) {
        for id in [self.foreground, self.secondary].into_iter().flatten() {
            self.pump_seat(id);
        }
    }

    /// Sends the next batch of a seated instance (the foreground or the secondary engine) for
    /// its wish when nothing is in flight.
    fn pump_seat(&mut self, id: InstanceId) {
        let interval = self.interval_centisec;
        let Some(inst) = self.get(id) else { return };
        if inst.phase != Phase::Ready || inst.fence.is_some() || inst.unrestored.is_some() || self.leased(id) {
            return;
        }
        let first_new = inst.next_id;
        match self.wish_of(id).cloned() {
            None => {
                self.stop_stream(id);
            }
            Some((target, analyse)) => {
                let key = target.key();
                if let Some(r) = &target.rules {
                    self.request_rules(id, r);
                }
                let inst = self.get(id).unwrap();
                if inst.rejected == Some(key) {
                    self.stop_stream(id);
                } else {
                    let (commands, board) = sync_commands(inst, &target);
                    let options = self.options_for(inst);
                    let args = self.search_args(id, &target, inst);
                    // The stream searching this position with the same options and region: a
                    // request changing only its focus continues that search, and its time.
                    let same_search = inst.stream.and_then(|n| inst.request(n)).filter(|r| {
                        r.key == key
                            && (inst.dialect() != Some(Dialect::Kata) || inst.stream_options == options)
                            && inst.stream_region == args.region
                    });
                    let streaming_here = same_search.is_some() && inst.stream_args == args.text();
                    let started = same_search.filter(|_| commands.is_empty()).map(|r| r.started);
                    let dialect = inst.dialect();
                    let root_info = inst.supports_root_info();
                    if commands.is_empty() {
                        self.get_mut(id).unwrap().synced = true;
                    } else {
                        let i = self.get_mut(id).unwrap();
                        i.board = Some(board);
                        i.syncing = Some(key);
                        for c in commands {
                            self.send(id, &c, Purpose::Sync);
                        }
                    }
                    let streaming_here = streaming_here && self.get(id).unwrap().stream.is_some();
                    let raw_mode = self.get(id).unwrap().raw_mode;
                    if raw_mode != RawNetMode::Off {
                        self.stop_stream(id);
                        let inst = self.get(id).unwrap();
                        let revision = inst.raw_revision;
                        if analyse && inst.raw_requested != Some((key, revision)) {
                            let request = Request {
                                key, to_play: target.record.node(target.node).to_play,
                                size: target.record.size, started: Instant::now(),
                            };
                            let command = if inst.kind == EngineKind::KataGo {
                                let symmetry =
                                    std::collections::hash_map::RandomState::new().hash_one((id, first_new)) % 8;
                                format!("kata-raw-nn {symmetry}")
                            } else { "heatmap".to_string() };
                            if let Some(n) = self.send(id, &command, Purpose::RawNet) {
                                let inst = self.get_mut(id).unwrap();
                                inst.raw_requested = Some((key, revision));
                                inst.raw_request = Some(RawRequest {
                                    id: n, request, revision, mode: raw_mode,
                                    text: String::new(), response: None, complete: false,
                                });
                            }
                        }
                        let inst = self.get_mut(id).unwrap();
                        if inst.next_id > first_new { inst.fence = Some(inst.next_id - 1); }
                        return;
                    }
                    match (analyse, dialect) {
                        (true, Some(d)) if !streaming_here => {
                            let to_play = target.record.node(target.node).to_play;
                            let color = if to_play == Stone::Black { "B" } else { "W" };
                            let cmd = match d {
                                Dialect::Kata => format!(
                                    "kata-analyze {color} {interval}{}{}{}{}{}{}{}",
                                    args.region,
                                    if root_info { " rootInfo true" } else { "" },
                                    if options.ownership { " ownership true" } else { "" },
                                    if options.pv_visits { " pvVisits true" } else { "" },
                                    if options.moves_ownership { " movesOwnership true" } else { "" },
                                    if !args.focus.is_empty() && !root_info { " rootInfo true" } else { "" },
                                    args.focus
                                ),
                                Dialect::Leela | Dialect::Sai => format!("lz-analyze {color} {interval}{}", args.region),
                                Dialect::Sayuri => format!("analyze {color} {interval}{}", args.region),
                                Dialect::Leela011 => "time_left b 0 0".to_string(),
                            };
                            let request = Request {
                                key,
                                to_play,
                                size: target.record.size,
                                started: started.unwrap_or_else(Instant::now),
                            };
                            let purpose = if d == Dialect::Leela011 {
                                Purpose::Ponder
                            } else {
                                Purpose::Analyze
                            };
                            if let Some(n) = self.send(id, &cmd, purpose) {
                                let i = self.get_mut(id).unwrap();
                                // A changed region is another search of the same position: the old
                                // stream's late lines, searched without it, are not published.
                                if let Some(old) = i.stream.filter(|_| i.stream_region != args.region) {
                                    i.requests.retain(|(r, q)| *r != old || q.key != key);
                                }
                                i.stream = Some(n);
                                i.stream_options = options;
                                i.stream_args = args.text();
                                i.stream_region = args.region;
                                i.stream_focus = args.focus_points;
                                i.requests.push_back((n, request));
                                if d == Dialect::Leela011 {
                                    i.summaries.push_back((n, request, InfoLine::default()));
                                    i.ponder_due = Some(Instant::now() + PONDER_INTERVAL);
                                }
                                while i.requests.len() > REQUEST_MEMORY {
                                    i.requests.pop_front();
                                }
                            }
                        }
                        (true, Some(Dialect::Leela011)) => {
                            let i = self.get(id).unwrap();
                            let n = i.stream.unwrap();
                            // A completed table starts the next cycle for the same request;
                            // its elapsed time and position ownership do not restart.
                            if i.ponder_due.is_none() && !i.summaries.iter().any(|(owner, _, _)| *owner == n) {
                                let request = i.request(n).unwrap();
                                if self.send(id, "time_left b 0 0", Purpose::Ponder).is_some() {
                                    let i = self.get_mut(id).unwrap();
                                    i.summaries.push_back((n, request, InfoLine::default()));
                                    i.ponder_due = Some(Instant::now() + PONDER_INTERVAL);
                                }
                            }
                        }
                        (false, _) | (true, None) => {
                            self.stop_stream(id);
                        }
                        _ => {}
                    }
                }
            }
        }
        let i = self.get_mut(id).unwrap();
        if i.next_id > first_new {
            i.fence = Some(i.next_id - 1);
        }
    }

    /// Writes a numbered command. A non-analysis command ends the current stream, except
    /// Leela 0.11 cycle commands, whose table retains the original request owner.
    fn send(&mut self, id: InstanceId, command: &str, purpose: Purpose) -> Option<u64> {
        let inst = self.get_mut(id)?;
        let n = inst.next_id;
        inst.next_id += 1;
        if !matches!(purpose, Purpose::Analyze | Purpose::Ponder | Purpose::Probe) {
            inst.stream = None;
            inst.ponder_due = None;
        }
        let line = format!("{n} {command}");
        let ok = match (inst.queued.as_mut(), inst.io.as_mut()) {
            (Some(queue), _) => {
                queue.push(line.clone());
                true
            }
            (None, Some(io)) => io.send(&line).is_ok(),
            (None, None) => false,
        };
        inst.waiting.insert(n, purpose);
        let name = inst.display_name();
        self.trace(&name, ">>", &line);
        self.console_push(name, ConsoleKind::Sent, command.to_string());
        ok.then_some(n)
    }

    /// Marks a batch the caller sent outside `pump` as the one in flight.
    fn fence_last(&mut self, id: InstanceId) {
        if let Some(i) = self.get_mut(id) {
            i.fence = i.next_id.checked_sub(1);
        }
    }

    // -------------------------------------------------------------------- console and rules

    /// GtpConsolePane send: a raw command to the foreground engine, and with `mirror` (双引擎模式,
    /// Java `sendRawConsoleCommand` mirrors to `leelaz2`) to the secondary engine too.
    /// Board-changing commands are refused because the record owns the position. After its
    /// answer, analysis resumes.
    pub fn send_user(&mut self, command: &str, mirror: bool) -> Result<(), UserCommandError> {
        let command = command.trim();
        if command.is_empty() {
            return Err(UserCommandError::Empty);
        }
        let Some(fg) = self.foreground.filter(|&f| self.get(f).is_some()) else {
            return Err(UserCommandError::NoEngine);
        };
        let word = command.split_whitespace().next().unwrap_or("");
        if BOARD_COMMANDS.contains(&word) {
            return Err(UserCommandError::ChangesBoard);
        }
        let secondary = self.secondary.filter(|_| mirror);
        for id in std::iter::once(fg).chain(secondary) {
            self.send(id, command, Purpose::User);
            self.fence_last(id);
        }
        Ok(())
    }

    /// Sends `kata-set-rules` and the confirming `kata-get-rules` once per requested rules value;
    /// an engine without the commands fails at once (`OPERATION_REJECTED`).
    fn request_rules(&mut self, id: InstanceId, rules: &KataRules) {
        let Some(inst) = self.get_mut(id) else { return };
        if inst.confirmed_rules.as_ref() == Some(rules) {
            inst.rules_request = Some((rules.clone(), RulesOutcome::Confirmed));
            return;
        }
        if inst.rules_request.as_ref().is_some_and(|(r, _)| r == rules) {
            return;
        }
        if inst.kind != EngineKind::KataGo || !inst.has("kata-set-rules") || !inst.has("kata-get-rules") {
            inst.rules_request = Some((rules.clone(), RulesOutcome::Failed("OPERATION_REJECTED")));
            return;
        }
        inst.rules_request = Some((rules.clone(), RulesOutcome::Pending));
        self.send(
            id,
            &format!("kata-set-rules {}", rules.to_gtp_argument()),
            Purpose::SetRules,
        );
        self.send(id, "kata-get-rules", Purpose::GetRules);
    }

    /// How making engine `id` use `rules` went.
    pub fn rules_outcome(&self, id: InstanceId, rules: &KataRules) -> RulesOutcome {
        let Some(inst) = self.get(id) else {
            return RulesOutcome::Failed("ENGINE_UNAVAILABLE");
        };
        if inst.confirmed_rules.as_ref() == Some(rules) {
            return RulesOutcome::Confirmed;
        }
        match &inst.rules_request {
            Some((r, outcome)) if r == rules => *outcome,
            _ => RulesOutcome::Pending,
        }
    }

    /// The rules engine `id` last reported.
    pub fn confirmed_rules(&self, id: InstanceId) -> Option<&KataRules> {
        self.get(id)?.confirmed_rules.as_ref()
    }

    /// `Leelaz.canSuicidal`: the engine reported rules that allow suicide.
    pub fn allows_suicide(&self, id: InstanceId) -> bool {
        self.confirmed_rules(id)
            .and_then(|r| r.field("suicide"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    }

    /// `Leelaz.canAddPlayer`: the engine analyses for either side to play, so 交换落子顺序 can
    /// change the side to play instead of passing. Java sets it from the GTP name (KataGo, Leela
    /// Zero / Pachi, `llzero`, Sayuri) and clears it for a non-KataGo version `x.15`.
    pub fn can_add_player(&self, id: InstanceId) -> bool {
        let Some(i) = self.get(id).filter(|i| i.phase == Phase::Ready) else {
            return false;
        };
        let named = matches!(i.kind, EngineKind::KataGo | EngineKind::LeelaZero | EngineKind::Sayuri)
            || i.gtp_name.to_lowercase().starts_with("llzero");
        let minor_15 = i.kind != EngineKind::KataGo
            && i.version.split('.').nth(1).and_then(|m| m.trim().parse::<u32>().ok()) == Some(15);
        named && !minor_15
    }

    // -------------------------------------------------------------------- input

    /// Output of instance `id`. Returns what the application flow must act on.
    pub fn input(&mut self, id: InstanceId, input: EngineInput) -> Vec<RuntimeEvent> {
        let mut events = std::mem::take(&mut self.pending);
        if self.workers.iter().any(|w| w.id == id) {
            self.worker_input(id, input, &mut events);
            return events;
        }
        let Some(inst) = self.get_mut(id) else {
            return events; // a retired instance
        };
        let name = inst.display_name();
        match input {
            EngineInput::Stderr(line) => {
                push_tail(&mut inst.output_tail, &line);
                if inst.kind != EngineKind::KataGo {
                    if let Some(raw) = inst.raw_request.as_mut() {
                        if !raw.complete {
                            let row = line.split_whitespace().count() == usize::from(raw.request.size.width)
                                && line.split_whitespace().all(|s| s.parse::<u32>().is_ok());
                            if row || line.contains("winrate:") {
                                raw.text.push_str(&line);
                                raw.text.push('\n');
                            }
                            if line.contains("winrate:") { raw.complete = true; }
                        }
                    }
                }
                self.finish_raw(id, &mut events);
                self.summary(id, &line, &mut events);
                self.trace(&name, "!!", &line);
                self.console_push(name, ConsoleKind::Error, line);
            }
            EngineInput::Exited(code) => {
                let message = match code {
                    Some(c) => format!("exit code {c}"),
                    None => "terminated".to_string(),
                };
                let failure = inst.failure(message.clone());
                self.trace(&name, "--", &format!("exited: {message}"));
                self.terminate(id);
                events.push(RuntimeEvent::Failed(failure));
            }
            EngineInput::Stdout(line) => {
                let mut frames = Vec::new();
                inst.framer.feed(&line, &mut frames);
                let is_info = line.starts_with("info ");
                if !is_info {
                    push_tail(&mut inst.output_tail, &line);
                }
                self.trace(&name, "<<", &line);
                if !is_info && !line.trim().is_empty() {
                    self.console_push(name, ConsoleKind::Output, line);
                }
                for f in frames {
                    self.frame(id, f, &mut events);
                }
            }
        }
        // Lease steps that could not be written report through `pending`.
        events.append(&mut self.pending);
        events
    }

    fn finish_raw(&mut self, id: InstanceId, events: &mut Vec<RuntimeEvent>) {
        let current = self.wish_of(id).map(|(t, _)| t.key());
        let seated = self.seated(id);
        let Some(inst) = self.get_mut(id) else { return };
        if !inst.raw_request.as_ref().is_some_and(|r| r.response.is_some() && r.complete) { return; }
        let raw = inst.raw_request.take().unwrap();
        let request = raw.request;
        if raw.response == Some(true)
            && seated
            && current == Some(request.key)
            && raw.revision == inst.raw_revision
            && raw.mode == inst.raw_mode
            && inst.rejected != Some(request.key)
        {
            let parsed = if inst.kind == EngineKind::KataGo {
                raw_net::parse_kata(&raw.text, request.size, raw.mode, request.key.node)
            } else {
                raw_net::parse_leela(&raw.text, request.size, request.key.node, request.to_play)
            };
            if let Some(view) = parsed {
                inst.raw_data = Some((request.key, view));
                events.push(RuntimeEvent::RawNet { instance: id });
            }
        }
        self.answered(id, raw.id);
    }

    /// Leela 0.11's stderr has no GTP ids. Each `time_left` cycle owns exactly one table,
    /// including a late table produced by the command that stopped an old position.
    fn summary(&mut self, id: InstanceId, line: &str, events: &mut Vec<RuntimeEvent>) {
        let Some(inst) = self.get_mut(id) else { return };
        let Some((_, request, table)) = inst.summaries.front_mut() else {
            return;
        };
        if !line.starts_with("=====") {
            if let Some(mut candidate) = gtp::parse_leela011_summary(line, request.size) {
                candidate.order = table.candidates.len() as u32;
                table.candidates.push(candidate);
            }
            return;
        }
        let (owner, request, table) = inst.summaries.pop_front().unwrap();
        if inst.stream == Some(owner) {
            inst.ponder_due = None;
        }
        // Keep rejected cycles in the stderr queue until their delimiter, so their late
        // rows cannot become the next request's table, but never publish their payload.
        if inst.request(owner).is_none() {
            self.pump();
            return;
        }
        events.push(RuntimeEvent::Analysis {
            instance: id,
            doc: request.key.doc,
            node: request.key.node,
            stamp: request.key.stamp,
            komi: f64::from_bits(request.key.komi_bits),
            rules_rev: request.key.rules_rev,
            to_play: request.to_play,
            engine_name: inst.display_name(),
            line: table,
            elapsed: request.started.elapsed(),
        });
        self.pump();
    }

    fn frame(&mut self, id: InstanceId, frame: Frame, events: &mut Vec<RuntimeEvent>) {
        if self.lease.as_ref().is_some_and(|l| l.instance == id) && self.lease_frame(&frame, events) {
            return;
        }
        let Some(inst) = self.get_mut(id) else { return };
        match frame {
            Frame::Header { id: n } => {
                // The engine has consumed everything up to an analysis request: that request's
                // batch is answered even though its stream keeps going.
                if matches!(inst.waiting.get(&n), Some(Purpose::Analyze)) {
                    inst.waiting.remove(&n);
                    // The stream still wished for accepted its focus points.
                    if inst.stream == Some(n) {
                        inst.installed_from = Some(n);
                        if inst.installed_focus != inst.stream_focus {
                            inst.installed_focus = inst.stream_focus.clone();
                            events.push(RuntimeEvent::FocusConfirmed { instance: id });
                        }
                    }
                    self.answered(id, n);
                }
            }
            Frame::Info { id: n, line } => {
                let Some(request) = inst.request(n) else { return };
                let Some(dialect) = inst.dialect() else { return };
                events.push(RuntimeEvent::Analysis {
                    instance: id,
                    doc: request.key.doc,
                    node: request.key.node,
                    stamp: request.key.stamp,
                    komi: f64::from_bits(request.key.komi_bits),
                    rules_rev: request.key.rules_rev,
                    to_play: request.to_play,
                    engine_name: inst.display_name(),
                    line: gtp::parse_info(&line, dialect, request.size),
                    elapsed: request.started.elapsed(),
                });
            }
            Frame::Response { id: n, ok, text } => {
                if inst.stream == Some(n) && !matches!(inst.waiting.get(&n), Some(Purpose::Ponder)) {
                    inst.stream = None;
                }
                // The stream that searched the installed focus points ended.
                if inst.installed_from == Some(n) {
                    inst.installed_from = None;
                    if !inst.installed_focus.is_empty() {
                        inst.installed_focus.clear();
                        events.push(RuntimeEvent::FocusConfirmed { instance: id });
                    }
                }
                match inst.waiting.remove(&n) {
                    Some(Purpose::Name) => {
                        if !ok {
                            let failure = inst.failure(format!("name: {text}"));
                            self.terminate(id);
                            events.push(RuntimeEvent::Failed(failure));
                            return;
                        }
                        (inst.kind, inst.no_analyze) = classify(&text);
                        inst.gtp_name = text;
                    }
                    Some(Purpose::Version) => inst.version = text,
                    Some(Purpose::FocusProbe) => {
                        let args = inst.focus_probe.take().map(|(_, a)| a).unwrap_or_default();
                        let payload = text.trim();
                        inst.focus = if ok {
                            FocusCapability::Supported
                        } else if payload.eq_ignore_ascii_case("unknown analyze option focus")
                            || payload == format!("Could not parse analyze arguments or arguments out of range: '{args}'")
                        {
                            FocusCapability::Unsupported
                        } else {
                            FocusCapability::Failed
                        };
                    }
                    Some(Purpose::ListCommands) => {
                        inst.commands = text
                            .lines()
                            .map(|l| l.trim().to_string())
                            .filter(|l| !l.is_empty())
                            .collect();
                        self.boot_setup(id);
                    }
                    Some(Purpose::Sync) if !ok => {
                        // The engine refused part of the position: its board is unknown now, the
                        // analysis requested after it in the same batch is not for the position
                        // it names, and this position is not retried until another one is wished.
                        inst.board = None;
                        inst.rejected = inst.syncing;
                        inst.requests.retain(|(m, _)| *m < n);
                        inst.raw_data = None;
                    }
                    Some(Purpose::SetRules) if !ok => {
                        if let Some((_, outcome)) = inst.rules_request.as_mut() {
                            *outcome = RulesOutcome::Failed("OPERATION_REJECTED");
                        }
                        events.push(RuntimeEvent::Rules { instance: id });
                    }
                    Some(Purpose::GetRules) => {
                        let observed = if ok { KataRules::parse(&text) } else { None };
                        if let Some((requested, outcome @ RulesOutcome::Pending)) = inst.rules_request.as_mut() {
                            *outcome = match &observed {
                                None => RulesOutcome::Failed("UNCONFIRMED"),
                                Some(o) if o == requested => RulesOutcome::Confirmed,
                                Some(_) => RulesOutcome::Failed("TARGET_MISMATCH"),
                            };
                        }
                        if observed.is_some() {
                            inst.confirmed_rules = observed;
                        }
                        events.push(RuntimeEvent::Rules { instance: id });
                    }
                    Some(Purpose::RawNet) => {
                        if let Some(raw) = inst.raw_request.as_mut().filter(|r| r.id == n) {
                            raw.response = Some(ok);
                            if inst.kind == EngineKind::KataGo || !text.is_empty() || !ok {
                                raw.text = text;
                                raw.complete = true;
                            }
                        }
                        self.finish_raw(id, events);
                        return;
                    }
                    _ => {}
                }
                let Some(inst) = self.get_mut(id) else { return };
                if inst.ready_fence == Some(n) {
                    inst.phase = Phase::Ready;
                    inst.ready_fence = None;
                    events.push(RuntimeEvent::Ready { instance: id });
                    if self.seated(id) {
                        self.pump_seat(id);
                    }
                }
                self.answered(id, n);
            }
        }
    }

    fn answered(&mut self, id: InstanceId, n: u64) {
        let Some(inst) = self.get_mut(id) else { return };
        if inst.fence != Some(n) {
            return;
        }
        inst.fence = None;
        if inst.board.is_some() {
            inst.synced = true;
        }
        if self.seated(id) {
            self.pump_seat(id);
        }
    }

    /// After `list_commands`: komi, board size, the profile's initial commands, start-up rules
    /// and the rules query; the answer to the last of them marks the engine ready.
    fn boot_setup(&mut self, id: InstanceId) {
        let Some(inst) = self.get_mut(id) else { return };
        let boot = inst.boot.take().unwrap_or(Bootstrap { komi: 7.5, rules: None });
        let (w, h) = (inst.profile.width.max(1), inst.profile.height.max(1));
        let mut cmds = vec![format!("komi {}", komi_text(boot.komi))];
        cmds.push(if w == h || !inst.has("rectangular_boardsize") {
            format!("boardsize {w}")
        } else {
            format!("rectangular_boardsize {w} {h}")
        });
        cmds.extend(
            inst.profile
                .initial_command
                .split(';')
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .map(str::to_string),
        );
        let katago = inst.kind == EngineKind::KataGo;
        if katago {
            let threads = inst.launch_threads;
            if threads > 0 {
                cmds.push(format!("kata-set-param numSearchThreads {threads}"));
            }
        }
        if let Some(configuration) = &inst.profile.gtp_configuration {
            if configuration.protocol == "zengtp-config" && !configuration.profile.is_empty() {
                if let Ok(json) = serde_json::to_string(&configuration.profile) {
                    cmds.push(format!("zengtp_config_set {json}"));
                }
            }
        }
        if katago && inst.has("kata-set-rules") {
            if let Some(r) = &boot.rules {
                cmds.push(format!("kata-set-rules {}", r.to_gtp_argument()));
            }
        }
        let get_rules = katago && inst.has("kata-get-rules");
        for c in cmds {
            self.send(id, &c, Purpose::Setup);
        }
        if get_rules {
            self.send(id, "kata-get-rules", Purpose::GetRules);
        }
        // 选点重点研究's capability probe (`Leelaz.startMoveFocusProbe…`), part of every start-up
        // so neither a paused start nor an engine switch skips it: an empty-board analysis with
        // a zero-probability focus, stopped at once. Its lines are attributed to no request.
        let probe = katago && self.get(id).is_some_and(|i| !i.no_analyze && i.has("kata-analyze"));
        if probe {
            let args = format!("{} rootInfo true focus pass 0", self.interval_centisec);
            if let Some(n) = self.send(id, &format!("kata-analyze {args}"), Purpose::FocusProbe) {
                self.get_mut(id).unwrap().focus_probe = Some((n, args));
            }
            self.send(id, "stop", Purpose::Setup);
        }
        let inst = self.get_mut(id).unwrap();
        inst.ready_fence = inst.next_id.checked_sub(1);
        inst.board = None;
    }

    // -------------------------------------------------------------------- foreground lease

    fn leased(&self, id: InstanceId) -> bool {
        self.lease.as_ref().is_some_and(|l| l.instance == id)
    }

    /// Whether the foreground engine can be borrowed now (`previewForegroundAnalysisLeaseAvailability`).
    pub fn lease_availability(&self) -> LeaseAvailability {
        let Some(inst) = self.foreground.and_then(|id| self.get(id)) else {
            return LeaseAvailability::NoForegroundEngine;
        };
        if inst.unrestored.is_some() {
            LeaseAvailability::EngineStateUnrestored
        } else if inst.phase != Phase::Ready {
            LeaseAvailability::EngineNotReady
        } else if inst.kind != EngineKind::KataGo {
            LeaseAvailability::NotKatago
        } else if !LEASE_COMMANDS.iter().all(|c| inst.has(c)) {
            LeaseAvailability::MissingCapability
        } else if self.lease.is_some() {
            LeaseAvailability::ExistingLease
        } else {
            LeaseAvailability::Available
        }
    }

    /// How giving instance `id` back failed, while it stays unrestored.
    pub fn unrestored(&self, id: InstanceId) -> Option<LeaseFailure> {
        self.get(id)?.unrestored
    }

    /// Takes the foreground engine over (`acquireForegroundAnalysisLease`): `stop`, then its rules
    /// are captured; it does not follow its wish until given back. Searches may start after
    /// [`RuntimeEvent::LeaseReady`].
    pub fn begin_lease(&mut self) -> Result<u64, LeaseAvailability> {
        match self.lease_availability() {
            LeaseAvailability::Available => {}
            other => return Err(other),
        }
        let id = self.foreground.expect("an available lease has a foreground engine");
        let Some(n) = self.send(id, "stop", Purpose::Lease) else {
            return Err(LeaseAvailability::EngineNotReady);
        };
        let lease = self.next_lease;
        self.next_lease += 1;
        self.lease = Some(Lease {
            id: lease,
            instance: id,
            step: LeaseStep::InitialStop(n),
            deadline: Some(Instant::now() + LEASE_ACK),
            captured_rules: None,
            rules_changed: false,
            releasing: false,
            restore_failed: false,
            search: None,
        });
        Ok(lease)
    }

    /// Whether lease `lease` is still out (neither given back nor lost).
    pub fn lease_alive(&self, lease: u64) -> bool {
        self.lease.as_ref().is_some_and(|l| l.id == lease)
    }

    /// Whether lease `lease` is ready for a search.
    pub fn lease_idle(&self, lease: u64) -> bool {
        self.lease
            .as_ref()
            .is_some_and(|l| l.id == lease && l.step == LeaseStep::Idle && !l.releasing)
    }

    /// A finite search of `target` on the borrowed engine: its position, `rules` (a
    /// `kata-set-rules` argument) when given, then `kata-analyze … 1 maxmoves 1 rootInfo true`
    /// stopped once the root has `visits` visits.
    pub fn lease_search(&mut self, lease: u64, target: &Target, visits: u64, rules: Option<&str>) -> bool {
        // Rules are changed only when the engine's own can be put back.
        if !self.lease_idle(lease)
            || (rules.is_some() && self.lease.as_ref().is_some_and(|l| l.captured_rules.is_none()))
        {
            return false;
        }
        let id = self.lease.as_ref().unwrap().instance;
        let Some(inst) = self.get(id) else { return false };
        let (commands, board) = sync_commands(inst, target);
        self.get_mut(id).unwrap().board = Some(board);
        for c in commands {
            self.send(id, &c, Purpose::Lease);
        }
        if let Some(r) = rules {
            self.send(id, &format!("kata-set-rules {r}"), Purpose::Lease);
        }
        let to_play = target.record.node(target.node).to_play;
        let Some(n) = self.send(
            id,
            &format!("kata-analyze {} 1 maxmoves 1 rootInfo true", color_letter(to_play)),
            Purpose::Lease,
        ) else {
            return false;
        };
        let l = self.lease.as_mut().unwrap();
        l.rules_changed |= rules.is_some();
        l.step = LeaseStep::Search;
        l.deadline = Some(Instant::now() + LEASE_ACK);
        l.search = Some(LeaseSearch {
            analyze: n,
            request: Request {
                key: target.key(),
                to_play,
                size: target.record.size,
                started: Instant::now(),
            },
            target_visits: visits,
            best: None,
            failed: false,
        });
        true
    }

    /// Gives the engine back (`endForegroundAnalysisLease`): a search is stopped, then `stop`,
    /// the captured rules, the seat's position and `name`; [`RuntimeEvent::LeaseEnded`] tells
    /// how it went.
    pub fn end_lease(&mut self, lease: u64) {
        let Some(l) = self.lease.as_mut().filter(|l| l.id == lease && !l.releasing) else {
            return;
        };
        l.releasing = true;
        match l.step {
            LeaseStep::Idle => self.lease_final_stop(),
            LeaseStep::Search => self.lease_stop_search(),
            // The step in flight continues the release when answered.
            _ => {}
        }
    }

    fn lease_stop_search(&mut self) {
        let id = self.lease.as_ref().unwrap().instance;
        let n = self.send(id, "stop", Purpose::Lease);
        let l = self.lease.as_mut().unwrap();
        match n {
            Some(n) => {
                l.step = LeaseStep::SearchStop(n);
                l.deadline = Some(Instant::now() + LEASE_STOP_ACK);
            }
            None => {
                l.step = LeaseStep::Idle;
                l.deadline = None;
            }
        }
    }

    fn lease_final_stop(&mut self) {
        let id = self.lease.as_ref().unwrap().instance;
        match self.send(id, "stop", Purpose::Lease) {
            Some(n) => {
                let l = self.lease.as_mut().unwrap();
                l.step = LeaseStep::FinalStop(n);
                l.deadline = Some(Instant::now() + LEASE_ACK);
            }
            None => {
                let mut events = Vec::new();
                self.lease_failed(LeaseFailure::FinalStopError, &mut events);
                self.pending.append(&mut events);
            }
        }
    }

    /// After the final `stop`: the captured rules when a search changed them, the position the
    /// seat wishes, then `name` to confirm the engine answers in order.
    fn lease_restore(&mut self) {
        let (id, rules) = {
            let l = self.lease.as_ref().unwrap();
            (l.instance, l.captured_rules.clone().filter(|_| l.rules_changed))
        };
        if let Some(r) = rules {
            self.send(id, &format!("kata-set-rules {r}"), Purpose::Lease);
        }
        let target = self.wish_of(id).map(|(t, _)| t.clone());
        if let (Some(target), Some(inst)) = (target, self.get(id)) {
            let (commands, board) = sync_commands(inst, &target);
            let inst = self.get_mut(id).unwrap();
            inst.board = Some(board);
            inst.syncing = Some(target.key());
            for c in commands {
                self.send(id, &c, Purpose::Lease);
            }
        }
        let n = self.send(id, "name", Purpose::Lease);
        let l = self.lease.as_mut().unwrap();
        match n {
            Some(n) => {
                l.step = LeaseStep::Restore(n);
                l.deadline = Some(Instant::now() + LEASE_RESTORE);
            }
            None => l.restore_failed = true,
        }
        if n.is_none() {
            let mut events = Vec::new();
            self.lease_failed(LeaseFailure::RestoreFailed, &mut events);
            self.pending.append(&mut events);
        }
    }

    /// The lease is over without the engine confirmed back. A failure after the take-over leaves
    /// the engine unrestored: it follows no wish until restarted.
    fn lease_failed(&mut self, failure: LeaseFailure, events: &mut Vec<RuntimeEvent>) {
        let Some(l) = self.lease.take() else { return };
        if failure.unrestores() {
            if let Some(inst) = self.get_mut(l.instance) {
                inst.unrestored = Some(failure);
                inst.stream = None;
            }
        }
        let name = self.display_name(l.instance).unwrap_or_default();
        self.trace(&name, "--", &format!("lease {} failed: {failure:?}", l.id));
        events.push(RuntimeEvent::LeaseEnded {
            lease: l.id,
            failure: Some(failure),
        });
        if !failure.unrestores() {
            self.pump_seat(l.instance);
        }
    }

    /// A search's end: its result, or a failure.
    fn lease_search_done(&mut self, events: &mut Vec<RuntimeEvent>) {
        let l = self.lease.as_mut().unwrap();
        let lease = l.id;
        l.step = LeaseStep::Idle;
        l.deadline = None;
        let releasing = l.releasing;
        let search = l.search.take();
        match search {
            Some(s) if !s.failed && s.best.as_ref().is_some_and(|b| !b.candidates.is_empty()) => {
                let line = s.best.unwrap();
                events.push(RuntimeEvent::LeaseResult {
                    lease,
                    doc: s.request.key.doc,
                    node: s.request.key.node,
                    stamp: s.request.key.stamp,
                    komi: f64::from_bits(s.request.key.komi_bits),
                    rules_rev: s.request.key.rules_rev,
                    to_play: s.request.to_play,
                    line,
                });
            }
            _ if releasing => {}
            _ => events.push(RuntimeEvent::LeaseSearchFailed { lease }),
        }
        if releasing {
            self.lease_final_stop();
        }
    }

    /// A frame of the leased engine: `true` when it belonged to the lease.
    fn lease_frame(&mut self, frame: &Frame, events: &mut Vec<RuntimeEvent>) -> bool {
        let (id, step, analyze) = {
            let l = self.lease.as_ref().unwrap();
            (l.instance, l.step, l.search.as_ref().map(|s| s.analyze))
        };
        match frame {
            Frame::Header { id: n } => matches!(self.get(id).and_then(|i| i.waiting.get(n)), Some(Purpose::Lease)),
            Frame::Info { id: n, line } => {
                if analyze != Some(*n) {
                    return false;
                }
                let l = self.lease.as_mut().unwrap();
                let s = l.search.as_mut().unwrap();
                let info = gtp::parse_info(line, Dialect::Kata, s.request.size);
                let total = info
                    .root_visits
                    .unwrap_or_else(|| info.candidates.iter().map(|c| c.visits).sum());
                if !info.candidates.is_empty() {
                    s.best = Some(info);
                }
                if total >= s.target_visits && step == LeaseStep::Search {
                    self.lease_stop_search();
                }
                true
            }
            Frame::Response { id: n, ok, text } => {
                let Some(inst) = self.get_mut(id) else { return false };
                if !matches!(inst.waiting.get(n), Some(Purpose::Lease)) {
                    return false;
                }
                inst.waiting.remove(n);
                let (n, ok) = (*n, *ok);
                match step {
                    LeaseStep::InitialStop(s) if s == n => {
                        if !ok {
                            self.lease_failed(LeaseFailure::InitialStopError, events);
                        } else if self.lease.as_ref().unwrap().releasing {
                            self.lease_restore();
                        } else if let Some(m) = self.send(id, "kata-get-rules", Purpose::Lease) {
                            let l = self.lease.as_mut().unwrap();
                            l.step = LeaseStep::Capture(m);
                            l.deadline = Some(Instant::now() + LEASE_ACK);
                        }
                    }
                    LeaseStep::Capture(s) if s == n => {
                        // Without the engine's own rules it could not be given back as it was
                        // (`setRestoreRules`): the attempt fails before searching.
                        let captured = Some(text.trim())
                            .filter(|t| {
                                ok && serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(t).is_ok()
                            })
                            .map(str::to_string);
                        let l = self.lease.as_mut().unwrap();
                        l.step = LeaseStep::Idle;
                        l.deadline = None;
                        let ready = captured.is_some();
                        l.captured_rules = captured;
                        if l.releasing {
                            self.lease_final_stop();
                        } else if ready {
                            events.push(RuntimeEvent::LeaseReady { lease: l.id });
                        } else {
                            events.push(RuntimeEvent::LeaseSearchFailed { lease: l.id });
                        }
                    }
                    LeaseStep::Search if analyze == Some(n) => {
                        // The stream ended without a stop: the request was refused.
                        self.lease.as_mut().unwrap().search.as_mut().unwrap().failed = true;
                        self.lease_search_done(events);
                    }
                    LeaseStep::SearchStop(s) if s == n => {
                        if !ok {
                            self.lease.as_mut().unwrap().search.as_mut().unwrap().failed = true;
                        }
                        self.lease_search_done(events);
                    }
                    LeaseStep::FinalStop(s) if s == n => {
                        if ok {
                            self.lease_restore();
                        } else {
                            self.lease_failed(LeaseFailure::FinalStopError, events);
                        }
                    }
                    LeaseStep::Restore(s) if s == n => {
                        if ok && !self.lease.as_ref().unwrap().restore_failed {
                            let l = self.lease.take().unwrap();
                            if let Some(inst) = self.get_mut(id) {
                                inst.synced = inst.board.is_some();
                                inst.syncing = None;
                            }
                            events.push(RuntimeEvent::LeaseEnded {
                                lease: l.id,
                                failure: None,
                            });
                            self.pump_seat(id);
                        } else {
                            self.lease_failed(LeaseFailure::RestoreFailed, events);
                        }
                    }
                    // Position, rules and other answers within a step. A refused position leaves
                    // the engine's board unknown: giving it back rebuilds the position.
                    _ if !ok => {
                        let l = self.lease.as_mut().unwrap();
                        let searching = matches!(l.step, LeaseStep::Search | LeaseStep::SearchStop(_));
                        match l.step {
                            LeaseStep::Search | LeaseStep::SearchStop(_) => {
                                if let Some(s) = l.search.as_mut() {
                                    s.failed = true;
                                }
                            }
                            LeaseStep::Restore(_) => l.restore_failed = true,
                            _ => {}
                        }
                        if searching {
                            if let Some(inst) = self.get_mut(id) {
                                inst.board = None;
                            }
                        }
                    }
                    _ => {}
                }
                true
            }
        }
    }

    /// A lease step that was not answered in time.
    fn lease_timeout(&mut self, events: &mut Vec<RuntimeEvent>) {
        let l = self.lease.as_mut().unwrap();
        l.deadline = None;
        match l.step {
            LeaseStep::InitialStop(_) => self.lease_failed(LeaseFailure::InitialStopTimeout, events),
            LeaseStep::Capture(_) => {
                l.step = LeaseStep::Idle;
                if l.releasing {
                    self.lease_final_stop();
                } else {
                    events.push(RuntimeEvent::LeaseSearchFailed { lease: l.id });
                }
            }
            LeaseStep::Search => {
                if let Some(s) = l.search.as_mut() {
                    s.failed = true;
                }
                self.lease_stop_search();
            }
            LeaseStep::SearchStop(_) => {
                if let Some(s) = l.search.as_mut() {
                    s.failed = true;
                }
                self.lease_search_done(events);
            }
            LeaseStep::FinalStop(_) => self.lease_failed(LeaseFailure::FinalStopTimeout, events),
            LeaseStep::Restore(_) => self.lease_failed(LeaseFailure::RestoreFailed, events),
            LeaseStep::Idle => {}
        }
    }

    // -------------------------------------------------------------------- analysis processes

    /// Starts a KataGo analysis process for `command` (`AnalysisEngine.startEngine`); relative
    /// paths resolve like an engine's. Its stdout lines arrive as [`RuntimeEvent::WorkerLine`].
    pub fn start_worker(&mut self, command: &str, name: &str) -> Result<InstanceId, String> {
        let mut spec = gtp::prepare_launch(command, &self.roots).ok_or_else(|| "empty command".to_string())?;
        spec.library_dirs = self.library_dirs.clone();
        let id = self.next_instance;
        self.next_instance += 1;
        self.trace(name, "--", &format!("start {command}"));
        let io = self.launcher.launch(id, &spec).map_err(|e| e.to_string())?;
        self.workers.push(Worker {
            id,
            name: name.to_string(),
            io: Some(io),
            tail: VecDeque::new(),
            queued: None,
        });
        Ok(id)
    }

    /// A whole-game process can defer its launch for an accepted measured-scene fingerprint
    /// check. JSON requests queue until the existing off-lock LaunchCheck returns.
    pub fn start_whole_game_worker(&mut self, command: &str, configured: &str, saved: Option<&EngineProfile>) -> Result<InstanceId, String> {
        let measured = saved.and_then(|p| p.thread_policy.as_ref()).zip(self.measured_hardware.as_ref());
        let Some((policy, hardware)) = measured else { return self.start_worker(command, "整盘精析") };
        let mut spec = gtp::prepare_launch(command, &self.roots).ok_or_else(|| "empty command".to_string())?;
        spec.library_dirs = self.library_dirs.clone();
        let id = self.next_instance; self.next_instance += 1;
        self.checks.push(LaunchCheck { instance: id, command: configured.into(), policy: policy.clone(),
            roots: self.roots.clone(), spec, apple: None, measured: Some(hardware.clone()),
            whole_game: saved.map(|p| p.command.clone()) });
        self.workers.push(Worker { id, name: "整盘精析".into(), io: None, tail: VecDeque::new(), queued: Some(Vec::new()) });
        Ok(id)
    }

    /// Writes one line to analysis process `id`; `false` when it is gone or its input closed.
    pub fn worker_send(&mut self, id: InstanceId, line: &str) -> bool {
        let Some(w) = self.workers.iter_mut().find(|w| w.id == id) else {
            return false;
        };
        let ok = if let Some(queue) = &mut w.queued { queue.push(line.into()); true }
            else { w.io.as_mut().is_some_and(|io| io.send(line).is_ok()) };
        let name = w.name.clone();
        self.trace(&name, ">>", line);
        ok
    }

    pub fn worker_alive(&self, id: InstanceId) -> bool {
        self.workers.iter().any(|w| w.id == id)
    }

    pub fn worker_launch_pending(&self, id: InstanceId) -> bool {
        self.workers.iter().any(|w| w.id == id && w.queued.is_some())
    }

    fn worker_input(&mut self, id: InstanceId, input: EngineInput, events: &mut Vec<RuntimeEvent>) {
        let w = self.workers.iter_mut().find(|w| w.id == id).unwrap();
        let name = w.name.clone();
        match input {
            EngineInput::Stdout(line) => {
                self.trace(&name, "<<", &line);
                events.push(RuntimeEvent::WorkerLine { instance: id, line });
            }
            EngineInput::Stderr(line) => {
                push_tail(&mut w.tail, &line);
                self.trace(&name, "!!", &line);
            }
            EngineInput::Exited(code) => {
                let message = match code {
                    Some(c) => format!("exit code {c}"),
                    None => "terminated".to_string(),
                };
                let output = w.tail.iter().cloned().collect();
                self.trace(&name, "--", &format!("exited: {message}"));
                self.terminate(id);
                events.push(RuntimeEvent::WorkerExited {
                    instance: id,
                    message,
                    output,
                });
            }
        }
    }
}

fn push_tail(tail: &mut VecDeque<String>, line: &str) {
    tail.push_back(line.to_string());
    while tail.len() > OUTPUT_TAIL {
        tail.pop_front();
    }
}

/// Java `"komi " + (komi == 0.0 ? "0" : komi)`: Java's double text.
pub fn komi_text(komi: f64) -> String {
    if komi == 0.0 {
        "0".to_string()
    } else if komi.fract() == 0.0 {
        format!("{komi:.1}")
    } else {
        komi.to_string()
    }
}

/// `checkNameAndVersionForCurrentReader`.
fn classify(name: &str) -> (EngineKind, bool) {
    let name = name.trim();
    let lower = name.to_lowercase();
    let kind = if lower.starts_with("kata") {
        EngineKind::KataGo
    } else if name == "Sayuri" {
        EngineKind::Sayuri
    } else if lower.starts_with("sai") {
        EngineKind::Sai
    } else if name == "Leela" {
        EngineKind::Leela011
    } else if (lower.starts_with("leela") && lower.contains("zero")) || lower.starts_with("pachi") {
        EngineKind::LeelaZero
    } else {
        EngineKind::Other
    };
    (kind, lower.starts_with("katajigo"))
}

fn color_letter(c: Stone) -> &'static str {
    match c {
        Stone::Black => "B",
        Stone::White => "W",
    }
}

/// Commands that bring the engine from what it holds to `target`, and what it holds then.
/// Within one document and anchor, `undo` back to the common moves and `play` forward; anything
/// else rebuilds: board size, `clear_board`, komi, the anchor position, then the moves. Nodes are
/// compared by stamp, so an edited anchor or replayed moves are synced again.
fn sync_commands(inst: &Instance, target: &Target) -> (Vec<String>, EngineBoard) {
    let rec = &target.record;
    let path = rec.path_to(target.node);
    let anchor_at = path
        .iter()
        .rposition(|&n| n == ROOT || !rec.node(n).setup.is_empty())
        .unwrap_or(0);
    let anchor = path[anchor_at];
    let action_nodes: Vec<NodeId> = path[anchor_at + 1..]
        .iter()
        .copied()
        .filter(|&n| rec.node(n).kind.is_action())
        .collect();
    let want = EngineBoard {
        doc: target.doc,
        size: rec.size,
        komi_bits: target.komi.to_bits(),
        anchor: rec.node(anchor).stamp,
        actions: action_nodes.iter().map(|&n| rec.node(n).stamp).collect(),
    };
    let play = |n: NodeId| match rec.node(n).kind {
        NodeKind::Move { color, point } => format!("play {} {}", color_letter(color), gtp::vertex(rec.size, point)),
        NodeKind::Pass { color } => format!("play {} pass", color_letter(color)),
        _ => unreachable!("only actions are replayed"),
    };
    let mut cmds = Vec::new();
    if let Some(have) = &inst.board {
        if have == &want {
            return (cmds, want);
        }
        if have.doc == want.doc && have.size == want.size && have.anchor == want.anchor && inst.has("undo") {
            let common = have
                .actions
                .iter()
                .zip(&want.actions)
                .take_while(|(a, b)| a == b)
                .count();
            cmds.extend(std::iter::repeat_n("undo".to_string(), have.actions.len() - common));
            if have.komi_bits != want.komi_bits {
                cmds.push(format!("komi {}", komi_text(target.komi)));
            }
            cmds.extend(action_nodes[common..].iter().map(|&n| play(n)));
            return (cmds, want);
        }
    }
    let (w, h) = (rec.size.width, rec.size.height);
    cmds.push(if w == h {
        format!("boardsize {w}")
    } else {
        format!("rectangular_boardsize {w} {h}")
    });
    cmds.push("clear_board".to_string());
    cmds.push(format!("komi {}", komi_text(target.komi)));
    let pos = rec.position(anchor);
    let stones: Vec<String> = pos
        .stones
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let p = Point {
                x: (i % w as usize) as u16,
                y: (i / w as usize) as u16,
            };
            s.map(|c| format!("{} {}", color_letter(c), gtp::vertex(rec.size, p)))
        })
        .collect();
    if !stones.is_empty() {
        if inst.has("set_position") {
            cmds.push(format!("set_position {}", stones.join(" ")));
        } else {
            cmds.extend(stones.iter().map(|s| format!("play {s}")));
        }
    }
    cmds.extend(action_nodes.iter().map(|&n| play(n)));
    (cmds, want)
}

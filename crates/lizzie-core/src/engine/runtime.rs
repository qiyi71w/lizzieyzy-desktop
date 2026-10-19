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
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::gtp::{self, Dialect, Frame, Framer, InfoLine, LaunchSpec};
use super::profile::EngineProfile;
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
/// session requires (`None`: whatever the engine has).
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
            komi_bits: self.komi.to_bits(),
            rules_rev: self.rules_rev,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TargetKey {
    doc: u64,
    node: NodeId,
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
    /// The process failed to start or ended unexpectedly; the instance is gone.
    Failed(EngineFailure),
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
    /// Leela 0.11 starts pondering with an ordinary, immediately answered GTP command.
    Ponder,
    /// Ends a pondering cycle to obtain its stderr summary, without changing its owner.
    Probe,
    Stop,
    User,
}

/// What the engine will hold once it has processed every command sent so far.
#[derive(Clone, Debug, PartialEq)]
struct EngineBoard {
    doc: u64,
    size: BoardSize,
    komi_bits: u64,
    anchor: NodeId,
    actions: Vec<NodeId>,
}

/// An analysis request: which position it is for.
#[derive(Clone, Copy)]
struct Request {
    key: TargetKey,
    to_play: Stone,
    size: BoardSize,
    started: Instant,
}

struct Instance {
    id: InstanceId,
    profile: EngineProfile,
    io: Option<Box<dyn EngineIo>>,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UserCommandError {
    Empty,
    NoEngine,
    /// The command would change the engine's board behind the record's back.
    ChangesBoard,
}

pub struct Engines {
    launcher: Box<dyn Launcher>,
    roots: Vec<PathBuf>,
    instances: Vec<Instance>,
    next_instance: InstanceId,
    foreground: Option<InstanceId>,
    wish: Option<(Target, bool)>,
    interval_centisec: u32,
    console: VecDeque<ConsoleLine>,
    console_seq: u64,
    trace: Option<Box<dyn Write + Send>>,
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
            instances: Vec::new(),
            next_instance: 1,
            foreground: None,
            wish: None,
            interval_centisec: 10,
            console: VecDeque::new(),
            console_seq: 0,
            trace: None,
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
        self.foreground
            .and_then(|id| self.get(id))
            .is_some_and(|i| i.phase == Phase::Ready && i.synced)
    }

    /// Whether the foreground engine is producing analysis for this node.
    pub fn analysing(&self, doc: u64, node: NodeId) -> bool {
        self.foreground.and_then(|id| self.get(id)).is_some_and(|i| {
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

    /// `analyze-update-interval-centisec`; 0 means the default 10.
    pub fn set_interval(&mut self, centisec: u32) {
        self.interval_centisec = if centisec == 0 { 10 } else { centisec };
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
        let spec = gtp::prepare_launch(&profile.command, &self.roots).ok_or_else(|| fail("empty command".into()))?;
        let io = self.launcher.launch(id, &spec).map_err(|e| fail(e.to_string()))?;
        self.instances.push(Instance {
            id,
            profile: profile.clone(),
            io: Some(io),
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
        });
        let name = profile.name.clone();
        self.trace(&name, "--", &format!("start {}", profile.command));
        self.send(id, "name", Purpose::Name);
        self.send(id, "version", Purpose::Version);
        self.send(id, "list_commands", Purpose::ListCommands);
        Ok(id)
    }

    /// Makes `id` (or nothing) the foreground engine. The previous foreground engine stops
    /// analysing; it keeps running when `keep_previous` (fast engine change) and is ended
    /// otherwise.
    pub fn set_foreground(&mut self, id: Option<InstanceId>, keep_previous: bool) {
        let previous = self.foreground;
        if previous == id {
            return;
        }
        self.foreground = id;
        if let Some(p) = previous {
            if keep_previous {
                self.stop_stream(p);
            } else {
                self.terminate(p);
            }
        }
        if let Some(i) = id.and_then(|id| self.get_mut(id)) {
            i.synced = false;
            i.rejected = None;
        }
        self.pump();
    }

    /// Ends one instance. Its later output is ignored.
    pub fn terminate(&mut self, id: InstanceId) {
        let Some(at) = self.instances.iter().position(|i| i.id == id) else {
            return;
        };
        let mut inst = self.instances.remove(at);
        if self.foreground == Some(id) {
            self.foreground = None;
        }
        self.trace(&inst.display_name(), "--", "terminate");
        if let Some(io) = inst.io.take() {
            io.terminate();
        }
    }

    pub fn terminate_all(&mut self) {
        let ids: Vec<InstanceId> = self.instances.iter().map(|i| i.id).collect();
        for id in ids {
            self.terminate(id);
        }
    }

    // -------------------------------------------------------------------- wishes

    /// What the foreground engine should do: hold `target` (synced even while not analysing, so
    /// a switch completes) and analyse it when `analyse`. `None` stops and leaves the board.
    pub fn want(&mut self, target: Option<Target>, analyse: bool) {
        self.wish = target.map(|t| (t, analyse));
        if let Some(inst) = self
            .foreground
            .and_then(|id| self.instances.iter_mut().find(|i| i.id == id))
        {
            let keep_pondering = self.wish.as_ref().is_some_and(|(target, analyse)| {
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
        self.pump();
    }

    /// When [`tick`](Self::tick) has work next (a Leela 0.11 pondering probe), if ever.
    pub fn next_tick(&self) -> Option<Instant> {
        let inst = self.get(self.foreground?)?;
        if inst.fence.is_none() && inst.stream.is_some() {
            inst.ponder_due
        } else {
            None
        }
    }

    /// Advances timed engine work without owning a thread: the platform calls this at
    /// [`next_tick`](Self::next_tick); controlled-protocol tests supply a later instant directly.
    pub fn tick(&mut self, now: Instant) {
        let Some(id) = self.foreground else { return };
        if self.next_tick().is_some_and(|due| now >= due) {
            self.get_mut(id).unwrap().ponder_due = None;
            self.send(id, "name", Purpose::Probe);
            self.fence_last(id);
        }
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

    /// Sends the next batch for the foreground wish when nothing is in flight.
    fn pump(&mut self) {
        let Some(fg) = self.foreground else { return };
        let interval = self.interval_centisec;
        let Some(inst) = self.get(fg) else { return };
        if inst.phase != Phase::Ready || inst.fence.is_some() {
            return;
        }
        let first_new = inst.next_id;
        match self.wish.clone() {
            None => {
                self.stop_stream(fg);
            }
            Some((target, analyse)) => {
                let key = target.key();
                if let Some(r) = &target.rules {
                    self.request_rules(fg, r);
                }
                let inst = self.get(fg).unwrap();
                if inst.rejected == Some(key) {
                    self.stop_stream(fg);
                } else {
                    let (commands, board) = sync_commands(inst, &target);
                    let streaming_here = inst.stream.and_then(|n| inst.request(n)).is_some_and(|r| r.key == key);
                    let dialect = inst.dialect();
                    let root_info = inst.supports_root_info();
                    if commands.is_empty() {
                        self.get_mut(fg).unwrap().synced = true;
                    } else {
                        let i = self.get_mut(fg).unwrap();
                        i.board = Some(board);
                        i.syncing = Some(key);
                        for c in commands {
                            self.send(fg, &c, Purpose::Sync);
                        }
                    }
                    let streaming_here = streaming_here && self.get(fg).unwrap().stream.is_some();
                    match (analyse, dialect) {
                        (true, Some(d)) if !streaming_here => {
                            let to_play = target.record.node(target.node).to_play;
                            let color = if to_play == Stone::Black { "B" } else { "W" };
                            let cmd = match d {
                                Dialect::Kata => format!(
                                    "kata-analyze {color} {interval}{}",
                                    if root_info { " rootInfo true" } else { "" }
                                ),
                                Dialect::Leela | Dialect::Sai => format!("lz-analyze {color} {interval}"),
                                Dialect::Sayuri => format!("analyze {color} {interval}"),
                                Dialect::Leela011 => "time_left b 0 0".to_string(),
                            };
                            let request = Request {
                                key,
                                to_play,
                                size: target.record.size,
                                started: Instant::now(),
                            };
                            let purpose = if d == Dialect::Leela011 {
                                Purpose::Ponder
                            } else {
                                Purpose::Analyze
                            };
                            if let Some(n) = self.send(fg, &cmd, purpose) {
                                let i = self.get_mut(fg).unwrap();
                                i.stream = Some(n);
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
                            let i = self.get(fg).unwrap();
                            let n = i.stream.unwrap();
                            // A completed table starts the next cycle for the same request;
                            // its elapsed time and position ownership do not restart.
                            if i.ponder_due.is_none() && !i.summaries.iter().any(|(owner, _, _)| *owner == n) {
                                let request = i.request(n).unwrap();
                                if self.send(fg, "time_left b 0 0", Purpose::Ponder).is_some() {
                                    let i = self.get_mut(fg).unwrap();
                                    i.summaries.push_back((n, request, InfoLine::default()));
                                    i.ponder_due = Some(Instant::now() + PONDER_INTERVAL);
                                }
                            }
                        }
                        (false, _) | (true, None) => {
                            self.stop_stream(fg);
                        }
                        _ => {}
                    }
                }
            }
        }
        let i = self.get_mut(fg).unwrap();
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
        let ok = inst.io.as_mut().is_some_and(|io| io.send(&line).is_ok());
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

    /// GtpConsolePane send: a raw command to the foreground engine. Board-changing commands are
    /// refused because the record owns the position. After its answer, analysis resumes.
    pub fn send_user(&mut self, command: &str) -> Result<(), UserCommandError> {
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
        self.send(fg, command, Purpose::User);
        self.fence_last(fg);
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

    // -------------------------------------------------------------------- input

    /// Output of instance `id`. Returns what the application flow must act on.
    pub fn input(&mut self, id: InstanceId, input: EngineInput) -> Vec<RuntimeEvent> {
        let mut events = Vec::new();
        let Some(inst) = self.get_mut(id) else {
            return events; // a retired instance
        };
        let name = inst.display_name();
        match input {
            EngineInput::Stderr(line) => {
                push_tail(&mut inst.output_tail, &line);
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
        events
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
        let Some(inst) = self.get_mut(id) else { return };
        match frame {
            Frame::Header { id: n } => {
                // The engine has consumed everything up to an analysis request: that request's
                // batch is answered even though its stream keeps going.
                if matches!(inst.waiting.get(&n), Some(Purpose::Analyze)) {
                    inst.waiting.remove(&n);
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
                    _ => {}
                }
                let Some(inst) = self.get_mut(id) else { return };
                if inst.ready_fence == Some(n) {
                    inst.phase = Phase::Ready;
                    inst.ready_fence = None;
                    events.push(RuntimeEvent::Ready { instance: id });
                    if self.foreground == Some(id) {
                        self.pump();
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
        if self.foreground == Some(id) {
            self.pump();
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
        let inst = self.get_mut(id).unwrap();
        inst.ready_fence = inst.next_id.checked_sub(1);
        inst.board = None;
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
/// else rebuilds: board size, `clear_board`, komi, the anchor position, then the moves.
fn sync_commands(inst: &Instance, target: &Target) -> (Vec<String>, EngineBoard) {
    let rec = &target.record;
    let path = rec.path_to(target.node);
    let anchor_at = path
        .iter()
        .rposition(|&n| n == ROOT || !rec.node(n).setup.is_empty())
        .unwrap_or(0);
    let anchor = path[anchor_at];
    let actions: Vec<NodeId> = path[anchor_at + 1..]
        .iter()
        .copied()
        .filter(|&n| rec.node(n).kind.is_action())
        .collect();
    let want = EngineBoard {
        doc: target.doc,
        size: rec.size,
        komi_bits: target.komi.to_bits(),
        anchor,
        actions,
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
            cmds.extend(want.actions[common..].iter().map(|&n| play(n)));
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
    cmds.extend(want.actions.iter().map(|&n| play(n)));
    (cmds, want)
}

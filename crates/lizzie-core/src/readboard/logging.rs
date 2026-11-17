//! The helper's diagnostic logging negotiation (Java `ReadBoardLoggingProtocol`,
//! `ReadBoardLoggingControl`, `ReadBoardLoggingSnapshot` and the `ReadBoard` methods that drive
//! them: `completeLaunchArguments`, `handleReady`, `handleLoggingControlLine`,
//! `requestLoggingSet`, the logging timeout and `disconnectLoggingControl`).
//!
//! A host that has an absolute Windows log directory and an opaque session id launches the helper
//! with named arguments (`--log-dir … --logging-contract 1 …`); such a helper answers `ready` with
//! a `readboardLoggingV1` capability line, after which the host may ask it to switch its
//! diagnostics, capture and trace writers with `readboardLoggingSet` and waits for the matching
//! `readboardLoggingObserved`. Nothing is ever reported as enabled before the helper confirmed it,
//! and a missing answer within [`CAPABILITY_TIMEOUT`] leaves the state unknown, never successful.
//! Everything here is pure: the caller passes clock readings and random request-id bytes.

use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

use base64::Engine as _;
use serde::Serialize;
use ts_rs::TS;

pub const CAPABILITY_COMMAND: &str = "readboardLoggingV1";
pub const SET_COMMAND: &str = "readboardLoggingSet";
pub const OBSERVED_COMMAND: &str = "readboardLoggingObserved";
const ARG_LOG_DIR: &str = "--log-dir";
const ARG_HOST_SESSION_ID: &str = "--host-session-id";
const ARG_LOGGING_CONTRACT: &str = "--logging-contract";
const ARG_DIAGNOSTICS: &str = "--diagnostics";
const ARG_CAPTURE: &str = "--capture";
const CONTRACT_VERSION: &str = "1";

/// Java `ReadBoard.LOGGING_CAPABILITY_TIMEOUT_MS`: how long the host waits for the capability
/// line after `ready`, and for the acknowledgement of a set request.
pub const CAPABILITY_TIMEOUT: Duration = Duration::from_millis(5000);

/// One writer switch as the wire carries it (`on`/`off`, `unknown` only in observed lines).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    Off,
    On,
    Unknown,
}

impl Toggle {
    fn of(on: bool) -> Toggle {
        if on {
            Toggle::On
        } else {
            Toggle::Off
        }
    }

    fn parse_determinate(token: &str) -> Option<Toggle> {
        match token {
            "on" => Some(Toggle::On),
            "off" => Some(Toggle::Off),
            _ => None,
        }
    }

    fn parse_observed(token: &str) -> Option<Toggle> {
        if token == "unknown" {
            Some(Toggle::Unknown)
        } else {
            Toggle::parse_determinate(token)
        }
    }

    /// Java `formatDeterminateToggle`: anything but `On` is written as `off`.
    fn determinate_token(self) -> &'static str {
        if self == Toggle::On {
            "on"
        } else {
            "off"
        }
    }

    fn token(self) -> &'static str {
        match self {
            Toggle::Off => "off",
            Toggle::On => "on",
            Toggle::Unknown => "unknown",
        }
    }
}

/// Health of the helper's log writers, worst last.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, TS)]
pub enum Persistence {
    Healthy,
    Degraded,
    Unavailable,
}

impl Persistence {
    fn parse(token: &str) -> Option<Persistence> {
        match token {
            "healthy" => Some(Persistence::Healthy),
            "degraded" => Some(Persistence::Degraded),
            "unavailable" => Some(Persistence::Unavailable),
            _ => None,
        }
    }

    fn token(self) -> &'static str {
        match self {
            Persistence::Healthy => "healthy",
            Persistence::Degraded => "degraded",
            Persistence::Unavailable => "unavailable",
        }
    }
}

/// Why the helper reports its observed state. Only these fixed tokens are accepted, so raw paths
/// or exception text never travel through the logging channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Applied,
    LegacyHelper,
    CapabilityTimeout,
    PathUnavailable,
    WriterFault,
    InvalidRequest,
}

impl Reason {
    fn parse(token: &str) -> Option<Reason> {
        match token {
            "applied" => Some(Reason::Applied),
            "legacy-helper" => Some(Reason::LegacyHelper),
            "capability-timeout" => Some(Reason::CapabilityTimeout),
            "path-unavailable" => Some(Reason::PathUnavailable),
            "writer-fault" => Some(Reason::WriterFault),
            "invalid-request" => Some(Reason::InvalidRequest),
            _ => None,
        }
    }
}

/// `readboardLoggingV1 <processSession> <diagnostics> <capture> <trace> <persistence> <drops>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capability {
    pub process_session_id: String,
    pub diagnostics: Toggle,
    pub capture: Toggle,
    pub trace: Toggle,
    pub persistence: Persistence,
    pub drop_count: u32,
}

/// `readboardLoggingSet <requestId> <diagnostics> <capture> <trace>` (host → helper).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetRequest {
    pub request_id: String,
    pub diagnostics: Toggle,
    pub capture: Toggle,
    pub trace: Toggle,
}

/// `readboardLoggingObserved <requestId> <processSession> <diagnostics> <capture> <trace>
/// <persistence> <drops> <reason>`. A capability line is kept as an observed state with an empty
/// request id and reason `Applied` (Java `observedFromCapability`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observed {
    pub request_id: String,
    pub process_session_id: String,
    pub diagnostics: Toggle,
    pub capture: Toggle,
    pub trace: Toggle,
    pub persistence: Persistence,
    pub drop_count: u32,
    pub reason: Reason,
}

/// Java `trim()`: strips characters up to U+0020 at both ends.
fn java_trim(text: &str) -> &str {
    text.trim_matches(|c: char| c <= ' ')
}

fn without_line_breaks(line: &str) -> String {
    line.chars().filter(|&c| c != '\r' && c != '\n').collect()
}

/// Java `ReadBoardLoggingProtocol.isControlLine`: the first word names one of the three logging
/// commands, whether or not the rest is valid. Such lines never reach the sync protocol.
pub fn is_control_line(line: &str) -> bool {
    let cleaned = without_line_breaks(line);
    let command = java_trim(&cleaned).split(' ').next().unwrap_or("");
    command == CAPABILITY_COMMAND || command == SET_COMMAND || command == OBSERVED_COMMAND
}

/// Java `isOpaqueId`: non-empty, only ASCII letters, digits, `-` and `_`.
pub fn is_opaque_id(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Java `isAbsoluteLogDirectory` (same rule as the helper's `LaunchOptions`): a Windows drive
/// path `X:\…` or `X:/…` whose next character is not another separator, or a UNC path
/// `\\server\share…` (not `\\?\`). Unix paths are not accepted, because the helper runs on
/// Windows. Text rules only, so they behave identically on every platform.
pub fn is_absolute_log_directory(path: &str) -> bool {
    if java_trim(path).is_empty() {
        return false;
    }
    let chars: Vec<char> = path.chars().collect();
    let separator = |c: char| c == '\\' || c == '/';
    if chars.len() >= 4
        && chars[0].is_ascii_alphabetic()
        && chars[1] == ':'
        && separator(chars[2])
        && !separator(chars[3])
    {
        return true;
    }
    if !path.starts_with("\\\\") || path.starts_with("\\\\?\\") {
        return false;
    }
    let find = |start: usize| chars[start..].iter().position(|&c| separator(c)).map(|i| i + start);
    let server_end = match find(2) {
        Some(end) if end > 2 => end,
        _ => return false,
    };
    let share_start = server_end + 1;
    if share_start >= chars.len() {
        return false;
    }
    let share_end = find(share_start).unwrap_or(chars.len());
    share_end > share_start
}

/// The logging inputs of one launch (Java `completeLaunchArguments` reads them from the running
/// `LoggingRuntime`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoggingLaunch {
    /// The helper's log directory, see [`readboard_log_directory`].
    pub log_dir: String,
    /// The host's application log session id.
    pub host_session_id: String,
    /// The host's diagnostics policy (`LoggingSettings.diagnosticsEnabled`).
    pub diagnostics: bool,
}

impl LoggingLaunch {
    /// Whether this launch uses the logging contract: an absolute Windows log directory and an
    /// opaque session id. Otherwise the helper starts without named arguments (legacy launch).
    pub fn is_contract(&self) -> bool {
        is_absolute_log_directory(&self.log_dir) && is_opaque_id(&self.host_session_id)
    }
}

/// Java `ReadBoardLoggingControl.readBoardLogDirectory`: `<logs>/readboard`.
pub fn readboard_log_directory(logs_dir: &Path) -> String {
    logs_dir.join("readboard").to_string_lossy().into_owned()
}

/// Java `ReadBoardLoggingControl.appendNamedLoggingArguments`: the named arguments that follow the
/// seven positional ones. Capture always starts off; empty for a legacy launch.
pub fn named_launch_arguments(launch: &LoggingLaunch) -> Vec<String> {
    if !launch.is_contract() {
        return Vec::new();
    }
    vec![
        ARG_LOG_DIR.to_string(),
        launch.log_dir.clone(),
        ARG_HOST_SESSION_ID.to_string(),
        launch.host_session_id.clone(),
        ARG_LOGGING_CONTRACT.to_string(),
        CONTRACT_VERSION.to_string(),
        ARG_DIAGNOSTICS.to_string(),
        Toggle::of(launch.diagnostics).determinate_token().to_string(),
        ARG_CAPTURE.to_string(),
        Toggle::Off.determinate_token().to_string(),
    ]
}

/// Java `splitFields`: line breaks removed, trimmed, split on single spaces; exactly `count`
/// non-empty fields.
fn split_fields(line: &str, count: usize) -> Option<Vec<String>> {
    let cleaned = without_line_breaks(line);
    let trimmed = java_trim(&cleaned);
    if trimmed.is_empty() {
        return None;
    }
    let fields: Vec<String> = trimmed.split(' ').map(str::to_string).collect();
    if fields.len() != count || fields.iter().any(String::is_empty) {
        return None;
    }
    Some(fields)
}

/// Java `parseDropCount`: ASCII digits only, within Java's `int` range.
fn parse_drop_count(token: &str) -> Option<u32> {
    if token.is_empty() || !token.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    token.parse::<i32>().ok().map(|v| v as u32)
}

/// Java `tryParseCapability`.
pub fn parse_capability(line: &str) -> Option<Capability> {
    let f = split_fields(line, 7)?;
    if f[0] != CAPABILITY_COMMAND || !is_opaque_id(&f[1]) {
        return None;
    }
    Some(Capability {
        diagnostics: Toggle::parse_determinate(&f[2])?,
        capture: Toggle::parse_determinate(&f[3])?,
        trace: Toggle::parse_determinate(&f[4])?,
        persistence: Persistence::parse(&f[5])?,
        drop_count: parse_drop_count(&f[6])?,
        process_session_id: f[1].clone(),
    })
}

/// Java `tryParseSet`.
pub fn parse_set(line: &str) -> Option<SetRequest> {
    let f = split_fields(line, 5)?;
    if f[0] != SET_COMMAND || !is_opaque_id(&f[1]) {
        return None;
    }
    Some(SetRequest {
        diagnostics: Toggle::parse_determinate(&f[2])?,
        capture: Toggle::parse_determinate(&f[3])?,
        trace: Toggle::parse_determinate(&f[4])?,
        request_id: f[1].clone(),
    })
}

/// Java `tryParseObserved`.
pub fn parse_observed(line: &str) -> Option<Observed> {
    let f = split_fields(line, 9)?;
    if f[0] != OBSERVED_COMMAND || !is_opaque_id(&f[1]) || !is_opaque_id(&f[2]) {
        return None;
    }
    Some(Observed {
        diagnostics: Toggle::parse_observed(&f[3])?,
        capture: Toggle::parse_observed(&f[4])?,
        trace: Toggle::parse_observed(&f[5])?,
        persistence: Persistence::parse(&f[6])?,
        drop_count: parse_drop_count(&f[7])?,
        reason: Reason::parse(&f[8])?,
        request_id: f[1].clone(),
        process_session_id: f[2].clone(),
    })
}

/// Java `formatSet`.
pub fn format_set(request: &SetRequest) -> String {
    format!(
        "{SET_COMMAND} {} {} {} {}",
        request.request_id,
        request.diagnostics.determinate_token(),
        request.capture.determinate_token(),
        request.trace.determinate_token()
    )
}

/// The writers the user wants (Java `ReadBoardLoggingControl.Desired`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Desired {
    pub diagnostics: bool,
    pub capture: bool,
    pub trace: bool,
}

impl Desired {
    /// Java `Desired.launchDefaults`: the diagnostics policy, capture and trace off.
    pub fn launch_defaults(diagnostics: bool) -> Desired {
        Desired { diagnostics, capture: false, trace: false }
    }
}

/// Java `ReadBoardLoggingControl.Status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum LoggingStatus {
    /// Contract launch waiting for the capability line.
    Starting,
    CapabilityReady,
    /// A set request waits for its acknowledgement.
    Applying,
    Observed,
    /// Timed out or disconnected: nothing is known.
    Unknown,
    /// A launch without the logging contract (old helper or no usable log directory).
    LegacyUnconfirmed,
}

/// Java `ReadBoardLoggingControl.Presentation`: what the diagnostics view shows for one writer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum Presentation {
    Off,
    On,
    OnStorageDegraded,
    OnStorageUnavailable,
    NotApplied,
    Unknown,
    LegacyUnconfirmed,
}

/// Java `ReadBoardLoggingControl.presentation`.
fn presentation(
    status: LoggingStatus,
    desired_on: bool,
    observed: Toggle,
    persistence: Option<Persistence>,
) -> Presentation {
    if status == LoggingStatus::LegacyUnconfirmed {
        return Presentation::LegacyUnconfirmed;
    }
    if matches!(status, LoggingStatus::Unknown | LoggingStatus::Starting) || observed == Toggle::Unknown {
        return Presentation::Unknown;
    }
    if desired_on && observed == Toggle::Off {
        return Presentation::NotApplied;
    }
    if observed == Toggle::On {
        return match persistence {
            Some(Persistence::Degraded) => Presentation::OnStorageDegraded,
            Some(Persistence::Unavailable) => Presentation::OnStorageUnavailable,
            _ => Presentation::On,
        };
    }
    Presentation::Off
}

/// A read-only view for the diagnostics UI (Java `ReadBoardLoggingSnapshot`).
#[derive(Clone, Debug, PartialEq)]
pub struct LoggingSnapshot {
    /// A helper (and its control) exists.
    pub attached: bool,
    pub contract_launch: bool,
    pub status: LoggingStatus,
    /// Empty while no process session is known.
    pub process_session_id: String,
    /// A capability or acknowledgement was received.
    pub capability_known: bool,
    pub desired: Desired,
    pub observed_diagnostics: Toggle,
    pub observed_capture: Toggle,
    pub observed_trace: Toggle,
    pub persistence: Option<Persistence>,
    pub drop_count: Option<u32>,
    pub reason: Option<Reason>,
    pub diagnostics_presentation: Presentation,
    pub capture_presentation: Presentation,
    pub trace_presentation: Presentation,
    /// Java `captureSummary`: `no capture session`, or
    /// `processSession=<id> desired=<on|off> observed=<toggle> persistence=<health|none>`.
    pub capture_summary: String,
    /// When the process session was first seen (wall clock passed in by the caller).
    pub process_session_observed_at: Option<SystemTime>,
}

const NO_CAPTURE_SESSION: &str = "no capture session";

impl LoggingSnapshot {
    /// Java `ReadBoardLoggingSnapshot.detached`: no helper; unknown, never success.
    pub fn detached() -> LoggingSnapshot {
        LoggingSnapshot {
            attached: false,
            contract_launch: false,
            status: LoggingStatus::Unknown,
            process_session_id: String::new(),
            capability_known: false,
            desired: Desired::launch_defaults(false),
            observed_diagnostics: Toggle::Unknown,
            observed_capture: Toggle::Unknown,
            observed_trace: Toggle::Unknown,
            persistence: None,
            drop_count: None,
            reason: None,
            diagnostics_presentation: Presentation::Unknown,
            capture_presentation: Presentation::Unknown,
            trace_presentation: Presentation::Unknown,
            capture_summary: NO_CAPTURE_SESSION.to_string(),
            process_session_observed_at: None,
        }
    }
}

/// The logging state of one helper launch: Java `ReadBoardLoggingControl` plus the `ReadBoard`
/// code that feeds it lines, arms its timeouts and sends its requests. The workbench owns one per
/// helper, routes logging lines (see [`is_control_line`]) to [`LoggingControl::on_line`], calls
/// [`LoggingControl::tick`] at [`LoggingControl::next_deadline`] and disconnects it when the
/// helper stops.
#[derive(Clone, Debug)]
pub struct LoggingControl {
    contract_launch: bool,
    /// Java `RequestGate`: the newest request id; only its acknowledgement counts.
    latest_request: Option<String>,
    desired: Desired,
    observed: Option<Observed>,
    process_session_id: Option<String>,
    process_session_observed_at: Option<SystemTime>,
    status: LoggingStatus,
    /// Java `stateGeneration`: every transition that settles or abandons a wait bumps it, so a
    /// timeout armed for an older wait never fires.
    generation: u64,
    disconnected: bool,
    /// The armed wait: its generation and when it times out.
    wait: Option<(u64, Instant)>,
}

impl LoggingControl {
    /// Java `completeLaunchArguments` → `ReadBoardLoggingControl.forLaunch`: a contract launch
    /// when `launch` is usable (the same test that adds the named arguments), otherwise a legacy
    /// launch. Desired writers start from the diagnostics policy.
    pub fn for_launch(launch: Option<&LoggingLaunch>) -> LoggingControl {
        let contract_launch = launch.is_some_and(LoggingLaunch::is_contract);
        let diagnostics = launch.is_some_and(|l| l.diagnostics);
        LoggingControl {
            contract_launch,
            latest_request: None,
            desired: Desired::launch_defaults(diagnostics),
            observed: None,
            process_session_id: None,
            process_session_observed_at: None,
            status: if contract_launch { LoggingStatus::Starting } else { LoggingStatus::LegacyUnconfirmed },
            generation: 0,
            disconnected: false,
            wait: None,
        }
    }

    pub fn is_contract_launch(&self) -> bool {
        self.contract_launch
    }

    pub fn status(&self) -> LoggingStatus {
        self.status
    }

    pub fn desired(&self) -> Desired {
        self.desired
    }

    pub fn observed(&self) -> Option<&Observed> {
        self.observed.as_ref()
    }

    pub fn process_session_id(&self) -> Option<&str> {
        self.process_session_id.as_deref()
    }

    /// Java `invalidatePendingWaitLocked`.
    fn invalidate_wait(&mut self) {
        self.generation += 1;
        self.wait = None;
    }

    fn arm_wait(&mut self, now: Instant) {
        self.generation += 1;
        self.wait = Some((self.generation, now + CAPABILITY_TIMEOUT));
    }

    fn clear_observed_success(&mut self) {
        self.process_session_id = None;
        self.process_session_observed_at = None;
        self.observed = None;
    }

    /// Java `isTimeoutGenerationCurrent`.
    fn wait_is_current(&self, generation: u64) -> bool {
        !self.disconnected
            && generation == self.generation
            && matches!(self.status, LoggingStatus::Starting | LoggingStatus::Applying)
    }

    /// The first `ready` of the helper (Java `handleReady` → `onReady` + `scheduleLoggingTimeout`):
    /// a contract launch starts waiting [`CAPABILITY_TIMEOUT`] for the capability line; a legacy
    /// launch stays legacy.
    pub fn on_ready(&mut self, now: Instant) {
        if self.disconnected {
            return;
        }
        if !self.contract_launch {
            self.status = LoggingStatus::LegacyUnconfirmed;
            self.clear_observed_success();
            self.invalidate_wait();
            return;
        }
        if matches!(
            self.status,
            LoggingStatus::CapabilityReady | LoggingStatus::Observed | LoggingStatus::Applying
        ) {
            return;
        }
        self.status = LoggingStatus::Starting;
        self.arm_wait(now);
    }

    /// A logging control line from the helper (Java `handleLoggingControlLine`). `now` is the wall
    /// clock, kept as the time the process session was first seen. Returns whether the state
    /// changed; invalid, stale or foreign lines are ignored.
    pub fn on_line(&mut self, line: &str, now: SystemTime) -> bool {
        if let Some(capability) = parse_capability(line) {
            return self.on_capability(capability, now);
        }
        match parse_observed(line) {
            Some(observed) => self.on_observed(observed, now),
            None => false,
        }
    }

    /// Java `onCapability`: establishes the process session (a capability from another session is
    /// ignored) and its initial observed state; any pending wait is settled.
    fn on_capability(&mut self, capability: Capability, now: SystemTime) -> bool {
        if self.disconnected
            || self.process_session_id.as_ref().is_some_and(|id| *id != capability.process_session_id)
        {
            return false;
        }
        // Java `observedSessionAt`: the first sighting of the session is kept.
        self.process_session_observed_at = Some(self.process_session_observed_at.unwrap_or(now));
        self.observed = Some(Observed {
            request_id: String::new(),
            process_session_id: capability.process_session_id.clone(),
            diagnostics: capability.diagnostics,
            capture: capability.capture,
            trace: capability.trace,
            persistence: capability.persistence,
            drop_count: capability.drop_count,
            reason: Reason::Applied,
        });
        self.process_session_id = Some(capability.process_session_id);
        self.latest_request = None;
        self.status = LoggingStatus::CapabilityReady;
        self.invalidate_wait();
        true
    }

    /// Java `onObserved`: only the acknowledgement of the newest request, while it is still
    /// awaited, from the known process session.
    fn on_observed(&mut self, incoming: Observed, now: SystemTime) -> bool {
        if self.disconnected
            || self.status != LoggingStatus::Applying
            || self.latest_request.as_deref() != Some(incoming.request_id.as_str())
            || self.process_session_id.as_ref().is_some_and(|id| *id != incoming.process_session_id)
        {
            return false;
        }
        self.process_session_observed_at = Some(self.process_session_observed_at.unwrap_or(now));
        self.process_session_id = Some(incoming.process_session_id.clone());
        self.observed = Some(incoming);
        self.status = LoggingStatus::Observed;
        self.invalidate_wait();
        true
    }

    /// Java `requestLoggingSet` → `beginSetIfCurrent`: asks the helper to switch its writers.
    /// Needs a known process session (a capability line); returns the line to send, or `None`.
    /// The request id is `id_bytes` in URL-safe base64 without padding (Java: 12 `SecureRandom`
    /// bytes). Arms a [`CAPABILITY_TIMEOUT`] wait for the acknowledgement. If the line cannot be
    /// written, call [`LoggingControl::send_failed`].
    pub fn request_set(
        &mut self,
        diagnostics: bool,
        capture: bool,
        trace: bool,
        id_bytes: [u8; 12],
        now: Instant,
    ) -> Option<String> {
        if self.disconnected || self.process_session_id.as_deref().is_none_or(str::is_empty) {
            return None;
        }
        let request_id = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(id_bytes);
        let line = format_set(&SetRequest {
            request_id: request_id.clone(),
            diagnostics: Toggle::of(diagnostics),
            capture: Toggle::of(capture),
            trace: Toggle::of(trace),
        });
        self.desired = Desired { diagnostics, capture, trace };
        self.latest_request = Some(request_id);
        self.status = LoggingStatus::Applying;
        self.arm_wait(now);
        Some(line)
    }

    /// Java `failLoggingRequestAfterSendFailure`: the set line could not be written, so the
    /// request fails closed (unknown) at once.
    pub fn send_failed(&mut self) {
        if let Some((generation, _)) = self.wait {
            self.timeout_if_current(generation);
        }
    }

    /// When [`LoggingControl::tick`] must run next.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.wait.filter(|&(generation, _)| self.wait_is_current(generation)).map(|(_, at)| at)
    }

    /// Settles a wait whose time has come (Java `settleLoggingTimeoutIfCurrent`). Returns whether
    /// the state changed.
    pub fn tick(&mut self, now: Instant) -> bool {
        match self.wait {
            Some((generation, at)) if now >= at => self.timeout_if_current(generation),
            _ => false,
        }
    }

    /// Java `onCapabilityTimeoutIfCurrent`: a missing capability clears success; a missing
    /// acknowledgement keeps the last observed state but becomes unknown, and a late
    /// acknowledgement cannot revive it.
    fn timeout_if_current(&mut self, generation: u64) -> bool {
        if self.disconnected || generation != self.generation {
            self.wait = None;
            return false;
        }
        if !self.contract_launch {
            self.status = LoggingStatus::LegacyUnconfirmed;
            self.clear_observed_success();
            self.invalidate_wait();
            return true;
        }
        match self.status {
            LoggingStatus::Starting => {
                self.status = LoggingStatus::Unknown;
                self.clear_observed_success();
                self.invalidate_wait();
                true
            }
            LoggingStatus::Applying => {
                self.status = LoggingStatus::Unknown;
                self.invalidate_wait();
                true
            }
            _ => {
                self.wait = None;
                false
            }
        }
    }

    /// The helper stopped (Java `disconnectLoggingControl` → `onDisconnect`): unknown, capture
    /// and trace wishes dropped, no wait may fire any more.
    pub fn disconnect(&mut self) {
        self.disconnected = true;
        self.status = if self.contract_launch { LoggingStatus::Unknown } else { LoggingStatus::LegacyUnconfirmed };
        self.desired = Desired::launch_defaults(self.desired.diagnostics);
        self.clear_observed_success();
        self.invalidate_wait();
    }

    /// Java `ReadBoardLoggingSnapshot.from`.
    pub fn snapshot(&self) -> LoggingSnapshot {
        let observed = self.observed.as_ref();
        let toggle = |pick: fn(&Observed) -> Toggle| observed.map_or(Toggle::Unknown, pick);
        let diagnostics = toggle(|o| o.diagnostics);
        let capture = toggle(|o| o.capture);
        let trace = toggle(|o| o.trace);
        let persistence = observed.map(|o| o.persistence);
        let session = self.process_session_id.clone().unwrap_or_default();
        let capture_summary = if session.is_empty() {
            NO_CAPTURE_SESSION.to_string()
        } else {
            format!(
                "processSession={session} desired={} observed={} persistence={}",
                if self.desired.capture { "on" } else { "off" },
                capture.token(),
                persistence.map_or("none", Persistence::token)
            )
        };
        LoggingSnapshot {
            attached: true,
            contract_launch: self.contract_launch,
            status: self.status,
            process_session_id: session,
            capability_known: observed.is_some(),
            desired: self.desired,
            observed_diagnostics: diagnostics,
            observed_capture: capture,
            observed_trace: trace,
            persistence,
            drop_count: observed.map(|o| o.drop_count),
            reason: observed.map(|o| o.reason),
            diagnostics_presentation: presentation(self.status, self.desired.diagnostics, diagnostics, persistence),
            capture_presentation: presentation(self.status, self.desired.capture, capture, persistence),
            trace_presentation: presentation(self.status, self.desired.trace, trace, persistence),
            capture_summary,
            process_session_observed_at: self.process_session_observed_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROCESS_SESSION: &str = "dGVzdFByb2Nlc3NJRA";
    const HOST_SESSION: &str = "dGVzdEhvc3RTZXNzaW9u";
    const CAPABILITY: &str = "readboardLoggingV1 dGVzdFByb2Nlc3NJRA on off off healthy 0";

    fn contract(diagnostics: bool) -> LoggingControl {
        LoggingControl::for_launch(Some(&LoggingLaunch {
            log_dir: "C:\\work\\logs\\readboard".to_string(),
            host_session_id: HOST_SESSION.to_string(),
            diagnostics,
        }))
    }

    fn wall() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000)
    }

    /// The acknowledgement a helper would send for a set line.
    fn observed_for(set_line: &str) -> String {
        let request = parse_set(set_line).unwrap();
        format!(
            "readboardLoggingObserved {} {PROCESS_SESSION} {} {} {} healthy 0 applied",
            request.request_id,
            request.diagnostics.token(),
            request.capture.token(),
            request.trace.token()
        )
    }

    #[test]
    fn capability_set_and_observed_lines_parse_their_fields() {
        let capability = parse_capability(CAPABILITY).unwrap();
        assert_eq!(capability.process_session_id, PROCESS_SESSION);
        assert_eq!(
            (capability.diagnostics, capability.capture, capability.trace),
            (Toggle::On, Toggle::Off, Toggle::Off)
        );
        assert_eq!((capability.persistence, capability.drop_count), (Persistence::Healthy, 0));

        let set_line = "readboardLoggingSet cmVxdWVzdDE on off on";
        let set = parse_set(set_line).unwrap();
        assert_eq!(set.request_id, "cmVxdWVzdDE");
        assert_eq!((set.diagnostics, set.capture, set.trace), (Toggle::On, Toggle::Off, Toggle::On));
        assert_eq!(format_set(&set), set_line);

        let observed = parse_observed(
            "readboardLoggingObserved cmVxdWVzdDE dGVzdFByb2Nlc3NJRA on off on degraded 3 writer-fault",
        )
        .unwrap();
        assert_eq!(observed.request_id, "cmVxdWVzdDE");
        assert_eq!(observed.process_session_id, PROCESS_SESSION);
        assert_eq!(
            (observed.diagnostics, observed.capture, observed.trace),
            (Toggle::On, Toggle::Off, Toggle::On)
        );
        assert_eq!(
            (observed.persistence, observed.drop_count, observed.reason),
            (Persistence::Degraded, 3, Reason::WriterFault)
        );
        assert_eq!(
            parse_observed("readboardLoggingObserved a b unknown off off unavailable 0 path-unavailable")
                .map(|o| (o.diagnostics, o.reason)),
            Some((Toggle::Unknown, Reason::PathUnavailable))
        );
    }

    #[test]
    fn illegal_logging_lines_are_rejected_but_still_control_lines() {
        for line in [
            "readboardLoggingV1 dGVzdFByb2Nlc3NJRA on off off healthy",
            "readboardLoggingV1 dGVzdFByb2Nlc3NJRA unknown off off healthy 0",
            "readboardLoggingSet cmVxdWVzdDE on off",
            "readboardLoggingSet cmVxdWVzdDE unknown off off",
            "readboardLoggingObserved cmVxdWVzdDE dGVzdFByb2Nlc3NJRA on off off healthy 0",
            "readboardLoggingObserved cmVxdWVzdDE dGVzdFByb2Nlc3NJRA on off off healthy 0 java.io.IOException: C:\\x",
            "readboardLoggingObserved cmVxdWVzdDE dGVzdFByb2Nlc3NJRA on off off healthy -1 writer-fault",
        ] {
            assert_eq!(parse_capability(line), None, "{line}");
            assert_eq!(parse_set(line), None, "{line}");
            assert_eq!(parse_observed(line), None, "{line}");
            assert!(is_control_line(line), "{line}");
        }
        assert!(is_control_line(" readboardLoggingV1\r\n"));
        assert!(!is_control_line("ready"));
        assert!(!is_control_line("readboardLoggingV1x a"));
        assert!(!is_control_line(""));
    }

    #[test]
    fn named_arguments_follow_contract_and_force_capture_off() {
        let launch = LoggingLaunch {
            log_dir: "C:\\work\\logs\\readboard".to_string(),
            host_session_id: HOST_SESSION.to_string(),
            diagnostics: true,
        };
        assert_eq!(
            named_launch_arguments(&launch),
            [
                "--log-dir",
                "C:\\work\\logs\\readboard",
                "--host-session-id",
                HOST_SESSION,
                "--logging-contract",
                "1",
                "--diagnostics",
                "on",
                "--capture",
                "off"
            ]
        );
        let off = LoggingLaunch { diagnostics: false, ..launch.clone() };
        let args = named_launch_arguments(&off);
        assert_eq!(args.len(), 10);
        assert_eq!((args[7].as_str(), args[9].as_str()), ("off", "off"));
        assert!(!args.iter().any(|a| a == "--trace"));

        let unc = LoggingLaunch { log_dir: "\\\\server\\share\\logs\\readboard".to_string(), ..launch.clone() };
        assert_eq!(named_launch_arguments(&unc)[1], "\\\\server\\share\\logs\\readboard");
        assert!(LoggingLaunch { log_dir: "d:/logs".to_string(), ..launch.clone() }.is_contract());
        for bad_dir in ["/tmp/logs/readboard", "logs\\readboard", "\\\\server", "\\\\?\\C:\\x", "C:\\\\x", "C:", ""] {
            let bad = LoggingLaunch { log_dir: bad_dir.to_string(), ..launch.clone() };
            assert!(named_launch_arguments(&bad).is_empty(), "{bad_dir}");
            assert!(!bad.is_contract());
        }
        let bad_id = LoggingLaunch { host_session_id: "a b".to_string(), ..launch };
        assert!(named_launch_arguments(&bad_id).is_empty());
    }

    #[test]
    fn readboard_log_directory_is_logs_readboard_child() {
        let logs = Path::new("work").join("logs");
        assert_eq!(readboard_log_directory(&logs), logs.join("readboard").to_string_lossy());
    }

    #[test]
    fn capability_establishes_process_session_and_initial_observed() {
        let mut control = contract(true);
        let t0 = Instant::now();
        control.on_ready(t0);
        assert_eq!(control.status(), LoggingStatus::Starting);
        assert_eq!(control.next_deadline(), Some(t0 + CAPABILITY_TIMEOUT));

        assert!(control.on_line(CAPABILITY, wall()));
        assert_eq!(control.status(), LoggingStatus::CapabilityReady);
        assert_eq!(control.process_session_id(), Some(PROCESS_SESSION));
        let observed = control.observed().unwrap();
        assert_eq!((observed.diagnostics, observed.capture, observed.trace), (Toggle::On, Toggle::Off, Toggle::Off));
        assert_eq!(control.next_deadline(), None);
        // The settled wait does not fire later.
        assert!(!control.tick(t0 + CAPABILITY_TIMEOUT));
        assert_eq!(control.status(), LoggingStatus::CapabilityReady);
        assert_eq!(control.snapshot().process_session_observed_at, Some(wall()));
        // A capability from another process session is ignored.
        assert!(!control.on_line("readboardLoggingV1 other off off off healthy 0", wall()));
        assert_eq!(control.process_session_id(), Some(PROCESS_SESSION));
    }

    #[test]
    fn newest_request_id_wins_and_late_ack_is_ignored() {
        let mut control = contract(false);
        let now = Instant::now();
        control.on_line(CAPABILITY, wall());
        let first = control.request_set(true, false, false, [1; 12], now).unwrap();
        let second = control.request_set(false, false, true, [2; 12], now).unwrap();
        assert_ne!(parse_set(&first).unwrap().request_id, parse_set(&second).unwrap().request_id);

        assert!(!control.on_line(&observed_for(&first), wall()));
        assert_eq!(control.status(), LoggingStatus::Applying);
        assert!(control.on_line(&observed_for(&second), wall()));
        assert_eq!(control.status(), LoggingStatus::Observed);
        let observed = control.observed().unwrap();
        assert_eq!((observed.trace, observed.diagnostics), (Toggle::On, Toggle::Off));
        assert!(!control.desired().capture);
        assert!(control.desired().trace);
    }

    #[test]
    fn request_set_formats_frozen_wire_and_needs_a_process_session() {
        let mut control = contract(false);
        let now = Instant::now();
        assert_eq!(control.request_set(false, true, true, [0; 12], now), None);
        assert_eq!(control.status(), LoggingStatus::Starting);

        control.on_line(CAPABILITY, wall());
        let line = control.request_set(true, false, true, [0xfb; 12], now).unwrap();
        let request = parse_set(&line).unwrap();
        assert!(is_opaque_id(&request.request_id));
        assert_eq!(request.request_id, "-_v7-_v7-_v7-_v7");
        assert_eq!(line, format!("readboardLoggingSet {} on off on", request.request_id));
        assert_eq!(control.next_deadline(), Some(now + CAPABILITY_TIMEOUT));
        let again = control.request_set(true, false, false, [3; 12], now).unwrap();
        assert!(again.ends_with(" on off off"));
    }

    #[test]
    fn timeout_and_disconnect_are_unknown_not_success() {
        let mut control = contract(false);
        let t0 = Instant::now();
        control.on_ready(t0);
        assert!(!control.tick(t0 + CAPABILITY_TIMEOUT - Duration::from_millis(1)));
        assert!(control.tick(t0 + CAPABILITY_TIMEOUT));
        assert_eq!(control.status(), LoggingStatus::Unknown);
        assert!(control.observed().is_none());
        assert_eq!(control.snapshot().diagnostics_presentation, Presentation::Unknown);
        assert_eq!(control.next_deadline(), None);

        let mut after_ready = contract(false);
        after_ready.on_line(CAPABILITY, wall());
        after_ready.disconnect();
        assert_eq!(after_ready.status(), LoggingStatus::Unknown);
        assert!(after_ready.observed().is_none());
        assert_eq!(after_ready.request_set(true, false, false, [0; 12], t0), None);
    }

    #[test]
    fn old_helper_or_no_log_directory_is_legacy_unconfirmed() {
        for launch in [
            None,
            Some(LoggingLaunch {
                log_dir: "/tmp/logs/readboard".to_string(),
                host_session_id: HOST_SESSION.to_string(),
                diagnostics: false,
            }),
        ] {
            let mut control = LoggingControl::for_launch(launch.as_ref());
            assert!(!control.is_contract_launch());
            let t0 = Instant::now();
            control.on_ready(t0);
            assert_eq!(control.next_deadline(), None);
            assert!(!control.tick(t0 + CAPABILITY_TIMEOUT));
            assert_eq!(control.status(), LoggingStatus::LegacyUnconfirmed);
            let snapshot = control.snapshot();
            assert_eq!(snapshot.diagnostics_presentation, Presentation::LegacyUnconfirmed);
            assert_eq!(snapshot.capture_presentation, Presentation::LegacyUnconfirmed);
        }
    }

    #[test]
    fn desired_on_observed_off_is_not_applied_and_degraded_is_not_unknown() {
        let mut control = contract(true);
        control.on_line("readboardLoggingV1 dGVzdFByb2Nlc3NJRA off off off healthy 0", wall());
        assert_eq!(control.snapshot().diagnostics_presentation, Presentation::NotApplied);

        let mut degraded = contract(true);
        degraded.on_line("readboardLoggingV1 dGVzdFByb2Nlc3NJRA on off off degraded 0", wall());
        assert_eq!(degraded.snapshot().diagnostics_presentation, Presentation::OnStorageDegraded);

        let mut unavailable = contract(true);
        unavailable.on_line("readboardLoggingV1 dGVzdFByb2Nlc3NJRA on off off unavailable 0", wall());
        assert_eq!(unavailable.snapshot().diagnostics_presentation, Presentation::OnStorageUnavailable);
    }

    #[test]
    fn snapshot_does_not_report_enabled_before_observed_ack() {
        let mut control = contract(false);
        control.on_line("readboardLoggingV1 dGVzdFByb2Nlc3NJRA off off off healthy 0", wall());
        control.request_set(true, false, false, [0; 12], Instant::now());

        let snapshot = control.snapshot();
        assert!(snapshot.desired.diagnostics);
        assert_eq!(snapshot.observed_diagnostics, Toggle::Off);
        assert_eq!(snapshot.diagnostics_presentation, Presentation::NotApplied);
    }

    #[test]
    fn snapshot_keeps_observed_on_until_disable_ack() {
        let mut control = contract(true);
        control.on_line(CAPABILITY, wall());
        control.request_set(false, false, false, [0; 12], Instant::now());

        let snapshot = control.snapshot();
        assert!(!snapshot.desired.diagnostics);
        assert_eq!(snapshot.observed_diagnostics, Toggle::On);
        assert_eq!(snapshot.diagnostics_presentation, Presentation::On);
    }

    #[test]
    fn snapshot_exposes_capability_persistence_drop_count_and_process_session() {
        let mut control = contract(true);
        control.on_line("readboardLoggingV1 dGVzdFByb2Nlc3NJRA on off off degraded 4", wall());

        let snapshot = control.snapshot();
        assert!(snapshot.attached);
        assert!(snapshot.capability_known);
        assert_eq!(snapshot.process_session_id, PROCESS_SESSION);
        assert_eq!(snapshot.persistence, Some(Persistence::Degraded));
        assert_eq!(snapshot.drop_count, Some(4));
        assert_eq!(snapshot.diagnostics_presentation, Presentation::OnStorageDegraded);
        assert_eq!(snapshot.capture_presentation, Presentation::Off);
        assert_eq!(
            snapshot.capture_summary,
            "processSession=dGVzdFByb2Nlc3NJRA desired=off observed=off persistence=degraded"
        );
    }

    #[test]
    fn snapshot_after_disconnect_is_unknown_and_does_not_keep_capture_desired() {
        let mut control = contract(true);
        control.on_line(CAPABILITY, wall());
        control.request_set(true, true, true, [0; 12], Instant::now());
        control.disconnect();

        let snapshot = control.snapshot();
        assert_eq!(snapshot.status, LoggingStatus::Unknown);
        assert_eq!(snapshot.desired, Desired { diagnostics: true, capture: false, trace: false });
        assert_eq!(snapshot.capture_presentation, Presentation::Unknown);
        assert_eq!(snapshot.diagnostics_presentation, Presentation::Unknown);
        assert_eq!(snapshot.capture_summary, "no capture session");
        assert_eq!(control.next_deadline(), None);
    }

    #[test]
    fn detached_snapshot_is_unknown_not_success() {
        let snapshot = LoggingSnapshot::detached();
        assert!(!snapshot.attached);
        assert!(!snapshot.capability_known);
        assert_eq!(snapshot.diagnostics_presentation, Presentation::Unknown);
        assert_eq!(snapshot.capture_presentation, Presentation::Unknown);
        assert_eq!(snapshot.trace_presentation, Presentation::Unknown);
        assert_eq!(snapshot.capture_summary, "no capture session");
    }

    #[test]
    fn set_acknowledgement_timeout_leaves_unknown_not_success() {
        let mut control = contract(false);
        let t0 = Instant::now();
        control.on_line(CAPABILITY, wall());
        let line = control.request_set(true, false, false, [7; 12], t0).unwrap();
        assert!(control.tick(t0 + CAPABILITY_TIMEOUT));
        assert_eq!(control.status(), LoggingStatus::Unknown);
        assert_eq!(control.snapshot().diagnostics_presentation, Presentation::Unknown);
        // A late acknowledgement cannot revive success.
        assert!(!control.on_line(&observed_for(&line), wall()));
        assert_eq!(control.status(), LoggingStatus::Unknown);
    }

    #[test]
    fn acknowledgement_and_timeout_settle_only_their_exact_generation() {
        let mut control = contract(false);
        let t0 = Instant::now();
        control.on_line(CAPABILITY, wall());

        let first = control.request_set(true, false, false, [1; 12], t0).unwrap();
        assert!(control.on_line(&observed_for(&first), wall()));
        // The first request's timeout passes after its acknowledgement: nothing changes.
        assert!(!control.tick(t0 + CAPABILITY_TIMEOUT));
        assert_eq!(control.status(), LoggingStatus::Observed);

        let t1 = t0 + Duration::from_secs(10);
        let second = control.request_set(false, false, true, [2; 12], t1).unwrap();
        assert!(control.tick(t1 + CAPABILITY_TIMEOUT));
        assert_eq!(control.status(), LoggingStatus::Unknown);
        assert!(!control.on_line(&observed_for(&second), wall()));
        assert_eq!(control.status(), LoggingStatus::Unknown);
    }

    #[test]
    fn disconnect_during_request_cannot_arm_or_revive_the_old_wait() {
        let mut control = contract(false);
        let t0 = Instant::now();
        control.on_line(CAPABILITY, wall());
        let line = control.request_set(true, false, false, [1; 12], t0).unwrap();
        control.disconnect();

        assert_eq!(control.next_deadline(), None);
        assert!(!control.tick(t0 + CAPABILITY_TIMEOUT));
        assert!(!control.on_line(&observed_for(&line), wall()));
        assert_eq!(control.status(), LoggingStatus::Unknown);
        assert_eq!(control.process_session_id(), None);
    }

    #[test]
    fn send_failure_fails_applying_state_closed() {
        let mut control = contract(false);
        control.on_line(CAPABILITY, wall());
        control.request_set(true, false, false, [1; 12], Instant::now()).unwrap();
        control.send_failed();
        assert_eq!(control.status(), LoggingStatus::Unknown);
        assert_eq!(control.next_deadline(), None);
        assert_eq!(control.snapshot().diagnostics_presentation, Presentation::Unknown);
    }

    #[test]
    fn observed_line_without_a_request_is_ignored() {
        let mut control = contract(false);
        assert!(!control.on_line(
            "readboardLoggingObserved cmVxdWVzdDE dGVzdFByb2Nlc3NJRA on off on degraded 3 writer-fault",
            wall()
        ));
        assert!(!control.on_line("readboardLoggingV1 broken", wall()));
        assert_eq!(control.status(), LoggingStatus::Starting);
    }
}

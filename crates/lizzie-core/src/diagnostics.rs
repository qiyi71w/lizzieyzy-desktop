//! 帮助 › 诊断与日志: the diagnostic switches the configuration keeps (Java `LoggingSettings`,
//! `DiagnosticModule`, `TraceScope`) and the 完整日志 session (Java `LoggingRuntime`).
//!
//! One [`Diagnostics`] is shared by everything that records: the engines, the 棋盘同步 flow and
//! the network adapters. A module's diagnostic records reach the application log only while
//! 诊断记录 and that module are on; a full-log file is written only while a session started for
//! its scope runs. Every line passes the credential sanitizer before it is written.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::sanitize::sanitize;

/// A diagnostics module (Java `DiagnosticModule`), stored by its Java wire name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum DiagnosticModule {
    #[serde(rename = "engine")]
    Engine,
    #[serde(rename = "gtp-summary")]
    GtpSummary,
    #[serde(rename = "readboard-yike")]
    ReadBoardYike,
    #[serde(rename = "network-remote")]
    NetworkRemote,
}

impl DiagnosticModule {
    pub const ALL: [DiagnosticModule; 4] = [Self::Engine, Self::GtpSummary, Self::ReadBoardYike, Self::NetworkRemote];

    /// Java `DiagnosticModule.fromWireName`.
    pub fn from_wire_name(name: &str) -> Option<DiagnosticModule> {
        Self::ALL.into_iter().find(|m| m.wire_name() == name)
    }

    pub fn wire_name(self) -> &'static str {
        match self {
            Self::Engine => "engine",
            Self::GtpSummary => "gtp-summary",
            Self::ReadBoardYike => "readboard-yike",
            Self::NetworkRemote => "network-remote",
        }
    }
}

/// A 完整日志 scope (Java `TraceScope`), stored by its Java wire name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum TraceScope {
    #[serde(rename = "engine-gtp")]
    EngineGtp,
    #[serde(rename = "readboard-yike")]
    ReadBoardYike,
    #[serde(rename = "network-websocket")]
    NetworkWebSocket,
}

impl TraceScope {
    pub const ALL: [TraceScope; 3] = [Self::EngineGtp, Self::ReadBoardYike, Self::NetworkWebSocket];

    /// Java `TraceScope.fromWireName`.
    pub fn from_wire_name(name: &str) -> Option<TraceScope> {
        Self::ALL.into_iter().find(|s| s.wire_name() == name)
    }

    pub fn wire_name(self) -> &'static str {
        match self {
            Self::EngineGtp => "engine-gtp",
            Self::ReadBoardYike => "readboard-yike",
            Self::NetworkWebSocket => "network-websocket",
        }
    }

    /// The file in the log folder the scope's full log is written to.
    pub fn file_name(self) -> &'static str {
        match self {
            Self::EngineGtp => "engine-trace.log",
            Self::ReadBoardYike => "readboard-trace.log",
            Self::NetworkWebSocket => "network-trace.log",
        }
    }
}

/// The configuration's `logging` section (Java `LoggingSettings`; all on by default). The
/// preferred scopes are only what the next 完整日志 session starts with: they never start one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct LoggingSettings {
    /// 诊断记录 (`diagnostics-enabled`).
    pub diagnostics_enabled: bool,
    /// The modules whose diagnostic records are written (`diagnostic-modules`).
    pub diagnostic_modules: Vec<DiagnosticModule>,
    /// 完整日志's scope choice for the next session (`preferred-full-trace-scopes`).
    pub preferred_full_trace_scopes: Vec<TraceScope>,
}

impl Default for LoggingSettings {
    fn default() -> Self {
        LoggingSettings {
            diagnostics_enabled: true,
            diagnostic_modules: DiagnosticModule::ALL.to_vec(),
            preferred_full_trace_scopes: TraceScope::ALL.to_vec(),
        }
    }
}

impl LoggingSettings {
    /// The same choices once each, in Java's enum order (an `EnumSet`).
    pub fn normalized(mut self) -> LoggingSettings {
        self.diagnostic_modules = DiagnosticModule::ALL.into_iter().filter(|m| self.diagnostic_modules.contains(m)).collect();
        self.preferred_full_trace_scopes = scopes_in_order(&self.preferred_full_trace_scopes);
        self
    }

    /// The module's diagnostic records are written (Java: its logger is at DEBUG).
    pub fn records(&self, module: DiagnosticModule) -> bool {
        self.diagnostics_enabled && self.diagnostic_modules.contains(&module)
    }
}

fn scopes_in_order(scopes: &[TraceScope]) -> Vec<TraceScope> {
    TraceScope::ALL.into_iter().filter(|s| scopes.contains(s)).collect()
}

/// Severity of an application-log line written for a diagnostic record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Warn,
}

/// Where application-log lines go (the adapter's `app.log` writer). Lines are sanitized.
pub type LogSink = Arc<dyn Fn(LogLevel, &str) + Send + Sync>;

/// Java `NetworkEndpointCategory`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndpointCategory {
    Protocol,
    Authentication,
    Account,
    Payment,
    Credential,
    Other,
}

impl EndpointCategory {
    fn wire_name(self) -> &'static str {
        match self {
            Self::Protocol => "protocol",
            Self::Authentication => "authentication",
            Self::Account => "account",
            Self::Payment => "payment",
            Self::Credential => "credential",
            Self::Other => "other",
        }
    }

    /// Java `prohibitsBodies`: these exchanges are never written in full.
    fn prohibits_bodies(self) -> bool {
        matches!(self, Self::Authentication | Self::Account | Self::Payment | Self::Credential)
    }
}

/// One network exchange (Java `NetworkObservation.recordNetwork` / `recordRemote`).
#[derive(Clone, Debug)]
pub struct NetworkExchange<'a> {
    /// The cloud account service (`remote`) rather than other network use (`network`).
    pub remote: bool,
    pub method: &'a str,
    /// A host or an address; only its host is written.
    pub host: &'a str,
    pub category: EndpointCategory,
    pub status: Option<u16>,
    pub latency_ms: u64,
    /// `ok`, `failed`, `error` or another short word.
    pub outcome: &'a str,
}

/// Java `ObservationText.RAW_EVENT_MAX_UTF8_BYTES`.
const RAW_EVENT_MAX_BYTES: usize = 16 * 1024;

#[derive(Default)]
struct Inner {
    logs_dir: Option<PathBuf>,
    settings: RwLock<LoggingSettings>,
    session: Mutex<Option<Session>>,
    sink: RwLock<Option<LogSink>>,
}

struct Session {
    files: Vec<(TraceScope, File)>,
}

/// The diagnostics switches in effect and the running 完整日志 session; clones share both.
#[derive(Clone, Default)]
pub struct Diagnostics(Arc<Inner>);

fn millis(at: SystemTime) -> u128 {
    at.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis())
}

/// The sanitized text, at most [`RAW_EVENT_MAX_BYTES`] (Java `boundedRawEvent`).
fn bounded(text: &str) -> String {
    let mut safe = sanitize(text);
    if safe.len() > RAW_EVENT_MAX_BYTES {
        let mut end = RAW_EVENT_MAX_BYTES;
        while !safe.is_char_boundary(end) {
            end -= 1;
        }
        safe.truncate(end);
        safe.push_str(" [truncated]");
    }
    safe
}

/// Java `NetworkObservation.safeHost`: the host of an address, never its path or query.
fn safe_host(host: &str) -> String {
    if host.is_empty() {
        return "unknown".into();
    }
    if !host.contains("://") && !host.contains('/') && !host.contains('?') {
        return host.into();
    }
    let address = if host.contains("://") { host.to_string() } else { format!("https://{host}") };
    url::Url::parse(&address)
        .ok()
        .and_then(|u| u.host_str().filter(|h| !h.is_empty()).map(str::to_string))
        .unwrap_or_else(|| "unknown".into())
}

impl Diagnostics {
    /// Full logs go to `logs_dir`; with the Java defaults until [`Diagnostics::apply`].
    pub fn new(logs_dir: PathBuf) -> Diagnostics {
        Diagnostics(Arc::new(Inner { logs_dir: Some(logs_dir), ..Inner::default() }))
    }

    /// Sends application-log lines to `sink`.
    pub fn set_sink(&self, sink: LogSink) {
        *self.0.sink.write().unwrap() = Some(sink);
    }

    /// The saved switches take effect at once (Java `applySettings`); a running session keeps
    /// its scopes.
    pub fn apply(&self, settings: &LoggingSettings) {
        *self.0.settings.write().unwrap() = settings.clone();
    }

    pub fn records(&self, module: DiagnosticModule) -> bool {
        self.0.settings.read().unwrap().records(module)
    }

    fn write(&self, level: LogLevel, text: &str) {
        let sink = self.0.sink.read().unwrap().clone();
        if let Some(sink) = sink {
            sink(level, &sanitize(text));
        }
    }

    /// A diagnostic record of `module`, built only when the module records.
    pub fn debug(&self, module: DiagnosticModule, text: impl FnOnce() -> String) {
        if self.records(module) {
            self.write(LogLevel::Debug, &text());
        }
    }

    /// Java `NetworkObservation.record`: a failure is always written as a warning, any other
    /// outcome only while 网络/远程 records.
    pub fn network(&self, e: &NetworkExchange) {
        let outcome = if e.outcome.is_empty() { "unknown" } else { e.outcome };
        let failed = matches!(outcome, "failed" | "error");
        if !failed && !self.records(DiagnosticModule::NetworkRemote) {
            return;
        }
        let line = format!(
            "{} event=http method={} host={} category={} status={} latencyMs={} outcome={outcome}",
            if e.remote { "remote" } else { "network" },
            if e.method.is_empty() { "UNKNOWN" } else { e.method },
            safe_host(e.host),
            e.category.wire_name(),
            e.status.map_or_else(|| "-".to_string(), |s| s.to_string()),
            e.latency_ms,
        );
        self.write(if failed { LogLevel::Warn } else { LogLevel::Debug }, &line);
    }

    /// Starts a 完整日志 session for `scopes` (all of them when none is given, as the dialog
    /// does), ending a running one first. Each scope's file is appended to.
    pub fn start_full_trace(&self, scopes: &[TraceScope]) -> std::io::Result<()> {
        let dir = self.0.logs_dir.clone().ok_or_else(|| std::io::Error::other("no log folder"))?;
        let scopes = if scopes.is_empty() { TraceScope::ALL.to_vec() } else { scopes_in_order(scopes) };
        std::fs::create_dir_all(&dir)?;
        let mut files = Vec::new();
        for scope in scopes {
            let file = OpenOptions::new().create(true).append(true).open(dir.join(scope.file_name()))?;
            files.push((scope, file));
        }
        self.stop_full_trace();
        let now = SystemTime::now();
        for (scope, file) in &mut files {
            let _ = writeln!(file, "{} full trace session started scope={}", millis(now), scope.wire_name());
        }
        *self.0.session.lock().unwrap() = Some(Session { files });
        Ok(())
    }

    /// Ends the running session; `false` when none ran.
    pub fn stop_full_trace(&self) -> bool {
        let Some(mut session) = self.0.session.lock().unwrap().take() else {
            return false;
        };
        let now = millis(SystemTime::now());
        for (scope, file) in &mut session.files {
            let _ = writeln!(file, "{now} full trace session stopped scope={}", scope.wire_name());
            let _ = file.flush();
        }
        true
    }

    /// The running session's scopes; empty when none runs.
    pub fn full_trace(&self) -> Vec<TraceScope> {
        self.0.session.lock().unwrap().as_ref().map(|s| s.files.iter().map(|(scope, _)| *scope).collect()).unwrap_or_default()
    }

    /// One line of `scope`'s full log, built only while the session records it.
    pub fn trace(&self, scope: TraceScope, text: impl FnOnce() -> String) {
        let mut session = self.0.session.lock().unwrap();
        let Some(file) = session.as_mut().and_then(|s| s.files.iter_mut().find(|(sc, _)| *sc == scope)).map(|(_, f)| f) else {
            return;
        };
        let _ = writeln!(file, "{} {}", millis(SystemTime::now()), bounded(&text()));
        let _ = file.flush();
    }

    /// Java `NetworkObservation.tracePayload`: a WebSocket payload of the network scope.
    pub fn trace_payload(&self, category: EndpointCategory, direction: &str, payload: &str) {
        if category.prohibits_bodies() {
            return;
        }
        self.trace(TraceScope::NetworkWebSocket, || format!("network raw direction={direction} payload={payload}"));
    }
}

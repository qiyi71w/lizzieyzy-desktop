//! 棋盘同步 diagnostics for the diagnostics package (Java `SyncDiagnosticsRecorder`,
//! `SyncDiagnosticsExporter.writeSnapshotEntries` and `DiagnosticBundleExporter.renderObserved`):
//! ring buffers of the recent helper protocol lines ([`PROTOCOL_CAPACITY`]) and sync decisions
//! ([`DECISION_CAPACITY`]), and the `snapshots/` entries rendered from them.
//!
//! A protocol line is summarised when it is recorded (Java `summarizeProtocolLine`): commands
//! that carry a room, a title, a record or a file keep only their name, as does any line longer
//! than 80 characters, so those payloads never reach the buffer. This client has no 弈客
//! integration, so its two entries hold no session and no events.

use std::collections::VecDeque;
use std::time::SystemTime;

use serde_json::{json, Value};

use super::logging::{LoggingSnapshot, LoggingStatus, Persistence, Reason, Toggle};
use super::protocol::{parse_line, Inbound};
use super::session::{RecoveryOutcome, SyncDecision};

/// Java `SyncDiagnosticsRecorder.PROTOCOL_EVENT_CAPACITY`.
pub const PROTOCOL_CAPACITY: usize = 100;
/// Java `SyncDiagnosticsRecorder.DECISION_TRACE_CAPACITY`.
pub const DECISION_CAPACITY: usize = 50;

/// The `snapshots/` entries [`SyncDiagnostics::snapshot_entries`] writes, in Java's order.
pub const SNAPSHOT_ENTRIES: [&str; 7] = [
    "summary.txt",
    "sync-context.json",
    "yike-session.json",
    "recent-decisions.jsonl",
    "readboard-protocol.log",
    "yike-events.jsonl",
    "environment.txt",
];

#[derive(Clone, Debug, PartialEq)]
struct ProtocolEvent {
    at_millis: u64,
    summary: String,
}

#[derive(Clone, Debug, PartialEq)]
struct DecisionTrace {
    at_millis: u64,
    outcome: RecoveryOutcome,
    reason: &'static str,
}

/// What the workbench knows about the sync when the package is made (Java
/// `SyncDiagnosticsSnapshot`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SyncState {
    /// A helper of this launch exists. It is dropped when its output ends, so it is also
    /// connected (Java `readBoardConnected`).
    pub attached: bool,
    pub syncing: bool,
    pub awaiting_first_sync_frame: bool,
    /// The session holds the node the last accepted frame resolved to (Java resume state).
    pub has_resolved_node: bool,
}

/// The 棋盘同步 part of the diagnostics package.
#[derive(Clone, Debug, PartialEq)]
pub struct ReadBoardDiagnostics {
    /// The version the last helper reported, for `snapshots/versions.json`.
    pub helper_version: Option<String>,
    /// The [`SNAPSHOT_ENTRIES`] with their text.
    pub snapshots: Vec<(&'static str, String)>,
    /// `snapshots/readboard-observed.json` ([`observed_json`]).
    pub observed: String,
}

/// The recent protocol lines and decisions of every helper of this run (Java's recorder is one
/// per application).
#[derive(Clone, Debug, Default)]
pub struct SyncDiagnostics {
    protocol: VecDeque<ProtocolEvent>,
    decisions: VecDeque<DecisionTrace>,
    helper_version: Option<String>,
}

fn millis(at: SystemTime) -> u64 {
    at.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn push<T>(buffer: &mut VecDeque<T>, capacity: usize, value: T) {
    if buffer.len() == capacity {
        buffer.pop_front();
    }
    buffer.push_back(value);
}

/// Java `ReadBoard.summarizeProtocolLine`.
fn summarize_line(line: &str) -> String {
    let trimmed = line.trim_matches(|c: char| c <= ' ');
    if trimmed.is_empty() {
        return "empty".into();
    }
    let command = match trimmed.find(char::is_whitespace) {
        Some(end) => &trimmed[..end],
        None if trimmed.chars().count() > 40 => "<payload>",
        None => trimmed,
    };
    let sensitive = ["roomtoken", "livetitle", "sgf", "loadsgf", "recordtitlefingerprint", "readboardupdateready"]
        .contains(&command.to_lowercase().as_str());
    if sensitive || trimmed.chars().count() > 80 {
        format!("{command} <redacted>")
    } else {
        trimmed.to_string()
    }
}

/// Java `CompleteSnapshotRecoveryOutcome.name()`.
pub(crate) fn outcome_name(outcome: RecoveryOutcome) -> &'static str {
    match outcome {
        RecoveryOutcome::NoChange => "NO_CHANGE",
        RecoveryOutcome::SingleMoveRecovery => "SINGLE_MOVE_RECOVERY",
        RecoveryOutcome::Hold => "HOLD",
        RecoveryOutcome::ForceRebuild => "FORCE_REBUILD",
    }
}

impl SyncDiagnostics {
    /// A line of the current helper (Java `recordProtocolLine`, every line `parseLine` sees);
    /// returns its recorded summary (sensitive and long lines redacted).
    pub fn record_line(&mut self, line: &str, at: SystemTime) -> &str {
        // The version the session reads (`version: 220430` or `version 220430`).
        if let Inbound::Version { number: Some(version) } = parse_line(line) {
            self.helper_version = Some(version.to_string());
        }
        push(&mut self.protocol, PROTOCOL_CAPACITY, ProtocolEvent { at_millis: millis(at), summary: summarize_line(line) });
        &self.protocol.back().expect("just recorded").summary
    }

    /// A recovery decision of the session (Java `publishCompleteSnapshotRecoveryDiagnostics`).
    pub fn record_decision(&mut self, decision: SyncDecision, at: SystemTime) {
        let trace = DecisionTrace { at_millis: millis(at), outcome: decision.outcome, reason: decision.reason };
        push(&mut self.decisions, DECISION_CAPACITY, trace);
    }

    /// The version the last helper reported (`version: <n>`).
    pub fn helper_version(&self) -> Option<&str> {
        self.helper_version.as_deref()
    }

    /// The seven `snapshots/` entries of Java `writeSnapshotEntries` ([`SNAPSHOT_ENTRIES`]),
    /// without the `snapshots/` prefix.
    pub fn snapshot_entries(&self, state: SyncState, app_version: &str, captured_at: SystemTime) -> Vec<(&'static str, String)> {
        let now = millis(captured_at);
        let last_line = self.protocol.back();
        let last_line_text = last_line.map_or("none", |e| e.summary.as_str());
        let last_line_at = last_line.map_or(0, |e| e.at_millis);
        let latest = self.decisions.back();

        let mut summary = String::from("Sync Diagnostics Export\n");
        summary += &format!("capturedAtMillis: {now}\nsource: ReadBoard\n\nReadBoard\n");
        summary += &format!("  attached: {0}\n  connected: {0}\n  usePipe: true\n", state.attached);
        summary += &format!("  syncing: {}\n  awaitingFirstSyncFrame: {}\n", state.syncing, state.awaiting_first_sync_frame);
        summary += &format!(
            "  hasResumeState: {0}\n  hasLastResolvedSnapshotNode: {0}\n",
            state.has_resolved_node
        );
        summary += &format!("  lastProtocolLine: {last_line_text}\n  lastProtocolTimestampMillis: {last_line_at}\n");
        summary += "\nYike\n  available: false\n\nLatest decision\n";
        summary += &format!(
            "  result: {}\n  reason: {}\n",
            latest.map_or("none", |d| outcome_name(d.outcome)),
            latest.map_or("none", |d| d.reason)
        );
        summary += &format!(
            "\nCounts\n  recentProtocolEvents: {}\n  recentDecisionTraces: {}\n  recentYikeEvents: 0\n  exportTimestampMillis: {now}\n",
            self.protocol.len(),
            self.decisions.len()
        );

        let context = json!({
            "readBoardAttached": state.attached,
            "readBoardConnected": state.attached,
            "usePipe": true,
            "syncing": state.syncing,
            "awaitingFirstSyncFrame": state.awaiting_first_sync_frame,
            "hasResumeState": state.has_resolved_node,
            "hasLastResolvedSnapshotNode": state.has_resolved_node,
            "lastProtocolLineSummary": last_line_text,
            "lastProtocolTimestampMillis": last_line_at,
            "timestampMillis": now,
            "source": "ReadBoard",
            "summary": "readboard diagnostics snapshot",
        });
        let yike = json!({ "listenerEnabled": Value::Null, "summary": "no Yike session" });
        let decisions: String = self
            .decisions
            .iter()
            .map(|d| {
                let line = json!({
                    "timestampMillis": d.at_millis,
                    "result": outcome_name(d.outcome),
                    "reasonCode": d.reason,
                    "source": "ReadBoard.syncBoardStones",
                });
                format!("{line}\n")
            })
            .collect();
        let protocol: String = self.protocol.iter().map(|e| format!("{}\t{}\n", e.at_millis, e.summary)).collect();
        let environment = format!(
            "appVersion: {app_version}\nosName: {}\nosArch: {}\ntimestampMillis: {now}\n",
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        vec![
            (SNAPSHOT_ENTRIES[0], summary),
            (SNAPSHOT_ENTRIES[1], context.to_string()),
            (SNAPSHOT_ENTRIES[2], yike.to_string()),
            (SNAPSHOT_ENTRIES[3], decisions),
            (SNAPSHOT_ENTRIES[4], protocol),
            (SNAPSHOT_ENTRIES[5], String::new()),
            (SNAPSHOT_ENTRIES[6], environment),
        ]
    }
}

fn toggle_token(t: Toggle) -> &'static str {
    match t {
        Toggle::Off => "off",
        Toggle::On => "on",
        Toggle::Unknown => "unknown",
    }
}

/// `snapshots/readboard-observed.json` (Java `renderObserved`): the helper's logging state; its
/// process session id is replaced by an alias.
pub fn observed_json(s: &LoggingSnapshot) -> String {
    let status = match s.status {
        LoggingStatus::Starting => "STARTING",
        LoggingStatus::CapabilityReady => "CAPABILITY_READY",
        LoggingStatus::Applying => "APPLYING",
        LoggingStatus::Observed => "OBSERVED",
        LoggingStatus::Unknown => "UNKNOWN",
        LoggingStatus::LegacyUnconfirmed => "LEGACY_UNCONFIRMED",
    };
    let persistence = s.persistence.map(|p| match p {
        Persistence::Healthy => "healthy",
        Persistence::Degraded => "degraded",
        Persistence::Unavailable => "unavailable",
    });
    let reason = s.reason.map(|r| match r {
        Reason::Applied => "applied",
        Reason::LegacyHelper => "legacy-helper",
        Reason::CapabilityTimeout => "capability-timeout",
        Reason::PathUnavailable => "path-unavailable",
        Reason::WriterFault => "writer-fault",
        Reason::InvalidRequest => "invalid-request",
    });
    let session = (!s.process_session_id.is_empty()).then_some("session-1");
    // `captureSummary` names the process session too.
    let capture_summary = if s.process_session_id.is_empty() {
        s.capture_summary.clone()
    } else {
        s.capture_summary.replace(&s.process_session_id, "session-1")
    };
    let value = json!({
        "attached": s.attached,
        "contractLaunch": s.contract_launch,
        "status": status,
        "processSessionId": session,
        "capabilityKnown": s.capability_known,
        "diagnosticsDesired": s.desired.diagnostics,
        "captureDesired": s.desired.capture,
        "traceDesired": s.desired.trace,
        "observedDiagnostics": toggle_token(s.observed_diagnostics),
        "observedCapture": toggle_token(s.observed_capture),
        "observedTrace": toggle_token(s.observed_trace),
        "persistence": persistence,
        "dropCount": s.drop_count,
        "reason": reason,
        "captureSummary": capture_summary,
    });
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(ms: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_millis(ms)
    }

    fn entry<'a>(entries: &'a [(&'static str, String)], name: &str) -> &'a str {
        &entries.iter().find(|(n, _)| *n == name).expect(name).1
    }

    #[test]
    fn the_buffers_keep_the_latest_hundred_lines_and_fifty_decisions() {
        let mut d = SyncDiagnostics::default();
        for i in 0..130 {
            d.record_line(&format!("line{i}"), at(i));
        }
        for i in 0..70 {
            d.record_decision(SyncDecision { outcome: RecoveryOutcome::Hold, reason: "held" }, at(i));
        }
        let entries = d.snapshot_entries(SyncState::default(), "1.0", at(1000));
        assert_eq!(entries.iter().map(|(n, _)| *n).collect::<Vec<_>>(), SNAPSHOT_ENTRIES);
        let protocol = entry(&entries, "readboard-protocol.log");
        assert_eq!(protocol.lines().count(), PROTOCOL_CAPACITY);
        assert_eq!(protocol.lines().next(), Some("30\tline30"));
        assert_eq!(protocol.lines().last(), Some("129\tline129"));
        let decisions = entry(&entries, "recent-decisions.jsonl");
        assert_eq!(decisions.lines().count(), DECISION_CAPACITY);
        for line in decisions.lines() {
            let v: Value = serde_json::from_str(line).unwrap();
            assert_eq!((v["result"].as_str(), v["reasonCode"].as_str()), (Some("HOLD"), Some("held")));
        }
        assert!(entry(&entries, "summary.txt").contains("recentProtocolEvents: 100\n  recentDecisionTraces: 50\n"));
    }

    #[test]
    fn payload_lines_keep_only_their_command() {
        let mut d = SyncDiagnostics::default();
        d.record_line("roomToken secret-room", at(1));
        d.record_line("readboardUpdateReady\tv1.2.3\tC:\\Users\\me\\update.zip", at(2));
        d.record_line(&format!("place {}", "9 ".repeat(50)), at(3));
        d.record_line("version 230616", at(4));
        d.record_line("re=0,0,0", at(5));
        let entries = d.snapshot_entries(SyncState::default(), "1.0", at(10));
        let protocol: Vec<&str> = entry(&entries, "readboard-protocol.log").lines().collect();
        assert_eq!(
            protocol,
            [
                "1\troomToken <redacted>",
                "2\treadboardUpdateReady <redacted>",
                "3\tplace <redacted>",
                "4\tversion 230616",
                "5\tre=0,0,0"
            ]
        );
        assert_eq!(d.helper_version(), Some("230616"));
        // The helper's real answer has a colon (`version: 220430`).
        d.record_line("version: 220430", at(6));
        assert_eq!(d.helper_version(), Some("220430"));
    }

    #[test]
    fn the_observed_state_hides_the_process_session() {
        let mut s = LoggingSnapshot::detached();
        s.process_session_id = "abc123".into();
        s.capture_summary = "processSession=abc123 desired=off observed=unknown persistence=none".into();
        let text = observed_json(&s);
        assert!(!text.contains("abc123"), "{text}");
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["processSessionId"], "session-1");
        assert_eq!(v["status"], "UNKNOWN");
        assert_eq!(v["observedCapture"], "unknown");
    }
}

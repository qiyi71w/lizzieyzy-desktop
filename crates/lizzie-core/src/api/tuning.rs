//! Engine performance operations. Engine ids are saved profile ids, never list indices.
use crate::engine::tuning::{GtpConfiguration, ThreadPolicy, ThreadSource};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum TuningCommand {
    /// Refresh the profile summaries without starting work.
    Refresh,
    /// Advanced settings opt-in; false by default. Never starts work immediately on toggle.
    SetStartupBenchmark { enabled: bool },
    /// Explicit official optimization; experimental=true requires Apple Silicon confirmation in UI.
    StartBenchmark { engine_id: String, experimental: bool },
    /// Cancel the current benchmark; the shell kills its process before completion.
    StopBenchmark,
    /// Closing the automatic notice cancels that task and remembers dismissal for its environment.
    DismissStartupNotice { task_id: u64 },
    /// Select CFG or BENCHMARK (BENCHMARK requires a stored positive recommendation).
    SetThreadSource { engine_id: String, source: ThreadSource },
    /// Choose a measured report when path is null; validate and return evidence for explicit review.
    ImportMeasured { engine_id: String, path: Option<String> },
    /// Apply exactly the reviewed report after a fresh fingerprint check; stale reviews are refused.
    ConfirmMeasured { engine_id: String, review_id: u64 },
    /// Remove measured scene overlays without modifying the command or benchmark policy.
    RestoreMeasured { engine_id: String },
    /// Probe an isolated engine for the zengtp-config v1 schema.
    InspectGtpConfiguration { engine_id: String },
    /// Validate, set and save the engine-owned profile using the probed schema.
    SaveGtpConfiguration {
        engine_id: String,
        configuration: GtpConfiguration,
    },
    /// Scan helper chooser; engine_type is KataGo, LeelaZero, ikatago or other.
    Scan { engine_type: String },
    /// ikatago Scan prompts for platform/account in UI, then chooses its executable in the shell.
    ScanIkatago {
        platform: String,
        user_name: String,
        password: String,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum TuningResult {
    /// Synchronous operation finished; updated state is published as a view.
    Done,
    /// Benchmark was accepted and now runs in the background.
    Started { task_id: u64 },
    /// A chooser was dismissed; no state changed.
    Cancelled,
    /// Probe outcome. schema=null means this engine does not support visual configuration.
    GtpSchema {
        engine_id: String,
        #[ts(type = "Record<string, unknown> | null")]
        schema: Option<Value>,
    },
    /// A Scan command to place in the uncommitted MoreEngines form.
    Scanned { command: String },
    /// Verified report, not yet applied. Contains scene, alternating runs, baseline/candidate parameters and fingerprint.
    MeasuredReview {
        engine_id: String,
        review_id: u64,
        #[ts(type = "Record<string, unknown>")]
        report: Value,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TuningProfileView {
    /// Stable saved engine identity.
    pub engine_id: String,
    /// Saved display name.
    pub name: String,
    /// Only local KataGo profiles can use thread tuning.
    pub eligible: bool,
    /// Full saved policy, including benchmark metadata and measured scene presence.
    pub policy: ThreadPolicy,
    /// Java message key when the environment changed or cannot be fingerprinted.
    pub environment_status: Option<String>,
    /// Resolved executable / configuration / network metadata for the performance card.
    pub engine: String,
    /// Resolved GTP configuration path.
    pub config: String,
    /// Resolved neural network path.
    pub model: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkTaskView {
    /// Workbench-assigned identity used to reject stale progress and completion.
    pub id: u64,
    /// Profile captured when the run started.
    pub engine_id: String,
    /// Whether the experimental Apple topology search was explicitly requested.
    pub experimental: bool,
    /// Automatic opt-in startup run; display the non-modal Java benchmark notice.
    pub automatic: bool,
    /// Progress in thousandths, from real KataGo positions output.
    pub permille: u16,
    /// Recent sanitized process status (not analysis output).
    pub status_lines: Vec<String>,
    /// Stop requested; remains active until the process has been reaped.
    pub cancelling: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TuningOutcome {
    /// Completed task identity.
    pub task_id: u64,
    /// Profile targeted by the task.
    pub engine_id: String,
    /// Java message key for successful, cancelled or failed outcome.
    pub key: String,
    /// Sanitized explanatory detail.
    pub detail: String,
    /// True only when a valid result was committed.
    pub success: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TuningView {
    /// Saved engine policies, including profiles without a result.
    pub profiles: Vec<TuningProfileView>,
    /// At most one active benchmark.
    pub task: Option<BenchmarkTaskView>,
    /// Automatic run notice (same task identity/progress); closing it cancels, it never hides work.
    pub startup_notice: Option<BenchmarkTaskView>,
    /// Terminal outcome sent once, not replayed by unrelated updates.
    pub outcome: Option<TuningOutcome>,
    /// Enables the experimental button only on native macOS arm64.
    pub apple_silicon: bool,
}

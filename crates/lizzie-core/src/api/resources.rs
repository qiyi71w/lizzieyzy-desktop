//! Confirmed engine-resource operations. Detection never downloads or changes a profile.
use crate::{resources::*, settings::NetworkSettings};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A user-selected acquisition, never an arbitrary download URL.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum ResourceItem {
    /// Trusted engine archive.
    Package {
        /// Catalog asset identifier.
        asset: String,
        /// Explicit experimental-install confirmation.
        confirmed: bool,
        /// Optional experimental device variant.
        backend: Option<ExperimentalBackend>,
    },
    /// Pinned model.
    Model {
        /// Catalog model identifier.
        model: String,
    },
    /// A network from the last successful official refresh.
    OnlineWeight {
        /// Network name, not a URL.
        name: String,
    },
    /// Windows-only NVIDIA redistributables.
    NvidiaRuntime {
        /// `cuda` or `tensorrt`.
        backend: String,
        /// Explicit runtime-install confirmation.
        confirmed: bool,
    },
}
/// Engine and supporting files chosen by the user; absent paths invoke native choosers.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LocalEngineChoice {
    /// Executable path; None opens the executable chooser.
    pub executable: Option<String>,
    /// GTP config; discovered beside the executable before asking.
    pub config: Option<String>,
    /// Model; discovered beside the executable before asking.
    pub weight: Option<String>,
}
/// Verified executable to use for planning or explicit verification.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ResourceEngineChoice {
    /// Installed asset identity, or `local` for an explicitly chosen executable.
    pub asset_id: String,
    /// Absolute executable path.
    pub executable: String,
    /// GTP config path.
    pub config: String,
    /// Experimental device command overrides.
    pub backend: Option<ExperimentalBackend>,
    /// Managed profile to update; None adds a new profile.
    pub update_profile_id: Option<String>,
}
/// Workbench commands; destructive/acquisition actions originate from explicit user clicks.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum ResourceCommand {
    /// Detect facts and read the bounded resource inventory.
    Inspect {
        /// Re-probe even when cached.
        refresh: bool,
    },
    /// Download and install the selected resource.
    Download {
        /// Catalog or cached item.
        item: ResourceItem,
    },
    /// Cancel only this current task.
    CancelTask {
        /// Workbench-assigned identity.
        task: u64,
    },
    /// Import a trusted package matching the detected platform.
    ImportPackage {
        /// None opens the zip chooser.
        path: Option<String>,
        /// Experimental device variant.
        backend: Option<ExperimentalBackend>,
        /// Confirmation for experimental packages.
        confirmed: bool,
    },
    /// Copy a local .bin.gz/.txt.gz into the resource store.
    ImportWeight {
        /// None opens the gzip chooser.
        path: Option<String>,
    },
    /// Choose executable/supporting files and explicitly check its version.
    ChooseLocalEngine {
        /// Paths or chooser requests.
        choice: LocalEngineChoice,
    },
    /// Explicit version verification and required CUDA inference probe.
    Verify {
        /// Engine selection.
        target: ResourceEngineChoice,
        /// Weight for a required CUDA probe.
        weight: Option<String>,
    },
    /// Preview exactly one profile addition/update without saving anything.
    PlanSetup {
        /// Engine/config selection.
        engine: ResourceEngineChoice,
        /// Installed or explicitly chosen weight path.
        weight: String,
    },
    /// Commit the preview if its captured profile snapshot still matches.
    ApplySetup {
        /// Plan identity.
        plan: u64,
        /// Set the committed profile as default.
        make_default: bool,
        /// Start through the normal switch-engine path.
        switch_now: bool,
    },
    /// Refresh strongest API and networks table, retaining cache on failure.
    RefreshRemoteWeights,
    /// Restore a missing analysis config without overwriting user edits.
    RepairAnalysisConfig,
    /// Open the per-user resource directory.
    OpenResourceFolder,
    /// Return cache size, or delete only after confirmation.
    CleanNvidiaCache {
        /// False inspects; true confirms deletion.
        confirmed: bool,
    },
    /// Windows-only, confirmed TensorRT engine/runtime repair.
    RepairTensorRt {
        /// Explicit download/install confirmation.
        confirmed: bool,
    },
    /// Preview TensorRT using the current weight; apply remains separate.
    EnableTensorRt,
    /// Preview CUDA using the current weight; apply remains separate.
    SwitchBackToCuda,
    /// Save Java-compatible network proxy settings.
    SetNetworkProxy {
        /// Direct/System/Manual configuration.
        settings: NetworkSettings,
    },
}
/// Preview requiring a separate ApplySetup confirmation.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePlanView {
    /// Opaque workbench plan identity.
    pub id: u64,
    /// Existing managed profile, or None for addition.
    pub replaces_profile_id: Option<String>,
    /// Name that will be saved.
    pub name: String,
    /// Exact launch command that will be saved.
    pub command: String,
    /// Whether the target is currently the default.
    pub is_default: bool,
}
/// Completion of a resource command (long operations return Started).
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(tag = "type")]
pub enum ResourceResult {
    /// Immediate operation completed.
    Done,
    /// Background operation accepted.
    Started {
        /// Task identity.
        task: u64,
    },
    /// Profile change preview.
    Plan {
        /// Exact changes awaiting confirmation.
        plan: ResourcePlanView,
    },
    /// Version/probe result from an already-active target.
    Verified {
        /// Version classification.
        validation: EngineValidation,
        /// Required inference probe, when applicable.
        cuda: Option<CudaProbeResult>,
    },
    /// Native choosers completed (also delivered in the view after asynchronous verification).
    Chosen {
        /// Complete engine selection.
        engine: ResourceEngineChoice,
        /// Selected model.
        weight: String,
    },
    /// Cancellation requested or chooser dismissed.
    Cancelled,
    /// Cache inspection for a separate confirmation.
    CacheSize {
        /// Bytes in the NVIDIA download cache only.
        bytes: u64,
    },
    /// Operation did not change resources or settings.
    Refused {
        /// Java localization key.
        message_key: String,
    },
}
/// One running task; all updates are keyed by id and stale updates are ignored.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTaskView {
    /// Workbench-assigned identity.
    pub id: u64,
    /// Operation category.
    pub kind: String,
    /// Safe catalog identity, never a URL or personal path.
    pub item: String,
    /// Bytes received.
    pub bytes: u64,
    /// Entire resource size, if known.
    pub total: Option<u64>,
    /// Current phase (detecting/downloading/installing/verifying).
    pub phase: String,
    /// Whether cancellation may be requested.
    pub cancellable: bool,
}
/// A terminal state delivered once in incremental views.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ResourceOutcome {
    /// Task identity.
    pub task: u64,
    /// done / cancelled / failed.
    pub status: String,
    /// Java localization key.
    pub message_key: String,
    /// Sanitized bounded diagnostic, without personal paths or credentials.
    pub detail: String,
}
/// Foreground/default profile details used by Overview.
#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSetupView {
    /// Selected profile identity.
    pub profile_id: Option<String>,
    /// Executable path.
    pub executable: Option<String>,
    /// Explicitly verified version or the ready foreground KataGo's GTP identity; no probe is started.
    pub engine_version: Option<String>,
    /// Weight path.
    pub weight: Option<String>,
    /// Identity read from the weight header.
    pub model_name: Option<String>,
    /// B11 transformer notice applies.
    pub b11: bool,
    /// GTP config path.
    pub config: Option<String>,
    /// Analysis config path.
    pub analysis_config: Option<String>,
    /// The command still matches its store-owned marker.
    pub managed: bool,
    /// foreground / default / local / downloaded / imported.
    pub discovery_source: String,
}
/// Acceleration status, kept separate from runnable/fastest claims.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ResourceAccelerationView {
    /// Backend identifier.
    pub backend: String,
    /// Platform supports the installer.
    pub applicable: bool,
    /// Required missing runtime file patterns.
    pub missing: Vec<String>,
    /// Bytes safe to delete after confirmation.
    pub cache_bytes: u64,
    /// Experimental packages are never automatic recommendations.
    pub experimental: bool,
}
/// Resource repair action attached to a managed NVIDIA engine failure.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ResourceRepairView {
    /// Failed managed profile.
    pub profile_id: String,
    /// CUDA or TensorRT backend.
    pub backend: String,
    /// Missing runtime items.
    pub missing: Vec<String>,
    /// Java action label.
    pub message_key: String,
}
/// Complete resources projection; omitted from WorkbenchView when unchanged.
#[derive(Clone, Debug, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ResourcesView {
    /// Last detected facts; None means not yet detected.
    pub machine: Option<MachineFacts>,
    /// Detection currently running.
    pub detecting: bool,
    /// Structured package/model recommendations.
    pub recommendation: Option<Recommendation>,
    /// Trusted package descriptions, licenses, source and capabilities.
    pub packages: Vec<Asset>,
    /// Pinned model descriptions, licenses, source and capabilities.
    pub models: Vec<Model>,
    /// Trusted source repository and release page for these packages.
    pub engine_release_url: String,
    /// KataGo version pinned by the catalog.
    pub engine_version: String,
    /// Full upstream commit used to build the packages.
    pub engine_source_commit: String,
    /// Catalog asset identity to trusted download URL.
    pub package_urls: std::collections::BTreeMap<String, String>,
    /// Catalog model identity to trusted download URL.
    pub model_urls: std::collections::BTreeMap<String, String>,
    /// Bounded store inventory including receipts and header identities.
    pub inventory: ResourceInventory,
    /// Cached official network list.
    pub online_weights: Vec<RemoteWeight>,
    /// idle / refreshing / done / Java cache-preserving failure key.
    pub remote_status: String,
    /// Current foreground/default setup.
    pub current_setup: ResourceSetupView,
    /// CUDA, TensorRT and experimental status rows.
    pub acceleration: Vec<ResourceAccelerationView>,
    /// Current operation.
    pub task: Option<ResourceTaskView>,
    /// Last terminal outcome, consumed once in incremental delivery.
    pub last_outcome: Option<ResourceOutcome>,
    /// Last explicitly verified executable and result.
    pub verification: Option<EngineValidation>,
    /// Executable associated with `verification`; both are updated atomically on task completion.
    pub verification_executable: Option<String>,
    /// Required CUDA inference result.
    pub cuda_probe: Option<CudaProbeResult>,
    /// Complete local selection after native choosers.
    pub chosen_engine: Option<ResourceEngineChoice>,
    /// Complete local weight selection.
    pub chosen_weight: Option<String>,
    /// No configured command and startup is not Empty (Java banner has no dismiss).
    pub startup_needs_repair: bool,
    /// Persisted network configuration.
    pub network: NetworkSettings,
}

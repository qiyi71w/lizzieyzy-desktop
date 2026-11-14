use crate::update::{FailureKind, InstallForm, UpdateChannel, UpdateSettings};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 检查更新 (Menu.java:5274-5277, `CheckUpdateDialog`, `WindowsUpdateDialog`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum UpdateCommand {
    /// The page's 更新通道, 更新源 and the two automatic options, saved at once.
    Settings { settings: UpdateSettings },
    /// 检查更新: a manual check with the saved channel and source.
    Check,
    /// 立即更新 on the offer `offer`: prepares its package if needed, then installs it.
    Update { offer: u64 },
    /// 暂停 / 继续 of a running download.
    TogglePause,
    /// 取消: stops the download; what was downloaded is kept for the next try.
    Cancel,
    /// 继续 after a cancelled or failed download.
    Retry,
}

/// Who started the current check (Java `UpdateCheckCoordinator.Origin`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum UpdateOrigin {
    Automatic,
    Manual,
}

/// Java `UpdateCheckResult.Reason`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum UpdateOutcome {
    UnavailableBuild,
    SigningNotConfigured,
    UnsupportedForm,
    NoUpdate,
    NoPackage,
    Offer,
    Failed,
}

/// Java `UpdateCheckCoordinator.DownloadPhase`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, TS)]
pub enum UpdateDownloadPhase {
    #[default]
    None,
    Downloading,
    Paused,
    Cancelled,
    Failed,
    /// Downloaded and its signature verified; waiting for 立即更新.
    Ready,
}

/// Why preparing or installing failed (`UpdateText.userFacingError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum UpdateError {
    /// The package's signature, version or size did not match; nothing was installed.
    Integrity,
    /// No download source could deliver the package.
    Sources,
    /// Installing the verified package failed; the installed program is unchanged.
    Install,
    /// Portable zip: the program folder cannot be written, so nothing was changed.
    Permission,
    /// Portable zip: a program file was in use and could not be replaced; the old files were put
    /// back.
    InUse,
    /// Portable zip: the replacement was interrupted (the computer stopped, say); the old files
    /// were put back at start.
    Interrupted,
    /// Portable zip: the old files could not all be put back yet; it is tried again at the next
    /// start.
    Unrecovered,
    /// Deb: the system's authorization prompt was dismissed; nothing was installed.
    AuthorizationDismissed,
    /// Deb: the system did not grant authorization, or could not ask for it; nothing was
    /// installed.
    Unauthorized,
    /// Deb: the system package manager refused or failed; the installed program is unchanged.
    PackageManager,
    /// Deb: the installation stopped part-way and the package is not in an installed state; it
    /// has to be installed again.
    PackageUnfinished,
    Generic,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
pub struct UpdateOfferView {
    /// Identifies this offer for [`UpdateCommand::Update`].
    pub id: u64,
    pub version: String,
    pub current_version: String,
    pub form: InstallForm,
    pub asset_name: String,
    pub size: u64,
    /// 查看 Release.
    pub notes_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
pub struct UpdateResultView {
    pub outcome: UpdateOutcome,
    pub failure: Option<FailureKind>,
    /// The channel the check used.
    pub channel: UpdateChannel,
    /// 测试 channel: one candidate could not be verified (`UpdateCheckFeedback.warning`).
    pub stable_candidate_failed: bool,
    pub beta_candidate_failed: bool,
    pub offer: Option<UpdateOfferView>,
}

/// The package transfer as Java's dialogs show it (`UpdateDownloadText.status`): the phase, the
/// bytes against the total, and the signature check once the bytes are in.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, TS)]
pub struct UpdateDownloadView {
    pub phase: UpdateDownloadPhase,
    /// No progress reported yet (`准备下载...`).
    pub started: bool,
    pub bytes: u64,
    pub total: u64,
    /// Downloaded; checking the signature.
    pub verifying: bool,
    pub error: Option<UpdateError>,
    /// With `error`, what the system itself said (the package manager's reasons), or the command
    /// that installs the prepared package by hand. Not translated.
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
pub struct UpdateView {
    /// 当前版本.
    pub current_version: String,
    pub settings: UpdateSettings,
    /// This run's automatic check: a checked option without it shows (下次启动生效).
    pub auto_check_active: bool,
    pub auto_download_active: bool,
    pub checking: bool,
    /// A check, download or installation is under way: 检查更新 and 立即更新 wait.
    pub busy: bool,
    pub origin: Option<UpdateOrigin>,
    pub result: Option<UpdateResultView>,
    pub download: UpdateDownloadView,
    /// Increases when the offer should be shown: after a manual check, or once for an automatic
    /// check or download (`origin` tells which; an automatic one does not take focus).
    pub remind: u64,
    /// The installation was handed over: the program closes (and restarts) now.
    pub installing: bool,
    /// The previous run closed to install an update, which did not happen: why (`Install` when
    /// the installer did not start). Shown once at start.
    pub handover_failed: Option<UpdateError>,
}

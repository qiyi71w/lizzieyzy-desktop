//! 检查更新's lifecycle (Java `UpdateCheckCoordinator`): one check, offer, package preparation
//! and installation handover at a time. Checking and downloading run off the state lock in the
//! platform adapter, which hands each result back with the task id it was given; a result of a
//! task that is no longer current is dropped.
//!
//! Authority follows Java: seeing an offer grants nothing; 自动下载更新 only prepares the package;
//! 立即更新 is the one confirmation that prepares (if needed) and installs it. The automatic
//! options are taken from the settings once at start (`captureStartupPolicy`): turning one on
//! applies from the next start, turning it off (or changing the channel or source) withdraws
//! this run's automatic work at once.

use super::Workbench;
use crate::api::{
    ApiError, UpdateCommand, UpdateDownloadPhase, UpdateDownloadView, UpdateError, UpdateOfferView, UpdateOrigin,
    UpdateOutcome, UpdateResultView, UpdateView,
};
use crate::resources::DownloadSpec;
use crate::update::{
    CheckOutcome, CheckResult, InstallForm, InstalledApp, Offer, Selection, UpdateChannel, UpdateSettings,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A check to run off-lock with [`crate::update::discover`]; hand the result to
/// [`Workbench::complete_update_check`].
#[derive(Clone, Debug)]
pub struct UpdateCheckJob {
    pub id: u64,
    pub installed: InstalledApp,
    pub selection: Selection,
}

/// A package to prepare: download into `dir` (keeping a partial file for later), from `urls` in
/// order, then verify its signature against `offer`. Progress goes to
/// [`Workbench::update_download_progress`], the result to [`Workbench::complete_update_download`].
#[derive(Clone, Debug)]
pub struct UpdateDownloadJob {
    pub id: u64,
    /// The update staging folder of this release; other releases' folders beside it are stale.
    pub dir: PathBuf,
    pub spec: DownloadSpec,
    pub urls: Vec<String>,
    pub offer: Offer,
    pub cancel: Arc<AtomicBool>,
}

/// A prepared package to install. Failure goes to [`Workbench::update_install_failed`].
#[derive(Clone, Debug)]
pub struct UpdateInstallJob {
    pub offer: Offer,
    pub form: InstallForm,
    pub path: PathBuf,
}

#[derive(Clone, Debug)]
pub enum UpdateTask {
    Check(UpdateCheckJob),
    Download(UpdateDownloadJob),
    Install(UpdateInstallJob),
}

/// What an update operation did: whether the view changed, and the work to start.
#[derive(Clone, Debug, Default)]
pub struct UpdateStep {
    pub changed: bool,
    pub task: Option<UpdateTask>,
}

impl UpdateStep {
    fn changed() -> Self {
        UpdateStep {
            changed: true,
            task: None,
        }
    }
    fn task(task: Option<UpdateTask>) -> Self {
        UpdateStep { changed: true, task }
    }
}

struct ActiveDownload {
    id: u64,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
struct Transfer {
    phase: UpdateDownloadPhase,
    bytes: u64,
    total: u64,
    /// A progress report arrived since 立即更新 or 自动下载 started this preparation.
    started: bool,
    verifying: bool,
    error: Option<UpdateError>,
}

pub(super) struct UpdateFlow {
    installed: InstalledApp,
    staging: PathBuf,
    startup_captured: bool,
    startup_consumed: bool,
    startup: Option<Selection>,
    automatic_check: bool,
    automatic_download: bool,
    checking: bool,
    origin: Option<UpdateOrigin>,
    selection: Option<Selection>,
    result: Option<CheckResult>,
    /// Identity of `result`: the offer the page and the offer window refer to.
    result_id: u64,
    transfer: Transfer,
    next_task: u64,
    active_check: Option<u64>,
    active_download: Option<ActiveDownload>,
    /// 继续 was pressed while the paused transfer was still winding down.
    resume_pending: bool,
    /// 立即更新 confirmed this preparation: install as soon as it is ready.
    manual_download: bool,
    /// The automatic work was withdrawn while it ran.
    invalidated: bool,
    reminded: bool,
    show_offer: u64,
    prepared: Option<PathBuf>,
    installing: bool,
}

impl Default for UpdateFlow {
    fn default() -> Self {
        UpdateFlow {
            installed: InstalledApp {
                version: semver::Version::new(0, 0, 0),
                os: String::new(),
                arch: String::new(),
                form: InstallForm::Unpackaged,
                signing_configured: false,
            },
            staging: PathBuf::new(),
            startup_captured: false,
            startup_consumed: false,
            startup: None,
            automatic_check: false,
            automatic_download: false,
            checking: false,
            origin: None,
            selection: None,
            result: None,
            result_id: 0,
            transfer: Transfer::default(),
            next_task: 0,
            active_check: None,
            active_download: None,
            resume_pending: false,
            manual_download: false,
            invalidated: false,
            reminded: false,
            show_offer: 0,
            prepared: None,
            installing: false,
        }
    }
}

impl UpdateFlow {
    fn offer(&self) -> Option<&Offer> {
        match &self.result {
            Some(CheckResult {
                outcome: CheckOutcome::Offer(offer),
                ..
            }) => Some(offer),
            _ => None,
        }
    }
    fn downloading(&self) -> bool {
        matches!(
            self.transfer.phase,
            UpdateDownloadPhase::Downloading | UpdateDownloadPhase::Paused
        )
    }
    /// Java `inFlight`, with a paused transfer counted as still running.
    fn busy(&self) -> bool {
        self.active_check.is_some()
            || self.active_download.is_some()
            || self.transfer.phase == UpdateDownloadPhase::Paused
            || self.installing
    }
    fn next_id(&mut self) -> u64 {
        self.next_task += 1;
        self.next_task
    }
    /// The state without a check or offer (Java `new State(false, null, null, null)`).
    fn clear(&mut self) {
        self.checking = false;
        self.origin = None;
        self.selection = None;
        self.result = None;
        self.transfer = Transfer::default();
    }
    fn remind(&mut self) {
        if !self.reminded {
            self.reminded = true;
            self.show_offer += 1;
        }
    }
}

impl Workbench {
    /// The running client, and where packages are prepared. Takes this run's automatic policy
    /// from the saved settings (Java `captureStartupPolicy`, once, before any control shows).
    pub fn set_update_installation(&mut self, installed: InstalledApp, staging: PathBuf) {
        let u = &mut self.update;
        u.installed = installed;
        u.staging = staging;
        if u.startup_captured {
            return;
        }
        u.startup_captured = true;
        let settings = self.config.update.normalized();
        u.automatic_check = settings.auto_check;
        u.automatic_download = settings.auto_download;
        u.startup = settings.auto_check.then(|| Selection::of(&settings));
    }

    /// The main window is ready: 启动时自动检查更新 starts its one check (`applicationReady`).
    pub fn update_application_ready(&mut self) -> UpdateStep {
        let u = &mut self.update;
        if u.startup_consumed {
            return UpdateStep::default();
        }
        u.startup_consumed = true;
        match u.startup.take() {
            Some(selection) => self.start_update_check(UpdateOrigin::Automatic, selection),
            None => UpdateStep::default(),
        }
    }

    pub fn update(&mut self, op: UpdateCommand) -> Result<UpdateStep, ApiError> {
        Ok(match op {
            UpdateCommand::Settings { settings } => self.set_update_settings(settings)?,
            UpdateCommand::Check => {
                let selection = Selection::of(&self.config.update);
                self.start_update_check(UpdateOrigin::Manual, selection)
            }
            UpdateCommand::Update { offer } if offer == self.update.result_id => self.start_update_download(true),
            UpdateCommand::Update { .. } => UpdateStep::default(),
            UpdateCommand::TogglePause => self.toggle_update_pause(),
            UpdateCommand::Cancel => self.cancel_update_download(),
            UpdateCommand::Retry => {
                let manual = self.update.manual_download;
                self.start_update_download(manual)
            }
        })
    }

    /// Java `UpdatePreferences.persistAutoCheck` / `persistAutoDownload` and
    /// `CheckUpdateDialog.persistSelection`: 更新源 is kept while 测试 is chosen.
    fn set_update_settings(&mut self, settings: UpdateSettings) -> Result<UpdateStep, ApiError> {
        let old = self.config.update;
        let mut next = settings.normalized();
        if next.channel == UpdateChannel::Beta {
            next.source = old.source;
        }
        if next == old {
            return Ok(UpdateStep::default());
        }
        self.commit_config(|c| c.update = next)?;
        let selection_changed =
            old.channel != next.channel || (next.channel != UpdateChannel::Beta && old.source != next.source);
        if (old.auto_check && !next.auto_check) || selection_changed {
            self.invalidate_automatic_update();
        } else if old.auto_download && !next.auto_download {
            self.update.automatic_download = false;
            let u = &self.update;
            if u.origin == Some(UpdateOrigin::Automatic) && !u.manual_download && u.downloading() {
                self.cancel_update_download();
            }
        }
        Ok(UpdateStep::changed())
    }

    /// Java `invalidateAutomatic`: this run's automatic check and download end; a manual check,
    /// or a package 立即更新 asked for, is left alone.
    fn invalidate_automatic_update(&mut self) {
        let u = &mut self.update;
        u.automatic_check = false;
        u.automatic_download = false;
        u.startup = None;
        if u.origin != Some(UpdateOrigin::Automatic) || u.manual_download {
            return;
        }
        if let Some(active) = &u.active_download {
            active.cancel.store(true, Ordering::Release);
        }
        u.prepared = None;
        u.invalidated = true;
        u.resume_pending = false;
        let checking = u.active_check.is_some();
        u.clear();
        u.checking = checking;
        u.origin = Some(UpdateOrigin::Automatic);
    }

    fn start_update_check(&mut self, origin: UpdateOrigin, selection: Selection) -> UpdateStep {
        if self.update.busy() {
            return UpdateStep::default();
        }
        let id = self.update.next_id();
        let u = &mut self.update;
        u.active_check = Some(id);
        u.invalidated = false;
        u.prepared = None;
        u.manual_download = false;
        u.clear();
        u.checking = true;
        u.origin = Some(origin);
        u.selection = Some(selection);
        UpdateStep::task(Some(UpdateTask::Check(UpdateCheckJob {
            id,
            installed: u.installed.clone(),
            selection,
        })))
    }

    /// The finished check `id` (Java `complete`): an offer from 启动时自动检查更新 is prepared
    /// when 自动下载更新 is on, else shown once; a manual check's offer is shown.
    pub fn complete_update_check(&mut self, id: u64, result: CheckResult) -> UpdateStep {
        let u = &mut self.update;
        if u.active_check != Some(id) {
            return UpdateStep::default();
        }
        u.active_check = None;
        u.checking = false;
        if u.origin == Some(UpdateOrigin::Automatic) && u.invalidated {
            u.clear();
            return UpdateStep::changed();
        }
        u.result = Some(result);
        u.result_id = id;
        if u.offer().is_none() {
            return UpdateStep::changed();
        }
        if u.origin == Some(UpdateOrigin::Automatic) {
            if u.automatic_download {
                return self.start_update_download(false);
            }
            u.remind();
        } else {
            u.show_offer += 1;
        }
        UpdateStep::changed()
    }

    /// Java `startDownload(expected, completeWhenReady)`: `install` is 立即更新's authority to
    /// install once prepared.
    fn start_update_download(&mut self, install: bool) -> UpdateStep {
        let u = &mut self.update;
        if u.busy() || u.offer().is_none() {
            return UpdateStep::default();
        }
        if u.transfer.phase == UpdateDownloadPhase::Ready {
            if !install {
                return UpdateStep::default();
            }
            u.manual_download = true;
            return self.install_prepared_update();
        }
        u.manual_download = install;
        u.invalidated = false;
        u.prepared = None;
        u.transfer = Transfer {
            phase: UpdateDownloadPhase::Downloading,
            ..Transfer::default()
        };
        self.launch_update_download()
    }

    /// A transfer for the current offer, continuing a partial file left by an earlier one.
    fn launch_update_download(&mut self) -> UpdateStep {
        let id = self.update.next_id();
        let u = &mut self.update;
        let Some(offer) = u.offer().cloned() else {
            return UpdateStep::default();
        };
        let cancel = Arc::new(AtomicBool::new(false));
        u.active_download = Some(ActiveDownload {
            id,
            cancel: cancel.clone(),
        });
        u.transfer.verifying = false;
        u.transfer.error = None;
        let mut urls = vec![offer.asset.url.clone()];
        urls.extend(offer.asset.mirrors.iter().cloned());
        let job = UpdateDownloadJob {
            id,
            dir: u.staging.join(offer.release.version.to_string()),
            spec: DownloadSpec {
                url: offer.asset.url.clone(),
                file_name: offer.asset.file_name(),
                size: Some(offer.asset.size),
                sha256: None,
            },
            urls,
            offer,
            cancel,
        };
        UpdateStep::task(Some(UpdateTask::Download(job)))
    }

    /// Download progress of transfer `id`. Returns whether the view changed.
    pub fn update_download_progress(&mut self, id: u64, bytes: u64, total: Option<u64>) -> bool {
        let u = &mut self.update;
        if u.active_download.as_ref().is_none_or(|a| a.id != id) || u.transfer.phase != UpdateDownloadPhase::Downloading
        {
            return false;
        }
        let t = &mut u.transfer;
        t.started = true;
        t.bytes = bytes;
        t.total = total.unwrap_or(t.total);
        t.verifying = false;
        true
    }

    /// Transfer `id` is complete and its signature is being checked.
    pub fn update_download_verifying(&mut self, id: u64) -> bool {
        let u = &mut self.update;
        if u.active_download.as_ref().is_none_or(|a| a.id != id) {
            return false;
        }
        u.transfer.verifying = true;
        u.transfer.bytes = u.transfer.total.max(u.transfer.bytes);
        true
    }

    /// The end of transfer `id`: the verified package, or the error (`None`: it was stopped).
    pub fn complete_update_download(&mut self, id: u64, result: Result<PathBuf, Option<UpdateError>>) -> UpdateStep {
        let u = &mut self.update;
        if u.active_download.as_ref().is_none_or(|a| a.id != id) {
            return UpdateStep::default();
        }
        u.active_download = None;
        u.transfer.verifying = false;
        if u.invalidated {
            u.clear();
            return UpdateStep::changed();
        }
        match result {
            Err(_) if u.resume_pending => {
                u.resume_pending = false;
                return self.launch_update_download();
            }
            // Paused or cancelled: the phase the user chose stays.
            Err(_)
                if matches!(
                    u.transfer.phase,
                    UpdateDownloadPhase::Paused | UpdateDownloadPhase::Cancelled
                ) => {}
            Err(None) => u.transfer.phase = UpdateDownloadPhase::Cancelled,
            Err(Some(error)) => {
                u.transfer.phase = UpdateDownloadPhase::Failed;
                u.transfer.error = Some(error);
            }
            Ok(_) if u.transfer.phase == UpdateDownloadPhase::Cancelled => {}
            Ok(path) => {
                u.resume_pending = false;
                u.prepared = Some(path);
                u.transfer.phase = UpdateDownloadPhase::Ready;
                u.transfer.bytes = u.transfer.total;
                if u.manual_download {
                    return self.install_prepared_update();
                }
                u.remind();
            }
        }
        UpdateStep::changed()
    }

    /// Java `togglePause`. Pausing stops the transfer and keeps what arrived; 继续 continues it.
    fn toggle_update_pause(&mut self) -> UpdateStep {
        let u = &mut self.update;
        match u.transfer.phase {
            UpdateDownloadPhase::Downloading => {
                if let Some(active) = &u.active_download {
                    active.cancel.store(true, Ordering::Release);
                }
                u.resume_pending = false;
                u.transfer.phase = UpdateDownloadPhase::Paused;
                UpdateStep::changed()
            }
            UpdateDownloadPhase::Paused => {
                u.transfer.phase = UpdateDownloadPhase::Downloading;
                if u.active_download.is_some() {
                    u.resume_pending = true;
                    return UpdateStep::changed();
                }
                self.launch_update_download()
            }
            _ => UpdateStep::default(),
        }
    }

    /// Java `cancelDownload`: the transfer stops; its partial file stays for 继续.
    fn cancel_update_download(&mut self) -> UpdateStep {
        let u = &mut self.update;
        if !u.downloading() {
            return UpdateStep::default();
        }
        if let Some(active) = &u.active_download {
            active.cancel.store(true, Ordering::Release);
        }
        u.resume_pending = false;
        u.prepared = None;
        u.transfer.phase = UpdateDownloadPhase::Cancelled;
        UpdateStep::changed()
    }

    /// Java `applyPrepared`.
    fn install_prepared_update(&mut self) -> UpdateStep {
        let u = &mut self.update;
        let (Some(path), Some(offer)) = (u.prepared.clone(), u.offer().cloned()) else {
            return UpdateStep::default();
        };
        if u.transfer.phase != UpdateDownloadPhase::Ready || u.invalidated || u.busy() {
            return UpdateStep::default();
        }
        u.installing = true;
        u.transfer.error = None;
        let form = u.installed.form;
        UpdateStep::task(Some(UpdateTask::Install(UpdateInstallJob { offer, form, path })))
    }

    /// The handover failed and the installed program is unchanged. A package that no longer
    /// verifies is discarded; otherwise it stays ready for another try.
    pub fn update_install_failed(&mut self, error: UpdateError) -> bool {
        let u = &mut self.update;
        if !u.installing {
            return false;
        }
        u.installing = false;
        u.transfer.error = Some(error);
        if error == UpdateError::Integrity {
            u.prepared = None;
            u.transfer.phase = UpdateDownloadPhase::Failed;
        }
        true
    }

    /// 查看 Release of the current offer.
    pub fn update_release_page(&self) -> Option<String> {
        self.update.offer().and_then(|o| o.release.notes_url.clone())
    }

    pub(super) fn update_view(&self) -> UpdateView {
        let u = &self.update;
        let result = u.result.as_ref().map(|r| {
            let (outcome, failure) = match &r.outcome {
                CheckOutcome::UnavailableBuild => (UpdateOutcome::UnavailableBuild, None),
                CheckOutcome::SigningNotConfigured => (UpdateOutcome::SigningNotConfigured, None),
                CheckOutcome::UnsupportedForm => (UpdateOutcome::UnsupportedForm, None),
                CheckOutcome::NoUpdate => (UpdateOutcome::NoUpdate, None),
                CheckOutcome::NoPackage => (UpdateOutcome::NoPackage, None),
                CheckOutcome::Offer(_) => (UpdateOutcome::Offer, None),
                CheckOutcome::Failed(kind) => (UpdateOutcome::Failed, Some(*kind)),
            };
            UpdateResultView {
                outcome,
                failure,
                channel: u.selection.map(|s| s.channel).unwrap_or_default(),
                stable_candidate_failed: r.stable_failure.is_some(),
                beta_candidate_failed: r.beta_failure.is_some(),
                offer: u.offer().map(|o| UpdateOfferView {
                    id: u.result_id,
                    version: o.release.version.to_string(),
                    current_version: u.installed.version.to_string(),
                    form: u.installed.form,
                    asset_name: o.asset.file_name(),
                    size: o.asset.size,
                    notes_url: o.release.notes_url.clone(),
                }),
            }
        });
        let t = &u.transfer;
        UpdateView {
            current_version: u.installed.version.to_string(),
            settings: self.config.update,
            auto_check_active: u.automatic_check,
            auto_download_active: u.automatic_download,
            checking: u.checking,
            busy: u.busy(),
            origin: u.origin,
            result,
            download: UpdateDownloadView {
                phase: t.phase,
                bytes: t.bytes,
                total: t.total,
                started: t.started,
                verifying: t.verifying,
                error: t.error,
            },
            remind: u.show_offer,
            installing: u.installing,
        }
    }
}

//! 检查更新's platform side: which installation this is, reading manifests and packages off the
//! state lock, checking package signatures with the Tauri updater, and handing a verified package
//! to the Tauri updater's installer for this installation form.
//!
//! Windows (NSIS): the program first goes through its normal exit (saves, window state, engines),
//! then the installer starts and reopens the program. macOS (app bundle) and Linux (AppImage):
//! the bundle is replaced in place (the previous one is restored if that fails), then the
//! program restarts through its normal exit.
use crate::resources::HttpFetch;
use crate::{publish, State};
use lizzie_core::api::{ApiError, CommandResult, UpdateCommand, UpdateError};
use lizzie_core::resources as r;
use lizzie_core::update::{self as u, InstallForm, InstalledApp, Offer};
use lizzie_core::workbench::{UpdateCheckJob, UpdateDownloadJob, UpdateInstallJob, UpdateStep, UpdateTask};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};
use std::thread;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::UpdaterExt;

/// The project's update public key built into this program (`plugins.updater.pubkey`).
static PUBKEY: OnceLock<String> = OnceLock::new();
/// A Windows installation waiting for the program's exit.
static PENDING: Mutex<Option<(tauri_plugin_updater::Update, Vec<u8>)>> = Mutex::new(None);
/// Left in the update staging folder when the installer did not start after the program's exit;
/// the reopened program reports it.
static HANDOVER_FAILED: OnceLock<PathBuf> = OnceLock::new();

fn pubkey() -> &'static str {
    PUBKEY.get().map(String::as_str).unwrap_or("")
}

/// The update key: the configured one, or in an acceptance build the test key given by
/// `LIZZIEYZY_TAURI_UPDATE_PUBKEY`.
fn configured_pubkey(app: &AppHandle) -> String {
    #[cfg(feature = "acceptance")]
    if let Ok(key) = std::env::var("LIZZIEYZY_TAURI_UPDATE_PUBKEY") {
        return key.trim().to_string();
    }
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|c| c.get("pubkey"))
        .and_then(|k| k.as_str())
        .unwrap_or("")
        .trim()
        .to_string()
}

/// How this program was installed. Windows: an NSIS installation has the installer's
/// `uninstall.exe` beside the program; a release program without it is the portable zip.
/// macOS: the program runs inside an `.app` bundle. Linux: the Tauri bundler's marker, with the
/// AppImage runtime's `APPIMAGE` for an AppImage.
fn install_form() -> InstallForm {
    #[cfg(feature = "acceptance")]
    if let Ok(form) = std::env::var("LIZZIEYZY_TAURI_UPDATE_FORM") {
        return match form.as_str() {
            "nsis" => InstallForm::Nsis,
            "app" => InstallForm::App,
            "appimage" => InstallForm::AppImage,
            "zip" => InstallForm::PortableZip,
            "deb" => InstallForm::Deb,
            "other" => InstallForm::Other,
            _ => InstallForm::Unpackaged,
        };
    }
    #[allow(unused_imports)]
    use tauri::utils::{config::BundleType, platform::bundle_type};
    let exe = std::env::current_exe().ok();
    #[cfg(windows)]
    {
        let uninstaller = exe
            .as_deref()
            .and_then(Path::parent)
            .is_some_and(|d| d.join("uninstall.exe").is_file());
        match bundle_type() {
            Some(BundleType::Nsis) if uninstaller => InstallForm::Nsis,
            Some(BundleType::Msi) => InstallForm::Other,
            _ if cfg!(debug_assertions) => InstallForm::Unpackaged,
            _ => InstallForm::PortableZip,
        }
    }
    #[cfg(target_os = "macos")]
    {
        let in_bundle = exe.as_deref().and_then(Path::parent).is_some_and(|macos| {
            macos.ends_with("Contents/MacOS")
                && macos
                    .parent()
                    .and_then(Path::parent)
                    .is_some_and(|app| app.extension().is_some_and(|e| e == "app"))
        });
        if in_bundle {
            InstallForm::App
        } else {
            InstallForm::Unpackaged
        }
    }
    #[cfg(target_os = "linux")]
    {
        let _ = exe;
        match bundle_type() {
            Some(BundleType::AppImage) if std::env::var_os("APPIMAGE").is_some() => InstallForm::AppImage,
            Some(BundleType::Deb) => InstallForm::Deb,
            Some(BundleType::Rpm) => InstallForm::Other,
            _ => InstallForm::Unpackaged,
        }
    }
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = exe;
        InstallForm::Unpackaged
    }
}

fn installation(app: &AppHandle) -> InstalledApp {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        os => os,
    };
    InstalledApp {
        version: app.package_info().version.clone(),
        os: os.into(),
        arch: std::env::consts::ARCH.into(),
        form: install_form(),
        signing_configured: !pubkey().is_empty(),
    }
}

/// Start-up: records the installation and this run's automatic policy.
pub fn setup(app: &AppHandle, shared: &State) {
    let _ = PUBKEY.set(configured_pubkey(app));
    let installed = installation(app);
    let staging = app
        .path()
        .app_cache_dir()
        .or_else(|_| app.path().app_local_data_dir())
        .map(|d| d.join("updates"))
        .unwrap_or_default();
    shared.log.info(&format!(
        "update: version {} {}-{} {:?}, signing {}",
        installed.version,
        installed.os,
        installed.arch,
        installed.form,
        if installed.signing_configured {
            "configured"
        } else {
            "not configured"
        }
    ));
    let marker = HANDOVER_FAILED.get_or_init(|| staging.join("handover-failed"));
    let handover_failed = std::fs::remove_file(marker).is_ok();
    let mut wb = shared.workbench.lock().unwrap();
    wb.set_update_installation(installed, staging);
    if handover_failed {
        shared
            .log
            .warn("update: the previous run's installer did not start; the program is unchanged");
        wb.update_handover_failed_last_run();
    }
}

/// The main window is ready: 启动时自动检查更新.
pub fn application_ready(app: &AppHandle, shared: &State) {
    let step = shared.workbench.lock().unwrap().update_application_ready();
    run(app, shared, step);
}

pub fn dispatch(app: &AppHandle, shared: &State, op: UpdateCommand) -> Result<CommandResult, ApiError> {
    let step = shared.workbench.lock().unwrap().update(op)?;
    let changed = step.changed;
    run(app, shared, step);
    Ok(if changed {
        CommandResult::Done
    } else {
        CommandResult::Unchanged
    })
}

fn run(app: &AppHandle, shared: &State, step: UpdateStep) {
    if step.changed {
        publish(app, shared);
    }
    let Some(task) = step.task else { return };
    let (app, worker) = (app.clone(), shared.clone());
    let spawned = thread::Builder::new()
        .name("lizzie-update".into())
        .spawn(move || match task {
            UpdateTask::Check(job) => check(&app, &worker, job),
            UpdateTask::Download(job) => download(&app, &worker, job),
            UpdateTask::Install(job) => install(&app, &worker, job),
        });
    if let Err(e) = spawned {
        shared.log.warn(&format!("update: the worker could not start: {e}"));
    }
}

/// Manifests and packages through the user's network settings. In an acceptance build,
/// `LIZZIEYZY_TAURI_UPDATE_BASE` stands in for GitHub: `<base>/stable/` and `<base>/beta/`
/// serve the two manifests.
struct UpdateFetch(HttpFetch);

impl r::Fetch for UpdateFetch {
    fn get(&self, url: &str, start: Option<u64>) -> r::ResourceResult<r::FetchResponse> {
        self.0.get(&manifest_url(url), start)
    }
}

#[cfg(feature = "acceptance")]
fn manifest_url(url: &str) -> String {
    if let Ok(base) = std::env::var("LIZZIEYZY_TAURI_UPDATE_BASE") {
        let base = base.trim_end_matches('/');
        if url == u::GITHUB_STABLE_MANIFEST {
            return format!("{base}/stable/{}", u::MANIFEST_NAME);
        }
        if url == u::GITHUB_BETA_MANIFEST {
            return format!("{base}/beta/{}", u::MANIFEST_NAME);
        }
    }
    url.into()
}
#[cfg(not(feature = "acceptance"))]
fn manifest_url(url: &str) -> String {
    url.into()
}

fn fetcher(shared: &State) -> Result<UpdateFetch, String> {
    let network = shared.workbench.lock().unwrap().resource_network();
    HttpFetch::new(&network).map(UpdateFetch).map_err(|e| e.to_string())
}

fn check(app: &AppHandle, shared: &State, job: UpdateCheckJob) {
    let result = match fetcher(shared) {
        Ok(fetch) => u::discover(&job.installed, job.selection, &fetch),
        Err(e) => {
            shared.log.warn(&format!("update check: {e}"));
            u::CheckResult {
                outcome: u::CheckOutcome::Failed(u::FailureKind::Fetch),
                stable_failure: None,
                beta_failure: None,
            }
        }
    };
    shared.log.info(&format!("update check: {}", describe(&result.outcome)));
    let step = shared.workbench.lock().unwrap().complete_update_check(job.id, result);
    run(app, shared, step);
}

fn describe(outcome: &u::CheckOutcome) -> String {
    match outcome {
        u::CheckOutcome::Offer(o) => format!("offer {} ({})", o.release.version, o.key),
        other => format!("{other:?}"),
    }
}

fn host(url: &str) -> String {
    let host = tauri::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_default();
    if host == "github.com" || host.ends_with(".githubusercontent.com") {
        "GitHub".into()
    } else if host.is_empty() {
        "download source".into()
    } else {
        host
    }
}

/// The Tauri updater's view of `offer`, with this build's key and the offer's own platform
/// entry. Restoring always requires the signature to carry the announced version.
fn tauri_update(app: &AppHandle, offer: &Offer) -> Result<tauri_plugin_updater::Update, String> {
    let endpoint = tauri::Url::parse(u::GITHUB_STABLE_MANIFEST).map_err(|e| e.to_string())?;
    let updater = app
        .updater_builder()
        .pubkey(pubkey())
        .target(offer.key.clone())
        .endpoints(vec![endpoint])
        .and_then(|b| b.build())
        .map_err(|e| e.to_string())?;
    updater
        .restore_update(offer.release.raw.clone())
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "the release is not newer than this program".to_string())
}

/// Checks a downloaded package's signature; a package that fails is deleted.
fn verify(app: &AppHandle, offer: &Offer, path: &Path) -> Result<(), UpdateError> {
    let update = tauri_update(app, offer).map_err(|_| UpdateError::Generic)?;
    let bytes = std::fs::read(path).map_err(|_| UpdateError::Generic)?;
    if update.verify(&bytes).is_err() {
        let _ = std::fs::remove_file(path);
        return Err(UpdateError::Integrity);
    }
    Ok(())
}

/// Removes the staging folders of other releases; only the update staging folder is touched.
fn remove_stale(dir: &Path) {
    let (Some(staging), Some(keep)) = (dir.parent(), dir.file_name()) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(staging) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name() != keep && entry.file_type().is_ok_and(|t| t.is_dir()) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

fn download(app: &AppHandle, shared: &State, job: UpdateDownloadJob) {
    remove_stale(&job.dir);
    let result = transfer(app, shared, &job).and_then(|path| {
        if shared.workbench.lock().unwrap().update_download_verifying(job.id) {
            publish(app, shared);
        }
        verify(app, &job.offer, &path).map(|()| path)
    });
    let result = match result {
        Err(_) if job.cancel.load(Ordering::Acquire) => Err(None),
        other => other.map_err(Some),
    };
    if let Err(Some(error)) = &result {
        shared.log.warn(&format!("update download failed: {error:?}"));
    }
    let step = shared
        .workbench
        .lock()
        .unwrap()
        .complete_update_download(job.id, result);
    run(app, shared, step);
}

/// Java `ResumableDownloader.download`: each source in turn, continuing the partial file.
fn transfer(app: &AppHandle, shared: &State, job: &UpdateDownloadJob) -> Result<PathBuf, UpdateError> {
    let fetch = fetcher(shared).map_err(|_| UpdateError::Sources)?;
    let mut failure = UpdateError::Sources;
    for url in &job.urls {
        let source = host(url);
        let spec = r::DownloadSpec {
            url: url.clone(),
            ..job.spec.clone()
        };
        let progress = |p: r::DownloadProgress| {
            if shared
                .workbench
                .lock()
                .unwrap()
                .update_download_progress(job.id, p.bytes, p.total)
            {
                publish(app, shared);
            }
        };
        match r::download(&fetch, &job.dir, &spec, &job.cancel, progress) {
            Ok(done) => return Ok(done.path),
            Err(r::ResourceError::Cancelled) => return Err(UpdateError::Generic),
            Err(r::ResourceError::Integrity { .. }) => return Err(UpdateError::Integrity),
            Err(e @ r::ResourceError::Io(_)) => {
                shared.log.warn(&format!("update download from {source}: {e}"));
                failure = UpdateError::Generic;
            }
            Err(e) => shared.log.warn(&format!("update download from {source}: {e}")),
        }
        if job.cancel.load(Ordering::Acquire) {
            break;
        }
    }
    Err(failure)
}

/// 查看 Release: the offered release's page (`notesUrl`), else the project's releases. No
/// caller-provided address is opened.
#[tauri::command]
pub fn open_update_release(app: AppHandle, shared: tauri::State<'_, State>) -> Result<(), ApiError> {
    use tauri_plugin_opener::OpenerExt;
    let url = shared
        .workbench
        .lock()
        .unwrap()
        .update_release_page()
        .unwrap_or_else(|| "https://github.com/qiyi71w/lizzieyzy-desktop/releases".into());
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| ApiError::new(lizzie_core::api::ErrorKind::Unavailable, e.to_string()))
}

fn install(app: &AppHandle, shared: &State, job: UpdateInstallJob) {
    let failed = |error: UpdateError, reason: String| {
        shared.log.warn(&format!("update install failed: {reason}"));
        if shared.workbench.lock().unwrap().update_install_failed(error) {
            publish(app, shared);
        }
    };
    let update = match tauri_update(app, &job.offer) {
        Ok(update) => update,
        Err(e) => return failed(UpdateError::Generic, e),
    };
    let bytes = match std::fs::read(&job.path) {
        Ok(bytes) => bytes,
        Err(e) => return failed(UpdateError::Generic, e.to_string()),
    };
    if let Err(e) = update.verify(&bytes) {
        let _ = std::fs::remove_file(&job.path);
        return failed(UpdateError::Integrity, e.to_string());
    }
    shared.log.info(&format!(
        "update: installing {} ({})",
        job.offer.release.version, job.offer.key
    ));
    if job.form == InstallForm::Nsis {
        // The installer starts after the normal exit and reopens the program.
        *PENDING.lock().unwrap() = Some((update, bytes));
        app.exit(0);
        return;
    }
    if let Err(e) = update.install(&bytes) {
        return failed(UpdateError::Install, e.to_string());
    }
    shared.log.info("update: installed; restarting");
    app.request_restart();
}

/// The end of the program's exit: starts a Windows installation waiting for it. The installer
/// takes over and the process ends here. When it cannot start, the program opens again and says
/// so (Java keeps its window and shows the failure; this program has already closed).
pub fn finish_on_exit(app: &AppHandle, shared: &State) {
    let Some((update, bytes)) = PENDING.lock().unwrap().take() else {
        return;
    };
    shared.log.info("update: starting the installer");
    let Err(e) = update.install(&bytes) else {
        return;
    };
    shared.log.warn(&format!("update installer did not start: {e}"));
    if let Some(marker) = HANDOVER_FAILED.get() {
        let _ = std::fs::write(marker, b"");
    }
    let env = app.env();
    let reopened = tauri::process::current_binary(&env).and_then(|program| {
        std::process::Command::new(program)
            .args(env.args_os.iter().skip(1))
            .spawn()
    });
    if let Err(e) = reopened {
        shared.log.warn(&format!("update: the program could not reopen: {e}"));
    }
}

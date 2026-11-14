//! 检查更新's platform side: which installation this is, reading manifests and packages off the
//! state lock, checking package signatures with the Tauri updater, and handing a verified package
//! over for this installation form.
//!
//! Windows (NSIS): the program first goes through its normal exit (saves, window state, engines),
//! then the installer starts and reopens the program. macOS (app bundle) and Linux (AppImage):
//! the bundle is replaced in place (the previous one is restored if that fails), then the
//! program restarts through its normal exit. Windows portable zip: the package is unpacked into
//! the program folder, the program goes through its normal exit, and a copy of the program
//! started as the helper ([`helper`]) replaces the files and reopens it.
use crate::resources::HttpFetch;
use crate::{publish, State};
use lizzie_core::api::{ApiError, CommandResult, UpdateCommand, UpdateError};
use lizzie_core::resources as r;
use lizzie_core::update::portable::{self, PortableFailure, Recovery};
use lizzie_core::update::{self as u, InstallForm, InstalledApp, Offer};
use lizzie_core::workbench::{UpdateCheckJob, UpdateDownloadJob, UpdateInstallJob, UpdateStep, UpdateTask};
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;
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
    let mut failure = std::fs::read_to_string(marker).ok().map(|reason| marker_error(&reason));
    let _ = std::fs::remove_file(marker);
    if let Ok(notes) = std::fs::read_to_string(staging.join(HELPER_LOG)) {
        for line in notes.lines() {
            shared.log.info(&format!("update helper: {line}"));
        }
        let _ = std::fs::remove_file(staging.join(HELPER_LOG));
    }
    if installed.form == InstallForm::PortableZip {
        let folder = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf));
        match folder.map(|f| portable::recover(&f)) {
            Some(Recovery::Completed) => shared.log.info("update: finished an interrupted portable update"),
            Some(Recovery::RolledBack) => {
                shared.log.warn("update: undid an interrupted portable update; the program is unchanged");
                failure = failure.or(Some(UpdateError::Interrupted));
            }
            Some(Recovery::Failed(e)) => {
                shared.log.warn(&format!("update: an interrupted portable update could not be undone: {e}"));
                failure = Some(UpdateError::Unrecovered);
            }
            Some(Recovery::None) | None => {}
        }
    }
    remove_installed(&staging, &installed.version);
    let mut wb = shared.workbench.lock().unwrap();
    wb.set_update_installation(installed, staging);
    if let Some(error) = failure {
        shared
            .log
            .warn(&format!("update: the previous run's update did not happen ({error:?})"));
        wb.update_handover_failed_last_run(error);
    }
}

/// Removes the staging folders of releases not newer than this program: they are installed.
fn remove_installed(staging: &Path, version: &semver::Version) {
    let Ok(entries) = std::fs::read_dir(staging) else {
        return;
    };
    for entry in entries.flatten() {
        let installed = entry
            .file_name()
            .to_str()
            .and_then(|n| semver::Version::parse(n).ok())
            .is_some_and(|v| v <= *version);
        if installed && entry.file_type().is_ok_and(|t| t.is_dir()) {
            let _ = std::fs::remove_dir_all(entry.path());
        }
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
    if job.form == InstallForm::PortableZip {
        drop(bytes);
        // The helper replaces the files after the normal exit and reopens the program.
        match prepare_portable(app, &job) {
            Ok(handover) => {
                *PORTABLE.lock().unwrap() = Some(handover);
                app.exit(0);
            }
            Err((error, reason)) => failed(error, reason),
        }
        return;
    }
    if let Err(e) = update.install(&bytes) {
        return failed(UpdateError::Install, e.to_string());
    }
    shared.log.info("update: installed; restarting");
    app.request_restart();
}

/// The end of the program's exit: starts a Windows installation or the portable update's helper
/// waiting for it, which takes over; the process ends here. When it cannot start, the program
/// opens again and says so (Java keeps its window and shows the failure; this program has
/// already closed).
pub fn finish_on_exit(app: &AppHandle, shared: &State) {
    if let Some(handover) = PORTABLE.lock().unwrap().take() {
        return start_helper(app, shared, handover);
    }
    let Some((update, bytes)) = PENDING.lock().unwrap().take() else {
        return;
    };
    shared.log.info("update: starting the installer");
    let Err(e) = update.install(&bytes) else {
        return;
    };
    shared.log.warn(&format!("update installer did not start: {e}"));
    handover_failed(app, shared, "install");
}

/// The handover did not start after the program's exit: the program opens again and says so.
fn handover_failed(app: &AppHandle, shared: &State, reason: &str) {
    if let Some(marker) = HANDOVER_FAILED.get() {
        let _ = std::fs::write(marker, reason);
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

// ------------------------------------------------------------------ portable zip

/// The command-line flag that makes this program the portable update's helper.
const HELPER_FLAG: &str = "--lizzieyzy-portable-update";
/// The helper's notes for the next start's log, in the update staging folder.
const HELPER_LOG: &str = "portable-helper.log";
/// How long the helper waits for the program to close (Java `waitForProcessExit`).
const EXIT_WAIT: Duration = Duration::from_secs(30);

/// A portable update waiting for the program's exit.
struct PortableHandover {
    /// A copy of this program that applies the update.
    helper: PathBuf,
    folder: PathBuf,
    program: OsString,
}

static PORTABLE: Mutex<Option<PortableHandover>> = Mutex::new(None);

fn portable_error(failure: PortableFailure) -> UpdateError {
    match failure {
        PortableFailure::Permission => UpdateError::Permission,
        PortableFailure::InUse => UpdateError::InUse,
        PortableFailure::Unrecovered => UpdateError::Unrecovered,
        PortableFailure::Package | PortableFailure::Io => UpdateError::Install,
    }
}

/// The helper's result as the handover marker records it.
fn marker_reason(failure: PortableFailure) -> &'static str {
    match portable_error(failure) {
        UpdateError::Permission => "permission",
        UpdateError::InUse => "in-use",
        UpdateError::Unrecovered => "unrecovered",
        _ => "install",
    }
}

fn marker_error(reason: &str) -> UpdateError {
    match reason.trim() {
        "permission" => UpdateError::Permission,
        "in-use" => UpdateError::InUse,
        "unrecovered" => UpdateError::Unrecovered,
        _ => UpdateError::Install,
    }
}

/// 立即更新 on the portable zip, while the program runs: unpacks the verified package into the
/// program folder's work folder and copies this program as the helper. Nothing in the folder
/// changes yet; a folder that cannot be written fails here.
fn prepare_portable(app: &AppHandle, job: &UpdateInstallJob) -> Result<PortableHandover, (UpdateError, String)> {
    let program = std::env::current_exe().map_err(|e| (UpdateError::Install, e.to_string()))?;
    let (Some(folder), Some(name)) = (program.parent(), program.file_name()) else {
        return Err((UpdateError::Install, format!("no program folder for {}", program.display())));
    };
    let binary = format!(
        "{}{}",
        app.config()
            .main_binary_name
            .clone()
            .unwrap_or_else(|| app.package_info().crate_name.to_string()),
        std::env::consts::EXE_SUFFIX
    );
    portable::prepare(
        &job.path,
        folder,
        &name.to_string_lossy(),
        &binary,
        &job.offer.release.version.to_string(),
    )
    .map_err(|e| (portable_error(e.failure), e.to_string()))?;
    let helper = job
        .path
        .with_file_name(format!("lizzieyzy-update-helper{}", std::env::consts::EXE_SUFFIX));
    if let Err(e) = std::fs::copy(&program, &helper) {
        portable::discard(folder);
        return Err((UpdateError::Install, format!("the helper could not be copied: {e}")));
    }
    Ok(PortableHandover {
        helper,
        folder: folder.to_path_buf(),
        program: name.to_os_string(),
    })
}

/// The end of the program's exit: starts the helper, which waits for this process to end,
/// replaces the files and opens the program again.
fn start_helper(app: &AppHandle, shared: &State, handover: PortableHandover) {
    let Some(staging) = HANDOVER_FAILED.get().and_then(|m| m.parent()) else {
        return;
    };
    shared.log.info("update: starting the portable update helper");
    let mut command = Command::new(&handover.helper);
    command
        .arg(HELPER_FLAG)
        .arg(&handover.folder)
        .arg(&handover.program)
        .arg(std::process::id().to_string())
        .arg(staging)
        .current_dir(staging);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS: no console of its own.
        command.creation_flags(0x0000_0008);
    }
    if let Err(e) = command.spawn() {
        shared.log.warn(&format!("update: the portable update helper did not start: {e}"));
        portable::discard(&handover.folder);
        handover_failed(app, shared, "install");
    }
}

/// When this process was started as the portable update's helper: applies the update and
/// returns the exit code. The program's own start is never reached.
pub fn helper() -> Option<i32> {
    let mut args = std::env::args_os().skip(1);
    if args.next()? != HELPER_FLAG {
        return None;
    }
    let args: Vec<OsString> = args.collect();
    let [folder, program, parent, staging] = args.as_slice() else {
        return Some(2);
    };
    let Some(parent) = parent.to_str().and_then(|p| p.parse::<u32>().ok()) else {
        return Some(2);
    };
    let (folder, staging) = (PathBuf::from(folder), PathBuf::from(staging));
    Some(run_helper(&folder, program, parent, &staging))
}

fn run_helper(folder: &Path, program: &OsStr, parent: u32, staging: &Path) -> i32 {
    let mut log = std::fs::File::create(staging.join(HELPER_LOG)).ok();
    let mut note = |text: &str| {
        if let Some(file) = log.as_mut() {
            let _ = writeln!(file, "{text}");
        }
    };
    let failed = |note: &mut dyn FnMut(&str), failure: PortableFailure, detail: &str| {
        note(&format!("failed ({failure:?}): {detail}"));
        let _ = std::fs::write(staging.join("handover-failed"), marker_reason(failure));
    };
    note(&format!("helper for {} in {}", program.to_string_lossy(), folder.display()));
    let claim = match portable::claim(folder) {
        Ok(claim) => claim,
        Err(e) => {
            failed(&mut note, e.failure, &e.detail);
            reopen(folder, program, &mut note);
            return 1;
        }
    };
    note(&format!("version {}", claim.version()));
    if !wait_for_exit(parent, EXIT_WAIT) {
        // The program is still open: nothing was changed, and it is not opened a second time.
        failed(&mut note, PortableFailure::InUse, "the program did not close");
        return 1;
    }
    #[cfg(feature = "acceptance")]
    let stop_after = std::env::var("LIZZIEYZY_TAURI_UPDATE_HELPER_STOP_AFTER")
        .ok()
        .and_then(|n| n.parse::<usize>().ok());
    let mut count = 0usize;
    let result = claim.apply(&mut |file| {
        count += 1;
        note(&format!("replaced {file}"));
        // Acceptance builds only: the helper stops as if the computer had, mid-way.
        #[cfg(feature = "acceptance")]
        if stop_after == Some(count) {
            std::process::exit(9);
        }
    });
    match result {
        Ok(()) => note(&format!("done: {count} files")),
        Err(e) => failed(&mut note, e.failure, &e.detail),
    }
    reopen(folder, program, &mut note);
    0
}

fn reopen(folder: &Path, program: &OsStr, note: &mut dyn FnMut(&str)) {
    if let Err(e) = Command::new(folder.join(program)).current_dir(folder).spawn() {
        note(&format!("the program could not open: {e}"));
    }
}

/// Waits up to `timeout` for process `pid` to end; true when it has.
#[cfg(windows)]
fn wait_for_exit(pid: u32, timeout: Duration) -> bool {
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
    // SAFETY: the handle is used for this wait only and closed afterwards.
    unsafe {
        let Ok(handle) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) else {
            // The process has already ended.
            return true;
        };
        let ended = WaitForSingleObject(handle, timeout.as_millis() as u32) == WAIT_OBJECT_0;
        let _ = CloseHandle(handle);
        ended
    }
}

/// Waits up to `timeout` for process `pid` to end (or to be a zombie); true when it has.
#[cfg(target_os = "linux")]
fn wait_for_exit(pid: u32, timeout: Duration) -> bool {
    let end = std::time::Instant::now() + timeout;
    loop {
        let running = std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .ok()
            .and_then(|s| s.rsplit_once(')').map(|(_, rest)| rest.trim_start().starts_with(|c| c != 'Z')))
            .unwrap_or(false);
        if !running {
            return true;
        }
        if std::time::Instant::now() >= end {
            return false;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// The portable zip exists only on Windows (and is exercised on Linux in acceptance builds).
#[cfg(not(any(windows, target_os = "linux")))]
fn wait_for_exit(_pid: u32, _timeout: Duration) -> bool {
    true
}

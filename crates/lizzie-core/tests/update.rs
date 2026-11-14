//! 检查更新 through the operations the page, the offer window and the platform adapter use: the
//! release manifest, the check, package preparation, installation authority and the Java import.
use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use lizzie_core::api::{
    ImportReason, ImportStatus, ImportTarget, UpdateCommand, UpdateDownloadPhase, UpdateError, UpdateOrigin,
    UpdateOutcome, UpdateView,
};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::java_import::{self, JavaSource};
use lizzie_core::resources::{Fetch, FetchResponse, ResourceError, ResourceResult};
use lizzie_core::settings::Storage;
use lizzie_core::update::{
    discover, parse_release, CheckOutcome, CheckResult, FailureKind, InstallForm, InstalledApp, Selection,
    UpdateChannel, UpdateSettings, UpdateSource, GITHUB_BETA_MANIFEST, GITHUB_STABLE_MANIFEST,
};
use lizzie_core::workbench::{UpdateDownloadJob, UpdateStep, UpdateTask, Workbench};
use serde_json::json;

struct NoEngine;
impl Launcher for NoEngine {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        Err(std::io::Error::other("not started"))
    }
}

fn workbench(root: &Path) -> Workbench {
    Workbench::new(
        Storage::in_dirs(&root.join("config"), &root.join("data")),
        Some("zh-CN"),
        Box::new(NoEngine),
        Vec::new(),
    )
}

fn installed(form: InstallForm) -> InstalledApp {
    let (os, arch) = match form {
        InstallForm::Nsis | InstallForm::PortableZip => ("windows", "x86_64"),
        InstallForm::App => ("darwin", "aarch64"),
        _ => ("linux", "x86_64"),
    };
    InstalledApp {
        version: semver::Version::new(0, 1, 0),
        os: os.into(),
        arch: arch.into(),
        form,
        signing_configured: true,
    }
}

fn entry(name: &str) -> serde_json::Value {
    json!({"url": format!("https://example.test/{name}"), "signature": "c2lnbmF0dXJl", "size": 1234})
}

/// A release manifest with entries for Windows NSIS, macOS x86_64 and the Linux AppImage.
fn manifest(version: &str, prerelease: bool) -> String {
    json!({
        "version": version,
        "prerelease": prerelease,
        "notesUrl": format!("https://example.test/releases/{version}"),
        "platforms": {
            "windows-x86_64-nsis": entry("setup.exe"),
            "darwin-x86_64-app": entry("app.tar.gz"),
            "linux-x86_64-appimage": entry("app.AppImage"),
        },
    })
    .to_string()
}

/// Manifests by address; an address not listed is unreachable.
#[derive(Default)]
struct Site(HashMap<String, String>);

impl Site {
    fn with(mut self, url: &str, body: String) -> Self {
        self.0.insert(url.into(), body);
        self
    }
}

impl Fetch for Site {
    fn get(&self, url: &str, _: Option<u64>) -> ResourceResult<FetchResponse> {
        let body = self
            .0
            .get(url)
            .ok_or_else(|| ResourceError::Invalid(format!("{url} unreachable")))?;
        Ok(FetchResponse {
            status: 200,
            total_length: Some(body.len() as u64),
            body: Box::new(Cursor::new(body.clone().into_bytes())),
        })
    }
}

const STABLE: Selection = Selection {
    channel: UpdateChannel::Stable,
    source: UpdateSource::Official,
};
const BETA: Selection = Selection {
    channel: UpdateChannel::Beta,
    source: UpdateSource::Official,
};

fn offered_version(result: &CheckResult) -> Option<String> {
    match &result.outcome {
        CheckOutcome::Offer(offer) => Some(offer.release.version.to_string()),
        _ => None,
    }
}

#[test]
fn a_manifest_entry_without_signature_size_or_valid_addresses_is_refused() {
    let release = parse_release(&manifest("v0.2.0", false)).unwrap();
    assert_eq!(release.version, semver::Version::new(0, 2, 0));
    assert_eq!(
        release.notes_url.as_deref(),
        Some("https://example.test/releases/v0.2.0")
    );
    let broken = [
        json!({"version": "0.2.0", "platforms": {"linux-x86_64-appimage": {"url": "https://e.test/a", "size": 1}}}),
        json!({"version": "0.2.0", "platforms": {"linux-x86_64-appimage": {"url": "https://e.test/a", "signature": "s", "size": 0}}}),
        json!({"version": "0.2.0", "platforms": {"linux-x86_64-appimage": {"url": "file:///a", "signature": "s", "size": 1}}}),
        json!({"version": "0.2.0", "platforms": {"linux-x86_64-appimage": {"url": "https://e.test/a", "signature": "s", "size": 1, "mirrors": ["ftp://m/a"]}}}),
        json!({"version": "0.2.0", "notesUrl": "javascript:alert(1)", "platforms": {"linux-x86_64-appimage": entry("a")}}),
        json!({"version": "0.2.0", "prerelease": "yes", "platforms": {"linux-x86_64-appimage": entry("a")}}),
        json!({"version": "0.2.0", "platforms": {}}),
        json!({"version": "latest", "platforms": {"linux-x86_64-appimage": entry("a")}}),
    ];
    for text in broken {
        assert!(parse_release(&text.to_string()).is_err(), "{text}");
    }
}

#[test]
fn a_build_that_cannot_update_itself_reads_nothing() {
    // An empty site: any read would fail the check instead of giving these answers.
    let site = Site::default();
    let mut unsigned = installed(InstallForm::AppImage);
    unsigned.signing_configured = false;
    for (app, expected) in [
        (installed(InstallForm::Unpackaged), CheckOutcome::UnavailableBuild),
        (unsigned, CheckOutcome::SigningNotConfigured),
        (installed(InstallForm::PortableZip), CheckOutcome::UnsupportedForm),
        (installed(InstallForm::Deb), CheckOutcome::UnsupportedForm),
    ] {
        assert_eq!(discover(&app, STABLE, &site).outcome, expected, "{:?}", app.form);
    }
}

#[test]
fn the_stable_channel_offers_the_package_of_this_installation_only() {
    let site = Site::default().with(GITHUB_STABLE_MANIFEST, manifest("0.2.0", false));
    let result = discover(&installed(InstallForm::Nsis), STABLE, &site);
    let CheckOutcome::Offer(offer) = &result.outcome else {
        panic!("{result:?}")
    };
    assert_eq!(
        (offer.key.as_str(), offer.asset.file_name()),
        ("windows-x86_64-nsis", "setup.exe".to_string())
    );
    // The manifest has an Intel package only: an Apple Silicon Mac gets none.
    assert_eq!(
        discover(&installed(InstallForm::App), STABLE, &site).outcome,
        CheckOutcome::NoPackage
    );

    let same = Site::default().with(GITHUB_STABLE_MANIFEST, manifest("0.1.0", false));
    assert_eq!(
        discover(&installed(InstallForm::Nsis), STABLE, &same).outcome,
        CheckOutcome::NoUpdate
    );
    // 正式 never offers a test release, even a newer one.
    let test = Site::default().with(GITHUB_STABLE_MANIFEST, manifest("0.3.0-beta.1", true));
    assert_eq!(
        discover(&installed(InstallForm::Nsis), STABLE, &test).outcome,
        CheckOutcome::NoUpdate
    );
    assert_eq!(
        discover(&installed(InstallForm::Nsis), STABLE, &Site::default()).outcome,
        CheckOutcome::Failed(FailureKind::Fetch)
    );
}

#[test]
fn the_beta_channel_takes_the_newer_verified_candidate_and_reports_a_missing_one() {
    let app = installed(InstallForm::AppImage);
    let both = Site::default()
        .with(GITHUB_STABLE_MANIFEST, manifest("0.2.0", false))
        .with(GITHUB_BETA_MANIFEST, manifest("0.3.0-beta.1", true));
    assert_eq!(
        offered_version(&discover(&app, BETA, &both)).as_deref(),
        Some("0.3.0-beta.1")
    );

    let stable_newer = Site::default()
        .with(GITHUB_STABLE_MANIFEST, manifest("0.3.0", false))
        .with(GITHUB_BETA_MANIFEST, manifest("0.3.0-beta.1", true));
    assert_eq!(
        offered_version(&discover(&app, BETA, &stable_newer)).as_deref(),
        Some("0.3.0")
    );

    let beta_missing = Site::default().with(GITHUB_STABLE_MANIFEST, manifest("0.2.0", false));
    let result = discover(&app, BETA, &beta_missing);
    assert_eq!(offered_version(&result).as_deref(), Some("0.2.0"));
    assert_eq!(
        (result.stable_failure, result.beta_failure),
        (None, Some(FailureKind::Fetch))
    );

    // A beta pointer at a non-test release is not trusted as the beta candidate.
    let mislabelled = Site::default()
        .with(GITHUB_STABLE_MANIFEST, manifest("0.2.0", false))
        .with(GITHUB_BETA_MANIFEST, manifest("0.9.0", false));
    let result = discover(&app, BETA, &mislabelled);
    assert_eq!(offered_version(&result).as_deref(), Some("0.2.0"));
    assert_eq!(result.beta_failure, Some(FailureKind::InvalidCandidate));

    let result = discover(&app, BETA, &Site::default());
    assert_eq!(result.outcome, CheckOutcome::Failed(FailureKind::Fetch));
    assert_eq!(
        (result.stable_failure, result.beta_failure),
        (Some(FailureKind::Fetch), Some(FailureKind::Fetch))
    );
}

// ------------------------------------------------------------------ the coordinator

fn offer_result(version: &str) -> CheckResult {
    let site = Site::default().with(GITHUB_STABLE_MANIFEST, manifest(version, false));
    discover(&installed(InstallForm::AppImage), STABLE, &site)
}

fn started(dir: &Path, settings: Option<UpdateSettings>) -> Workbench {
    if let Some(settings) = settings {
        // Saved in an earlier run.
        let mut wb = workbench(dir);
        wb.update(UpdateCommand::Settings { settings }).unwrap();
    }
    let mut wb = workbench(dir);
    wb.set_update_installation(installed(InstallForm::AppImage), dir.join("cache/updates"));
    wb
}

fn view(wb: &mut Workbench) -> UpdateView {
    wb.view(false).update
}

fn check_id(step: UpdateStep) -> u64 {
    match step.task {
        Some(UpdateTask::Check(job)) => job.id,
        other => panic!("expected a check, got {other:?}"),
    }
}

fn download(step: UpdateStep) -> UpdateDownloadJob {
    match step.task {
        Some(UpdateTask::Download(job)) => job,
        other => panic!("expected a download, got {other:?}"),
    }
}

fn offer_id(wb: &mut Workbench) -> u64 {
    view(wb).result.and_then(|r| r.offer).expect("an offer").id
}

const AUTO: UpdateSettings = UpdateSettings {
    auto_check: true,
    auto_download: false,
    channel: UpdateChannel::Stable,
    source: UpdateSource::Official,
};

#[test]
fn turning_on_the_automatic_check_applies_from_the_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = started(dir.path(), None);
    assert!(wb.update_application_ready().task.is_none(), "off by default");
    wb.update(UpdateCommand::Settings { settings: AUTO }).unwrap();
    let v = view(&mut wb);
    assert!(
        v.settings.auto_check && !v.auto_check_active,
        "saved, shown as 下次启动生效"
    );
    assert!(wb.update_application_ready().task.is_none());
    drop(wb);

    let mut wb = started(dir.path(), None);
    assert!(view(&mut wb).auto_check_active);
    let id = check_id(wb.update_application_ready());
    assert!(
        wb.update_application_ready().task.is_none(),
        "one automatic check per run"
    );
    assert_eq!(view(&mut wb).origin, Some(UpdateOrigin::Automatic));
    // The automatic offer is shown once without anything downloading.
    let step = wb.complete_update_check(id, offer_result("0.2.0"));
    assert!(step.task.is_none());
    let v = view(&mut wb);
    assert_eq!((v.remind, v.download.phase), (1, UpdateDownloadPhase::None));
}

#[test]
fn automatic_download_prepares_but_only_update_now_installs() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = started(
        dir.path(),
        Some(UpdateSettings {
            auto_download: true,
            ..AUTO
        }),
    );
    let id = check_id(wb.update_application_ready());
    let job = download(wb.complete_update_check(id, offer_result("0.2.0")));
    assert_eq!(job.dir, dir.path().join("cache/updates/0.2.0"));
    assert_eq!(view(&mut wb).remind, 0, "shown once prepared");
    assert!(wb.update_download_progress(job.id, 600, Some(1234)));
    let v = view(&mut wb).download;
    assert_eq!(
        (v.phase, v.started, v.bytes, v.total),
        (UpdateDownloadPhase::Downloading, true, 600, 1234)
    );

    let package = job.dir.join("app.AppImage");
    let step = wb.complete_update_download(job.id, Ok(package.clone()));
    assert!(step.task.is_none(), "an automatic download never installs");
    let v = view(&mut wb);
    assert_eq!(
        (v.download.phase, v.remind, v.installing),
        (UpdateDownloadPhase::Ready, 1, false)
    );

    // 立即更新 for an offer that is no longer current does nothing.
    let offer = offer_id(&mut wb);
    assert!(wb
        .update(UpdateCommand::Update { offer: offer + 1 })
        .unwrap()
        .task
        .is_none());
    let step = wb.update(UpdateCommand::Update { offer }).unwrap();
    let Some(UpdateTask::Install(install)) = step.task else {
        panic!("expected the installation")
    };
    assert_eq!((install.form, install.path), (InstallForm::AppImage, package));
    let v = view(&mut wb);
    assert!(v.installing && v.busy);
}

#[test]
fn update_now_downloads_pauses_resumes_and_installs() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = started(dir.path(), None);
    let id = check_id(wb.update(UpdateCommand::Check).unwrap());
    assert!(
        wb.update(UpdateCommand::Check).unwrap().task.is_none(),
        "one check at a time"
    );
    wb.complete_update_check(id, offer_result("0.2.0"));
    assert_eq!(view(&mut wb).remind, 1, "a manual check's offer is shown");

    let offer = offer_id(&mut wb);
    let job = download(wb.update(UpdateCommand::Update { offer }).unwrap());
    assert!(!view(&mut wb).download.started, "准备下载... until progress arrives");
    wb.update_download_progress(job.id, 100, Some(1234));
    wb.update(UpdateCommand::TogglePause).unwrap();
    assert!(job.cancel.load(Ordering::Acquire), "pausing stops the transfer");
    assert!(
        !wb.update_download_progress(job.id, 200, Some(1234)),
        "late progress is ignored"
    );
    wb.complete_update_download(job.id, Err(None));
    assert_eq!(view(&mut wb).download.phase, UpdateDownloadPhase::Paused);
    assert!(view(&mut wb).busy, "a paused download still holds 检查更新");

    let resumed = download(wb.update(UpdateCommand::TogglePause).unwrap());
    assert_eq!(
        (resumed.dir.clone(), resumed.spec.file_name.clone()),
        (job.dir.clone(), job.spec.file_name.clone())
    );
    assert!(
        wb.complete_update_download(job.id, Ok(PathBuf::from("stale")))
            .task
            .is_none(),
        "an old transfer's result is dropped"
    );
    wb.update_download_verifying(resumed.id);
    assert!(view(&mut wb).download.verifying);
    let step = wb.complete_update_download(resumed.id, Ok(resumed.dir.join("app.AppImage")));
    assert!(
        matches!(step.task, Some(UpdateTask::Install(_))),
        "立即更新 installs once prepared"
    );

    // The handover failed: the package stays ready for another try.
    assert!(wb.update_install_failed(UpdateError::Install));
    let v = view(&mut wb);
    assert_eq!(
        (v.installing, v.download.phase, v.download.error),
        (false, UpdateDownloadPhase::Ready, Some(UpdateError::Install))
    );
    assert!(matches!(
        wb.update(UpdateCommand::Update { offer }).unwrap().task,
        Some(UpdateTask::Install(_))
    ));
    // A package that no longer verifies is discarded instead.
    assert!(wb.update_install_failed(UpdateError::Integrity));
    assert_eq!(view(&mut wb).download.phase, UpdateDownloadPhase::Failed);
}

#[test]
fn a_cancelled_or_failed_download_can_be_retried() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = started(dir.path(), None);
    let id = check_id(wb.update(UpdateCommand::Check).unwrap());
    wb.complete_update_check(id, offer_result("0.2.0"));
    let offer = offer_id(&mut wb);
    let job = download(wb.update(UpdateCommand::Update { offer }).unwrap());
    wb.update(UpdateCommand::Cancel).unwrap();
    assert!(job.cancel.load(Ordering::Acquire));
    assert!(
        wb.update(UpdateCommand::Retry).unwrap().task.is_none(),
        "not until the transfer has stopped"
    );
    wb.complete_update_download(job.id, Err(None));
    assert_eq!(view(&mut wb).download.phase, UpdateDownloadPhase::Cancelled);

    let retry = download(wb.update(UpdateCommand::Retry).unwrap());
    wb.complete_update_download(retry.id, Err(Some(UpdateError::Integrity)));
    let v = view(&mut wb).download;
    assert_eq!(
        (v.phase, v.error),
        (UpdateDownloadPhase::Failed, Some(UpdateError::Integrity))
    );
    assert!(!view(&mut wb).busy);
    // 立即更新 asked for this preparation, so the retried one installs when ready.
    let again = download(wb.update(UpdateCommand::Retry).unwrap());
    assert!(matches!(
        wb.complete_update_download(again.id, Ok(again.dir.join("p"))).task,
        Some(UpdateTask::Install(_))
    ));
}

#[test]
fn turning_off_the_automatic_options_withdraws_this_runs_work_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = started(
        dir.path(),
        Some(UpdateSettings {
            auto_download: true,
            ..AUTO
        }),
    );
    let id = check_id(wb.update_application_ready());
    let job = download(wb.complete_update_check(id, offer_result("0.2.0")));

    wb.update(UpdateCommand::Settings {
        settings: UpdateSettings {
            auto_check: false,
            auto_download: true,
            ..AUTO
        },
    })
    .unwrap();
    assert!(job.cancel.load(Ordering::Acquire));
    let v = view(&mut wb);
    assert!(
        !v.settings.auto_download,
        "自动下载更新 goes off with 启动时自动检查更新"
    );
    assert!(!v.auto_check_active && v.result.is_none());
    // The withdrawn transfer's late success prepares nothing.
    wb.complete_update_download(job.id, Ok(job.dir.join("p")));
    let v = view(&mut wb);
    assert_eq!((v.download.phase, v.remind), (UpdateDownloadPhase::None, 0));

    // A channel change withdraws a running automatic check: its result is dropped.
    let dir = tempfile::tempdir().unwrap();
    let mut wb = started(dir.path(), Some(AUTO));
    let id = check_id(wb.update_application_ready());
    wb.update(UpdateCommand::Settings {
        settings: UpdateSettings {
            channel: UpdateChannel::Beta,
            ..AUTO
        },
    })
    .unwrap();
    wb.complete_update_check(id, offer_result("0.2.0"));
    assert!(view(&mut wb).result.is_none());
    assert!(!view(&mut wb).busy);
}

#[test]
fn a_manual_check_is_not_withdrawn_and_old_check_results_are_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = started(dir.path(), Some(AUTO));
    let auto = check_id(wb.update_application_ready());
    assert!(wb
        .complete_update_check(auto + 100, offer_result("0.2.0"))
        .task
        .is_none());
    assert!(view(&mut wb).checking, "an unknown task's result is dropped");
    wb.complete_update_check(
        auto,
        CheckResult {
            outcome: CheckOutcome::NoUpdate,
            stable_failure: None,
            beta_failure: None,
        },
    );
    let manual = check_id(wb.update(UpdateCommand::Check).unwrap());
    wb.update(UpdateCommand::Settings {
        settings: UpdateSettings {
            auto_check: false,
            ..AUTO
        },
    })
    .unwrap();
    wb.complete_update_check(manual, offer_result("0.2.0"));
    let v = view(&mut wb);
    assert_eq!(v.result.map(|r| r.outcome), Some(UpdateOutcome::Offer));
    assert_eq!(v.origin, Some(UpdateOrigin::Manual));
}

#[test]
fn the_beta_choice_keeps_the_saved_source() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = started(dir.path(), None);
    let github = UpdateSettings {
        source: UpdateSource::Github,
        ..UpdateSettings::default()
    };
    wb.update(UpdateCommand::Settings { settings: github }).unwrap();
    wb.update(UpdateCommand::Settings {
        settings: UpdateSettings {
            channel: UpdateChannel::Beta,
            source: UpdateSource::Official,
            ..github
        },
    })
    .unwrap();
    let s = view(&mut wb).settings;
    assert_eq!((s.channel, s.source), (UpdateChannel::Beta, UpdateSource::Github));
    drop(wb);
    assert_eq!(view(&mut started(dir.path(), None)).settings, s, "saved");
}

// ------------------------------------------------------------------ Java import

fn java_dir(root: &Path, config: serde_json::Value) -> PathBuf {
    let java = root.join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), config.to_string()).unwrap();
    java
}

#[test]
fn java_update_settings_come_over_with_javas_reading() {
    let dir = tempfile::tempdir().unwrap();
    let java = java_dir(
        dir.path(),
        json!({"ui": {"auto-check-update": true, "auto-download-update": true, "update-channel": " BETA ", "update-source": "GitHub"}}),
    );
    let mut wb = workbench(dir.path());
    let p = wb.preview_java_import(java_import::read(&JavaSource::new(&java, None)).unwrap());
    for key in [
        "ui.auto-check-update",
        "ui.auto-download-update",
        "ui.update-channel",
        "ui.update-source",
    ] {
        let it = p.items.iter().find(|i| i.key == key).unwrap();
        assert_eq!(
            (it.status, it.target),
            (ImportStatus::Imported, Some(ImportTarget::Update)),
            "{key}"
        );
    }
    wb.apply_java_import(p.id, &[], false, false, true, None)
        .unwrap()
        .unwrap();
    let expected = UpdateSettings {
        auto_check: true,
        auto_download: true,
        channel: UpdateChannel::Beta,
        source: UpdateSource::Github,
    };
    assert_eq!(wb.view(false).update.settings, expected);
    assert!(
        !wb.view(false).update.auto_check_active,
        "imported automatic options apply from the next start"
    );
    assert_eq!(workbench(dir.path()).view(false).update.settings, expected);
}

#[test]
fn java_update_values_outside_its_choices_read_as_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let java = java_dir(
        dir.path(),
        json!({"ui": {"auto-check-update": "maybe", "auto-download-update": true, "update-channel": "nightly", "update-source": 3}}),
    );
    let mut wb = workbench(dir.path());
    let p = wb.preview_java_import(java_import::read(&JavaSource::new(&java, None)).unwrap());
    let it = p.items.iter().find(|i| i.key == "ui.auto-check-update").unwrap();
    assert_eq!(
        (it.status, it.reason),
        (ImportStatus::Incompatible, Some(ImportReason::InvalidValue))
    );
    wb.apply_java_import(p.id, &[], false, false, true, None)
        .unwrap()
        .unwrap();
    // 自动下载更新 needs 启动时自动检查更新; other channels and sources mean 正式 / 官网.
    assert_eq!(wb.view(false).update.settings, UpdateSettings::default());
}

#[test]
fn a_java_import_that_turns_the_automatic_options_off_withdraws_this_runs_work() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = started(dir.path(), Some(UpdateSettings { auto_download: true, ..AUTO }));
    let id = check_id(wb.update_application_ready());
    let job = download(wb.complete_update_check(id, offer_result("0.2.0")));
    let java = java_dir(dir.path(), json!({"ui": {"auto-check-update": false}}));
    let p = wb.preview_java_import(java_import::read(&JavaSource::new(&java, None)).unwrap());
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    assert!(job.cancel.load(Ordering::Acquire), "the automatic download stops");
    wb.complete_update_download(job.id, Ok(job.dir.join("p")));
    let v = view(&mut wb);
    assert!(!v.settings.auto_check && !v.settings.auto_download && !v.auto_check_active);
    assert_eq!((v.download.phase, v.remind, v.result.is_none()), (UpdateDownloadPhase::None, 0, true));
}

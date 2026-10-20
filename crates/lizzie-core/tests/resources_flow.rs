//! Resource lifecycle through Workbench and the same off-lock runner as the shell.
use lizzie_core::{
    api::{ResourceCommand, ResourceItem, ResourceResult},
    engine::{
        gtp::LaunchSpec,
        profile::{EngineProfile, EngineStartup},
        runtime::{EngineIo, InstanceId, Launcher},
    },
    resources as r,
    settings::{NetworkSettings, ProxyMode, Storage},
    workbench::{run_resource_job, ResourceJob, Workbench},
};
use sha2::{Digest, Sha256};
use std::{
    io::{self, Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
struct NoLaunch;
impl Launcher for NoLaunch {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> io::Result<Box<dyn EngineIo>> {
        Err(io::Error::other("not launched in this test"))
    }
}
struct Probe;
impl r::Probe for Probe {
    fn run(&self, program: &str, args: &[String], _: Duration) -> io::Result<r::ProbeOutput> {
        Ok(if args == ["version"] {
            r::ProbeOutput {
                exit_code: Some(0),
                stdout: "KataGo v1.18.2".into(),
                ..Default::default()
            }
        } else if program.contains("nvidia-smi") {
            r::ProbeOutput {
                exit_code: Some(0),
                stdout: "RTX 4070, 8.9, 580.88, 12288".into(),
                ..Default::default()
            }
        } else {
            r::ProbeOutput {
                exit_code: Some(1),
                ..Default::default()
            }
        })
    }
    fn read_file(&self, _: &Path) -> io::Result<Vec<u8>> {
        Err(io::ErrorKind::NotFound.into())
    }
    fn list_dir(&self, _: &Path) -> io::Result<Vec<PathBuf>> {
        Ok(Vec::new())
    }
    fn env_var(&self, _: &str) -> Option<String> {
        None
    }
}
struct Fetch(Vec<u8>);
impl r::Fetch for Fetch {
    fn get(&self, _: &str, start: Option<u64>) -> r::ResourceResult<r::FetchResponse> {
        let offset = start.unwrap_or(0) as usize;
        Ok(r::FetchResponse {
            status: if offset == 0 { 200 } else { 206 },
            total_length: Some(self.0.len() as u64),
            body: Box::new(Cursor::new(self.0[offset..].to_vec())),
        })
    }
}
fn fixture() -> (tempfile::TempDir, Workbench) {
    let d = tempfile::tempdir().unwrap();
    let wb = Workbench::new(
        Storage::in_dirs(&d.path().join("config"), &d.path().join("data")),
        None,
        Box::new(NoLaunch),
        vec![d.path().into()],
    );
    (d, wb)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn archive() -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in [
        ("katago", b"fixture executable".as_slice()),
        ("default_gtp.cfg", b"numSearchThreads=4"),
        ("analysis_example.cfg", b"analysis=1"),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap().into_inner()
}
fn package_job(wb: &mut Workbench, bytes: &[u8]) -> ResourceJob {
    let mut job = wb
        .begin_resource_task(ResourceCommand::Download {
            item: ResourceItem::Package {
                asset: "linux-cpu".into(),
                confirmed: true,
                backend: None,
            },
        })
        .unwrap();
    let a = job.catalog.assets.get_mut("linux-cpu").unwrap();
    a.size_bytes = bytes.len() as u64;
    a.sha256 = digest(bytes);
    a.executable_sha256 = digest(b"fixture executable");
    job
}
fn run(wb: &mut Workbench, job: &ResourceJob, fetch: &dyn r::Fetch) {
    let result = run_resource_job(job, fetch, &Probe, |phase, b, t| {
        wb.resource_progress(job.id, phase, b, t);
    });
    assert!(wb.complete_resource_task(job.id, result));
}
#[test]
fn download_install_inventory_and_one_shot_outcome() {
    let (_d, mut wb) = fixture();
    let bytes = archive();
    let job = package_job(&mut wb, &bytes);
    run(&mut wb, &job, &Fetch(bytes));
    let view = wb.view(false).resources.unwrap();
    assert_eq!(view.inventory.packages.len(), 1);
    assert!(view.inventory.packages[0].usable);
    assert!(view.inventory.gtp_config.is_some());
    assert_eq!(view.last_outcome.unwrap().status, "done");
    assert!(wb.view(false).resources.is_none_or(|v| v.last_outcome.is_none()));
    assert_eq!(wb.take_logs().len(), 1);
    assert!(wb.config().engine.engines.is_empty());
}
#[test]
fn integrity_failure_installs_nothing_and_stale_result_is_dropped() {
    let (_d, mut wb) = fixture();
    let bytes = archive();
    let job = package_job(&mut wb, &bytes);
    let bad = Fetch(vec![0; bytes.len()]);
    run(&mut wb, &job, &bad);
    assert!(wb.resource_snapshot().inventory.packages.is_empty());
    assert_eq!(
        wb.resource_snapshot().last_outcome.unwrap().message_key,
        "AutoSetup.weightChecksumFailed"
    );
    assert!(!wb.complete_resource_task(job.id, Ok(Default::default())));
    assert!(!wb.resource_progress(job.id, "late", 42, None));
    assert!(wb.config().engine.engines.is_empty());
}
struct CancelRead {
    bytes: Cursor<Vec<u8>>,
    cancel: Arc<AtomicBool>,
    reads: u32,
}
impl Read for CancelRead {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.reads += 1;
        if self.reads == 2 {
            self.cancel.store(true, Ordering::Release);
        }
        let n = b.len().min(32);
        self.bytes.read(&mut b[..n])
    }
}
struct CancelFetch {
    bytes: Vec<u8>,
    cancel: Arc<AtomicBool>,
}
impl r::Fetch for CancelFetch {
    fn get(&self, _: &str, _: Option<u64>) -> r::ResourceResult<r::FetchResponse> {
        Ok(r::FetchResponse {
            status: 200,
            total_length: Some(self.bytes.len() as u64),
            body: Box::new(CancelRead {
                bytes: Cursor::new(self.bytes.clone()),
                cancel: self.cancel.clone(),
                reads: 0,
            }),
        })
    }
}
#[test]
fn cancel_retains_partial_and_changes_no_profile() {
    let (_d, mut wb) = fixture();
    let bytes = archive();
    let job = package_job(&mut wb, &bytes);
    let fetch = CancelFetch {
        bytes,
        cancel: job.cancel.clone(),
    };
    run(&mut wb, &job, &fetch);
    let part = job
        .store
        .downloads_dir()
        .join(format!("{}.part", job.catalog.assets["linux-cpu"].asset_name));
    assert!(part.is_file());
    assert!(std::fs::metadata(part).unwrap().len() > 0);
    assert_eq!(wb.resource_snapshot().last_outcome.unwrap().status, "cancelled");
    assert!(wb.config().engine.engines.is_empty());
    assert!(job.store.inventory().unwrap().packages.is_empty());
}
#[test]
fn mismatched_import_refuses_without_install() {
    let (d, mut wb) = fixture();
    let inspect = wb
        .begin_resource_task(ResourceCommand::Inspect { refresh: true })
        .unwrap();
    run(&mut wb, &inspect, &Fetch(Vec::new()));
    let path = d.path().join("wrong.zip");
    std::fs::write(&path, b"not a trusted package").unwrap();
    let job = wb
        .begin_resource_task(ResourceCommand::ImportPackage {
            path: Some(path.to_string_lossy().into()),
            backend: None,
            confirmed: true,
        })
        .unwrap();
    run(&mut wb, &job, &Fetch(Vec::new()));
    let outcome = wb.resource_snapshot().last_outcome.unwrap();
    assert_eq!(outcome.message_key, "AutoSetup.importPackageMismatch");
    assert!(outcome.detail.contains("does not match"));
    assert!(wb.resource_snapshot().inventory.packages.is_empty());
}
fn choice(d: &Path, update: Option<String>) -> r::SetupChoice {
    let exe = d.join("katago");
    let cfg = d.join("gtp.cfg");
    let weight = d.join("model.bin.gz");
    for p in [&exe, &cfg, &weight] {
        std::fs::write(p, b"fixture").unwrap();
    }
    r::SetupChoice {
        asset_id: "linux-cpu".into(),
        executable: exe,
        config: cfg,
        weight,
        model_display_name: "test model".into(),
        model_name: Some("kata1-b18c384-test".into()),
        engine_version: Some("1.18.2".into()),
        experimental_backend: None,
        update_profile_id: update,
        set_as_default: false,
        switch_now: false,
    }
}
fn plan(result: ResourceResult) -> u64 {
    if let ResourceResult::Plan { plan } = result {
        plan.id
    } else {
        panic!("expected profile plan: {result:?}")
    }
}
#[test]
fn plan_apply_adds_managed_and_preserves_unmanaged_command() {
    let (d, mut wb) = fixture();
    let original = EngineProfile {
        id: "user".into(),
        command: "custom gtp".into(),
        ..Default::default()
    };
    wb.save_engine_settings(vec![original.clone()], EngineStartup::Empty)
        .unwrap();
    let id = plan(wb.plan_resource_setup(choice(d.path(), Some("user".into()))));
    assert_eq!(wb.config().engine.engines.len(), 1);
    wb.apply_resource_setup(id, true, false).unwrap();
    assert_eq!(wb.config().engine.engines[0].command, original.command);
    let added = &wb.config().engine.engines[1];
    assert!(r::is_managed(added));
    assert!(added.is_default);
    assert!(added.thread_policy.as_ref().unwrap().initial_setup);
    assert_eq!(added.name, "KataGo (test model)");
}
#[test]
fn stale_plan_refused_then_edited_managed_target_becomes_addition() {
    let (d, mut wb) = fixture();
    let first = plan(wb.plan_resource_setup(choice(d.path(), None)));
    wb.apply_resource_setup(first, false, false).unwrap();
    let target = wb.config().engine.engines[0].id.clone();
    let stale = plan(wb.plan_resource_setup(choice(d.path(), Some(target.clone()))));
    let mut profiles = wb.config().engine.engines.clone();
    profiles[0].command = "user edited command".into();
    wb.save_engine_settings(profiles, EngineStartup::Empty).unwrap();
    assert!(
        matches!(wb.apply_resource_setup(stale,false,false).unwrap(),ResourceResult::Refused{message_key} if message_key=="AutoSetup.planStale")
    );
    let fresh = plan(wb.plan_resource_setup(choice(d.path(), Some(target))));
    wb.apply_resource_setup(fresh, false, false).unwrap();
    assert_eq!(wb.config().engine.engines.len(), 2);
    assert_eq!(wb.config().engine.engines[0].command, "user edited command");
}
#[test]
fn mutual_busy_and_proxy_validation() {
    let (d, mut wb) = fixture();
    let c = choice(d.path(), None);
    let command = r::build_gtp_command(&c.executable, &c.weight, &c.config, None, d.path()).unwrap();
    wb.save_engine_settings(
        vec![EngineProfile {
            id: "engine".into(),
            command,
            ..Default::default()
        }],
        EngineStartup::Empty,
    )
    .unwrap();
    let benchmark = wb.begin_benchmark("engine", false).unwrap();
    assert!(wb
        .begin_resource_task(ResourceCommand::Inspect { refresh: false })
        .is_err());
    wb.stop_benchmark();
    wb.complete_benchmark(benchmark.id, Err("cancelled".into()));
    let resource = wb
        .begin_resource_task(ResourceCommand::Inspect { refresh: false })
        .unwrap();
    assert!(wb.begin_benchmark("engine", false).is_err());
    wb.cancel_resource_task(resource.id);
    wb.complete_resource_task(resource.id, Err(r::ResourceError::Cancelled));
    for (settings, key) in [
        (
            NetworkSettings {
                proxy_mode: ProxyMode::Manual,
                proxy_host: String::new(),
                proxy_port: 80,
            },
            "ConfigDialog2.modern.proxy.hostRequired",
        ),
        (
            NetworkSettings {
                proxy_mode: ProxyMode::Manual,
                proxy_host: "localhost".into(),
                proxy_port: 0,
            },
            "ConfigDialog2.modern.proxy.invalidPort",
        ),
    ] {
        assert!(
            matches!(wb.set_resource_proxy(settings).unwrap(),ResourceResult::Refused{message_key} if message_key==key)
        );
        assert_eq!(wb.config().network.proxy_mode, ProxyMode::Direct);
    }
    let manual = NetworkSettings {
        proxy_mode: ProxyMode::Manual,
        proxy_host: "localhost".into(),
        proxy_port: 8080,
    };
    wb.set_resource_proxy(manual.clone()).unwrap();
    assert_eq!(wb.config().network, manual);
}
#[test]
fn failed_settings_save_does_not_apply_plan() {
    let (d, mut wb) = fixture();
    let id = plan(wb.plan_resource_setup(choice(d.path(), None)));
    let path = wb.storage().config_file.clone();
    std::fs::create_dir_all(&path).unwrap();
    assert!(wb.apply_resource_setup(id, true, false).is_err());
    assert!(wb.config().engine.engines.is_empty());
}

#[test]
fn explicit_verification_and_startup_repair_are_projected() {
    let (d, mut wb) = fixture();
    assert!(wb.resource_snapshot().startup_needs_repair);
    wb.save_engine_settings(Vec::new(), EngineStartup::Empty).unwrap();
    assert!(!wb.resource_snapshot().startup_needs_repair);
    let c = choice(d.path(), None);
    let target = lizzie_core::api::ResourceEngineChoice {
        asset_id: c.asset_id,
        executable: c.executable.to_string_lossy().into_owned(),
        config: c.config.to_string_lossy().into_owned(),
        backend: None,
        update_profile_id: None,
    };
    let job = wb
        .begin_resource_task(ResourceCommand::Verify {
            target: target.clone(),
            weight: Some(c.weight.to_string_lossy().into_owned()),
        })
        .unwrap();
    run(&mut wb, &job, &Fetch(Vec::new()));
    assert_eq!(
        wb.resource_snapshot().verification.unwrap().status,
        r::ValidationStatus::Valid
    );
    assert_eq!(
        wb.resource_snapshot().verification_executable.as_deref(),
        Some(target.executable.as_str())
    );
    assert_eq!(
        wb.resource_verified_version(&target.executable).as_deref(),
        Some("1.18.2")
    );
}

#[test]
fn failed_remote_refresh_preserves_last_catalog() {
    let (_d, mut wb) = fixture();
    let cached = wb.resource_snapshot().online_weights;
    let job = wb.begin_resource_task(ResourceCommand::RefreshRemoteWeights).unwrap();
    run(&mut wb, &job, &Fetch(b"invalid metadata".to_vec()));
    let view = wb.resource_snapshot();
    assert_eq!(view.online_weights, cached);
    assert_eq!(view.remote_status, "AutoSetup.remoteRefreshFailedCached");
    let outcome = view.last_outcome.unwrap();
    assert_eq!(outcome.status, "failed");
    assert_eq!(outcome.message_key, "AutoSetup.remoteRefreshFailedCached");
}

#[test]
fn import_and_cache_cleanup_have_specific_completion_messages() {
    let (directory, mut wb) = fixture();
    let weight = directory.path().join("custom.bin.gz");
    std::fs::write(&weight, b"user supplied model").unwrap();
    let import = wb
        .begin_resource_task(ResourceCommand::ImportWeight {
            path: Some(weight.to_string_lossy().into_owned()),
        })
        .unwrap();
    run(&mut wb, &import, &Fetch(Vec::new()));
    assert_eq!(
        wb.resource_snapshot().last_outcome.unwrap().message_key,
        "AutoSetup.importWeightDone"
    );
    let cache = wb.resource_store().runtime_downloads_dir();
    std::fs::create_dir_all(&cache).unwrap();
    std::fs::write(cache.join("installer.zip"), b"cached").unwrap();
    let clean = wb
        .begin_resource_task(ResourceCommand::CleanNvidiaCache { confirmed: true })
        .unwrap();
    run(&mut wb, &clean, &Fetch(Vec::new()));
    let outcome = wb.resource_snapshot().last_outcome.unwrap();
    assert_eq!(outcome.message_key, "AutoSetup.cleanTensorRtCacheDone");
    assert_eq!(outcome.detail, "6 bytes");
}

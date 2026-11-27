//! Explanations for an engine that cannot start, through the public `Workbench` operations: the
//! Windows path error 206 in an engine's start-up output (Java `EngineOutputDiagnostic`) and a
//! packaged engine refused on too old a macOS (Java `MacOsEngineCompatibility`), with the system
//! version injected.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lizzie_core::api::{
    BatchMode, EngineFailureView, FlashTarget, HumanSlCommand, HumanSlNotice, OpenSource, ResourceCommand,
    ResourceEngineChoice,
};
use lizzie_core::engine::diagnosis::{diagnose_output, HostSystem, StartupDiagnosis};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, Engines, InstanceId, Launcher};
use lizzie_core::formats::{self, sgf};
use lizzie_core::record::RecordFormat;
use lizzie_core::resources as r;
use lizzie_core::settings::{self, AnalysisEngineSettings, Config, HumanSlTrainingSettings, Storage};
use lizzie_core::workbench::{run_resource_job, OpenOrigin, Workbench};

/// Per started process: its instance, program and the commands it was sent and not yet answered.
#[derive(Clone, Default)]
struct World(Arc<Mutex<Vec<(InstanceId, String, Vec<String>)>>>);

impl World {
    fn launched(&self) -> Vec<String> {
        self.0.lock().unwrap().iter().map(|p| p.1.clone()).collect()
    }
    fn last(&self) -> InstanceId {
        self.0.lock().unwrap().last().unwrap().0
    }
    fn take(&self, id: InstanceId) -> Vec<String> {
        let mut w = self.0.lock().unwrap();
        w.iter_mut()
            .find(|p| p.0 == id)
            .map(|p| std::mem::take(&mut p.2))
            .unwrap_or_default()
    }
}

struct Io(InstanceId, World);
impl EngineIo for Io {
    fn send(&mut self, line: &str) -> io::Result<()> {
        let mut w = (self.1).0.lock().unwrap();
        w.iter_mut().find(|p| p.0 == self.0).unwrap().2.push(line.to_string());
        Ok(())
    }
    fn terminate(self: Box<Self>) {}
}

struct FakeLauncher(World);
impl Launcher for FakeLauncher {
    fn launch(&self, id: InstanceId, spec: &LaunchSpec) -> io::Result<Box<dyn EngineIo>> {
        self.0 .0.lock().unwrap().push((id, spec.program.clone(), Vec::new()));
        Ok(Box::new(Io(id, self.0.clone())))
    }
}

const KATA: &str =
    "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nkata-analyze\nkata-get-rules\nstop";
const CHINESE: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;

/// Answers every outstanding command of `id` as KataGo does until none is left.
fn answer_all(wb: &mut Workbench, world: &World, id: InstanceId) {
    loop {
        let pending = world.take(id);
        if pending.is_empty() {
            return;
        }
        for line in pending {
            let (n, cmd) = line.split_once(' ').unwrap();
            let reply = match cmd.split(' ').next().unwrap() {
                "name" => "KataGo",
                "version" => "1.16.4",
                "list_commands" => KATA,
                "kata-get-rules" => CHINESE,
                _ => "",
            };
            wb.engine_input(id, EngineInput::Stdout(format!("={n} {reply}")));
            wb.engine_input(id, EngineInput::Stdout(String::new()));
        }
    }
}

fn workbench(dir: &Path, world: &World) -> Workbench {
    let mut wb = Workbench::new(
        Storage::in_dirs(&dir.join("config"), &dir.join("data")),
        None,
        Box::new(FakeLauncher(world.clone())),
        Vec::new(),
    );
    wb.set_auto_quick_analyze(false).unwrap();
    wb
}

fn start(wb: &mut Workbench, program: &Path) {
    let profile = EngineProfile {
        name: "KataGo".into(),
        command: format!("{} gtp", program.display()),
        ..EngineProfile::default()
    };
    wb.save_engine_settings(vec![profile], EngineStartup::Default).unwrap();
    wb.start_engines();
}

fn failure(wb: &mut Workbench) -> EngineFailureView {
    wb.view(false).engine_failures.pop().expect("an engine failure")
}

/// A packaged engine as the resource store installs it: the executable beside its receipt.
fn package(dir: &Path, asset_id: &str) -> PathBuf {
    let package = dir.join("packages").join(format!("{asset_id}-0123456789ab"));
    std::fs::create_dir_all(&package).unwrap();
    let receipt = r::ResourceReceipt {
        asset_id: asset_id.into(),
        version: "1.18.2".into(),
        archive_sha256: "0".repeat(64),
        executable_sha256: "0".repeat(64),
        executable: "katago".into(),
        executable_size: 1,
        source: r::ResourceSource::Download,
        installed_at: 0,
        experimental_backend: None,
    };
    std::fs::write(package.join("resource.json"), serde_json::to_vec(&receipt).unwrap()).unwrap();
    std::fs::write(package.join("katago"), b"x").unwrap();
    package.join("katago")
}

fn mac(version: &str) -> HostSystem {
    HostSystem {
        os: "macos".into(),
        version: Some(version.into()),
    }
}

fn too_old(actual: &str) -> Option<StartupDiagnosis> {
    Some(StartupDiagnosis::MacOsTooOld {
        minimum: "15.0".into(),
        actual: actual.into(),
    })
}

const PATH_ERROR: &str = "Could not load library cudnn_engines_precompiled64_9.dll. Error code 206";

#[test]
fn error_206_in_the_start_up_output_explains_a_path_too_long() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    start(&mut wb, Path::new("/opt/katago"));
    let id = world.last();
    wb.engine_input(id, EngineInput::Stderr(PATH_ERROR.into()));
    wb.engine_input(id, EngineInput::Exited(Some(1)));
    let f = failure(&mut wb);
    assert!(f.during_startup);
    assert_eq!(f.diagnosis, Some(StartupDiagnosis::PathTooLong));
    // The failure's own message and output stay as they were.
    assert_eq!(f.message, "exit code 1");
    assert_eq!(f.output, vec![PATH_ERROR.to_string()]);

    // Another loader error keeps the plain failure.
    start(&mut wb, Path::new("/opt/other"));
    let id = world.last();
    wb.engine_input(
        id,
        EngineInput::Stderr("Could not load library cudnn64_9.dll. Error code 126".into()),
    );
    wb.engine_input(id, EngineInput::Exited(Some(1)));
    assert_eq!(failure(&mut wb).diagnosis, None);
}

#[test]
fn an_engine_ending_after_start_up_gets_no_start_up_explanation() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    start(&mut wb, Path::new("/opt/katago"));
    let id = world.last();
    answer_all(&mut wb, &world, id);
    assert!(wb.view(false).engine.foreground.is_some_and(|f| f.ready));
    wb.engine_input(id, EngineInput::Stderr(PATH_ERROR.into()));
    wb.engine_input(id, EngineInput::Exited(Some(1)));
    let f = failure(&mut wb);
    assert!(!f.during_startup);
    assert_eq!(f.diagnosis, None);
}

#[test]
fn the_explanation_needs_the_explicit_loader_error_206_of_a_dll_load_failure() {
    for text in [
        PATH_ERROR,
        "LoadLibraryW failed for C:\\a\\b\\nvrtc64_120_0.dll: error code: 206",
        "无法加载 cudnn64_9.dll error code=206",
        // Each clause keeps its own code: the second one is the path error.
        "Could not load library missing.dll. Error code 126; Could not load library long.dll. Error code 206",
    ] {
        assert_eq!(diagnose_output(&[text]), Some(StartupDiagnosis::PathTooLong), "{text}");
    }
    for text in [
        // Path lengths and other numbers are not the error.
        "Could not load library foo.dll. Error code 2060",
        "Could not load library foo.dll. After 206 attempts",
        "Could not load library foo.dll: 206",
        "Could not load library foo.dll. Model code 206",
        "Could not load library \"C:/nested/nested/nested/nested/nested/nested/nested/foo.dll\"",
        // Another clause's code is not borrowed.
        "Could not load library long.dll. Error code 126; Error code 206",
        // A success or a negated failure is no failure.
        "Successfully loaded foo.dll. Error code 206",
        "Did not fail to load foo.dll. Error code 206",
        // An earlier rule decides the clause: a missing DLL, a bad image, an entry point.
        "Failed to load foo.dll: The specified module could not be found. Error code 206",
        "The program can't start because long.dll is missing from your computer. Error code 206",
        "Bad Image: C:\\tools\\corrupt.dll is either not designed to run on Windows. Error code 206",
        "The procedure entry point foo could not be located in the dynamic link library bar.dll. Error code 206",
        // No DLL is named.
        "error code 206",
    ] {
        assert_eq!(diagnose_output(&[text]), None, "{text}");
    }
}

#[test]
fn a_packaged_engine_is_refused_on_a_mac_older_than_its_minimum() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let katago = package(dir.path(), "macos-arm64");
    wb.set_host_system(mac("12.7.6"));
    start(&mut wb, &katago);
    let f = failure(&mut wb);
    assert_eq!(f.diagnosis, too_old("12.7.6"));
    assert!(f.during_startup);
    assert!(world.launched().is_empty(), "no process is started");
}

#[test]
fn the_minimum_or_a_newer_mac_starts_the_packaged_engine() {
    for version in ["15", "15.0", "15.0.1", "15.7", "26.1"] {
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let mut wb = workbench(dir.path(), &world);
        let katago = package(dir.path(), "macos-arm64");
        wb.set_host_system(mac(version));
        start(&mut wb, &katago);
        assert!(wb.view(false).engine_failures.is_empty(), "{version}");
        assert_eq!(world.launched(), vec![katago.display().to_string()], "{version}");
    }
}

#[test]
fn only_packaged_engines_with_a_minimum_are_checked_and_only_on_a_mac() {
    let cases = [
        // The user's own engine: no receipt beside it.
        ("own", None, mac("12.7.6")),
        // A packaged engine whose asset records no minimum.
        ("no minimum", Some("linux-x64"), mac("12.7.6")),
        // Not a Mac.
        (
            "not a mac",
            Some("macos-arm64"),
            HostSystem {
                os: "linux".into(),
                version: Some("6.8".into()),
            },
        ),
        // An unreadable version is not older.
        ("unreadable", Some("macos-arm64"), mac("unknown")),
    ];
    for (name, asset, host) in cases {
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let mut wb = workbench(dir.path(), &world);
        let katago = match asset {
            Some(asset) => package(dir.path(), asset),
            None => dir.path().join("own/katago"),
        };
        wb.set_host_system(host);
        start(&mut wb, &katago);
        assert!(wb.view(false).engine_failures.is_empty(), "{name}");
        assert_eq!(world.launched().len(), 1, "{name}");
    }
}

#[test]
fn an_analysis_process_of_a_refused_engine_is_not_started_either() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut engines = Engines::new(Box::new(FakeLauncher(world.clone())), Vec::new());
    engines.set_host_system(mac("14.6"), r::Catalog::embedded().unwrap());
    let katago = package(dir.path(), "macos-amd64");
    let refused = engines.start_worker(&format!("{} analysis", katago.display()), "AnalysisEngine");
    assert_eq!(refused.unwrap_err().diagnosis, too_old("14.6"));
    assert!(world.launched().is_empty());
}

#[test]
fn a_refused_humansl_engine_reports_the_explanation_without_a_detail() {
    let dir = tempfile::tempdir().unwrap();
    let katago = package(dir.path(), "macos-arm64");
    let storage = Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data"));
    let mut config = Config::default();
    config.analysis_engine.command = format!("{} analysis", katago.display());
    config.analysis_engine.command_customized = true;
    let model = dir.path().join("human.bin.gz");
    std::fs::File::create(&model).unwrap().set_len(1024 * 1024 + 1).unwrap();
    config.human_sl.model_path = Some(model.display().to_string());
    settings::save_json(&storage.config_file, &config).unwrap();
    let world = World::default();
    let mut wb = Workbench::new(storage, None, Box::new(FakeLauncher(world.clone())), Vec::new());
    wb.set_host_system(mac("12.7.6"));
    wb.human_sl(HumanSlCommand::Start {
        settings: HumanSlTrainingSettings::default(),
        from_current: false,
        advanced_visible: true,
        stop_batch: false,
    })
    .unwrap();
    let failed: Vec<_> = wb
        .inspect()
        .human_sl_notices
        .into_iter()
        .filter(|n| matches!(n, HumanSlNotice::StartFailed { .. }))
        .collect();
    assert_eq!(
        failed,
        vec![HumanSlNotice::StartFailed {
            reason: "HumanSlTraining.preparing.failedDetail".into(),
            detail: None,
            diagnosis: too_old("12.7.6"),
        }]
    );
    assert!(world.launched().is_empty());
}

#[test]
fn a_refused_analysis_process_reports_the_explanation_in_the_failure_window() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let katago = package(dir.path(), "macos-arm64");
    wb.set_host_system(mac("12.7.6"));
    // The user's own engine plays; 整盘速览 runs the packaged engine as the analysis process.
    start(&mut wb, Path::new("/opt/katago"));
    let fg = world.last();
    answer_all(&mut wb, &world, fg);
    let settings = AnalysisEngineSettings {
        command: format!("{} analysis", katago.display()),
        reuse_current_engine: false,
        ..AnalysisEngineSettings::default()
    };
    wb.save_analysis_engine_settings(settings, "", true).unwrap().unwrap();
    let t = wb.begin_open(OpenOrigin::File {
        path: "/x.sgf".into(),
        source: OpenSource::Drop,
    });
    assert!(wb.complete_open(
        t,
        Ok((RecordFormat::Sgf, sgf::read("(;SZ[19];B[pd];W[dp];B[qp])").unwrap()))
    ));
    answer_all(&mut wb, &world, fg);
    wb.start_flash_analysis(FlashTarget::WholeGame).unwrap().ok();
    let f = failure(&mut wb);
    assert_eq!(f.engine_name, "AnalysisEngine");
    assert_eq!(f.diagnosis, too_old("12.7.6"));
    assert_eq!(world.launched(), vec!["/opt/katago".to_string()], "no analysis process");
}

/// 整盘速览 and 批量精析 run their own analysis process.
#[derive(Clone, Copy, Debug)]
enum AnalysisRun {
    Overview,
    Batch,
}

/// The user's own engine plays `(;SZ[19];B[pd];W[dp];B[qp])` and `run` analyses it with an
/// analysis process, which prints `line` on stdout or stderr and exits, before or after it
/// answered a query. Returns the failure the view reports for the analysis process.
fn analysis_process_failure(run: AnalysisRun, answered: bool, on_stdout: bool, line: &str) -> EngineFailureView {
    const RECORD: &str = "(;SZ[19];B[pd];W[dp];B[qp])";
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    start(&mut wb, Path::new("/opt/katago"));
    let fg = world.last();
    answer_all(&mut wb, &world, fg);
    let settings = AnalysisEngineSettings {
        command: "/opt/analysis/katago analysis -config /a.cfg".into(),
        reuse_current_engine: false,
        ..AnalysisEngineSettings::default()
    };
    wb.save_analysis_engine_settings(settings, "", true).unwrap().unwrap();
    let t = wb.begin_open(OpenOrigin::File {
        path: "/x.sgf".into(),
        source: OpenSource::Drop,
    });
    assert!(wb.complete_open(t, Ok((RecordFormat::Sgf, sgf::read(RECORD).unwrap()))));
    answer_all(&mut wb, &world, fg);
    let deliver_reads = |wb: &mut Workbench| {
        for (ticket, path, no_capture) in wb.take_batch_reads() {
            assert!(wb.complete_batch_read(ticket, formats::read_file(&path, no_capture)));
        }
    };
    match run {
        AnalysisRun::Overview => {
            assert_eq!(wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(), Ok(true));
        }
        AnalysisRun::Batch => {
            let path = dir.path().join("a.sgf");
            std::fs::write(&path, RECORD).unwrap();
            let files = vec![path.to_string_lossy().into_owned()];
            assert_eq!(wb.open_batch(BatchMode::Flash, files, OpenSource::Chooser), Ok(true));
            deliver_reads(&mut wb);
            assert_eq!(wb.start_batch_flash(-1, -1, 2, false).unwrap(), Ok(true));
            deliver_reads(&mut wb);
        }
    }
    answer_all(&mut wb, &world, fg);
    let worker = world.last();
    assert_eq!(world.launched().last().unwrap(), "/opt/analysis/katago");
    let queries: Vec<serde_json::Value> = world
        .take(worker)
        .iter()
        .map(|q| serde_json::from_str(q).unwrap())
        .collect();
    assert!(queries.len() >= 2, "{run:?}: {queries:?}");
    if answered {
        let reply = serde_json::json!({
            "id": queries[0]["id"],
            "moveInfos": [{"move": "K10", "visits": 2, "winrate": 0.5, "scoreLead": 1.0, "order": 0, "pv": ["K10"]}],
            "rootInfo": {"visits": 2, "winrate": 0.5, "scoreLead": 1.0},
        });
        wb.engine_input(worker, EngineInput::Stdout(reply.to_string()));
    }
    let printed = if on_stdout { EngineInput::Stdout(line.into()) } else { EngineInput::Stderr(line.into()) };
    wb.engine_input(worker, printed);
    wb.engine_input(worker, EngineInput::Exited(Some(1)));
    let f = failure(&mut wb);
    assert_eq!(f.engine_name, "AnalysisEngine", "{run:?}");
    // The failure's own message and output stay as they were.
    assert_eq!(f.message, "exit code 1", "{run:?}");
    assert_eq!(f.output, vec![line.to_string()], "{run:?}");
    f
}

#[test]
fn an_analysis_process_ending_with_error_206_before_any_answer_explains_a_path_too_long() {
    for run in [AnalysisRun::Overview, AnalysisRun::Batch] {
        for on_stdout in [false, true] {
            let f = analysis_process_failure(run, false, on_stdout, PATH_ERROR);
            assert!(f.during_startup, "{run:?} stdout {on_stdout}");
            assert_eq!(f.diagnosis, Some(StartupDiagnosis::PathTooLong), "{run:?} stdout {on_stdout}");
        }
    }
}

#[test]
fn an_analysis_process_that_answered_or_failed_otherwise_gets_no_path_explanation() {
    for run in [AnalysisRun::Overview, AnalysisRun::Batch] {
        for on_stdout in [false, true] {
            let f = analysis_process_failure(run, true, on_stdout, PATH_ERROR);
            assert!(!f.during_startup, "{run:?} stdout {on_stdout}");
            assert_eq!(f.diagnosis, None, "{run:?} stdout {on_stdout}: it had answered");
            let f = analysis_process_failure(run, false, on_stdout, "Could not load library cudnn64_9.dll. Error code 126");
            assert_eq!(f.diagnosis, None, "{run:?} stdout {on_stdout}: error 126");
        }
    }
}

/// `key` of the Java catalog in `language`, as the interface shows it.
fn java_text(language: &str, key: &str) -> String {
    let catalog: serde_json::Value =
        serde_json::from_str(include_str!("../../../ui/src/i18n/javaStrings.json")).unwrap();
    catalog["strings"][language][key].as_str().unwrap().to_string()
}

#[test]
fn a_benchmark_or_gtp_configuration_refusal_reads_in_the_interface_language() {
    for (locale, language) in [("th-TH", "ThTh"), ("en-US", "EnUs"), ("zh-CN", "ZhCn")] {
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let mut wb = Workbench::new(
            Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data")),
            Some(locale),
            Box::new(FakeLauncher(world.clone())),
            Vec::new(),
        );
        let katago = package(dir.path(), "macos-arm64");
        let (config, model) = (dir.path().join("gtp.cfg"), dir.path().join("model.bin.gz"));
        std::fs::write(&config, "numSearchThreads = 4\n").unwrap();
        std::fs::write(&model, b"x").unwrap();
        wb.set_host_system(mac("12.7.6"));
        let profile = EngineProfile {
            id: "packaged".into(),
            name: "KataGo".into(),
            command: format!(
                "\"{}\" gtp -config \"{}\" -model \"{}\"",
                katago.display(),
                config.display(),
                model.display()
            ),
            ..EngineProfile::default()
        };
        wb.save_engine_settings(vec![profile], EngineStartup::Empty).unwrap();
        let expected = java_text(language, "KataGoSetup.macos.minimumVersion")
            .replace("{0}", "15.0")
            .replace("{1}", "12.7.6");
        assert_eq!(
            wb.begin_benchmark("packaged", false).err().unwrap().message,
            expected,
            "{language}"
        );
        assert_eq!(
            wb.gtp_configuration_target("packaged").err().unwrap().message,
            expected,
            "{language}"
        );
        assert!(world.launched().is_empty());
    }
}

struct VersionProbe(&'static str);
impl r::Probe for VersionProbe {
    fn run(&self, program: &str, args: &[String], _: Duration) -> io::Result<r::ProbeOutput> {
        if program == "/usr/bin/sw_vers" && args == ["-productVersion"] {
            Ok(r::ProbeOutput {
                exit_code: Some(0),
                stdout: self.0.into(),
                ..Default::default()
            })
        } else {
            Ok(r::ProbeOutput {
                exit_code: Some(1),
                ..Default::default()
            })
        }
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

struct NoFetch;
impl r::Fetch for NoFetch {
    fn get(&self, _: &str, _: Option<u64>) -> r::ResourceResult<r::FetchResponse> {
        Err(r::ResourceError::Invalid("no network in this test".into()))
    }
}

#[test]
fn the_engine_check_refuses_a_packaged_engine_too_new_for_this_mac() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let katago = package(dir.path(), "macos-arm64");
    // The version is read through the system probe.
    wb.set_host_system(HostSystem::detect(&VersionProbe("12.7.6\n"), "macos"));
    let job = wb
        .begin_resource_task(ResourceCommand::Verify {
            target: ResourceEngineChoice {
                asset_id: "macos-arm64".into(),
                executable: katago.display().to_string(),
                config: String::new(),
                backend: None,
                update_profile_id: None,
            },
            weight: None,
        })
        .unwrap();
    let result = run_resource_job(&job, &NoFetch, &VersionProbe("12.7.6"), |_, _, _| {});
    assert!(wb.complete_resource_task(job.id, result));
    let resources = wb.view(false).resources.unwrap();
    let validation = resources.verification.unwrap();
    assert_eq!(validation.status, r::ValidationStatus::StartFailed);
    assert_eq!(validation.diagnosis, too_old("12.7.6"));
    let outcome = resources.last_outcome.unwrap();
    assert_eq!(
        (outcome.status.as_str(), outcome.diagnosis),
        ("failed", too_old("12.7.6"))
    );
}

#[test]
fn the_system_version_is_read_on_a_mac_only() {
    assert_eq!(HostSystem::detect(&VersionProbe("14.6.1\n"), "macos"), mac("14.6.1"));
    assert_eq!(
        HostSystem::detect(&VersionProbe("14.6.1"), "windows"),
        HostSystem {
            os: "windows".into(),
            version: None
        }
    );
}

//! Java benchmark observations and application lifecycle through consumer interfaces.
use lizzie_core::engine::{
    benchmark::{self, Observation, Progress},
    gtp::LaunchSpec,
    profile::{EngineProfile, EngineStartup},
    runtime::{EngineIo, InstanceId, Launcher},
    tuning::{self, ThreadPolicy, ThreadSource},
};
use lizzie_core::{settings::Storage, workbench::Workbench};
struct NoLaunch;
impl Launcher for NoLaunch {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        Err(std::io::Error::other("not used"))
    }
}
fn fixture() -> (tempfile::TempDir, Workbench, String) {
    let dir = tempfile::tempdir().unwrap();
    for file in ["katago", "gtp.cfg", "model.bin.gz"] {
        std::fs::write(dir.path().join(file), b"fixture").unwrap();
    }
    let command = format!(
        "\"{}\" gtp -config \"{}\" -model \"{}\"",
        dir.path().join("katago").display(),
        dir.path().join("gtp.cfg").display(),
        dir.path().join("model.bin.gz").display()
    );
    let mut wb = Workbench::new(
        Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data")),
        None,
        Box::new(NoLaunch),
        vec![dir.path().into()],
    );
    wb.save_engine_settings(
        vec![EngineProfile {
            id: "test".into(),
            name: "KataGo".into(),
            command,
            thread_policy: Some(ThreadPolicy::initial_setup()),
            ..EngineProfile::default()
        }],
        EngineStartup::Empty,
    )
    .unwrap();
    (dir, wb, "test".into())
}
fn valid() -> Observation {
    Observation {
        recommended_threads: 10,
        current_threads: 4,
        backend: "CUDA".into(),
        nn_server_threads: 2,
        batch_size: 5,
        visits_per_second: 386.0,
        nn_evals_per_second: 320.0,
        average_batch_size: 7.6,
        ..Observation::default()
    }
}
#[test]
fn benchmark_busy_cancel_stale_and_commit() {
    let (_dir, mut wb, id) = fixture();
    let first = wb.begin_benchmark(&id, false).unwrap();
    assert_eq!(
        wb.begin_benchmark(&id, false).err().unwrap().message,
        "EngineThreadPolicy.benchmarkBusy"
    );
    assert!(!wb.benchmark_progress(first.id + 1, "stale"));
    assert!(!wb.complete_benchmark(first.id + 1, Ok(valid())));
    assert!(wb.stop_benchmark());
    assert!(first.cancel.load(std::sync::atomic::Ordering::Acquire));
    assert!(wb.complete_benchmark(first.id, Ok(valid())));
    assert_eq!(wb.config().engine.engines[0].thread_policy.as_ref().unwrap().threads, 0);
    let outcome = wb.view(false).tuning.unwrap().outcome.unwrap();
    assert_eq!(outcome.key, "AutoSetup.benchmarkCancelled");
    assert!(wb.view(false).tuning.is_none_or(|view| view.outcome.is_none()));
    let next = wb.begin_benchmark(&id, false).unwrap();
    assert!(wb.complete_benchmark(next.id, Ok(valid())));
    let policy = wb.config().engine.engines[0].thread_policy.as_ref().unwrap();
    assert_eq!(policy.source, ThreadSource::BENCHMARK);
    assert!(!policy.initial_setup);
    assert_eq!(policy.threads, 10);
    assert_eq!(policy.batch_size, 5);
    assert!(policy.environment.is_some());
}
#[test]
fn deleted_and_invalid_results_do_not_commit() {
    let (_dir, mut wb, id) = fixture();
    let job = wb.begin_benchmark(&id, false).unwrap();
    wb.complete_benchmark(job.id, Ok(Observation::default()));
    assert_eq!(
        wb.view(false).tuning.unwrap().outcome.unwrap().key,
        "EngineThreadPolicy.invalidRecommendation"
    );
    let job = wb.begin_benchmark(&id, false).unwrap();
    wb.save_engine_settings(Vec::new(), EngineStartup::Empty).unwrap();
    wb.complete_benchmark(job.id, Ok(valid()));
    assert_eq!(
        wb.view(false).tuning.unwrap().outcome.unwrap().key,
        "EngineThreadPolicy.targetDeleted"
    );
}
#[test]
fn source_revision_prevents_initial_setup_switch() {
    let (_dir, mut wb, id) = fixture();
    assert!(wb.set_thread_source(&id, ThreadSource::BENCHMARK).is_err());
    let job = wb.begin_benchmark(&id, false).unwrap();
    wb.set_thread_source(&id, ThreadSource::CFG).unwrap();
    wb.complete_benchmark(job.id, Ok(valid()));
    let p = wb.config().engine.engines[0].thread_policy.as_ref().unwrap();
    assert_eq!(p.source, ThreadSource::CFG);
    assert_eq!(p.source_revision, 1);
    assert!(!p.initial_setup);
}
#[test]
fn parses_katago_1182_java_samples() {
    // KataGoBenchmarkParserTest.java:62-82 (fixed Java cb53acc).
    let output="You are currently using the CUDA version of KataGo.\nYour GTP config is currently set to use numSearchThreads = 4\nnumSearchThreads = 10: 6 / 6 positions, visits/s = 386.0 nnEvals/s = 320.0 nnBatches/s = 42.0 avgBatchSize = 7.6 (10.0 secs)\nnumSearchThreads = 10: +72 Elo (recommended)\nRunning additional tests of a few other settings at numSearchThreads = 10.\nnumSearchThreads = 10: 6 / 6 positions, visits/s = 420.0 nnEvals/s = 350.0 nnBatches/s = 49.0 avgBatchSize = 5.0\nADDITIONAL RECOMMENDATION: 2 NN server threads per GPU measured faster. Set:\nnumNNServerThreadsPerModel = 2\nADDITIONAL RECOMMENDATION: set nnMaxBatchSize = 5\n";
    let result = benchmark::parse(output, 0);
    assert_eq!(result.recommended_threads, 10);
    assert_eq!(result.current_threads, 4);
    assert_eq!(result.backend, "CUDA");
    assert_eq!(result.visits_per_second, 386.0);
    assert_eq!(result.nn_server_threads, 2);
    assert_eq!(result.batch_size, 5);
    let single="numSearchThreads = 1: 6 / 6 positions, visits/s = 74.25 nnEvals/s = 63.50 nnBatches/s = 40.00 avgBatchSize = 1.59";
    assert!(benchmark::probe_passed(single));
    assert!(!benchmark::probe_passed(&format!("{single}\nFATAL: model load failed")));
    assert!(!benchmark::probe_passed(
        "numSearchThreads = 1: 0 / 6 positions, visits/s = NaN"
    ));
    assert!(benchmark::probe_passed(&format!(
        "Beginning GPU tuning\nError: failed candidate kernel\nDone tuning\n{single}"
    )));
    let mut progress = Progress::default();
    progress.line("Possible numbers of threads to test: 1 2 4 8");
    let a = progress.line("numSearchThreads = 4: 1/6 positions");
    let b = progress.line("numSearchThreads = 4: 2/6 positions");
    assert!(a > 0 && b > a && b < 1000);
}
#[test]
fn benchmark_target_command_and_environment() {
    let (_dir, wb, id) = fixture();
    let target = wb.tuning_target(&id).unwrap();
    let launch = target.benchmark(100);
    assert!(launch.args.windows(2).any(|p| p == ["-time", "15"]));
    assert!(launch.args.windows(2).any(|p| p == ["-n", "6"]));
    assert!(launch.args.contains(&"-s".into()));
    assert!(benchmark::Target::capture("leelaz -g -w x", &[]).is_err());
    assert!(benchmark::Target::capture("katago gtp", &[]).is_err());
}
#[test]
fn policy_launch_is_scoped_and_user_overrides_win() {
    let (_dir, wb, id) = fixture();
    let target = wb.tuning_target(&id).unwrap();
    let mut p = ThreadPolicy {
        source: ThreadSource::BENCHMARK,
        threads: 8,
        nn_server_threads: 2,
        batch_size: 16,
        environment: Some(target.environment.clone()),
        ..ThreadPolicy::default()
    };
    let mut spec = target.launch.clone();
    tuning::apply_launch_policy(&target.command, Some(&p), &mut spec);
    let overrides = tuning::overrides(&spec.args);
    assert_eq!(overrides["numsearchthreads"], "8");
    assert_eq!(overrides["nnmaxbatchsize"], "16");
    p.source = ThreadSource::CFG;
    assert_eq!(p.threads_for_launch(&target.command), 0);
    p.source = ThreadSource::BENCHMARK;
    assert_eq!(
        p.threads_for_launch(&format!("{} -override-config numSearchThreads=3", target.command)),
        0
    );
    assert_eq!(p.threads_for_launch(&format!("{} -threads 3", target.command)), 0);
    p.environment = None;
    let mut stale = target.launch.clone();
    tuning::apply_launch_policy(&target.command, Some(&p), &mut stale);
    assert!(!tuning::overrides(&stale.args).contains_key("nnmaxbatchsize"));
}
fn report() -> serde_json::Value {
    let hash = "a".repeat(64);
    let mut runs = Vec::new();
    for round in 1..=3 {
        for profile in if round % 2 == 1 {
            ["baseline", "candidate"]
        } else {
            ["candidate", "baseline"]
        } {
            runs.push(serde_json::json!({"profile":profile,"round":round,"phase":"warm","seconds":if profile=="baseline"{10.0}else{8.0},"firstResultSeconds":0.1,"observedRootVisits":800,"responseSeconds":0.01,"edtP95Seconds":0.01,"maxMemoryMiB":1000,"maxEngineProcesses":1}));
        }
    }
    serde_json::json!({"schemaVersion":1,"measurementMode":"application","scene":"live","budget":800,"positions":1,"fixtureSha256":hash,"metricScope":"totalGpu","fingerprint":{"engineSha256":hash,"modelSha256":hash,"configSha256":hash,"gpuName":"GPU","driverVersion":"1","memoryMiB":8000,"configIncludes":[],"commandSemantics":{}},"baselineParameters":{"numSearchThreads":4,"nnMaxBatchSize":4},"candidateParameters":{"numSearchThreads":8,"nnMaxBatchSize":8},"runs":runs})
}
#[test]
fn measured_report_accepts_only_stable_complete_application_evidence() {
    let good = report();
    assert!(tuning::validate_measured_report(&good).is_ok());
    for (key, value) in [
        ("schemaVersion", serde_json::json!(2)),
        ("measurementMode", serde_json::json!("benchmark")),
        ("positions", serde_json::json!(2)),
    ] {
        let mut bad = good.clone();
        bad[key] = value;
        assert!(tuning::validate_measured_report(&bad).is_err());
    }
    let mut bad = good.clone();
    bad["runs"][1]["observedRootVisits"] = serde_json::json!(799);
    assert!(tuning::validate_measured_report(&bad).is_err());
    let mut bad = good;
    bad["candidateParameters"]["arbitraryCommand"] = serde_json::json!(1);
    assert!(tuning::validate_measured_report(&bad).is_err());
}

struct Protocol {
    replies: std::collections::VecDeque<Result<String, String>>,
    sent: Vec<String>,
}
impl lizzie_core::engine::gtp_config::Session for Protocol {
    fn request(&mut self, command: &str) -> Result<String, String> {
        self.sent.push(command.into());
        self.replies.pop_front().unwrap()
    }
}
fn schema() -> serde_json::Value {
    serde_json::json!({"protocol":"zengtp-config","version":1,"batchSemantics":"atomic","persistenceOwner":"client","fields":[{"name":"threads","type":"integer","minimum":1,"maximum":64,"defaultValue":4,"apply":"next-search","group":"basic"}]})
}
#[test]
fn gtp_configuration_inspects_validates_and_saves_engine_returned_profile() {
    use lizzie_core::engine::gtp_config;
    let mut unsupported = Protocol {
        replies: [Ok("false".into())].into(),
        sent: vec![],
    };
    assert!(gtp_config::inspect(&mut unsupported).unwrap().is_none());
    let mut probe = Protocol {
        replies: [
            Ok("true".into()),
            Ok(schema().to_string()),
            Ok(String::new()),
            Ok(r#"{"profile":{"threads":8},"state":{}}"#.into()),
        ]
        .into(),
        sent: vec![],
    };
    let saved = gtp_config::apply(&mut probe, &[("threads".into(), serde_json::json!(8))].into()).unwrap();
    assert_eq!(saved.profile["threads"], 8);
    assert_eq!(probe.sent.last().unwrap(), "zengtp_config_save");
    assert!(probe.sent[2].starts_with("zengtp_config_set {"));
    let mut bad = Protocol {
        replies: [Ok("true".into()), Ok(schema().to_string())].into(),
        sent: vec![],
    };
    assert!(gtp_config::apply(&mut bad, &[("threads".into(), serde_json::json!(0))].into()).is_err());
    assert_eq!(bad.sent.len(), 2);
    let mut bad_schema = schema();
    bad_schema["fields"][0]["requiresRestart"] = serde_json::json!(true);
    assert!(gtp_config::validate_schema(&bad_schema).is_err());
}
#[test]
fn measured_import_restore_never_rewrites_command() {
    let (_dir, mut wb, id) = fixture();
    let command = wb.config().engine.engines[0].command.clone();
    wb.commit_measured_tuning(&id, &command, report()).unwrap();
    assert!(wb.config().engine.engines[0]
        .thread_policy
        .as_ref()
        .unwrap()
        .measured_scenes
        .is_some());
    wb.restore_measured_tuning(&id).unwrap();
    assert_eq!(wb.config().engine.engines[0].command, command);
    assert!(wb.config().engine.engines[0]
        .thread_policy
        .as_ref()
        .unwrap()
        .measured_scenes
        .is_none());
}
#[test]
fn apple_candidates_respect_memory_and_model_limits() {
    let low = benchmark::apple_candidates(0, 0);
    assert_eq!(low.len(), 7);
    assert!(!low.iter().any(|c| c.devices.len() > 2));
    let high = benchmark::apple_candidates(16 * 1024 * 1024 * 1024, 1024);
    assert_eq!(high.len(), 11);
    assert!(high.iter().any(|c| c.devices == [0, 0, 100, 100]));
    assert_eq!(
        benchmark::apple_candidates(16 * 1024 * 1024 * 1024, 2 * 1024 * 1024 * 1024).len(),
        7
    );
    let candidate = low[0].clone();
    let mut fast = valid();
    fast.visits_per_second = 100.0;
    let mut slow = valid();
    slow.visits_per_second = 80.0;
    let shortlist = benchmark::apple_shortlist(vec![(low[1].clone(), fast), (candidate.clone(), slow)]).unwrap();
    assert_eq!(shortlist[0], candidate);
    assert!(benchmark::apple_shortlist(vec![(low[1].clone(), valid())]).is_err());
}
#[test]
fn fresh_fingerprint_detects_same_size_artifact_replacement() {
    let (dir, wb, id) = fixture();
    let target = wb.tuning_target(&id).unwrap();
    let before = tuning::measured_fingerprint(&target, "GPU", "driver", 8000).unwrap();
    std::fs::write(dir.path().join("model.bin.gz"), b"changed").unwrap();
    let after = tuning::measured_fingerprint(&target, "GPU", "driver", 8000).unwrap();
    assert_ne!(before["modelSha256"], after["modelSha256"]);
}

#[test]
fn measured_review_is_explicit_and_stale_reviews_are_refused() {
    let (_dir, mut wb, id) = fixture();
    let command = wb.config().engine.engines[0].command.clone();
    let old = wb.review_measured_tuning(&id, &command, report()).unwrap();
    assert!(wb.config().engine.engines[0]
        .thread_policy
        .as_ref()
        .unwrap()
        .measured_scenes
        .is_none());
    let current = wb.review_measured_tuning(&id, &command, report()).unwrap();
    assert!(wb.confirm_measured_tuning(&id, old).is_err());
    wb.confirm_measured_tuning(&id, current).unwrap();
    assert!(wb.config().engine.engines[0]
        .thread_policy
        .as_ref()
        .unwrap()
        .measured_scenes
        .is_some());
    assert!(wb.confirm_measured_tuning(&id, current).is_err());
    let review = wb.review_measured_tuning(&id, &command, report()).unwrap();
    let mut engines = wb.config().engine.engines.clone();
    engines[0]
        .command
        .push_str(" -override-config numNNServerThreadsPerModel=3");
    wb.save_engine_settings(engines, EngineStartup::Empty).unwrap();
    assert!(wb.confirm_measured_tuning(&id, review).is_err());
}

#[test]
fn measured_live_overlay_requires_fresh_matching_hardware_and_artifacts() {
    let (dir, wb, id) = fixture();
    let target = wb.tuning_target(&id).unwrap();
    let mut measured = report();
    measured["fingerprint"] = tuning::measured_fingerprint(&target, "GPU", "driver", 8000).unwrap();
    let policy = ThreadPolicy {
        measured_scenes: Some(
            serde_json::json!({"live":{"schemaVersion":1,"command":target.command,"report":measured}}),
        ),
        ..ThreadPolicy::default()
    };
    let mut matching = target.launch.clone();
    tuning::apply_measured_live(&target.command, &policy, &[], &mut matching, "GPU", "driver", 8000);
    assert_eq!(tuning::overrides(&matching.args)["numsearchthreads"], "8");
    let mut wrong_gpu = target.launch.clone();
    tuning::apply_measured_live(
        &target.command,
        &policy,
        &[],
        &mut wrong_gpu,
        "other GPU",
        "driver",
        8000,
    );
    assert!(tuning::overrides(&wrong_gpu.args).is_empty());
    std::fs::write(dir.path().join("model.bin.gz"), b"changed").unwrap();
    let mut changed = target.launch.clone();
    tuning::apply_measured_live(&target.command, &policy, &[], &mut changed, "GPU", "driver", 8000);
    assert!(tuning::overrides(&changed.args).is_empty());
}
#[test]
fn apple_selection_requires_three_stable_samples_and_material_gain() {
    let candidates = benchmark::apple_candidates(0, 0);
    let baseline = candidates[0].clone();
    let challenger = candidates[1].clone();
    let observed = |speed| {
        let mut o = valid();
        o.recommended_threads = 8;
        o.visits_per_second = speed;
        o.metrics = vec![benchmark::Metrics {
            threads: 8,
            completed: 3,
            total: 3,
            visits: speed,
            nn_evals: 100.0,
            nn_batches: 25.0,
            average_batch: 4.0,
        }];
        o
    };
    let official = vec![
        (baseline.clone(), observed(100.0)),
        (challenger.clone(), observed(150.0)),
    ];
    let samples = [
        (
            baseline.id.clone(),
            vec![observed(100.0), observed(101.0), observed(99.0)],
        ),
        (
            challenger.id.clone(),
            vec![observed(120.0), observed(121.0), observed(119.0)],
        ),
    ]
    .into();
    assert_eq!(
        benchmark::apple_select(official.clone(), &samples)
            .unwrap()
            .apple_candidate
            .unwrap(),
        challenger
    );
    let samples = [
        (
            baseline.id.clone(),
            vec![observed(100.0), observed(101.0), observed(99.0)],
        ),
        (
            challenger.id.clone(),
            vec![observed(120.0), observed(180.0), observed(119.0)],
        ),
    ]
    .into();
    assert_eq!(
        benchmark::apple_select(official, &samples)
            .unwrap()
            .apple_candidate
            .unwrap(),
        baseline
    );
}

#[test]
fn java_apple_profile_is_content_and_hardware_scoped() {
    let (dir, wb, id) = fixture();
    let target = wb.tuning_target(&id).unwrap();
    let hardware = serde_json::json!({"model":"Mac16,1","chipName":"Apple M4","architecture":"arm64","logicalCores":"10","memoryBytes":"17179869184","osBuild":"24G90","rosetta":"0"});
    let digest = tuning::apple_fingerprint(&target.command, &target.launch, &hardware, false).unwrap();
    let policy = ThreadPolicy {
        tuning_profiles: Some(
            serde_json::json!({"schemaVersion":2,"mode":"EXPERIMENTAL_HARDWARE","fingerprintDigest":digest,"devices":[0,100],"batch":4,"threads":8}),
        ),
        ..ThreadPolicy::default()
    };
    let mut matched = target.launch.clone();
    tuning::apply_apple_launch_policy(&target.command, &policy, &mut matched, &hardware);
    let overrides = tuning::overrides(&matched.args);
    assert_eq!(overrides["numnnserverthreadspermodel"], "2");
    assert_eq!(overrides["metaldevicetousemodel0thread1"], "100");
    assert_eq!(overrides["nnmaxbatchsize"], "4");
    let mut changed_hardware = hardware.clone();
    changed_hardware["osBuild"] = serde_json::json!("new build");
    let mut unmatched = target.launch.clone();
    tuning::apply_apple_launch_policy(&target.command, &policy, &mut unmatched, &changed_hardware);
    assert!(tuning::overrides(&unmatched.args).is_empty());
    std::fs::write(dir.path().join("model.bin.gz"), b"changed").unwrap();
    let mut changed_model = target.launch.clone();
    tuning::apply_apple_launch_policy(&target.command, &policy, &mut changed_model, &hardware);
    assert!(tuning::overrides(&changed_model.args).is_empty());
}

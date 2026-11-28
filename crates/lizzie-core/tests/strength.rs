//! 棋力评估 through the public `Workbench::strength` command and view, with saved analysis read
//! from SGF and a controlled HumanSL 40 policy process.
use lizzie_core::api::{
    CommandResult, EditCommand, FlashTarget, OpenSource, PhaseBadge, PhaseReview, PhaseVerdict, StrengthCommand,
    StrengthContext, StrengthManualOutcome, StrengthPhases, StrengthReport, StrengthSideReport, StrengthStage,
    StrengthSummary, TrackingCommand,
};
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats;
use lizzie_core::record::Point;
use lizzie_core::record::RecordFormat;
use lizzie_core::settings::{self, Config, Storage};
use lizzie_core::strength::StrengthModel;
use lizzie_core::workbench::{OpenOrigin, Workbench, SETTINGS_SAVE_FAILED};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 61 moves, every position analysed (KataGo payload), Java's saved-analysis format. The fixed Java
/// build shows this game as Black 业余2.9段 / White 业余7.8段 (acceptance screenshot j01).
const GAME: &str = include_str!("fixtures/strength/seed12-kata-61.sgf");
/// 开始计算 (HumanSL 40) as the dashboard first sends it.
const RUN: StrengthCommand = StrengthCommand::RunHumanSl { resume: false };

#[derive(Default)]
struct Process {
    id: InstanceId,
    args: Vec<String>,
    sent: Vec<String>,
    dead: bool,
}
#[derive(Clone, Default)]
struct World(Arc<Mutex<Vec<Process>>>);
struct Io {
    world: World,
    id: InstanceId,
}
impl EngineIo for Io {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        let mut all = self.world.0.lock().unwrap();
        all.iter_mut().find(|p| p.id == self.id).unwrap().sent.push(line.into());
        Ok(())
    }
    fn terminate(self: Box<Self>) {
        let mut all = self.world.0.lock().unwrap();
        all.iter_mut().find(|p| p.id == self.id).unwrap().dead = true;
    }
}
impl Launcher for World {
    fn launch(&self, id: InstanceId, spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        self.0.lock().unwrap().push(Process {
            id,
            args: spec.args.clone(),
            ..Process::default()
        });
        Ok(Box::new(Io {
            world: self.clone(),
            id,
        }))
    }
}
impl World {
    fn launched(&self) -> usize {
        self.0.lock().unwrap().len()
    }
    fn last(&self) -> InstanceId {
        self.0.lock().unwrap().last().unwrap().id
    }
    fn take(&self, id: InstanceId) -> Vec<String> {
        std::mem::take(&mut self.0.lock().unwrap().iter_mut().find(|p| p.id == id).unwrap().sent)
    }
    fn dead(&self, id: InstanceId) -> bool {
        self.0.lock().unwrap().iter().find(|p| p.id == id).unwrap().dead
    }
    fn args(&self, id: InstanceId) -> Vec<String> {
        self.0.lock().unwrap().iter().find(|p| p.id == id).unwrap().args.clone()
    }
}

struct Fixture {
    dir: tempfile::TempDir,
    wb: Workbench,
    world: World,
    /// What the foreground engine answers to `name`.
    gtp_name: &'static str,
}

/// A workbench whose analysis command names a local KataGo stand-in and whose HumanSL model is a
/// stand-in admitted by its own digest.
fn fixture() -> Fixture {
    fixture_with(|config, engine| {
        config.analysis_engine.command = format!("{} analysis -config a.cfg -model m.bin.gz", engine.display());
        config.analysis_engine.command_customized = true;
    })
}

/// [`fixture`] without the automatic quick analysis of a loaded record, so that no analysis runs
/// until one is started.
fn still() -> Fixture {
    fixture_with(|config, engine| {
        config.analysis_engine.command = format!("{} analysis -config a.cfg -model m.bin.gz", engine.display());
        config.analysis_engine.command_customized = true;
        config.record.auto_quick_analyze_on_load = false;
    })
}

/// [`fixture`] with the KataGo stand-in at `engine` named by `setup` instead; `setup` may also drop
/// the HumanSL stand-in.
fn fixture_with(setup: impl FnOnce(&mut Config, &std::path::Path)) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let engine = dir.path().join("katago");
    std::fs::write(&engine, b"stand-in engine").unwrap();
    let human = dir.path().join("human.bin.gz");
    std::fs::write(&human, b"stand-in HumanSL network").unwrap();
    let storage = Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data"));
    let mut config = Config::default();
    config.human_sl.model_path = Some(human.to_string_lossy().into_owned());
    setup(&mut config, &engine);
    settings::save_json(&storage.config_file, &config).unwrap();
    let world = World::default();
    let mut wb = Workbench::new(storage, None, Box::new(world.clone()), vec![]);
    wb.set_strength_human_digest(&lizzie_core::resources::sha256_file(&human).unwrap());
    open(&mut wb, GAME);
    Fixture { dir, wb, world, gtp_name: "KataGo" }
}

fn open(wb: &mut Workbench, text: &str) {
    let read = formats::parse(RecordFormat::Sgf, text, wb.config().edit.no_capture);
    let t = wb.begin_open(OpenOrigin::File {
        path: "/game.sgf".into(),
        source: OpenSource::Drop,
    });
    assert!(wb.complete_open(t, read.map(|r| (RecordFormat::Sgf, r))));
}

fn report(wb: &mut Workbench) -> StrengthReport {
    estimate(wb).0
}

fn context(wb: &mut Workbench) -> StrengthContext {
    estimate(wb).1
}

fn estimate(wb: &mut Workbench) -> (StrengthReport, StrengthContext) {
    match wb.strength(StrengthCommand::Report).unwrap() {
        CommandResult::Strength { report, context, .. } => (*report, context),
        other => panic!("{other:?}"),
    }
}

/// [`GAME`] cut after its first `moves` moves, with the saved analysis of node `i` (0: the root)
/// kept only where `analysed(i)` holds.
fn game(moves: usize, analysed: impl Fn(usize) -> bool) -> String {
    let nodes: Vec<&str> = GAME.trim_end().trim_end_matches(')').split("\n;").collect();
    assert_eq!(nodes.len(), 62);
    let kept: Vec<String> = nodes[..=moves]
        .iter()
        .enumerate()
        .map(|(i, node)| match node.find("LZ[") {
            Some(start) if !analysed(i) => {
                let end = start + node[start..].find(']').unwrap() + 1;
                format!("{}{}", &node[..start], &node[end..])
            }
            _ => (*node).to_string(),
        })
        .collect();
    format!("{})\n", kept.join("\n;"))
}

/// Java `%.1f` / `%.0f%%` as the cards show them.
fn one(v: f64) -> String {
    format!("{v:.1}")
}
fn percent(v: f64) -> String {
    format!("{:.0}%", v * 100.0)
}

/// Ticks until the policy process for the queued preparation has been launched.
fn policy_worker(f: &mut Fixture, before: usize) -> InstanceId {
    for _ in 0..500 {
        f.wb.engine_tick(Instant::now());
        if f.world.launched() > before {
            return f.world.last();
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("no policy process was launched");
}

/// A valid KataGo `humanPolicy` (362 entries summing to 1).
fn policy(id: &Value) -> String {
    json!({"id": id, "humanPolicy": vec![1.0 / 362.0; 362]}).to_string()
}

/// Answers every query the process has received; returns how many.
fn answer(f: &mut Fixture, id: InstanceId) -> usize {
    let mut answered = 0;
    loop {
        let sent = f.world.take(id);
        if sent.is_empty() {
            return answered;
        }
        for line in sent {
            let q: Value = serde_json::from_str(&line).unwrap();
            f.wb.engine_input(id, EngineInput::Stdout(policy(&q["id"])));
            answered += 1;
        }
    }
}

#[test]
fn report_from_saved_analysis_matches_the_fixed_java_dashboard() {
    let mut f = fixture();
    let r = report(&mut f.wb);
    assert_eq!(r.model.id, "bundled:xgboost20tun");
    assert!(r.enough_data);
    // Java j01: 黑 业余2.9段 31 手 52% 58% 3.0 4.3 / 白 业余7.8段 30 手 63% 70% 1.6 1.9.
    for (side, rank, moves, first, good, avg, weighted) in [
        (&r.black, "2.9", 31, "52%", "58%", "3.0", "4.3"),
        (&r.white, "7.8", 30, "63%", "70%", "1.6", "1.9"),
    ] {
        let s = &side.summary;
        assert_eq!(one(s.rank_value.unwrap()), rank);
        assert_eq!(s.sample_count, moves);
        assert_eq!(percent(s.first_choice_rate), first);
        assert_eq!(percent(s.move_rank_good_move_rate), good);
        assert_eq!(one(s.average_score_loss), avg);
        assert_eq!(one(s.weighted_score_loss), weighted);
        assert_eq!(side.samples.len(), moves as usize);
    }
    // Java j05 发挥水准: 黑 最佳 16 / 差手 11, 白 最佳 19 / 差手 4 come from the same samples.
    assert_eq!(r.black.samples.iter().filter(|s| s.first_choice).count(), 16);
    assert_eq!(r.white.samples.iter().filter(|s| s.first_choice).count(), 19);
}

#[test]
fn humansl40_without_prepared_policies_reports_no_estimate() {
    let mut f = fixture();
    assert_eq!(
        f.wb.strength(StrengthCommand::SelectModel {
            id: "bundled:humansl40".into()
        })
        .unwrap(),
        CommandResult::Done
    );
    let r = report(&mut f.wb);
    assert!(r.model.human_sl);
    assert_eq!(r.overall.summary.sample_count, 0);
    assert!(!r.enough_data);
    assert_eq!(r.black.summary.rank_value, None);
}

/// Opens `sgf` and answers the dialog's context for it.
fn context_of(f: &mut Fixture, sgf: &str) -> StrengthContext {
    open(&mut f.wb, sgf);
    context(&mut f.wb)
}

/// Java `StrengthDashboard.stageFor`, `showShortGame` and the partial line (`isPartlyAnalyzed`:
/// fewer than nine in ten main-line moves analysed), with `StrengthDialogHost.analysisProgress`.
#[test]
fn the_context_tells_how_much_of_the_game_is_analysed_and_what_is_missing() {
    let mut f = still();
    let stage = |c: StrengthContext| (c.stage, c.analyzed, c.total, c.analyzing);
    // No analysis: the dialog asks for it.
    assert_eq!(stage(context_of(&mut f, &game(61, |_| false))), (StrengthStage::NeedsAnalysis, 0, 61, false));
    // Three analysed moves are too few for an estimate yet.
    assert_eq!(stage(context_of(&mut f, &game(61, |i| i <= 3))), (StrengthStage::NeedsAnalysis, 3, 61, false));
    // Half the game: the figures, said to rest on 30 of 61 moves.
    assert_eq!(
        stage(context_of(&mut f, &game(61, |i| i <= 30))),
        (StrengthStage::Ready { partial: true }, 30, 61, false)
    );
    // 54 of 61 is under nine in ten, 55 is not.
    assert_eq!(context_of(&mut f, &game(61, |i| i <= 54)).stage, StrengthStage::Ready { partial: true });
    assert_eq!(context_of(&mut f, &game(61, |i| i <= 55)).stage, StrengthStage::Ready { partial: false });
    // Five moves, all analysed: too short, and analysis would not help.
    assert_eq!(stage(context_of(&mut f, &game(5, |_| true))), (StrengthStage::TooShort, 5, 5, false));
    // The whole game analysed.
    assert_eq!(stage(context_of(&mut f, GAME)), (StrengthStage::Ready { partial: false }, 61, 61, false));
}

#[test]
fn a_running_analysis_shows_its_progress_instead_of_the_partial_line() {
    let mut f = still();
    open(&mut f.wb, &game(5, |_| false));
    assert!(!f.wb.view(false).strength.analyzing);
    assert_eq!(f.wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(), Ok(true));
    assert!(f.wb.view(false).strength.analyzing);
    let c = context(&mut f.wb);
    assert_eq!((c.stage, c.analyzed, c.total, c.analyzing), (StrengthStage::NeedsAnalysis, 0, 5, true));
    assert!(f.wb.cancel_flash_analysis());
    assert!(!f.wb.view(false).strength.analyzing);

    open(&mut f.wb, &game(61, |i| i <= 30));
    assert_eq!(f.wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(), Ok(true));
    let c = context(&mut f.wb);
    assert_eq!((c.stage, c.analyzed, c.total, c.analyzing), (StrengthStage::Ready { partial: false }, 30, 61, true));
    assert!(f.wb.cancel_flash_analysis());
    assert_eq!(context(&mut f.wb).stage, StrengthStage::Ready { partial: true });
}

#[test]
fn the_human_style_model_asks_for_its_calculation_once_the_analysis_is_enough() {
    let mut f = still();
    assert_eq!(
        f.wb.strength(StrengthCommand::SelectModel {
            id: "bundled:humansl40".into()
        })
        .unwrap(),
        CommandResult::Done
    );
    let c = context(&mut f.wb);
    assert_eq!((c.stage, c.human_style_download), (StrengthStage::NeedsHumanStyle, None));
    assert_eq!(context_of(&mut f, &game(61, |i| i <= 3)).stage, StrengthStage::NeedsAnalysis);
    assert_eq!(context_of(&mut f, &game(5, |_| true)).stage, StrengthStage::TooShort);
}

#[test]
fn without_the_human_style_weights_the_context_names_their_download_size() {
    let mut f = fixture_with(|config, _| config.human_sl.model_path = None);
    let official = lizzie_core::resources::Catalog::embedded().unwrap();
    let size = official.model("humansl-b18c384nbt-v0").unwrap().size_bytes;
    assert_eq!(context(&mut f.wb).human_style_download, Some(size));
}

#[test]
fn manual_humansl40_prepares_every_turn_then_reports_and_reuses_its_cache() {
    let mut f = fixture();
    let before = f.world.launched();
    assert_eq!(f.wb.strength(RUN).unwrap(), CommandResult::Done);
    assert!(f.wb.view(false).strength.manual.unwrap().running);
    let worker = policy_worker(&mut f, before);
    // KataGo refuses the policy config's humanSL* keys unless the network is also the human model
    // (Java HumanSlAnalysisRunner.buildHumanSlCommand appends it).
    let args = f.world.args(worker);
    let human = f.dir.path().join("human.bin.gz").to_string_lossy().into_owned();
    let at = args.iter().position(|a| a == "-human-model").expect("-human-model");
    assert_eq!(args[at + 1], human);
    assert_eq!(args[0], "analysis");
    let mut queries = 0;
    for _ in 0..200 {
        let n = answer(&mut f, worker);
        queries += n;
        if n == 0 {
            break;
        }
    }
    // Three profiles (rank_1d, rank_5d, rank_9d) for each of the 61 positions before a move.
    assert_eq!(queries, 61 * 3);
    let v = f.wb.view(false).strength;
    assert_eq!(v.status.key, "HumanSL40.backgroundReady");
    assert_eq!((v.status.completed, v.status.total), (61, 61));
    assert_eq!(v.manual.unwrap().outcome, Some(StrengthManualOutcome::Ready));
    assert!(f.world.dead(worker), "the policy process ends with the batch");

    f.wb.strength(StrengthCommand::SelectModel {
        id: "bundled:humansl40".into(),
    })
    .unwrap();
    let r = report(&mut f.wb);
    assert!(r.model.human_sl);
    assert_eq!((r.black.summary.sample_count, r.white.summary.sample_count), (31, 30));
    assert!(r.black.summary.rank_value.is_some() && r.white.summary.rank_value.is_some());
    assert!(r.black.samples.iter().all(|s| s.human.is_some()));

    // The same game again: every policy comes from the per-query cache.
    let before = f.world.launched();
    let first = f.wb.view(false).strength.manual.unwrap().run;
    f.wb.strength(RUN).unwrap();
    // The new run replaces the previous outcome under a new number, so a dialog started for it
    // cannot take the previous run's Ready as its own.
    let admitted = f.wb.view(false).strength.manual.unwrap();
    assert_eq!(
        (admitted.run, admitted.running, admitted.outcome),
        (first + 1, true, None)
    );
    let again = policy_worker(&mut f, before);
    for _ in 0..50 {
        f.wb.engine_tick(Instant::now());
    }
    assert!(f.world.take(again).is_empty(), "cached policies are not queried again");
    assert_eq!(
        f.wb.view(false).strength.manual.unwrap().outcome,
        Some(StrengthManualOutcome::Ready)
    );
}

#[test]
fn cancelling_stops_the_policy_process_and_ignores_its_late_answers() {
    let mut f = fixture();
    let before = f.world.launched();
    f.wb.strength(RUN).unwrap();
    let worker = policy_worker(&mut f, before);
    let pending = f.world.take(worker);
    assert!(!pending.is_empty());
    assert_eq!(
        f.wb.strength(StrengthCommand::CancelHumanSl).unwrap(),
        CommandResult::Done
    );
    assert!(f.world.dead(worker));
    let v = f.wb.view(false).strength;
    assert_eq!(v.status.key, "HumanSL40.cancelled");
    assert_eq!(v.manual.unwrap().outcome, Some(StrengthManualOutcome::Cancelled));
    let revision = v.status.revision;
    // An answer already on its way when the process was stopped changes nothing.
    let q: Value = serde_json::from_str(&pending[0]).unwrap();
    f.wb.engine_input(worker, EngineInput::Stdout(policy(&q["id"])));
    assert_eq!(f.wb.view(false).strength.status.revision, revision);
    // The next heartbeat reports the background state (Java observe) and starts nothing.
    f.wb.engine_tick(Instant::now() + Duration::from_secs(2));
    assert_eq!(f.wb.view(false).strength.status.key, "HumanSL40.backgroundOff");
    assert_eq!(f.world.launched(), before + 1, "no automatic restart");
}

#[test]
fn a_changed_game_fails_the_manual_run_and_drops_prepared_policies() {
    let mut f = fixture();
    let before = f.world.launched();
    f.wb.strength(RUN).unwrap();
    let worker = policy_worker(&mut f, before);
    // Another game opens while the policies are being prepared: the observation supersedes the
    // task and the HumanSL 40 report stays empty for the new game.
    open(&mut f.wb, "(;SZ[19];B[pd];W[dp])");
    f.wb.engine_tick(Instant::now() + Duration::from_secs(2));
    assert!(f.world.dead(worker));
    f.wb.strength(StrengthCommand::SelectModel {
        id: "bundled:humansl40".into(),
    })
    .unwrap();
    assert_eq!(report(&mut f.wb).overall.summary.sample_count, 0);
}

#[test]
fn refusals_name_the_missing_humansl40_prerequisite() {
    let mut f = fixture();
    open(&mut f.wb, "(;SZ[13];B[dd];W[jj])");
    assert_eq!(
        f.wb.strength(RUN).unwrap(),
        CommandResult::Refused {
            message_key: "HumanSL40.boardSize".into()
        }
    );
    open(&mut f.wb, GAME);
    f.wb.set_strength_human_digest(&"0".repeat(64));
    assert_eq!(
        f.wb.strength(RUN).unwrap(),
        CommandResult::Refused {
            message_key: "HumanSL40.invalidModel".into()
        }
    );
}

#[test]
fn a_run_resumed_after_the_model_download_keeps_its_game() {
    let mut f = fixture();
    let human = f.dir.path().join("human.bin.gz");
    let network = std::fs::read(&human).unwrap();
    std::fs::remove_file(&human).unwrap();
    let resume = StrengthCommand::RunHumanSl { resume: true };
    let download = CommandResult::Refused {
        message_key: "HumanSL40.downloadRequired".into(),
    };
    // HumanSlStrengthDialog: the capture taken before the download is the one prepared after it.
    assert_eq!(f.wb.strength(RUN).unwrap(), download);
    open(&mut f.wb, "(;SZ[19];B[pd];W[dp])");
    std::fs::write(&human, &network).unwrap();
    assert_eq!(
        f.wb.strength(resume.clone()).unwrap(),
        CommandResult::Refused {
            message_key: "HumanSL40.gameChanged".into()
        }
    );
    std::fs::remove_file(&human).unwrap();
    assert_eq!(f.wb.strength(RUN).unwrap(), download);
    std::fs::write(&human, &network).unwrap();
    let before = f.world.launched();
    assert_eq!(f.wb.strength(resume).unwrap(), CommandResult::Done);
    policy_worker(&mut f, before);
}

#[test]
fn a_saved_katago_engine_without_an_analysis_config_runs_the_policy_process() {
    // HumanSlStrengthPreparation.discoverAssets: the saved engine's executable, whether or not an
    // analysis command could be made from it (no -config here).
    let mut f = fixture_with(|config, engine| {
        config.engine.engines = vec![EngineProfile {
            name: "KataGo".into(),
            command: format!("{} gtp -model m.bin.gz", engine.display()),
            ..EngineProfile::default()
        }];
        config.engine.startup = EngineStartup::Empty;
    });
    let before = f.world.launched();
    assert_eq!(f.wb.strength(RUN).unwrap(), CommandResult::Done);
    let worker = policy_worker(&mut f, before);
    assert_eq!(f.world.args(worker)[0], "analysis");
}

#[test]
fn java_import_copies_the_strength_models_once_and_leaves_java_alone() {
    use lizzie_core::java_import::{self, JavaSource};
    let mut f = fixture();
    let java = f.dir.path().join("java");
    let model = java.join("strength-models").join("club");
    let team = java.join("strength-models").join("team");
    std::fs::create_dir_all(&model).unwrap();
    std::fs::create_dir_all(&team).unwrap();
    std::fs::write(java.join("config.txt"), "{}").unwrap();
    let booster = include_bytes!("../resources/strength/xgboost20tun_booster.json");
    std::fs::write(model.join("booster.json"), booster).unwrap();
    // Java lists every directory: the same trees under another shown name are another model.
    std::fs::write(team.join("booster.json"), booster).unwrap();
    std::fs::write(team.join("display-name.txt"), "Team").unwrap();
    // This client already has a different model in a directory of the same name.
    let mine = f.dir.path().join("data").join("strength-models").join("club");
    std::fs::create_dir_all(&mine).unwrap();
    std::fs::write(mine.join("booster.json"), booster).unwrap();
    std::fs::write(mine.join("display-name.txt"), "Mine").unwrap();
    let java_files = || std::fs::read_dir(&model).unwrap().count();
    for _ in 0..2 {
        let p =
            f.wb.preview_java_import(java_import::read(&JavaSource::new(&java, None)).unwrap());
        let it = p.items.iter().find(|i| i.key == "file:strength-models").unwrap();
        assert_eq!(it.value.as_deref(), Some("2"));
        f.wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    }
    let names: Vec<_> = f.wb.view(false).strength.models.into_iter().map(|m| m.name).collect();
    assert_eq!(
        names,
        ["xgboost20tun", "HumanSL 40", "club", "Mine", "Team"],
        "imported twice, listed once; the renamed copy keeps Java's name"
    );
    assert_eq!(java_files(), 1);
    let id = f.wb.view(false).strength.models[2].id.clone();
    f.wb.strength(StrengthCommand::SelectModel { id }).unwrap();
    assert_eq!(one(report(&mut f.wb).black.summary.rank_value.unwrap()), "2.9");
}

#[test]
fn background_choice_persists_and_imported_models_become_active() {
    let mut f = fixture();
    assert_eq!(
        f.wb.strength(StrengthCommand::SetBackground { enabled: true }).unwrap(),
        CommandResult::Done
    );
    // Background preparation waits for foreground analysis (Java observe's foregroundAnalyzing).
    let launched = f.world.launched();
    f.wb.engine_tick(Instant::now() + Duration::from_secs(2));
    assert_eq!(f.wb.view(false).strength.status.key, "HumanSL40.backgroundIdle");
    assert_eq!(f.world.launched(), launched);
    let storage = Storage::in_dirs(&f.dir.path().join("config"), &f.dir.path().join("data"));
    let again = Workbench::new(storage, None, Box::new(f.world.clone()), vec![]);
    assert!(again.config().strength.humansl_background);

    let bad = f.dir.path().join("broken.json");
    std::fs::write(&bad, b"{}").unwrap();
    assert!(matches!(
        f.wb.strength(StrengthCommand::ImportModel {
            path: bad.to_string_lossy().into_owned()
        })
        .unwrap(),
        CommandResult::StrengthImportFailed { .. }
    ));
    let booster = f.dir.path().join("booster.json");
    std::fs::write(
        &booster,
        include_bytes!("../resources/strength/xgboost20tun_booster.json"),
    )
    .unwrap();
    assert_eq!(
        f.wb.strength(StrengthCommand::ImportModel {
            path: booster.to_string_lossy().into_owned()
        })
        .unwrap(),
        CommandResult::Done
    );
    let v = f.wb.view(false).strength;
    assert_eq!(v.models.len(), 3);
    assert_eq!(v.active, v.models[2].id);
    // A booster with the bundled trees and features scores the game like the bundled model.
    assert_eq!(one(report(&mut f.wb).black.summary.rank_value.unwrap()), "2.9");
}

/// Starts a foreground KataGo and answers its handshake until it analyses.
fn foreground(f: &mut Fixture) -> InstanceId {
    f.wb.save_engine_settings(
        vec![EngineProfile {
            name: "KataGo".into(),
            command: "/fake/foreground gtp".into(),
            ..EngineProfile::default()
        }],
        EngineStartup::Default,
    )
    .unwrap();
    f.wb.start_engines();
    let id = f.world.last();
    serve(f, id);
    id
}

/// Answers the foreground engine's commands until none is pending.
fn serve(f: &mut Fixture, id: InstanceId) {
    for _ in 0..30 {
        let pending = f.world.take(id);
        if pending.is_empty() {
            return;
        }
        for line in pending {
            let (n, command) = line.split_once(' ').unwrap();
            let word = command.split_whitespace().next().unwrap();
            let text = match word {
                "name" => f.gtp_name,
                "version" => "1.18.2",
                "list_commands" => "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nkata-analyze\nkata-set-rules\nkata-get-rules\nstop",
                "kata-get-rules" => r#"{"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N","friendlyPassOk":true,"hasButton":false}"#,
                _ => "",
            };
            f.wb.engine_input(id, EngineInput::Stdout(String::new()));
            f.wb.engine_input(id, EngineInput::Stdout(format!("={n} {text}")));
            if word != "kata-analyze" {
                f.wb.engine_input(id, EngineInput::Stdout(String::new()));
            }
        }
    }
    panic!("the foreground engine kept asking");
}

#[test]
fn ordinary_estimate_ignores_candidates_beyond_the_suggestion_limit() {
    let mut f = fixture();
    open(&mut f.wb, "(;SZ[19]KM[7.5];B[pd])");
    let id = foreground(&mut f);
    let mut limits = f.wb.config().engine.analysis.clone();
    limits.limit_max_suggestion = 1;
    f.wb.set_analysis_settings(limits).unwrap();
    serve(&mut f, id);
    // Root, Black to move: Q16 (pd) is the second candidate, beyond the limit of one.
    f.wb.engine_input(id, EngineInput::Stdout("info move D4 visits 100 winrate 0.55 scoreLead 2 prior 0.2 order 0 pv D4 info move Q16 visits 50 winrate 0.5 scoreLead 1 prior 0.1 order 1 pv Q16".into()));
    f.wb.navigate(Navigation::Forward(1));
    serve(&mut f, id);
    f.wb.engine_input(
        id,
        EngineInput::Stdout("info move D4 visits 80 winrate 0.52 scoreLead 0.5 prior 0.2 order 0 pv D4".into()),
    );
    let r = report(&mut f.wb);
    let s = &r.black.samples[0];
    // Java keeps Q16 in bestMovesOutOfRange (BoardData.tryToLimitMoves), which the estimator does
    // not read: the move is no candidate and its loss is the positions' difference, 0.5 + 2.
    assert_eq!((s.ai_rank, s.first_choice), (999, false));
    assert_eq!(s.score_loss, Some(2.5));
}

#[test]
fn a_candidate_kept_beyond_the_limit_still_counts_as_one() {
    let mut f = fixture();
    open(&mut f.wb, "(;SZ[19]KM[7.5];B[pd])");
    let id = foreground(&mut f);
    let mut limits = f.wb.config().engine.analysis.clone();
    limits.limit_max_suggestion = 1;
    f.wb.set_analysis_settings(limits).unwrap();
    serve(&mut f, id);
    // Q16 leads, then drops to second while its search still grows: BoardData.tryToLimitMoves
    // keeps it (lastTimeUnlimited), so the estimate takes the move as the second candidate.
    f.wb.engine_input(id, EngineInput::Stdout("info move Q16 visits 60 winrate 0.55 scoreLead 2 prior 0.2 order 0 pv Q16 info move D4 visits 40 winrate 0.5 scoreLead 1 prior 0.1 order 1 pv D4".into()));
    f.wb.engine_input(id, EngineInput::Stdout("info move D4 visits 100 winrate 0.56 scoreLead 2 prior 0.2 order 0 pv D4 info move Q16 visits 70 winrate 0.54 scoreLead 1.5 prior 0.1 order 1 pv Q16".into()));
    assert!(f.wb.view(false).analysis.unwrap().candidates[1].kept);
    f.wb.navigate(Navigation::Forward(1));
    serve(&mut f, id);
    f.wb.engine_input(
        id,
        EngineInput::Stdout("info move D4 visits 80 winrate 0.52 scoreLead 0.5 prior 0.2 order 0 pv D4".into()),
    );
    let s = &report(&mut f.wb).black.samples[0];
    assert_eq!((s.ai_rank, s.first_choice), (1, false));
    assert_eq!(s.score_loss, Some(0.5), "the candidates' difference, 2 - 1.5");
}

#[test]
fn a_tracked_candidate_beyond_the_limit_counts_while_its_tracking_lasted() {
    // 选点重点研究 on Q16 (pd), third of a limit of one: the result that comes while it is tracked
    // keeps it in the node's candidates (tryToLimitMoves' attended points), and leaving the
    // position keeps that; clearing the point first drops it (refreshFocusCandidateVisibility).
    for (clear, expected) in [(false, (1, Some(0.5))), (true, (999, Some(2.5)))] {
        let mut f = fixture();
        open(&mut f.wb, "(;SZ[19]KM[7.5];B[pd])");
        let id = foreground(&mut f);
        let mut limits = f.wb.config().engine.analysis.clone();
        limits.limit_max_suggestion = 1;
        f.wb.set_analysis_settings(limits).unwrap();
        serve(&mut f, id);
        let q16 = Point { x: 15, y: 3 };
        assert!(f.wb.tracking(TrackingCommand::Track { point: q16 }).unwrap());
        serve(&mut f, id);
        f.wb.engine_input(id, EngineInput::Stdout("info move D4 visits 100 winrate 0.56 scoreLead 2 prior 0.2 order 0 pv D4 info move C3 visits 60 winrate 0.55 scoreLead 1.8 prior 0.1 order 1 pv C3 info move Q16 visits 40 winrate 0.54 scoreLead 1.5 prior 0.1 order 2 pv Q16".into()));
        if clear {
            assert!(f.wb.tracking(TrackingCommand::Clear).unwrap());
            serve(&mut f, id);
        }
        f.wb.navigate(Navigation::Forward(1));
        serve(&mut f, id);
        f.wb.engine_input(
            id,
            EngineInput::Stdout("info move D4 visits 80 winrate 0.52 scoreLead 0.5 prior 0.2 order 0 pv D4".into()),
        );
        assert!(f.wb.view(false).focus.points.is_empty());
        let s = &report(&mut f.wb).black.samples[0];
        assert_eq!((s.ai_rank, s.score_loss), expected, "cleared first: {clear}");
    }
}

#[test]
fn a_priority_point_beyond_the_limit_counts_as_a_candidate() {
    // 优先分析此点 on Q16 (pd), second of a limit of one, on a KataGoPda: BoardData.tryToLimitMoves
    // keeps a priorityMoveCoords point in bestMoves, so the estimate takes the move as the second
    // candidate (the ordinary case without it is ai_rank 999, score_loss 2.5).
    let mut f = fixture();
    f.gtp_name = "KataGoPda 1.16.4";
    open(&mut f.wb, "(;SZ[19]KM[7.5];B[pd])");
    let id = foreground(&mut f);
    let mut limits = f.wb.config().engine.analysis.clone();
    limits.limit_max_suggestion = 1;
    f.wb.set_analysis_settings(limits).unwrap();
    serve(&mut f, id);
    let q16 = Point { x: 15, y: 3 };
    assert!(f.wb.set_priority_point(q16).unwrap());
    serve(&mut f, id);
    f.wb.engine_input(id, EngineInput::Stdout("info move D4 visits 100 winrate 0.55 scoreLead 2 prior 0.2 order 0 pv D4 info move Q16 visits 50 winrate 0.5 scoreLead 1 prior 0.1 order 1 pv Q16".into()));
    f.wb.navigate(Navigation::Forward(1));
    serve(&mut f, id);
    f.wb.engine_input(
        id,
        EngineInput::Stdout("info move D4 visits 80 winrate 0.52 scoreLead 0.5 prior 0.2 order 0 pv D4".into()),
    );
    let s = &report(&mut f.wb).black.samples[0];
    assert_eq!((s.ai_rank, s.first_choice), (1, false));
    assert_eq!(s.score_loss, Some(1.0), "the candidates' difference, 2 - 1");
}

#[test]
fn a_background_choice_that_cannot_be_saved_reports_why_and_stays_off() {
    let mut f = fixture();
    let config = f.dir.path().join("config").join("config.json");
    std::fs::remove_file(&config).unwrap();
    std::fs::create_dir(&config).unwrap();
    // LizzieFrame: the box returns to the saved choice and the save error's message is shown.
    let error =
        f.wb.strength(StrengthCommand::SetBackground { enabled: true })
            .unwrap_err();
    assert!(error.message.starts_with(&format!("{SETTINGS_SAVE_FAILED}\n")), "{}", error.message);
    assert!(!f.wb.view(false).strength.background);
}

#[test]
fn background_preparation_follows_foreground_analysis_and_pause() {
    let mut f = fixture();
    f.wb.strength(StrengthCommand::SetBackground { enabled: true }).unwrap();
    foreground(&mut f);
    assert!(f.wb.view(false).engine.analysing);
    let before = f.world.launched();
    let mut now = Instant::now();
    for _ in 0..200 {
        now += Duration::from_millis(1100);
        f.wb.engine_tick(now);
        if f.world.launched() > before {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let worker = f.world.last();
    assert!(f.world.launched() > before, "{:?}", f.wb.view(false).strength.status);
    assert_eq!(
        f.wb.view(false).strength.manual,
        None,
        "a background batch is not a manual run"
    );
    // Pausing the user's analysis stops the background batch (Java observe: userPaused).
    f.wb.toggle_analysis().unwrap();
    f.wb.engine_tick(now + Duration::from_secs(2));
    assert!(f.world.dead(worker));
    assert_eq!(f.wb.view(false).strength.status.key, "HumanSL40.backgroundPaused");
}

#[test]
fn a_changed_suggestion_limit_leaves_loaded_analysis_as_it_was_split() {
    // SGFParser limits saved candidates when the record is loaded; changing 最大候选点数 later does
    // not rebuild bestMoves, which PlayerStrengthEstimator.findPlayedCandidate reads as they are.
    let sgf = "(;SZ[19]KM[7.5]LZOP[b18 55.0 150 2.0 0.5 0 rootVisits=150\n\
               move D4 visits 100 winrate 5500 prior 2000 scoreMean 2 order 0 pv D4 info \
               move Q16 visits 50 winrate 5000 prior 1000 scoreMean 1 order 1 pv Q16]\
               ;B[pd]LZ[b18 48.0 80 0.5 0.5 0 rootVisits=80\n\
               move D4 visits 80 winrate 5200 prior 2000 scoreMean 0.5 order 0 pv D4])";
    let mut f = fixture();
    let mut limits = f.wb.config().engine.analysis.clone();
    limits.limit_max_suggestion = 2;
    f.wb.set_analysis_settings(limits.clone()).unwrap();
    open(&mut f.wb, sgf);
    // Root: D4 first, Q16 (pd, the move played) second: the candidates' difference, 2 - 1.
    let rank = |r: &StrengthReport| (r.black.samples[0].ai_rank, r.black.samples[0].score_loss);
    assert_eq!(rank(&report(&mut f.wb)), (1, Some(1.0)));
    limits.limit_max_suggestion = 1;
    f.wb.set_analysis_settings(limits).unwrap();
    assert_eq!(rank(&report(&mut f.wb)), (1, Some(1.0)), "only the setting changed");
    // Loaded again under the limit of one, Q16 is no candidate: the positions' difference, 0.5 + 2.
    open(&mut f.wb, sgf);
    assert_eq!(rank(&report(&mut f.wb)), (999, Some(2.5)));
}

#[test]
fn trial_play_keeps_the_suggestion_limit_its_continuations_were_split_with() {
    // 试下 sets the continuations aside and brings them back; their results keep the 最大候选点数
    // they came with, so a candidate past it stays out of the estimate.
    let mut f = fixture();
    open(&mut f.wb, "(;SZ[19]KM[7.5];B[dd];W[pd])");
    let id = foreground(&mut f);
    let mut limits = f.wb.config().engine.analysis.clone();
    limits.limit_max_suggestion = 1;
    f.wb.set_analysis_settings(limits).unwrap();
    f.wb.navigate(Navigation::Forward(1));
    serve(&mut f, id);
    // After B[dd], White to move: Q16 (pd, played) is the second candidate, beyond the limit of one.
    f.wb.engine_input(id, EngineInput::Stdout("info move D4 visits 100 winrate 0.55 scoreLead 2 prior 0.2 order 0 pv D4 info move Q16 visits 50 winrate 0.5 scoreLead 1 prior 0.1 order 1 pv Q16".into()));
    f.wb.navigate(Navigation::Forward(1));
    serve(&mut f, id);
    f.wb.engine_input(
        id,
        EngineInput::Stdout("info move D4 visits 80 winrate 0.52 scoreLead 0.5 prior 0.2 order 0 pv D4".into()),
    );
    let rank = |r: &StrengthReport| (r.white.samples[0].ai_rank, r.white.samples[0].score_loss);
    assert_eq!(rank(&report(&mut f.wb)), (999, Some(2.5)), "the positions' difference, 0.5 + 2");
    f.wb.navigate(Navigation::First);
    serve(&mut f, id);
    f.wb.edit(EditCommand::TryPlay).unwrap();
    serve(&mut f, id);
    f.wb.edit(EditCommand::TryPlay).unwrap();
    serve(&mut f, id);
    assert_eq!(rank(&report(&mut f.wb)), (999, Some(2.5)), "Q16 is still past the limit");
}

/// The GTP vertex of SGF point (`x`, `y`) on 19 × 19.
fn vertex(x: usize, y: usize) -> String {
    format!("{}{}", "ABCDEFGHJKLMNOPQRST".as_bytes()[x] as char, 19 - y)
}

/// Move `k` (1-based) of [`long_game`]: Black fills the top rows and White the bottom ones from the
/// left, so that no stone is ever captured.
fn long_move(k: usize) -> (usize, usize) {
    let j = (k - 1) / 2;
    if k % 2 == 1 {
        (j % 19, j / 19)
    } else {
        (j % 19, 18 - j / 19)
    }
}

/// A fixed game of `moves` moves (at most 266) with saved KataGo analysis on every position. Each
/// analysis offers the next move and one point of the empty middle rows; Black misses the better
/// one more often as the game goes on (every fifth move of the first 60, third up to 150, then
/// every other), White every fourth move. A miss loses three points.
fn long_game(moves: usize) -> String {
    let missed = |k: usize| {
        if k % 2 == 0 {
            k % 4 == 0
        } else if k <= 60 {
            k % 5 == 0
        } else if k <= 150 {
            k % 3 == 0
        } else {
            k % 2 == 1 && (k / 2) % 2 == 0
        }
    };
    scored_game(moves, |k| if missed(k) { 3.0 } else { 0.0 })
}

/// [`long_game`]'s moves, move `k` losing `loss(k)` points against the better point (0: it is the
/// better point; then the other point loses three).
fn scored_game(moves: usize, loss: impl Fn(usize) -> f64) -> String {
    let mut sgf = String::from("(;GM[1]FF[4]CA[UTF-8]SZ[19]KM[7.5]RU[Chinese]PB[Black]PW[White]");
    for k in 0..=moves {
        if k > 0 {
            let (x, y) = long_move(k);
            let colour = if k % 2 == 1 { "B" } else { "W" };
            sgf.push_str(&format!("\n;{colour}[{}{}]", (b'a' + x as u8) as char, (b'a' + y as u8) as char));
        }
        let next = k + 1;
        let (nx, ny) = long_move(next.min(266));
        let played = vertex(nx, ny);
        let other = vertex(k % 19, 9);
        let lost = if next <= moves { loss(next) } else { 0.0 };
        let (top, second, second_score) = if lost > 0.0 { (other, played, 0.5 - lost) } else { (played, other, -2.5) };
        sgf.push_str(&format!(
            "LZ[KataGo 50.0 500 0.5 10.0 0.0 rootVisits=500\nmove {top} visits 300 winrate 5000 prior 3000 scoreMean 0.50 order 0 edgeVisits 300 pv {top} info move {second} visits 200 winrate 4700 prior 2000 scoreMean {second_score:.2} order 1 edgeVisits 200 pv {second}]"
        ));
    }
    sgf.push_str(")\n");
    sgf
}

/// How a side plays a phase of [`phased_game`]: every `nth` of its moves there (0: none) loses
/// `points`.
#[derive(Clone, Copy, Debug)]
struct Misses {
    nth: usize,
    points: f64,
}

const fn misses(nth: usize, points: f64) -> Misses {
    Misses { nth, points }
}

/// [`scored_game`] with each side's misses chosen per phase at the default boundaries (60, 150):
/// `black[i]` and `white[i]` in phase `i`.
fn phased_game(moves: usize, black: [Misses; 3], white: [Misses; 3]) -> String {
    scored_game(moves, |k| {
        let (phase, first) = if k <= 60 {
            (0, 1)
        } else if k <= 150 {
            (1, 61)
        } else {
            (2, 151)
        };
        let m = if k % 2 == 1 { black[phase] } else { white[phase] };
        // The side's j-th move of the phase.
        let j = (k - first) / 2;
        if m.nth > 0 && j % m.nth == m.nth - 1 {
            m.points
        } else {
            0.0
        }
    })
}

fn phases_now(wb: &mut Workbench) -> StrengthPhases {
    match wb.strength(StrengthCommand::Report).unwrap() {
        CommandResult::Strength { phases, .. } => *phases,
        other => panic!("{other:?}"),
    }
}

fn set_bounds(wb: &mut Workbench, bounds: [u32; 2]) -> StrengthPhases {
    match wb.strength(StrengthCommand::SetPhaseBounds { bounds }).unwrap() {
        CommandResult::StrengthPhases { phases } => *phases,
        other => panic!("{other:?}"),
    }
}

/// Each side's samples of moves `first` to `last` of `side`, estimated on their own.
fn alone(side: &StrengthSideReport, model: &StrengthModel, first: u32, last: u32) -> StrengthSummary {
    let part: Vec<_> = side.samples.iter().filter(|s| (first..=last).contains(&s.move_number)).cloned().collect();
    lizzie_core::strength::summarize_samples(&part, model).summary
}

/// Java `StrengthPhases.split`: opening 1–60, middle game 61–150, endgame to the end by default,
/// each side's estimate the whole-game model's over that side's moves of the phase alone.
#[test]
fn each_phase_is_estimated_over_its_own_moves_by_the_whole_game_model() {
    let mut f = still();
    open(&mut f.wb, &long_game(200));
    let (r, _) = estimate(&mut f.wb);
    let p = phases_now(&mut f.wb);
    let model = StrengthModel::xgboost20tun();
    assert_eq!(r.model.id, model.id);
    assert_eq!((p.bounds, p.default_bounds, p.min_phase_moves, p.last_move), ([60, 150], [60, 150], 10, 200));
    let spans: Vec<(u32, u32)> = p.phases.iter().map(|ph| (ph.first, ph.last)).collect();
    assert_eq!(spans, [(1, 60), (61, 150), (151, 200)]);
    for ph in &p.phases {
        assert_eq!(ph.black.summary, alone(&r.black, &model, ph.first, ph.last), "{}–{}", ph.first, ph.last);
        assert_eq!(ph.white.summary, alone(&r.white, &model, ph.first, ph.last), "{}–{}", ph.first, ph.last);
        assert!(ph.black.ranked && ph.white.ranked);
    }
    let counts: Vec<(u32, u32)> = p.phases.iter().map(|ph| (ph.black.summary.sample_count, ph.white.summary.sample_count)).collect();
    assert_eq!(counts, [(30, 30), (45, 45), (25, 25)]);
    // Black misses more moves phase by phase, so its opening is its best phase and the endgame its
    // weakest.
    assert_eq!(p.phases[0].black.badge, Some(PhaseBadge::Best));
    assert_eq!(p.phases[2].black.badge, Some(PhaseBadge::Weakest));
}

/// Moved boundaries split the game anew and are kept for this and later games, also after a
/// restart; 重置 sends the defaults.
#[test]
fn moved_boundaries_change_the_estimates_and_are_kept_for_later_games() {
    let mut f = still();
    open(&mut f.wb, &long_game(200));
    let before = phases_now(&mut f.wb);
    let (r, _) = estimate(&mut f.wb);
    let model = StrengthModel::xgboost20tun();

    // A drag shows each step without keeping it.
    let preview = match f.wb.strength(StrengthCommand::PreviewPhases { bounds: [80, 180] }).unwrap() {
        CommandResult::StrengthPhases { phases } => *phases,
        other => panic!("{other:?}"),
    };
    assert_eq!(preview.bounds, [80, 180]);
    assert_eq!(f.wb.config().strength.phase_bounds, [60, 150]);
    assert_eq!(phases_now(&mut f.wb).bounds, [60, 150]);

    let moved = set_bounds(&mut f.wb, [80, 180]);
    assert_eq!(moved, preview);
    assert_eq!(moved.phases[0].black.summary, alone(&r.black, &model, 1, 80));
    assert_eq!(moved.phases[0].black.summary.sample_count, 40);
    assert_ne!(moved.phases[0].black.summary, before.phases[0].black.summary);
    assert_eq!(moved.phases[2].white.summary, alone(&r.white, &model, 181, 200));

    let storage = Storage::in_dirs(&f.dir.path().join("config"), &f.dir.path().join("data"));
    let mut again = Workbench::new(storage, None, Box::new(f.world.clone()), vec![]);
    assert_eq!(again.config().strength.phase_bounds, [80, 180]);
    open(&mut again, &long_game(220));
    let later = phases_now(&mut again);
    assert_eq!((later.bounds, later.phases[2].first, later.phases[2].last), ([80, 180], 181, 220));

    let reset = set_bounds(&mut again, [60, 150]);
    assert_eq!(reset.bounds, [60, 150]);
    assert_eq!(again.config().strength.phase_bounds, [60, 150]);
}

/// Java `StrengthPhaseCard.verdict`: a game of 100 moves never reaches the endgame; a phase in
/// which a side has fewer than six analysed moves has no rank and says so.
#[test]
fn an_unreached_phase_and_a_phase_with_too_few_moves_say_why_they_have_no_rank() {
    let mut f = still();
    open(&mut f.wb, &long_game(100));
    let p = phases_now(&mut f.wb);
    assert_eq!((p.phases[1].first, p.phases[1].last), (61, 100), "the middle game ends with the game");
    let end = &p.phases[2];
    assert_eq!(end.verdict, PhaseVerdict::NotReached);
    assert!(end.last < end.first && !end.black.ranked && !end.white.ranked);
    assert_eq!(end.black.summary.sample_count, 0);

    // A middle game of ten moves: five per side.
    let p = set_bounds(&mut f.wb, [60, 70]);
    assert_eq!(p.phases[1].black.summary.sample_count, 5);
    assert!(!p.phases[1].black.ranked);
    assert_eq!(p.phases[1].verdict, PhaseVerdict::TooFew);
    assert_ne!(p.phases[0].verdict, PhaseVerdict::TooFew);
}

/// The human-style model reads the first 210 moves: later moves count in no phase, and a phase
/// that starts after them says so. The standard model estimates the same phase.
#[test]
fn the_human_style_model_leaves_out_moves_after_the_210th() {
    let mut f = fixture();
    open(&mut f.wb, &long_game(230));
    let standard = set_bounds(&mut f.wb, [150, 210]);
    assert_eq!((standard.phases[2].first, standard.phases[2].last), (211, 230));
    assert_eq!(standard.phases[2].black.summary.sample_count, 10);
    assert!(!matches!(standard.phases[2].verdict, PhaseVerdict::HumanStyleLimit { .. }));

    let before = f.world.launched();
    assert_eq!(f.wb.strength(RUN).unwrap(), CommandResult::Done);
    let worker = policy_worker(&mut f, before);
    for _ in 0..400 {
        if answer(&mut f, worker) == 0 && f.wb.view(false).strength.manual.unwrap().outcome.is_some() {
            break;
        }
        f.wb.engine_tick(Instant::now());
    }
    assert_eq!(f.wb.view(false).strength.manual.unwrap().outcome, Some(StrengthManualOutcome::Ready));
    f.wb.strength(StrengthCommand::SelectModel { id: "bundled:humansl40".into() }).unwrap();
    let (r, _) = estimate(&mut f.wb);
    assert!(r.model.human_sl && r.enough_data);
    let human = phases_now(&mut f.wb);
    assert_eq!(human.bounds, [150, 210]);
    assert_eq!(human.phases[2].verdict, PhaseVerdict::HumanStyleLimit { moves: 210 });
    let counted: u32 = human.phases.iter().map(|p| p.black.summary.sample_count + p.white.summary.sample_count).sum();
    assert_eq!(counted, 210);
    assert_eq!(human.phases[2].black.summary.sample_count, 0);
    assert!(human.phases[1].black.ranked, "151–210 is read");
}

/// Each phase's rank of `black`'s or White's side, `None` where it has too few moves to be ranked.
fn phase_ranks(p: &StrengthPhases, black: bool) -> Vec<Option<f64>> {
    p.phases
        .iter()
        .map(|ph| {
            let side = if black { &ph.black } else { &ph.white };
            side.ranked.then(|| side.summary.rank_value.unwrap())
        })
        .collect()
}

/// Java `StrengthPhases.EVEN` and `CLEAR`: a lead under 0.3 is even, under 1.0 a little better,
/// from 1.0 clearly better; each boundary from both sides, with Black and then White ahead. The
/// opening of a 60-move game: each side misses as given, which puts the lead next to a boundary.
#[test]
fn who_played_better_follows_the_even_and_clear_thresholds() {
    let mut f = still();
    let cases: [(Misses, Misses, std::ops::Range<f64>, fn(bool) -> PhaseVerdict); 4] = [
        (misses(4, 4.0), misses(2, 1.5), 0.28..0.3, |_| PhaseVerdict::Even),
        (misses(6, 6.0), misses(2, 1.5), 0.3..0.32, |black| PhaseVerdict::Slightly { black }),
        (misses(5, 0.8), misses(8, 4.0), 0.98..1.0, |black| PhaseVerdict::Slightly { black }),
        (misses(15, 1.2), misses(8, 4.0), 1.0..1.02, |black| PhaseVerdict::Clearly { black }),
    ];
    for (ahead, behind, lead, verdict) in cases {
        for black_ahead in [true, false] {
            let (black, white) = if black_ahead { (ahead, behind) } else { (behind, ahead) };
            open(&mut f.wb, &phased_game(60, [black; 3], [white; 3]));
            let p = phases_now(&mut f.wb);
            let (b, w) = (phase_ranks(&p, true)[0].unwrap(), phase_ranks(&p, false)[0].unwrap());
            let shown = if black_ahead { b - w } else { w - b };
            assert!(lead.contains(&shown), "{ahead:?} against {behind:?} leads by {shown}, not {lead:?}");
            assert_eq!(p.phases[0].verdict, verdict(black_ahead), "{ahead:?} against {behind:?}, Black ahead: {black_ahead}");
            assert_eq!(p.phases[1].verdict, PhaseVerdict::NotReached);
        }
    }
}

/// Java `StrengthPhases.extreme`, `worthReviewing` and `StrengthPhaseView.review`, each side's
/// ranks set by how it misses phase by phase.
#[test]
fn best_and_weakest_phases_and_what_is_worth_a_review() {
    let mut f = still();
    let spread = |ranks: &[Option<f64>]| {
        let ranked: Vec<f64> = ranks.iter().flatten().copied().collect();
        ranked.iter().copied().fold(f64::NEG_INFINITY, f64::max) - ranked.iter().copied().fold(f64::INFINITY, f64::min)
    };
    let badges = |p: &StrengthPhases| p.phases.iter().map(|ph| (ph.black.badge, ph.white.badge)).collect::<Vec<_>>();
    // White misses every move by 0.8 throughout: its phases lie within 0.3 of each other.
    let steady = [misses(1, 0.8); 3];

    // Black: best in the middle game, weakest in the endgame, at least 0.6 apart.
    open(&mut f.wb, &phased_game(200, [misses(1, 0.3), misses(3, 0.8), misses(5, 2.5)], steady));
    let p = phases_now(&mut f.wb);
    let [o, m, e] = phase_ranks(&p, true).try_into().unwrap();
    let (o, m, e) = (o.unwrap(), m.unwrap(), e.unwrap());
    assert!(m > o && o > e && m - e >= 0.6, "Black {o} / {m} / {e}");
    assert!(spread(&phase_ranks(&p, false)) < 0.3, "{:?}", phase_ranks(&p, false));
    assert_eq!(badges(&p), [(None, None), (Some(PhaseBadge::Best), None), (Some(PhaseBadge::Weakest), None)]);
    assert_eq!(p.review, PhaseReview::One { black: true, phase: 2 });

    // Badges from 0.3 apart, a review from 0.6. In 160 moves the endgame is too short to rank.
    open(&mut f.wb, &phased_game(160, [misses(1, 0.3), misses(3, 0.8), misses(1, 0.3)], steady));
    let p = phases_now(&mut f.wb);
    let black = phase_ranks(&p, true);
    assert!(black[2].is_none() && (0.3..0.6).contains(&spread(&black)), "Black {black:?}");
    assert!(black[1] > black[0]);
    assert_eq!(badges(&p), [(Some(PhaseBadge::Weakest), None), (Some(PhaseBadge::Best), None), (None, None)]);
    assert_eq!(p.review, PhaseReview::Steady);

    // Both sides weakest in the opening, at least 0.6 below the middle game; then White weakest in
    // the middle game instead.
    let black = [misses(1, 0.8), misses(4, 1.2), misses(1, 0.3)];
    open(&mut f.wb, &phased_game(160, black, [misses(1, 0.3), misses(2, 0.3), misses(1, 0.3)]));
    let p = phases_now(&mut f.wb);
    for side in [true, false] {
        let r = phase_ranks(&p, side);
        assert!(r[1].unwrap() - r[0].unwrap() >= 0.6, "{side}: {r:?}");
    }
    assert_eq!(p.review, PhaseReview::Same { phase: 0 });
    open(&mut f.wb, &phased_game(160, black, [misses(0, 0.0), misses(6, 2.5), misses(1, 0.3)]));
    let p = phases_now(&mut f.wb);
    let white = phase_ranks(&p, false);
    assert!(white[0].unwrap() - white[1].unwrap() >= 0.6, "White {white:?}");
    assert_eq!(p.review, PhaseReview::Both { black: 0, white: 1 });

    // Fewer than two ranked phases on a side: nothing to compare. In 70 moves only the opening is.
    open(&mut f.wb, &phased_game(70, black, steady));
    let p = phases_now(&mut f.wb);
    assert_eq!(phase_ranks(&p, true).iter().flatten().count(), 1);
    assert_eq!(p.review, PhaseReview::None);
    assert!(p.phases.iter().all(|ph| ph.black.badge.is_none() && ph.white.badge.is_none()));
}

/// Boundaries that are not two integers, are reversed or equal, leave the opening or the middle
/// game under ten moves or end the middle game past move 1000 are read as the defaults; the rest of the
/// settings still load, a save writes the defaults and the phases split there. Asked to keep such
/// boundaries, the dialog keeps the defaults.
#[test]
fn unusable_phase_boundaries_fall_back_to_the_defaults() {
    let f = still();
    let storage = || Storage::in_dirs(&f.dir.path().join("config"), &f.dir.path().join("data"));
    let file = storage().config_file;
    let saved = || -> Value { serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap() };
    for stored in [
        json!([3, 12]),
        json!([80, 85]),
        json!([5, 100]),
        json!([60.5, 150]),
        json!(["60", 150]),
        json!([150, 60]),
        json!([60, 60]),
        json!([60]),
        json!([60, 150, 200]),
        json!([-60, 150]),
        json!("60,150"),
        json!(null),
        json!([4294967290_u32, u32::MAX]),
        json!([u32::MAX, u32::MAX]),
        json!([60, u32::MAX]),
        json!([60, u32::MAX - 1]),
        json!([60, 1001]),
        json!([1000, 1010]),
    ] {
        let mut config = saved();
        config["strength"] = json!({ "humanslBackground": true, "phaseBounds": stored });
        std::fs::write(&file, config.to_string()).unwrap();
        let mut wb = Workbench::new(storage(), None, Box::new(f.world.clone()), vec![]);
        assert_eq!(wb.config().strength.phase_bounds, [60, 150], "{stored}");
        assert!(wb.config().strength.humansl_background, "{stored}");
        open(&mut wb, &long_game(200));
        let p = phases_now(&mut wb);
        let spans: Vec<(u32, u32)> = p.phases.iter().map(|ph| (ph.first, ph.last)).collect();
        assert_eq!((p.bounds, spans), ([60, 150], vec![(1, 60), (61, 150), (151, 200)]), "{stored}");
        // Any settings save writes the whole configuration: the defaults, the other settings kept.
        wb.strength(StrengthCommand::SetBackground { enabled: true }).unwrap();
        assert_eq!(saved()["strength"]["phaseBounds"], json!([60, 150]), "{stored}");
        assert_eq!(saved()["strength"]["humanslBackground"], json!(true), "{stored}");
    }
    // The latest middle game a stored file can end at is kept.
    let mut config = saved();
    config["strength"] = json!({ "phaseBounds": [990, 1000] });
    std::fs::write(&file, config.to_string()).unwrap();
    let wb = Workbench::new(storage(), None, Box::new(f.world.clone()), vec![]);
    assert_eq!(wb.config().strength.phase_bounds, [990, 1000]);

    let mut f = still();
    open(&mut f.wb, &long_game(200));
    let file = Storage::in_dirs(&f.dir.path().join("config"), &f.dir.path().join("data")).config_file;
    for bounds in [
        [3, 12],
        [80, 85],
        [150, 60],
        [60, 60],
        [4294967290, u32::MAX],
        [u32::MAX, u32::MAX],
        [60, u32::MAX],
        [60, u32::MAX - 1],
        [60, 1001],
        [1000, 1010],
    ] {
        set_bounds(&mut f.wb, [70, 160]);
        assert_eq!(set_bounds(&mut f.wb, bounds).bounds, [60, 150], "{bounds:?}");
        assert_eq!(f.wb.config().strength.phase_bounds, [60, 150], "{bounds:?}");
        let saved: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(saved["strength"]["phaseBounds"], json!([60, 150]), "{bounds:?}");
        assert_eq!(phases_now(&mut f.wb).bounds, [60, 150], "{bounds:?}");
    }
    // A middle game ending at move 1000 is kept; past the end of the game the endgame is not reached.
    let p = set_bounds(&mut f.wb, [60, 1000]);
    let spans: Vec<(u32, u32)> = p.phases.iter().map(|ph| (ph.first, ph.last)).collect();
    assert_eq!(spans, [(1, 60), (61, 200), (1001, 200)]);
    assert_eq!(p.phases[2].verdict, PhaseVerdict::NotReached);
    assert_eq!((p.max_bound, phases_now(&mut f.wb).bounds), (1000, [60, 1000]));
}

/// Past the main line's first 211 positions a view counts the analysed ones instead of
/// fingerprinting them: a result arriving there, and its clearing, still move the stamp on.
#[test]
fn the_stamp_follows_new_analysis_late_in_the_main_line() {
    let mut f = still();
    let mut sgf = String::from("(;SZ[19]KM[7.5]");
    for k in 1..=230 {
        let (x, y) = long_move(k);
        let colour = if k % 2 == 1 { "B" } else { "W" };
        sgf.push_str(&format!(";{colour}[{}{}]", (b'a' + x as u8) as char, (b'a' + y as u8) as char));
    }
    sgf.push(')');
    open(&mut f.wb, &sgf);
    let id = foreground(&mut f);
    f.wb.navigate(Navigation::Forward(220));
    serve(&mut f, id);
    let before = f.wb.view(false).strength.stamp;
    assert_eq!(f.wb.view(false).strength.stamp, before, "nothing changed");
    // The middle rows stay empty in this game.
    let point = vertex(9, 9);
    f.wb.engine_input(
        id,
        EngineInput::Stdout(format!("info move {point} visits 100 winrate 0.55 scoreLead 2 prior 0.2 order 0 pv {point}")),
    );
    let v = f.wb.view(false);
    assert_eq!(v.position.move_number, 220);
    assert!(v.analysis.is_some_and(|a| a.visits > 0), "move 220 is analysed");
    assert_ne!(v.strength.stamp, before);
    // Clearing that move's analysis takes it out of the count again.
    f.wb.clear_analysis_info(lizzie_core::api::AnalysisScope::Current);
    let v = f.wb.view(false);
    assert!(v.analysis.is_none_or(|a| a.visits == 0), "move 220's analysis is cleared");
    assert_eq!(v.strength.stamp, before);
}

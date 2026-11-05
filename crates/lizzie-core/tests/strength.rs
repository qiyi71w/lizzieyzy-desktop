//! 棋力评估 through the public `Workbench::strength` command and view, with saved analysis read
//! from SGF and a controlled HumanSL 40 policy process.
use lizzie_core::api::{CommandResult, OpenSource, StrengthCommand, StrengthManualOutcome, StrengthReport};
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats;
use lizzie_core::record::RecordFormat;
use lizzie_core::settings::{self, Config, Storage};
use lizzie_core::workbench::{OpenOrigin, Workbench};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// 61 moves, every position analysed (KataGo payload), Java's saved-analysis format. The fixed Java
/// build shows this game as Black 业余2.9段 / White 业余7.8段 (acceptance screenshot j01).
const GAME: &str = include_str!("fixtures/strength/seed12-kata-61.sgf");
/// 补算 HumanSL as the dashboard first sends it.
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
}

/// A workbench whose analysis command names a local KataGo stand-in and whose HumanSL model is a
/// stand-in admitted by its own digest.
fn fixture() -> Fixture {
    fixture_with(|config, engine| {
        config.analysis_engine.command = format!("{} analysis -config a.cfg -model m.bin.gz", engine.display());
        config.analysis_engine.command_customized = true;
    })
}

/// [`fixture`] with the KataGo stand-in at `engine` named by `setup` instead.
fn fixture_with(setup: impl FnOnce(&mut Config, &std::path::Path)) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let engine = dir.path().join("katago");
    std::fs::write(&engine, b"stand-in engine").unwrap();
    let human = dir.path().join("human.bin.gz");
    std::fs::write(&human, b"stand-in HumanSL network").unwrap();
    let storage = Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data"));
    let mut config = Config::default();
    setup(&mut config, &engine);
    config.human_sl.model_path = Some(human.to_string_lossy().into_owned());
    settings::save_json(&storage.config_file, &config).unwrap();
    let world = World::default();
    let mut wb = Workbench::new(storage, None, Box::new(world.clone()), vec![]);
    wb.set_strength_human_digest(&lizzie_core::resources::sha256_file(&human).unwrap());
    open(&mut wb, GAME);
    Fixture { dir, wb, world }
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
    match wb.strength(StrengthCommand::Report).unwrap() {
        CommandResult::Strength { report } => *report,
        other => panic!("{other:?}"),
    }
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
        f.wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
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
                "name" => "KataGo",
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
fn a_background_choice_that_cannot_be_saved_reports_why_and_stays_off() {
    let mut f = fixture();
    let config = f.dir.path().join("config").join("config.json");
    std::fs::remove_file(&config).unwrap();
    std::fs::create_dir(&config).unwrap();
    // LizzieFrame: the box returns to the saved choice and the save error's message is shown.
    let error =
        f.wb.strength(StrengthCommand::SetBackground { enabled: true })
            .unwrap_err();
    assert!(error.message.starts_with("saving settings failed"), "{}", error.message);
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

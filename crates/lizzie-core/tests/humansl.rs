//! HumanSL's public command/engine-input seam with controlled analysis and foreground processes.
use lizzie_core::api::*;
use lizzie_core::browse::Navigation;
use lizzie_core::engine::{
    gtp::LaunchSpec,
    profile::{EngineProfile, EngineStartup},
    runtime::{EngineInput, EngineIo, InstanceId, KataParam, Launcher},
};
use lizzie_core::humansl::{CoachLevel, Pace};
use lizzie_core::record::{DocumentEdit, NodeId, NodeKind, Point, Stone, ROOT};
use lizzie_core::settings::{self, Config, HumanSlTrainingSettings, PlayerColor, Storage, TrainingMode};
use lizzie_core::workbench::Workbench;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
        self.world
            .0
            .lock()
            .unwrap()
            .iter_mut()
            .find(|p| p.id == self.id)
            .unwrap()
            .sent
            .push(line.into());
        Ok(())
    }
    fn terminate(self: Box<Self>) {
        self.world
            .0
            .lock()
            .unwrap()
            .iter_mut()
            .find(|p| p.id == self.id)
            .unwrap()
            .dead = true;
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
    fn take(&self, id: InstanceId) -> Vec<String> {
        std::mem::take(&mut self.0.lock().unwrap().iter_mut().find(|p| p.id == id).unwrap().sent)
    }
    fn worker(&self) -> InstanceId {
        self.0
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|p| p.args.first().is_some_and(|s| s == "analysis"))
            .unwrap()
            .id
    }
    fn dead(&self, id: InstanceId) -> bool {
        self.0.lock().unwrap().iter().find(|p| p.id == id).unwrap().dead
    }
    fn query(&self, id: InstanceId) -> Value {
        self.take(id)
            .into_iter()
            .filter_map(|s| serde_json::from_str::<Value>(&s).ok())
            .find(|v| v.get("action").is_none())
            .expect("analysis request")
    }
}
struct Fixture {
    dir: tempfile::TempDir,
    wb: Workbench,
    world: World,
}
fn fixture(model: bool) -> Fixture {
    fixture_with(model, |_| {})
}
fn fixture_with(model: bool, adjust: impl FnOnce(&mut Config)) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data"));
    let mut config = Config::default();
    config.analysis_engine.command = "/fake/katago analysis -config /fake/analysis.cfg -model /fake/main.bin.gz".into();
    config.analysis_engine.command_customized = true;
    if model {
        let path = dir.path().join("custom-human.bin.gz");
        std::fs::File::create(&path).unwrap().set_len(1024 * 1024 + 1).unwrap();
        config.human_sl.model_path = Some(path.to_string_lossy().into_owned());
    }
    adjust(&mut config);
    settings::save_json(&storage.config_file, &config).unwrap();
    let world = World::default();
    let mut wb = Workbench::new(storage, None, Box::new(world.clone()), vec![]);
    wb.set_human_sl_random_seed(123);
    Fixture { dir, wb, world }
}
fn settings(color: PlayerColor) -> HumanSlTrainingSettings {
    HumanSlTrainingSettings {
        player_color: color,
        ..HumanSlTrainingSettings::default()
    }
}
fn start(f: &mut Fixture, s: HumanSlTrainingSettings, current: bool) -> InstanceId {
    assert_eq!(
        f.wb.human_sl(HumanSlCommand::Start {
            settings: s,
            from_current: current,
            advanced_visible: true,
            stop_batch: false
        })
        .unwrap(),
        CommandResult::Done
    );
    f.world.worker()
}
fn out(wb: &mut Workbench, id: InstanceId, value: Value) {
    wb.engine_input(id, EngineInput::Stdout(value.to_string()));
}
fn ready(f: &mut Fixture, id: InstanceId) {
    let q = f.world.query(id);
    out(
        &mut f.wb,
        id,
        json!({"id":q["id"],"humanPolicy":{"D4":1.0},"moveInfos":[{"move":"D4","order":0,"utility":0.0,"visits":1}]}),
    );
}
fn answer(f: &mut Fixture, id: InstanceId, vertex: &str) {
    for _ in 0..6 {
        let pending = f.world.take(id);
        if pending.is_empty() {
            break;
        }
        for text in pending {
            let q: Value = serde_json::from_str(&text).unwrap();
            if q.get("action").is_some() {
                continue;
            }
            out(
                &mut f.wb,
                id,
                json!({"id":q["id"],"humanPolicy":{(vertex):1.0},"moveInfos":[{"move":vertex,"order":0,"utility":0.0,"visits":q["maxVisits"]}]}),
            );
        }
    }
    // Past the slowest pace's longest think time (慢, 14 s), so the chosen reply is played.
    f.wb.engine_tick(Instant::now() + Duration::from_secs(15));
}
fn foreground(f: &mut Fixture) -> InstanceId {
    foreground_with(f, CHINESE)
}
/// A foreground KataGo that reports `rules` (`kata-get-rules`).
fn foreground_with(f: &mut Fixture, rules: &str) -> InstanceId {
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
    let id = f.world.0.lock().unwrap().last().unwrap().id;
    serve_with(f, id, false, rules);
    id
}
/// KataGo's chinese rules, as `kata-get-rules` reports them.
const CHINESE: &str = r#"{"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N","friendlyPassOk":true,"hasButton":false}"#;
/// Answers the foreground engine's commands; returns them.
fn serve(f: &mut Fixture, id: InstanceId, fail_name: bool) -> Vec<String> {
    serve_with(f, id, fail_name, CHINESE)
}
fn serve_with(f: &mut Fixture, id: InstanceId, fail_name: bool, rules: &str) -> Vec<String> {
    let mut seen = Vec::new();
    for _ in 0..30 {
        let pending = f.world.take(id);
        if pending.is_empty() {
            return seen;
        }
        for line in pending {
            f.wb.engine_input(id, EngineInput::Stdout(String::new()));
            let (n, command) = line.split_once(' ').unwrap();
            let word = command.split_whitespace().next().unwrap();
            let text=match word {"name"=>"KataGo","version"=>"1.18.2","list_commands"=>"name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nset_position\nkata-analyze\nkata-set-rules\nkata-get-rules\nstop","kata-get-rules"=>rules,_=>""};
            f.wb.engine_input(
                id,
                EngineInput::Stdout(format!(
                    "{}{n} {text}",
                    if fail_name && word == "name" { "?" } else { "=" }
                )),
            );
            if word != "kata-analyze" {
                f.wb.engine_input(id, EngineInput::Stdout(String::new()));
            }
            seen.push(command.to_string());
        }
    }
    panic!("foreground handshake did not settle");
}

#[test]
fn missing_model_refuses_without_starting_a_process() {
    let mut f = fixture(false);
    assert!(matches!(
        f.wb.human_sl(HumanSlCommand::Start {
            settings: settings(PlayerColor::Black),
            from_current: false,
            advanced_visible: false,
            stop_batch: false
        })
        .unwrap(),
        CommandResult::Refused { .. }
    ));
    assert!(f.world.0.lock().unwrap().is_empty());
    assert!(f.wb.inspect().human_sl.is_none());
}
/// A two-stone game: the player takes Black and the stones whatever colour the settings chose,
/// with a komi of 0.5 (B2 `CoachSetupForm.config`, `HumanSlTrainingConfig.resolveHumanIsBlack`);
/// the board changes only once the companion is ready.
#[test]
fn preparation_is_transactional_and_success_persists_settings() {
    for color in [PlayerColor::White, PlayerColor::Random, PlayerColor::Black] {
        let mut f = fixture(true);
        f.wb.edit(EditCommand::Play {
            point: Point { x: 0, y: 0 },
            color: None,
        })
        .unwrap();
        let old = f.wb.document().id;
        let s = HumanSlTrainingSettings {
            handicap: 2,
            ..settings(color)
        };
        let id = start(&mut f, s, false);
        assert_eq!(f.wb.document().id, old);
        assert_eq!(f.wb.inspect().human_sl.unwrap().phase, HumanSlPhase::Preparing);
        ready(&mut f, id);
        let view = f.wb.inspect();
        assert_ne!(view.document.id, old);
        assert_eq!(view.document.info.komi, 0.5, "{color:?}");
        assert_eq!(view.komi, 0.5);
        assert_eq!(view.document.info.handicap, 2);
        assert_eq!(view.position.stones.iter().flatten().count(), 2);
        assert_eq!(view.position.to_play, Stone::White);
        let session = view.human_sl.unwrap();
        assert!(session.human_is_black, "{color:?}");
        assert!(!session.human_turn && session.ai_thinking);
        assert!(f.wb.config().human_sl.advanced_visible);
        let storage = f.wb.storage().clone();
        let reopened = Workbench::new(storage, None, Box::new(World::default()), vec![]);
        let saved = &reopened.config().human_sl.training;
        assert_eq!((saved.handicap, saved.player_color), (2, PlayerColor::Black));
    }
}
#[test]
fn ai_moves_first_and_human_clicks_are_turn_gated() {
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::White), false);
    ready(&mut f, id);
    assert!(f.wb.inspect().human_sl.unwrap().ai_thinking);
    assert_eq!(
        f.wb.edit(EditCommand::Play {
            point: Point { x: 0, y: 0 },
            color: None
        })
        .unwrap(),
        CommandResult::Unchanged
    );
    answer(&mut f, id, "D4");
    assert_eq!(f.wb.inspect().position.move_number, 1);
    assert_eq!(
        f.wb.edit(EditCommand::Play {
            point: Point { x: 0, y: 0 },
            color: None
        })
        .unwrap(),
        CommandResult::Done
    );
    assert_eq!(f.wb.inspect().position.move_number, 2);
    assert!(f.wb.inspect().human_sl.unwrap().ai_thinking);
    let args = f
        .world
        .0
        .lock()
        .unwrap()
        .iter()
        .find(|p| p.id == id)
        .unwrap()
        .args
        .clone();
    assert!(args.iter().any(|a| a == "-human-model"));
    assert!(args.iter().any(|a| a.contains("numAnalysisThreads=1")));
}
#[test]
fn from_current_forces_a_new_variation_for_stone_and_pass() {
    for pass in [false, true] {
        let mut f = fixture(true);
        let op = if pass {
            EditCommand::Pass
        } else {
            EditCommand::Play {
                point: Point { x: 3, y: 15 },
                color: None,
            }
        };
        f.wb.edit(op.clone()).unwrap();
        let existing = f.wb.cursor();
        f.wb.navigate(Navigation::First);
        let id = start(&mut f, settings(PlayerColor::Black), true);
        ready(&mut f, id);
        f.wb.edit(op).unwrap();
        assert_ne!(f.wb.cursor(), existing);
        assert_eq!(f.wb.document().record.node(ROOT).children.len(), 2);
    }
}
#[test]
fn early_ai_pass_is_retried_then_reported_and_retry_ai_recovers() {
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::White), false);
    ready(&mut f, id);
    for _ in 0..2 {
        let id = f.world.worker();
        let q = f.world.query(id);
        out(
            &mut f.wb,
            id,
            json!({"id":q["id"],"humanPolicy":{"pass":1.0},"moveInfos":[{"move":"pass","order":0,"utility":0.0}]}),
        );
        assert!(f.world.dead(id));
    }
    let view = f.wb.inspect();
    assert_eq!(view.position.move_number, 0);
    assert!(view.human_sl.unwrap().ai_failed);
    assert!(view.human_sl_notices.contains(&HumanSlNotice::AiMoveFailed));
    f.wb.human_sl(HumanSlCommand::RetryAi).unwrap();
    let retry = f.world.worker();
    assert_ne!(retry, id);
    answer(&mut f, retry, "D4");
    assert_eq!(f.wb.inspect().position.move_number, 1);
}
#[test]
fn readiness_timeout_worker_exit_and_cancel_leave_board_unchanged() {
    for mode in 0..3 {
        let mut f = fixture(true);
        let document = f.wb.document().id;
        let id = start(&mut f, settings(PlayerColor::Black), false);
        match mode {
            0 => {
                f.wb.engine_tick(Instant::now() + Duration::from_secs(181));
            }
            1 => {
                f.wb.engine_input(id, EngineInput::Exited(Some(1)));
            }
            _ => {
                f.wb.human_sl(HumanSlCommand::CancelPreparation).unwrap();
            }
        }
        let view = f.wb.inspect();
        assert_eq!(view.document.id, document);
        assert!(view.human_sl.is_none());
        assert!(f.world.dead(id));
        assert_eq!(
            view.human_sl_notices
                .iter()
                .any(|n| matches!(n, HumanSlNotice::StartFailed { .. })),
            mode != 2
        );
    }
}
#[test]
fn foreground_lease_display_and_recovery_retry_are_explicit() {
    let mut f = fixture(true);
    let fg = foreground(&mut f);
    let display = f.wb.config().display.clone();
    let id = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, id);
    assert_eq!(f.wb.inspect().engine.lease, LeaseAvailabilityView::HumanSlGame);
    assert_eq!(
        f.wb.inspect().human_sl.unwrap().analysis_display,
        HumanSlAnalysisDisplay::Suppress
    );
    assert_eq!(f.wb.config().display, display);
    f.wb.human_sl(HumanSlCommand::Finish).unwrap();
    serve(&mut f, fg, true);
    let view = f.wb.inspect();
    assert_eq!(view.human_sl.unwrap().phase, HumanSlPhase::CleanupPending);
    assert!(view.human_sl_notices.contains(&HumanSlNotice::ExitRecoveryFailed));
    f.wb.human_sl(HumanSlCommand::Finish).unwrap();
    serve(&mut f, fg, false);
    assert!(f.wb.inspect().human_sl.is_none());
    assert!(f.world.dead(id));
    assert!(!f.wb.inspect().engine.paused);
}
#[test]
fn live_analysis_keeps_foreground_running_and_does_not_mutate_display_preferences() {
    let mut f = fixture(true);
    let fg = foreground(&mut f);
    let display = f.wb.config().display.clone();
    let id = start(
        &mut f,
        HumanSlTrainingSettings {
            mode: TrainingMode::LiveAnalysis,
            ..settings(PlayerColor::Black)
        },
        false,
    );
    ready(&mut f, id);
    serve(&mut f, fg, false);
    assert_eq!(
        f.wb.inspect().human_sl.unwrap().analysis_display,
        HumanSlAnalysisDisplay::Show
    );
    assert!(!f.wb.inspect().engine.paused);
    assert_eq!(f.wb.config().display, display);
    f.wb.human_sl(HumanSlCommand::Finish).unwrap();
    serve(&mut f, fg, false);
    assert!(f.wb.inspect().human_sl.is_none());
}
/// The toolbar 激进度 / 广度 still set the paused foreground KataGo during AI 陪练, as Java's
/// toolbar does (`Menu.breadthAdmissionAvailable` follows the engine's own state only).
#[test]
fn toolbar_search_parameters_still_reach_the_foreground_engine_during_training() {
    let mut f = fixture(true);
    let fg = foreground(&mut f);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, id);
    serve(&mut f, fg, false);
    let set = Command::SetKataParam { param: KataParam::Wrn, enabled: true, value: 0.3 };
    assert_eq!(f.wb.game_gate(&set), None);
    assert_eq!(f.wb.set_kata_param(KataParam::Pda, true, 0.5).unwrap(), None);
    assert_eq!(f.wb.set_kata_param(KataParam::Wrn, true, 0.3).unwrap(), None);
    let sent = f.world.take(fg);
    for c in ["kata-set-param playoutDoublingAdvantage 0.5", "kata-set-param analysisWideRootNoise 0.3"] {
        assert!(sent.iter().any(|s| s.ends_with(c)), "{sent:?}");
    }
    assert!(f.wb.inspect().human_sl.is_some(), "the session goes on");
}
#[test]
fn deferred_open_is_released_only_after_exit_and_first_request_wins() {
    let mut f = fixture(true);
    let fg = foreground(&mut f);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, id);
    let first = Command::OpenRecord {
        path: "/first.sgf".into(),
        source: OpenSource::Drop,
    };
    let second = Command::OpenRecord {
        path: "/second.sgf".into(),
        source: OpenSource::Drop,
    };
    assert!(f.wb.game_gate(&first).is_some());
    assert!(f.wb.game_gate(&second).is_some());
    assert!(f.wb.take_human_sl_deferred().is_none());
    serve(&mut f, fg, false);
    assert_eq!(f.wb.take_human_sl_deferred(), Some(first));
    assert!(f.wb.take_human_sl_deferred().is_none());
    let old = f.wb.document().id;
    let ticket = f.wb.begin_open(lizzie_core::workbench::OpenOrigin::File {
        path: "/first.sgf".into(),
        source: OpenSource::Drop,
    });
    let record = lizzie_core::formats::sgf::read("(;SZ[19];B[aa])").unwrap();
    assert!(f
        .wb
        .complete_open(ticket, Ok((lizzie_core::record::RecordFormat::Sgf, record))));
    assert_ne!(f.wb.document().id, old);
    assert!(f.wb.inspect().human_sl.is_none());
}
#[test]
fn normal_exit_and_app_close_terminate_worker() {
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, id);
    f.wb.human_sl(HumanSlCommand::Finish).unwrap();
    assert!(f.world.dead(id));
    let id = start(&mut f, settings(PlayerColor::Black), false);
    f.wb.end_engines();
    assert!(f.world.dead(id));
    assert!(f.dir.path().is_dir());
}

#[test]
fn active_t20_game_refuses_humansl_start() {
    let mut f = fixture(true);
    let _fg = foreground(&mut f);
    f.wb.game(GameCommand::Start {
        kind: GameKind::Genmove,
        settings: settings::GameSettings::default(),
        autoplay: None,
        engine: None,
    })
    .unwrap();
    assert!(
        matches!(f.wb.human_sl(HumanSlCommand::Start {settings:settings(PlayerColor::Black),from_current:false,advanced_visible:false,stop_batch:false}).unwrap(),CommandResult::Refused {message_key} if message_key=="LizzieFrame.engineGameStopFirstHint")
    );
    assert!(f.wb.inspect().human_sl.is_none());
}

#[test]
fn readiness_capability_failure_and_retry_are_not_ordinary_analysis() {
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    let q = f.world.query(id);
    out(
        &mut f.wb,
        id,
        json!({"id":q["id"],"policy":[1.0],"moveInfos":[{"move":"D4"}]}),
    );
    assert!(f.world.dead(id));
    assert!(f.wb.inspect().human_sl.is_none());
    assert!(f
        .wb
        .inspect()
        .human_sl_notices
        .iter()
        .any(|n| matches!(n, HumanSlNotice::StartFailed { .. })));
    let id = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, id);
    assert_eq!(f.wb.inspect().human_sl.unwrap().phase, HumanSlPhase::Playing);
}

#[test]
fn quick_analysis_cannot_borrow_training_foreground() {
    // Two moves without analysis on the main trunk; 自动快析 borrows the foreground engine.
    let prepare = || {
        let mut f = fixture(true);
        let fg = foreground(&mut f);
        f.wb.set_auto_quick_analyze(false).unwrap();
        let mut s = f.wb.config().analysis_engine.clone();
        s.reuse_current_engine = true;
        let command = s.command.clone();
        f.wb.save_analysis_engine_settings(s, &command, false).unwrap().unwrap();
        for (x, y) in [(3, 3), (15, 15)] {
            f.wb.edit(EditCommand::Play { point: Point { x, y }, color: None }).unwrap();
        }
        assert!(f.wb.navigate(Navigation::First));
        serve(&mut f, fg, false);
        (f, fg)
    };
    let browse = |f: &mut Fixture, fg: InstanceId| {
        f.wb.set_auto_quick_analyze(true).unwrap();
        assert!(f.wb.navigate(Navigation::Forward(1)));
        // Browsing settles (700 ms) before the analysis starts.
        f.wb.engine_tick(Instant::now() + Duration::from_secs(1));
        serve(f, fg, false)
    };
    let borrowed = |sent: &[String]| sent.iter().any(|c| c.contains("maxmoves 1"));

    // Without training, browsing starts a finite search on the borrowed foreground engine.
    let (mut f, fg) = prepare();
    let sent = browse(&mut f, fg);
    assert!(borrowed(&sent), "{sent:?}");

    // During training whose foreground keeps analysing, the same browsing borrows nothing.
    let (mut f, fg) = prepare();
    let id = start(&mut f, HumanSlTrainingSettings { mode: TrainingMode::LiveAnalysis, ..settings(PlayerColor::Black) }, true);
    ready(&mut f, id);
    serve(&mut f, fg, false);
    assert!(!f.wb.inspect().engine.paused);
    let sent = browse(&mut f, fg);
    assert!(!borrowed(&sent), "{sent:?}");
    assert_eq!(f.wb.inspect().engine.lease, LeaseAvailabilityView::HumanSlGame);
    assert!(f.wb.inspect().human_sl.is_some());
    assert!(!f.world.dead(id));
    assert!(!f.wb.view(false).quick.running);
}

#[test]
fn preparation_failure_restores_the_previously_running_foreground() {
    let mut f = fixture(true);
    let fg = foreground(&mut f);
    let before = f.wb.document().id;
    let id = start(&mut f, settings(PlayerColor::Black), false);
    f.wb.engine_input(id, EngineInput::Exited(Some(1)));
    serve(&mut f, fg, false);
    assert!(f.wb.inspect().human_sl.is_none());
    assert_eq!(f.wb.document().id, before);
    assert!(!f.wb.inspect().engine.paused);
}

fn level(level: CoachLevel, pace: Pace) -> HumanSlTrainingSettings {
    HumanSlTrainingSettings {
        level,
        pace,
        ..settings(PlayerColor::White)
    }
}

/// The fixed Java's level table (`fixtures/coach/b2-coach.json`, written by running it).
fn b2_level(level: CoachLevel) -> Value {
    let b2: Value = serde_json::from_str(include_str!("fixtures/coach/b2-coach.json")).unwrap();
    let rows = b2["levels"].as_array().unwrap();
    rows.iter()
        .find(|row| serde_json::from_value::<CoachLevel>(row["name"].clone()).ok() == Some(level))
        .unwrap()
        .clone()
}

fn same(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

/// `maxTime` of a request sent right away: the budget less the return reserve, at 85%.
fn assert_budget(q: &Value, seconds: f64) {
    let max_time = q["overrideSettings"]["maxTime"].as_f64().unwrap();
    let full = (seconds - 0.45) * 0.85;
    assert!(max_time <= full && max_time > full - 0.1, "{max_time} for {seconds} s");
}

#[test]
fn each_level_asks_the_companion_what_java_asks() {
    for level in [
        CoachLevel::Kyu10,
        CoachLevel::Kyu1,
        CoachLevel::Dan1,
        CoachLevel::StrongDan3,
        CoachLevel::Dan9,
        CoachLevel::StrongDan10,
        CoachLevel::Dan11,
    ] {
        let java = b2_level(level);
        let play = &java["play"];
        let profile = play["profile"].as_str().unwrap();
        let elite = ["rank_7d", "rank_8d", "rank_9d"].contains(&profile);
        let mut f = fixture(true);
        let id = start(&mut f, self::level(level, Pace::Natural), false);
        let ready = f.world.query(id);
        assert_eq!(
            (ready["maxVisits"].as_u64(), &ready["includePolicy"]),
            (Some(1), &json!(true)),
            "{level:?}"
        );
        let o = &ready["overrideSettings"];
        assert_eq!(o["humanSLProfile"], profile);
        assert_eq!(o["rootNumSymmetriesToSample"], 1);
        assert_eq!(
            o["humanSLRootExploreProbWeightless"].as_f64(),
            Some(if elite { 0.8 } else { 0.5 }),
            "{level:?}"
        );
        out(&mut f.wb, id, json!({"id":ready["id"],"humanPolicy":{"D4":1.0}}));
        let q = f.world.query(id);
        assert!(
            q.get("allowMoves").is_none(),
            "{level:?}: no verification of candidates"
        );
        assert_eq!(q["includePolicy"], true);
        let seconds = java["moveTimeoutSeconds"].as_f64().unwrap();
        assert_budget(&q, seconds);
        let o = &q["overrideSettings"];
        if play["searches"] == true {
            // One search with every setting of the level.
            assert_eq!(q["maxVisits"], play["maxVisits"], "{level:?}");
            for pair in java["searchOverrides"].as_array().unwrap() {
                let key = pair[0].as_str().unwrap();
                assert!(same(&o[key], &pair[1]), "{level:?} {key}: {} vs {}", o[key], pair[1]);
            }
            out(
                &mut f.wb,
                id,
                json!({"id":q["id"],"humanPolicy":{"D4":0.5,"Q16":0.5},
                "moveInfos":[{"move":"D4","order":0,"visits":30,"utility":0.1},{"move":"Q16","order":1,"visits":20,"utility":0.0}]}),
            );
        } else {
            // The raw policy of the profile from one visit, sampled; no search per move.
            assert_eq!(q["maxVisits"], 1, "{level:?}");
            assert_eq!(o["humanSLProfile"], profile);
            assert_eq!(o["rootNumSymmetriesToSample"], 2);
            assert_eq!(o["humanSLRootExploreProbWeightless"].as_f64(), Some(0.5));
            assert_eq!(o["ignorePreRootHistory"], false);
            out(&mut f.wb, id, json!({"id":q["id"],"humanPolicy":{"D4":0.6,"Q16":0.4}}));
        }
        assert!(
            f.world.take(id).is_empty(),
            "{level:?}: nothing more is asked for the move"
        );
        f.wb.engine_tick(Instant::now() + Duration::from_secs(15));
        assert_eq!(f.wb.inspect().position.move_number, 1, "{level:?}");
    }
}

/// Ten seconds a move, twenty for 强10段 and 11段; AI 思考时间 and an old 用时 change nothing.
#[test]
fn the_search_budget_follows_the_level_alone() {
    for (level, seconds) in [
        (CoachLevel::Kyu5, 10),
        (CoachLevel::Dan3, 10),
        (CoachLevel::Dan10, 10),
        (CoachLevel::StrongDan10, 20),
        (CoachLevel::Dan11, 20),
    ] {
        for pace in [Pace::Natural, Pace::Fast, Pace::Slow] {
            let mut f = fixture(true);
            let id = start(&mut f, self::level(level, pace), false);
            ready(&mut f, id);
            assert_budget(&f.world.query(id), f64::from(seconds));
            f.wb.engine_tick(Instant::now() + Duration::from_secs_f64(f64::from(seconds) - 0.5));
            assert!(!f.world.dead(id), "{level:?} {pace:?}: within its budget");
            f.wb.engine_tick(Instant::now() + Duration::from_secs_f64(f64::from(seconds) + 0.5));
            assert!(
                f.world.dead(id),
                "{level:?} {pace:?}: past its budget the attempt is retried"
            );
            assert_ne!(f.world.worker(), id);
        }
    }
    // A setting saved with 用时 无限制 reads as 慢 and searches no longer.
    let old: HumanSlTrainingSettings =
        serde_json::from_value(json!({"moveTimeSeconds": 86400, "rank": 3, "danRank": true})).unwrap();
    assert_eq!(old.pace, Pace::Slow);
    let mut f = fixture(true);
    let id = start(
        &mut f,
        HumanSlTrainingSettings {
            player_color: PlayerColor::White,
            ..old
        },
        false,
    );
    ready(&mut f, id);
    assert_budget(&f.world.query(id), 10.0);
}

#[test]
fn policy_level_asks_katago_about_passing_only_when_it_may_pass() {
    let mut f = fixture(true);
    for _ in 0..200 {
        f.wb.edit(EditCommand::Pass).unwrap();
    }
    let id = start(&mut f, level(CoachLevel::Kyu5, Pace::Natural), true);
    ready(&mut f, id);
    let probe = f.world.query(id);
    assert_eq!(probe["maxVisits"], 1);
    out(&mut f.wb, id, json!({"id":probe["id"],"humanPolicy":{"D4":1.0}}));
    let check = f.world.query(id);
    assert_eq!(check["maxVisits"], 40);
    assert!(check.get("includePolicy").is_none() && check["overrideSettings"].get("humanSLProfile").is_none());
    out(
        &mut f.wb,
        id,
        json!({"id":check["id"],"moveInfos":[{"move":"pass","order":0,"visits":30},{"move":"D4","order":1,"visits":10}]}),
    );
    f.wb.engine_tick(Instant::now() + Duration::from_secs(15));
    let view = f.wb.inspect();
    assert_eq!(view.position.move_number, 201);
    assert!(view.human_sl_notices.contains(&HumanSlNotice::AiPassed));
}

/// The reply is played no earlier than its think time after the AI's turn began.
#[test]
fn an_early_reply_waits_for_its_think_time() {
    for (pace, longest) in [(Pace::Fast, 1.8), (Pace::Natural, 8.0), (Pace::Slow, 14.0)] {
        let mut f = fixture(true);
        let id = start(&mut f, level(CoachLevel::Dan3, pace), false);
        ready(&mut f, id);
        let q = f.world.query(id);
        out(
            &mut f.wb,
            id,
            json!({"id":q["id"],"humanPolicy":{"D4":1.0},"moveInfos":[{"move":"D4","order":0,"visits":60,"utility":0.0}]}),
        );
        f.wb.engine_tick(Instant::now());
        let view = f.wb.inspect();
        assert_eq!(
            view.position.move_number, 0,
            "{pace:?}: not before the shortest think time"
        );
        assert!(view.human_sl.unwrap().ai_thinking);
        f.wb.engine_tick(Instant::now() + Duration::from_secs_f64(longest + 0.01));
        assert_eq!(
            f.wb.inspect().position.move_number,
            1,
            "{pace:?}: by the longest think time"
        );
    }
}

#[test]
fn a_late_reply_is_played_at_once() {
    let mut f = fixture(true);
    let id = start(&mut f, level(CoachLevel::Dan3, Pace::Fast), false);
    ready(&mut f, id);
    let q = f.world.query(id);
    // 快 thinks at most 1.8 s; the engine answers later than that.
    std::thread::sleep(Duration::from_millis(1_850));
    out(
        &mut f.wb,
        id,
        json!({"id":q["id"],"humanPolicy":{"D4":1.0},"moveInfos":[{"move":"D4","order":0,"visits":60,"utility":0.0}]}),
    );
    assert_eq!(f.wb.inspect().position.move_number, 1);
}

#[test]
fn finishing_while_the_reply_waits_plays_nothing() {
    let mut f = fixture(true);
    let id = start(&mut f, level(CoachLevel::Kyu5, Pace::Slow), false);
    ready(&mut f, id);
    let q = f.world.query(id);
    out(&mut f.wb, id, json!({"id":q["id"],"humanPolicy":{"D4":1.0}}));
    assert!(f.wb.inspect().human_sl.unwrap().ai_thinking);
    f.wb.human_sl(HumanSlCommand::Finish).unwrap();
    f.wb.engine_tick(Instant::now() + Duration::from_secs(15));
    let view = f.wb.inspect();
    assert!(view.human_sl.is_none());
    assert_eq!(view.position.move_number, 0);
    assert!(f.wb.document().record.node(ROOT).children.is_empty());
}

/// AI 思考时间 only paces the reply: with the same random draws every pace plays the same moves.
#[test]
fn the_pace_does_not_change_the_chosen_moves() {
    let policy = json!({"D4":0.3,"Q16":0.25,"C3":0.2,"R4":0.15,"K10":0.1});
    for level in [CoachLevel::Kyu5, CoachLevel::Dan3] {
        let games: Vec<Vec<Option<Stone>>> = [Pace::Natural, Pace::Fast, Pace::Slow]
            .into_iter()
            .map(|pace| {
                let mut f = fixture(true);
                let id = start(&mut f, self::level(level, pace), false);
                ready(&mut f, id);
                for human in [Point { x: 0, y: 0 }, Point { x: 18, y: 18 }, Point { x: 0, y: 18 }] {
                    let q = f.world.query(id);
                    let infos: Vec<Value> = ["D4", "Q16", "C3", "R4", "K10"]
                        .iter()
                        .enumerate()
                        .map(|(i, v)| json!({"move":v,"order":i,"visits":10,"utility":-0.05 * i as f64}))
                        .collect();
                    out(
                        &mut f.wb,
                        id,
                        json!({"id":q["id"],"humanPolicy":policy,"moveInfos":infos}),
                    );
                    f.wb.engine_tick(Instant::now() + Duration::from_secs(15));
                    f.wb.edit(EditCommand::Play {
                        point: human,
                        color: None,
                    })
                    .unwrap();
                }
                f.wb.inspect().position.stones
            })
            .collect();
        assert_eq!(games[0].iter().flatten().count(), 6, "{level:?}");
        assert!(games.iter().all(|g| *g == games[0]), "{level:?}");
    }
}

#[test]
fn stale_session_generation_cannot_mutate_a_replacement_session() {
    let mut f = fixture(true);
    let old = start(&mut f, settings(PlayerColor::Black), false);
    let q = f.world.query(old);
    f.wb.human_sl(HumanSlCommand::CancelPreparation).unwrap();
    let new = start(&mut f, settings(PlayerColor::White), false);
    let before = f.wb.inspect();
    out(&mut f.wb, old, json!({"id":q["id"],"humanPolicy":{"D4":1.0}}));
    assert_eq!(f.wb.inspect().human_sl.unwrap().id, before.human_sl.unwrap().id);
    assert_eq!(f.wb.document().id, before.document.id);
    ready(&mut f, new);
    assert!(f.wb.inspect().human_sl.unwrap().ai_thinking);
}

#[test]
fn same_node_with_a_new_position_stamp_also_discards_ai_results() {
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::White), false);
    ready(&mut f, id);
    let q = f.world.query(id);
    f.wb.edit_document(lizzie_core::record::DocumentEdit::SetupSideToPlay { color: Stone::White })
        .unwrap();
    let revision = f.wb.document().revision;
    out(&mut f.wb, id, json!({"id":q["id"],"humanPolicy":{"D4":1.0}}));
    assert!(f.wb.inspect().human_sl.is_none());
    assert_eq!(f.wb.cursor(), ROOT);
    assert_eq!(f.wb.document().revision, revision);
    assert_eq!(f.wb.inspect().position.to_play, Stone::White);
}

#[test]
fn cancellation_restores_an_originally_paused_foreground_without_resuming_it() {
    let mut f = fixture(true);
    let fg = foreground(&mut f);
    f.wb.toggle_analysis().unwrap();
    serve(&mut f, fg, false);
    assert!(f.wb.inspect().engine.paused);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    f.wb.human_sl(HumanSlCommand::CancelPreparation).unwrap();
    serve(&mut f, fg, false);
    assert!(f.wb.inspect().human_sl.is_none());
    assert!(f.wb.inspect().engine.paused);
    assert!(f.world.dead(id));
}

#[test]
fn stderr_startup_stages_do_not_claim_readiness_until_the_probe_answers() {
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    for (line, stage) in [
        ("Analysis engine starting", HumanSlStartupStage::LoadingModels),
        ("Building network", HumanSlStartupStage::OptimizingGpu),
        ("Using existing timing cache", HumanSlStartupStage::CacheReady),
    ] {
        f.wb.engine_input(id, EngineInput::Stderr(line.into()));
        let view = f.wb.inspect().human_sl.unwrap();
        assert_eq!(view.phase, HumanSlPhase::Preparing);
        assert_eq!(view.stage, Some(stage));
    }
    ready(&mut f, id);
    assert_eq!(f.wb.inspect().human_sl.unwrap().phase, HumanSlPhase::Playing);
    assert_eq!(f.wb.inspect().human_sl.unwrap().stage, None);
}

#[test]
fn timeout_and_capability_diagnostics_attribute_failures_without_exposing_paths_or_tokens() {
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    f.wb.engine_input(
        id,
        EngineInput::Stderr("Failed /home/private/model.bin.gz token=secret-value".into()),
    );
    f.wb.engine_tick(Instant::now() + Duration::from_secs(181));
    let notice =
        f.wb.inspect()
            .human_sl_notices
            .into_iter()
            .find_map(|n| {
                if let HumanSlNotice::StartFailed { detail, .. } = n {
                    detail
                } else {
                    None
                }
            })
            .unwrap();
    assert!(notice.contains("timed out"));
    assert!(!notice.contains("/home/private"));
    assert!(!notice.contains("secret-value"));
}

#[test]
fn companion_exit_on_human_turn_restarts_for_the_next_ai_move() {
    let mut f = fixture(true);
    let old = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, old);
    let session = f.wb.inspect().human_sl.unwrap().id;
    f.wb.engine_input(old, EngineInput::Exited(Some(1)));
    assert_eq!(f.wb.inspect().human_sl.unwrap().id, session);
    f.wb.edit(EditCommand::Play {
        point: Point { x: 0, y: 0 },
        color: None,
    })
    .unwrap();
    let new = f.world.worker();
    assert_ne!(new, old);
    let query = f.world.query(new);
    assert_eq!(query["includePolicy"], true);
    assert_eq!(query["overrideSettings"]["humanSLProfile"], "rank_3k");
    out(&mut f.wb, old, json!({"id": query["id"], "error": "stale old process"}));
    f.wb.engine_input(old, EngineInput::Exited(Some(1)));
    out(
        &mut f.wb,
        new,
        json!({"id": query["id"], "humanPolicy": {"D4": 1.0},
            "moveInfos": [{"move": "D4", "order": 0, "utility": 0.0}]}),
    );
    answer(&mut f, new, "D4");
    assert_eq!(f.wb.inspect().position.move_number, 2);
    assert_eq!(f.wb.inspect().human_sl.unwrap().id, session);
    f.wb.human_sl(HumanSlCommand::Finish).unwrap();
    assert!(f.world.dead(new));
}

#[test]
fn failed_attempts_restart_the_companion_and_retry_ai_starts_fresh() {
    for exit_first in [false, true] {
        let mut f = fixture(true);
        let first = start(&mut f, settings(PlayerColor::White), false);
        ready(&mut f, first);
        let query = f.world.query(first);
        if exit_first {
            f.wb.engine_input(first, EngineInput::Exited(Some(1)));
        } else {
            out(&mut f.wb, first, json!({"id": query["id"], "error": "first attempt rejected"}));
        }
        assert!(f.world.dead(first), "between-attempt cancellation must kill the companion");
        let second = f.world.worker();
        assert_ne!(second, first);
        let query = f.world.query(second);
        assert_eq!(query["includePolicy"], true);
        assert_eq!(query["overrideSettings"]["humanSLProfile"], "rank_3k");
        f.wb.engine_input(second, EngineInput::Exited(Some(1)));
        let view = f.wb.inspect();
        assert!(view.human_sl.unwrap().ai_failed);
        assert!(view.human_sl_notices.contains(&HumanSlNotice::AiMoveFailed));
        assert!(!view.human_sl_notices.contains(&HumanSlNotice::Ended));
        f.wb.human_sl(HumanSlCommand::RetryAi).unwrap();
        let third = f.world.worker();
        assert_ne!(third, second);
        answer(&mut f, third, "D4");
        assert_eq!(f.wb.inspect().position.move_number, 1);
        f.wb.end_engines();
        assert!(f.world.dead(third));
    }
}

/// Browsing back to where a game from the current position started, a stone or a pass there only
/// brings the board back: neither the record's line nor a new variation at the start gets the
/// move, and nothing is played on the game's position.
#[test]
fn a_move_at_the_training_start_while_browsing_never_branches_there() {
    for pass in [false, true] {
        let mut f = fixture(true);
        let original_move = if pass {
            EditCommand::Pass
        } else {
            EditCommand::Play {
                point: Point { x: 3, y: 15 },
                color: None,
            }
        };
        f.wb.edit(original_move.clone()).unwrap();
        let original = f.wb.cursor();
        f.wb.navigate(Navigation::First);
        let worker = start(&mut f, settings(PlayerColor::Black), true);
        ready(&mut f, worker);
        f.wb.edit(original_move.clone()).unwrap();
        answer(&mut f, worker, "Q16");
        let live = f.wb.cursor();
        f.wb.navigate(Navigation::First);
        assert_eq!(f.wb.edit(original_move).unwrap(), CommandResult::Done);
        let record = &f.wb.document().record;
        assert_eq!(record.node(ROOT).children.len(), 2);
        assert_eq!(record.node(ROOT).children[0], original);
        assert!(record.node(original).children.is_empty());
        assert_eq!(f.wb.cursor(), live);
        assert!(record.node(live).children.is_empty());
        assert!(f.wb.inspect().human_sl_notices.contains(&HumanSlNotice::BackToLiveGame));
    }
}

#[test]
fn cancel_and_prepare_failure_preserve_a_foreground_analysis_limit_stop() {
    for fail in [false, true] {
        let mut f = fixture(true);
        let fg = foreground(&mut f);
        let mut analysis = f.wb.config().engine.analysis.clone();
        analysis.limit_playout = true;
        analysis.limit_playouts = 6;
        f.wb.set_analysis_settings(analysis).unwrap();
        f.wb.engine_input(
            fg,
            EngineInput::Stdout("info move D4 visits 7 winrate 0.5 prior 0.2 order 0 pv D4".into()),
        );
        assert!(f.wb.inspect().engine.stopped_by_limit);
        serve(&mut f, fg, false);
        assert!(!f.wb.inspect().engine.analysing);
        let worker = start(&mut f, settings(PlayerColor::Black), false);
        if fail {
            f.wb.engine_input(worker, EngineInput::Exited(Some(1)));
        } else {
            f.wb.human_sl(HumanSlCommand::CancelPreparation).unwrap();
        }
        serve(&mut f, fg, false);
        let engine = f.wb.inspect().engine;
        assert!(engine.stopped_by_limit);
        assert!(!engine.analysing);
        assert!(!engine.paused);
    }
}

#[test]
fn installed_tensorrt_without_cuda_companion_reports_the_specific_start_failure() {
    use lizzie_core::resources::{Catalog, ResourceReceipt, ResourceSource};
    let mut f = fixture(true);
    let catalog = Catalog::embedded().unwrap();
    let asset = &catalog.assets["windows-tensorrt"];
    let store = f.wb.resource_store();
    let directory = store.package_dir(asset, None).unwrap();
    let executable = directory.join(&asset.executable);
    std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
    std::fs::File::create(&executable)
        .unwrap()
        .set_len(asset.executable_size_bytes)
        .unwrap();
    let receipt = ResourceReceipt {
        asset_id: asset.id.clone(),
        version: asset.version.clone(),
        archive_sha256: asset.sha256.clone(),
        executable_sha256: asset.executable_sha256.clone(),
        executable: asset.executable.clone(),
        executable_size: asset.executable_size_bytes,
        source: ResourceSource::Import,
        installed_at: 0,
        experimental_backend: None,
    };
    std::fs::write(directory.join("resource.json"), serde_json::to_vec(&receipt).unwrap()).unwrap();
    let inventory = store.inventory().unwrap();
    assert!(inventory.packages.iter().any(|p| p.is_current(asset)));
    assert!(!inventory.packages.iter().any(|p| p.receipt.asset_id == "windows-nvidia"));
    let command = format!("\"{}\" analysis -config /fake/analysis.cfg", executable.display());
    let mut analysis = f.wb.config().analysis_engine.clone();
    analysis.command = command.clone();
    f.wb.save_analysis_engine_settings(analysis, &command, true)
        .unwrap()
        .unwrap();
    f.wb.human_sl(HumanSlCommand::Start {
        settings: settings(PlayerColor::Black),
        from_current: false,
        advanced_visible: false,
        stop_batch: false,
    })
    .unwrap();
    assert_eq!(
        f.wb.inspect().human_sl_notices,
        vec![HumanSlNotice::StartFailed {
            reason: "HumanSlGame.error.tensorRtCompanionMissing".into(),
            detail: None,
        }]
    );
    assert!(f.wb.inspect().human_sl.is_none());
    assert!(f.world.0.lock().unwrap().is_empty());
}

fn abort_then(f: &mut Fixture, continuation: &str) -> CommandResult {
    let op = serde_json::from_value::<HumanSlCommand>(json!({
        "type": "AbortThen", "continuation": continuation,
    })).expect("the shared HumanSL continuation command must be supported");
    f.wb.human_sl(op).unwrap()
}

fn published_continuation(f: &mut Fixture) -> Value {
    serde_json::to_value(f.wb.view(false)).unwrap()["human_sl_continuation"].clone()
}

#[test]
fn ui_exit_continuation_owns_the_same_slot_as_deferred_commands() {
    let mut f = fixture(true);
    let fg = foreground(&mut f);
    f.wb.toggle_analysis().unwrap();
    serve(&mut f, fg, false);
    let worker = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, worker);
    let document = f.wb.document().id;
    assert_eq!(abort_then(&mut f, "NewGame"), CommandResult::Done);
    let open = Command::OpenRecord { path: "/later.sgf".into(), source: OpenSource::Drop };
    assert_eq!(f.wb.game_gate(&open), Some(CommandResult::Unchanged));
    assert_eq!(abort_then(&mut f, "EngineGame"), CommandResult::Unchanged);
    assert_eq!(published_continuation(&mut f), Value::Null);
    serve(&mut f, fg, false);
    assert_eq!(f.wb.document().id, document);
    assert!(f.wb.take_human_sl_deferred().is_none());
    assert!(f.wb.inspect().engine.paused, "the continuation replaces forced post-game analysis");
    assert_eq!(published_continuation(&mut f), "NewGame");
    assert_eq!(published_continuation(&mut f), Value::Null);
}

#[test]
fn deferred_command_first_swallows_ui_exit_continuations() {
    let mut f = fixture(true);
    let fg = foreground(&mut f);
    f.wb.toggle_analysis().unwrap();
    serve(&mut f, fg, false);
    let worker = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, worker);
    let open = Command::OpenRecord { path: "/first.sgf".into(), source: OpenSource::Drop };
    assert_eq!(f.wb.game_gate(&open), Some(CommandResult::Done));
    assert_eq!(abort_then(&mut f, "NewAnalysisGame"), CommandResult::Unchanged);
    serve(&mut f, fg, false);
    assert_eq!(published_continuation(&mut f), Value::Null);
    assert!(f.wb.inspect().engine.paused);
    assert_eq!(f.wb.take_human_sl_deferred(), Some(open));
    assert!(f.wb.take_human_sl_deferred().is_none());
    let ticket = f.wb.begin_open(lizzie_core::workbench::OpenOrigin::File {
        path: "/first.sgf".into(), source: OpenSource::Drop,
    });
    let record = lizzie_core::formats::sgf::read("(;SZ[19];B[aa])").unwrap();
    assert!(f.wb.complete_open(ticket, Ok((lizzie_core::record::RecordFormat::Sgf, record))));
    assert_eq!(published_continuation(&mut f), Value::Null);
}

#[test]
fn ui_exit_continuation_survives_cleanup_failure_and_runs_once_after_retry() {
    let mut f = fixture(true);
    let fg = foreground(&mut f);
    let worker = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, worker);
    assert_eq!(abort_then(&mut f, "EngineGame"), CommandResult::Done);
    serve(&mut f, fg, true);
    assert_eq!(f.wb.inspect().human_sl.unwrap().phase, HumanSlPhase::CleanupPending);
    assert_eq!(published_continuation(&mut f), Value::Null);
    assert_eq!(abort_then(&mut f, "NewGame"), CommandResult::Unchanged);
    assert_eq!(f.wb.inspect().human_sl.unwrap().phase, HumanSlPhase::CleanupPending);
    f.wb.human_sl(HumanSlCommand::Finish).unwrap();
    serve(&mut f, fg, false);
    assert_eq!(published_continuation(&mut f), "EngineGame");
    assert_eq!(published_continuation(&mut f), Value::Null);
}

#[test]
fn abort_then_runs_immediately_without_a_session_and_waits_during_cleanup_pending() {
    let mut f = fixture(true);
    assert_eq!(abort_then(&mut f, "NewAnalysisGame"), CommandResult::Done);
    assert_eq!(published_continuation(&mut f), "NewAnalysisGame");
    assert_eq!(published_continuation(&mut f), Value::Null);
    let fg = foreground(&mut f);
    let worker = start(&mut f, settings(PlayerColor::Black), false);
    f.wb.human_sl(HumanSlCommand::CancelPreparation).unwrap();
    serve(&mut f, fg, true);
    assert!(f.world.dead(worker));
    assert_eq!(abort_then(&mut f, "NewGame"), CommandResult::Done);
    assert_eq!(f.wb.inspect().human_sl.unwrap().phase, HumanSlPhase::CleanupPending);
    assert_eq!(published_continuation(&mut f), Value::Null);
    f.wb.human_sl(HumanSlCommand::Finish).unwrap();
    serve(&mut f, fg, false);
    assert_eq!(published_continuation(&mut f), "NewGame");
    assert_eq!(published_continuation(&mut f), Value::Null);
}

#[test]
fn abort_then_during_preparation_restores_without_setting_up_the_game() {
    let mut f = fixture(true);
    let fg = foreground(&mut f);
    f.wb.toggle_analysis().unwrap();
    serve(&mut f, fg, false);
    let document = f.wb.document().id;
    let revision = f.wb.document().revision;
    let worker = start(&mut f, settings(PlayerColor::Black), false);
    assert_eq!(abort_then(&mut f, "NewGame"), CommandResult::Done);
    assert!(f.world.dead(worker));
    assert_eq!(published_continuation(&mut f), Value::Null);
    serve(&mut f, fg, false);
    assert!(f.wb.inspect().human_sl.is_none());
    assert!(f.wb.inspect().engine.paused);
    assert_eq!(f.wb.document().id, document);
    assert_eq!(f.wb.document().revision, revision);
    assert_eq!(published_continuation(&mut f), "NewGame");
    assert_eq!(published_continuation(&mut f), Value::Null);
}

fn click(f: &mut Fixture, x: u16, y: u16) -> CommandResult {
    f.wb.edit(EditCommand::Play {
        point: Point { x, y },
        color: None,
    })
    .unwrap()
}
fn kind(f: &Fixture, node: NodeId) -> NodeKind {
    f.wb.document().record.node(node).kind
}

/// The board is browsed back two moves while the AI thinks: the game goes on, and the reply is
/// played on the game's position with the board back on it (B2 `applyAiMoveIfCurrent`).
#[test]
fn browsing_back_while_the_ai_thinks_keeps_the_game_and_the_reply_brings_the_board_back() {
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, id);
    click(&mut f, 3, 3);
    answer(&mut f, id, "Q16");
    click(&mut f, 15, 15);
    let live = f.wb.cursor();
    assert!(f.wb.inspect().human_sl.unwrap().ai_thinking);
    assert!(f.wb.navigate(Navigation::Back(1)));
    assert!(f.wb.navigate(Navigation::Back(1)));
    assert_eq!(f.wb.inspect().position.move_number, 1);
    answer(&mut f, id, "D4");
    let view = f.wb.inspect();
    let session = view.human_sl.expect("the game goes on");
    assert_eq!(session.phase, HumanSlPhase::Playing);
    assert!(session.human_turn && !session.ai_thinking);
    assert_eq!(view.position.move_number, 4);
    assert_eq!(f.wb.document().record.parent(f.wb.cursor()), Some(live));
    assert!(matches!(
        kind(&f, f.wb.cursor()),
        NodeKind::Move {
            color: Stone::White,
            point: Point { x: 3, y: 15 }
        }
    ));
}

/// On the player's turn a click on an earlier position only brings the board back; the next
/// click plays (B2 `onBoardClicked` → `atLiveGame`).
#[test]
fn a_click_on_an_earlier_position_brings_the_board_back_and_the_next_click_plays() {
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, id);
    click(&mut f, 3, 3);
    answer(&mut f, id, "Q16");
    let live = f.wb.cursor();
    let revision = f.wb.document().revision;
    assert!(f.wb.navigate(Navigation::First));
    assert_eq!(click(&mut f, 15, 15), CommandResult::Done);
    assert_eq!(f.wb.cursor(), live);
    assert_eq!(f.wb.document().revision, revision);
    assert!(f.wb.document().record.node(live).children.is_empty());
    let view = f.wb.inspect();
    assert!(view.human_sl.unwrap().human_turn);
    assert!(view.human_sl_notices.contains(&HumanSlNotice::BackToLiveGame));
    assert_eq!(click(&mut f, 15, 15), CommandResult::Done);
    assert_eq!(f.wb.document().record.parent(f.wb.cursor()), Some(live));
    assert_eq!(f.wb.inspect().position.move_number, 3);
    assert!(f.wb.inspect().human_sl.unwrap().ai_thinking);
}

/// 停一手 on an earlier position only brings the board back to the game's position, saying so;
/// the next 停一手 passes there (B2 `humanPass` → `atLiveGame`).
#[test]
fn pass_on_an_earlier_position_brings_the_board_back_and_the_next_pass_passes() {
    for keyboard in [false, true] {
        let mut f = fixture(true);
        let id = start(&mut f, settings(PlayerColor::Black), false);
        ready(&mut f, id);
        click(&mut f, 3, 3);
        answer(&mut f, id, "Q16");
        let live = f.wb.cursor();
        let revision = f.wb.document().revision;
        assert!(f.wb.navigate(Navigation::Back(2)));
        let pass = |f: &mut Fixture| {
            if keyboard {
                f.wb.edit(EditCommand::Pass).unwrap()
            } else {
                f.wb.human_sl(HumanSlCommand::Pass).unwrap()
            }
        };
        assert_eq!(pass(&mut f), CommandResult::Done);
        assert_eq!(f.wb.cursor(), live);
        assert_eq!(f.wb.document().revision, revision);
        assert!(f.wb.document().record.node(live).children.is_empty());
        let view = f.wb.inspect();
        assert!(view.human_sl_notices.contains(&HumanSlNotice::BackToLiveGame));
        assert!(view.human_sl.unwrap().human_turn);
        assert_eq!(pass(&mut f), CommandResult::Done);
        assert_eq!(f.wb.document().record.parent(f.wb.cursor()), Some(live));
        assert_eq!(kind(&f, f.wb.cursor()), NodeKind::Pass { color: Stone::Black });
        assert!(f.wb.inspect().human_sl.unwrap().ai_thinking);
    }
}

/// A reply that arrives after the game was ended is dropped and never reaches the record.
#[test]
fn a_reply_arriving_after_the_game_ended_is_dropped() {
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, id);
    click(&mut f, 3, 3);
    let live = f.wb.cursor();
    assert!(f.wb.navigate(Navigation::First));
    f.wb.human_sl(HumanSlCommand::Finish).unwrap();
    let revision = f.wb.document().revision;
    answer(&mut f, id, "Q16");
    assert!(f.wb.inspect().human_sl.is_none());
    assert_eq!(f.wb.cursor(), ROOT);
    assert_eq!(f.wb.document().revision, revision);
    assert!(f.wb.document().record.node(live).children.is_empty());
}

/// Deleting the moves at the game's position ends the session with a message, when the AI's
/// reply arrives or the player next acts (B2 `returnToGamePosition`, `gameMovesDeleted`).
#[test]
fn deleting_the_games_moves_ends_the_session_with_a_message() {
    for ai_thinking in [true, false] {
        let mut f = fixture(true);
        let id = start(&mut f, settings(PlayerColor::Black), false);
        ready(&mut f, id);
        click(&mut f, 3, 3);
        if !ai_thinking {
            answer(&mut f, id, "Q16");
        }
        let live = f.wb.cursor();
        assert!(f.wb.navigate(Navigation::First));
        assert!(f.wb.edit_document(DocumentEdit::Remove { node: live }).unwrap());
        let revision = f.wb.document().revision;
        if ai_thinking {
            answer(&mut f, id, "Q16");
        } else {
            assert_eq!(click(&mut f, 15, 15), CommandResult::Done);
        }
        let view = f.wb.inspect();
        assert!(view.human_sl.is_none(), "ai thinking: {ai_thinking}");
        assert!(view.human_sl_notices.contains(&HumanSlNotice::GameMovesDeleted));
        assert!(f.world.dead(id));
        assert_eq!(f.wb.cursor(), ROOT);
        assert_eq!(f.wb.document().revision, revision);
        assert_eq!(f.wb.document().record.nodes().count(), if ai_thinking { 1 } else { 2 });
    }
}

/// `n` stones, Black from the top rows down and White from the bottom rows up, none captured.
fn fill(f: &mut Fixture, n: u16) {
    for i in 0..n {
        let k = i / 2;
        let (x, y) = (k % 19, k / 19);
        click(f, x, if i % 2 == 0 { y } else { 18 - y });
    }
}
/// The AI's turn after `stones` stones of an earlier game, continued from there; with none, the
/// player's pass that opens a new game.
fn ai_turn(level: CoachLevel, stones: u16) -> (Fixture, InstanceId) {
    let mut f = fixture(true);
    fill(&mut f, stones);
    // The AI has the side to play after the stones; the player passes first in a new game.
    let human = if stones > 0 && stones % 2 == 0 {
        PlayerColor::White
    } else {
        PlayerColor::Black
    };
    let s = HumanSlTrainingSettings {
        player_color: human,
        ..self::level(level, Pace::Fast)
    };
    let id = start(&mut f, s, stones > 0);
    ready(&mut f, id);
    if stones == 0 {
        assert_eq!(f.wb.human_sl(HumanSlCommand::Pass).unwrap(), CommandResult::Done);
    }
    (f, id)
}
/// KataGo's search: `top` first, then `second`, with their score leads.
fn infos(top: (&str, f64), second: (&str, f64)) -> Value {
    json!([
        {"move":top.0,"order":0,"visits":50,"utility":0.1,"scoreLead":top.1},
        {"move":second.0,"order":1,"visits":10,"utility":0.0,"scoreLead":second.1}
    ])
}
/// Answers the companion's queries with `policy` and the search's `infos` until it asks no more,
/// then lets the reply's think time pass; returns the visits each query asked for.
fn respond(f: &mut Fixture, id: InstanceId, policy: Value, infos: Value) -> Vec<u64> {
    let mut visits = Vec::new();
    for _ in 0..6 {
        let pending = f.world.take(id);
        if pending.is_empty() {
            break;
        }
        for text in pending {
            let q: Value = serde_json::from_str(&text).unwrap();
            if q.get("action").is_some() {
                continue;
            }
            visits.push(q["maxVisits"].as_u64().unwrap());
            out(
                &mut f.wb,
                id,
                json!({"id":q["id"],"humanPolicy":policy,"moveInfos":infos}),
            );
        }
    }
    f.wb.engine_tick(Instant::now() + Duration::from_secs(15));
    visits
}
fn ai_move(f: &Fixture) -> NodeKind {
    let view = f.wb.inspect();
    assert!(view.human_sl.unwrap().human_turn, "the AI has moved");
    kind(f, f.wb.cursor())
}
/// A policy level, which asks KataGo separately whether to pass, and a search level.
const LEVELS: [CoachLevel; 2] = [CoachLevel::Kyu5, CoachLevel::Dan3];

/// From 40% of the board on (move 145 of 361 points), well before move 200, the AI passes when
/// KataGo's search says so (B2 `HumanSlAnalysisRunner.mayPass`, `PASS_BOARD_SHARE`).
#[test]
fn the_ai_passes_when_katago_says_so_once_forty_percent_of_the_board_is_played() {
    for level in LEVELS {
        let (mut f, id) = ai_turn(level, 145);
        let visits = respond(
            &mut f,
            id,
            json!({"K10":0.7,"pass":0.3}),
            infos(("pass", 0.5), ("K10", 0.4)),
        );
        assert_eq!(ai_move(&f), NodeKind::Pass { color: Stone::White }, "{level:?}");
        assert_eq!(f.wb.inspect().position.move_number, 146);
        assert!(f.wb.inspect().human_sl_notices.contains(&HumanSlNotice::AiPassed));
        if level == CoachLevel::Kyu5 {
            assert_eq!(visits, [1, 40], "the policy level checks with KataGo's own search");
        }
    }
}

/// Below 40% (move 144 < 0.4 × 361) and without the player's pass the AI plays on although
/// KataGo would pass; a policy level does not ask.
#[test]
fn before_forty_percent_the_ai_does_not_pass_unless_the_player_just_passed() {
    for level in LEVELS {
        let (mut f, id) = ai_turn(level, 144);
        let visits = respond(
            &mut f,
            id,
            json!({"K10":0.7,"pass":0.3}),
            infos(("pass", 0.5), ("K10", 0.4)),
        );
        assert_eq!(
            ai_move(&f),
            NodeKind::Move {
                color: Stone::Black,
                point: Point { x: 9, y: 9 }
            },
            "{level:?}"
        );
        if level == CoachLevel::Kyu5 {
            assert_eq!(visits, [1]);
        }
    }
}

/// Right after the player's pass the AI may pass on the first move: when KataGo's top move is a
/// pass, and when passing too gives up at most a point (B2 `SearchView.passEnds`,
/// `PASS_TOLERANCE_POINTS`). Both players having passed, KataGo then counts the game.
#[test]
fn after_the_players_pass_the_ai_may_pass_early() {
    for level in LEVELS {
        for top in [("pass", 0.5), ("K10", 0.8), ("K10", 1.0)] {
            let second = if top.0 == "pass" { ("K10", 0.4) } else { ("pass", 0.0) };
            let (mut f, id) = ai_turn(level, 0);
            respond(&mut f, id, json!({"K10":0.7,"pass":0.3}), infos(top, second));
            assert_eq!(kind(&f, f.wb.cursor()), NodeKind::Pass { color: Stone::White }, "{level:?} {top:?}");
            assert_eq!(f.wb.inspect().position.move_number, 2);
            assert!(f.wb.inspect().human_sl.unwrap().counting);
        }
    }
    // Passing would cost 1.5 points: the AI plays on.
    let (mut f, id) = ai_turn(CoachLevel::Kyu5, 0);
    respond(
        &mut f,
        id,
        json!({"K10":0.7,"pass":0.3}),
        infos(("K10", 1.5), ("pass", 0.0)),
    );
    assert!(matches!(
        ai_move(&f),
        NodeKind::Move {
            color: Stone::White,
            ..
        }
    ));
}

/// When most players of a policy level would pass but KataGo still sees a point that matters, the
/// AI plays KataGo's move rather than what is left of the human policy (B2 `HUMAN_PASS_SHARE`).
#[test]
fn a_policy_level_that_would_pass_plays_katagos_point_instead() {
    let (mut f, id) = ai_turn(CoachLevel::Kyu5, 0);
    respond(
        &mut f,
        id,
        json!({"K10":0.4,"pass":0.6}),
        infos(("Q16", 3.0), ("pass", 0.0)),
    );
    assert_eq!(
        ai_move(&f),
        NodeKind::Move {
            color: Stone::White,
            point: Point { x: 15, y: 3 }
        }
    );
}

// ---- the game's end: komi, both passes and KataGo's count, resignations (B2 `GameResult`,
// `AiResignation`, `HumanSlGameController.countAndEnd` / `resign`, `CoachSetupForm.config`).

/// KataGo's chinese rules with the button: a drawn count goes to the first player to pass.
const BUTTON: &str = r#"{"ko":"SIMPLE","scoring":"AREA","tax":"NONE","suicide":false,"hasButton":true,"whiteHandicapBonus":"N","friendlyPassOk":true}"#;
fn black(s: HumanSlTrainingSettings) -> HumanSlTrainingSettings {
    HumanSlTrainingSettings {
        player_color: PlayerColor::Black,
        ..s
    }
}

/// An even game starts with the even-game komi of the rules the foreground KataGo plays: the 2027
/// Chinese rules 7, the Japanese 6.5, the 2027 rules with the button 7; rules of no rule set keep
/// the current komi. The settings view shows that komi before the start.
#[test]
fn an_even_game_takes_the_komi_of_the_rules_katago_plays() {
    let japanese = r#"{"ko":"SIMPLE","scoring":"TERRITORY","tax":"SEKI","suicide":false,"hasButton":false,"whiteHandicapBonus":"0","friendlyPassOk":false}"#;
    let aga = r#"{"ko":"SITUATIONAL","scoring":"AREA","tax":"NONE","suicide":false,"hasButton":false,"whiteHandicapBonus":"N-1","friendlyPassOk":true}"#;
    for (rules, komi, even) in [(CHINESE, 7.0, 7.0), (japanese, 7.5, 6.5), (BUTTON, 7.5, 7.0), (aga, 6.0, 6.0)] {
        let mut f = fixture(true);
        let fg = foreground_with(&mut f, rules);
        f.wb.set_komi(komi).unwrap();
        serve_with(&mut f, fg, false, rules);
        assert_eq!(f.wb.inspect().settings.human_sl.even_komi, even, "{rules}");
        let id = start(&mut f, settings(PlayerColor::Black), false);
        ready(&mut f, id);
        let view = f.wb.inspect();
        assert_eq!((view.document.info.komi, view.komi), (even, even), "{rules}");
    }
}

/// A new game, the player Black under `komi`: the player passes, then the AI after KataGo's
/// search; returns KataGo's count query.
fn both_pass(mut f: Fixture, lv: CoachLevel, komi: f64) -> (Fixture, InstanceId, Value) {
    f.wb.set_komi(komi).unwrap();
    let id = start(&mut f, black(level(lv, Pace::Fast)), false);
    ready(&mut f, id);
    assert_eq!(f.wb.human_sl(HumanSlCommand::Pass).unwrap(), CommandResult::Done);
    respond(&mut f, id, json!({"K10":0.7,"pass":0.3}), infos(("pass", 0.5), ("K10", 0.4)));
    let id = f.world.worker();
    let q = f.world.query(id);
    (f, id, q)
}
/// KataGo's count: `lead`, the score lead of the side to move.
fn count(f: &mut Fixture, id: InstanceId, q: &Value, lead: f64) {
    out(
        &mut f.wb,
        id,
        json!({"id":q["id"],"rootInfo":{"scoreLead":lead,"winrate":0.5,"visits":200},
            "moveInfos":[{"move":"pass","order":0,"visits":200,"scoreLead":lead}]}),
    );
}
fn result(f: &Fixture) -> Option<HumanSlResult> {
    f.wb.inspect().human_sl.unwrap().result
}
fn won(winner: Stone, margin: &str) -> Option<HumanSlResult> {
    Some(HumanSlResult::Counted {
        verdict: ScoreVerdict::Win {
            winner,
            margin: margin.into(),
        },
    })
}
/// The record's result once saved and read back.
fn saved_result(f: &mut Fixture) -> String {
    let path = f.dir.path().join("coach.sgf");
    let job = f.wb.begin_save(path.to_str().unwrap(), "LizzieYzy Tauri:test");
    let written = lizzie_core::formats::write_record_file(&job.path, &job.text).map_err(|e| e.to_string());
    assert!(f.wb.complete_save(job.ticket, written));
    lizzie_core::formats::read_file(&path, false).unwrap().1.info.result
}

/// Both players passing ends the game: KataGo counts it with 200 visits of its own search, the
/// result rounds its lead to the half-point komi and goes into the record, which keeps it when
/// saved and read back. Nothing is played while KataGo counts or after; the session stays on.
#[test]
fn both_passes_end_the_game_and_the_count_goes_into_the_record() {
    for lv in LEVELS {
        let (mut f, id, q) = both_pass(fixture(true), lv, 7.5);
        assert_eq!(q["maxVisits"], 200, "{lv:?}");
        assert!(q["overrideSettings"].get("humanSLProfile").is_none());
        assert_eq!(q["komi"], 7.5);
        let view = f.wb.inspect();
        let s = view.human_sl.unwrap();
        assert!(s.counting && !s.human_turn && !s.ai_thinking && s.clock.is_none() && s.result.is_none());
        assert!(view.human_sl_notices.contains(&HumanSlNotice::BothPassed));
        assert!(!view.human_sl_notices.contains(&HumanSlNotice::AiPassed));
        assert_eq!(f.wb.edit(EditCommand::Pass).unwrap(), CommandResult::Unchanged);
        assert_eq!(f.wb.human_sl(HumanSlCommand::Resign).unwrap(), CommandResult::Unchanged);
        // Black, to move, is 6.8 points behind: White wins by 6.5.
        count(&mut f, id, &q, -6.8);
        assert_eq!(result(&f), won(Stone::White, "6.5"), "{lv:?}");
        let s = f.wb.inspect().human_sl.unwrap();
        assert!(s.phase == HumanSlPhase::Playing && !s.counting && !s.human_turn && s.clock.is_none());
        assert_eq!(f.wb.document().record.info.result, "W+6.5");
        assert_eq!(click(&mut f, 3, 3), CommandResult::Unchanged);
        assert_eq!(f.wb.inspect().position.move_number, 2);
        assert_eq!(saved_result(&mut f), "W+6.5");
        f.wb.human_sl(HumanSlCommand::Finish).unwrap();
        assert!(f.wb.inspect().human_sl.is_none());
        assert_eq!(f.wb.document().record.info.result, "W+6.5");
    }
    // The player White: the AI opens, both pass, White is to move and 3.2 ahead.
    let mut f = fixture(true);
    f.wb.set_komi(7.5).unwrap();
    let id = start(&mut f, settings(PlayerColor::White), false);
    ready(&mut f, id);
    answer(&mut f, id, "D4");
    assert_eq!(f.wb.human_sl(HumanSlCommand::Pass).unwrap(), CommandResult::Done);
    respond(&mut f, id, json!({"K10":0.7,"pass":0.3}), infos(("pass", 0.5), ("K10", 0.4)));
    let id = f.world.worker();
    let q = f.world.query(id);
    count(&mut f, id, &q, 3.2);
    assert_eq!(result(&f), won(Stone::White, "3.5"));
    assert_eq!(f.wb.document().record.info.result, "W+3.5");
}

/// Under the rules analysis asks for with the button, KataGo's lead holds the first passer's half
/// point, which comes off before rounding to the whole-point komi: an even count goes to the
/// first player to pass by no margin (`B+`, `W+`), and other counts keep a whole margin. Without
/// the button an even count is a draw (`0`).
#[test]
fn under_button_rules_an_even_count_goes_to_the_first_passer() {
    let button = || {
        fixture_with(true, |c| {
            c.analysis_engine.use_current_rules = false;
            c.analysis_engine.specific_rules = BUTTON.into();
        })
    };
    for (lead, expected, record) in [
        (
            0.5,
            Some(HumanSlResult::Counted {
                verdict: ScoreVerdict::DrawToFirstPasser { winner: Stone::Black },
            }),
            "B+",
        ),
        (2.6, won(Stone::Black, "2"), "B+2"),
    ] {
        let (mut f, id, q) = both_pass(button(), CoachLevel::Kyu5, 7.0);
        assert_eq!(q["rules"]["hasButton"], true);
        count(&mut f, id, &q, lead);
        assert_eq!(result(&f), expected, "{lead}");
        assert_eq!(f.wb.document().record.info.result, record);
        assert_eq!(saved_result(&mut f), record);
    }
    // White passed first, before the game started from the current position.
    let mut f = button();
    f.wb.set_komi(7.0).unwrap();
    click(&mut f, 3, 3);
    f.wb.edit(EditCommand::Pass).unwrap();
    let id = start(&mut f, black(level(CoachLevel::Kyu5, Pace::Fast)), true);
    ready(&mut f, id);
    assert!(f.wb.inspect().human_sl.unwrap().human_is_black);
    assert_eq!(f.wb.human_sl(HumanSlCommand::Pass).unwrap(), CommandResult::Done);
    respond(&mut f, id, json!({"K10":0.7,"pass":0.3}), infos(("pass", 0.5), ("K10", 0.4)));
    let id = f.world.worker();
    let q = f.world.query(id);
    count(&mut f, id, &q, -0.5);
    assert_eq!(
        result(&f),
        Some(HumanSlResult::Counted {
            verdict: ScoreVerdict::DrawToFirstPasser { winner: Stone::White }
        })
    );
    assert_eq!(f.wb.document().record.info.result, "W+");
    // No button: an even count under komi 7 is a draw.
    let (mut f, id, q) = both_pass(fixture(true), CoachLevel::Kyu5, 7.0);
    count(&mut f, id, &q, 0.3);
    assert_eq!(
        result(&f),
        Some(HumanSlResult::Counted {
            verdict: ScoreVerdict::Draw
        })
    );
    assert_eq!(saved_result(&mut f), "0");
}

/// The count is judged under the rules it was asked under: when the foreground KataGo whose
/// button rules the count was sent with exits before the companion answers, an even count still
/// goes to the first player to pass, in the record and once saved, rather than under the rules
/// analysis falls back to without a foreground.
#[test]
fn a_count_keeps_the_rules_it_was_asked_under_when_the_foreground_exits() {
    let mut f = fixture(true);
    let fg = foreground_with(&mut f, BUTTON);
    f.wb.set_komi(7.0).unwrap();
    serve_with(&mut f, fg, false, BUTTON);
    let (mut f, id, q) = both_pass(f, CoachLevel::Kyu5, 7.0);
    assert_eq!(q["rules"]["hasButton"], true);
    f.wb.engine_input(fg, EngineInput::Exited(Some(1)));
    f.wb.engine_tick(Instant::now() + Duration::from_secs(1));
    assert!(f.wb.inspect().human_sl.unwrap().counting);
    count(&mut f, id, &q, 0.5);
    assert_eq!(
        result(&f),
        Some(HumanSlResult::Counted {
            verdict: ScoreVerdict::DrawToFirstPasser { winner: Stone::Black }
        })
    );
    assert_eq!(f.wb.document().record.info.result, "B+");
    assert_eq!(saved_result(&mut f), "B+");
}

/// A count that times out after 20 seconds, that KataGo answers with an error or without a score
/// lead, or whose companion exits, leaves the game not counted and the record without a result;
/// it is not asked again.
#[test]
fn a_failed_count_leaves_the_game_uncounted_and_the_record_without_a_result() {
    for failure in ["timeout", "error", "no lead", "exit"] {
        let (mut f, id, q) = both_pass(fixture(true), CoachLevel::Kyu5, 7.5);
        match failure {
            "timeout" => {
                // The count was asked at the tick that played the AI's pass, 15 s on.
                f.wb.engine_tick(Instant::now() + Duration::from_secs(15 + 19));
                assert!(f.wb.inspect().human_sl.unwrap().counting);
                f.wb.engine_tick(Instant::now() + Duration::from_secs(15 + 21));
            }
            "error" => out(&mut f.wb, id, json!({"id":q["id"],"error":"search failed"})),
            "no lead" => out(&mut f.wb, id, json!({"id":q["id"],"moveInfos":[]})),
            _ => {
                f.wb.engine_input(id, EngineInput::Exited(Some(1)));
            }
        }
        assert_eq!(result(&f), Some(HumanSlResult::Uncounted), "{failure}");
        assert!(!f.wb.inspect().human_sl.unwrap().counting);
        assert_eq!(f.wb.document().record.info.result, "", "{failure}");
        let asked_again = f.world.0.lock().unwrap().iter().any(|p| {
            p.sent.iter().filter_map(|l| serde_json::from_str::<Value>(l).ok()).any(|q| {
                q.get("action").is_none() && q["id"].as_str().is_some_and(|id| id.starts_with("humansl-count"))
            })
        });
        assert!(!asked_again, "{failure}: the count is not asked again");
        assert_eq!(saved_result(&mut f), "");
    }
}

/// 认输: the player gives up, also while the AI thinks; the AI wins by resignation, written to the
/// record, and its reply arriving afterwards is dropped. From a browsed position the button only
/// brings the board back. After the game nothing is played.
#[test]
fn the_player_resigns_and_the_ai_wins_by_resignation() {
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, id);
    click(&mut f, 3, 3);
    let live = f.wb.cursor();
    assert!(f.wb.inspect().human_sl.unwrap().ai_thinking);
    assert!(f.wb.navigate(Navigation::First));
    assert_eq!(f.wb.human_sl(HumanSlCommand::Resign).unwrap(), CommandResult::Done);
    assert_eq!(f.wb.cursor(), live);
    assert!(f.wb.inspect().human_sl_notices.contains(&HumanSlNotice::BackToLiveGame));
    assert_eq!(result(&f), None);
    assert_eq!(f.wb.human_sl(HumanSlCommand::Resign).unwrap(), CommandResult::Done);
    assert_eq!(result(&f), Some(HumanSlResult::Resigned { winner: Stone::White }));
    let s = f.wb.inspect().human_sl.unwrap();
    assert!(s.phase == HumanSlPhase::Playing && !s.ai_thinking && !s.human_turn && s.clock.is_none());
    assert_eq!(f.wb.document().record.info.result, "W+R");
    answer(&mut f, id, "Q16");
    assert_eq!(f.wb.inspect().position.move_number, 1);
    assert_eq!(click(&mut f, 15, 15), CommandResult::Unchanged);
    assert_eq!(f.wb.human_sl(HumanSlCommand::Pass).unwrap(), CommandResult::Unchanged);
    assert_eq!(f.wb.human_sl(HumanSlCommand::Resign).unwrap(), CommandResult::Unchanged);
    assert_eq!(saved_result(&mut f), "W+R");
}

/// Up to `turns` of the AI's turns: KataGo sees the AI at `winrate` and `lead` and its top move is
/// a stone; the player plays a stone, or passes, in between. Returns the turn the AI resigned on,
/// if it did.
fn hopeless_turns(f: &mut Fixture, turns: usize, winrate: f64, lead: f64, player_passes: bool) -> Option<usize> {
    let size = f.wb.document().record.size;
    for turn in 0..turns {
        let (x, row) = ((turn % 19) as u16, (turn / 19) as u16);
        let vertex = lizzie_core::engine::gtp::vertex(size, Point { x, y: 6 + row });
        let id = f.world.worker();
        for _ in 0..6 {
            let pending = f.world.take(id);
            if pending.is_empty() {
                break;
            }
            for text in pending {
                let q: Value = serde_json::from_str(&text).unwrap();
                if q.get("action").is_none() {
                    out(
                        &mut f.wb,
                        id,
                        json!({"id":q["id"],"humanPolicy":{(vertex.clone()):1.0},
                            "moveInfos":[{"move":vertex,"order":0,"visits":50,"utility":-0.9,"scoreLead":lead}],
                            "rootInfo":{"winrate":winrate,"scoreLead":lead}}),
                    );
                }
            }
        }
        f.wb.engine_tick(Instant::now() + Duration::from_secs(15));
        if let Some(result) = result(f) {
            let human = if f.wb.inspect().human_sl.unwrap().human_is_black { Stone::Black } else { Stone::White };
            assert_eq!(result, HumanSlResult::Resigned { winner: human });
            return Some(turn + 1);
        }
        let played = if player_passes {
            f.wb.human_sl(HumanSlCommand::Pass).unwrap()
        } else {
            click(f, x, 10 + row)
        };
        assert_eq!(played, CommandResult::Done, "turn {turn}");
    }
    None
}

/// The AI resigns by its level once 40% of the board was played: a policy level after 20 turns in
/// a row at most 0.5% and 40 points behind, a search level after 10 turns at most 1% and 20 points
/// behind. A turn short, a point short or before 40% of the board it plays on.
#[test]
fn the_ai_resigns_by_its_level() {
    for (lv, turns, winrate, lead) in [(CoachLevel::Kyu5, 20, 0.005, -40.0), (CoachLevel::Dan3, 10, 0.01, -20.0)] {
        // From move 145 of 361 points, the thresholds met every turn: it plays on for one turn
        // short of the count and resigns on the last.
        let (mut f, _) = ai_turn(lv, 145);
        assert_eq!(hopeless_turns(&mut f, turns, winrate, lead, false), Some(turns), "{lv:?}");
        assert!(f.wb.inspect().human_sl_notices.contains(&HumanSlNotice::AiResigned));
        assert_eq!(f.wb.inspect().position.move_number, 145 + 2 * (turns as u32 - 1));
        assert_eq!(f.wb.document().record.info.result, "B+R");
        assert_eq!(saved_result(&mut f), "B+R");
        // A point short.
        let (mut f, _) = ai_turn(lv, 145);
        assert_eq!(hopeless_turns(&mut f, turns + 1, winrate, lead + 1.0, false), None, "{lv:?}");
        // Before 40% of the board (moves 100 to 138 < 144.4), the player passing so that a policy
        // level asks KataGo too.
        let (mut f, _) = ai_turn(lv, 100);
        assert_eq!(hopeless_turns(&mut f, turns, winrate, lead, true), None, "{lv:?}");
    }
}

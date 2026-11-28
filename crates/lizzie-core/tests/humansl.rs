//! HumanSL's public command/engine-input seam with controlled analysis and foreground processes.
use lizzie_core::api::*;
use lizzie_core::browse::Navigation;
use lizzie_core::engine::{
    gtp::LaunchSpec,
    profile::{EngineProfile, EngineStartup},
    runtime::{EngineInput, EngineIo, InstanceId, KataParam, Launcher},
};
use lizzie_core::humansl::{CoachLevel, Pace};
use lizzie_core::record::{DocumentEdit, NodeId, NodeKind, Point, Stone, StoneChange, ROOT};
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
/// The AI's play and the session's lifecycle are the same in every mode; these scenarios run in
/// 实战后复盘, which asks the companion nothing beyond the AI's moves. 边下边提醒's reviews have
/// their own scenarios.
fn settings(color: PlayerColor) -> HumanSlTrainingSettings {
    HumanSlTrainingSettings {
        mode: TrainingMode::PostGameReview,
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
/// The engine's answer `value`, read at `at`.
fn out_at(wb: &mut Workbench, id: InstanceId, value: Value, at: Instant) {
    wb.engine_input_at(id, EngineInput::Stdout(value.to_string()), at);
}
fn ready(f: &mut Fixture, id: InstanceId) {
    ready_at(f, id, Instant::now());
}
/// The readiness check answers at `at`, when the game begins.
fn ready_at(f: &mut Fixture, id: InstanceId, at: Instant) {
    let q = f.world.query(id);
    out_at(
        &mut f.wb,
        id,
        json!({"id":q["id"],"humanPolicy":{"D4":1.0},"moveInfos":[{"move":"D4","order":0,"utility":0.0,"visits":1}]}),
        at,
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
/// with a komi of 0.5 (Java `CoachSetupForm.config`, `HumanSlTrainingConfig.resolveHumanIsBlack`);
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

/// The fixed Java's level table (`fixtures/coach/java-coach.json`, written by running it).
fn java_level(level: CoachLevel) -> Value {
    let table: Value = serde_json::from_str(include_str!("fixtures/coach/java-coach.json")).unwrap();
    let rows = table["levels"].as_array().unwrap();
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
        let java = java_level(level);
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
            mode: TrainingMode::PostGameReview,
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
        // The AI's turn begins with the game at `t`, and the companion answers at once.
        let t = Instant::now();
        ready_at(&mut f, id, t);
        let q = f.world.query(id);
        out_at(
            &mut f.wb,
            id,
            json!({"id":q["id"],"humanPolicy":{"D4":1.0},"moveInfos":[{"move":"D4","order":0,"visits":60,"utility":0.0}]}),
            t,
        );
        f.wb.engine_tick(t);
        let view = f.wb.inspect();
        assert_eq!(
            view.position.move_number, 0,
            "{pace:?}: not before the shortest think time"
        );
        assert!(view.human_sl.unwrap().ai_thinking);
        f.wb.engine_tick(t + Duration::from_secs_f64(longest + 0.01));
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
    // An installed engine with its analysis config and weight: only the CUDA companion is missing.
    let config = store.analysis_config();
    let weight = store.weights_dir().join("main.bin.gz");
    touch(&config);
    touch(&weight);
    let command = format!(
        "\"{}\" analysis -config \"{}\" -model \"{}\"",
        executable.display(),
        config.display(),
        weight.display()
    );
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
            diagnosis: None,
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
/// played on the game's position with the board back on it (Java `applyAiMoveIfCurrent`).
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
/// click plays (Java `onBoardClicked` → `atLiveGame`).
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
/// the next 停一手 passes there (Java `humanPass` → `atLiveGame`).
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
/// reply arrives or the player next acts (Java `returnToGamePosition`, `gameMovesDeleted`).
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
/// KataGo's search says so (Java `HumanSlAnalysisRunner.mayPass`, `PASS_BOARD_SHARE`).
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
/// pass, and when passing too gives up at most a point (Java `SearchView.passEnds`,
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
/// AI plays KataGo's move rather than what is left of the human policy (Java `HUMAN_PASS_SHARE`).
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

// ---- the game's end: komi, both passes and KataGo's count, resignations (Java `GameResult`,
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

/// After a handicap game under rules of no rule set (KataGo's Chinese rules with komi 0.5), an
/// even game takes the komi new games start with: that of the rule set chosen in the rules panel
/// with 启动时自动应用 on, else 7.5. After an even game the current komi stays (Java
/// `RulePreset.evenGameKomi`, `newGameKomi(config, 7.5f)`).
#[test]
fn after_a_handicap_game_an_even_game_takes_the_chosen_rule_sets_komi() {
    for (chosen, record, even) in [
        (true, "(;SZ[19]HA[2]KM[0.5]AB[dd][pp])", 7.0),
        (false, "(;SZ[19]HA[2]KM[0.5]AB[dd][pp])", 7.5),
        (true, "(;SZ[19]KM[6.5])", 6.5),
    ] {
        let mut f = fixture_with(true, |c| {
            if chosen {
                c.engine.rule_set = Some("CHINESE_2027".into());
                c.engine.auto_load_kata_rules = true;
                c.engine.kata_rules = "chinese".into();
            }
        });
        let fg = foreground(&mut f);
        let opening = f.wb.begin_open(lizzie_core::workbench::OpenOrigin::File {
            path: "/x.sgf".into(),
            source: OpenSource::Drop,
        });
        let read = lizzie_core::formats::sgf::read(record).unwrap();
        assert!(f.wb.complete_open(opening, Ok((lizzie_core::record::RecordFormat::Sgf, read))));
        serve(&mut f, fg, false);
        assert_eq!(f.wb.inspect().settings.human_sl.even_komi, even, "{chosen} {record}");
        let id = start(&mut f, settings(PlayerColor::Black), false);
        ready(&mut f, id);
        let view = f.wb.inspect();
        assert_eq!((view.document.info.komi, view.komi), (even, even), "{chosen} {record}");
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

// ---------------------------------------------------------------------------- 悔棋

fn take_back(f: &mut Fixture) -> CommandResult {
    f.wb.human_sl(HumanSlCommand::TakeBack).unwrap()
}
fn session(f: &Fixture) -> HumanSlView {
    f.wb.inspect().human_sl.expect("the session goes on")
}
fn children(f: &Fixture, node: NodeId) -> Vec<NodeId> {
    f.wb.document().record.node(node).children.clone()
}
/// The companion was asked for the AI's move: it answers `vertex`, and the reply is played.
fn reply(f: &mut Fixture, id: InstanceId, vertex: &str) {
    let q = f.world.query(id);
    out(
        &mut f.wb,
        id,
        json!({"id":q["id"],"humanPolicy":{(vertex):1.0},"moveInfos":[{"move":vertex,"order":0,"utility":0.0,"visits":q["maxVisits"]}]}),
    );
    f.wb.engine_tick(Instant::now() + Duration::from_secs(15));
}
fn played(f: &Fixture, node: NodeId) -> Option<Point> {
    match kind(f, node) {
        NodeKind::Move { point, .. } => Some(point),
        _ => None,
    }
}

/// After the AI replied, 悔棋 goes back before the player's move: the game's position is two
/// moves back, it is the player's turn, and both moves stay in the record (Java `takeBack`,
/// `GameTreePosition.takeBackMoves`).
#[test]
fn a_take_back_after_the_ai_replied_goes_back_two_moves_and_keeps_them() {
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, id);
    let start_node = f.wb.cursor();
    click(&mut f, 3, 3);
    let human = f.wb.cursor();
    reply(&mut f, id, "C17");
    let ai = f.wb.cursor();
    let s = session(&f);
    assert!(s.human_turn && s.can_take_back);
    assert_eq!(s.take_backs, 0);
    let revision = f.wb.document().revision;
    assert_eq!(take_back(&mut f), CommandResult::Done);
    assert_eq!(f.wb.cursor(), start_node);
    assert_eq!(f.wb.inspect().position.move_number, 0);
    let s = session(&f);
    assert!(s.human_turn && !s.ai_thinking && s.phase == HumanSlPhase::Playing);
    assert_eq!(s.take_backs, 1);
    assert!(!s.can_take_back, "nothing of the game is left to take back");
    assert_eq!(take_back(&mut f), CommandResult::Unchanged);
    assert_eq!(children(&f, start_node), vec![human]);
    assert_eq!(children(&f, human), vec![ai]);
    assert_eq!(f.wb.document().revision, revision, "the record is not changed");
    // The game goes on from there.
    click(&mut f, 15, 15);
    assert!(session(&f).ai_thinking);
    assert_eq!(f.wb.inspect().position.move_number, 1);
}

/// While the AI thinks, 悔棋 takes back only the player's move; its search is ended, and its
/// reply, arriving before or after the player's next move, is dropped.
#[test]
fn a_take_back_while_the_ai_thinks_takes_one_move_and_drops_the_reply_on_its_way() {
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, id);
    click(&mut f, 3, 3);
    reply(&mut f, id, "C17");
    let ai = f.wb.cursor();
    click(&mut f, 15, 15);
    let human = f.wb.cursor();
    let old = f.world.query(id);
    assert!(session(&f).can_take_back);
    assert_eq!(take_back(&mut f), CommandResult::Done);
    assert_eq!(f.wb.cursor(), ai);
    let s = session(&f);
    assert!(s.human_turn && !s.ai_thinking);
    assert_eq!(s.take_backs, 1);
    let sent = f.world.take(id);
    let cancelled: Vec<Value> = sent
        .iter()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .filter(|q| q["action"] == "terminate")
        .collect();
    assert_eq!(cancelled.len(), 1, "{sent:?}");
    assert_eq!(
        cancelled[0]["terminateId"], old["id"],
        "the search ended is the one asked for the reply"
    );
    let late = json!({"id":old["id"],"humanPolicy":{"R3":1.0},"moveInfos":[{"move":"R3","order":0,"utility":0.0,"visits":old["maxVisits"]}]});
    out(&mut f.wb, id, late.clone());
    f.wb.engine_tick(Instant::now() + Duration::from_secs(15));
    assert_eq!(f.wb.cursor(), ai);
    assert!(children(&f, human).is_empty());
    // The player moves elsewhere; the old reply arrives during the new search and is dropped too.
    click(&mut f, 9, 9);
    let again = f.wb.cursor();
    out(&mut f.wb, id, late);
    f.wb.engine_tick(Instant::now());
    assert_eq!(f.wb.cursor(), again);
    assert!(session(&f).ai_thinking);
    reply(&mut f, id, "D4");
    assert_eq!(f.wb.document().record.parent(f.wb.cursor()), Some(again));
    assert_eq!(played(&f, f.wb.cursor()), Some(Point { x: 3, y: 15 }));
    assert!(children(&f, human).is_empty());
}

/// 悔棋 while the AI's chosen reply waits for its think time (AI 思考时间): one move goes back,
/// and when the reply's time comes it is not played.
#[test]
fn a_take_back_while_the_reply_waits_for_its_think_time_drops_it() {
    let mut f = fixture(true);
    let id = start(&mut f, black(level(CoachLevel::Dan3, Pace::Slow)), false);
    ready(&mut f, id);
    click(&mut f, 3, 3);
    let human = f.wb.cursor();
    let q = f.world.query(id);
    out(
        &mut f.wb,
        id,
        json!({"id":q["id"],"humanPolicy":{"D4":1.0},"moveInfos":[{"move":"D4","order":0,"visits":60,"utility":0.0}]}),
    );
    f.wb.engine_tick(Instant::now());
    assert_eq!(f.wb.inspect().position.move_number, 1, "the reply waits");
    assert!(session(&f).ai_thinking);
    assert_eq!(take_back(&mut f), CommandResult::Done);
    assert_eq!(f.wb.inspect().position.move_number, 0);
    f.wb.engine_tick(Instant::now() + Duration::from_secs(15));
    let s = session(&f);
    assert!(s.human_turn && !s.ai_thinking);
    assert_eq!(f.wb.inspect().position.move_number, 0);
    assert!(children(&f, human).is_empty());
}

/// Nothing to take back: before the player's first move, on the AI's first move as White, and
/// in a handicap game with only the handicap stones; never a move from before the game. From a
/// browsed position 悔棋 only brings the board back; after the game it does nothing.
#[test]
fn take_back_needs_a_move_of_the_players_in_the_game() {
    // Even game, Black: nothing before the first move.
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, id);
    assert!(!session(&f).can_take_back);
    assert_eq!(take_back(&mut f), CommandResult::Unchanged);
    // White: the AI's first move cannot be taken back, only with the player's after it.
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::White), false);
    ready(&mut f, id);
    assert!(!session(&f).can_take_back);
    reply(&mut f, id, "D4");
    let first = f.wb.cursor();
    assert!(!session(&f).can_take_back);
    assert_eq!(take_back(&mut f), CommandResult::Unchanged);
    assert_eq!(f.wb.cursor(), first);
    click(&mut f, 15, 15);
    assert!(session(&f).can_take_back);
    // From a browsed position the board only comes back, saying so.
    let live = f.wb.cursor();
    assert!(f.wb.navigate(Navigation::First));
    assert_eq!(take_back(&mut f), CommandResult::Done);
    assert_eq!(f.wb.cursor(), live);
    assert!(f.wb.inspect().human_sl_notices.contains(&HumanSlNotice::BackToLiveGame));
    assert_eq!(session(&f).take_backs, 0);
    assert_eq!(take_back(&mut f), CommandResult::Done);
    assert_eq!(f.wb.cursor(), first);
    // A handicap game: the AI (White) moves first after the stones.
    let mut f = fixture(true);
    let id = start(&mut f, HumanSlTrainingSettings { handicap: 2, ..settings(PlayerColor::Black) }, false);
    ready(&mut f, id);
    assert!(!session(&f).can_take_back);
    reply(&mut f, id, "C17");
    assert!(!session(&f).can_take_back);
    assert_eq!(take_back(&mut f), CommandResult::Unchanged);
    assert_eq!(f.wb.inspect().position.stones.iter().flatten().count(), 3);
    // After the game.
    click(&mut f, 9, 9);
    reply(&mut f, id, "R3");
    assert!(session(&f).can_take_back);
    assert_eq!(f.wb.human_sl(HumanSlCommand::Resign).unwrap(), CommandResult::Done);
    assert!(!session(&f).can_take_back);
    assert_eq!(take_back(&mut f), CommandResult::Unchanged);
}

/// Taking back and playing again keeps every attempt (Java `CoachTakenBackLine`, upstream test
/// `replayingTakenBackMoveKeepsOneBranchAndRetainsEarlierAiReply`): replaying a withdrawn move
/// follows its node and the AI is asked again; the same reply follows the AI's old node, a
/// different one takes the game line's slot with the older reply beside it. A record's own equal
/// continuation at the start is never taken for the game's.
#[test]
fn replaying_a_taken_back_move_keeps_one_branch_and_retains_earlier_ai_replies() {
    let mut f = fixture(true);
    f.wb.edit(EditCommand::Pass).unwrap();
    let imported = f.wb.cursor();
    f.wb.navigate(Navigation::First);
    let id = start(&mut f, settings(PlayerColor::Black), true);
    ready(&mut f, id);
    let pass = |f: &mut Fixture| assert_eq!(f.wb.human_sl(HumanSlCommand::Pass).unwrap(), CommandResult::Done);

    pass(&mut f);
    reply(&mut f, id, "C17");
    let first_human = f.wb.document().record.parent(f.wb.cursor()).unwrap();
    let first_ai = f.wb.cursor();
    assert_eq!(children(&f, ROOT), vec![imported, first_human]);

    // The same pass again follows the withdrawn one; the AI, asked again, answers differently.
    assert_eq!(take_back(&mut f), CommandResult::Done);
    pass(&mut f);
    assert_eq!(f.wb.cursor(), first_human);
    reply(&mut f, id, "R3");
    let second_ai = f.wb.cursor();
    assert_eq!(children(&f, ROOT), vec![imported, first_human]);
    assert_eq!(children(&f, first_human), vec![second_ai, first_ai]);
    assert_eq!(played(&f, second_ai), Some(Point { x: 16, y: 16 }));

    // A different move takes the game line's slot at the start.
    assert_eq!(take_back(&mut f), CommandResult::Done);
    click(&mut f, 2, 0);
    let different = f.wb.cursor();
    reply(&mut f, id, "D4");
    assert_eq!(children(&f, ROOT), vec![imported, different, first_human]);

    // Back to the older attempt with its replies: a new reply becomes its line.
    assert_eq!(take_back(&mut f), CommandResult::Done);
    pass(&mut f);
    assert_eq!(f.wb.cursor(), first_human);
    reply(&mut f, id, "K3");
    let third_ai = f.wb.cursor();
    assert_eq!(children(&f, ROOT), vec![imported, first_human, different]);
    assert!(children(&f, imported).is_empty());
    assert_eq!(children(&f, first_human), vec![third_ai, second_ai, first_ai]);

    // The oldest reply again follows its node and goes back to the front.
    assert_eq!(take_back(&mut f), CommandResult::Done);
    pass(&mut f);
    reply(&mut f, id, "C17");
    assert_eq!(f.wb.cursor(), first_ai);
    assert_eq!(children(&f, first_human), vec![first_ai, third_ai, second_ai]);
    assert_eq!(children(&f, ROOT), vec![imported, first_human, different]);
    let s = session(&f);
    assert_eq!(s.take_backs, 4);
    assert!(s.human_turn && s.phase == HumanSlPhase::Playing);
}

/// Three take-backs in a row, then on again: no move is lost, an older attempt's deeper moves
/// stay under it when a different reply takes its place, and the record keeps them all when
/// saved and read back (upstream `consecutiveTakeBacksPreserveDeeperContinuationsWhenAnOlderAttemptIsReplayed`).
#[test]
fn consecutive_take_backs_lose_no_move() {
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, id);
    let mut line = Vec::new();
    for ((x, y), vertex) in [((3, 3), "C17"), ((15, 15), "R3"), ((9, 9), "K3")] {
        click(&mut f, x, y);
        line.push(f.wb.cursor());
        reply(&mut f, id, vertex);
        line.push(f.wb.cursor());
    }
    let nodes = f.wb.document().record.nodes().count();
    for _ in 0..3 {
        assert_eq!(take_back(&mut f), CommandResult::Done);
    }
    assert_eq!(f.wb.inspect().position.move_number, 0);
    assert_eq!(session(&f).take_backs, 3);
    assert_eq!(f.wb.document().record.nodes().count(), nodes);
    assert!(line.iter().all(|&n| f.wb.document().record.contains(n)));

    // The first round again, the same moves: the old nodes, nothing new.
    click(&mut f, 3, 3);
    assert_eq!(f.wb.cursor(), line[0]);
    reply(&mut f, id, "C17");
    assert_eq!(f.wb.cursor(), line[1]);
    // The second: the same move, a different reply.
    click(&mut f, 15, 15);
    assert_eq!(f.wb.cursor(), line[2]);
    reply(&mut f, id, "D4");
    let reply2 = f.wb.cursor();
    assert_eq!(children(&f, line[2]), vec![reply2, line[3]]);
    assert_eq!(children(&f, line[3]), vec![line[4]]);
    assert_eq!(children(&f, line[4]), vec![line[5]]);
    assert_eq!(f.wb.document().record.nodes().count(), nodes + 1);

    let path = f.dir.path().join("take-backs.sgf");
    let job = f.wb.begin_save(path.to_str().unwrap(), "LizzieYzy Tauri:test");
    let written = lizzie_core::formats::write_record_file(&job.path, &job.text).map_err(|e| e.to_string());
    assert!(f.wb.complete_save(job.ticket, written));
    let saved = lizzie_core::formats::read_file(&path, false).unwrap().1;
    assert_eq!(saved.nodes().count(), nodes + 1);
    let first = |n: NodeId| saved.node(n).children[0];
    let h2 = first(first(first(ROOT)));
    assert_eq!(saved.node(h2).children.len(), 2);
    let older = saved.node(h2).children[1];
    let deeper = first(first(older));
    assert!(matches!(saved.node(deeper).kind, NodeKind::Move { color: Stone::White, point: Point { x: 9, y: 16 } }));
}

/// What a game withdrew is that game's alone: within it a pass replays the withdrawn one; after
/// it ended, a new game from the same position starts a new variation there.
#[test]
fn a_new_game_does_not_replay_what_the_last_game_took_back() {
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::Black), true);
    ready(&mut f, id);
    assert_eq!(f.wb.human_sl(HumanSlCommand::Pass).unwrap(), CommandResult::Done);
    reply(&mut f, id, "C17");
    assert_eq!(take_back(&mut f), CommandResult::Done);
    assert_eq!(f.wb.human_sl(HumanSlCommand::Pass).unwrap(), CommandResult::Done);
    assert_eq!(children(&f, ROOT).len(), 1, "the same game replays its pass");
    reply(&mut f, id, "C17");
    assert_eq!(take_back(&mut f), CommandResult::Done);
    assert_eq!(f.wb.human_sl(HumanSlCommand::Resign).unwrap(), CommandResult::Done);
    f.wb.human_sl(HumanSlCommand::Finish).unwrap();
    assert!(f.wb.inspect().human_sl.is_none());

    f.wb.navigate(Navigation::First);
    let id = start(&mut f, settings(PlayerColor::Black), true);
    ready(&mut f, id);
    assert_eq!(session(&f).take_backs, 0);
    assert_eq!(f.wb.human_sl(HumanSlCommand::Pass).unwrap(), CommandResult::Done);
    assert_eq!(children(&f, ROOT).len(), 2, "a new game starts its own variation");
}

/// Deleting moves during the game is refused with Java's hint to use 悔棋 (`showDeleteBlockedHint`)
/// by the session's gate and by the edit itself, also after the game ended until 结束并复盘; other
/// edits keep the session's refusal. Once the session is over, deleting works again.
#[test]
fn deleting_moves_during_the_game_is_refused_with_the_take_back_hint() {
    fn deletes_blocked(f: &mut Fixture) {
        let blocked = CommandResult::Refused { message_key: "HumanSlGame.deleteBlocked".into() };
        let revision = f.wb.document().revision;
        let cursor = f.wb.cursor();
        for op in [
            EditCommand::DeleteMove { confirmed: None },
            EditCommand::DeleteMoveNoHint,
            EditCommand::DeleteBranch { confirmed: None },
            EditCommand::EditStone { point: Point { x: 3, y: 3 }, change: StoneChange::Delete },
        ] {
            assert_eq!(f.wb.game_gate(&Command::Edit { op: op.clone() }), Some(blocked.clone()), "{op:?}");
            assert_eq!(f.wb.edit(op.clone()).unwrap(), blocked, "{op:?}");
        }
        let other = CommandResult::Refused { message_key: "AnalysisSettings.reuseStatus.human_sl_game".into() };
        assert_eq!(f.wb.game_gate(&Command::Edit { op: EditCommand::SetAsMain }), Some(other.clone()));
        assert_eq!(f.wb.edit(EditCommand::SetAsMain).unwrap(), other);
        assert_eq!(f.wb.document().revision, revision, "the record is not changed");
        assert_eq!(f.wb.cursor(), cursor);
    }
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, id);
    click(&mut f, 3, 3);
    reply(&mut f, id, "C17");
    let ai = f.wb.cursor();
    deletes_blocked(&mut f);
    assert_eq!(session(&f).take_backs, 0, "a refused delete is no take-back");
    assert_eq!(f.wb.human_sl(HumanSlCommand::Resign).unwrap(), CommandResult::Done);
    deletes_blocked(&mut f);
    f.wb.human_sl(HumanSlCommand::Finish).unwrap();
    assert!(f.wb.inspect().human_sl.is_none());
    assert_eq!(f.wb.game_gate(&Command::Edit { op: EditCommand::DeleteMoveNoHint }), None);
    let deleted = f.wb.edit(EditCommand::DeleteMoveNoHint).unwrap();
    assert!(!matches!(deleted, CommandResult::Refused { .. }), "{deleted:?}");
    assert!(!f.wb.document().record.contains(ai));
}

/// In the modes that hide analysis, 悔棋 stays within AI 陪练: the paused foreground engine is
/// sent nothing (Java `applyReadBoardSyncLocally`). With 实时分析 it follows the board.
#[test]
fn a_take_back_in_a_hidden_mode_sends_nothing_to_the_foreground_engine() {
    for mode in [TrainingMode::PostGameReview, TrainingMode::LiveAnalysis] {
        let mut f = fixture(true);
        let fg = foreground(&mut f);
        let id = start(&mut f, HumanSlTrainingSettings { mode, ..settings(PlayerColor::Black) }, false);
        ready(&mut f, id);
        serve(&mut f, fg, false);
        click(&mut f, 3, 3);
        serve(&mut f, fg, false);
        reply(&mut f, id, "C17");
        serve(&mut f, fg, false);
        f.world.take(fg);
        assert_eq!(take_back(&mut f), CommandResult::Done);
        f.wb.engine_tick(Instant::now());
        let sent = serve(&mut f, fg, false);
        if mode == TrainingMode::LiveAnalysis {
            assert!(!sent.is_empty(), "the analysing engine follows the board");
        } else {
            assert!(sent.is_empty(), "{mode:?}: {sent:?}");
        }
        assert_eq!(f.wb.inspect().position.move_number, 0);
    }
}

/// H in AI 陪练 (Java `Input` VK_H with `activeCoachGame`): from the game's start until the
/// session is over, a mode that hides analysis keeps the raw network off, H asking for a hint
/// instead, also after the game ended; with 实时分析 the raw network toggles, during and after the
/// game, as it does before the game starts and once the session is over.
#[test]
fn the_raw_network_toggles_unless_the_game_hides_analysis() {
    let hidden = Some(CommandResult::Refused { message_key: "AnalysisSettings.reuseStatus.human_sl_game".into() });
    for mode in [TrainingMode::LiveAnalysis, TrainingMode::LiveFeedback, TrainingMode::PostGameReview] {
        let mut f = fixture(true);
        let fg = foreground(&mut f);
        let id = start(&mut f, HumanSlTrainingSettings { mode, ..settings(PlayerColor::Black) }, false);
        assert_eq!(f.wb.game_gate(&Command::ToggleRawNet), None, "{mode:?}: before the game");
        ready(&mut f, id);
        serve(&mut f, fg, false);
        let in_game = if mode == TrainingMode::LiveAnalysis { None } else { hidden.clone() };
        assert_eq!(f.wb.game_gate(&Command::ToggleRawNet), in_game, "{mode:?}: in the game");
        if in_game.is_none() {
            assert!(f.wb.toggle_raw_net(), "{mode:?}");
            assert!(f.wb.toggle_raw_net(), "{mode:?}");
        }
        assert_eq!(f.wb.human_sl(HumanSlCommand::Resign).unwrap(), CommandResult::Done);
        assert!(session(&f).result.is_some());
        assert_eq!(f.wb.game_gate(&Command::ToggleRawNet), in_game, "{mode:?}: after the game");
        f.wb.human_sl(HumanSlCommand::Finish).unwrap();
        serve(&mut f, fg, false);
        assert!(f.wb.inspect().human_sl.is_none());
        assert_eq!(f.wb.game_gate(&Command::ToggleRawNet), None, "{mode:?}: after the session");
    }
}

/// While a session stops or waits to retry its cleanup, H follows whether it ever had a game
/// (Java `activeCoachGame`: the controller is set when the game plays): one cancelled during its
/// preparation toggles the raw network; one whose game started keeps it off in the modes that hide
/// analysis, H asking for a hint; 实时分析 toggles it. The session projects that fact for the UI.
#[test]
fn while_a_session_stops_h_follows_whether_its_game_started() {
    let hidden = Some(CommandResult::Refused { message_key: "AnalysisSettings.reuseStatus.human_sl_game".into() });
    for mode in [TrainingMode::LiveFeedback, TrainingMode::PostGameReview, TrainingMode::LiveAnalysis] {
        for started in [false, true] {
            let mut f = fixture(true);
            let fg = foreground(&mut f);
            let id = start(&mut f, HumanSlTrainingSettings { mode, ..settings(PlayerColor::Black) }, false);
            let what = format!("{mode:?}, game started: {started}");
            assert!(!session(&f).game_started, "{what}: preparing");
            if started {
                ready(&mut f, id);
                serve(&mut f, fg, false);
                assert!(session(&f).game_started, "{what}: playing");
                f.wb.human_sl(HumanSlCommand::Finish).unwrap();
            } else {
                f.wb.human_sl(HumanSlCommand::CancelPreparation).unwrap();
            }
            let expected = if started && mode != TrainingMode::LiveAnalysis { hidden.clone() } else { None };
            let s = session(&f);
            assert_eq!((s.phase, s.game_started), (HumanSlPhase::Stopping, started), "{what}");
            assert_eq!(f.wb.game_gate(&Command::ToggleRawNet), expected, "{what}: stopping");
            // The foreground engine refuses to be given back: the cleanup waits for a retry.
            serve(&mut f, fg, true);
            let s = session(&f);
            assert_eq!((s.phase, s.game_started), (HumanSlPhase::CleanupPending, started), "{what}");
            assert_eq!(f.wb.game_gate(&Command::ToggleRawNet), expected, "{what}: cleanup pending");
            if expected.is_none() {
                assert!(f.wb.toggle_raw_net(), "{what}");
            }
        }
    }
}

/// A click on the players' names toggles the win rates beside them during the game too (Java
/// `Input` hands the coach game's clicks to `LizzieFrame.onClicked`, which tries
/// `togglePlayerNameWinratesAt` first; the floating board's `IndependentMainBoard.onClicked` the
/// same). Other display settings keep the session's refusal.
#[test]
fn the_names_toggle_their_win_rates_during_the_game() {
    let refused = Some(CommandResult::Refused { message_key: "AnalysisSettings.reuseStatus.human_sl_game".into() });
    for mode in [TrainingMode::LiveFeedback, TrainingMode::PostGameReview, TrainingMode::LiveAnalysis] {
        let mut f = fixture(true);
        let id = start(&mut f, HumanSlTrainingSettings { mode, ..settings(PlayerColor::Black) }, false);
        ready(&mut f, id);
        let before = f.wb.inspect().settings.display;
        for on in [!before.name_winrates, before.name_winrates] {
            let toggled = settings::DisplaySettings { name_winrates: on, ..f.wb.inspect().settings.display };
            assert_eq!(f.wb.game_gate(&Command::SetDisplaySettings { settings: toggled.clone() }), None, "{mode:?}");
            assert!(f.wb.set_display_settings(toggled).unwrap(), "{mode:?}");
            assert_eq!(f.wb.inspect().settings.display.name_winrates, on, "{mode:?}");
        }
        let other = settings::DisplaySettings { coordinates: !before.coordinates, ..before.clone() };
        assert_eq!(f.wb.game_gate(&Command::SetDisplaySettings { settings: other }), refused, "{mode:?}");
        let both = settings::DisplaySettings { coordinates: !before.coordinates, name_winrates: !before.name_winrates, ..before };
        assert_eq!(f.wb.game_gate(&Command::SetDisplaySettings { settings: both }), refused, "{mode:?}");
        assert_eq!(session(&f).phase, HumanSlPhase::Playing);
        assert_eq!(f.wb.inspect().position.move_number, 0, "{mode:?}: no move");
    }
}

/// The rank table the setup dialog shows: every opponent, weakest first, with its KataGo level,
/// Fox / Tygem rank and real-world grade as the fixed Java's `CoachLevel` gives them (the sample
/// `fixtures/coach/java-coach.json` was written by running it).
#[test]
fn the_published_opponents_carry_javas_rank_table() {
    let table: Value = serde_json::from_str(include_str!("fixtures/coach/java-coach.json")).unwrap();
    let rows = table["levels"].as_array().unwrap();
    let mut f = fixture(false);
    // A fixed catalogue: in full views (the UI's start-up snapshot) and observations, not in every
    // published view.
    let levels = f.wb.view(true).human_sl_levels.expect("a full view carries the catalogue");
    assert!(f.wb.view(false).human_sl_levels.is_none());
    assert_eq!(serde_json::to_value(f.wb.inspect().human_sl_levels).unwrap(), serde_json::to_value(Some(&levels)).unwrap());
    assert_eq!(levels.len(), 31);
    assert_eq!(rows.len(), 31);
    for (ours, java) in levels.iter().zip(rows) {
        let name = java["name"].as_str().unwrap();
        let ours = serde_json::to_value(ours).unwrap();
        assert_eq!(ours["level"], serde_json::to_value(level_named(name)).unwrap(), "order");
        let kind = match java["kind"].as_str().unwrap() {
            "KYU" => "Kyu",
            "DAN" => "Dan",
            _ => "StrongDan",
        };
        let grade = match java["grade"].as_str().unwrap() {
            "BEYOND_TOP_PRO" => "BeyondTopPro",
            "TOP_PRO" => "TopPro",
            "PRO" => "Pro",
            "TOP_AMATEUR" => "TopAmateur",
            "AMATEUR_DAN" => "AmateurDan",
            "AMATEUR_KYU" => "AmateurKyu",
            other => panic!("{other}"),
        };
        assert_eq!(
            (
                &ours["kind"],
                &ours["rank"],
                &ours["fox"],
                &ours["grade"],
                &ours["grade_number"]
            ),
            (
                &json!(kind),
                &java["rank"],
                &java["fox"],
                &json!(grade),
                &java["gradeNumber"]
            ),
            "{name}"
        );
    }
}

fn level_named(name: &str) -> CoachLevel {
    serde_json::from_value(json!(name)).unwrap()
}

/// Writes an empty file at `path`, with its directories.
fn touch(path: &std::path::Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"").unwrap();
}

/// The start's failure, if it failed, and whether a companion was launched.
fn start_outcome(f: &mut Fixture) -> (Option<String>, bool) {
    assert_eq!(
        f.wb.human_sl(HumanSlCommand::Start {
            settings: settings(PlayerColor::Black),
            from_current: false,
            advanced_visible: false,
            stop_batch: false,
        })
        .unwrap(),
        CommandResult::Done
    );
    let reason = f.wb.inspect().human_sl_notices.into_iter().find_map(|n| match n {
        HumanSlNotice::StartFailed { reason, .. } => Some(reason),
        _ => None,
    });
    let launched = f
        .world
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|p| p.args.first().is_some_and(|a| a == "analysis"));
    (reason, launched)
}

const NO_ENGINE: &str = "AnalysisEngineCommandHelper.noEngineSelected";
const NO_ANALYSIS_CONFIG: &str = "AnalysisEngineCommandHelper.noAnalysisConfig";
const NO_WEIGHT: &str = "AnalysisEngineCommandHelper.noWeight";

/// A KataGo that 一键设置 installed (in the resource directory): AI 陪练 names the first part it
/// lacks, in Java's order (`AnalysisEngineCommandHelper.missingPart`): the engine, its analysis
/// config, its weight; with all three it starts.
#[test]
fn an_installed_engine_names_the_first_missing_part() {
    // (engine, analysis config, weight) present.
    for (parts, expected) in [
        ((false, false, false), Some(NO_ENGINE)),
        ((false, true, true), Some(NO_ENGINE)),
        ((true, false, false), Some(NO_ANALYSIS_CONFIG)),
        ((true, false, true), Some(NO_ANALYSIS_CONFIG)),
        ((true, true, false), Some(NO_WEIGHT)),
        ((true, true, true), None),
    ] {
        let mut f = fixture(true);
        let resources = f.dir.path().join("data/resources");
        let (engine, config, weight) = (
            resources.join("engines/katago/katago"),
            resources.join("configs/analysis.cfg"),
            resources.join("weights/main.bin.gz"),
        );
        for (present, path) in [(parts.0, &engine), (parts.1, &config), (parts.2, &weight)] {
            if present {
                touch(path);
            }
        }
        let command = format!(
            "\"{}\" analysis -model \"{}\" -config \"{}\" -quit-without-waiting",
            engine.display(),
            weight.display(),
            config.display()
        );
        let mut analysis = f.wb.config().analysis_engine.clone();
        analysis.command = command.clone();
        f.wb.save_analysis_engine_settings(analysis, &command, true)
            .unwrap()
            .unwrap();
        let (reason, launched) = start_outcome(&mut f);
        assert_eq!(reason.as_deref(), expected, "{parts:?}");
        assert_eq!(launched, expected.is_none(), "{parts:?}");
        assert_eq!(f.wb.inspect().human_sl.is_some(), expected.is_none(), "{parts:?}");
    }
}

/// No engine to make the analysis command from: the engine is missing.
#[test]
fn without_an_engine_the_engine_is_missing() {
    let mut f = fixture_with(true, |c| {
        c.analysis_engine = Default::default();
    });
    assert_eq!(start_outcome(&mut f), (Some(NO_ENGINE.into()), false));
}

/// An engine's own command made into the analysis command: a missing `analysis.cfg` is made beside
/// its config (a KataGo set up for play only) and the game starts; when it cannot be made (here a
/// file stands where its directory should be), the analysis config is missing.
#[test]
fn an_engine_set_up_for_play_only_gets_its_analysis_config_unless_it_cannot_be_made() {
    for made in [true, false] {
        let mut f = fixture_with(true, |c| c.analysis_engine = Default::default());
        let resources = f.dir.path().join("data/resources");
        let engine = resources.join("engines/katago/katago");
        let weight = resources.join("weights/main.bin.gz");
        let configs = resources.join(if made { "configs" } else { "blocked" });
        touch(&engine);
        touch(&weight);
        if made {
            touch(&configs.join("gtp.cfg"));
        } else {
            touch(&configs);
        }
        f.wb.save_engine_settings(
            vec![EngineProfile {
                name: "KataGo".into(),
                command: format!(
                    "\"{}\" gtp -model \"{}\" -config \"{}\"",
                    engine.display(),
                    weight.display(),
                    configs.join("gtp.cfg").display()
                ),
                ..EngineProfile::default()
            }],
            EngineStartup::Default,
        )
        .unwrap();
        let (reason, launched) = start_outcome(&mut f);
        if made {
            assert_eq!((reason, launched), (None, true));
            assert!(configs.join("analysis.cfg").is_file());
        } else {
            assert_eq!((reason.as_deref(), launched), (Some(NO_ANALYSIS_CONFIG), false));
        }
    }
}

/// A command of the user's own runs as written, as in Java: its files are not checked (its paths
/// may be the engine's own, such as a Windows engine's run from WSL).
#[test]
fn a_users_own_command_is_not_checked_for_missing_parts() {
    for command in [
        "/fake/katago analysis -config /fake/analysis.cfg -model /fake/main.bin.gz",
        "/mnt/e/engines/kata/katago.exe analysis -model E:\\engines\\kata\\weights.bin.gz -config E:\\engines\\kata\\analysis.cfg",
    ] {
        let mut f = fixture(true);
        let mut analysis = f.wb.config().analysis_engine.clone();
        analysis.command = command.into();
        f.wb.save_analysis_engine_settings(analysis, command, true)
            .unwrap()
            .unwrap();
        assert_eq!(start_outcome(&mut f), (None, true), "{command}");
    }
}

// ---- 边下边提醒 (Java `MoveReviewer`, `CoachAssist`, `HumanSlGameController`)

use lizzie_core::humansl::assist::HintStep;
use lizzie_core::humansl::feedback::Verdict;

/// 边下边提醒 against a kyu level at the fast pace (at most 1.8 s), the player Black.
fn feedback_game(f: &mut Fixture) -> InstanceId {
    let s = HumanSlTrainingSettings {
        mode: TrainingMode::LiveFeedback,
        player_color: PlayerColor::Black,
        ..level(CoachLevel::Kyu5, Pace::Fast)
    };
    let id = start(f, s, false);
    ready(f, id);
    id
}
fn feedback(f: &Fixture) -> HumanSlFeedbackView {
    session(f).feedback.expect("边下边提醒")
}
/// The companion's queries since the last look, of kind `review`, `policy`, ... (their ids).
fn asked(f: &Fixture, id: InstanceId, kind: &str) -> Vec<Value> {
    let all: Vec<Value> = f.world.take(id).iter().filter_map(|s| serde_json::from_str::<Value>(s).ok()).collect();
    let (want, keep): (Vec<Value>, Vec<Value>) = all
        .into_iter()
        .partition(|q| q.get("action").is_none() && q["id"].as_str().is_some_and(|i| i.starts_with(&format!("humansl-{kind}-"))));
    // Others stay pending for the next look.
    let mut world = f.world.0.lock().unwrap();
    let process = world.iter_mut().find(|p| p.id == id).unwrap();
    process.sent.splice(0..0, keep.iter().map(Value::to_string));
    want
}
fn one(f: &Fixture, id: InstanceId, kind: &str) -> Value {
    let mut qs = asked(f, id, kind);
    assert_eq!(qs.len(), 1, "one {kind} query: {qs:?}");
    qs.remove(0)
}
/// The review the companion gives: KataGo's search `infos` ((move, visits, lead), best first) over
/// the HumanSL spread `stdev`, and the human policy `shares`.
fn review_answer(q: &Value, stdev: f64, infos: &[(&str, u64, f64)], shares: Value) -> Value {
    let infos: Vec<Value> =
        infos.iter().enumerate().map(|(i, (m, v, l))| json!({"move":m,"order":i,"visits":v,"scoreLead":l})).collect();
    let root = infos.first().map_or(json!(0.0), |i| i["scoreLead"].clone());
    json!({"id":q["id"],"rootInfo":{"scoreLead":root,"humanScoreStdev":stdev},"moveInfos":infos,"humanPolicy":shares})
}
fn reviewed(f: &mut Fixture, id: InstanceId, q: &Value, stdev: f64, infos: &[(&str, u64, f64)], shares: Value) {
    out(&mut f.wb, id, review_answer(q, stdev, infos, shares));
}
/// The player's position: KataGo plays Q16; D4 loses 10 points over a spread of 1 (a 恶手), but
/// with too few visits to rate it from here.
fn review_turn(f: &mut Fixture, id: InstanceId, d4_visits: u64) {
    let q = one(f, id, "review");
    reviewed(f, id, &q, 1.0, &[("Q16", 400, 0.0), ("D4", d4_visits, -10.0)], json!({"Q16":0.2,"D4":0.8}));
}
/// The AI's policy probe answers `vertex` at `at`, past the fast pace's think time.
fn ai_answers(f: &mut Fixture, id: InstanceId, vertex: &str, at: Instant) {
    let q = one(f, id, "policy");
    out_at(&mut f.wb, id, json!({"id":q["id"],"humanPolicy":{(vertex):1.0},"moveInfos":[]}), at);
    f.wb.engine_tick(at);
}
const D4: (u16, u16) = (3, 15);

/// The player's position is reviewed while they think, with Java's budget and the opponent's
/// profile; a move it searched closely is rated at once, and a 恶手 holds the AI's reply.
#[test]
fn the_players_position_is_reviewed_and_a_blunder_holds_the_reply() {
    let mut f = fixture(true);
    let id = feedback_game(&mut f);
    let q = one(&f, id, "review");
    // MoveReviewer.TURN_VISITS / TURN_TIME, HumanSlAnalysisRunner.buildHumanSlRequest.
    assert_eq!(q["maxVisits"], 400);
    let o = &q["overrideSettings"];
    assert_eq!(o["maxTime"], 1.5);
    assert_eq!(o["humanSLProfile"], CoachLevel::Kyu5.play().profile);
    assert_eq!((o["rootNumSymmetriesToSample"].as_u64(), o["ignorePreRootHistory"].as_bool()), (Some(1), Some(false)));
    assert_eq!((o["humanSLRootExploreProbWeightless"].as_f64(), q["includePolicy"].as_bool()), (Some(0.5), Some(true)));
    assert_eq!(o["reportAnalysisWinratesAs"], "SIDETOMOVE");
    reviewed(&mut f, id, &q, 1.0, &[("Q16", 400, 0.0), ("D4", 400, -10.0)], json!({"Q16":0.2,"D4":0.8}));
    assert_eq!(session(&f).last_move, None);
    assert_eq!(click(&mut f, D4.0, D4.1), CommandResult::Done);
    let fb = feedback(&f);
    assert!(!fb.judging, "the review had looked at D4 closely");
    assert_eq!(fb.ratings.len(), 1);
    let r = &fb.ratings[0];
    assert_eq!((r.move_number, r.vertex.as_str(), r.verdict, r.best_move.as_str()), (1, "D4", Verdict::Blunder, "Q16"));
    assert_eq!(session(&f).last_move, Some(HumanSlLastMove { move_number: 1, vertex: "D4".into(), verdict: Some(Verdict::Blunder) }));
    ai_answers(&mut f, id, "C17", Instant::now() + Duration::from_secs(2));
    let s = session(&f);
    let decision = s.feedback.as_ref().unwrap().decision.clone().expect("the AI waits for the player");
    assert_eq!((decision.rating.verdict, decision.best_shown), (Verdict::Blunder, false));
    assert!(!s.ai_thinking && !s.human_turn && s.can_take_back);
    assert_eq!(s.clock, Some(HumanSlSide::Human), "the player decides on their clock");
    assert_eq!(f.wb.inspect().position.move_number, 1, "the reply is not played");
    // Nothing else is asked or played while the player decides; no hint, no move.
    f.wb.engine_tick(Instant::now() + Duration::from_secs(60));
    assert!(session(&f).feedback.unwrap().decision.is_some());
    assert_eq!(f.wb.human_sl(HumanSlCommand::Hint).unwrap(), CommandResult::Unchanged);
    assert_eq!(click(&mut f, 15, 15), CommandResult::Unchanged);
    assert_eq!(f.wb.human_sl(HumanSlCommand::Pass).unwrap(), CommandResult::Unchanged);
    assert_eq!(f.wb.inspect().position.move_number, 1);
}

/// 看 AI 推荐 marks KataGo's choice; 不重下，继续对局 plays the held reply; 重下这手 is the
/// take-back of the 恶手, whose rating goes with it, and the held reply is never played.
#[test]
fn the_three_choices_after_a_blunder() {
    for choice in [HumanSlDecision::ShowBest, HumanSlDecision::PlayOn, HumanSlDecision::Retry] {
        let mut f = fixture(true);
        let id = feedback_game(&mut f);
        let start_node = f.wb.cursor();
        review_turn(&mut f, id, 400);
        click(&mut f, D4.0, D4.1);
        let blunder = f.wb.cursor();
        ai_answers(&mut f, id, "C17", Instant::now() + Duration::from_secs(2));
        assert!(feedback(&f).decision.is_some());
        assert_eq!(f.wb.human_sl(HumanSlCommand::Decide { choice }).unwrap(), CommandResult::Done, "{choice:?}");
        let s = session(&f);
        let fb = s.feedback.clone().unwrap();
        match choice {
            HumanSlDecision::ShowBest => {
                assert!(fb.decision.as_ref().unwrap().best_shown);
                assert_eq!(fb.marks.point, Some(Point { x: 15, y: 3 }), "Q16");
                assert_eq!(fb.marks.area, None);
                assert_eq!(f.wb.cursor(), blunder, "still waiting for the player");
            }
            HumanSlDecision::PlayOn => {
                assert!(fb.decision.is_none() && s.human_turn);
                assert_eq!(f.wb.document().record.parent(f.wb.cursor()), Some(blunder));
                assert_eq!(played(&f, f.wb.cursor()), Some(Point { x: 2, y: 2 }), "C17");
                assert_eq!(fb.ratings.len(), 1, "the 恶手 stays rated");
                // The player's next turn is reviewed.
                assert_eq!(asked(&f, id, "review").len(), 1);
            }
            HumanSlDecision::Retry => {
                assert_eq!(f.wb.cursor(), start_node);
                assert!(s.human_turn && fb.decision.is_none() && fb.ratings.is_empty() && s.last_move.is_none());
                assert_eq!(s.take_backs, 1);
                assert_eq!(children(&f, start_node), vec![blunder], "the 恶手 stays in the record");
                f.wb.engine_tick(Instant::now() + Duration::from_secs(30));
                assert_eq!(f.wb.cursor(), start_node, "the held reply is never played");
                assert!(children(&f, blunder).is_empty());
            }
        }
        // A choice is only for a held reply.
        if choice != HumanSlDecision::ShowBest {
            assert_eq!(f.wb.human_sl(HumanSlCommand::Decide { choice: HumanSlDecision::PlayOn }).unwrap(), CommandResult::Unchanged);
        }
    }
}

/// The reply waits for a rating still on its way for at most 4 s after its think time; a 恶手
/// rated in time holds it. One rated later does not, whether the rating or the end of the wait is
/// handled first: the reply is played once and the rating is kept (Java `blunderToHoldFor` takes
/// the rating there is when the wait ends).
#[test]
fn the_reply_waits_four_seconds_for_the_rating() {
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Rated {
        InTime,
        AfterTheWaitEnded,
        LateBeforeTheWaitEnded,
    }
    for rated in [Rated::InTime, Rated::AfterTheWaitEnded, Rated::LateBeforeTheWaitEnded] {
        let mut f = fixture(true);
        let id = feedback_game(&mut f);
        review_turn(&mut f, id, 10);
        click(&mut f, D4.0, D4.1);
        let d4 = f.wb.cursor();
        assert!(feedback(&f).judging, "D4 needs the position after it");
        let after = one(&f, id, "review");
        // MoveReviewer.AFTER_VISITS / AFTER_TIME.
        assert_eq!((after["maxVisits"].as_u64(), after["overrideSettings"]["maxTime"].as_f64()), (Some(200), Some(0.8)));
        // The reply is chosen at `t`, past its think time: the wait ends at `t` + 4 s.
        let t = Instant::now() + Duration::from_secs(2);
        ai_answers(&mut f, id, "C17", t);
        assert_eq!(f.wb.inspect().position.move_number, 1, "the reply waits for the rating");
        assert!(session(&f).ai_thinking);
        f.wb.engine_tick(t + Duration::from_millis(3900));
        assert_eq!(f.wb.inspect().position.move_number, 1);
        // White leads by 10 points after D4.
        let rating = review_answer(&after, 1.0, &[("C17", 200, 10.0)], json!({"C17":1.0}));
        let late = t + Duration::from_millis(4100);
        match rated {
            Rated::InTime => {
                out_at(&mut f.wb, id, rating, t + Duration::from_millis(3950));
                let fb = feedback(&f);
                assert_eq!(fb.decision.map(|d| d.rating.verdict), Some(Verdict::Blunder));
                assert_eq!(f.wb.inspect().position.move_number, 1, "held");
                continue;
            }
            Rated::AfterTheWaitEnded => {
                f.wb.engine_tick(late);
                assert_eq!(f.wb.inspect().position.move_number, 2, "played without the rating");
                out_at(&mut f.wb, id, rating, late);
            }
            Rated::LateBeforeTheWaitEnded => {
                out_at(&mut f.wb, id, rating, late);
                f.wb.engine_tick(late);
            }
        }
        let fb = feedback(&f);
        assert!(fb.decision.is_none() && !fb.judging, "{rated:?}: {fb:?}");
        assert_eq!(fb.ratings.iter().map(|r| r.verdict).collect::<Vec<_>>(), vec![Verdict::Blunder], "{rated:?}: rated late");
        assert_eq!(f.wb.inspect().position.move_number, 2, "{rated:?}: the reply is played");
        let replies = children(&f, d4);
        assert_eq!(replies.len(), 1, "{rated:?}: one reply");
        assert_eq!(played(&f, replies[0]), Some(Point { x: 2, y: 2 }), "{rated:?}: C17");
        assert!(session(&f).human_turn, "{rated:?}");
    }
}

/// A good move does not hold the reply; a review that does not answer in 15 s is ended, and a
/// move the engine could not review is not rated, nor does it hold the reply.
#[test]
fn a_good_move_or_a_review_that_times_out_holds_nothing() {
    let mut f = fixture(true);
    let id = feedback_game(&mut f);
    review_turn(&mut f, id, 400);
    click(&mut f, 15, 3);
    assert_eq!(feedback(&f).ratings[0].verdict, Verdict::Good);
    let t = Instant::now() + Duration::from_secs(2);
    ai_answers(&mut f, id, "C17", t);
    assert_eq!(f.wb.inspect().position.move_number, 2);
    // The review of the next turn, asked when the reply was played, never answers.
    let turn = one(&f, id, "review");
    click(&mut f, D4.0, D4.1);
    assert!(feedback(&f).judging);
    ai_answers(&mut f, id, "R3", t + Duration::from_secs(1));
    f.wb.engine_tick(t + Duration::from_secs(6));
    assert_eq!(f.wb.inspect().position.move_number, 4, "the reply waited 4 s at most");
    f.wb.engine_tick(t + Duration::from_secs(16));
    let lines: Vec<Value> = f.world.take(id).iter().filter_map(|s| serde_json::from_str::<Value>(s).ok()).collect();
    let cancelled: Vec<&Value> = lines.iter().filter(|q| q["action"] == "terminate").collect();
    assert_eq!(cancelled.len(), 1, "{lines:?}");
    assert_eq!(cancelled[0]["terminateId"], turn["id"]);
    // The position before D4 is asked again; the engine fails it, and D4 is not rated.
    let again: Vec<&Value> = lines.iter().filter(|q| q["id"].as_str().is_some_and(|i| i.starts_with("humansl-review-"))).collect();
    assert_eq!(again.len(), 1, "{lines:?}");
    assert_eq!(again[0]["analyzeTurns"], json!([2]));
    out(&mut f.wb, id, json!({"id":again[0]["id"],"error":"engine failure"}));
    let fb = feedback(&f);
    assert!(!fb.judging && fb.ratings.len() == 1);
    assert_eq!(session(&f).last_move.map(|m| (m.move_number, m.verdict)), Some((3, None)), "这手没能评估");
}

/// A review that has not answered within 15 s counts as none, also when its answer is read before
/// the timeout is handled: no hint, marks or hint count come of it, and nothing is kept for the
/// position. The same answer in time gives the hint (Java `MoveReviewer.TIMEOUT`).
#[test]
fn a_review_answered_after_fifteen_seconds_counts_as_none() {
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Answered {
        InTime,
        AfterTheTimeout,
        LateBeforeTheTimeout,
    }
    let mut late = Vec::new();
    for answered in [Answered::InTime, Answered::AfterTheTimeout, Answered::LateBeforeTheTimeout] {
        let mut f = fixture(true);
        let id = start(
            &mut f,
            HumanSlTrainingSettings {
                mode: TrainingMode::LiveFeedback,
                player_color: PlayerColor::Black,
                ..level(CoachLevel::Kyu5, Pace::Fast)
            },
            false,
        );
        // The game begins at `t` with the player to move, whose position is asked about then.
        let t = Instant::now();
        ready_at(&mut f, id, t);
        let q = one(&f, id, "review");
        assert_eq!(f.wb.human_sl(HumanSlCommand::Hint).unwrap(), CommandResult::Done);
        let answer = review_answer(&q, 1.0, &[("Q16", 400, 0.0), ("D4", 400, -10.0)], json!({"Q16":0.2,"D4":0.8}));
        let timeout = t + Duration::from_secs(15);
        match answered {
            Answered::InTime => {
                out_at(&mut f.wb, id, answer, t + Duration::from_millis(14_900));
                let fb = feedback(&f);
                assert_eq!(fb.hint.map(|h| h.step), Some(HintStep::Area));
                assert_eq!((fb.hints, fb.marks.area), (1, Some([13, 0, 18, 5])));
                continue;
            }
            Answered::AfterTheTimeout => {
                f.wb.engine_tick(timeout);
                out_at(&mut f.wb, id, answer, timeout);
            }
            Answered::LateBeforeTheTimeout => {
                out_at(&mut f.wb, id, answer, timeout);
                f.wb.engine_tick(timeout);
            }
        }
        let fb = feedback(&f);
        assert_eq!(fb.hint.as_ref().map(|h| h.step), Some(HintStep::Waiting), "{answered:?}");
        assert_eq!((fb.hints, fb.marks.area, fb.marks.point), (0, None, None), "{answered:?}");
        // D4 was searched closely, but the position is asked about again to rate it.
        click(&mut f, D4.0, D4.1);
        assert!(feedback(&f).judging, "{answered:?}");
        assert_eq!(one(&f, id, "review")["analyzeTurns"], json!([0]), "{answered:?}");
        late.push(fb);
    }
    assert_eq!(late[0], late[1], "the order the answer and the timeout are handled in changes nothing");
}

/// A rating that arrives after the session ended changes nothing, and nothing is being rated any
/// more (Java `MoveReviewer.close`); a take-back drops the ratings of the moves it withdrew, also
/// one still on its way.
#[test]
fn ratings_after_the_session_or_of_withdrawn_moves_are_dropped() {
    let mut f = fixture(true);
    let fg = foreground(&mut f);
    let id = feedback_game(&mut f);
    serve(&mut f, fg, false);
    review_turn(&mut f, id, 10);
    click(&mut f, D4.0, D4.1);
    let after = one(&f, id, "review");
    // 悔棋 while the AI thinks: the move's rating, still on its way, is dropped when it comes.
    assert_eq!(take_back(&mut f), CommandResult::Done);
    asked(&f, id, "policy");
    let fb = feedback(&f);
    assert!(!fb.judging && fb.ratings.is_empty() && session(&f).last_move.is_none());
    reviewed(&mut f, id, &after, 1.0, &[("C17", 200, 10.0)], json!({"C17":1.0}));
    assert!(feedback(&f).ratings.is_empty());
    // In the mode that hides analysis, the foreground engine heard nothing of it.
    f.wb.engine_tick(Instant::now());
    assert!(serve(&mut f, fg, false).is_empty());
    // A move rated from the review kept for the position, then taken back with the AI's reply.
    assert!(asked(&f, id, "review").is_empty(), "the position was reviewed before");
    click(&mut f, 15, 3);
    ai_answers(&mut f, id, "C17", Instant::now() + Duration::from_secs(2));
    assert_eq!(feedback(&f).ratings.len(), 1);
    assert_eq!(take_back(&mut f), CommandResult::Done);
    assert!(feedback(&f).ratings.is_empty());
    // A move being rated when the session ends (C3, which the review did not search): its rating
    // comes too late.
    click(&mut f, 2, 16);
    assert!(feedback(&f).judging);
    let after = asked(&f, id, "review").pop().unwrap();
    let worker = f.world.worker();
    f.wb.human_sl(HumanSlCommand::Finish).unwrap();
    let s = session(&f);
    assert_eq!(s.phase, HumanSlPhase::Stopping, "the foreground engine is given back first");
    assert!(s.feedback.is_none(), "nothing is rated any more");
    let record = f.wb.document().revision;
    reviewed(&mut f, worker, &after, 1.0, &[("C17", 200, 10.0)], json!({"C17":1.0}));
    assert!(session(&f).feedback.is_none());
    assert_eq!(f.wb.document().revision, record);
}

/// 悔棋 while the withdrawn move is being rated from the position after it ends that review: the
/// next move's review is asked at once, not after it or its 15 s, and its late answer changes
/// nothing (the take-back ends the searches in progress).
#[test]
fn a_take_back_ends_the_review_of_the_withdrawn_move() {
    let mut f = fixture(true);
    let id = feedback_game(&mut f);
    review_turn(&mut f, id, 10);
    click(&mut f, D4.0, D4.1);
    let withdrawn = one(&f, id, "review");
    let ai = one(&f, id, "policy");
    assert_eq!(take_back(&mut f), CommandResult::Done);
    let lines: Vec<Value> = f.world.take(id).iter().filter_map(|s| serde_json::from_str::<Value>(s).ok()).collect();
    let ended: Vec<&Value> = lines.iter().filter(|q| q["action"] == "terminate").map(|q| &q["terminateId"]).collect();
    assert!(ended.contains(&&withdrawn["id"]), "{lines:?}");
    assert!(ended.contains(&&ai["id"]), "{lines:?}");
    // C3, which the review did not search, is rated from the position after it.
    click(&mut f, 2, 16);
    assert!(feedback(&f).judging);
    let next = one(&f, id, "review");
    assert_ne!(next["id"], withdrawn["id"]);
    reviewed(&mut f, id, &withdrawn, 1.0, &[("C17", 200, 10.0)], json!({"C17":1.0}));
    let fb = feedback(&f);
    assert!(fb.judging && fb.ratings.is_empty() && fb.decision.is_none(), "{fb:?}");
    reviewed(&mut f, id, &next, 1.0, &[("C17", 200, 0.0)], json!({"C17":1.0}));
    let fb = feedback(&f);
    assert!(!fb.judging);
    assert_eq!(fb.ratings.iter().map(|r| (r.move_number, r.vertex.as_str())).collect::<Vec<_>>(), vec![(1, "C3")]);
}

/// 悔棋 after the AI replied while the player's move was still being rated: the review of the
/// withdrawn move is ended, the position after the AI's withdrawn reply, whose turn review was
/// waiting behind it, is not searched, and the next move's review is asked at once; only that
/// move ends up rated.
#[test]
fn a_take_back_after_the_reply_searches_nothing_of_the_withdrawn_moves() {
    let mut f = fixture(true);
    let id = feedback_game(&mut f);
    review_turn(&mut f, id, 10);
    click(&mut f, D4.0, D4.1);
    let withdrawn = one(&f, id, "review");
    // The reply waits its 4 s for D4's rating, which does not come, and is played.
    let t = Instant::now() + Duration::from_secs(2);
    ai_answers(&mut f, id, "C17", t);
    f.wb.engine_tick(t + Duration::from_millis(4100));
    assert_eq!(f.wb.inspect().position.move_number, 2, "C17 is played");
    assert!(feedback(&f).judging, "D4 is still being rated");
    assert!(asked(&f, id, "review").is_empty(), "the turn after C17 waits for D4's review");
    assert_eq!(take_back(&mut f), CommandResult::Done);
    assert_eq!(f.wb.inspect().position.move_number, 0);
    let lines: Vec<Value> = f.world.take(id).iter().filter_map(|s| serde_json::from_str::<Value>(s).ok()).collect();
    let ended: Vec<&Value> = lines.iter().filter(|q| q["action"] == "terminate").map(|q| &q["terminateId"]).collect();
    assert_eq!(ended, vec![&withdrawn["id"]], "{lines:?}");
    let searched: Vec<&Value> = lines.iter().filter(|q| q.get("action").is_none()).collect();
    assert!(searched.is_empty(), "the position to move in was reviewed before, the withdrawn ones are not: {searched:?}");
    // C3, which the review did not search, is rated from the position after it, asked at once.
    click(&mut f, 2, 16);
    assert!(feedback(&f).judging);
    let next = one(&f, id, "review");
    assert_eq!(next["moves"], json!([["B", "C3"]]), "{next}");
    reviewed(&mut f, id, &withdrawn, 1.0, &[("C17", 200, 10.0)], json!({"C17":1.0}));
    let fb = feedback(&f);
    assert!(fb.judging && fb.ratings.is_empty() && fb.decision.is_none(), "{fb:?}");
    reviewed(&mut f, id, &next, 1.0, &[("C17", 200, 0.0)], json!({"C17":1.0}));
    let fb = feedback(&f);
    assert!(!fb.judging);
    assert_eq!(fb.ratings.iter().map(|r| (r.move_number, r.vertex.as_str())).collect::<Vec<_>>(), vec![(1, "C3")]);
    assert!(asked(&f, id, "review").is_empty(), "{:?}", f.world.take(id));
}

/// 提示: the area of KataGo's choice first, then its point; asked before the position's review,
/// the area comes with it. The session counts the positions a hint was asked in.
#[test]
fn a_hint_marks_the_area_then_the_point_and_is_counted() {
    let mut f = fixture(true);
    let id = feedback_game(&mut f);
    let fb = feedback(&f);
    assert!(fb.can_hint && fb.hint.is_none() && fb.hints == 0);
    assert_eq!(f.wb.human_sl(HumanSlCommand::Hint).unwrap(), CommandResult::Done);
    let fb = feedback(&f);
    assert_eq!(fb.hint.map(|h| h.step), Some(HintStep::Waiting));
    assert_eq!((fb.hints, fb.marks.area, fb.marks.point), (0, None, None));
    review_turn(&mut f, id, 400);
    let fb = feedback(&f);
    let hint = fb.hint.unwrap();
    // Q16 is in the top right of the nine areas (0, 6, 13 and 19 split a side).
    assert_eq!((hint.step, hint.area, hint.vertex.as_deref(), hint.share), (HintStep::Area, Some(2), Some("Q16"), Some(0.2)));
    assert_eq!((fb.marks.area, fb.marks.point, fb.hints), (Some([13, 0, 18, 5]), None, 1));
    f.wb.human_sl(HumanSlCommand::Hint).unwrap();
    let fb = feedback(&f);
    assert_eq!(fb.hint.map(|h| h.step), Some(HintStep::Point));
    assert_eq!((fb.marks.area, fb.marks.point, fb.hints), (None, Some(Point { x: 15, y: 3 }), 1));
    f.wb.human_sl(HumanSlCommand::Hint).unwrap();
    assert_eq!(feedback(&f).marks.point, Some(Point { x: 15, y: 3 }));
    assert_eq!(feedback(&f).hints, 1);
    // The move clears the turn's help; the next turn's hint counts again, at once from its review.
    click(&mut f, 15, 3);
    let fb = feedback(&f);
    assert!(fb.hint.is_none() && fb.marks.area.is_none() && fb.marks.point.is_none() && !fb.can_hint);
    assert_eq!(f.wb.human_sl(HumanSlCommand::Hint).unwrap(), CommandResult::Unchanged, "the AI's turn");
    ai_answers(&mut f, id, "C17", Instant::now() + Duration::from_secs(2));
    let q = one(&f, id, "review");
    reviewed(&mut f, id, &q, 1.0, &[("D16", 400, 0.0)], json!({"D16":1.0}));
    f.wb.human_sl(HumanSlCommand::Hint).unwrap();
    let fb = feedback(&f);
    assert_eq!((fb.hint.unwrap().area, fb.marks.area, fb.hints), (Some(0), Some([0, 0, 5, 5]), 2));
    // On a browsed position the board only goes back to the game's.
    assert!(f.wb.navigate(Navigation::Back(1)));
    f.wb.view(false);
    f.wb.human_sl(HumanSlCommand::Hint).unwrap();
    assert_eq!(f.wb.inspect().position.move_number, 2);
    assert!(f.wb.inspect().human_sl_notices.contains(&HumanSlNotice::BackToLiveGame));
    assert_eq!(feedback(&f).hint.unwrap().step, HintStep::Area, "no second step yet");
}

/// H in 实战后复盘 says analysis stays hidden; 边下边分析 has no hint.
#[test]
fn the_hint_in_the_other_modes() {
    for mode in [TrainingMode::PostGameReview, TrainingMode::LiveAnalysis] {
        let mut f = fixture(true);
        let id = start(&mut f, HumanSlTrainingSettings { mode, ..settings(PlayerColor::Black) }, false);
        ready(&mut f, id);
        assert!(session(&f).feedback.is_none());
        let done = f.wb.human_sl(HumanSlCommand::Hint).unwrap();
        let notices = f.wb.inspect().human_sl_notices;
        if mode == TrainingMode::PostGameReview {
            assert_eq!(done, CommandResult::Done);
            assert!(notices.contains(&HumanSlNotice::AnalysisHidden), "{notices:?}");
        } else {
            assert_eq!(done, CommandResult::Unchanged);
            assert!(!notices.contains(&HumanSlNotice::AnalysisHidden));
        }
        assert!(f.world.take(id).is_empty(), "nothing is reviewed");
    }
}

/// A data folder that never saved a choice starts AI 陪练 in 边下边提醒; a saved choice stays.
#[test]
fn live_feedback_is_the_default_only_without_a_saved_choice() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data"));
    let wb = Workbench::new(storage, None, Box::new(World::default()), vec![]);
    assert!(!dir.path().join("config").exists() || std::fs::read_dir(dir.path().join("config")).unwrap().next().is_none());
    assert_eq!(wb.config().human_sl.training.mode, TrainingMode::LiveFeedback);
    for mode in [TrainingMode::PostGameReview, TrainingMode::LiveAnalysis] {
        let f = fixture_with(true, |c| c.human_sl.training.mode = mode);
        assert_eq!(f.wb.config().human_sl.training.mode, mode);
        assert_eq!(f.wb.inspect().settings.human_sl.training.mode, mode);
    }
}

// ---- What the game shows and its summary (Java `CoachPanelState`, `MoveReviewer`,
// `HumanSlGameController.reviewFrom`, `abortAndThen`)

/// 边下边提醒 through a fixed game, the player Black against a kyu level: after a hint, KataGo's
/// choice B1; the AI A1; A2, KataGo's choice, takes it; the AI Q16; D4, which loses 10 points over
/// a spread of 1 (a 恶手), and the player plays on; the AI C17. The next turn's review is asked
/// and not answered.
fn rated_game(f: &mut Fixture) -> InstanceId {
    let id = feedback_game(f);
    f.wb.human_sl(HumanSlCommand::Hint).unwrap();
    let q = one(f, id, "review");
    reviewed(f, id, &q, 1.0, &[("B1", 400, 0.0)], json!({"B1":1.0}));
    assert_eq!(click(f, 1, 18), CommandResult::Done);
    ai_answers(f, id, "A1", Instant::now() + Duration::from_secs(2));
    let q = one(f, id, "review");
    reviewed(f, id, &q, 1.0, &[("A2", 400, 0.0)], json!({"A2":1.0}));
    assert_eq!(click(f, 0, 17), CommandResult::Done);
    ai_answers(f, id, "Q16", Instant::now() + Duration::from_secs(2));
    let q = one(f, id, "review");
    reviewed(f, id, &q, 1.0, &[("D16", 400, 0.0), ("D4", 400, -10.0)], json!({"D16":0.5,"D4":0.5}));
    assert_eq!(click(f, D4.0, D4.1), CommandResult::Done);
    ai_answers(f, id, "C17", Instant::now() + Duration::from_secs(2));
    assert!(feedback(f).decision.is_some(), "the AI holds its reply to the 恶手");
    f.wb.human_sl(HumanSlCommand::Decide { choice: HumanSlDecision::PlayOn }).unwrap();
    id
}

fn close_to(got: &[f64], want: &[f64]) {
    assert_eq!(got.len(), want.len(), "{got:?} against {want:?}");
    assert!(got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-3), "{got:?} against {want:?}");
}

fn move_numbers(moves: &[lizzie_core::humansl::feedback::MoveFeedback]) -> Vec<u32> {
    moves.iter().map(|m| m.move_number).collect()
}

/// Through a fixed game the session shows the game's captures, komi, clocks, rating counts, rated
/// moves, hints, take-backs, moves and the player's win chance, as the game went.
#[test]
fn the_game_shown_follows_a_fixed_sequence() {
    let mut f = fixture(true);
    let id = rated_game(&mut f);
    let s = session(&f);
    let fb = s.feedback.clone().unwrap();
    assert_eq!(s.moves, 6, "B1 A1 A2 Q16 D4 C17");
    assert_eq!((s.black_captures, s.white_captures), (1, 0), "A2 took A1");
    assert_eq!(s.komi, f.wb.inspect().komi);
    assert!(s.ai_ms >= 4000, "two replies after 2 s each: {}", s.ai_ms);
    assert_eq!(s.last_move, Some(HumanSlLastMove { move_number: 5, vertex: "D4".into(), verdict: Some(Verdict::Blunder) }));
    assert_eq!(fb.ratings.iter().map(|r| (r.move_number, r.verdict)).collect::<Vec<_>>(), vec![
        (1, Verdict::Good),
        (3, Verdict::Good),
        (5, Verdict::Blunder),
    ]);
    assert_eq!(fb.counts, [0, 2, 0, 0, 1]);
    assert_eq!(fb.key_mistakes, 1);
    assert_eq!(move_numbers(&fb.worst), vec![5]);
    assert_eq!((fb.hints, s.take_backs), (1, 0));
    // Before and after each rated move; D4 left the player almost nothing.
    close_to(s.win_chances.as_deref().unwrap(), &[50.0, 50.0, 50.0, 50.0, 50.0, 0.0]);
    // The position to move in, once reviewed: KataGo's choice leads by 2 points over a spread of 1.
    let q = one(&f, id, "review");
    reviewed(&mut f, id, &q, 1.0, &[("Q4", 400, 2.0)], json!({"Q4":1.0}));
    close_to(session(&f).win_chances.as_deref().unwrap(), &[50.0, 50.0, 50.0, 50.0, 50.0, 0.0, 97.725]);
    // Browsing shows another position on the board; the game's numbers stay the game's.
    assert!(f.wb.navigate(Navigation::First));
    assert_eq!(f.wb.inspect().position.black_captures, 0);
    let browsed = session(&f);
    assert_eq!((browsed.black_captures, browsed.moves, browsed.last_move.map(|m| m.move_number)), (1, 6, Some(5)));
}

/// After the player resigned, the summary has the result, the moves of both players, the key
/// mistakes (the 恶手 only), the hints, the take-backs and the worst mistakes until the session ends.
#[test]
fn the_summary_after_resigning() {
    let mut f = fixture(true);
    rated_game(&mut f);
    assert_eq!(f.wb.human_sl(HumanSlCommand::Resign).unwrap(), CommandResult::Done);
    let s = session(&f);
    assert_eq!(s.result, Some(HumanSlResult::Resigned { winner: Stone::White }));
    let fb = s.feedback.expect("the ratings stay for the summary");
    assert_eq!((s.moves, fb.key_mistakes, fb.hints, s.take_backs), (6, 1, 1, 0));
    assert_eq!(move_numbers(&fb.worst), vec![5]);
    assert_eq!(fb.counts, [0, 2, 0, 0, 1]);
    assert_eq!(f.wb.document().record.info.result, "W+R");
}

/// A take-back drops what was said about the withdrawn moves from the counts, the worst mistakes
/// and the win chance.
#[test]
fn a_take_back_leaves_the_withdrawn_ratings_out_of_the_summary() {
    let mut f = fixture(true);
    rated_game(&mut f);
    assert_eq!(take_back(&mut f), CommandResult::Done);
    let s = session(&f);
    let fb = s.feedback.clone().unwrap();
    assert_eq!((s.take_backs, s.moves), (1, 4));
    assert_eq!(fb.counts, [0, 2, 0, 0, 0]);
    assert_eq!(fb.key_mistakes, 0);
    assert!(fb.worst.is_empty());
    assert_eq!(s.last_move, Some(HumanSlLastMove { move_number: 3, vertex: "A2".into(), verdict: Some(Verdict::Good) }));
    // Before and after B1 and A2, then in the position to move in again: the take-back ended the
    // review of the withdrawn position, and the one kept for this position is shown at once.
    close_to(s.win_chances.as_deref().unwrap(), &[50.0, 50.0, 50.0, 50.0, 50.0]);
}

/// 边下边分析 and 实战后复盘 show the player's last move without a rating, and nothing of the
/// ratings or the help.
#[test]
fn the_other_modes_show_the_last_move_unrated() {
    for mode in [TrainingMode::PostGameReview, TrainingMode::LiveAnalysis] {
        let mut f = fixture(true);
        let id = start(&mut f, HumanSlTrainingSettings { mode, ..settings(PlayerColor::Black) }, false);
        ready(&mut f, id);
        assert_eq!(session(&f).last_move, None);
        click(&mut f, D4.0, D4.1);
        answer(&mut f, id, "Q16");
        let s = session(&f);
        assert_eq!(s.last_move, Some(HumanSlLastMove { move_number: 1, vertex: "D4".into(), verdict: None }), "{mode:?}");
        assert_eq!(s.moves, 2);
        assert!(s.feedback.is_none(), "{mode:?}");
    }
}

/// The win chance by mode: none in 实战后复盘; in 边下边分析 the foreground analysis of each
/// analysed position of the game from its start, for the player's side, whatever the board shows.
#[test]
fn the_win_chance_by_mode() {
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, id);
    assert_eq!(session(&f).win_chances, None, "实战后复盘");

    let mut f = fixture(true);
    let fg = foreground(&mut f);
    let id = start(&mut f, HumanSlTrainingSettings { mode: TrainingMode::LiveAnalysis, ..settings(PlayerColor::Black) }, false);
    ready(&mut f, id);
    assert_eq!(session(&f).win_chances, Some(vec![]));
    // The start, Black (the player) to move: 60%.
    serve(&mut f, fg, false);
    f.wb.engine_input(fg, EngineInput::Stdout("info move D4 visits 10 winrate 0.6 scoreLead 1 prior 0.2 order 0 pv D4".into()));
    click(&mut f, D4.0, D4.1);
    // After D4, White (the AI) to move at 70%: 30% for the player.
    serve(&mut f, fg, false);
    f.wb.engine_input(fg, EngineInput::Stdout("info move Q16 visits 10 winrate 0.7 scoreLead 1 prior 0.2 order 0 pv Q16".into()));
    // The AI's reply is not analysed yet and adds nothing.
    answer(&mut f, id, "Q16");
    serve(&mut f, fg, false);
    close_to(session(&f).win_chances.as_deref().unwrap(), &[60.0, 30.0]);
    assert!(f.wb.navigate(Navigation::First));
    serve(&mut f, fg, false);
    close_to(session(&f).win_chances.as_deref().unwrap(), &[60.0, 30.0]);
}

/// 再来一盘 after the game ends the session like the other continuations: the setup is published
/// once after the exit, the analysis does not start, the result stays, and a second request finds
/// the slot taken.
#[test]
fn play_again_ends_the_session_then_opens_the_setup() {
    let mut f = fixture(true);
    let fg = foreground(&mut f);
    f.wb.toggle_analysis().unwrap();
    serve(&mut f, fg, false);
    rated_game(&mut f);
    f.wb.human_sl(HumanSlCommand::Resign).unwrap();
    assert_eq!(abort_then(&mut f, "HumanSlSetup"), CommandResult::Done);
    assert_eq!(abort_then(&mut f, "HumanSlSetup"), CommandResult::Unchanged);
    assert_eq!(published_continuation(&mut f), Value::Null, "not before the exit");
    serve(&mut f, fg, false);
    assert!(f.wb.inspect().human_sl.is_none());
    assert!(f.wb.inspect().engine.paused, "no analysis after the game");
    assert_eq!(f.wb.document().record.info.result, "W+R");
    assert_eq!(published_continuation(&mut f), "HumanSlSetup");
    assert_eq!(published_continuation(&mut f), Value::Null);
}

/// A mistake picked after the game ends the session and shows the position before it, without
/// starting the analysis; the result stays. Nothing happens while the game is on or in another
/// mode, nor once the exit has a continuation; a move the game does not have only ends it.
#[test]
fn reviewing_from_a_mistake_shows_the_position_before_it() {
    let review_from = |f: &mut Fixture, n: u32| f.wb.human_sl(HumanSlCommand::ReviewFrom { move_number: n }).unwrap();

    let mut f = fixture(true);
    let fg = foreground(&mut f);
    f.wb.toggle_analysis().unwrap();
    serve(&mut f, fg, false);
    rated_game(&mut f);
    assert_eq!(review_from(&mut f, 5), CommandResult::Unchanged, "the game is on");
    f.wb.human_sl(HumanSlCommand::Resign).unwrap();
    assert!(f.wb.navigate(Navigation::First));
    assert_eq!(review_from(&mut f, 5), CommandResult::Done);
    assert_eq!(abort_then(&mut f, "NewGame"), CommandResult::Unchanged, "one continuation");
    serve(&mut f, fg, false);
    let v = f.wb.inspect();
    assert!(v.human_sl.is_none());
    assert_eq!(v.position.move_number, 4, "before D4");
    assert!(v.engine.paused, "the analysis does not start");
    assert_eq!(f.wb.document().record.info.result, "W+R");
    assert_eq!(published_continuation(&mut f), Value::Null);

    // A move number the game does not have: the session ends where the board is.
    let mut f = fixture(true);
    rated_game(&mut f);
    f.wb.human_sl(HumanSlCommand::Resign).unwrap();
    let at = f.wb.cursor();
    assert_eq!(review_from(&mut f, 40), CommandResult::Done);
    assert!(f.wb.inspect().human_sl.is_none());
    assert_eq!(f.wb.cursor(), at);

    // Another mode.
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, id);
    click(&mut f, D4.0, D4.1);
    f.wb.human_sl(HumanSlCommand::Resign).unwrap();
    assert_eq!(review_from(&mut f, 1), CommandResult::Unchanged);
    assert!(f.wb.inspect().human_sl.is_some());
}

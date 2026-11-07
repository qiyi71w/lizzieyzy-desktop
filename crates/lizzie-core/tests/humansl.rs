//! HumanSL's public command/engine-input seam with controlled analysis and foreground processes.
use lizzie_core::api::*;
use lizzie_core::browse::Navigation;
use lizzie_core::engine::{
    gtp::LaunchSpec,
    profile::{EngineProfile, EngineStartup},
    runtime::{EngineInput, EngineIo, InstanceId, Launcher},
};
use lizzie_core::record::{Point, Stone, ROOT};
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
    f.wb.engine_tick(Instant::now() + Duration::from_secs(5));
}
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
    let id = f.world.0.lock().unwrap().last().unwrap().id;
    serve(f, id, false);
    id
}
fn serve(f: &mut Fixture, id: InstanceId, fail_name: bool) {
    for _ in 0..30 {
        let pending = f.world.take(id);
        if pending.is_empty() {
            return;
        }
        for line in pending {
            f.wb.engine_input(id, EngineInput::Stdout(String::new()));
            let (n, command) = line.split_once(' ').unwrap();
            let word = command.split_whitespace().next().unwrap();
            let text=match word {"name"=>"KataGo","version"=>"1.18.2","list_commands"=>"name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nset_position\nkata-analyze\nkata-set-rules\nkata-get-rules\nstop","kata-get-rules"=>r#"{"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N","friendlyPassOk":true,"hasButton":false}"#,_=>""};
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
#[test]
fn preparation_is_transactional_and_success_persists_settings() {
    let mut f = fixture(true);
    f.wb.edit(EditCommand::Play {
        point: Point { x: 0, y: 0 },
        color: None,
    })
    .unwrap();
    let old = f.wb.document().id;
    let s = HumanSlTrainingSettings {
        komi: 0.5,
        handicap: 2,
        ..settings(PlayerColor::White)
    };
    let id = start(&mut f, s, false);
    assert_eq!(f.wb.document().id, old);
    assert_eq!(f.wb.inspect().human_sl.unwrap().phase, HumanSlPhase::Preparing);
    ready(&mut f, id);
    let view = f.wb.inspect();
    assert_ne!(view.document.id, old);
    assert_eq!(view.document.info.komi, 0.5);
    assert_eq!(view.document.info.handicap, 2);
    assert_eq!(view.position.stones.iter().flatten().count(), 2);
    assert_eq!(view.position.to_play, Stone::White);
    assert!(view.human_sl.unwrap().human_turn);
    assert!(f.wb.config().human_sl.advanced_visible);
    let storage = f.wb.storage().clone();
    let reopened = Workbench::new(storage, None, Box::new(World::default()), vec![]);
    assert_eq!(reopened.config().human_sl.training.handicap, 2);
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
fn stale_ai_result_ends_session_without_moving_the_navigated_position() {
    let mut f = fixture(true);
    let id = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, id);
    f.wb.edit(EditCommand::Play {
        point: Point { x: 0, y: 0 },
        color: None,
    })
    .unwrap();
    let q = f.world.query(id);
    f.wb.navigate(Navigation::First);
    let before = f.wb.cursor();
    out(
        &mut f.wb,
        id,
        json!({"id":q["id"],"humanPolicy":{"D4":1.0},"moveInfos":[]}),
    );
    assert_eq!(f.wb.cursor(), before);
    assert_eq!(before, ROOT);
    assert!(f.wb.inspect().human_sl.is_none());
    assert!(f.world.dead(id));
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
fn ai_pass_after_move_200_is_committed_and_announced() {
    let mut f = fixture(true);
    for _ in 0..200 {
        f.wb.edit(EditCommand::Pass).unwrap();
    }
    let id = start(&mut f, settings(PlayerColor::White), true);
    ready(&mut f, id);
    answer(&mut f, id, "pass");
    let view = f.wb.inspect();
    assert_eq!(view.position.move_number, 201);
    assert!(view.human_sl_notices.contains(&HumanSlNotice::AiPassed));
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
    let mut f = fixture(true);
    let fg = foreground(&mut f);
    let mut s = f.wb.config().analysis_engine.clone();
    s.reuse_current_engine = true;
    let command = s.command.clone();
    f.wb.save_analysis_engine_settings(s, &command, false).unwrap().unwrap();
    let id = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, id);
    f.wb.set_auto_quick_analyze(true).unwrap();
    f.wb.engine_tick(Instant::now() + Duration::from_secs(1));
    serve(&mut f, fg, false);
    assert_eq!(f.wb.inspect().engine.lease, LeaseAvailabilityView::HumanSlGame);
    assert_eq!(
        f.world
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.args.first().is_some_and(|s| s == "analysis") && !p.dead)
            .count(),
        1
    );
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

#[test]
fn opponent_profiles_and_verification_budgets_reach_the_real_protocol() {
    for (opponent, dan_rank, rank, profile, visits, symmetries) in [
        (settings::OpponentPreset::Rank, true, 3, "rank_3d", 64, 1),
        (settings::OpponentPreset::Rank, false, 20, "rank_20k", 64, 1),
        (settings::OpponentPreset::ModernPro, true, 3, "proyear_2023", 128, 2),
        (settings::OpponentPreset::Online9d, true, 3, "rank_9d", 64, 1),
    ] {
        let mut f = fixture(true);
        let id = start(
            &mut f,
            HumanSlTrainingSettings {
                opponent,
                dan_rank,
                rank,
                ..settings(PlayerColor::White)
            },
            false,
        );
        let q = f.world.query(id);
        assert_eq!(q["overrideSettings"]["humanSLProfile"], profile);
        assert_eq!(q["maxVisits"], 1);
        out(&mut f.wb, id, json!({"id":q["id"],"humanPolicy":{"D4":1.0}}));
        let q = f.world.query(id);
        assert_eq!(q["overrideSettings"]["humanSLProfile"], profile);
        assert_eq!(q["overrideSettings"]["rootNumSymmetriesToSample"], symmetries);
        out(
            &mut f.wb,
            id,
            json!({"id":q["id"],"humanPolicy":{"D4":1.0},"policy":[],"moveInfos":[{"move":"D4","order":0,"utility":0.0}]}),
        );
        let verify = f.world.query(id);
        assert_eq!(verify["maxVisits"], visits);
        assert_eq!(verify["allowMoves"][0]["moves"], json!(["D4"]));
        assert_eq!(verify["overrideSettings"]["humanSLCpuctPermanent"], 2.0);
        assert!(verify["overrideSettings"]["maxTime"].as_f64().unwrap() > 0.0);
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
    assert_eq!(query["overrideSettings"]["humanSLProfile"], "rank_3d");
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
        assert_eq!(query["overrideSettings"]["humanSLProfile"], "rank_3d");
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

#[test]
fn returning_to_training_start_always_creates_a_fresh_variation() {
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
        let first_training_child = f.wb.cursor();
        answer(&mut f, worker, "Q16");
        f.wb.navigate(Navigation::First);
        f.wb.edit(original_move).unwrap();
        let replayed = f.wb.cursor();
        assert_ne!(replayed, original);
        assert_ne!(replayed, first_training_child);
        assert_eq!(f.wb.document().record.node(ROOT).children.len(), 3);
        assert_eq!(f.wb.document().record.node(ROOT).children[0], original);
        assert!(f.wb.document().record.node(original).children.is_empty());
    }
}

#[test]
fn navigating_away_and_back_invalidates_the_in_flight_ai_request() {
    let mut f = fixture(true);
    let worker = start(&mut f, settings(PlayerColor::Black), false);
    ready(&mut f, worker);
    f.wb.edit(EditCommand::Play {
        point: Point { x: 0, y: 0 },
        color: None,
    })
    .unwrap();
    let requested_node = f.wb.cursor();
    let revision = f.wb.document().revision;
    let query = f.world.query(worker);
    assert!(f.wb.navigate(Navigation::First));
    assert!(f.wb.navigate(Navigation::Forward(1)));
    assert_eq!(f.wb.cursor(), requested_node);
    out(
        &mut f.wb,
        worker,
        json!({"id": query["id"], "humanPolicy": {"D4": 1.0},
            "moveInfos": [{"move": "D4", "order": 0, "utility": 0.0}]}),
    );
    answer(&mut f, worker, "D4");
    assert!(f.wb.inspect().human_sl.is_none());
    assert_eq!(f.wb.cursor(), requested_node);
    assert_eq!(f.wb.document().revision, revision);
    assert!(f.wb.document().record.node(requested_node).children.is_empty());
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

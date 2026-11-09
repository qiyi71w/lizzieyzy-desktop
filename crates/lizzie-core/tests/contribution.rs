//! 跑谱贡献 through the public `Workbench` interface, with a controlled worker process: the
//! command line, refusals, the watched game built from KataGo's JSON stream, retries, pause/slow/
//! force stop, saving and the gates.

use std::path::Path;
use std::sync::{Arc, Mutex};

use lizzie_core::api::{
    Command, CommandResult, ContributionCommand, ContributionNotice, ContributionPhase,
    EditCommand, GameCommand, SavedEngineKind, SavedEngineReason,
};
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineSettings};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::record::{Point, Stone};
use lizzie_core::settings::{ContributionSettings, Storage};
use lizzie_core::workbench::Workbench;
use serde_json::{json, Value};

const CONTRIBUTING: &str = "Contribute.tips.contributingAndStartAnotherLizzieYzy";

#[derive(Default)]
struct Proc {
    program: String,
    args: Vec<String>,
    /// The command line handed to the SSH host, for a remote launch.
    remote: Option<String>,
    sent: Vec<String>,
    terminated: bool,
}

#[derive(Clone, Default)]
struct World(Arc<Mutex<Vec<(InstanceId, Proc)>>>);

impl World {
    fn launched(&self) -> usize {
        self.0.lock().unwrap().len()
    }
    fn last(&self) -> (InstanceId, Proc) {
        let world = self.0.lock().unwrap();
        let (id, proc) = world.last().unwrap();
        (
            *id,
            Proc {
                program: proc.program.clone(),
                args: proc.args.clone(),
                remote: proc.remote.clone(),
                sent: proc.sent.clone(),
                terminated: proc.terminated,
            },
        )
    }
    fn sent(&self, id: InstanceId) -> Vec<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .find(|(instance, _)| *instance == id)
            .map(|(_, proc)| proc.sent.clone())
            .unwrap_or_default()
    }
    fn terminated(&self, id: InstanceId) -> bool {
        self.0
            .lock()
            .unwrap()
            .iter()
            .any(|(instance, proc)| *instance == id && proc.terminated)
    }
}

struct Io {
    id: InstanceId,
    world: World,
}

impl EngineIo for Io {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        let mut world = self.world.0.lock().unwrap();
        world
            .iter_mut()
            .find(|(id, _)| *id == self.id)
            .unwrap()
            .1
            .sent
            .push(line.to_string());
        Ok(())
    }
    fn terminate(self: Box<Self>) {
        let mut world = self.world.0.lock().unwrap();
        world
            .iter_mut()
            .find(|(id, _)| *id == self.id)
            .unwrap()
            .1
            .terminated = true;
    }
}

struct FakeLauncher(World);

impl Launcher for FakeLauncher {
    fn launch(&self, id: InstanceId, spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        self.0 .0.lock().unwrap().push((
            id,
            Proc {
                program: spec.program.clone(),
                args: spec.args.clone(),
                remote: spec.remote.as_ref().map(|remote| remote.command.clone()),
                ..Proc::default()
            },
        ));
        Ok(Box::new(Io {
            id,
            world: self.0.clone(),
        }))
    }
}

struct Fixture {
    wb: Workbench,
    world: World,
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let mut wb = Workbench::new(
            Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data")),
            Some("zh-CN"),
            Box::new(FakeLauncher(world.clone())),
            vec![dir.path().to_path_buf()],
        );
        wb.set_auto_quick_analyze(false).unwrap();
        Fixture { wb, world, dir }
    }

    fn settings(engine_path: &str) -> ContributionSettings {
        ContributionSettings {
            engine_path: engine_path.into(),
            user_name: "alice".into(),
            batch_games: "6".into(),
            ..ContributionSettings::default()
        }
    }

    fn configure(&mut self, settings: ContributionSettings) {
        assert_eq!(
            self.wb
                .contribution(ContributionCommand::SaveSettings { settings })
                .unwrap(),
            CommandResult::Done
        );
    }

    fn start(&mut self) -> InstanceId {
        assert_eq!(
            self.wb.contribution_start(Some("s3cret".into()), None).unwrap(),
            CommandResult::Done
        );
        self.world.last().0
    }

    fn stdout(&mut self, id: InstanceId, line: &str) {
        self.wb
            .engine_input(id, EngineInput::Stdout(line.to_string()));
    }

    fn stderr(&mut self, id: InstanceId, line: &str) {
        self.wb
            .engine_input(id, EngineInput::Stderr(line.to_string()));
    }

    fn exited(&mut self, id: InstanceId) {
        self.wb.engine_input(id, EngineInput::Exited(Some(1)));
    }

    fn view(&self) -> lizzie_core::api::WorkbenchView {
        self.wb.inspect()
    }

    fn contribution(&self) -> lizzie_core::api::ContributionView {
        self.view().contribution.expect("a contribution session")
    }
}

fn snapshot(
    game_id: &str,
    history: Value,
    mv: Value,
    winrate: f64,
    score_lead: f64,
    ownership: Option<Value>,
) -> String {
    let mv_name = mv[1].as_str().unwrap();
    let mut json = json!({
        "gameId": game_id,
        "blackPlayer": "kata1-b18c384nbt-s1-d2",
        "whitePlayer": "kata1-b18c384nbt-s1-d2",
        "boardXSize": 19,
        "boardYSize": 19,
        "rules": {"komi": 7.5, "scoring": "AREA", "ko": "POSITIONAL", "suicide": false,
                  "tax": "NONE", "whiteHandicapBonus": "N", "hasButton": false},
        "initialStones": [],
        "moves": history,
        "move": mv,
        "moveInfos": [{
            "move": mv_name,
            "visits": 120,
            "winrate": winrate,
            "scoreLead": score_lead,
            "scoreStdev": 4.0,
            "prior": 0.42,
            "lcb": 0.51,
            "order": 0,
            "pv": [mv_name, "Q3"],
            "pvVisits": [100, 20]
        }]
    });
    if let Some(ownership) = ownership {
        json["ownership"] = ownership;
    }
    json.to_string()
}

/// The main line through first children, from the start.
fn line_moves(wb: &mut Workbench) -> Vec<(Stone, Option<Point>)> {
    wb.navigate(Navigation::First);
    let mut moves = Vec::new();
    loop {
        let view = wb.inspect();
        if let Some(last) = view.position.last_action {
            moves.push((last.color, last.point));
        }
        if view.position.node == view.position.line_end {
            break;
        }
        if !wb.navigate(Navigation::Forward(1)) {
            break;
        }
    }
    moves
}

fn failure(view: &lizzie_core::api::WorkbenchView) -> Option<lizzie_core::api::EngineFailureView> {
    view.contribution_notices
        .iter()
        .find_map(|notice| match notice {
            ContributionNotice::Failed { failure } => Some(failure.as_ref().clone()),
            _ => None,
        })
}

#[test]
fn a_start_without_a_user_name_opens_the_settings_and_changes_nothing() {
    let mut fixture = Fixture::new();
    let mut settings = Fixture::settings("/opt/katago");
    settings.user_name.clear();
    fixture.configure(settings);
    let before = fixture.view().document;
    assert_eq!(
        fixture.wb.contribution_start(Some("x".into()), None).unwrap(),
        CommandResult::Done
    );
    let view = fixture.view();
    assert_eq!(view.document.id, before.id);
    assert!(view.contribution.is_none());
    assert!(matches!(
        view.contribution_notices.as_slice(),
        [ContributionNotice::OpenSettings { key }] if key == "Contribute.tips.noUserName"
    ));
    assert_eq!(fixture.world.launched(), 0);
}

#[test]
fn a_start_without_an_engine_opens_the_settings_and_changes_nothing() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings(""));
    let before = fixture.view().document;
    assert_eq!(
        fixture.wb.contribution_start(None, None).unwrap(),
        CommandResult::Done
    );
    let view = fixture.view();
    assert_eq!(view.document.id, before.id);
    assert!(view.contribution.is_none());
    assert!(matches!(
        view.contribution_notices.as_slice(),
        [ContributionNotice::OpenSettings { key }] if key == "Contribute.tips.noEnginePath"
    ));
    assert_eq!(fixture.world.launched(), 0);
}

#[test]
fn a_second_start_is_refused_while_the_first_runs() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    let (id, _) = fixture.start_pair();
    let document = fixture.view().document.id;
    assert_eq!(
        fixture.wb.contribution_start(None, None).unwrap(),
        CommandResult::Refused {
            message_key: "Contribute.tips.alreadyTraining".into()
        }
    );
    assert_eq!(fixture.view().document.id, document);
    assert_eq!(fixture.world.launched(), 1);
    assert!(!fixture.world.terminated(id));
}

impl Fixture {
    fn start_pair(&mut self) -> (InstanceId, Proc) {
        assert_eq!(
            self.wb.contribution_start(Some("s3cret".into()), None).unwrap(),
            CommandResult::Done
        );
        self.world.last()
    }
}

/// Java joins the quoted `"key=value",` pieces into the one argument after `-override-config`
/// (`Utils.splitCommand` concatenates adjacent quoted runs).
const OVERRIDES: &str = "serverUrl=https://katagotraining.org/,username=alice,password=s3cret,\
                         maxSimultaneousGames=6,includeOwnership=false,logGamesAsJson=true";

#[test]
fn the_plain_command_is_built_like_java_and_the_password_is_masked() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    let (id, proc) = fixture.start_pair();
    assert_eq!(proc.program, "/opt/katago");
    assert_eq!(proc.args, vec!["contribute", "-override-config", OVERRIDES]);
    // No view shows the password.
    assert!(!serde_json::to_string(&fixture.view()).unwrap().contains("s3cret"));
    // The failure window masks the command line (a permanent failure ends without a retry).
    fixture.stdout(id, "Unexpected error: Compile with -DBUILD_DISTRIBUTED=1");
    let view = fixture.view();
    let failure = failure(&view).expect("a failure notice");
    assert!(!failure.command.contains("s3cret"));
    assert!(!serde_json::to_string(&view).unwrap().contains("s3cret"));
}

#[test]
fn a_config_file_replaces_the_server_override() {
    let mut fixture = Fixture::new();
    let settings = ContributionSettings {
        config_path: "/tmp/contribute.cfg".into(),
        ..Fixture::settings("/opt/katago")
    };
    fixture.configure(settings);
    let (_, proc) = fixture.start_pair();
    let args = proc.args;
    assert!(args.contains(&"-config".to_string()));
    assert!(args.contains(&"/tmp/contribute.cfg".to_string()));
    assert!(!args.iter().any(|arg| arg.starts_with("serverUrl=")));
}

#[test]
fn a_pure_custom_command_gets_no_overrides() {
    let mut fixture = Fixture::new();
    let settings = ContributionSettings {
        use_command: true,
        use_pure_command: true,
        command: "my-katago contribute -config /tmp/x.cfg".into(),
        ..Fixture::settings("/opt/katago")
    };
    fixture.configure(settings);
    let (_, proc) = fixture.start_pair();
    assert_eq!(proc.program, "my-katago");
    assert_eq!(proc.args, vec!["contribute", "-config", "/tmp/x.cfg"]);
}

#[test]
fn a_custom_command_gets_the_missing_overrides() {
    let mut fixture = Fixture::new();
    let settings = ContributionSettings {
        use_command: true,
        command: "my-katago contribute".into(),
        ..Fixture::settings("/opt/katago")
    };
    fixture.configure(settings);
    let (_, proc) = fixture.start_pair();
    assert_eq!(proc.args, vec!["contribute", "-override-config", OVERRIDES]);
}

#[test]
fn a_remote_custom_command_runs_over_ssh() {
    let mut fixture = Fixture::new();
    let settings = ContributionSettings {
        use_command: true,
        remote: true,
        ssh: lizzie_core::engine::ssh::SshSettings {
            host: "example.org".into(),
            port: "2222".into(),
            user: "bob".into(),
            ..Default::default()
        },
        ..Fixture::settings("/opt/katago")
    };
    fixture.configure(settings);
    let (_, proc) = fixture.start_pair();
    let remote = proc.remote.expect("the command runs on the SSH host");
    // The remote shell gets Java's quoted overrides as written (" -override-config " then
    // " \"serverUrl=…\"," for a custom command).
    assert!(remote.ends_with(
        "-override-config  \"serverUrl=https://katagotraining.org/\",\"username=alice\",\
         \"password=s3cret\",\"maxSimultaneousGames=6\",\"includeOwnership=false\",\
         \"logGamesAsJson=true\""
    ), "{remote}");
}

#[test]
fn json_snapshots_build_the_watched_game_and_hold_the_last_move() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    let id = fixture.start();
    fixture.stdout(
        id,
        &snapshot(
            "g1",
            json!([]),
            json!(["B", "Q16"]),
            0.6,
            1.5,
            Some(json!([0.4, -0.4])),
        ),
    );
    let view = fixture.view();
    let session = view.contribution.clone().unwrap();
    assert_eq!(session.phase, ContributionPhase::Running);
    assert!(session.active);
    assert_eq!(session.watching, 1);
    assert_eq!(session.total, 1);
    let game = session.game.unwrap();
    assert_eq!(game.black, "b18c384nbt-s1");
    assert_eq!(game.white, "b18c384nbt-s1");
    assert!(!game.rating);
    assert_eq!(game.komi, 7.5);
    assert!(!game.complete);
    assert!(game.rules_text.contains('\r'));
    assert_eq!(view.document.info.black, "b18c384nbt-s1");
    assert_eq!(view.document.info.komi, 7.5);
    let root = view.analysis.as_ref().expect("the first position's search");
    assert_eq!(root.to_play, Stone::Black);
    assert_eq!(root.candidates[0].point, Some(Point { x: 15, y: 3 }));
    assert!((root.winrate - 60.0).abs() < 1e-6);
    // One move placed; the cursor stays at the first analysed node (the root).
    assert_eq!(view.position.node, 0);

    // The second turn: White's analysis of the position after Q16, its move held back.
    fixture.stdout(
        id,
        &snapshot("g1", json!([["B", "Q16"]]), json!(["W", "D4"]), 0.6, 2.0, None),
    );
    let view = fixture.view();
    assert_eq!(view.position.node, 0, "the user's cursor is not moved");
    assert_eq!(
        line_moves(&mut fixture.wb),
        vec![(Stone::Black, Some(Point { x: 15, y: 3 }))]
    );
    // The held position's numbers are White's.
    fixture.wb.navigate(Navigation::First);
    assert!(fixture.wb.navigate(Navigation::Forward(1)));
    let analysis = fixture.view().analysis.expect("the held analysis");
    assert_eq!(analysis.to_play, Stone::White);
    assert!((analysis.winrate - 40.0).abs() < 1e-6);
    assert_eq!(analysis.score_lead, Some(-2.0));
    assert!((analysis.candidates[0].prior - 42.0).abs() < 1e-6);
    assert_eq!(
        analysis.candidates[0].pv,
        vec![Some(Point { x: 3, y: 15 }), Some(Point { x: 15, y: 16 })]
    );
    assert_eq!(analysis.candidates[0].visits, 120);
    assert_eq!(analysis.candidates[0].allocation_visits, 120);
}

#[test]
fn a_finished_game_reads_its_result_from_the_working_directory() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    let id = fixture.start();
    fixture.stdout(
        id,
        &snapshot("ABC123", json!([]), json!(["B", "Q16"]), 0.6, 1.5, None),
    );
    let sgf = fixture
        .dir
        .path()
        .join("data/contribution/kata_contribute/kata1/sgfs/ABC123.sgf");
    std::fs::create_dir_all(sgf.parent().unwrap()).unwrap();
    std::fs::write(&sgf, "(;GM[1]KM[7.5]PB[Black]PW[White]RE[B+R]SZ[19])").unwrap();
    fixture.stdout(
        id,
        "2022-03-14 09:00:41+0800: Finished game 1 (training), uploaded sgf \
         kata_contribute/kata1/sgfs/ABC123.sgf and training data x (1 rows)",
    );
    // The result file is read off the workbench; the game is complete once its result is in.
    assert_eq!(fixture.contribution().finished, 0);
    for (ticket, path) in fixture.wb.take_contribution_reads() {
        assert_eq!(path, sgf);
        assert!(fixture.wb.complete_contribution_read(ticket, std::fs::read_to_string(&path).ok()));
    }
    let session = fixture.contribution();
    assert_eq!(session.finished, 1);
    let game = session.game.unwrap();
    assert!(game.complete);
    assert_eq!(game.result, "B+R");
    // The held last move is played when the game completes, and the result reaches the record.
    let view = fixture.view();
    assert_eq!(view.document.info.result, "B+R");
    assert_eq!(
        line_moves(&mut fixture.wb),
        vec![(Stone::Black, Some(Point { x: 15, y: 3 }))]
    );
}

#[test]
fn an_abnormal_exit_relaunches_and_ca_gets_the_bundle() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    let id = fixture.start();
    fixture.stderr(id, "Could not find CA certs");
    fixture.exited(id);
    assert_eq!(fixture.world.launched(), 2);
    let (_, proc) = fixture.world.last();
    let args = proc.args;
    assert!(args.contains(&"-cacerts".to_string()), "{args:?}");
    assert!(args.iter().any(|arg| arg.ends_with("cacert.pem")), "{args:?}");
    let session = fixture.contribution();
    assert_eq!(session.phase, ContributionPhase::Starting);
    assert_eq!(session.tip.key, "ContributeView.lblTip");
}

#[test]
fn the_retry_budget_ends_with_a_failure_notice() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    let mut id = fixture.start();
    for _ in 0..3 {
        fixture.exited(id);
        id = fixture.world.last().0;
    }
    fixture.exited(id);
    assert_eq!(fixture.world.launched(), 4, "four launches at most");
    let view = fixture.view();
    let session = view.contribution.as_ref().expect("the ended session");
    assert_eq!(session.phase, ContributionPhase::Exited);
    assert!(!session.active);
    let failure = failure(&view).expect("a failure notice");
    assert!(failure.during_startup);
    assert!(!failure.command.contains("s3cret"));
}

#[test]
fn a_permanent_failure_ends_without_a_retry() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    let id = fixture.start();
    fixture.stdout(
        id,
        "This version of KataGo was NOT compiled with support for distributed training. \
         Compile with -DBUILD_DISTRIBUTED=1",
    );
    assert_eq!(fixture.world.launched(), 1);
    assert!(fixture.world.terminated(id));
    let view = fixture.view();
    assert_eq!(view.contribution.as_ref().unwrap().phase, ContributionPhase::Exited);
    assert!(failure(&view).is_some());
}

#[test]
fn pause_is_throttled_and_slow_quit_repeats_are_refused() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    let id = fixture.start();
    assert_eq!(
        fixture
            .wb
            .contribution(ContributionCommand::PauseResume)
            .unwrap(),
        CommandResult::Done
    );
    let session = fixture.contribution();
    assert!(session.paused);
    assert!(!session.active);
    assert_eq!(fixture.world.sent(id), vec!["pause"]);
    assert_eq!(
        fixture
            .wb
            .contribution(ContributionCommand::PauseResume)
            .unwrap(),
        CommandResult::Refused {
            message_key: "Contribute.pauseResumeTooFrequently".into()
        }
    );
    // 顺序结束 resumes first, then sends quit; a second press is refused.
    assert_eq!(
        fixture
            .wb
            .contribution(ContributionCommand::SlowQuit)
            .unwrap(),
        CommandResult::Done
    );
    assert_eq!(fixture.world.sent(id), vec!["pause", "resume", "quit"]);
    let session = fixture.contribution();
    assert!(session.slow_quitting);
    assert!(!session.paused);
    assert_eq!(
        fixture
            .wb
            .contribution(ContributionCommand::SlowQuit)
            .unwrap(),
        CommandResult::Refused {
            message_key: "Contribute.slowQuit.repeatedly".into()
        }
    );
}

#[test]
fn a_clean_slow_exit_ends_and_close_view_removes_the_window() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    let id = fixture.start();
    assert_eq!(
        fixture
            .wb
            .contribution(ContributionCommand::SlowQuit)
            .unwrap(),
        CommandResult::Done
    );
    fixture.stderr(id, "Exited cleanly after signal. All cleaned up, quitting");
    fixture.exited(id);
    let view = fixture.view();
    let session = view.contribution.clone().unwrap();
    assert_eq!(session.phase, ContributionPhase::Exited);
    assert!(view.contribution_notices.iter().any(|notice| matches!(
        notice,
        ContributionNotice::Message { key } if key == "Contribute.tips.exitedAfterSignal"
    )));
    assert!(failure(&view).is_none(), "a clean end is not a failure");
    assert_eq!(
        fixture
            .wb
            .contribution(ContributionCommand::CloseView)
            .unwrap(),
        CommandResult::Done
    );
    assert!(fixture.view().contribution.is_none());
}

#[test]
fn closing_a_running_window_is_refused() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    fixture.start();
    assert_eq!(
        fixture
            .wb
            .contribution(ContributionCommand::CloseView)
            .unwrap(),
        CommandResult::Refused {
            message_key: "ContributeView.closeTip".into()
        }
    );
}

#[test]
fn force_quit_sends_forcequit_terminates_and_leaves_no_window() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    let id = fixture.start();
    assert_eq!(
        fixture
            .wb
            .contribution(ContributionCommand::ForceQuit)
            .unwrap(),
        CommandResult::Done
    );
    assert_eq!(fixture.world.sent(id), vec!["forcequit"]);
    assert!(fixture.world.terminated(id));
    assert!(fixture.view().contribution.is_none());
}

#[test]
fn the_board_is_read_only_while_contributing_and_free_when_paused() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    fixture.start();
    let revision = fixture.view().document.revision;
    assert_eq!(
        fixture
            .wb
            .edit(EditCommand::Play {
                point: Point { x: 3, y: 3 },
                color: None
            })
            .unwrap(),
        CommandResult::Unchanged
    );
    assert_eq!(fixture.view().document.revision, revision);
    for command in [
        Command::SwitchEngine { index: 0 },
        Command::Game {
            op: GameCommand::Pause,
        },
        Command::Tuning {
            op: lizzie_core::api::TuningCommand::StartBenchmark {
                engine_id: "e".into(),
                experimental: false,
            },
        },
    ] {
        match fixture.wb.game_gate(&command) {
            Some(CommandResult::Refused { message_key }) => assert_eq!(message_key, CONTRIBUTING),
            other => panic!("{command:?} was not refused: {other:?}"),
        }
    }
    // Comments and marks still go through.
    assert_eq!(
        fixture.wb.game_gate(&Command::Edit {
            op: EditCommand::ClearMarks
        }),
        None
    );
    // Paused: the gates are lifted.
    let _ = fixture.wb.contribution(ContributionCommand::PauseResume);
    assert_ne!(
        fixture.wb.game_gate(&Command::Game {
            op: GameCommand::Pause
        }),
        Some(CommandResult::Refused {
            message_key: CONTRIBUTING.into()
        })
    );
    assert_eq!(
        fixture
            .wb
            .edit(EditCommand::Play {
                point: Point { x: 3, y: 3 },
                color: None
            })
            .unwrap(),
        CommandResult::Done
    );
}

#[test]
fn saved_engines_report_their_local_executable_and_quoting() {
    let mut fixture = Fixture::new();
    let executable = fixture.dir.path().join("a b").join("katago");
    std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
    std::fs::write(&executable, b"x").unwrap();
    let missing = fixture.dir.path().join("missing-katago");
    let engines = vec![
        EngineProfile {
            name: "Local".into(),
            command: format!("\"{}\" gtp", executable.display()),
            ..EngineProfile::default()
        },
        EngineProfile {
            name: "Gone".into(),
            command: format!("{} gtp", missing.display()),
            ..EngineProfile::default()
        },
        EngineProfile {
            name: "Remote".into(),
            command: "katago gtp".into(),
            ssh: Some(Default::default()),
            ..EngineProfile::default()
        },
    ];
    fixture
        .wb
        .save_engine_settings(engines, EngineSettings::default().startup)
        .unwrap();
    let CommandResult::ContributionSavedEngines { candidates } = fixture
        .wb
        .contribution(ContributionCommand::SavedEngines)
        .unwrap()
    else {
        panic!("saved engine candidates");
    };
    assert_eq!(candidates.len(), 3);
    assert_eq!(candidates[0].index, 1);
    assert_eq!(candidates[0].name, "Local");
    assert_eq!(candidates[0].kind, SavedEngineKind::Local);
    let resolved = candidates[0].executable.clone().expect("a local file");
    assert_eq!(candidates[0].field_text, Some(format!("\"{resolved}\"")));
    assert_eq!(candidates[1].kind, SavedEngineKind::Local);
    assert_eq!(candidates[1].reason, Some(SavedEngineReason::MissingFile));
    assert_eq!(candidates[2].kind, SavedEngineKind::Remote);
    assert_eq!(
        candidates[2].reason,
        Some(SavedEngineReason::RemoteTransport)
    );
}

#[test]
fn save_all_games_writes_the_folder_and_refuses_with_none() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    let id = fixture.start();
    assert_eq!(
        fixture
            .wb
            .contribution(ContributionCommand::SaveAllGames)
            .unwrap(),
        CommandResult::Refused {
            message_key: "Contribute.tips.noGameToSave".into()
        }
    );
    fixture.stdout(
        id,
        &snapshot("ABC123", json!([]), json!(["B", "Q16"]), 0.6, 1.5, None),
    );
    let sgf = fixture
        .dir
        .path()
        .join("data/contribution/kata_contribute/kata1/sgfs/ABC123.sgf");
    std::fs::create_dir_all(sgf.parent().unwrap()).unwrap();
    std::fs::write(&sgf, "(;RE[B+R]SZ[19])").unwrap();
    fixture.stdout(
        id,
        "Finished game 1 (training), uploaded sgf kata_contribute/kata1/sgfs/ABC123.sgf \
         and training data x",
    );
    // The result file is read off the workbench, like a batch record.
    let reads = fixture.wb.take_contribution_reads();
    assert_eq!(reads.len(), 1);
    for (ticket, path) in reads {
        assert_eq!(path, sgf);
        assert!(fixture.wb.complete_contribution_read(ticket, std::fs::read_to_string(&path).ok()));
    }
    assert_eq!(
        fixture
            .wb
            .contribution(ContributionCommand::SaveAllGames)
            .unwrap(),
        CommandResult::Done
    );
    // The records go to the serial writer outside the workbench, like every other save; the
    // application waits for them before it exits.
    let jobs = fixture.wb.take_background_saves();
    assert_eq!(jobs.len(), 1);
    assert!(fixture.wb.has_pending_saves());
    assert!(!jobs[0].path.exists(), "nothing is written under the workbench");
    for job in &jobs {
        assert!(job.create_parent);
        std::fs::create_dir_all(job.path.parent().unwrap()).unwrap();
        std::fs::write(&job.path, &job.text).unwrap();
        assert!(fixture.wb.complete_save(job.ticket, Ok(())));
    }
    assert!(!fixture.wb.has_pending_saves());
    let view = fixture.view();
    let folder = view
        .contribution_notices
        .iter()
        .find_map(|notice| match notice {
            ContributionNotice::Saved { folder } => Some(folder.clone()),
            _ => None,
        })
        .expect("a saved notice");
    let folder = Path::new(&folder);
    assert_eq!(jobs[0].path.parent(), Some(folder));
    let saved: Vec<_> = std::fs::read_dir(folder)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    assert_eq!(saved.len(), 1);
    let name = saved[0].file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.contains("ABC123"), "{name}");
    assert!(name.contains("(B+R)"), "{name}");
    let text = std::fs::read_to_string(&saved[0]).unwrap();
    assert!(text.contains("RE[B+R]"), "{text}");
    assert!(text.contains("SZ[19]"));
}

#[test]
fn the_application_exit_ends_the_contribution_process() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    let id = fixture.start();
    fixture.wb.end_engines();
    assert_eq!(fixture.world.sent(id), vec!["forcequit"]);
    assert!(fixture.world.terminated(id));
    assert!(fixture.view().contribution.is_none());
}

fn sized(line: String, size: u16) -> String {
    let mut json: Value = serde_json::from_str(&line).unwrap();
    json["boardXSize"] = json!(size);
    json["boardYSize"] = json!(size);
    json.to_string()
}

/// Java `getJsonGameInfo`: a game that starts while another is watched only joins the list; the
/// board stays on the watched 19×19 game while a 9×9 game runs alongside.
#[test]
fn a_new_game_of_another_size_joins_the_list_without_taking_the_board() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    let id = fixture.start();
    fixture.stdout(id, &snapshot("g19", json!([]), json!(["B", "Q16"]), 0.5, 0.5, None));
    fixture.stdout(id, &sized(snapshot("g9", json!([]), json!(["B", "E5"]), 0.5, 0.5, None), 9));
    fixture.stdout(id, &snapshot("g19", json!([["B", "Q16"]]), json!(["W", "D4"]), 0.5, 0.5, None));
    fixture.stdout(id, &sized(snapshot("g9", json!([["B", "E5"]]), json!(["W", "C3"]), 0.5, 0.5, None), 9));
    fixture.stdout(id, &snapshot("g19", json!([["B", "Q16"], ["W", "D4"]]), json!(["B", "Q4"]), 0.5, 0.5, None));
    let session = fixture.contribution();
    assert_eq!((session.watching, session.total), (1, 2));
    assert_eq!(fixture.view().document.size.width, 19);
    assert_eq!(
        line_moves(&mut fixture.wb),
        vec![
            (Stone::Black, Some(Point { x: 15, y: 3 })),
            (Stone::White, Some(Point { x: 3, y: 15 })),
        ]
    );
}

/// An installed package of catalog asset `asset_id` under a folder with a space in its name.
fn install_package(fixture: &Fixture, asset_id: &str) -> std::path::PathBuf {
    let directory = fixture.dir.path().join("data/resources/engines/two words");
    std::fs::create_dir_all(&directory).unwrap();
    let executable = directory.join("katago.exe");
    std::fs::write(&executable, b"exe").unwrap();
    let receipt = json!({
        "assetId": asset_id, "version": "1.18.1", "archiveSha256": "0".repeat(64),
        "executableSha256": "0".repeat(64), "executable": "katago.exe", "executableSize": 3,
        "source": "download", "installedAt": 0, "experimentalBackend": null
    });
    std::fs::write(directory.join("resource.json"), receipt.to_string()).unwrap();
    executable
}

/// Qualification follows the engine the command actually runs: the path field as the chooser
/// and 已配置引擎 write it (quoted, so a path with a space stays one program), and nothing at all
/// for a custom command.
#[test]
fn an_unsupported_package_is_refused_however_its_path_is_written_and_a_custom_command_is_not() {
    let mut fixture = Fixture::new();
    let executable = install_package(&fixture, "windows-directml");
    let path = format!("\"{}\"", executable.to_string_lossy());
    let qualification = fixture.wb.contribution(ContributionCommand::Qualify { engine_path: path.clone() }).unwrap();
    assert!(
        matches!(&qualification, CommandResult::ContributionQualification {
            qualification: lizzie_core::api::ContributionQualification::Managed { asset_id, .. }
        } if asset_id == "windows-directml"),
        "{qualification:?}"
    );
    fixture.configure(Fixture::settings(&path));
    assert_eq!(
        fixture.wb.contribution_start(Some("s3cret".into()), None).unwrap(),
        CommandResult::Refused { message_key: "contribution.resourceUnsupported".into() }
    );
    assert_eq!(fixture.world.launched(), 0);
    fixture.configure(ContributionSettings {
        use_command: true,
        command: "my-katago contribute".into(),
        ..Fixture::settings(&path)
    });
    assert_eq!(fixture.wb.contribution_start(Some("s3cret".into()), None).unwrap(), CommandResult::Done);
    assert_eq!(fixture.world.last().1.program, "my-katago");
}

/// Starting shuts every engine down (`EngineManager.isEmpty = true`), the idle preloaded
/// analysis process included.
#[test]
fn starting_ends_an_idle_preloaded_analysis_process() {
    let mut fixture = Fixture::new();
    let settings = lizzie_core::settings::AnalysisEngineSettings {
        command: "/opt/katago analysis -config /a.cfg".into(),
        preload: true,
        ..Default::default()
    };
    fixture.wb.save_analysis_engine_settings(settings, "", true).unwrap().unwrap();
    fixture.wb.preload_analysis_engine();
    let (warm, proc) = fixture.world.last();
    assert_eq!(proc.args[0], "analysis");
    fixture.configure(Fixture::settings("/opt/katago"));
    fixture.start();
    assert!(fixture.world.terminated(warm), "the preloaded process ends");
}

/// A watched game is replayed under its own rules: a multi-stone suicide the server's rules
/// allow is kept, whatever engine was in use before.
#[test]
fn a_game_allowing_suicide_keeps_its_suicide_move() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    let id = fixture.start();
    let line = |history: Value, mv: Value| {
        let mut json: Value = serde_json::from_str(&sized(snapshot("s9", history, mv, 0.5, 0.5, None), 9)).unwrap();
        json["rules"]["suicide"] = json!(true);
        json.to_string()
    };
    let moves = [["B", "A1"], ["W", "A2"], ["B", "pass"], ["W", "B2"], ["B", "pass"], ["W", "C1"], ["B", "B1"]];
    for n in 0..moves.len() {
        fixture.stdout(id, &line(json!(moves[..n]), json!(moves[n])));
    }
    let sgf = fixture.dir.path().join("data/contribution/kata_contribute/kata1/sgfs/s9.sgf");
    std::fs::create_dir_all(sgf.parent().unwrap()).unwrap();
    std::fs::write(&sgf, "(;RE[W+R]SZ[9])").unwrap();
    fixture.stdout(id, "Finished game 1 (training), uploaded sgf kata_contribute/kata1/sgfs/s9.sgf and training data x");
    deliver_reads(&mut fixture);
    let played = line_moves(&mut fixture.wb);
    assert_eq!(played.last(), Some(&(Stone::Black, Some(Point { x: 1, y: 8 }))), "{played:?}");
    let stones = fixture.view().position.stones;
    assert_eq!(stones[8 * 9], None, "A1 was taken off");
    assert_eq!(stones[8 * 9 + 1], None, "B1 was taken off");
}

/// After the process ended cleanly the kept window still plays the games (Java's watch thread
/// only stops with 强制结束).
#[test]
fn the_kept_window_still_plays_after_a_clean_exit() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    let id = fixture.start();
    let moves = [["B", "Q16"], ["W", "D4"], ["B", "Q4"], ["W", "D16"]];
    for n in 0..moves.len() {
        fixture.stdout(id, &snapshot("g1", json!(moves[..n]), json!(moves[n]), 0.5, 0.5, None));
    }
    fixture.wb.contribution(ContributionCommand::SlowQuit).unwrap();
    fixture.stderr(id, "Exited cleanly after signal. All cleaned up, quitting");
    fixture.exited(id);
    assert_eq!(fixture.contribution().phase, ContributionPhase::Exited);
    let before = fixture.view().position.node;
    let due = fixture.wb.engine_timer().expect("the watch clock keeps running");
    fixture.wb.engine_tick(due + std::time::Duration::from_secs(3));
    assert_ne!(fixture.view().position.node, before, "auto play moved on");
}

/// The failure window masks the whole password, also one KataGo's override list cannot carry.
#[test]
fn the_failure_window_masks_a_password_with_a_comma() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    fixture.wb.contribution_start(Some("FIRSTSECRET,TAILSECRET".into()), None).unwrap();
    let id = fixture.world.last().0;
    fixture.stdout(id, "Compile with -DBUILD_DISTRIBUTED=1");
    let view = fixture.view();
    let shown = serde_json::to_string(&view).unwrap();
    assert!(failure(&view).is_some());
    assert!(!shown.contains("FIRSTSECRET") && !shown.contains("TAILSECRET"), "{}", failure(&view).unwrap().command);
}

/// A password typed into a custom command is kept like the training password: the saved setting,
/// the configuration file and the projection carry a marker; the run gets the command as typed.
#[test]
fn a_password_in_a_custom_command_is_never_saved() {
    let mut fixture = Fixture::new();
    let typed = "katago contribute -override-config username=bob,password=\"UNIQUE SECRET\"";
    fixture.configure(ContributionSettings {
        use_command: true,
        use_pure_command: true,
        command: typed.into(),
        ..Fixture::settings("/opt/katago")
    });
    let saved = fixture.wb.config().contribution.command.clone();
    assert_eq!(saved, "katago contribute -override-config username=bob,password=******");
    assert!(!serde_json::to_string(&fixture.view()).unwrap().contains("SECRET"));
    for entry in std::fs::read_dir(fixture.dir.path().join("config")).unwrap() {
        let text = std::fs::read(entry.unwrap().path()).unwrap_or_default();
        assert!(!String::from_utf8_lossy(&text).contains("SECRET"));
    }
    assert_eq!(fixture.wb.contribution_start(None, Some(typed)).unwrap(), CommandResult::Done);
    let launched = &fixture.world.last().1;
    assert_eq!(launched.args.last().unwrap(), "username=bob,password=UNIQUE SECRET");
    assert!(!serde_json::to_string(&fixture.view()).unwrap().contains("SECRET"));
}

/// Writes `game_id`'s finished record (result `B+R`) and reports it finished.
fn finish(fixture: &mut Fixture, id: InstanceId, game_id: &str) {
    let sgf = fixture
        .dir
        .path()
        .join(format!("data/contribution/kata_contribute/kata1/sgfs/{game_id}.sgf"));
    std::fs::create_dir_all(sgf.parent().unwrap()).unwrap();
    std::fs::write(&sgf, "(;RE[B+R]SZ[19])").unwrap();
    fixture.stdout(
        id,
        &format!("Finished game 1 (training), uploaded sgf kata_contribute/kata1/sgfs/{game_id}.sgf and training data x"),
    );
}

fn deliver_reads(fixture: &mut Fixture) {
    for (ticket, path) in fixture.wb.take_contribution_reads() {
        fixture.wb.complete_contribution_read(ticket, std::fs::read_to_string(&path).ok());
    }
}

/// A game that finishes, or whose result comes back, after the user opened another board (the
/// contribution paused) completes but leaves that board alone.
#[test]
fn a_late_result_does_not_touch_a_board_opened_meanwhile() {
    for finished_first in [true, false] {
        let mut fixture = Fixture::new();
        fixture.configure(Fixture::settings("/opt/katago"));
        let id = fixture.start();
        fixture.stdout(id, &snapshot("A", json!([]), json!(["B", "Q16"]), 0.5, 0.5, None));
        if finished_first {
            finish(&mut fixture, id, "A");
        }
        fixture.wb.contribution(ContributionCommand::PauseResume).unwrap();
        fixture.wb.edit(EditCommand::ClearBoard).unwrap();
        if !finished_first {
            finish(&mut fixture, id, "A");
        }
        let board = serde_json::to_value(&fixture.view().document).unwrap();
        deliver_reads(&mut fixture);
        assert_eq!(serde_json::to_value(&fixture.view().document).unwrap(), board, "{finished_first}");
        let game = fixture.contribution().game.unwrap();
        assert!(game.complete);
        assert_eq!(game.result, "B+R");
    }
}

/// 自动保存 of a game opened while its result is still being read waits for the result: the
/// file is named and written with it, also when another game is watched by then.
#[test]
fn an_automatic_save_waits_for_the_result() {
    for leave in [false, true] {
        let mut fixture = Fixture::new();
        fixture.configure(ContributionSettings {
            auto_save: true,
            ..Fixture::settings("/opt/katago")
        });
        let id = fixture.start();
        fixture.stdout(id, &snapshot("A", json!([]), json!(["B", "Q16"]), 0.5, 0.5, None));
        fixture.stdout(id, &snapshot("B", json!([]), json!(["B", "D4"]), 0.5, 0.5, None));
        finish(&mut fixture, id, "B");
        fixture.wb.contribution(ContributionCommand::WatchGame { index: 1 }).unwrap();
        if leave {
            fixture.wb.contribution(ContributionCommand::WatchGame { index: 0 }).unwrap();
        }
        assert!(fixture.wb.take_background_saves().is_empty(), "nothing saved before the result");
        deliver_reads(&mut fixture);
        let jobs = fixture.wb.take_background_saves();
        assert_eq!(jobs.len(), 1, "{leave}");
        assert!(jobs[0].path.to_string_lossy().ends_with("(B+R).sgf"), "{:?}", jobs[0].path);
        assert!(jobs[0].text.contains("RE[B+R]") && jobs[0].text.contains("B[dp]"), "{}", jobs[0].text);
        let watched = if leave { 1 } else { 2 };
        assert_eq!(fixture.contribution().watching, watched);
    }
}

/// 保存所有棋谱 while a finished game's result is still being read waits for it.
#[test]
fn save_all_waits_for_a_result_being_read() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    let id = fixture.start();
    fixture.stdout(id, &snapshot("A", json!([]), json!(["B", "Q16"]), 0.5, 0.5, None));
    finish(&mut fixture, id, "A");
    assert_eq!(fixture.wb.contribution(ContributionCommand::SaveAllGames).unwrap(), CommandResult::Done);
    assert!(fixture.wb.take_background_saves().is_empty());
    deliver_reads(&mut fixture);
    let jobs = fixture.wb.take_background_saves();
    assert_eq!(jobs.len(), 1);
    assert!(jobs[0].path.to_string_lossy().ends_with("(B+R).sgf"), "{:?}", jobs[0].path);
    assert!(jobs[0].text.contains("RE[B+R]") && jobs[0].text.contains("B[pd]"), "{}", jobs[0].text);
}

/// 保存所有棋谱 whose write fails says so instead of naming the folder.
#[test]
fn a_failed_save_all_is_not_reported_as_saved() {
    let mut fixture = Fixture::new();
    fixture.configure(Fixture::settings("/opt/katago"));
    let id = fixture.start();
    fixture.stdout(id, &snapshot("A", json!([]), json!(["B", "Q16"]), 0.5, 0.5, None));
    fixture.wb.contribution(ContributionCommand::SaveAllGames).unwrap();
    for job in fixture.wb.take_background_saves() {
        assert!(fixture.wb.complete_save(job.ticket, Err("disk full".into())));
    }
    let notices = fixture.view().contribution_notices;
    assert!(!notices.iter().any(|n| matches!(n, ContributionNotice::Saved { .. })), "{notices:?}");
    assert!(notices.contains(&ContributionNotice::Message {
        key: "LizzieFrame.saveFileFailed".into()
    }));
}

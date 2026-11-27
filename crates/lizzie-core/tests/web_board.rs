//! Web 旁观 through the operations the platform and the desktop use: the share session a viewer's
//! message belongs to, a viewer's 试下 on the open record (branches, the end placeholder, who may
//! start one, what the desktop may do meanwhile) and how it ends.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lizzie_core::api::{Command, CommandResult, EditCommand, EngineScope, MatchCommand, OpenSource, WebBoardCommand};
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats::sgf;
use lizzie_core::java_import::{self, JavaSource};
use lizzie_core::record::{NodeKind, Point, RecordFormat, ROOT};
use lizzie_core::settings::{AutoAnalysisSettings, ExtraMode, Storage};
use lizzie_core::web_board::{parse_client_message, ClientMessage, TRIAL_IDLE};
use lizzie_core::workbench::{OpenOrigin, Workbench};

/// The lines one started engine process was sent.
type Sent = (InstanceId, Vec<String>);

/// What each started engine process was sent.
#[derive(Clone, Default)]
struct World(Arc<Mutex<Vec<Sent>>>);

impl World {
    fn take(&self, id: InstanceId) -> Vec<String> {
        let mut w = self.0.lock().unwrap();
        w.iter_mut()
            .find(|p| p.0 == id)
            .map(|p| std::mem::take(&mut p.1))
            .unwrap_or_default()
    }
    fn latest(&self) -> InstanceId {
        self.0.lock().unwrap().last().unwrap().0
    }
}

struct FakeIo(World, InstanceId);

impl EngineIo for FakeIo {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        let mut w = self.0 .0.lock().unwrap();
        w.iter_mut().find(|p| p.0 == self.1).unwrap().1.push(line.to_string());
        Ok(())
    }
    fn terminate(self: Box<Self>) {}
}

struct FakeLauncher(World);

impl Launcher for FakeLauncher {
    fn launch(&self, id: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        self.0 .0.lock().unwrap().push((id, Vec::new()));
        Ok(Box::new(FakeIo(self.0.clone(), id)))
    }
}

const KATA: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nset_position\nkata-analyze\nkata-set-rules\nkata-get-rules\nstop";
const RULES: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;
const SESSION: u64 = 7;

/// Answers every outstanding command as KataGo; returns the commands answered.
fn serve(wb: &mut Workbench, world: &World, id: InstanceId) -> Vec<String> {
    let mut done = Vec::new();
    loop {
        let pending = world.take(id);
        if pending.is_empty() {
            return done;
        }
        for line in pending {
            let (n, cmd) = line.split_once(' ').unwrap();
            let reply = match cmd.split(' ').next().unwrap() {
                "name" => "KataGo",
                "version" => "1.18.2",
                "list_commands" => KATA,
                "kata-get-rules" => RULES,
                _ => "",
            };
            wb.engine_input(id, EngineInput::Stdout(format!("={n} {reply}")));
            wb.engine_input(id, EngineInput::Stdout(String::new()));
            done.push(cmd.to_string());
        }
    }
}

fn open(wb: &mut Workbench, text: &str) {
    let t = wb.begin_open(OpenOrigin::File {
        path: "/x.sgf".into(),
        source: OpenSource::Drop,
    });
    assert!(wb.complete_open(t, Ok((RecordFormat::Sgf, sgf::read(text).unwrap()))));
}

fn workbench(dir: &std::path::Path, world: &World) -> Workbench {
    let mut wb = Workbench::new(
        Storage::in_dirs(&dir.join("config"), &dir.join("data")),
        None,
        Box::new(FakeLauncher(world.clone())),
        Vec::new(),
    );
    wb.set_auto_quick_analyze(false).unwrap();
    wb
}

/// A record open, a ready KataGo foreground engine and Web 旁观 running as [`SESSION`].
fn sharing(dir: &std::path::Path, text: &str) -> (Workbench, World, InstanceId) {
    let world = World::default();
    let mut wb = workbench(dir, &world);
    open(&mut wb, text);
    let profile = EngineProfile {
        name: "kata".into(),
        command: "/opt/engine gtp".into(),
        ..EngineProfile::default()
    };
    wb.save_engine_settings(vec![profile], EngineStartup::Default).unwrap();
    wb.start_engines();
    let id = world.latest();
    serve(&mut wb, &world, id);
    wb.web_board_started(SESSION, "http://192.168.1.5:9998".into());
    (wb, world, id)
}

/// [`sharing`] in 双引擎模式: engines a (foreground), b (the secondary engine) and c configured,
/// both running engines ready. Returns the secondary engine's process.
fn sharing_with_two_engines(dir: &std::path::Path, text: &str) -> (Workbench, World, InstanceId) {
    let world = World::default();
    let mut wb = workbench(dir, &world);
    open(&mut wb, text);
    let engines = ["a", "b", "c"]
        .map(|name| EngineProfile {
            name: name.into(),
            command: format!("/opt/{name} gtp"),
            ..EngineProfile::default()
        })
        .to_vec();
    wb.save_engine_settings(engines, EngineStartup::Default).unwrap();
    wb.start_engines();
    let foreground = world.latest();
    serve(&mut wb, &world, foreground);
    assert!(wb.set_extra_mode(ExtraMode::DoubleEngine).unwrap());
    assert!(wb.switch_secondary_engine(1).unwrap());
    let secondary = world.latest();
    serve(&mut wb, &world, secondary);
    serve(&mut wb, &world, foreground);
    wb.web_board_started(SESSION, "http://192.168.1.5:9998".into());
    (wb, world, secondary)
}

fn msg(wb: &mut Workbench, text: &str, now: Instant) -> Option<String> {
    let message = parse_client_message(text).expect("a message Java reads");
    wb.web_board_message(SESSION, message, now)
}

fn enter(wb: &mut Workbench, client: &str, now: Instant) -> Option<String> {
    msg(wb, &format!(r#"{{"type":"enter_trial","clientId":"{client}"}}"#), now)
}

fn trial_move(wb: &mut Workbench, client: &str, x: u16, y: u16, now: Instant) {
    assert_eq!(
        msg(
            wb,
            &format!(r#"{{"type":"trial_move","clientId":"{client}","x":{x},"y":{y}}}"#),
            now
        ),
        None
    );
}

fn exit(wb: &mut Workbench, client: &str, now: Instant) {
    assert_eq!(
        msg(wb, &format!(r#"{{"type":"exit_trial","clientId":"{client}"}}"#), now),
        None
    );
}

fn moved_to(wb: &Workbench) -> Option<Point> {
    match wb.document().record.node(wb.cursor()).kind {
        NodeKind::Move { point, .. } => Some(point),
        _ => None,
    }
}

fn trial_active(wb: &mut Workbench) -> bool {
    wb.view(true).web_board.trial_active
}

#[test]
fn a_viewer_trial_plays_side_branches_the_engine_follows_and_browsing_returns_to_the_anchor() {
    let dir = tempfile::tempdir().unwrap();
    // B[dd] W[pp] B[dp] on the main line; browsing stands after W[pp].
    let (mut wb, world, id) = sharing(dir.path(), "(;SZ[19];B[dd];W[pp];B[dp])");
    wb.navigate(Navigation::First);
    wb.navigate(Navigation::Forward(2));
    let anchor = wb.cursor();
    let revision = wb.document().revision;
    let now = Instant::now();

    assert_eq!(enter(&mut wb, "a", now), None);
    assert!(trial_active(&mut wb));
    let frame = wb.web_board_frame();
    assert!(frame.trial.contains(r#""ownerClientId":"a""#), "{}", frame.trial);

    // Not the main continuation B[dp]: a new last variation, shown on the desktop as well.
    serve(&mut wb, &world, id);
    trial_move(&mut wb, "a", 16, 2, now);
    assert_eq!(moved_to(&wb), Some(Point { x: 16, y: 2 }));
    let rec = &wb.document().record;
    assert_eq!(rec.node(anchor).children.len(), 2);
    assert_eq!(rec.main_child(anchor), Some(rec.node(anchor).children[0]));
    assert!(wb.document().revision > revision, "the variation is a real edit");
    // The foreground engine follows the 试下's position.
    assert!(
        serve(&mut wb, &world, id).iter().any(|c| c.contains("R17")),
        "engine follows the viewer"
    );

    // The main move is followed rather than duplicated.
    assert_eq!(
        msg(
            &mut wb,
            r#"{"type":"trial_navigate","clientId":"a","direction":"back"}"#,
            now
        ),
        None
    );
    assert_eq!(wb.cursor(), anchor);
    trial_move(&mut wb, "a", 3, 15, now);
    assert_eq!(wb.cursor(), wb.document().record.node(anchor).children[0]);
    assert_eq!(wb.document().record.node(anchor).children.len(), 2);

    // Back at the anchor the variation is marked and reachable by its child index.
    assert_eq!(msg(&mut wb, r#"{"type":"trial_reset","clientId":"a"}"#, now), None);
    assert_eq!(wb.cursor(), anchor);
    assert!(wb.web_board_frame().trial.contains(r#""childIndex":1"#));
    assert_eq!(
        msg(
            &mut wb,
            r#"{"type":"trial_navigate","clientId":"a","childIndex":1}"#,
            now
        ),
        None
    );
    assert_eq!(moved_to(&wb), Some(Point { x: 16, y: 2 }));

    exit(&mut wb, "a", now);
    assert!(!trial_active(&mut wb));
    assert_eq!(wb.cursor(), anchor, "browsing returns to where the 试下 started");
    assert_eq!(
        wb.document().record.node(anchor).children.len(),
        2,
        "played variations are kept"
    );
}

#[test]
fn only_one_viewer_tries_at_a_time_and_only_its_owner_acts() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, _, _) = sharing(dir.path(), "(;SZ[9];B[ee])");
    let now = Instant::now();
    assert_eq!(enter(&mut wb, "a", now), None);
    // The same viewer again (a reload) is accepted as is.
    assert_eq!(enter(&mut wb, "a", now), None);
    let denied = enter(&mut wb, "b", now).expect("a second viewer is refused");
    assert!(
        denied.contains(r#""reason":"in_use""#) && denied.contains(r#""ownerClientId":"a""#),
        "{denied}"
    );

    let at = wb.cursor();
    trial_move(&mut wb, "b", 2, 2, now);
    exit(&mut wb, "b", now);
    assert_eq!(wb.cursor(), at);
    assert!(trial_active(&mut wb), "another viewer cannot end it");
}

#[test]
fn a_trial_needs_a_free_foreground_engine() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path(), &World::default());
    open(&mut wb, "(;SZ[9];B[ee])");
    wb.web_board_started(SESSION, "http://127.0.0.1:9998".into());
    let denied = enter(&mut wb, "a", Instant::now()).expect("no engine to follow the 试下");
    assert!(denied.contains(r#""reason":"engine_busy""#), "{denied}");
    assert!(!trial_active(&mut wb));

    // Automatic analysis holds the engine as well.
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, _, _) = sharing(dir.path(), "(;SZ[9];B[ee];W[cc])");
    assert_eq!(
        wb.start_auto_analysis(AutoAnalysisSettings::default()).unwrap(),
        Ok(true)
    );
    let denied = enter(&mut wb, "a", Instant::now()).expect("the engine is in use");
    assert!(denied.contains(r#""reason":"engine_busy""#), "{denied}");
}

/// While the desktop switches to an engine that is still starting, no viewer starts a 试下 (Java's
/// switch holds the lifecycle reservation until the engine is up); when that start fails, the
/// desktop switches back and a 试下 follows the ready engine.
#[test]
fn a_trial_waits_for_an_engine_switch_and_a_failed_switch_can_be_left() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    open(&mut wb, "(;SZ[9];B[ee])");
    let profile = |id: &str, name: &str| EngineProfile {
        id: id.into(),
        name: name.into(),
        command: format!("/opt/{id} gtp"),
        ..EngineProfile::default()
    };
    let mut a = profile("a", "Local A");
    a.is_default = true;
    wb.save_engine_settings(vec![a, profile("b", "Slow B")], EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let engine_a = world.latest();
    serve(&mut wb, &world, engine_a);
    wb.web_board_started(SESSION, "http://192.168.1.5:9998".into());

    assert_eq!(wb.game_gate(&Command::SwitchEngine { index: 1 }), None);
    wb.switch_engine(1).unwrap();
    let engine_b = world.latest();
    assert_ne!(engine_b, engine_a, "B is starting");
    let denied = enter(&mut wb, "a", Instant::now()).expect("the switch has not finished");
    assert!(denied.contains(r#""reason":"engine_busy""#), "{denied}");
    assert!(!trial_active(&mut wb));

    wb.engine_input(engine_b, EngineInput::Exited(Some(1)));
    assert_eq!(wb.game_gate(&Command::SwitchEngine { index: 0 }), None);
    wb.switch_engine(0).unwrap();
    for id in [engine_a, world.latest()] {
        serve(&mut wb, &world, id);
    }
    assert_eq!(enter(&mut wb, "a", Instant::now()), None);
}

#[test]
fn a_position_without_continuation_gets_a_placeholder_kept_only_under_played_moves() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, _, _) = sharing(dir.path(), "(;SZ[9];B[ee])");
    wb.navigate(Navigation::Last);
    let anchor = wb.cursor();
    let revision = wb.document().revision;
    let now = Instant::now();

    // Entered and left without a move: nothing changed, not even the document revision.
    assert_eq!(enter(&mut wb, "a", now), None);
    assert_eq!(wb.document().record.node(anchor).children.len(), 1);
    exit(&mut wb, "a", now);
    assert!(wb.document().record.node(anchor).children.is_empty());
    assert_eq!(wb.document().revision, revision);

    // A played move stays a branch beside the placeholder, never the main line.
    assert_eq!(enter(&mut wb, "a", now), None);
    trial_move(&mut wb, "a", 2, 2, now);
    let rec = &wb.document().record;
    let children = &rec.node(anchor).children;
    assert_eq!(children.len(), 2);
    assert_eq!(rec.node(children[0]).kind, NodeKind::DummyPass);
    exit(&mut wb, "a", now);
    let rec = &wb.document().record;
    assert_eq!(
        rec.node(anchor).children.len(),
        2,
        "the placeholder keeps the move off the main line"
    );
    assert_eq!(rec.node(rec.node(anchor).children[0]).kind, NodeKind::DummyPass);
}

/// The kinds along the main line from the root (first children).
fn main_line(wb: &Workbench) -> Vec<NodeKind> {
    let rec = &wb.document().record;
    let mut out = Vec::new();
    let mut n = ROOT;
    while let Some(c) = rec.main_child(n) {
        out.push(rec.node(c).kind);
        n = c;
    }
    out
}

#[test]
fn a_trial_followed_to_the_end_of_the_main_line_still_only_adds_branches() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, _, _) = sharing(dir.path(), "(;SZ[9];B[ee];W[cc])");
    wb.navigate(Navigation::First);
    wb.navigate(Navigation::Forward(1));
    let before = main_line(&wb);
    let now = Instant::now();
    assert_eq!(enter(&mut wb, "a", now), None);
    // Forward along the record to W[cc], its last move, then two moves of the 试下.
    assert_eq!(
        msg(
            &mut wb,
            r#"{"type":"trial_navigate","clientId":"a","direction":"forward"}"#,
            now
        ),
        None
    );
    let end = wb.cursor();
    trial_move(&mut wb, "a", 6, 6, now);
    trial_move(&mut wb, "a", 2, 6, now);
    exit(&mut wb, "a", now);
    let line = main_line(&wb);
    assert_eq!(line[..2], before[..], "the record's moves stay the main line");
    assert!(line[2..].iter().all(|&k| k == NodeKind::DummyPass), "{line:?}");
    let rec = &wb.document().record;
    let branch = rec.node(end).children[1];
    assert!(matches!(
        rec.node(branch).kind,
        NodeKind::Move {
            point: Point { x: 6, y: 6 },
            ..
        }
    ));
    assert_eq!(
        rec.node(branch).children.len(),
        1,
        "later 试下 moves continue their own branch"
    );
}

#[test]
fn during_a_trial_the_console_komi_and_the_foreground_engine_lifecycle_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, _, _) = sharing(dir.path(), "(;SZ[9];B[ee];W[cc])");
    assert_eq!(enter(&mut wb, "a", Instant::now()), None);
    let exclusive = || {
        Some(CommandResult::Refused {
            message_key: "AnalysisSettings.reuseStatus.application_exclusive_mode".into(),
        })
    };
    for command in [
        Command::SendGtpCommand { command: "undo".into() },
        Command::RestartEngine,
        Command::SwitchEngine { index: 0 },
        Command::ShutdownEngines {
            scope: EngineScope::Current,
        },
        Command::ShutdownEngines {
            scope: EngineScope::All,
        },
    ] {
        assert_eq!(wb.game_gate(&command), exclusive(), "{command:?}");
    }
    assert_eq!(
        wb.game_gate(&Command::SetKomi { komi: 6.5 }),
        Some(CommandResult::Refused {
            message_key: "LizzieFrame.webTrialDesktopBlocked".into()
        })
    );
    // Engines other than the foreground one are not the 试下's.
    assert_eq!(
        wb.game_gate(&Command::ShutdownEngines {
            scope: EngineScope::Others
        }),
        None
    );
}

/// Java's lifecycle reservation is refused while a 试下 runs, so in 双引擎模式 the secondary engine
/// is neither restarted nor replaced (`reStartEngine2`, `switchEngine(index, false)` →
/// `showForegroundEngineLeaseInUse`); closing it takes no reservation (`killThisEngines2`).
#[test]
fn during_a_trial_the_secondary_engine_is_not_restarted_or_replaced_but_can_be_closed() {
    const LEASED: &str = "AnalysisSettings.reuseStatus.existing_lease";
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, secondary) = sharing_with_two_engines(dir.path(), "(;SZ[9];B[ee];W[cc])");
    assert_eq!(enter(&mut wb, "a", Instant::now()), None);
    for command in [Command::RestartSecondaryEngine, Command::SwitchSecondaryEngine { index: 2 }] {
        assert_eq!(
            wb.game_gate(&command),
            Some(CommandResult::Refused {
                message_key: LEASED.into()
            }),
            "{command:?}"
        );
    }
    // The operations refuse as well: a command admitted just before the 试下 began, and an engine
    // reload that does not come through the command gate.
    assert_eq!(wb.restart_secondary_engine().unwrap_err().message, LEASED);
    assert_eq!(wb.switch_secondary_engine(2).unwrap_err().message, LEASED);
    assert_eq!(world.latest(), secondary, "no engine was started");
    assert_eq!(wb.view(true).engine.secondary.map(|s| (s.index, s.ready)), Some((1, true)));

    assert_eq!(wb.game_gate(&Command::ShutdownSecondaryEngine), None);
    assert!(wb.shutdown_secondary_engine());
    assert_eq!(wb.view(true).engine.secondary, None);
    assert!(trial_active(&mut wb));
}

/// Java's secondary restart reserves the foreground engine until the new process's board sync
/// (`reStartEngine2` → `reserveEngineLifecycle`), and a 试下 needs that reservation
/// (`enterTrialWithResult` → `beginEngineModeReservation`).
#[test]
fn a_trial_waits_for_a_restarting_secondary_engine() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, secondary) = sharing_with_two_engines(dir.path(), "(;SZ[9];B[ee];W[cc])");
    assert!(wb.restart_secondary_engine().unwrap());
    let restarted = world.latest();
    assert_ne!(restarted, secondary, "a new process");

    let denied = enter(&mut wb, "a", Instant::now()).expect("refused while it starts");
    assert!(denied.contains(r#""reason":"engine_busy""#), "{denied}");
    assert!(!trial_active(&mut wb));

    serve(&mut wb, &world, restarted);
    assert_eq!(enter(&mut wb, "a", Instant::now()), None);
    assert!(trial_active(&mut wb));
}

/// The detailed toolbar's 引擎对战 (the saved match settings) is refused like the dialog's start,
/// before anything touches the 试下's engine.
#[test]
fn during_a_trial_an_engine_match_from_the_toolbar_is_refused_before_it_starts() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, _) = sharing(dir.path(), "(;SZ[9];B[ee];W[cc])");
    assert_eq!(enter(&mut wb, "a", Instant::now()), None);
    let state = |wb: &mut Workbench| {
        let engine = wb.view(true).engine;
        (
            format!("{:?}", engine.foreground),
            engine.paused,
            world.0.lock().unwrap().len(),
        )
    };
    let before = state(&mut wb);
    // The application's command entry: the gate first, then the command.
    let toolbar = Command::Match {
        op: MatchCommand::StartToolbar,
    };
    let result = match wb.game_gate(&toolbar) {
        Some(result) => result,
        None => wb.engine_match(MatchCommand::StartToolbar).unwrap(),
    };
    assert_eq!(
        result,
        CommandResult::Refused {
            message_key: "AnalysisSettings.reuseStatus.application_exclusive_mode".into()
        }
    );
    assert_eq!(
        state(&mut wb),
        before,
        "the 试下's engine, its analysis and the processes are unchanged"
    );
    assert!(trial_active(&mut wb));
}

#[test]
fn during_a_trial_contribution_does_not_start_and_leaves_the_record_and_engine_alone() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, id) = sharing(dir.path(), "(;SZ[9];B[ee];W[cc])");
    assert_eq!(enter(&mut wb, "a", Instant::now()), None);
    let doc = wb.document().id;
    // LizzieFrame.startContributeEngine cannot take the lifecycle reservation while a 试下 holds
    // the engine (Leelaz.java:15193) and shows existing_lease.
    assert_eq!(
        wb.contribution_start(None, None).unwrap(),
        CommandResult::Refused {
            message_key: "AnalysisSettings.reuseStatus.existing_lease".into()
        }
    );
    assert!(trial_active(&mut wb));
    assert_eq!(wb.document().id, doc, "no empty contribution board replaces the record");
    assert_eq!(world.latest(), id, "no engine was closed or started");
    assert!(wb.view(true).engine.foreground.is_some_and(|f| f.ready), "the trial's engine keeps running");
}

/// Paths that replace the foreground engine without passing the desktop gate: 远程算力, a settings
/// import and a restart asked by another feature. Java's switch takes `reserveEngineLifecycle`,
/// which a 试下 refuses (EngineManager.java:14386-14426, Leelaz.java:15190-15198).
#[test]
fn during_a_trial_remote_compute_a_settings_import_or_a_restart_keeps_the_trial_engine() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, id) = sharing(dir.path(), "(;SZ[9];B[ee];W[cc])");
    assert_eq!(enter(&mut wb, "a", Instant::now()), None);
    let engines = wb.config().engine.engines.clone();

    let zhizi = lizzie_core::cloud::CloudSettings {
        provider: lizzie_core::cloud::Provider::Zhizi,
        identifier: "fixture-account".into(),
        ..Default::default()
    };
    let refused = wb.activate_cloud(wb.cloud_ticket(), zhizi, true).unwrap_err();
    assert_eq!(refused.message, "RemoteCompute.error.engineSwitchFailed");
    assert_eq!(wb.config().engine.engines, engines, "no remote profile is committed");

    let java = dir.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), r#"{"ui": {"play-sound": false}}"#).unwrap();
    let preview = wb.preview_java_import(java_import::read(&JavaSource::new(&java, None)).unwrap());
    assert_eq!(
        wb.apply_java_import(preview.id, &[], false, false, true, None).unwrap(),
        Err("AnalysisSettings.reuseStatus.application_exclusive_mode")
    );
    assert!(wb.config().ui.play_sound, "nothing is imported");

    assert!(wb.restart_engine().is_err());
    assert_eq!(world.latest(), id, "no engine was closed or started");
    assert!(trial_active(&mut wb));
    assert!(wb.view(true).engine.foreground.is_some_and(|f| f.ready), "the trial's engine keeps running");
}

/// An engine that dies under a 试下 is left for the user, as under any lifecycle reservation
/// (`beginAutomaticRestartAttempt`, Leelaz.java:2121-2127), and the 试下 still holds the engine
/// seat: 跑谱贡献 does not take it (Java's engine object outlives its process); once the 试下
/// ends the engine can be restarted.
#[test]
fn an_engine_that_dies_during_a_trial_is_not_restarted_automatically() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, id) = sharing(dir.path(), "(;SZ[9];B[ee];W[cc])");
    assert_eq!(enter(&mut wb, "a", Instant::now()), None);
    wb.engine_input(id, EngineInput::Exited(Some(1)));
    assert_eq!(world.latest(), id, "no automatic restart under the 试下");
    let doc = wb.document().id;
    assert_eq!(
        wb.contribution_start(None, None).unwrap(),
        CommandResult::Refused { message_key: "AnalysisSettings.reuseStatus.existing_lease".into() }
    );
    assert_eq!(wb.document().id, doc, "the 试下's record stays");
    assert!(trial_active(&mut wb));

    exit(&mut wb, "a", Instant::now());
    wb.restart_engine().unwrap();
    let restarted = world.latest();
    assert_ne!(restarted, id, "the user's restart starts a new process");
    serve(&mut wb, &world, restarted);
    assert!(wb.view(true).engine.foreground.is_some_and(|f| f.ready));
}

#[test]
fn during_a_trial_desktop_edits_and_engine_modes_are_refused_and_browsing_is_held() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, _, _) = sharing(dir.path(), "(;SZ[9];B[ee];W[cc])");
    assert_eq!(enter(&mut wb, "a", Instant::now()), None);
    let at = wb.cursor();

    let play = Command::Edit {
        op: EditCommand::Play {
            point: Point { x: 1, y: 1 },
            color: None,
        },
    };
    assert_eq!(
        wb.game_gate(&play),
        Some(CommandResult::Refused {
            message_key: "LizzieFrame.webTrialDesktopBlocked".into()
        })
    );
    assert_eq!(
        wb.game_gate(&Command::Navigate { nav: Navigation::First }),
        Some(CommandResult::Unchanged)
    );
    assert_eq!(wb.cursor(), at);
    let auto = Command::StartAutoAnalysis {
        settings: AutoAnalysisSettings::default(),
    };
    assert_eq!(
        wb.game_gate(&auto),
        Some(CommandResult::Refused {
            message_key: "AnalysisSettings.reuseStatus.application_exclusive_mode".into()
        })
    );

    // 强制结束试下 frees the desktop again.
    assert_eq!(
        wb.web_board(WebBoardCommand::ForceExitTrial).unwrap(),
        CommandResult::Done
    );
    assert_eq!(
        wb.web_board(WebBoardCommand::ForceExitTrial).unwrap(),
        CommandResult::Unchanged
    );
    assert_eq!(wb.game_gate(&Command::Navigate { nav: Navigation::First }), None);
}

#[test]
fn a_trial_ends_after_five_idle_minutes_counted_from_its_owners_last_action() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, _, _) = sharing(dir.path(), "(;SZ[9];B[ee])");
    let start = Instant::now();
    assert_eq!(enter(&mut wb, "a", start), None);
    let later = start + Duration::from_secs(60);
    trial_move(&mut wb, "a", 2, 2, later);
    assert!(wb.engine_timer().is_some_and(|t| t <= later + TRIAL_IDLE));

    wb.engine_tick(start + TRIAL_IDLE);
    assert!(trial_active(&mut wb), "the move restarted the idle time");
    assert!(wb.engine_tick(later + TRIAL_IDLE));
    assert!(!trial_active(&mut wb));
}

#[test]
fn messages_and_stops_of_another_session_are_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, _, _) = sharing(dir.path(), "(;SZ[9];B[ee])");
    let now = Instant::now();
    let stale = ClientMessage::EnterTrial { client: "a".into() };
    assert_eq!(wb.web_board_message(SESSION - 1, stale, now), None);
    assert!(!trial_active(&mut wb));

    wb.navigate(Navigation::Last);
    assert_eq!(enter(&mut wb, "a", now), None);
    assert!(!wb.web_board_stopped(SESSION - 1), "a late stop of an earlier share");
    assert!(trial_active(&mut wb));
    assert_eq!(wb.view(true).web_board.url.as_deref(), Some("http://192.168.1.5:9998"));

    // Stopping the running share ends its 试下; its viewers' late messages do nothing.
    let anchor = wb.cursor();
    assert!(wb.web_board_stopped(SESSION));
    assert!(!trial_active(&mut wb));
    assert_eq!(wb.view(true).web_board.url, None);
    assert_eq!(
        wb.web_board(WebBoardCommand::ForceExitTrial).unwrap(),
        CommandResult::Unchanged
    );
    let late = ClientMessage::TrialMove {
        client: "a".into(),
        x: 2,
        y: 2,
    };
    assert_eq!(wb.web_board_message(SESSION, late, now), None);
    assert_eq!(wb.cursor(), anchor);
    assert!(wb.document().record.node(anchor).children.is_empty());
}

#[test]
fn opening_another_record_ends_the_trial_without_touching_the_new_one() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, _, _) = sharing(dir.path(), "(;SZ[9];B[ee])");
    let now = Instant::now();
    assert_eq!(enter(&mut wb, "a", now), None);
    open(&mut wb, "(;SZ[9];B[cc])");
    assert!(!trial_active(&mut wb));
    let revision = wb.document().revision;
    trial_move(&mut wb, "a", 2, 2, now);
    assert_eq!(
        wb.document().record.node(ROOT).children.len(),
        1,
        "the old 试下's moves do not land here"
    );
    assert_eq!(wb.document().revision, revision);
}

#[test]
fn messages_java_ignores_are_not_read() {
    for text in [
        "not json",
        r#"{"type":"chat","clientId":"a"}"#,
        r#"{"type":"trial_move","clientId":"a","x":-1,"y":3}"#,
        r#"{"type":"trial_navigate","clientId":"a","direction":"sideways"}"#,
    ] {
        assert_eq!(parse_client_message(text), None, "{text}");
    }
    assert_eq!(
        parse_client_message(r#"{"type":"trial_move","clientId":"a","x":"4","y":5}"#),
        Some(ClientMessage::TrialMove {
            client: "a".into(),
            x: 4,
            y: 5
        })
    );
}

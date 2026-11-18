//! 棋盘同步工具 through the public `Workbench` operations with a scripted helper and a controlled
//! KataGo. Independent expectations: Java `ReadBoard.java` (launch arguments 6747-6780, stale
//! helper output, `place` after a local move 6459-6512), the GMA hand
//! (`ReadBoardGmaPreparation`, `Leelaz.genmoveAnalyzeForReadBoard`, its restore barrier and
//! `engineStateUnrestored` quarantine, Leelaz.java:15320-15343, 21614-21676) and the refusal of
//! an automatic engine restart while GMA holds the engine (Leelaz.java:2121-2126).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use lizzie_core::api::{
    BatchMode, Command, CommandResult, GameCommand, GameKind, ImportStatus, ImportTarget, LeaseAvailabilityView, OnlineJob,
    OpenSource, PageReport, PonderingNoticeDecision, ReadBoardCommand, ReadBoardNotice,
};
use lizzie_core::browse::Navigation;
use lizzie_core::engine::cloud::{CloudLaunch, CloudProvider};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats;
use lizzie_core::java_import::{self, JavaSource};
use lizzie_core::readboard::{HelperHost, HelperId, HelperInput, HelperLaunch};
use lizzie_core::record::{NodeKind, Point, Stone, ROOT};
use lizzie_core::settings::{AutoAnalysisSettings, GameSettings, Storage};
use lizzie_core::web_board::parse_client_message;
use lizzie_core::workbench::Workbench;

// ------------------------------------------------------------------------ scripted helper

#[derive(Default)]
struct HelperWorld {
    launched: Vec<(HelperId, HelperLaunch)>,
    sent: Vec<(HelperId, String)>,
    stopped: Vec<HelperId>,
    /// A stopped helper's process is still exiting: its exit is reported by the test
    /// (`Workbench::readboard_exited`). Otherwise it is gone when `stop` returns.
    exiting: bool,
}

#[derive(Clone, Default)]
struct Helpers(Arc<Mutex<HelperWorld>>);

impl Helpers {
    fn last(&self) -> HelperId {
        self.0.lock().unwrap().launched.last().expect("a helper was started").0
    }
    fn sent_to(&self, helper: HelperId) -> Vec<String> {
        self.0.lock().unwrap().sent.iter().filter(|(h, _)| *h == helper).map(|(_, l)| l.clone()).collect()
    }
}

struct Host(Helpers);
impl HelperHost for Host {
    fn launch(&mut self, helper: HelperId, spec: &HelperLaunch) -> std::io::Result<()> {
        self.0 .0.lock().unwrap().launched.push((helper, spec.clone()));
        Ok(())
    }
    fn send(&mut self, helper: HelperId, line: &str) -> std::io::Result<()> {
        self.0 .0.lock().unwrap().sent.push((helper, line.to_string()));
        Ok(())
    }
    fn stop(&mut self, helper: HelperId) -> bool {
        let mut world = self.0 .0.lock().unwrap();
        world.stopped.push(helper);
        world.exiting
    }
}

// ------------------------------------------------------------------------ controlled engine

#[derive(Default)]
struct Process {
    id: InstanceId,
    sent: Vec<String>,
}
#[derive(Clone, Default)]
struct Engines(Arc<Mutex<Vec<Process>>>);
impl Engines {
    fn take(&self, id: InstanceId) -> Vec<String> {
        std::mem::take(&mut self.0.lock().unwrap().iter_mut().find(|p| p.id == id).unwrap().sent)
    }
    fn last(&self) -> InstanceId {
        self.0.lock().unwrap().last().unwrap().id
    }
    fn count(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}
struct Io(Engines, InstanceId);
impl EngineIo for Io {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        self.0 .0.lock().unwrap().iter_mut().find(|p| p.id == self.1).unwrap().sent.push(line.into());
        Ok(())
    }
    fn terminate(self: Box<Self>) {}
}
struct Launch(Engines);
impl Launcher for Launch {
    fn launch(&self, id: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        self.0 .0.lock().unwrap().push(Process { id, ..Process::default() });
        Ok(Box::new(Io(self.0.clone(), id)))
    }
}

const KATA: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nset_position\nundo\nkata-analyze\n\
kata-set-rules\nkata-get-rules\nkata-get-param\nkata-set-param\nkata-genmove_analyze\ngenmove\nstop";
const RULES: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;

struct Rig {
    wb: Workbench,
    engines: Engines,
    helpers: Helpers,
    engine: InstanceId,
    /// Engine commands left unanswered: (gtp id, command).
    held: Vec<(String, String)>,
    analyzing: bool,
    _dir: tempfile::TempDir,
}

fn out(wb: &mut Workbench, id: InstanceId, text: &str) {
    wb.engine_input(id, EngineInput::Stdout(text.into()));
}

impl Rig {
    fn new() -> Rig {
        Rig::with_engine(EngineProfile { name: "KataGo".into(), command: "/opt/katago gtp".into(), ..EngineProfile::default() })
    }

    fn with_engine(profile: EngineProfile) -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("app");
        std::fs::create_dir_all(base.join("readboard")).unwrap();
        std::fs::write(base.join("readboard").join("readboard.exe"), b"helper").unwrap();
        let engines = Engines::default();
        let helpers = Helpers::default();
        let mut wb = Workbench::new(
            Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data")),
            None,
            Box::new(Launch(engines.clone())),
            vec![],
        );
        wb.set_auto_quick_analyze(false).unwrap();
        wb.save_engine_settings(vec![profile], EngineStartup::Default).unwrap();
        wb.set_readboard_host(Box::new(Host(helpers.clone())), vec![PathBuf::from(&base)]);
        wb.start_engines();
        let engine = engines.last();
        let mut r = Rig { wb, engines, helpers, engine, held: Vec::new(), analyzing: false, _dir: dir };
        r.serve(&|_| None);
        r
    }

    /// Answers what the engine was sent; `kata-genmove_analyze` is held. `answer` may give a
    /// reply (`?…` for a failure) or "HOLD".
    fn serve(&mut self, answer: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
        let mut seen = Vec::new();
        loop {
            let pending = self.engines.take(self.engine);
            if pending.is_empty() {
                return seen;
            }
            for line in pending {
                let (id, cmd) = line.split_once(' ').unwrap();
                seen.push(cmd.to_string());
                if std::mem::take(&mut self.analyzing) {
                    out(&mut self.wb, self.engine, "");
                }
                if cmd.starts_with("kata-genmove_analyze") {
                    self.held.push((id.to_string(), cmd.to_string()));
                    continue;
                }
                if let Some(text) = answer(cmd) {
                    if text == "HOLD" {
                        self.held.push((id.to_string(), cmd.to_string()));
                    } else {
                        self.reply(id, &text);
                    }
                    continue;
                }
                if cmd.starts_with("kata-analyze") {
                    out(&mut self.wb, self.engine, &format!("={id}"));
                    self.analyzing = true;
                    continue;
                }
                let text = match cmd.split(' ').next().unwrap() {
                    "name" => "KataGo",
                    "version" => "1.18.2",
                    "list_commands" => KATA,
                    "kata-get-rules" => RULES,
                    "kata-get-param" => match cmd.split(' ').nth(1) {
                        Some("ponderingEnabled") => "false",
                        Some("maxTime") => "1e+20",
                        _ => "1000000",
                    },
                    _ => "",
                };
                self.reply(id, text);
            }
        }
    }

    fn reply(&mut self, id: &str, text: &str) {
        match text.strip_prefix('?') {
            Some(error) => out(&mut self.wb, self.engine, &format!("?{id} {error}")),
            None => out(&mut self.wb, self.engine, &format!("={id} {text}")),
        }
        out(&mut self.wb, self.engine, "");
    }

    /// KataGo's `kata-genmove_analyze` answer: info lines, then `play <vertex>`.
    fn answer_genmove(&mut self, vertex: &str) {
        let at = self.held.iter().position(|(_, c)| c.starts_with("kata-genmove_analyze")).expect("a held GMA request");
        let (id, _) = self.held.remove(at);
        out(&mut self.wb, self.engine, &format!("={id}"));
        out(&mut self.wb, self.engine, &format!("info move {vertex} visits 10 winrate 0.5 scoreLead 0.1 prior 0.2 order 0 pv {vertex}"));
        out(&mut self.wb, self.engine, &format!("play {vertex}"));
        out(&mut self.wb, self.engine, "");
    }

    fn open(&mut self) -> HelperId {
        assert_eq!(self.wb.readboard(ReadBoardCommand::Open).unwrap(), CommandResult::Done);
        self.helpers.last()
    }

    fn line(&mut self, helper: HelperId, line: &str) -> bool {
        self.wb.readboard_input(helper, HelperInput::Line(line.to_string()))
    }

    fn lines(&mut self, helper: HelperId, lines: &[&str]) {
        for l in lines {
            self.line(helper, l);
        }
    }

    /// A 19×19 frame: `stones` as (x, y, code) with Java's codes (1 black, 2 white, 3/4 the
    /// marked last move), then `end`.
    fn frame(&mut self, helper: HelperId, stones: &[(usize, usize, u8)]) {
        for y in 0..19 {
            let mut row = vec!["0".to_string(); 19];
            for &(sx, sy, code) in stones {
                if sy == y {
                    row[sx] = code.to_string();
                }
            }
            self.line(helper, &format!("re={}", row.join(",")));
        }
        self.line(helper, "end");
    }

    fn main_line(&self) -> Vec<(Stone, Point)> {
        let rec = &self.wb.document().record;
        let mut node = ROOT;
        let mut moves = Vec::new();
        while let Some(c) = rec.main_child(node) {
            if let NodeKind::Move { color, point } = rec.node(c).kind {
                moves.push((color, point));
            }
            node = c;
        }
        moves
    }

    fn lease(&mut self) -> LeaseAvailabilityView {
        self.wb.view(false).engine.lease
    }
}

fn p(x: u16, y: u16) -> Point {
    Point { x, y }
}

/// A helper syncing both ways with GMA armed for Black on an empty board, its first hand asked.
fn gma_rig() -> (Rig, HelperId) {
    let mut r = Rig::new();
    let h = r.open();
    r.lines(h, &["ready", "start 19 19", "sync", "bothSync", "play>black>5 100 0 gma"]);
    r.frame(h, &[]);
    (r, h)
}

/// The 自建算力 engine (`remote-compute://custom-websocket`), here answering as KataGo through the
/// controlled process; its real adapter rejects `ponderingEnabled` (`kata-get-param` and
/// `kata-set-param` answer `unknown parameter`).
fn websocket_rig() -> Rig {
    let cloud = CloudLaunch { provider: CloudProvider::Custom, identifier: String::new(), args: String::new(), url: "ws://127.0.0.1:9/analysis".into() };
    Rig::with_engine(EngineProfile {
        name: "Custom Compute".into(),
        command: "remote-compute://custom-websocket".into(),
        cloud: Some(cloud),
        ..EngineProfile::default()
    })
}

fn websocket_adapter(cmd: &str) -> Option<String> {
    cmd.contains("ponderingEnabled").then(|| "?unknown parameter".to_string())
}

/// GMA armed for Black on an empty board, the helper having asked for pondering or not.
fn arm_gma(r: &mut Rig, ponder: bool) -> HelperId {
    let h = r.open();
    r.lines(h, &["ready", "start 19 19", "sync", "bothSync", if ponder { "playpon on" } else { "playpon off" }, "play>black>5 100 0 gma"]);
    r.frame(h, &[]);
    h
}

fn pondering_notice(r: &mut Rig) -> Option<u64> {
    r.wb.view(false).readboard.pondering_notice
}

fn answer(r: &mut Rig, generation: u64, decision: PonderingNoticeDecision) {
    let op = ReadBoardCommand::AnswerPonderingNotice { generation, decision };
    assert_eq!(r.wb.readboard(op).unwrap(), CommandResult::Done);
}

// ------------------------------------------------------------------------ helper lifecycle

#[test]
fn the_helper_starts_with_java_arguments_and_a_replaced_helper_is_ignored() {
    let mut r = Rig::new();
    let first = r.open();
    let spec = r.helpers.0.lock().unwrap().launched[0].1.clone();
    assert_eq!(&spec.args[..7], ["yzy", " ", " ", " ", "0", "cn", "-1"], "{:?}", spec.args);
    assert!(spec.program.ends_with("readboard/readboard.exe"));
    r.line(first, "ready");
    let sent = r.helpers.sent_to(first);
    assert_eq!(&sent[..3], ["version", "readboardUpdateSupported", "readboardUpdatePackageV2Supported"]);
    assert!(sent[3].starts_with("analysisState "), "{sent:?}");

    // Opening again closes the running helper first (`quit`, then stopped).
    let second = r.open();
    assert_ne!(first, second);
    assert_eq!(r.helpers.sent_to(first).last().map(String::as_str), Some("quit"));
    assert!(r.helpers.0.lock().unwrap().stopped.contains(&first));

    // The old helper's late lines and end change nothing of its successor's sync.
    assert!(!r.line(first, "sync"));
    assert!(!r.wb.readboard_input(first, HelperInput::Ended { error: None }));
    r.line(second, "sync");
    assert!(r.wb.view(false).readboard.syncing);
    r.frame(first, &[(3, 3, 3)]);
    assert!(r.main_line().is_empty(), "an old helper's frame never reaches the record");
}

/// Java `beginHostedReadBoardUpdate` / `queueReadBoardRestart`: while an accepted update is being
/// installed, opening the tool again starts nothing; the installer's restart is the only one.
#[test]
fn opening_the_tool_during_an_update_install_does_not_start_a_second_helper() {
    let mut r = Rig::new();
    let first = r.open();
    r.lines(first, &["ready", "version 230616"]);
    let zip_path = r._dir.path().join("readboard-github-release-v3.2.1.zip");
    {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
        for (name, content) in [
            ("readboard.exe", "new-exe"),
            ("readboard.dll", "new-dll"),
            ("readboard.runtimeconfig.json", "{}"),
            ("readboard.deps.json", "{}"),
            ("language_cn.txt", "cn"),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(content.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }
    r.line(first, &format!("readboardUpdateReady\tv3.2.1\t{}", zip_path.display()));
    assert_eq!(r.wb.view(false).readboard.update_offer.as_deref(), Some("v3.2.1"));
    r.wb.readboard(ReadBoardCommand::AnswerUpdate { accept: true }).unwrap();
    let install = r.wb.take_readboard_install().expect("an accepted update is installed");

    // The user opens the tool while the installer runs: nothing starts.
    r.wb.readboard(ReadBoardCommand::Open).unwrap();
    assert_eq!(r.helpers.0.lock().unwrap().launched.len(), 1, "no helper starts during the install");

    let result = install.run();
    r.wb.finish_readboard_install(install, result);
    let world = r.helpers.0.lock().unwrap();
    assert_eq!(world.launched.len(), 2, "the installer restarts the helper once");
    let current = world.launched[1].0;
    assert!(
        world.launched.iter().all(|(id, _)| *id == current || world.stopped.contains(id)),
        "every helper but the current one was stopped"
    );
}

/// 同步时不播放落子声: moves the helper syncs into the record make no stone sound.
#[test]
fn synced_moves_are_silent_when_sync_sounds_are_off() {
    let mut r = Rig::new();
    let h = r.open();
    r.lines(h, &["ready", "start 19 19", "sync"]);
    r.wb.view(false);
    r.frame(h, &[(3, 3, 3)]);
    assert_eq!(r.main_line().len(), 1);
    assert!(r.wb.view(false).sounds.is_empty(), "no sound while syncing with the option on");

    r.wb.readboard(ReadBoardCommand::SetNoSoundInSync { enabled: false }).unwrap();
    r.frame(h, &[(3, 3, 1), (15, 3, 4)]);
    assert_eq!(r.main_line().len(), 2);
    assert!(!r.wb.view(false).sounds.is_empty(), "the stone sounds with the option off");
}

#[test]
fn without_the_helper_program_the_menu_says_so() {
    let mut r = Rig::new();
    std::fs::remove_file(r._dir.path().join("app/readboard/readboard.exe")).unwrap();
    assert!(!r.wb.view(false).readboard.available);
    r.wb.readboard(ReadBoardCommand::Open).unwrap();
    assert!(r.helpers.0.lock().unwrap().launched.is_empty());
    let notices = r.wb.view(false).readboard.notices;
    assert_eq!(notices, [ReadBoardNotice::Message { key: "FunctionSearch.unavailable.readBoard".into(), args: vec![] }]);
}

#[test]
fn synced_frames_become_moves_and_a_local_move_is_clicked_on_the_target() {
    let mut r = Rig::new();
    let h = r.open();
    r.lines(h, &["ready", "start 19 19", "sync", "bothSync"]);
    r.frame(h, &[(3, 3, 3)]);
    assert_eq!(r.main_line(), [(Stone::Black, p(3, 3))]);
    r.wb.edit(lizzie_core::api::EditCommand::Play { point: p(15, 15), color: None }).unwrap();
    assert_eq!(r.main_line(), [(Stone::Black, p(3, 3)), (Stone::White, p(15, 15))]);
    assert_eq!(r.helpers.sent_to(h).last().map(String::as_str), Some("place 15 15"));

    // While that `place` is unconfirmed, another local move is dropped.
    r.wb.edit(lizzie_core::api::EditCommand::Play { point: p(16, 16), color: None }).unwrap();
    assert_eq!(r.main_line().len(), 2);
    // The frame showing the stone confirms it; the next move goes out again.
    r.frame(h, &[(3, 3, 1), (15, 15, 4)]);
    r.wb.edit(lizzie_core::api::EditCommand::Play { point: p(16, 16), color: None }).unwrap();
    assert_eq!(r.helpers.sent_to(h).last().map(String::as_str), Some("place 16 16"));
}

#[test]
fn one_way_sync_keeps_local_moves_off_the_synced_line() {
    let mut r = Rig::new();
    let h = r.open();
    r.lines(h, &["ready", "start 19 19", "sync"]);
    r.frame(h, &[(3, 3, 3)]);
    r.wb.edit(lizzie_core::api::EditCommand::Play { point: p(15, 15), color: None }).unwrap();
    assert_eq!(r.main_line(), [(Stone::Black, p(3, 3))], "the click is a variation");
    assert!(!r.helpers.sent_to(h).iter().any(|l| l.starts_with("place")));
    // The next synced move continues the main line.
    r.frame(h, &[(3, 3, 1), (16, 3, 4)]);
    assert_eq!(r.main_line(), [(Stone::Black, p(3, 3)), (Stone::White, p(16, 3))]);
}

// ------------------------------------------------------------------------ GMA

#[test]
fn a_gma_hand_plays_its_move_and_cleared_gma_gives_the_parameters_back() {
    let (mut r, h) = gma_rig();
    let sent = r.serve(&|_| None);
    for get in ["kata-get-param ponderingEnabled", "kata-get-param maxTime", "kata-get-param maxVisits"] {
        assert!(sent.iter().any(|c| c == get), "{get}: {sent:?}");
    }
    assert!(sent.iter().any(|c| c == "kata-set-param maxTime 5"), "{sent:?}");
    assert!(sent.iter().any(|c| c == "kata-set-param maxVisits 100"), "{sent:?}");
    assert!(r.held.iter().any(|(_, c)| c.starts_with("kata-genmove_analyze B")), "{:?}", r.held);
    assert_eq!(r.lease(), LeaseAvailabilityView::ReadBoardGma);
    // Engine modes are refused while GMA holds the engine.
    let start = Command::Game {
        op: GameCommand::Start { kind: GameKind::Genmove, settings: GameSettings::default(), autoplay: None, engine: None },
    };
    assert_eq!(
        r.wb.game_gate(&start),
        Some(CommandResult::Refused { message_key: "AnalysisSettings.reuseStatus.readboard_gma".into() })
    );
    assert_eq!(
        r.wb.game_gate(&Command::RestartEngine),
        Some(CommandResult::Refused { message_key: "AnalysisSettings.reuseStatus.existing_lease".into() }),
        "a hand in flight holds the engine"
    );

    r.answer_genmove("Q16");
    r.serve(&|_| None);
    assert_eq!(r.main_line(), [(Stone::Black, p(15, 3))]);
    assert_eq!(r.helpers.sent_to(h).last().map(String::as_str), Some("place 15 3"));
    assert_eq!(r.wb.game_gate(&Command::RestartEngine), None, "between hands the engine may be restarted");

    // `stopsync` clears GMA: the original parameters go back and the engine is free again.
    r.line(h, "stopsync");
    let sent = r.serve(&|_| None);
    assert!(sent.iter().any(|c| c == "kata-set-param maxTime 1e+20"), "{sent:?}");
    assert!(sent.iter().any(|c| c == "kata-set-param maxVisits 1000000"), "{sent:?}");
    assert_eq!(r.lease(), LeaseAvailabilityView::Available);
    assert_eq!(r.wb.view(false).readboard.gma, None);
}

#[test]
fn a_refused_parameter_leaves_the_engine_unrestored_until_it_is_restarted() {
    let (mut r, h) = gma_rig();
    r.serve(&|c| c.starts_with("kata-set-param maxTime").then(|| "?unknown parameter".to_string()));
    assert_eq!(r.lease(), LeaseAvailabilityView::EngineStateUnrestored);
    assert!(r.held.is_empty(), "no move is asked from an unrestored engine");
    // A new frame does not start another hand with the quarantined engine.
    r.frame(h, &[(3, 3, 4)]);
    r.frame(h, &[(3, 3, 2), (15, 15, 4)]);
    r.serve(&|_| None);
    assert!(r.held.is_empty());
    // The way out is restarting the engine; the old instance cannot touch its successor.
    let old = r.engine;
    assert_eq!(r.wb.game_gate(&Command::RestartEngine), None);
    r.wb.restart_engine().unwrap();
    r.engine = r.engines.last();
    assert_ne!(r.engine, old);
    out(&mut r.wb, old, "=99 late");
    r.serve(&|_| None);
    assert_ne!(r.lease(), LeaseAvailabilityView::EngineStateUnrestored);
}

/// `notifyAutoPlay` (Leelaz.java:8488-8491): with GMA armed, the analysis asked before it may
/// still report enough visits, but only GMA's own request moves; nothing is clicked on the target.
#[test]
fn analysis_reported_after_gma_is_armed_makes_no_move() {
    let mut r = Rig::new();
    let h = r.open();
    r.lines(h, &["ready", "start 19 19", "sync", "bothSync"]);
    r.serve(&|_| None);
    assert!(r.analyzing, "the analysis of the empty board is reporting");
    r.line(h, "play>black>3 200 0 gma");
    out(&mut r.wb, r.engine, "info move R4 visits 866 winrate 0.5 scoreLead 0.1 prior 0.2 order 0 pv R4");
    r.serve(&|_| None);
    assert_eq!(r.main_line(), []);
    assert_eq!(places(&r, h), Vec::<String>::new());
}

#[test]
fn an_engine_that_dies_during_a_gma_hand_is_not_restarted_automatically_but_the_user_can() {
    let (mut r, h) = gma_rig();
    r.serve(&|_| None);
    assert!(!r.held.is_empty(), "the hand waits for its move");
    let before = r.engines.count();
    r.wb.engine_input(r.engine, EngineInput::Exited(Some(1)));
    assert_eq!(r.engines.count(), before, "no automatic restart while GMA reserves the engine");

    // The user stops the sync and restarts the engine: a new process, ready and analysing.
    r.line(h, "stopsync");
    assert_eq!(r.wb.game_gate(&Command::RestartEngine), None, "the dead hand does not hold the engine");
    r.wb.restart_engine().unwrap();
    assert_eq!(r.engines.count(), before + 1, "the user's restart starts a new process");
    r.engine = r.engines.last();
    r.held.clear();
    let sent = r.serve(&|_| None);
    assert!(r.wb.view(false).engine.foreground.is_some_and(|f| f.ready), "{sent:?}");
    assert!(sent.iter().any(|c| c.starts_with("kata-analyze")), "analysis continues: {sent:?}");
    assert_ne!(r.lease(), LeaseAvailabilityView::ReadBoardGma);
}

#[test]
fn a_retired_cloud_session_is_not_rebuilt_while_gma_reserves_the_engine_or_left_it_unrestored() {
    // The cloud transport recovery goes through the same automatic restart admission
    // (`EngineManager.restartRemoteEngineInBackground` → `beginAutomaticRestartAttempt`).
    let (mut r, _h) = gma_rig();
    r.serve(&|_| None);
    assert!(!r.held.is_empty(), "the hand waits for its move");
    let before = r.engines.count();
    r.wb.engine_input(r.engine, EngineInput::CloudRecovery { message: "accepted progress stalled".into() });
    assert_eq!(r.engines.count(), before, "no session rebuild while a GMA hand reserves the engine");

    let (mut r, _h) = gma_rig();
    r.serve(&|c| c.starts_with("kata-set-param maxTime").then(|| "?unknown parameter".to_string()));
    assert_eq!(r.lease(), LeaseAvailabilityView::EngineStateUnrestored);
    let before = r.engines.count();
    r.wb.engine_input(r.engine, EngineInput::CloudRecovery { message: "accepted progress stalled".into() });
    assert_eq!(r.engines.count(), before, "no session rebuild of an engine left unrestored");
}

#[test]
fn contribution_does_not_start_while_a_gma_hand_reserves_the_engine() {
    let (mut r, _h) = gma_rig();
    r.serve(&|_| None);
    assert!(!r.held.is_empty(), "the hand waits for its move");
    let before = r.engines.count();
    // beginReadBoardGmaSession holds the lifecycle transition startContributeEngine needs
    // (Leelaz.java:21342-21357, 15190-15198).
    assert_eq!(
        r.wb.contribution_start(None, None).unwrap(),
        CommandResult::Refused { message_key: "AnalysisSettings.reuseStatus.existing_lease".into() }
    );
    assert_eq!(r.engines.count(), before);
    assert!(r.wb.view(false).engine.foreground.is_some(), "the GMA hand keeps its engine");
}

fn enter_trial(wb: &mut Workbench) -> Option<String> {
    let message = parse_client_message(r#"{"type":"enter_trial","clientId":"a"}"#).unwrap();
    wb.web_board_message(7, message, Instant::now())
}

/// A GMA hand holds `readBoardGmaReservation`, which Web 试下's `beginEngineModeReservation`
/// refuses (Leelaz.java:15200-15205); and GMA is not armed while a 试下 holds the engine
/// (`canArmReadBoardGma`, Leelaz.java:15165-15177).
#[test]
fn a_web_trial_and_a_gma_hand_do_not_take_the_engine_from_each_other() {
    let (mut r, _h) = gma_rig();
    r.serve(&|_| None);
    assert!(!r.held.is_empty(), "the hand waits for its move");
    r.wb.web_board_started(7, "http://127.0.0.1:9998".into());
    let denied = enter_trial(&mut r.wb).expect("the hand holds the engine");
    assert!(denied.contains(r#""reason":"engine_busy""#), "{denied}");
    assert!(!r.wb.view(false).web_board.trial_active);

    let mut r = Rig::new();
    r.wb.web_board_started(7, "http://127.0.0.1:9998".into());
    assert_eq!(enter_trial(&mut r.wb), None);
    let h = r.open();
    r.lines(h, &["ready", "start 19 19", "sync", "bothSync", "play>black>5 100 0 gma"]);
    r.frame(h, &[]);
    let sent = r.serve(&|_| None);
    assert!(r.held.is_empty() && !sent.iter().any(|c| c.starts_with("kata-set-param")), "no hand is asked: {sent:?}");
    // The refused `play>…gma` is not kept for later: after the 试下 the next frame asks no hand.
    let exit = parse_client_message(r#"{"type":"exit_trial","clientId":"a"}"#).unwrap();
    assert_eq!(r.wb.web_board_message(7, exit, Instant::now()), None);
    r.frame(h, &[]);
    let sent = r.serve(&|_| None);
    assert!(r.held.is_empty() && !sent.iter().any(|c| c.starts_with("kata-set-param")), "no hand is asked: {sent:?}");
    assert_ne!(r.lease(), LeaseAvailabilityView::ReadBoardGma);

    // A 试下 whose engine died still refuses arming: no hand is asked of the restarted engine.
    let mut r = Rig::new();
    r.wb.web_board_started(7, "http://127.0.0.1:9998".into());
    assert_eq!(enter_trial(&mut r.wb), None);
    r.wb.engine_input(r.engine, EngineInput::Exited(Some(1)));
    let h = r.open();
    r.lines(h, &["ready", "start 19 19", "sync", "bothSync", "play>black>5 100 0 gma"]);
    let exit = parse_client_message(r#"{"type":"exit_trial","clientId":"a"}"#).unwrap();
    assert_eq!(r.wb.web_board_message(7, exit, Instant::now()), None);
    r.wb.restart_engine().unwrap();
    r.engine = r.engines.last();
    r.serve(&|_| None);
    r.frame(h, &[]);
    let sent = r.serve(&|_| None);
    assert!(r.held.is_empty() && !sent.iter().any(|c| c.starts_with("kata-set-param")), "no hand is asked: {sent:?}");
    assert_ne!(r.lease(), LeaseAvailabilityView::ReadBoardGma);
}

/// 远程算力 and a settings import replace the engine outside the desktop gate; a GMA hand's
/// reservation refuses them as it refuses a switch (EngineManager.java:14386-14426).
#[test]
fn remote_compute_and_a_settings_import_wait_for_the_gma_hand() {
    let (mut r, _h) = gma_rig();
    r.serve(&|_| None);
    assert!(!r.held.is_empty(), "the hand waits for its move");
    let before = r.engines.count();
    let engines = r.wb.config().engine.engines.clone();
    let zhizi = lizzie_core::cloud::CloudSettings {
        provider: lizzie_core::cloud::Provider::Zhizi,
        identifier: "fixture-account".into(),
        ..Default::default()
    };
    assert_eq!(r.wb.activate_cloud(zhizi, true).unwrap_err().message, "RemoteCompute.error.engineSwitchFailed");
    assert_eq!(r.wb.config().engine.engines, engines);

    let java = r._dir.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), r#"{"ui": {"play-sound": false}}"#).unwrap();
    let preview = r.wb.preview_java_import(java_import::read(&JavaSource::new(&java, None)).unwrap());
    assert_eq!(
        r.wb.apply_java_import(preview.id, &[], false, false, true, None).unwrap(),
        Err("AnalysisSettings.reuseStatus.existing_lease")
    );
    assert!(r.wb.config().ui.play_sound, "nothing is imported");
    assert_eq!(r.engines.count(), before, "the hand keeps its engine");
    assert_eq!(r.lease(), LeaseAvailabilityView::ReadBoardGma);
}

#[test]
fn the_helper_ending_stops_gma_and_the_hand_still_restores_the_engine() {
    let (mut r, h) = gma_rig();
    r.serve(&|_| None);
    assert!(r.wb.readboard_input(h, HelperInput::Ended { error: None }));
    assert!(!r.wb.view(false).readboard.running);
    // The retired hand's late answer is not played; the engine is given its parameters back.
    r.answer_genmove("Q16");
    let sent = r.serve(&|_| None);
    assert!(r.main_line().is_empty(), "a retired hand's move is not placed");
    assert!(sent.iter().any(|c| c == "kata-set-param maxTime 1e+20"), "{sent:?}");
    assert_eq!(r.lease(), LeaseAvailabilityView::Available);
}

/// GMA's first hand (`gma_rig`) is retired by the helper ending; its stale answer leads to its
/// parameters being given back, the last restore (`maxVisits`, the batch's fence) left
/// unanswered.
fn retired_gma_hand_restoring_parameters() -> Rig {
    let (mut r, h) = gma_rig();
    r.serve(&|_| None);
    assert!(r.held.iter().any(|(_, c)| c.starts_with("kata-genmove_analyze B")), "{:?}", r.held);
    assert!(r.wb.readboard_input(h, HelperInput::Ended { error: None }));
    r.answer_genmove("Q16");
    let sent = r.serve(&|c| (c == "kata-set-param maxVisits 1000000").then(|| "HOLD".to_string()));
    assert!(r.main_line().is_empty(), "a retired hand's move is not placed");
    assert!(sent.iter().any(|c| c == "kata-set-param maxTime 1e+20"), "{sent:?}");
    assert!(r.held.iter().any(|(_, c)| c == "kata-set-param maxVisits 1000000"), "{:?}", r.held);
    assert_eq!(r.lease(), LeaseAvailabilityView::ReadBoardGma);
    r
}

/// The user plays D16 at the root (the helper is gone, so nothing gates the click): the
/// record's current position changes while the parameters are being given back.
fn change_position(r: &mut Rig) {
    r.wb.edit(lizzie_core::api::EditCommand::Play { point: p(3, 3), color: None }).unwrap();
    assert_eq!(r.main_line(), [(Stone::Black, p(3, 3))]);
}

/// Answers the held commands `which` picks with `text`.
fn answer_held(r: &mut Rig, which: &dyn Fn(&str) -> bool, text: &str) {
    for (id, cmd) in std::mem::take(&mut r.held) {
        if which(&cmd) {
            r.reply(&id, text);
        } else {
            r.held.push((id, cmd));
        }
    }
}

/// A command syncing the engine board to a position.
fn syncs(command: &str) -> bool {
    ["undo", "clear_board", "set_position", "play "].iter().any(|s| command.starts_with(s))
}

/// Java `ReadBoardGmaSession` `RestoringRuntime` → `RestoringExact` (latest wins): the position
/// changes while the parameters are being given back; the reservation stays until the engine
/// holds that position, and only then is the engine free.
#[test]
fn a_position_changed_during_the_gma_parameter_restore_is_synced_before_the_engine_is_free() {
    let mut r = retired_gma_hand_restoring_parameters();
    change_position(&mut r);
    let start = Command::Game {
        op: GameCommand::Start { kind: GameKind::Genmove, settings: GameSettings::default(), autoplay: None, engine: None },
    };
    let gma_holds = Some(CommandResult::Refused { message_key: "AnalysisSettings.reuseStatus.readboard_gma".into() });
    assert_eq!(r.wb.game_gate(&start), gma_holds);

    // The parameters are back; the engine still holds the old position.
    answer_held(&mut r, &|c| c == "kata-set-param maxVisits 1000000", "");
    let sync = r.serve(&|_| Some("HOLD".to_string()));
    assert!(sync.iter().any(|c| syncs(c)), "the new position is synced: {sync:?}");
    assert_eq!(r.lease(), LeaseAvailabilityView::ReadBoardGma);
    assert_eq!(r.wb.game_gate(&start), gma_holds, "a game cannot take the engine before it is restored");
    assert_eq!(
        r.wb.game_gate(&Command::RestartEngine),
        Some(CommandResult::Refused { message_key: "AnalysisSettings.reuseStatus.existing_lease".into() }),
        "the hand still holds the reservation"
    );

    answer_held(&mut r, &|_| true, "");
    r.serve(&|_| None);
    assert_eq!(r.lease(), LeaseAvailabilityView::Available);
    assert_eq!(r.wb.game_gate(&Command::RestartEngine), None);
}

/// The latest position refused after the parameter restore ends the hand FAILED: the engine is
/// left unrestored (`引擎状态未恢复，请切换或重启引擎`) instead of being given back.
#[test]
fn a_position_refused_after_the_gma_parameter_restore_leaves_the_engine_unrestored() {
    let mut r = retired_gma_hand_restoring_parameters();
    change_position(&mut r);
    answer_held(&mut r, &|c| c == "kata-set-param maxVisits 1000000", "");
    let sent = r.serve(&|c| syncs(c).then(|| "?illegal move".to_string()));
    assert!(sent.iter().any(|c| syncs(c)), "{sent:?}");
    assert_eq!(r.lease(), LeaseAvailabilityView::EngineStateUnrestored);
}

/// GMA armed for Black (5 s, 100 visits) on an empty board, the helper having asked for
/// pondering or not, its first hand played (Q16). The engine's own `ponderingEnabled` is
/// `ponder_original`.
fn first_gma_hand_played(ponder: bool, ponder_original: &'static str) -> (Rig, HelperId) {
    let mut r = Rig::new();
    let h = arm_gma(&mut r, ponder);
    let sent = r.serve(&move |c| (c == "kata-get-param ponderingEnabled").then(|| ponder_original.to_string()));
    assert!(sent.iter().any(|c| c == "kata-set-param maxTime 5"), "{sent:?}");
    r.answer_genmove("Q16");
    r.serve(&|_| None);
    assert_eq!(r.main_line(), [(Stone::Black, p(15, 3))]);
    assert!(r.held.is_empty(), "between hands: {:?}", r.held);
    (r, h)
}

/// GMA cleared (`stopsync`) between two hands while a viewer's Web 试下 holds the engine: its
/// parameters cannot be given back during the 试下 and go back as soon as it ends.
#[test]
fn gma_parameters_left_during_a_web_trial_go_back_when_the_trial_ends() {
    let (mut r, h) = first_gma_hand_played(false, "false");
    r.wb.web_board_started(7, "http://127.0.0.1:9998".into());
    assert_eq!(enter_trial(&mut r.wb), None, "between hands a 试下 may start");
    r.serve(&|_| None);
    r.line(h, "stopsync");
    let during = r.serve(&|_| None);
    assert!(!during.iter().any(|c| c.starts_with("kata-set-param")), "nothing reaches the 试下's engine: {during:?}");
    let exit = parse_client_message(r#"{"type":"exit_trial","clientId":"a"}"#).unwrap();
    assert_eq!(r.wb.web_board_message(7, exit, Instant::now()), None);
    let sent = r.serve(&|_| None);
    for restore in ["kata-set-param maxTime 1e+20", "kata-set-param maxVisits 1000000"] {
        assert!(sent.iter().any(|c| c == restore), "{restore} after the 试下: {sent:?}");
    }
    assert_eq!(r.lease(), LeaseAvailabilityView::Available);
}

/// `playpon off` between two hands (Java `disableReadBoardGmaPonderIfIdle`): the engine stops
/// pondering for GMA at once; its own value goes back when GMA is cleared.
#[test]
fn unticking_background_analysis_between_gma_hands_stops_the_pondering() {
    let (mut r, h) = first_gma_hand_played(true, "true");
    r.line(h, "playpon off");
    let sent = r.serve(&|_| None);
    assert!(sent.iter().any(|c| c == "kata-set-param ponderingEnabled false"), "{sent:?}");
    assert_eq!(r.lease(), LeaseAvailabilityView::ReadBoardGma, "GMA stays armed");
    r.line(h, "stopsync");
    let sent = r.serve(&|_| None);
    assert!(sent.iter().any(|c| c == "kata-set-param ponderingEnabled true"), "the engine's own value: {sent:?}");
    assert_eq!(r.lease(), LeaseAvailabilityView::Available);
}

/// `playpon off` while GMA waits for the opponent before its first hand and an ordinary 批量分析
/// analyses on the foreground engine meanwhile. Java `disableReadBoardGmaPonderIfIdle`
/// (ReadBoard.java:4595-4605) needs no earlier hand: the engine's own value is read, pondering
/// is turned off, the running analysis stops and nothing is played; clearing GMA gives the
/// value back.
#[test]
fn unticking_background_analysis_before_the_first_gma_hand_stops_the_running_analysis() {
    let own = |c: &str| (c == "kata-get-param ponderingEnabled").then(|| "true".to_string());
    let mut r = Rig::new();
    let h = r.open();
    r.lines(h, &["ready", "start 19 19", "sync", "bothSync", "playpon on", "play>white>5 100 0 gma"]);
    r.frame(h, &[]);
    let armed = r.serve(&own);
    assert!(!armed.iter().any(|c| c.starts_with("kata-genmove_analyze") || c.contains("ponderingEnabled")), "no hand yet: {armed:?}");

    let game = r._dir.path().join("a.sgf");
    std::fs::write(&game, "(;SZ[19];B[pd];W[dd];B[pp];W[dp])").unwrap();
    let paths = vec![game.to_string_lossy().into_owned()];
    assert_eq!(r.wb.open_batch(BatchMode::Ordinary, paths, OpenSource::Chooser), Ok(true));
    for (ticket, path, no_capture) in r.wb.take_batch_reads() {
        assert!(r.wb.complete_batch_read(ticket, formats::read_file(&path, no_capture)));
    }
    let settings = AutoAnalysisSettings { visits_enabled: true, visits: 10, time_enabled: false, auto_save: false, ..Default::default() };
    assert_eq!(r.wb.start_batch_ordinary(settings).unwrap(), Ok(true));
    let batch = r.serve(&own);
    assert!(batch.iter().any(|c| c.starts_with("kata-analyze")), "the batch analyses: {batch:?}");

    r.line(h, "playpon off");
    let sent = r.serve(&own);
    let ponder: Vec<_> = sent.iter().filter(|c| c.contains("ponderingEnabled")).collect();
    assert_eq!(ponder, ["kata-get-param ponderingEnabled", "kata-set-param ponderingEnabled false"], "{sent:?}");
    assert!(r.wb.view(false).engine.paused, "the analysis stops (`stopPonderingIfActive`)");
    assert!(!sent.iter().any(|c| c.starts_with("kata-analyze") || c.starts_with("kata-genmove_analyze")), "{sent:?}");
    assert!(!r.serve(&own).iter().any(|c| c.starts_with("kata-analyze")), "the batch analyses no more");
    assert!(r.main_line().is_empty());
    assert_eq!(r.lease(), LeaseAvailabilityView::ReadBoardGma, "GMA stays armed");

    r.line(h, "stopsync");
    let sent = r.serve(&own);
    assert!(sent.iter().any(|c| c == "kata-set-param ponderingEnabled true"), "the engine's own value: {sent:?}");
    assert_eq!(r.lease(), LeaseAvailabilityView::Available);
}

// ------------------------------------------------------------------------ helper restarts

/// The helper offers the update `tag` (a valid package in the rig's directory).
fn offer_update(r: &mut Rig, helper: HelperId, tag: &str) {
    use std::io::Write;
    let zip_path = r._dir.path().join(format!("readboard-github-release-{tag}.zip"));
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).unwrap());
    for (name, content) in [
        ("readboard.exe", "new-exe"),
        ("readboard.dll", "new-dll"),
        ("readboard.runtimeconfig.json", "{}"),
        ("readboard.deps.json", "{}"),
        ("language_cn.txt", "cn"),
    ] {
        zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(content.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    r.line(helper, &format!("readboardUpdateReady\t{tag}\t{}", zip_path.display()));
    assert_eq!(r.wb.view(false).readboard.update_offer.as_deref(), Some(tag));
}

/// Java `LizzieFrame.java:2753-2793`, `2955-3007`: reopening starts the successor only after the
/// old helper's process has exited (until then it may still click the target); Opens meanwhile
/// are that one start.
#[test]
fn reopening_waits_for_the_old_helper_to_exit_and_coalesces_requests() {
    let mut r = Rig::new();
    let first = r.open();
    r.line(first, "ready");
    r.helpers.0.lock().unwrap().exiting = true;
    assert_eq!(r.wb.readboard(ReadBoardCommand::Open).unwrap(), CommandResult::Done);
    assert_eq!(r.wb.readboard(ReadBoardCommand::Open).unwrap(), CommandResult::Done);
    assert_eq!(r.helpers.sent_to(first).last().map(String::as_str), Some("quit"));
    {
        let world = r.helpers.0.lock().unwrap();
        assert_eq!(world.launched.len(), 1, "nothing starts while the old helper still runs");
        assert_eq!(world.stopped, [first], "the old helper is stopped once");
    }
    assert!(!r.wb.view(false).readboard.running);

    // The old helper's late output, even the end of its output, reaches nothing and starts nothing.
    r.frame(first, &[(3, 3, 3)]);
    assert!(!r.wb.readboard_input(first, HelperInput::Ended { error: None }));
    assert!(r.main_line().is_empty());
    assert_eq!(r.helpers.0.lock().unwrap().launched.len(), 1, "the end of its output is not its exit");

    // Its exit starts exactly one successor with a new id; a repeated report starts nothing.
    assert!(r.wb.readboard_exited(first));
    assert!(!r.wb.readboard_exited(first));
    let second = {
        let world = r.helpers.0.lock().unwrap();
        assert_eq!(world.launched.len(), 2, "the two Opens are one start");
        world.launched[1].0
    };
    assert_ne!(second, first);
    assert!(r.wb.view(false).readboard.running);
    r.line(second, "sync");
    assert!(r.wb.view(false).readboard.syncing);

    // A later reopen waits for this helper in turn.
    r.wb.readboard(ReadBoardCommand::Open).unwrap();
    assert!(!r.wb.readboard_exited(first), "only the replaced helper's exit counts");
    assert_eq!(r.helpers.0.lock().unwrap().launched.len(), 2);
    assert!(r.wb.readboard_exited(second));
    assert_eq!(r.helpers.0.lock().unwrap().launched.len(), 3);
}

/// A helper whose output ended while its process still runs (closed output, read failure) is
/// still there for a reopen: the successor waits for its exit as for a replaced helper.
#[test]
fn a_helper_whose_output_ended_is_waited_for_before_a_reopen_starts_another() {
    let mut r = Rig::new();
    let first = r.open();
    r.line(first, "ready");
    r.helpers.0.lock().unwrap().exiting = true;
    assert!(r.wb.readboard_input(first, HelperInput::Ended { error: None }));
    assert!(!r.wb.view(false).readboard.running);
    assert_eq!(r.helpers.0.lock().unwrap().launched.len(), 1, "an end restarts nothing by itself");
    r.wb.readboard(ReadBoardCommand::Open).unwrap();
    r.wb.readboard(ReadBoardCommand::Open).unwrap();
    assert_eq!(r.helpers.0.lock().unwrap().launched.len(), 1, "nothing starts while its process runs");
    assert!(r.wb.readboard_exited(first));
    assert_eq!(r.helpers.0.lock().unwrap().launched.len(), 2, "its exit starts exactly one successor");

    // Exited before anyone reopened: the next Open starts at once.
    let second = r.helpers.0.lock().unwrap().launched[1].0;
    assert!(r.wb.readboard_input(second, HelperInput::Ended { error: None }));
    assert!(!r.wb.readboard_exited(second), "no start waited");
    r.wb.readboard(ReadBoardCommand::Open).unwrap();
    assert_eq!(r.helpers.0.lock().unwrap().launched.len(), 3);
}

/// Java `beginHostedReadBoardUpdate` / `restartReadBoardAfterHostedUpdate`: the installer's
/// restart stays the only start when the replaced helper exits late and the tool is opened before
/// and after that exit.
#[test]
fn an_update_install_owns_the_restart_even_when_the_old_helper_exits_late() {
    let mut r = Rig::new();
    let first = r.open();
    r.lines(first, &["ready", "version 230616"]);
    r.helpers.0.lock().unwrap().exiting = true;
    offer_update(&mut r, first, "v3.2.1");
    r.wb.readboard(ReadBoardCommand::AnswerUpdate { accept: true }).unwrap();
    let install = r.wb.take_readboard_install().expect("an accepted update is installed");

    r.wb.readboard(ReadBoardCommand::Open).unwrap();
    assert!(!r.wb.readboard_exited(first), "the install, not the old helper's exit, restarts it");
    r.wb.readboard(ReadBoardCommand::Open).unwrap();
    assert_eq!(r.helpers.0.lock().unwrap().launched.len(), 1, "no helper starts during the install");

    let result = install.run();
    assert!(r.wb.finish_readboard_install(install, result));
    {
        let world = r.helpers.0.lock().unwrap();
        assert_eq!(world.launched.len(), 2, "the installer restarts the helper once");
        assert_ne!(world.launched[1].0, first);
    }
    let notices = r.wb.view(false).readboard.notices;
    assert!(
        notices.iter().any(|n| matches!(n, ReadBoardNotice::Message { key, .. }
            if key == "ReadBoard.updateInstallSucceeded" || key == "ReadBoard.updateInstallFailed")),
        "{notices:?}"
    );
}

/// Application exit drops a start that waits for the old helper: nothing starts afterwards.
#[test]
fn application_exit_drops_a_waiting_restart() {
    let mut r = Rig::new();
    let first = r.open();
    r.helpers.0.lock().unwrap().exiting = true;
    r.wb.readboard(ReadBoardCommand::Open).unwrap();
    r.wb.readboard_shutdown();
    assert!(!r.wb.readboard_exited(first));
    assert_eq!(r.helpers.0.lock().unwrap().launched.len(), 1);
}

// ------------------------------------------------------------------------ snapshot carry

/// A rebuild keeps the captures and the move numbers of the sync start (Java stores them in the
/// snapshot `BoardData`); later moves add to the captures and show the carried numbers.
#[test]
fn a_rebuilt_snapshot_keeps_the_captures_and_numbers_for_later_moves() {
    let mut r = Rig::new();
    let mut display = r.wb.config().display.clone();
    display.move_numbers = -1;
    r.wb.set_display_settings(display).unwrap();
    // B[1,0] W[0,0] Bx[0,1] (takes 0,0) W[18,0] B[17,0]: Black has taken one stone.
    for (x, y) in [(1, 0), (0, 0), (0, 1), (18, 0), (17, 0)] {
        r.wb.edit(lizzie_core::api::EditCommand::Play { point: p(x, y), color: None }).unwrap();
    }
    let before = r.wb.view(false).position;
    assert_eq!((before.black_captures, before.white_captures), (1, 0));

    let h = r.open();
    r.lines(h, &["ready", "start 19 19", "sync", "forceRebuild"]);
    r.frame(h, &[(1, 0, 1), (0, 1, 1), (18, 0, 2), (17, 0, 3)]);
    let rebuilt = r.wb.view(false);
    assert_eq!((rebuilt.position.node, rebuilt.position.snapshot), (ROOT, true), "the board was rebuilt");
    assert!(r.main_line().is_empty());
    assert_eq!(rebuilt.position.stones, before.stones);
    assert_eq!((rebuilt.position.black_captures, rebuilt.position.white_captures), (1, 0));
    assert!(rebuilt.board.move_numbers.is_empty(), "the snapshot itself shows no numbers");
    let marked = rebuilt.position.move_number;

    assert!(r.wb.readboard_input(h, HelperInput::Ended { error: None }));
    // White elsewhere, then Black takes the corner stone at 18,0.
    for (x, y, color) in [(10, 10, Stone::White), (18, 1, Stone::Black)] {
        r.wb.edit(lizzie_core::api::EditCommand::Play { point: p(x, y), color: Some(color) }).unwrap();
    }
    let after = r.wb.view(false);
    assert_eq!(after.position.stones[18], None, "the corner stone was taken");
    assert_eq!((after.position.black_captures, after.position.white_captures), (2, 0), "captures add up");
    let mut labels: Vec<(u16, u16, u32, bool)> =
        after.board.move_numbers.iter().map(|l| (l.point.x, l.point.y, l.number, l.last)).collect();
    labels.sort_unstable();
    let mut expected = vec![
        (0, 1, 3, false),
        (1, 0, 1, false),
        (10, 10, marked + 1, false),
        (17, 0, marked, false),
        (18, 1, marked + 2, true),
    ];
    expected.sort_unstable();
    assert_eq!(labels, expected, "the marked stone keeps its number on later positions");
}

// ------------------------------------------------------------------------ float board

/// The main line's end of the open record (first children all the way down).
fn main_end(r: &Rig) -> lizzie_core::record::NodeId {
    let rec = &r.wb.document().record;
    let mut node = ROOT;
    while let Some(c) = rec.main_child(node) {
        node = c;
    }
    node
}

fn places(r: &Rig, helper: HelperId) -> Vec<String> {
    r.helpers.sent_to(helper).into_iter().filter(|l| l.starts_with("place")).collect()
}

/// Java `dispatchPlaceCommandOffEventThread` (ReadBoard.java:6487-6510): with
/// `foreFoxWithInBoard`, a shown float board is hidden first and `place` written afterwards.
#[test]
fn a_shown_float_board_is_hidden_before_the_place_is_written_once() {
    let mut r = Rig::new();
    let h = r.open();
    r.lines(h, &["ready", "start 19 19", "sync", "bothSync", "inboard 100 100 400 400 0", "foreFoxWithInBoard"]);
    r.frame(h, &[(3, 3, 3)]);
    assert!(r.wb.view(false).readboard.float_board.is_some());
    r.wb.edit(lizzie_core::api::EditCommand::Play { point: p(15, 15), color: None }).unwrap();
    assert!(places(&r, h).is_empty(), "nothing is written while the window may still be on screen");
    let rb = r.wb.view(false).readboard;
    assert!(rb.float_board.is_none(), "the window is asked to hide");
    let request = rb.float_hide_request.expect("the place waits for the hide");

    let stale = r.wb.readboard(ReadBoardCommand::FloatBoardHidden { request: request + 1 }).unwrap();
    assert_eq!(stale, CommandResult::Unchanged);
    assert!(places(&r, h).is_empty());
    let hidden = r.wb.readboard(ReadBoardCommand::FloatBoardHidden { request }).unwrap();
    assert_eq!(hidden, CommandResult::Done);
    assert_eq!(places(&r, h), ["place 15 15"]);
    assert_eq!(r.wb.view(false).readboard.float_hide_request, None);
    let again = r.wb.readboard(ReadBoardCommand::FloatBoardHidden { request }).unwrap();
    assert_eq!(again, CommandResult::Unchanged);
    assert_eq!(places(&r, h).len(), 1, "exactly one place");
}

#[test]
fn a_hide_confirmed_after_the_helper_was_replaced_places_nothing() {
    let mut r = Rig::new();
    let first = r.open();
    r.lines(first, &["ready", "start 19 19", "sync", "bothSync", "inboard 100 100 400 400 0", "foreFoxWithInBoard"]);
    r.frame(first, &[(3, 3, 3)]);
    r.wb.edit(lizzie_core::api::EditCommand::Play { point: p(15, 15), color: None }).unwrap();
    let request = r.wb.view(false).readboard.float_hide_request.expect("the place waits for the hide");

    let second = r.open();
    assert_ne!(first, second);
    assert_eq!(r.wb.view(false).readboard.float_hide_request, None);
    let late = r.wb.readboard(ReadBoardCommand::FloatBoardHidden { request }).unwrap();
    assert_eq!(late, CommandResult::Unchanged);
    assert!(places(&r, first).is_empty(), "the retired helper never clicks");
    assert!(places(&r, second).is_empty(), "its successor never clicks a move it was not sent");
}

/// `ReadBoard.applySyncViewState` (5443-5446) and `FloatBoard.changeEetEditMode` (457-467).
#[test]
fn in_the_float_edit_mode_synced_moves_keep_the_view_and_leaving_it_shows_the_main_end() {
    let mut r = Rig::new();
    let h = r.open();
    r.lines(h, &["ready", "start 19 19", "sync", "inboard 100 100 400 400 0"]);
    r.frame(h, &[(3, 3, 3)]);
    r.frame(h, &[(3, 3, 1), (15, 15, 4)]);
    assert_eq!(r.main_line().len(), 2);
    r.wb.navigate(Navigation::Back(1));
    let viewed = r.wb.cursor();
    assert_ne!(viewed, main_end(&r));
    assert_eq!(r.wb.readboard(ReadBoardCommand::SetFloatEditMode { enabled: true }).unwrap(), CommandResult::Done);

    r.frame(h, &[(3, 3, 1), (15, 15, 2), (16, 3, 3)]);
    assert_eq!(r.main_line(), [(Stone::Black, p(3, 3)), (Stone::White, p(15, 15)), (Stone::Black, p(16, 3))]);
    assert_eq!(r.wb.cursor(), viewed, "the view stays on the user's node");

    assert_eq!(r.wb.readboard(ReadBoardCommand::SetFloatEditMode { enabled: false }).unwrap(), CommandResult::Done);
    assert_eq!(r.wb.cursor(), main_end(&r), "leaving the edit mode shows the main line's end");
}

/// `BoardHistoryList.getCurOrMainEnd` (639-642) as `FloatBoardRenderer.prepareSelection` uses it.
#[test]
fn outside_the_edit_mode_the_float_board_draws_the_main_line_end() {
    let mut r = Rig::new();
    let h = r.open();
    r.lines(h, &["ready", "start 19 19", "sync", "inboard 100 100 400 400 0"]);
    r.frame(h, &[(3, 3, 3)]);
    r.frame(h, &[(3, 3, 1), (15, 15, 4)]);
    r.wb.navigate(Navigation::Back(1));
    let view = r.wb.view(false);
    assert_ne!(view.position.node, main_end(&r));
    let scene = view.readboard.float_board.and_then(|f| f.scene).expect("the overlay's node is projected");
    assert_eq!(scene.node, main_end(&r));
    assert_eq!(scene.to_play, Stone::Black);
    assert_eq!(scene.last_action.and_then(|a| a.point), Some(p(15, 15)));
    assert_eq!(scene.stones[15 * 19 + 15], Some(Stone::White));
    assert_eq!(view.position.stones[15 * 19 + 15], None, "the main window shows the earlier node");

    r.wb.readboard(ReadBoardCommand::SetFloatEditMode { enabled: true }).unwrap();
    let edit = r.wb.view(false).readboard.float_board.expect("still shown");
    assert!(edit.scene.is_none(), "the edit mode draws the current position");
}

// ------------------------------------------------------------------------ diagnostics

/// 导出诊断包 carries Java's seven sync snapshot entries, the helper's logging state and its
/// version; the protocol log keeps the latest 100 lines, payload lines only their command.
#[test]
fn the_diagnostics_package_holds_the_recent_sync_protocol_and_decisions() {
    use lizzie_core::readboard::diagnostics::{DECISION_CAPACITY, PROTOCOL_CAPACITY, SNAPSHOT_ENTRIES};
    let mut r = Rig::new();
    let h = r.open();
    r.lines(h, &["ready", "version 230616", "roomToken secret-room", "start 19 19", "sync", "forceRebuild"]);
    // Two new stones are no single move: the frame goes to recovery, which rebuilds.
    r.frame(h, &[(3, 3, 1), (15, 15, 2)]);
    for i in 0..120 {
        r.line(h, &format!("unknownLine{i}"));
    }
    let d = r.wb.readboard_diagnostics("9.9.9");
    assert_eq!(d.helper_version.as_deref(), Some("230616"));
    assert_eq!(d.snapshots.iter().map(|(n, _)| *n).collect::<Vec<_>>(), SNAPSHOT_ENTRIES);
    let entry = |name: &str| d.snapshots.iter().find(|(n, _)| *n == name).unwrap().1.clone();
    let protocol = entry("readboard-protocol.log");
    assert_eq!(protocol.lines().count(), PROTOCOL_CAPACITY);
    assert!(protocol.lines().last().unwrap().ends_with("\tunknownLine119"));
    let decisions = entry("recent-decisions.jsonl");
    assert!((1..=DECISION_CAPACITY).contains(&decisions.lines().count()));
    assert!(decisions.contains("\"FORCE_REBUILD\"") && decisions.contains("remote_force_rebuild_requested"), "{decisions}");
    assert!(entry("summary.txt").contains("  attached: true\n"));
    assert!(entry("environment.txt").starts_with("appVersion: 9.9.9\n"));
    let observed: serde_json::Value = serde_json::from_str(&d.observed).unwrap();
    assert_eq!(observed["attached"], true);

    // A payload line keeps only its command.
    let mut early = Rig::new();
    let h = early.open();
    early.line(h, "roomToken secret-room");
    let protocol = early.wb.readboard_diagnostics("9.9.9").snapshots[4].1.clone();
    assert!(protocol.ends_with("\troomToken <redacted>\n") && !protocol.contains("secret-room"), "{protocol}");
}

// ------------------------------------------------------------------------ custom WebSocket engine
// Java `ReadBoard.scheduleReadBoardGmaIfNeeded` (ReadBoard.java:4893-4922),
// `Leelaz.supportsReadBoardGmaPondering` (20561-20564), `ReadBoardGmaPreparation.start`
// (20195-20201) and `Config.suppressReadBoardWebSocketPonderingNotice` (3576-3580).

fn genmove_held(r: &Rig) -> bool {
    r.held.iter().any(|(_, c)| c.starts_with("kata-genmove_analyze B"))
}

#[test]
fn a_custom_websocket_engine_plays_gma_without_the_pondering_parameter_it_lacks() {
    let mut r = websocket_rig();
    arm_gma(&mut r, false);
    let sent = r.serve(&websocket_adapter);
    assert!(!sent.iter().any(|c| c.contains("ponderingEnabled")), "{sent:?}");
    assert!(sent.iter().any(|c| c == "kata-set-param maxTime 5"), "{sent:?}");
    assert!(genmove_held(&r), "{:?}", r.held);
    assert_eq!(pondering_notice(&mut r), None, "nothing to explain without the wish to ponder");
    r.answer_genmove("Q16");
    r.serve(&websocket_adapter);
    assert_eq!(r.main_line(), [(Stone::Black, p(15, 3))]);
}

#[test]
fn a_local_katago_ponders_for_gma_without_the_notice() {
    let mut r = Rig::new();
    arm_gma(&mut r, true);
    let sent = r.serve(&|_| None);
    assert!(sent.iter().any(|c| c == "kata-set-param ponderingEnabled true"), "{sent:?}");
    assert!(genmove_held(&r), "{:?}", r.held);
    assert_eq!(pondering_notice(&mut r), None);
}

#[test]
fn the_pondering_wish_on_a_websocket_engine_is_explained_once_per_arming_before_gma_plays() {
    let mut r = websocket_rig();
    let h = arm_gma(&mut r, true);
    let sent = r.serve(&websocket_adapter);
    assert!(!sent.iter().any(|c| c.starts_with("kata-get-param")) && !genmove_held(&r), "GMA waits: {sent:?}");
    let first = pondering_notice(&mut r).expect("the notice is shown");

    // Closed: GMA keeps waiting for this arming, and the notice is not shown again.
    answer(&mut r, first, PonderingNoticeDecision::Dismiss);
    r.frame(h, &[]);
    let sent = r.serve(&websocket_adapter);
    assert!(!sent.iter().any(|c| c.starts_with("kata-get-param")) && !genmove_held(&r), "{sent:?}");
    assert_eq!(pondering_notice(&mut r), None);

    // Armed again: asked again. 确定 lets this arming play, without pondering.
    r.line(h, "play>black>5 100 0 gma");
    r.frame(h, &[]);
    let second = pondering_notice(&mut r).expect("asked again for the new arming");
    assert_ne!(second, first);
    answer(&mut r, first, PonderingNoticeDecision::Confirm);
    assert_eq!(pondering_notice(&mut r), Some(second), "an answer for an earlier arming changes nothing");
    answer(&mut r, second, PonderingNoticeDecision::Confirm);
    assert_eq!(pondering_notice(&mut r), None);
    let sent = r.serve(&websocket_adapter);
    assert!(!sent.iter().any(|c| c.contains("ponderingEnabled")), "{sent:?}");
    assert!(genmove_held(&r), "{:?}", r.held);
    assert!(!r.wb.config().read_board.suppress_websocket_pondering_notice, "确定 saves nothing");
    r.answer_genmove("Q16");
    r.serve(&websocket_adapter);
    assert_eq!(r.main_line(), [(Stone::Black, p(15, 3))]);

    // The same arming's next hand does not ask.
    r.frame(h, &[(15, 3, 3)]);
    r.frame(h, &[(15, 3, 1), (3, 3, 4)]);
    r.serve(&websocket_adapter);
    assert_eq!(pondering_notice(&mut r), None);
    assert!(genmove_held(&r), "{:?}", r.held);
}

#[test]
fn not_showing_the_pondering_notice_again_is_saved() {
    let mut r = websocket_rig();
    let h = arm_gma(&mut r, true);
    r.serve(&websocket_adapter);
    let generation = pondering_notice(&mut r).expect("the notice is shown");
    answer(&mut r, generation, PonderingNoticeDecision::Suppress);
    assert!(r.wb.config().read_board.suppress_websocket_pondering_notice);
    let reopened = Workbench::new(
        Storage::in_dirs(&r._dir.path().join("config"), &r._dir.path().join("data")),
        None,
        Box::new(Launch(Engines::default())),
        vec![],
    );
    assert!(reopened.config().read_board.suppress_websocket_pondering_notice, "saved for the next start");
    let sent = r.serve(&websocket_adapter);
    assert!(!sent.iter().any(|c| c.contains("ponderingEnabled")) && genmove_held(&r), "{sent:?}");
    r.answer_genmove("Q16");
    r.serve(&websocket_adapter);

    // Armed again, it plays at once.
    r.frame(h, &[(15, 3, 3)]);
    r.line(h, "play>black>5 100 0 gma");
    r.frame(h, &[(15, 3, 1), (3, 3, 4)]);
    r.serve(&websocket_adapter);
    assert_eq!(pondering_notice(&mut r), None);
    assert!(genmove_held(&r), "{:?}", r.held);
}

#[test]
fn the_java_choice_not_to_show_the_pondering_notice_is_imported() {
    let mut r = websocket_rig();
    let java = r._dir.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), r#"{"ui": {"suppress-readboard-websocket-pondering-notice": true}}"#).unwrap();
    let preview = r.wb.preview_java_import(java_import::read(&JavaSource::new(&java, None)).unwrap());
    let item = preview.items.iter().find(|i| i.key == "ui.suppress-readboard-websocket-pondering-notice").expect("listed");
    assert_eq!((item.status, item.target), (ImportStatus::Imported, Some(ImportTarget::ReadBoardPonderingNotice)));
    r.wb.apply_java_import(preview.id, &[], false, false, true, None).unwrap().unwrap();
    assert!(r.wb.config().read_board.suppress_websocket_pondering_notice);

    r.engine = r.engines.last();
    r.serve(&websocket_adapter);
    arm_gma(&mut r, true);
    r.serve(&websocket_adapter);
    assert_eq!(pondering_notice(&mut r), None);
    assert!(genmove_held(&r), "{:?}", r.held);
}

/// A Java settings import reaches the running helper session at once: with 焦点归还 imported
/// off, the float board no longer gives the focus back.
#[test]
fn a_java_import_reaches_the_running_helper_session() {
    let mut r = Rig::new();
    let h = r.open();
    r.lines(h, &["ready", "start 19 19", "sync"]);
    r.serve(&|_| None);
    assert!(r.wb.config().read_board.get_focus, "焦点归还 is on");
    assert_eq!(r.wb.readboard(ReadBoardCommand::ReturnFocus).unwrap(), CommandResult::Done);
    assert_eq!(r.helpers.sent_to(h).last().map(String::as_str), Some("loss"), "the float board gives the focus back");

    let java = r._dir.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), r#"{"ui": {"read-board-get-focus": false}}"#).unwrap();
    let preview = r.wb.preview_java_import(java_import::read(&JavaSource::new(&java, None)).unwrap());
    r.wb.apply_java_import(preview.id, &[], false, false, true, None).unwrap().unwrap();
    r.engine = r.engines.last();
    r.serve(&|_| None);
    assert!(!r.wb.config().read_board.get_focus);
    let before = r.helpers.sent_to(h).len();
    assert_eq!(r.wb.readboard(ReadBoardCommand::ReturnFocus).unwrap(), CommandResult::Done);
    let sent = r.helpers.sent_to(h)[before..].to_vec();
    assert!(!sent.iter().any(|l| l == "loss"), "the running helper follows the imported setting: {sent:?}");
}

// ------------------------------------------------------------------------ 弈客 web window

const YIKE_ROOM: &str = "https://home.yikeweiqi.com/#/live/new-room/186031/0/0";

fn page_reloads(r: &mut Rig) -> Vec<String> {
    r.wb.take_online_jobs()
        .into_iter()
        .filter_map(|j| match j {
            OnlineJob::ReloadPage { url } => Some(url),
            _ => None,
        })
        .collect()
}

/// The helper's 弈客 signals drive the web window's sync (`handleYikeSyncStartCommand` /
/// `handleYikeSyncStopCommand`): `syncPlatform yike` and `yikeSyncStart` follow the room the page
/// shows, reloading it, unless that room is followed already; `yikeSyncStop` ends the sync.
#[test]
fn the_helpers_yike_signals_start_and_stop_the_web_window_sync() {
    let mut r = Rig::new();
    let h = r.open();
    r.line(h, "ready");
    r.line(h, "yikeSyncStart");
    assert!(r.wb.view(true).online.following.is_none(), "no page address, nothing to follow");
    r.wb.online_page(PageReport::Address { url: YIKE_ROOM.into() });
    let o = r.wb.view(true).online;
    assert!(!o.browser_sync && o.following.is_none(), "showing a room alone does not follow it");
    page_reloads(&mut r);

    r.line(h, "syncPlatform yike");
    let o = r.wb.view(true).online;
    assert!(o.browser_sync, "the window follows room addresses from now on");
    assert_eq!(o.following.map(|f| f.url).as_deref(), Some(YIKE_ROOM));
    assert_eq!(page_reloads(&mut r), [YIKE_ROOM]);
    r.line(h, "yikeSyncStart");
    assert_eq!(page_reloads(&mut r), Vec::<String>::new(), "the room is followed already");
    assert!(r.wb.view(true).online.following.is_some());

    r.line(h, "yikeSyncStop");
    let o = r.wb.view(true).online;
    assert!(!o.browser_sync && o.following.is_none(), "the helper's stop ends the web window's sync");
}

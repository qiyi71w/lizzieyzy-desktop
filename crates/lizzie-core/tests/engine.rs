//! Engines through the public `Workbench` operations with a controlled GTP process: start-up and
//! identification, command confirmation, position sync, analysis ownership (late results,
//! another document), pause, limits, switching, failure and exit.

use std::sync::{Arc, Mutex};

use lizzie_core::api::{AnalysisScope, EditCommand, EngineNotice, EngineScope, KataEngineParamsEdit, KataParamEdit};
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{
    AnalysisSettings, CustomPdaMode, CustomPdaSettings, EngineProfile, EngineStartup, LeelaOptionSetting,
    LeelaOptionSettings,
};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, KataParam, Launcher};
use lizzie_core::formats::sgf;
use lizzie_core::record::{Point, RecordFormat, Stone};
use lizzie_core::settings::Storage;
use lizzie_core::workbench::{OpenOrigin, Workbench};

#[derive(Default)]
struct Proc {
    sent: Vec<String>,
    terminated: bool,
    args: Vec<String>,
}

#[derive(Clone, Default)]
struct World(Arc<Mutex<Vec<(InstanceId, String, Proc)>>>);

impl World {
    fn sent(&self, id: InstanceId) -> Vec<String> {
        let w = self.0.lock().unwrap();
        w.iter()
            .find(|p| p.0 == id)
            .map(|p| p.2.sent.clone())
            .unwrap_or_default()
    }
    fn take(&self, id: InstanceId) -> Vec<String> {
        let mut w = self.0.lock().unwrap();
        w.iter_mut()
            .find(|p| p.0 == id)
            .map(|p| std::mem::take(&mut p.2.sent))
            .unwrap_or_default()
    }
    fn launched(&self) -> Vec<(InstanceId, String)> {
        self.0.lock().unwrap().iter().map(|p| (p.0, p.1.clone())).collect()
    }
    fn terminated(&self, id: InstanceId) -> bool {
        self.0.lock().unwrap().iter().any(|p| p.0 == id && p.2.terminated)
    }
}

struct FakeIo {
    id: InstanceId,
    world: World,
}

impl EngineIo for FakeIo {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        let mut w = self.world.0.lock().unwrap();
        w.iter_mut()
            .find(|p| p.0 == self.id)
            .unwrap()
            .2
            .sent
            .push(line.to_string());
        Ok(())
    }
    fn terminate(self: Box<Self>) {
        let mut w = self.world.0.lock().unwrap();
        w.iter_mut().find(|p| p.0 == self.id).unwrap().2.terminated = true;
    }
}

struct FakeLauncher(World);

impl Launcher for FakeLauncher {
    fn launch(&self, id: InstanceId, spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        if spec.program.contains("missing") {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No such file or directory",
            ));
        }
        self.0 .0.lock().unwrap().push((
            id,
            spec.program.clone(),
            Proc {
                args: spec.args.clone(),
                ..Proc::default()
            },
        ));
        Ok(Box::new(FakeIo {
            id,
            world: self.0.clone(),
        }))
    }
}

fn storage(dir: &std::path::Path) -> Storage {
    Storage::in_dirs(&dir.join("config"), &dir.join("data"))
}

fn profile(name: &str, program: &str) -> EngineProfile {
    EngineProfile {
        name: name.into(),
        command: format!("{program} gtp"),
        ..EngineProfile::default()
    }
}

fn workbench(dir: &std::path::Path, world: &World) -> Workbench {
    let mut wb = Workbench::new(storage(dir), None, Box::new(FakeLauncher(world.clone())), Vec::new());
    // About the foreground engine; automatic quick analysis has its own tests.
    wb.set_auto_quick_analyze(false).unwrap();
    wb
}

/// One stdout line from instance `id`.
fn out(wb: &mut Workbench, id: InstanceId, line: &str) {
    wb.engine_input(id, EngineInput::Stdout(line.to_string()));
}

/// Answers command number `n` with `text`.
fn answer(wb: &mut Workbench, id: InstanceId, n: u64, text: &str) {
    out(wb, id, &format!("={n} {text}"));
    out(wb, id, "");
}

fn number(line: &str) -> u64 {
    line.split(' ').next().unwrap().parse().unwrap()
}

/// Answers every outstanding command of `id` except analysis requests, which get only their
/// header (their stream stays open). Returns the commands answered (without numbers).
fn answer_all(wb: &mut Workbench, world: &World, id: InstanceId, list_commands: &str) -> Vec<String> {
    let mut done = Vec::new();
    loop {
        let pending = world.take(id);
        if pending.is_empty() {
            return done;
        }
        for line in pending {
            // Reading a new command ends an analysis stream with a blank line (KataGo, lz).
            out(wb, id, "");
            let n = number(&line);
            let cmd = line.split_once(' ').unwrap().1.to_string();
            match cmd.split(' ').next().unwrap() {
                "name" => answer(wb, id, n, "KataGo"),
                "version" => answer(wb, id, n, "1.16.4"),
                "list_commands" => answer(wb, id, n, list_commands),
                "kata-get-rules" => answer(
                    wb,
                    id,
                    n,
                    r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#,
                ),
                "kata-analyze" | "lz-analyze" => out(wb, id, &format!("={n}")),
                _ => answer(wb, id, n, ""),
            }
            done.push(cmd);
        }
    }
}

const KATA: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nset_position\nkata-analyze\nkata-set-rules\nkata-get-rules\nstop";

fn open(wb: &mut Workbench, text: &str) {
    let t = wb.begin_open(OpenOrigin::File {
        path: "/x.sgf".into(),
        source: lizzie_core::api::OpenSource::Drop,
    });
    assert!(wb.complete_open(t, Ok((RecordFormat::Sgf, sgf::read(text).unwrap()))));
}

fn info(mv: &str, visits: u64) -> String {
    format!("info move {mv} visits {visits} winrate 0.6 scoreLead 1.5 prior 0.2 order 0 pv {mv} D4 rootInfo visits {visits}")
}

/// A configured KataGo, started as the foreground engine and answered through start-up.
fn started(dir: &std::path::Path, world: &World) -> (Workbench, InstanceId) {
    started_listing(dir, world, KATA)
}

/// The same with `list` as its `list_commands` answer.
fn started_listing(dir: &std::path::Path, world: &World, list: &str) -> (Workbench, InstanceId) {
    let mut wb = workbench(dir, world);
    wb.save_engine_settings(vec![profile("KataGo", "/opt/katago")], EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let id = world.launched()[0].0;
    answer_all(&mut wb, world, id, list);
    (wb, id)
}

fn analysis_settings(wb: &mut Workbench, change: impl FnOnce(&mut AnalysisSettings)) {
    let mut settings = wb.config().engine.analysis.clone();
    change(&mut settings);
    wb.set_analysis_settings(settings).unwrap();
}

#[test]
fn start_up_identifies_then_syncs_and_analyses_the_current_position() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_engine_settings(vec![profile("Kata", "/opt/katago")], EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let id = world.launched()[0].0;
    assert_eq!(world.sent(id), ["1 name", "2 version", "3 list_commands"]);
    let v = wb.view(false);
    let fg = v.engine.foreground.expect("foreground while loading");
    assert!(!fg.ready, "not complete before identification and sync");

    let done = answer_all(&mut wb, &world, id, KATA);
    assert_eq!(
        done,
        [
            "name",
            "version",
            "list_commands",
            "komi 7.5",
            "boardsize 19",
            "kata-get-rules",
            // 选点重点研究's capability probe, stopped before the engine counts as started.
            "kata-analyze 10 rootInfo true focus pass 0",
            "stop",
            "boardsize 19",
            "clear_board",
            "komi 7.5",
            "kata-analyze B 10 rootInfo true",
        ]
    );
    let v = wb.view(false);
    let fg = v.engine.foreground.unwrap();
    assert!(fg.ready && fg.katago && fg.can_analyse);
    assert_eq!(fg.gtp_name, "KataGo");
    assert!(v.engine.analysing);
    assert_eq!(v.engine.rules_key.as_deref(), Some("LizzieFrame.currentRules.chinese"));
}

#[test]
fn showing_pv_visits_asks_katago_for_them_again() {
    // Leelaz.java:20069: ` pvVisits true` while 显示 PV 访问次数 is on.
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    world.take(id);
    let mut s = wb.config().display.clone();
    s.pv_visits = lizzie_core::settings::PvVisitsMode::AllMoves;
    wb.set_display_settings(s.clone()).unwrap();
    let sent = answer_all(&mut wb, &world, id, KATA);
    assert_eq!(sent, ["kata-analyze B 10 rootInfo true pvVisits true"]);
    out(&mut wb, id, "info move Q16 visits 30 winrate 0.6 scoreLead 1.5 prior 0.2 order 0 pv Q16 D4 pvVisits 30 12 rootInfo visits 30");
    assert_eq!(wb.view(false).analysis.unwrap().candidates[0].pv_visits, [30, 12]);
    s.pv_visits = lizzie_core::settings::PvVisitsMode::Off;
    wb.set_display_settings(s).unwrap();
    assert_eq!(answer_all(&mut wb, &world, id, KATA), ["kata-analyze B 10 rootInfo true"]);
}

#[test]
fn results_belong_to_the_position_they_were_requested_for() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    open(&mut wb, "(;SZ[19];B[pd];W[dd];B[pp])");
    answer_all(&mut wb, &world, id, KATA);
    let root_request = wb.view(false);
    assert!(root_request.engine.analysing);
    let req_root = number(
        world
            .sent(id)
            .iter()
            .rev()
            .find(|l| l.contains("kata-analyze"))
            .map_or("0", |s| s.as_str()),
    );
    let _ = req_root;

    // Analysis of the root arrives; then the user steps forward twice quickly.
    out(&mut wb, id, &info("Q16", 50));
    assert_eq!(wb.view(false).analysis.unwrap().visits, 50);
    wb.navigate(Navigation::Forward(1));
    // The first step's batch is in flight; the second step waits and replaces nothing yet.
    let first = world.take(id);
    assert_eq!(
        first.iter().map(|l| l.split_once(' ').unwrap().1).collect::<Vec<_>>(),
        ["play B Q16", "kata-analyze W 10 rootInfo true"]
    );
    wb.navigate(Navigation::Forward(1));
    assert!(
        world.sent(id).is_empty(),
        "nothing more is sent while a batch is unanswered"
    );
    assert!(
        wb.view(false).engine.analysing,
        "the shown position is being analysed while the batch before it is unanswered"
    );

    // A late line of the root request (still streaming before it read `play`) is history for
    // the root, not analysis of the shown node.
    out(&mut wb, id, &info("D4", 80));
    let v = wb.view(false);
    assert!(v.analysis.is_none(), "the shown node has no result of its own yet");
    assert!(
        v.history.iter().any(|p| p.node == 0 && p.visits == 80),
        "root history keeps the late result"
    );

    // The engine reads `play` (ending the root stream), answers it and reaches the first step's
    // request: then one batch goes to the latest position.
    out(&mut wb, id, "");
    answer(&mut wb, id, number(&first[0]), "");
    out(&mut wb, id, &format!("={}", number(&first[1])));
    let second = world.take(id);
    assert_eq!(
        second.iter().map(|l| l.split_once(' ').unwrap().1).collect::<Vec<_>>(),
        ["play W D16", "kata-analyze B 10 rootInfo true"]
    );
    out(&mut wb, id, "");
    answer(&mut wb, id, number(&second[0]), "");
    out(&mut wb, id, &format!("={}", number(&second[1])));
    out(&mut wb, id, &info("Q4", 30));
    let v = wb.view(false);
    assert_eq!(v.position.move_number, 2);
    assert_eq!(v.analysis.unwrap().candidates[0].visits, 30);

    // Going back undoes instead of rebuilding.
    wb.navigate(Navigation::Back(2));
    let back = world.take(id);
    assert_eq!(
        back.iter().map(|l| l.split_once(' ').unwrap().1).collect::<Vec<_>>(),
        ["undo", "undo", "kata-analyze B 10 rootInfo true"]
    );
    // Stored root analysis (80 visits) is shown again without waiting for the engine.
    assert_eq!(wb.view(false).analysis.unwrap().visits, 80);
}

#[test]
fn fewer_visits_do_not_replace_stored_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    out(&mut wb, id, &info("Q16", 200));
    out(&mut wb, id, &info("D4", 20));
    assert_eq!(wb.view(false).analysis.unwrap().visits, 200);
}

#[test]
fn another_documents_late_results_are_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    open(&mut wb, "(;SZ[9];B[ee])");
    // The 19x19 request was still streaming: its lines must not reach the 9x9 game.
    out(&mut wb, id, &info("Q16", 90));
    let v = wb.view(false);
    assert!(v.analysis.is_none() && v.history.is_empty());
    let sync = answer_all(&mut wb, &world, id, KATA);
    assert_eq!(&sync[..3], ["boardsize 9", "clear_board", "komi 7.5"]);
}

#[test]
fn pause_stops_keeps_results_and_is_not_remembered() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    out(&mut wb, id, &info("Q16", 40));
    wb.toggle_analysis().unwrap();
    let stop = world.take(id);
    assert_eq!(stop.len(), 1);
    assert!(stop[0].ends_with(" stop"));
    let v = wb.view(false);
    assert!(v.engine.paused && !v.engine.analysing);
    assert_eq!(v.analysis.unwrap().visits, 40, "paused display keeps stored analysis");

    // While paused, browsing still syncs the board but does not analyse.
    answer_all(&mut wb, &world, id, KATA);
    wb.end_engines();
    let mut fresh = workbench(dir.path(), &world);
    fresh.start_engines();
    assert!(!fresh.view(false).engine.paused, "a new session analyses again");
}

#[test]
fn an_analysis_limit_stops_this_position_once_and_navigation_resumes() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    // A time limit of 0 is Java's 9999 minutes, not "stop at once".
    let mut settings = wb.config().engine.analysis.clone();
    settings.max_analyze_time_seconds = 0;
    wb.set_analysis_settings(settings.clone()).unwrap();
    open(&mut wb, "(;B[pd];W[dd])");
    answer_all(&mut wb, &world, id, KATA);
    out(&mut wb, id, &info("Q16", 5));
    assert!(
        !wb.view(false).engine.stopped_by_limit,
        "a zero time limit does not stop"
    );
    settings.limit_playout = true;
    settings.limit_playouts = 6;
    wb.set_analysis_settings(settings).unwrap();
    out(&mut wb, id, &info("Q16", 7));
    let v = wb.view(false);
    assert!(v.engine.stopped_by_limit && !v.engine.paused);
    assert_eq!(v.engine_notice, Some(EngineNotice::StoppedByLimit));
    let stop = world.take(id);
    assert!(stop.len() == 1 && stop[0].ends_with(" stop"), "{stop:?}");
    out(&mut wb, id, "");
    answer(&mut wb, id, number(&stop[0]), "");
    wb.navigate(Navigation::Forward(1));
    let resumed = answer_all(&mut wb, &world, id, KATA);
    assert!(resumed.iter().any(|c| c.starts_with("kata-analyze")), "{resumed:?}");
    out(&mut wb, id, &info("D4", 5));
    assert_eq!(
        wb.view(false).engine_notice,
        None,
        "the notice is shown once per engine start"
    );
}

fn analyses(sent: &[String]) -> bool {
    sent.iter().any(|c| c.starts_with("kata-analyze"))
}

#[test]
fn a_raised_or_lifted_limit_resumes_the_analysis_it_stopped() {
    // Menu.reCalculateLeelazPonderingIfOutOfLimit (Menu.java:10385-10398).
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    analysis_settings(&mut wb, |s| {
        s.limit_playout = true;
        s.limit_playouts = 6;
    });
    open(&mut wb, "(;B[pd];W[dd])");
    answer_all(&mut wb, &world, id, KATA);
    out(&mut wb, id, &info("Q16", 7));
    assert!(wb.view(false).engine.stopped_by_limit);
    assert_eq!(answer_all(&mut wb, &world, id, KATA), ["stop"]);
    // A change that leaves the search past its limits keeps it stopped.
    analysis_settings(&mut wb, |s| s.max_analyze_time_seconds = 300);
    assert!(wb.view(false).engine.stopped_by_limit);
    assert_eq!(answer_all(&mut wb, &world, id, KATA), Vec::<String>::new());
    // A higher visit limit: it goes on here, and stops at the new limit.
    analysis_settings(&mut wb, |s| s.limit_playouts = 10);
    let v = wb.view(false);
    assert!(!v.engine.stopped_by_limit && !v.engine.paused);
    assert!(analyses(&answer_all(&mut wb, &world, id, KATA)));
    out(&mut wb, id, &info("Q16", 11));
    assert!(wb.view(false).engine.stopped_by_limit);
    assert_eq!(answer_all(&mut wb, &world, id, KATA), ["stop"]);
    // No visit limit at all.
    analysis_settings(&mut wb, |s| s.limit_playout = false);
    assert!(!wb.view(false).engine.stopped_by_limit);
    assert!(analyses(&answer_all(&mut wb, &world, id, KATA)));
    out(&mut wb, id, &info("Q16", 500));
    assert!(!wb.view(false).engine.stopped_by_limit);

    // 空棋盘停止计算 is no limit a new limit lifts.
    analysis_settings(&mut wb, |s| s.stop_at_empty_board = true);
    open(&mut wb, "(;SZ[19])");
    answer_all(&mut wb, &world, id, KATA);
    out(&mut wb, id, &info("Q16", 5));
    assert!(wb.view(false).engine.stopped_by_limit);
    answer_all(&mut wb, &world, id, KATA);
    analysis_settings(&mut wb, |s| s.limit_playouts = 1000);
    assert!(wb.view(false).engine.stopped_by_limit);
    assert_eq!(answer_all(&mut wb, &world, id, KATA), Vec::<String>::new());
}

#[test]
fn a_time_limit_counts_from_when_the_stopped_search_began() {
    // `System.currentTimeMillis() - getStartPonderTime()`: the stopped time counts too.
    use std::time::{Duration, Instant};
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    analysis_settings(&mut wb, |s| {
        s.limit_time = true;
        s.max_analyze_time_seconds = 1;
    });
    // The search begins between `asked` and `sent`.
    let asked = Instant::now();
    open(&mut wb, "(;B[pd];W[dd])");
    answer_all(&mut wb, &world, id, KATA);
    let sent = Instant::now();
    std::thread::sleep((sent + Duration::from_millis(1100)).saturating_duration_since(Instant::now()));
    out(&mut wb, id, &info("Q16", 5));
    assert!(wb.view(false).engine.stopped_by_limit);
    // The search ran for at most `ran` before it stopped. A limit above that but below the time
    // since it began, stopped time included, keeps it stopped.
    let ran = asked.elapsed();
    answer_all(&mut wb, &world, id, KATA);
    analysis_settings(&mut wb, |s| s.limit_playouts = 50);
    assert!(wb.view(false).engine.stopped_by_limit, "still past one second");
    let limit = ran.as_secs() as u32 + 2;
    let past = sent + Duration::from_secs(u64::from(limit)) + Duration::from_millis(300);
    std::thread::sleep(past.saturating_duration_since(Instant::now()));
    analysis_settings(&mut wb, |s| s.max_analyze_time_seconds = limit);
    assert!(wb.view(false).engine.stopped_by_limit, "{limit} s: the time it was stopped counts");
    assert_eq!(answer_all(&mut wb, &world, id, KATA), Vec::<String>::new());
    analysis_settings(&mut wb, |s| s.max_analyze_time_seconds = 600);
    assert!(!wb.view(false).engine.stopped_by_limit);
    assert!(analyses(&answer_all(&mut wb, &world, id, KATA)));
    // Unchecking the time limit resumes as well.
    analysis_settings(&mut wb, |s| s.max_analyze_time_seconds = 1);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    out(&mut wb, id, &info("Q16", 9));
    assert!(wb.view(false).engine.stopped_by_limit);
    answer_all(&mut wb, &world, id, KATA);
    analysis_settings(&mut wb, |s| s.limit_time = false);
    assert!(!wb.view(false).engine.stopped_by_limit);
    assert!(analyses(&answer_all(&mut wb, &world, id, KATA)));
}

#[test]
fn analyze_sides_judge_the_users_moves_and_browsing_but_not_opening() {
    // Leelaz.playMove / undo (Leelaz.java:19891-19911, 22923-22936) behind every move and
    // browsing step; Menu.java:9754-9793 for the check boxes.
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    open(&mut wb, "(;B[pd];W[dd];B[pp])");
    assert!(analyses(&answer_all(&mut wb, &world, id, KATA)));
    let held = |wb: &Workbench| {
        let e = wb.inspect().engine;
        !e.analysing && !e.paused && !e.stopped_by_limit
    };

    // Black is to move: unchecking White changes nothing now.
    assert!(wb.set_analyze_sides(true, false));
    let v = wb.view(false).engine;
    assert!(v.analyze_black && !v.analyze_white && v.analysing);
    assert_eq!(answer_all(&mut wb, &world, id, KATA), Vec::<String>::new());

    // A step forward to White's turn waits there; the next one resumes.
    wb.navigate(Navigation::Forward(1));
    assert!(!analyses(&answer_all(&mut wb, &world, id, KATA)));
    assert!(held(&wb));
    wb.navigate(Navigation::Forward(1));
    assert!(analyses(&answer_all(&mut wb, &world, id, KATA)));
    // 悔棋 back to White's turn waits; Space resumes it.
    wb.navigate(Navigation::Back(1));
    assert!(!analyses(&answer_all(&mut wb, &world, id, KATA)));
    assert!(held(&wb));
    wb.toggle_analysis().unwrap();
    assert!(analyses(&answer_all(&mut wb, &world, id, KATA)));
    assert!(!wb.view(false).engine.paused);

    // Black's new move leaves White to move.
    wb.navigate(Navigation::First);
    answer_all(&mut wb, &world, id, KATA);
    wb.edit(EditCommand::Play { point: Point { x: 2, y: 2 }, color: None }).unwrap();
    assert!(!analyses(&answer_all(&mut wb, &world, id, KATA)));
    assert!(held(&wb));
    // Checking White again resumes at once; unchecking it stops at once.
    wb.set_analyze_sides(true, true);
    assert!(analyses(&answer_all(&mut wb, &world, id, KATA)));
    wb.set_analyze_sides(true, false);
    assert!(answer_all(&mut wb, &world, id, KATA).iter().any(|c| c == "stop"));
    assert!(held(&wb));
    // Any browsing judges, a jump in the tree as well.
    wb.goto_node(lizzie_core::record::ROOT).unwrap();
    assert!(analyses(&answer_all(&mut wb, &world, id, KATA)));
    wb.goto_move_number(1);
    assert!(!analyses(&answer_all(&mut wb, &world, id, KATA)));
    assert!(held(&wb));

    // A paused analysis stays paused, and Space then analyses this side anyway.
    wb.navigate(Navigation::First);
    answer_all(&mut wb, &world, id, KATA);
    wb.toggle_analysis().unwrap();
    answer_all(&mut wb, &world, id, KATA);
    wb.navigate(Navigation::Forward(1));
    answer_all(&mut wb, &world, id, KATA);
    assert!(wb.view(false).engine.paused);
    wb.toggle_analysis().unwrap();
    assert!(analyses(&answer_all(&mut wb, &world, id, KATA)));

    // Opening a record is no step: its White to move is analysed.
    open(&mut wb, "(;PL[W])");
    assert!(analyses(&answer_all(&mut wb, &world, id, KATA)));
    assert!(wb.view(false).engine.analysing);

    // A new session analyses both sides.
    wb.end_engines();
    let mut fresh = workbench(dir.path(), &world);
    let v = fresh.view(false).engine;
    assert!(v.analyze_black && v.analyze_white);
}

#[test]
fn switching_keeps_the_previous_engine_and_reuses_it() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_engine_settings(
        vec![profile("A", "/opt/a"), profile("B", "/opt/b")],
        EngineStartup::Default,
    )
    .unwrap();
    wb.start_engines();
    let a = world.launched()[0].0;
    answer_all(&mut wb, &world, a, KATA);
    assert!(wb.switch_engine(1).unwrap());
    assert!(
        world.take(a).iter().any(|l| l.ends_with(" stop")),
        "the previous engine stops analysing"
    );
    assert!(!world.terminated(a), "fast engine change keeps it running");
    let b = world.launched()[1].0;
    let v = wb.view(false);
    assert_eq!(v.engine.foreground.as_ref().map(|f| f.index), Some(1));
    assert!(
        !v.engine.foreground.unwrap().ready,
        "not complete until B is identified and synced"
    );
    // A late line from A cannot become B's analysis.
    out(&mut wb, a, &info("Q16", 999));
    assert!(wb.view(false).analysis.is_none_or(|x| x.engine == "B"));
    answer_all(&mut wb, &world, b, KATA);
    assert!(wb.view(false).engine.foreground.unwrap().ready);
    assert!(wb.switch_engine(0).unwrap());
    assert_eq!(world.launched().len(), 2, "A is reused, not started again");
}

#[test]
fn a_crashed_engine_reports_its_output_and_only_it_is_cleaned_up() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let mut b = profile("B", "/opt/b");
    b.preload = true;
    wb.save_engine_settings(vec![profile("A", "/opt/a"), b], EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let (a, b) = (world.launched()[0].0, world.launched()[1].0);
    let v = wb.view(false);
    assert_eq!(
        v.engine.foreground.as_ref().map(|f| f.index),
        Some(0),
        "a preloaded engine is not the foreground"
    );
    wb.engine_input(
        a,
        EngineInput::Stderr("libcudart.so.12: cannot open shared object file".into()),
    );
    wb.engine_input(a, EngineInput::Exited(Some(127)));
    let v = wb.view(false);
    let f = v.engine_failures.into_iter().last().expect("failure shown");
    assert!(f.during_startup && f.message.contains("127"));
    assert!(f.output.iter().any(|l| l.contains("libcudart")));
    assert!(v.engine.foreground.is_none());
    assert!(!world.terminated(b), "the other engine keeps running");
    // Output of the dead instance is ignored.
    out(&mut wb, a, "=1 KataGo");
    assert!(wb.view(false).engine.foreground.is_none());
}

/// Every failure between two views reaches the next one, oldest first: here the startup engine
/// and a preloaded one both fail to launch within one `start_engines`.
#[test]
fn launch_failures_are_reported_without_an_instance_each_in_the_next_view() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let mut preload = profile("Y", "/opt/missing-too");
    preload.preload = true;
    wb.save_engine_settings(vec![profile("X", "/opt/missing"), preload], EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let v = wb.view(false);
    let names: Vec<&str> = v.engine_failures.iter().map(|f| f.engine_name.as_str()).collect();
    assert_eq!(names, ["X", "Y"]);
    assert!(v.engine_failures.iter().all(|f| f.message.contains("No such file")));
    assert!(v.engine.foreground.is_none() && world.launched().is_empty());
    assert!(wb.view(false).engine_failures.is_empty(), "each failure once");
}

#[test]
fn exit_ends_every_engine() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, a) = started(dir.path(), &world);
    let mut list = wb.config().engine.engines.clone();
    let mut b = profile("B", "/opt/b");
    b.preload = true;
    list.push(b);
    wb.save_engine_settings(list, EngineStartup::Default).unwrap();
    wb.switch_engine(1).unwrap();
    wb.end_engines();
    for (id, _) in world.launched() {
        assert!(world.terminated(id), "instance {id} left running");
    }
    let _ = a;
}

#[test]
fn shutdown_scopes_and_settings_restart_only_changed_engines() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_engine_settings(
        vec![profile("A", "/opt/a"), profile("B", "/opt/b")],
        EngineStartup::Default,
    )
    .unwrap();
    wb.start_engines();
    wb.switch_engine(1).unwrap();
    let (a, b) = (world.launched()[0].0, world.launched()[1].0);
    assert!(wb.shutdown_engines(EngineScope::Others));
    assert!(world.terminated(a) && !world.terminated(b));

    // Renaming B does not restart it; changing its command does, and it stays in front.
    let mut list = wb.config().engine.engines.clone();
    list[1].name = "B2".into();
    wb.save_engine_settings(list.clone(), EngineStartup::Default).unwrap();
    assert!(!world.terminated(b));
    list[1].command = "/opt/b2 gtp".into();
    wb.save_engine_settings(list, EngineStartup::Default).unwrap();
    assert!(world.terminated(b));
    let (b2, program) = world.launched()[2].clone();
    assert_eq!(program, "/opt/b2");
    assert_eq!(wb.view(false).engine.foreground.map(|f| f.index), Some(1));
    let _ = b2;
}

#[test]
fn engine_settings_reopen_and_choose_the_startup_engine() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let mut b = profile("B", "/opt/b");
    b.is_default = true;
    wb.save_engine_settings(vec![profile("A", "/opt/a"), b], EngineStartup::Default)
        .unwrap();
    let mut wb2 = workbench(dir.path(), &world);
    let s = wb2.view(false).settings.engine;
    assert_eq!(s.engines.len(), 2);
    assert!(s.engines.iter().all(|e| !e.id.is_empty()));
    wb2.start_engines();
    assert_eq!(world.launched()[0].1, "/opt/b", "the default engine starts");

    wb2.save_engine_settings(s.engines.clone(), EngineStartup::Empty)
        .unwrap();
    let mut wb3 = workbench(dir.path(), &world);
    wb3.start_engines();
    assert_eq!(world.launched().len(), 1, "empty start-up launches nothing");
}

#[test]
fn console_refuses_board_commands_and_resumes_analysis_after_others() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    assert_eq!(wb.send_gtp("play B D4").unwrap(), Some("product.consoleBoardCommand"));
    assert!(world.sent(id).is_empty());
    assert_eq!(wb.send_gtp("kata-get-param numSearchThreads").unwrap(), None);
    let sent = world.take(id);
    assert_eq!(sent.len(), 1);
    out(&mut wb, id, "");
    out(&mut wb, id, &format!("={} 8", number(&sent[0])));
    out(&mut wb, id, "");
    let after = world.take(id);
    assert!(after.iter().any(|l| l.contains("kata-analyze")), "{after:?}");
    let console = wb.console_since(0);
    assert!(console.iter().any(|l| l.text == "kata-get-param numSearchThreads"));
    assert!(!console.iter().any(|l| l.text.starts_with("info ")));
}

#[test]
fn komi_change_reaches_the_engine_and_discards_stored_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    out(&mut wb, id, &info("Q16", 40));
    assert!(wb.set_komi(6.5).unwrap());
    assert!(wb.view(false).analysis.is_none());
    // A line the engine computed at the old komi, before it read `komi 6.5`, is not adopted.
    out(&mut wb, id, &info("Q16", 400));
    assert!(wb.view(false).analysis.is_none());
    let sent = answer_all(&mut wb, &world, id, KATA);
    assert!(sent.contains(&"komi 6.5".to_string()), "{sent:?}");
    out(&mut wb, id, &info("Q16", 30));
    let a = wb.view(false).analysis.unwrap();
    assert_eq!((a.komi, a.visits), (6.5, 30));
}

fn search(wb: &mut Workbench) -> u64 {
    wb.view(false).engine.foreground.unwrap().search
}

/// The foreground search the title measures its speed within: every command that makes the
/// engine search the current position again begins another one; results, an unchanged wish and
/// a pause do not.
#[test]
fn searching_the_position_again_begins_another_foreground_search() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    let mut last = search(&mut wb);
    let mut again = |wb: &mut Workbench, what: &str| {
        let sent = answer_all(wb, &world, id, KATA);
        assert!(sent.iter().any(|c| c.starts_with("kata-analyze")), "{what}: {sent:?}");
        let now = search(wb);
        assert_ne!(now, last, "{what}");
        last = now;
    };
    let before = search(&mut wb);
    for visits in [40, 80, 120] {
        out(&mut wb, id, &info("Q16", visits));
    }
    assert!(world.take(id).is_empty());
    assert_eq!(search(&mut wb), before, "a search that goes on stays the same");

    assert!(wb.set_komi(6.5).unwrap());
    again(&mut wb, "komi");
    let chinese = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;
    assert!(matches!(wb.set_kata_rules(chinese, 6.5, None, false).unwrap(), lizzie_core::api::CommandResult::RulesApplied { .. }));
    again(&mut wb, "rules");
    wb.refresh_current_move();
    again(&mut wb, "refresh");
    wb.clear_analysis_info(AnalysisScope::Current);
    again(&mut wb, "clear");

    wb.toggle_analysis().unwrap();
    answer_all(&mut wb, &world, id, KATA);
    let paused = search(&mut wb);
    wb.toggle_analysis().unwrap();
    let sent = answer_all(&mut wb, &world, id, KATA);
    assert!(sent.iter().any(|c| c.starts_with("kata-analyze")), "{sent:?}");
    assert_ne!(search(&mut wb), paused, "a resumed analysis");
}

/// 广度 and 激进度 changes make the engine search again: another foreground search.
#[test]
fn a_changed_search_parameter_begins_another_foreground_search() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_engine_settings(vec![profile("KataGo", "/opt/katago")], EngineStartup::Default).unwrap();
    wb.start_engines();
    let id = world.launched()[0].0;
    let mut params = engine_params("0", "0");
    answer_params(&mut wb, &world, id, "KataGo", &mut params);
    out(&mut wb, id, &info("Q16", 400));
    for (param, value) in [(KataParam::Wrn, 0.5), (KataParam::Wrn, 0.6), (KataParam::Pda, 1.0)] {
        let before = search(&mut wb);
        assert_eq!(wb.set_kata_param(param, true, value).unwrap(), None);
        let sent = answer_params(&mut wb, &world, id, "KataGo", &mut params);
        assert!(sent.iter().any(|c| c.starts_with("kata-analyze")), "{param:?}: {sent:?}");
        assert_ne!(search(&mut wb), before, "{param:?} {value}");
    }
}

#[test]
fn record_rules_are_applied_before_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    // The engine reports Chinese rules at start-up; the record declares Japanese.
    open(&mut wb, "(;RU[Japanese];B[pd])");
    let sent = world.take(id);
    let cmds: Vec<&str> = sent.iter().map(|l| l.split_once(' ').unwrap().1).collect();
    assert!(
        cmds.iter()
            .any(|c| c.starts_with("kata-set-rules {") && c.contains("\"scoring\":\"TERRITORY\"")),
        "{cmds:?}"
    );
    assert!(
        !cmds.iter().any(|c| c.starts_with("kata-analyze")),
        "no analysis before the rules are confirmed"
    );
    for line in &sent {
        let n = number(line);
        out(&mut wb, id, "");
        if line.ends_with("kata-get-rules") {
            // KataGo reports friendlyPassOk false for Japanese rules (1.18.1).
            answer(
                &mut wb,
                id,
                n,
                r#"{"friendlyPassOk":false,"hasButton":false,"ko":"SIMPLE","scoring":"TERRITORY","suicide":false,"tax":"SEKI","whiteHandicapBonus":"0"}"#,
            );
        } else {
            answer(&mut wb, id, n, "");
        }
    }
    let next = answer_all(&mut wb, &world, id, KATA);
    assert!(next.iter().any(|c| c.starts_with("kata-analyze")), "{next:?}");
    let v = wb.view(false);
    assert!(v.rules_prompt.is_none() && v.engine.analysing);
    assert_eq!(v.engine.rules_key.as_deref(), Some("LizzieFrame.currentRules.japanese"));
}

#[test]
fn unappliable_rules_ask_and_the_answer_decides() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    // The engine keeps reporting Chinese rules: Japanese cannot be confirmed.
    open(&mut wb, "(;RU[Japanese];B[pd])");
    let sent = answer_all(&mut wb, &world, id, KATA);
    assert!(!sent.iter().any(|c| c.starts_with("kata-analyze")), "{sent:?}");
    let prompt = wb.view(false).rules_prompt.expect("asked");
    assert_eq!(
        (prompt.target.as_str(), prompt.failure.as_str()),
        ("Japanese", "TARGET_MISMATCH")
    );
    assert!(prompt.observed.unwrap().starts_with("ko=SIMPLE, scoring=AREA"));
    assert!(wb.answer_rules_prompt(prompt.prompt, true));
    let after = answer_all(&mut wb, &world, id, KATA);
    assert!(
        after.iter().any(|c| c.starts_with("kata-analyze")),
        "continue analyses with the engine's rules"
    );
    assert!(wb.view(false).rules_prompt.is_none(), "asked once");

    // An invalid declaration in another record: declining pauses analysis.
    open(&mut wb, "(;RU[my-house-rules];B[pd])");
    answer_all(&mut wb, &world, id, KATA);
    let prompt = wb.view(false).rules_prompt.expect("asked again for the new record");
    assert_eq!(prompt.failure, "INVALID_DECLARATION");
    assert!(
        !wb.answer_rules_prompt(prompt.prompt + 1, true),
        "a stale answer does nothing"
    );
    assert!(wb.answer_rules_prompt(prompt.prompt, false));
    assert!(wb.view(false).engine.paused);
}

#[test]
fn records_without_rules_use_the_engines_own() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    open(&mut wb, "(;B[pd])");
    let sent = answer_all(&mut wb, &world, id, KATA);
    assert!(!sent.iter().any(|c| c.starts_with("kata-set-rules")));
    assert!(sent.iter().any(|c| c.starts_with("kata-analyze")));
}

#[test]
fn switching_applies_the_engine_komi_unless_the_user_chose_it() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let mut b = profile("B", "/opt/b");
    b.komi = 6.5;
    let mut c = profile("C", "/opt/missing");
    c.komi = 5.5;
    wb.save_engine_settings(vec![profile("KataGo", "/opt/katago"), b, c], EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let a = world.launched()[0].0;
    answer_all(&mut wb, &world, a, KATA);
    out(&mut wb, a, &info("Q16", 500));

    // A failed start changes nothing.
    wb.switch_engine(2).unwrap();
    let v = wb.view(false);
    assert!(!v.engine_failures.is_empty());
    assert_eq!((v.komi, v.analysis.map(|x| x.visits)), (7.5, Some(500)));

    // A new engine's komi applies to the empty game once it is ready, discarding old analysis.
    wb.switch_engine(1).unwrap();
    assert_eq!(wb.view(false).komi, 7.5, "not before the engine is ready");
    let b = world.launched()[1].0;
    answer_all(&mut wb, &world, b, KATA);
    let v = wb.view(false);
    assert_eq!(v.komi, 6.5);
    assert!(v.analysis.is_none(), "analysis at komi 7.5 shown at 6.5");

    // A running engine's komi applies at once.
    wb.switch_engine(0).unwrap();
    assert_eq!(wb.view(false).komi, 7.5);

    // A komi the user chose stays.
    wb.set_komi(0.5).unwrap();
    wb.switch_engine(1).unwrap();
    assert_eq!(wb.view(false).komi, 0.5);
}

#[test]
fn a_komi_chosen_while_an_engine_starts_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let mut b = profile("B", "/opt/b");
    b.komi = 6.5;
    wb.save_engine_settings(vec![b], EngineStartup::Default).unwrap();
    wb.start_engines();
    // The user sets the komi before the new engine is ready (Java would overwrite it then).
    wb.set_komi(0.5).unwrap();
    let b = world.launched()[0].0;
    answer_all(&mut wb, &world, b, KATA);
    assert_eq!(wb.view(false).komi, 0.5);
}

#[test]
fn an_engines_board_size_applies_to_a_new_empty_game_only() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let mut nine = profile("Nine", "/opt/nine");
    (nine.width, nine.height) = (9, 9);
    let mut thirteen = profile("Thirteen", "/opt/thirteen");
    (thirteen.width, thirteen.height) = (13, 13);
    wb.save_engine_settings(vec![nine, thirteen], EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let a = world.launched()[0].0;
    let done = answer_all(&mut wb, &world, a, KATA);
    assert_eq!(wb.view(false).document.size.width, 9);
    // The engine is told the size, and no other.
    assert!(done.iter().any(|c| c == "boardsize 9"), "{done:?}");
    assert!(
        done.iter()
            .filter(|c| c.starts_with("boardsize"))
            .all(|c| c == "boardsize 9"),
        "{done:?}"
    );

    open(&mut wb, "(;SZ[19];B[pd])");
    wb.switch_engine(1).unwrap();
    assert_eq!(wb.view(false).document.size.width, 19, "a record keeps its size");
}

#[test]
fn applying_rules_discards_analysis_and_late_results_of_the_old_rules() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, a) = started(dir.path(), &world);
    out(&mut wb, a, &info("Q16", 500));
    wb.set_kata_rules(
        r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"TERRITORY","suicide":false,"tax":"SEKI","whiteHandicapBonus":"0"}"#,
        7.5,
        None,
        false,
    )
    .unwrap();
    assert!(
        wb.view(false).analysis.is_none(),
        "analysis under the old rules still shown"
    );
    // A late line of the old stream, before the engine reads the new commands.
    out(&mut wb, a, &info("Q16", 900));
    assert!(wb.view(false).analysis.is_none(), "late result of the old rules stored");
}

#[test]
fn console_maps_undo_to_the_record_and_refuses_engine_only_board_changes() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, a) = started(dir.path(), &world);
    open(&mut wb, "(;SZ[19];B[pd];W[dd])");
    wb.navigate(Navigation::Forward(2));
    answer_all(&mut wb, &world, a, KATA);
    assert_eq!(wb.send_gtp("undo").unwrap(), None);
    assert_eq!(wb.view(false).position.move_number, 1, "undo steps back in the record");
    world.take(a);
    assert_eq!(
        wb.send_gtp("place_free_handicap 2").unwrap(),
        Some("product.consoleBoardCommand")
    );
    assert!(world.sent(a).is_empty(), "nothing reaches the engine");
}

#[test]
fn a_new_record_ends_the_user_pause() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, a) = started(dir.path(), &world);
    wb.toggle_analysis().unwrap();
    wb.navigate(Navigation::Forward(1));
    assert!(wb.view(false).engine.paused, "browsing keeps the pause");
    open(&mut wb, "(;SZ[19];B[pd])");
    answer_all(&mut wb, &world, a, KATA);
    let v = wb.view(false);
    assert!(
        !v.engine.paused && v.engine.analysing,
        "a loaded record starts a new analysis context"
    );
}

#[test]
fn a_refused_sync_drops_the_analysis_requested_with_it() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, a) = started(dir.path(), &world);
    open(&mut wb, "(;SZ[19];B[pd];W[dd])");
    answer_all(&mut wb, &world, a, KATA);
    wb.navigate(Navigation::Forward(2));
    let sent = world.take(a);
    // The batch's sync is refused, and its search after it does deliver a result.
    let (mut refused, mut delivered) = (None, None);
    for (i, line) in sent.iter().enumerate() {
        let n = number(line);
        out(&mut wb, a, "");
        if line.contains(" play ") || line.contains(" set_position ") {
            out(&mut wb, a, &format!("?{n} illegal move"));
            out(&mut wb, a, "");
            refused.get_or_insert(i);
        } else if line.contains("kata-analyze") {
            out(&mut wb, a, &format!("={n}"));
            out(&mut wb, a, &info("D4", 900));
            delivered = Some(i);
        } else {
            answer(&mut wb, a, n, "");
        }
    }
    assert!(refused.zip(delivered).is_some_and(|(r, d)| r < d), "a refused sync, then a search: {sent:?}");
    let v = wb.view(false);
    assert!(v.analysis.is_none(), "result of a failed sync shown: {sent:?}");
    assert!(
        v.history.iter().all(|h| h.visits == 0),
        "result of a failed sync stored: {:?}",
        v.history
    );
}

#[test]
fn an_edited_position_drops_only_its_own_analysis_and_the_engine_follows_the_edit() {
    use lizzie_core::api::EditCommand;
    use lizzie_core::record::{Point, StoneChange};
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, a) = started(dir.path(), &world);
    open(&mut wb, "(;SZ[19];B[pd];W[dd];B[pp])");
    answer_all(&mut wb, &world, a, KATA);
    out(&mut wb, a, &info("Q16", 50));
    wb.navigate(Navigation::Forward(1));
    answer_all(&mut wb, &world, a, KATA);
    out(&mut wb, a, &info("D4", 40));
    assert_eq!(wb.view(false).analysis.unwrap().visits, 40);

    // 反色 of the stone just played: this node now holds another position.
    wb.edit(EditCommand::EditStone {
        point: Point { x: 15, y: 3 },
        change: StoneChange::Switch,
    })
    .unwrap();
    let v = wb.view(false);
    assert!(v.analysis.is_none(), "analysis of the old position is gone");
    assert!(
        v.history.iter().any(|h| h.node == 0 && h.visits == 50),
        "the unchanged root keeps its result: {:?}",
        v.history
    );
    // A line the engine computed for the old position before reading the edit is not adopted.
    out(&mut wb, a, &info("D4", 400));
    assert!(wb.view(false).analysis.is_none());
    let sent = answer_all(&mut wb, &world, a, KATA);
    assert!(sent.contains(&"play W Q16".to_string()), "{sent:?}");
    out(&mut wb, a, &info("D4", 30));
    assert_eq!(wb.view(false).analysis.unwrap().visits, 30);
}

#[test]
fn no_foreground_engine_starts_during_setup_mode() {
    use lizzie_core::api::EditCommand;
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_engine_settings(vec![profile("KataGo", "/opt/katago")], EngineStartup::Default)
        .unwrap();
    wb.edit(EditCommand::SetSetupMode {
        enabled: true,
        confirmed: None,
    })
    .unwrap();
    // Start-up (also after an import), the engine menu and the analysis toggle all start the
    // foreground engine through one operation.
    wb.start_engines();
    wb.toggle_analysis().unwrap();
    wb.switch_engine(0).unwrap();
    assert!(world.launched().is_empty());
    assert_eq!(wb.view(false).engine_notice, Some(EngineNotice::SetupModeActive));
    wb.edit(EditCommand::SetSetupMode {
        enabled: false,
        confirmed: None,
    })
    .unwrap();
    wb.switch_engine(0).unwrap();
    assert_eq!(world.launched().len(), 1);
}

#[test]
fn variations_carry_the_captures_of_each_move() {
    use lizzie_core::record::Point;
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, a) = started(dir.path(), &world);
    // White A19 in the corner, Black B19 beside it, Black to play.
    open(&mut wb, "(;SZ[19]PL[B]AB[ba]AW[aa])");
    answer_all(&mut wb, &world, a, KATA);
    // Black A18 captures A19; then white plays the emptied A19 back (no further capture), then a
    // move off the board ends the variation.
    out(
        &mut wb,
        a,
        "info move A18 visits 30 winrate 0.6 prior 0.2 order 0 pv A18 pass A19 Z99 C3 pvVisits 30 20 10 ownership 0 0 rootInfo visits 30",
    );
    let v = wb.view(false);
    let c = &v.analysis.unwrap().candidates[0];
    assert_eq!(c.pv, [Some(Point { x: 0, y: 1 }), None, Some(Point { x: 0, y: 0 })]);
    assert_eq!(c.pv_captures, [vec![Point { x: 0, y: 0 }], vec![], vec![]]);
    assert_eq!(c.pv_visits, [30, 20, 10]);
    // 变化图中去掉死子 only decides whether the board removes them: the captures stay facts.
    let mut settings = wb.config().display.clone();
    settings.remove_dead_in_variation = false;
    wb.set_display_settings(settings).unwrap();
    out(&mut wb, a, "info move A18 visits 40 winrate 0.6 prior 0.2 order 0 pv A18 pass A19 rootInfo visits 40");
    let view = wb.view(false);
    let streamed = &view.analysis.unwrap().candidates[0];
    assert!(streamed.pv_visits.is_empty(), "counts absent from the stream stay absent");
    assert_eq!(streamed.pv_captures[0], [Point { x: 0, y: 0 }]);
}

#[test]
fn engine_output_reaches_logs_and_the_failure_report_without_credentials() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_engine_settings(vec![profile("A", "/opt/a")], EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let a = world.launched()[0].0;
    wb.engine_input(a, EngineInput::Stderr("connecting with password=CANARY_SECRET".into()));
    wb.engine_input(a, EngineInput::Exited(Some(1)));
    let v = wb.view(false);
    let logs: Vec<String> = wb.take_logs().into_iter().map(|l| l.text).collect();
    assert!(!logs.iter().any(|l| l.contains("CANARY_SECRET")), "{logs:?}");
    assert!(!v
        .engine_failures
        .into_iter()
        .last()
        .unwrap()
        .output
        .iter()
        .any(|l| l.contains("CANARY_SECRET")));
}

/// Answers a named family's protocol without assuming KataGo's identity or streaming commands.
fn answer_family(wb: &mut Workbench, world: &World, id: InstanceId, name: &str, commands: &str) -> Vec<String> {
    let mut done = Vec::new();
    loop {
        let pending = world.take(id);
        if pending.is_empty() {
            return done;
        }
        for line in pending {
            out(wb, id, "");
            let n = number(&line);
            let cmd = line.split_once(' ').unwrap().1;
            match cmd.split_whitespace().next().unwrap() {
                "name" => answer(wb, id, n, name),
                "version" => answer(wb, id, n, "0.11"),
                "list_commands" => answer(wb, id, n, commands),
                "analyze" | "lz-analyze" | "kata-analyze" => out(wb, id, &format!("={n}")),
                _ => answer(wb, id, n, ""),
            }
            done.push(cmd.to_string());
        }
    }
}

fn start_family(
    dir: &std::path::Path,
    world: &World,
    name: &str,
    commands: &str,
) -> (Workbench, InstanceId, Vec<String>) {
    let mut wb = workbench(dir, world);
    wb.save_engine_settings(vec![profile(name, "/opt/engine")], EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let id = world.launched()[0].0;
    let sent = answer_family(&mut wb, world, id, name, commands);
    (wb, id, sent)
}

#[test]
fn sayuri_uses_analyze_and_preserves_side_to_move_score() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id, sent) = start_family(dir.path(), &world, "Sayuri", "name\nversion\nlist_commands\nanalyze");
    assert!(sent.iter().any(|s| s == "analyze B 10"), "{sent:?}");
    let node = wb.view(false).position.node;
    out(
        &mut wb,
        id,
        "info move Q16 visits 120 winrate 6000 scoreLead 2.5 prior 1230 lcb 5800 pv Q16 D4",
    );
    let result = wb.view(false).analysis.unwrap();
    assert_eq!(result.node, node);
    assert_eq!(result.winrate, 60.0);
    assert_eq!(result.score_lead, Some(2.5));
    assert_eq!(result.candidates[0].lcb, Some(58.0));
    wb.toggle_analysis().unwrap();
    assert!(world.take(id).iter().any(|s| s.ends_with(" name")));
}

#[test]
fn sai_areas_is_a_side_to_move_score_for_either_color() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let commands = "name\nversion\nlist_commands\nlz-analyze";
    let (mut wb, id, sent) = start_family(dir.path(), &world, "sAi 0.18", commands);
    assert!(sent.iter().any(|s| s == "lz-analyze B 10"));
    out(
        &mut wb,
        id,
        "info move Q16 visits 100 winrate 6000 areas 25000 prior 1230 pv Q16 D4",
    );
    assert_eq!(wb.view(false).analysis.unwrap().score_lead, Some(2.5));
    open(&mut wb, "(;SZ[19]PL[W])");
    let sent = answer_family(&mut wb, &world, id, "sAi 0.18", commands);
    assert!(sent.iter().any(|s| s == "lz-analyze W 10"));
    out(
        &mut wb,
        id,
        "info move Q16 visits 100 winrate 6000 areas 25000 prior 1230 pv Q16 D4",
    );
    assert_eq!(wb.view(false).analysis.unwrap().score_lead, Some(2.5));
}

const LEELA011_ROW: &str = " Q16 ->    1234 (W: 55.20%) (U: 0.0%) (V: 50.0%:  ...) (N: 12.3%) PV: Q16 D4";

fn leela011_table(wb: &mut Workbench, id: InstanceId) {
    wb.engine_input(id, EngineInput::Stderr(LEELA011_ROW.into()));
    wb.engine_input(id, EngineInput::Stderr("===== summary complete".into()));
}

#[test]
fn leela011_probes_collects_tables_and_pause_cancels_probes() {
    use lizzie_core::record::Point;
    use std::time::{Duration, Instant};
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let commands = "name\nversion\nlist_commands\ntime_left";
    // The probe is scheduled one interval after a moment inside `start_family`: before `before`
    // plus the interval it cannot be due, after `after` plus the interval it must be.
    let before = Instant::now();
    let (mut wb, id, sent) = start_family(dir.path(), &world, "Leela", commands);
    let after = Instant::now();
    assert_eq!(sent.iter().filter(|s| *s == "time_left b 0 0").count(), 1);
    let node = wb.view(false).position.node;
    wb.engine_tick(before + Duration::from_millis(999));
    assert!(world.take(id).is_empty(), "no probe before the interval");
    wb.engine_tick(after + Duration::from_millis(1001));
    let probe = world.take(id);
    assert_eq!(probe.len(), 1);
    assert!(probe[0].ends_with(" name"));
    wb.engine_tick(after + Duration::from_secs(3));
    assert!(world.take(id).is_empty(), "one probe in flight");
    // A response and stderr can arrive in either order. No new cycle before the table.
    answer(&mut wb, id, number(&probe[0]), "Leela");
    assert!(world.take(id).is_empty());
    wb.engine_input(id, EngineInput::Stderr("not a summary move".into()));
    assert!(wb.view(false).analysis.is_none());
    leela011_table(&mut wb, id);
    let result = wb.view(false).analysis.unwrap();
    assert_eq!(result.node, node);
    let c = &result.candidates[0];
    assert_eq!(c.point, Some(Point { x: 15, y: 3 }));
    assert_eq!(c.visits, 1234);
    assert_eq!(c.winrate, 55.2);
    assert_eq!(c.prior, 12.3);
    assert_eq!(c.pv, [Some(Point { x: 15, y: 3 }), Some(Point { x: 3, y: 15 })]);
    let next = answer_family(&mut wb, &world, id, "Leela", commands);
    assert_eq!(next, ["time_left b 0 0"]);
    wb.engine_tick(Instant::now() + Duration::from_secs(2));
    let probe = world.take(id);
    assert_eq!(probe.len(), 1, "a second cycle also gets a timed probe");
    // Pausing during an unanswered probe must not arm another one when it is answered.
    wb.toggle_analysis().unwrap();
    answer(&mut wb, id, number(&probe[0]), "Leela");
    let stopped = answer_family(&mut wb, &world, id, "Leela", commands);
    assert_eq!(stopped, ["name"]);
    leela011_table(&mut wb, id);
    wb.engine_tick(Instant::now() + Duration::from_secs(10));
    assert!(world.take(id).is_empty());
    assert!(!wb.view(false).engine.analysing);
}

#[test]
fn leela011_late_stderr_table_never_moves_to_a_new_position() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let commands = "name\nversion\nlist_commands\ntime_left";
    let (mut wb, id, _) = start_family(dir.path(), &world, "Leela", commands);
    open(&mut wb, "(;SZ[19];B[dd])");
    answer_family(&mut wb, &world, id, "Leela", commands);
    leela011_table(&mut wb, id); // late summary for the now-closed document
    assert!(wb.view(false).analysis.is_none());
    leela011_table(&mut wb, id); // the new document's own summary
    let v = wb.view(false);
    assert_eq!(v.analysis.unwrap().node, v.position.node);
    answer_family(&mut wb, &world, id, "Leela", commands);
    wb.navigate(Navigation::Forward(1));
    answer_family(&mut wb, &world, id, "Leela", commands);
    leela011_table(&mut wb, id); // root's late table stays root history
    assert!(wb.view(false).analysis.is_none());
    leela011_table(&mut wb, id);
    let v = wb.view(false);
    assert_eq!(v.position.move_number, 1);
    assert_eq!(v.analysis.unwrap().node, v.position.node);
}

#[test]
fn katajigo_is_katago_but_does_not_analyse() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, _, sent) = start_family(
        dir.path(),
        &world,
        "Katajigo",
        "name\nversion\nlist_commands\nkata-analyze",
    );
    assert!(!sent.iter().any(|s| s.starts_with("kata-analyze")));
    let engine = wb.view(false).engine.foreground.unwrap();
    assert!(engine.katago);
    assert!(!engine.can_analyse);
}

#[test]
fn profile_thread_policy_controls_launch_and_bootstrap() {
    use lizzie_core::engine::tuning::{self, ThreadPolicy, ThreadSource};
    let dir = tempfile::tempdir().unwrap();
    for name in ["katago", "gtp.cfg", "model.bin.gz"] {
        std::fs::write(dir.path().join(name), b"fixture").unwrap();
    }
    let base = format!(
        "\"{}\" gtp -config \"{}\" -model \"{}\"",
        dir.path().join("katago").display(),
        dir.path().join("gtp.cfg").display(),
        dir.path().join("model.bin.gz").display()
    );
    for (source, explicit, expected) in [
        (ThreadSource::CFG, false, false),
        (ThreadSource::BENCHMARK, false, true),
        (ThreadSource::BENCHMARK, true, false),
    ] {
        let world = World::default();
        let mut wb = workbench(dir.path(), &world);
        let command = if explicit {
            format!("{base} -override-config numSearchThreads=3")
        } else {
            base.clone()
        };
        let spec = lizzie_core::engine::gtp::prepare_launch(&command, &[]).unwrap();
        let policy = ThreadPolicy {
            source,
            threads: 8,
            nn_server_threads: 2,
            batch_size: 16,
            environment: Some(tuning::environment(&command, &spec).unwrap()),
            ..ThreadPolicy::default()
        };
        wb.save_engine_settings(
            vec![EngineProfile {
                id: "tuned".into(),
                command,
                thread_policy: Some(policy),
                ..EngineProfile::default()
            }],
            EngineStartup::Empty,
        )
        .unwrap();
        wb.switch_engine(0).unwrap();
        let id = world.launched().last().unwrap().0;
        let commands = answer_all(&mut wb, &world, id, KATA);
        assert_eq!(
            commands.iter().any(|s| s == "kata-set-param numSearchThreads 8"),
            expected
        );
        let args = world
            .0
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.0 == id)
            .unwrap()
            .2
            .args
            .clone();
        if expected {
            let overrides = tuning::overrides(&args);
            assert_eq!(overrides["numsearchthreads"], "8");
            assert_eq!(overrides["numnnserverthreadspermodel"], "2");
            assert_eq!(overrides["nnmaxbatchsize"], "16");
        }
        wb.end_engines();
    }
}

#[test]
fn startup_benchmark_is_opt_in_visible_stoppable_and_remembers_dismissal() {
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
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_engine_settings(
        vec![EngineProfile {
            id: "startup".into(),
            command,
            ..EngineProfile::default()
        }],
        EngineStartup::Empty,
    )
    .unwrap();
    wb.switch_engine(0).unwrap();
    let id = world.launched()[0].0;
    answer_all(&mut wb, &world, id, KATA);
    assert!(wb.begin_startup_benchmark(None).is_none());
    wb.set_startup_benchmark(true).unwrap();
    // Apple automatic optimization requires its trusted Metal resource package.
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        assert!(wb.begin_startup_benchmark(None).is_none());
        return;
    }
    let job = wb.begin_startup_benchmark(None).unwrap();
    let notice = wb.view(false).tuning.unwrap().startup_notice.unwrap();
    assert_eq!(notice.id, job.id);
    assert!(notice.automatic);
    assert!(!wb.dismiss_startup_notice(job.id + 1));
    assert!(wb.dismiss_startup_notice(job.id));
    wb.complete_benchmark(job.id, Err("cancelled".into()));
    assert!(wb.begin_startup_benchmark(None).is_none());
    assert_eq!(wb.config().engine.startup_benchmark_dismissed_version, 5);
}

#[test]
fn benchmark_analysis_pause_restores_only_when_user_has_not_paused() {
    for user_pauses in [false, true] {
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
        let world = World::default();
        let mut wb = workbench(dir.path(), &world);
        wb.save_engine_settings(
            vec![EngineProfile {
                id: "paused".into(),
                command,
                ..EngineProfile::default()
            }],
            EngineStartup::Empty,
        )
        .unwrap();
        wb.switch_engine(0).unwrap();
        let engine = world.launched()[0].0;
        answer_all(&mut wb, &world, engine, KATA);
        assert!(wb.view(false).engine.analysing);
        let job = wb.begin_benchmark("paused", false).unwrap();
        answer_all(&mut wb, &world, engine, KATA);
        assert!(!wb.view(false).engine.analysing);
        if user_pauses {
            wb.toggle_analysis().unwrap();
        }
        wb.complete_benchmark(job.id, Err("AutoSetup.benchmarkFailed".into()));
        answer_all(&mut wb, &world, engine, KATA);
        let view = wb.view(false);
        assert_eq!(view.engine.paused, user_pauses);
        assert_eq!(view.engine.analysing, !user_pauses);
    }
}

#[test]
fn the_commented_export_summarises_only_analysed_moves_against_their_parent() {
    use lizzie_core::api::{CommentLabels, ExportKind};
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    open(&mut wb, "(;SZ[19]C[root note];B[pd]C[first];W[dd])");
    answer_all(&mut wb, &world, id, KATA);
    // Root and B[pd] analysed (60 % and +1.5 for the side to move); W[dd] never.
    out(&mut wb, id, &info("Q16", 50));
    wb.navigate(Navigation::Forward(1));
    let batch = world.take(id);
    out(&mut wb, id, "");
    answer(&mut wb, id, number(&batch[0]), "");
    out(&mut wb, id, &format!("={}", number(&batch[1])));
    out(&mut wb, id, &info("D4", 1500));
    let labels = CommentLabels {
        black: "黑棋".into(),
        white: "白棋".into(),
        winrate: "胜率:".into(),
        lead: "领先:".into(),
        lead_with_komi: "盘面:".into(),
        stdev: "不确定度:".into(),
        playouts: "计算量".into(),
        komi: "贴目:".into(),
        pda: "PDA:".into(),
        wrn: "广度:".into(),
    };
    let text = wb.export_text(ExportKind::RawWithComments, &labels);
    // Black just moved: Black's 40 % is 20 points below Black's 60 % before the move, and its
    // lead fell from +1.5 to -1.5 (`formatComment`).
    assert!(
        text.contains("C[first\n\n黑棋 胜率: 40.0% (-20.0%)\n领先: -1.5 (-3.0)"),
        "{text}"
    );
    assert!(text.contains("1.5k 计算量"), "{text}");
    assert!(
        text.contains("C[root note]"),
        "the root comment is written as it is: {text}"
    );
    assert!(text.ends_with(";W[dd])"), "an unanalysed move gets no summary: {text}");
    assert!(!text.contains("LZ["), "{text}");
}

/// 总是显示黑胜率 × 目差显示 on the displays, for a position with White to move (fixed Java
/// `BoardRenderer` win rate flip, `Utils.convertScoreToString`, the black-perspective graph,
/// statistics panel and title). Stored results are converted again when the setting changes.
#[test]
fn the_displays_follow_the_viewpoint_settings() {
    use lizzie_core::settings::DisplaySettings;
    use lizzie_core::viewpoint::{AsCandidate, ShownValues};
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    open(&mut wb, "(;SZ[19]KM[7.5];B[pd])");
    wb.navigate(Navigation::Forward(1));
    answer_all(&mut wb, &world, id, KATA);
    // White to move: 60 % and +1.5 for White.
    out(&mut wb, id, &info("D4", 50));
    // (always black, with komi) → (board candidate, Black's lead, the move's table row as shown)
    let cases = [
        ((false, false), (60.0, 1.5), -1.5, (40.0, -1.5)),
        ((true, false), (40.0, -1.5), -1.5, (40.0, -1.5)),
        ((false, true), (60.0, 1.5 - 7.5), -1.5 + 7.5, (40.0, -1.5 + 7.5)),
        ((true, true), (40.0, -1.5 + 7.5), -1.5 + 7.5, (40.0, -1.5 + 7.5)),
    ];
    for ((always_black, with_komi), (winrate, lead), black_lead, (row_winrate, row_lead)) in cases {
        let settings = DisplaySettings {
            winrate_always_black: always_black,
            score_lead_with_komi: with_komi,
            ..DisplaySettings::default()
        };
        wb.set_display_settings(settings).unwrap();
        let v = wb.view(false);
        let at = format!("always black {always_black}, with komi {with_komi}");
        let a = v.analysis.unwrap();
        assert_eq!(
            a.candidates[0].shown,
            ShownValues {
                winrate,
                score_lead: Some(lead)
            },
            "{at}"
        );
        let black = ShownValues {
            winrate: 40.0,
            score_lead: Some(black_lead),
        };
        assert_eq!(a.black, black, "{at}");
        let point = v.history.iter().find(|p| p.node == a.node).unwrap();
        assert_eq!(point.black, black, "{at}");
        // The move's table row keeps Black's raw numbers for the change column.
        assert_eq!(
            point.as_candidate,
            AsCandidate {
                winrate: 40.0,
                score_lead: Some(-1.5),
                shown: ShownValues {
                    winrate: row_winrate,
                    score_lead: Some(row_lead)
                },
                // The independent list ignores 总是显示黑胜率: Black moved, so Black's numbers.
                side_to_move: ShownValues {
                    winrate: 40.0,
                    score_lead: Some(if with_komi { -1.5 + 7.5 } else { -1.5 })
                },
            },
            "{at}"
        );
    }
}

#[test]
fn stronger_live_analysis_replaces_loaded_results_and_is_saved_for_every_node() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    open(&mut wb, "(;SZ[9]KM[7.5]LZOP[old 48.0 200 0.5 1 0 rootVisits=200\nmove E5 visits 200 winrate 5200 scoreMean 0.5 pv E5]\
        ;B[ee]LZ[old 47.0 200 0.8 1 0 rootVisits=200\nmove C3 visits 200 winrate 5300 scoreMean 0.8 pv C3]LZ2[secondary])");
    wb.set_append_winrate_to_comment(false).unwrap();
    answer_all(&mut wb, &world, id, KATA);
    out(&mut wb, id, &info("D4", 100));
    assert_eq!(wb.view(false).analysis.unwrap().engine, "old", "weaker live data leaves the imported result");
    out(&mut wb, id, &info("D4", 300));
    assert_eq!(wb.view(false).analysis.unwrap().visits, 300);
    wb.navigate(Navigation::Forward(1));
    answer_all(&mut wb, &world, id, KATA);
    out(&mut wb, id, &info("C3", 400));
    let text = wb.begin_save("live.sgf", "interop:test").text;
    assert!(!text.contains("old "), "stale primary tags must not accompany live data: {text}");
    assert_eq!(text.matches("LZOP[").count(), 1, "{text}");
    assert_eq!(text.matches("LZ[").count(), 1, "{text}");
    assert!(
        text.contains("rootVisits=300") && text.contains("rootVisits=400"),
        "{text}"
    );
    // Secondary tags are written from the secondary engine's parsed results only, like Java's
    // `hasSecondaryAnalysisPayload`: an unreadable `LZ2` carries none.
    assert!(!text.contains("LZ2["), "{text}");
    let mut reopened = workbench(dir.path(), &world);
    open(&mut reopened, &text);
    let root = reopened.view(false).analysis.unwrap();
    assert_eq!((root.visits, root.root_visits, root.winrate, root.score_lead), (300, Some(300), 60.0, Some(1.5)));
    assert_eq!(root.candidates[0].visits, 300);
    reopened.navigate(Navigation::Forward(1));
    let next = reopened.view(false).analysis.unwrap();
    assert_eq!((next.visits, next.root_visits, next.winrate, next.score_lead), (400, Some(400), 60.0, Some(1.5)));
    assert_eq!(next.candidates[0].visits, 400);
}

/// A 9x9 record whose root (`LZOP`) and first move (`LZ`) carry saved 200-visit analysis.
const ANALYSED: &str = "(;SZ[9]KM[7.5]LZOP[old 48.0 200 0.5 1 0 rootVisits=200\nmove E5 visits 200 winrate 5200 scoreMean 0.5 pv E5]\
    ;B[ee]LZ[old 47.0 200 0.8 1 0 rootVisits=200\nmove C3 visits 200 winrate 5300 scoreMean 0.8 pv C3])";

#[test]
fn clearing_the_cache_keeps_the_analysis_but_lets_the_next_weaker_result_replace_it() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    open(&mut wb, ANALYSED);
    wb.set_append_winrate_to_comment(false).unwrap();
    answer_all(&mut wb, &world, id, KATA);
    out(&mut wb, id, &info("D4", 50));
    assert_eq!(wb.view(false).analysis.unwrap().engine, "old", "the cache keeps the stronger result");

    // 此手 marks the current move only.
    wb.navigate(Navigation::Forward(1));
    answer_all(&mut wb, &world, id, KATA);
    wb.clear_analysis_cache(AnalysisScope::Current);
    wb.navigate(Navigation::Back(1));
    answer_all(&mut wb, &world, id, KATA);
    out(&mut wb, id, &info("D4", 50));
    assert_eq!(wb.view(false).analysis.unwrap().engine, "old", "the root was not marked");

    // 全局 (Shift+A): nothing is sent and the shown and saved analysis stay as they were.
    wb.clear_analysis_cache(AnalysisScope::All);
    assert!(world.take(id).is_empty());
    assert_eq!(wb.view(false).analysis.unwrap().visits, 200);
    let text = wb.begin_save("cached.sgf", "interop:test").text;
    assert!(text.contains("LZOP[old") && text.contains("LZ[old"), "{text}");
    // The next result replaces the marked one even when weaker; after that the cache holds again.
    out(&mut wb, id, &info("D4", 50));
    assert_eq!(wb.view(false).analysis.unwrap().visits, 50);
    out(&mut wb, id, &info("D4", 20));
    assert_eq!(wb.view(false).analysis.unwrap().visits, 50);
    wb.navigate(Navigation::Forward(1));
    answer_all(&mut wb, &world, id, KATA);
    out(&mut wb, id, &info("C3", 30));
    assert_eq!(wb.view(false).analysis.unwrap().visits, 30, "the first move was marked too");
}

#[test]
fn without_the_lizzie_cache_every_result_replaces_the_stored_one() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    analysis_settings(&mut wb, |s| s.enable_lizzie_cache = false);
    out(&mut wb, id, &info("Q16", 200));
    out(&mut wb, id, &info("D4", 20));
    assert_eq!(wb.view(false).analysis.unwrap().visits, 20);
}

/// A stored result made with another PDA (`BoardData.pda`, the sixth header field) gives way to
/// the live search, whose PDA is 0 (`cachedPda != engine.pda`), even when the new one is weaker.
#[test]
fn a_result_saved_with_another_pda_gives_way_to_a_weaker_live_one() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    open(&mut wb, "(;SZ[9]KM[7.5]LZOP[old 48.0 200 0.5 1 1.5 rootVisits=200\nmove E5 visits 200 winrate 5200 scoreMean 0.5 pv E5])");
    answer_all(&mut wb, &world, id, KATA);
    out(&mut wb, id, &info("D4", 50));
    assert_eq!(wb.view(false).analysis.unwrap().visits, 50);
    out(&mut wb, id, &info("D4", 20));
    assert_eq!(wb.view(false).analysis.unwrap().visits, 50, "the live result is cached again");
}

#[test]
fn clearing_analysis_information_erases_it_from_the_board_and_the_saved_record() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    open(&mut wb, ANALYSED);
    wb.set_append_winrate_to_comment(false).unwrap();
    wb.navigate(Navigation::Forward(1));
    answer_all(&mut wb, &world, id, KATA);
    wb.clear_analysis_info(AnalysisScope::Current);
    let v = wb.view(false);
    assert!(v.analysis.is_none());
    assert!(!v.history.iter().any(|p| p.node == 1 && p.visits > 0), "{:?}", v.history);
    let text = wb.begin_save("cleared.sgf", "interop:test").text;
    assert!(text.contains("LZOP[old") && !text.contains("LZ[old"), "this move only: {text}");
    // This engine does not list `clear_cache`: the running search only starts again.
    assert_eq!(answer_all(&mut wb, &world, id, KATA), ["stop", "kata-analyze W 10 rootInfo true"]);

    wb.clear_analysis_info(AnalysisScope::All);
    let text = wb.begin_save("cleared.sgf", "interop:test").text;
    assert!(!text.contains("LZOP[") && !text.contains("LZ["), "{text}");
    let mut reopened = workbench(dir.path(), &world);
    open(&mut reopened, &text);
    assert!(reopened.view(false).analysis.is_none());
    reopened.navigate(Navigation::Forward(1));
    assert!(reopened.view(false).analysis.is_none());
}

#[test]
fn refreshing_sends_clear_cache_only_to_an_engine_that_lists_it_and_restarts_only_a_running_search() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let list = format!("{KATA}\nclear_cache");
    let (mut wb, id) = started_listing(dir.path(), &world, &list);
    out(&mut wb, id, &info("Q16", 40));
    wb.clear_analysis_info(AnalysisScope::Current);
    // The stopped search's last line, written before the engine read `clear_cache`, is not shown.
    out(&mut wb, id, &info("Q16", 45));
    assert!(wb.view(false).analysis.is_none());
    assert_eq!(answer_all(&mut wb, &world, id, &list), ["clear_cache", "kata-analyze B 10 rootInfo true"]);
    out(&mut wb, id, &info("Q16", 5));
    assert_eq!(wb.view(false).analysis.unwrap().visits, 5, "the new search is shown from its start");
    wb.toggle_analysis().unwrap();
    answer_all(&mut wb, &world, id, &list);
    wb.clear_analysis_info(AnalysisScope::Current);
    assert_eq!(answer_all(&mut wb, &world, id, &list), ["clear_cache"], "a paused engine stays paused");
}

#[test]
fn the_empty_board_stops_its_analysis_silently_until_the_user_resumes_it() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    analysis_settings(&mut wb, |s| s.stop_at_empty_board = true);
    answer_all(&mut wb, &world, id, KATA);
    out(&mut wb, id, &info("Q16", 5));
    let v = wb.view(false);
    assert!(v.engine.stopped_by_limit && !v.engine.paused);
    assert_eq!(v.engine_notice, None, "no limit notice for the empty board");
    assert_eq!(answer_all(&mut wb, &world, id, KATA), ["stop"]);
    // Resumed by the user, the empty board is searched on (`outOfPlayoutsLimit`).
    wb.toggle_analysis().unwrap();
    assert_eq!(answer_all(&mut wb, &world, id, KATA), ["kata-analyze B 10 rootInfo true"]);
    out(&mut wb, id, &info("Q16", 50));
    assert!(!wb.view(false).engine.stopped_by_limit);

    // Another empty board stops again; a position with stones does not.
    open(&mut wb, "(;SZ[19];B[pd])");
    answer_all(&mut wb, &world, id, KATA);
    out(&mut wb, id, &info("Q16", 5));
    assert!(wb.view(false).engine.stopped_by_limit);
    wb.navigate(Navigation::Forward(1));
    answer_all(&mut wb, &world, id, KATA);
    out(&mut wb, id, &info("D4", 5));
    assert!(!wb.view(false).engine.stopped_by_limit);
    open(&mut wb, "(;SZ[19]AB[pd])");
    answer_all(&mut wb, &world, id, KATA);
    out(&mut wb, id, &info("D4", 5));
    assert!(!wb.view(false).engine.stopped_by_limit, "setup stones are not the empty board");
}

#[test]
fn a_hidden_console_without_always_gtp_records_only_a_starting_engine() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    analysis_settings(&mut wb, |s| s.always_gtp = false);
    wb.save_engine_settings(vec![profile("KataGo", "/opt/katago")], EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let id = world.launched()[0].0;
    answer_all(&mut wb, &world, id, KATA);
    let start_up: Vec<String> = wb.console_since(0).into_iter().map(|l| l.text).collect();
    assert!(start_up.iter().any(|t| t == "list_commands"), "{start_up:?}");
    let seq = wb.console_seq();
    wb.toggle_analysis().unwrap();
    answer_all(&mut wb, &world, id, KATA);
    assert!(wb.console_since(seq).is_empty(), "{:?}", wb.console_since(seq));
    wb.set_console_visible(true);
    wb.toggle_analysis().unwrap();
    answer_all(&mut wb, &world, id, KATA);
    assert!(wb.console_since(seq).iter().any(|l| l.text.starts_with("kata-analyze")));
}

const KATA_PARAMS: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nset_position\nkata-analyze\nkata-set-rules\nkata-get-rules\nkata-set-param\nkata-get-param\nstop";

/// `answer_all` for an engine named `name` whose PDA / WRN are `pda` / `wrn`: `kata-set-param`
/// of either changes them, `kata-get-param` reads them.
fn answer_params(wb: &mut Workbench, world: &World, id: InstanceId, name: &str, params: &mut [(&str, String); 2]) -> Vec<String> {
    let mut done = Vec::new();
    loop {
        let pending = world.take(id);
        if pending.is_empty() {
            return done;
        }
        for line in pending {
            out(wb, id, "");
            let n = number(&line);
            let cmd = line.split_once(' ').unwrap().1.to_string();
            let words: Vec<&str> = cmd.split(' ').collect();
            let slot = words.get(1).and_then(|p| params.iter().position(|(k, _)| k == p));
            match (words[0], slot) {
                ("name", _) => answer(wb, id, n, name),
                ("version", _) => answer(wb, id, n, "1.16.4"),
                ("list_commands", _) => answer(wb, id, n, KATA_PARAMS),
                ("kata-analyze" | "lz-analyze", _) => out(wb, id, &format!("={n}")),
                ("kata-get-param", Some(slot)) => {
                    let value = params[slot].1.clone();
                    answer(wb, id, n, &value);
                }
                ("kata-set-param", Some(slot)) => {
                    params[slot].1 = words[2].to_string();
                    answer(wb, id, n, "");
                }
                _ => answer(wb, id, n, ""),
            }
            done.push(cmd);
        }
    }
}

fn engine_params(pda: &str, wrn: &str) -> [(&'static str, String); 2] {
    [("playoutDoublingAdvantage", pda.into()), ("analysisWideRootNoise", wrn.into())]
}

/// A row the user changed in full: every part of it applies.
fn row(enabled: bool, value: Option<f64>, auto_load: bool) -> KataParamEdit {
    KataParamEdit { enabled, value, auto_load, edited: true, auto_load_edited: true }
}

/// Without 自动加载 the engine keeps its own values; the toolbar shows what it reports.
#[test]
fn a_starting_katago_without_auto_load_gets_no_parameter_and_shows_its_own_values() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_engine_settings(vec![profile("KataGo", "/opt/katago")], EngineStartup::Default).unwrap();
    wb.start_engines();
    let id = world.launched()[0].0;
    let mut params = engine_params("0.25", "0.04");
    let start = answer_params(&mut wb, &world, id, "KataGo", &mut params);
    assert!(!start.iter().any(|c| c.starts_with("kata-set-param playout") || c.starts_with("kata-set-param analysisWide")), "{start:?}");
    let reads: Vec<&String> = start.iter().filter(|c| c.starts_with("kata-get-param")).collect();
    assert_eq!(reads, ["kata-get-param playoutDoublingAdvantage", "kata-get-param analysisWideRootNoise"]);
    let v = wb.view(false).engine.kata_params;
    assert_eq!((v.pda, v.wrn, v.pda_unavailable, v.wrn_unavailable), (Some(0.25), Some(0.04), None, None));
    assert!(v.pda_read_back.is_some() && v.wrn_read_back.is_some(), "the start-up read-back is one the toolbar shows");
}

/// 引擎参数 window: 确定 sends each row's value at once and remembers the checked 自动加载
/// values, which every KataGo start applies before reading the values back. The toolbar takes
/// the read-backs of each start, even of the same values, and not the answers to the sets.
#[test]
fn the_parameter_window_applies_values_now_and_its_auto_load_on_every_start() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_engine_settings(vec![profile("KataGo", "/opt/katago")], EngineStartup::Default).unwrap();
    wb.start_engines();
    let id = world.launched()[0].0;
    let mut params = engine_params("0", "0");
    answer_params(&mut wb, &world, id, "KataGo", &mut params);
    let first = wb.view(false).engine.kata_params;
    let first = (first.pda_read_back, first.wrn_read_back);
    let edit = KataEngineParamsEdit {
        pda: Some(row(true, Some(0.5), true)),
        wrn: row(true, Some(0.3), true),
        show_pda: false,
        show_wrn: true,
    };
    assert_eq!(wb.save_kata_engine_params(edit).unwrap(), None);
    let sent = answer_params(&mut wb, &world, id, "KataGo", &mut params);
    assert_eq!(&sent[..2], ["kata-set-param playoutDoublingAdvantage 0.5", "kata-set-param analysisWideRootNoise 0.3"]);
    let v = wb.view(false).engine.kata_params;
    assert_eq!((v.pda, v.wrn, (v.pda_read_back, v.wrn_read_back)), (Some(0.5), Some(0.3), first.clone()), "a confirmed set is no read-back");
    let s = &wb.config().engine.kata_params;
    assert_eq!((s.auto_load_pda, s.auto_load_pda_value, s.auto_load_wrn, s.auto_load_wrn_value), (true, Some(0.5), true, Some(0.3)));
    assert_eq!((s.show_pda, s.show_wrn), (false, true));

    // A restarted process starts with the engine's own values and gets the saved ones.
    wb.restart_engine().unwrap();
    let id = world.launched().last().unwrap().0;
    let mut fresh = engine_params("0", "0");
    let start = answer_params(&mut wb, &world, id, "KataGo", &mut fresh);
    let pda = start.iter().position(|c| c == "kata-set-param playoutDoublingAdvantage 0.5").expect("PDA");
    let wrn = start.iter().position(|c| c == "kata-set-param analysisWideRootNoise 0.3").expect("WRN");
    let read = start.iter().position(|c| c == "kata-get-param playoutDoublingAdvantage").expect("read");
    assert!(pda < wrn && wrn < read, "{start:?}");
    let v = wb.view(false).engine.kata_params;
    assert_eq!((v.pda, v.wrn), (Some(0.5), Some(0.3)));
    assert!(v.pda_read_back.is_some() && v.pda_read_back != first.0, "a new engine's read-back of the same values is shown");
    assert!(v.wrn_read_back.is_some() && v.wrn_read_back != first.1);

    // A `KataGoPda` engine keeps its own PDA (its 激进度 belongs to the PDA dialog).
    wb.restart_engine().unwrap();
    let id = world.launched().last().unwrap().0;
    let mut pda_engine = engine_params("1.2", "0");
    let start = answer_params(&mut wb, &world, id, "KataGoPda-1.0", &mut pda_engine);
    assert!(!start.iter().any(|c| c.starts_with("kata-set-param playoutDoublingAdvantage")), "{start:?}");
    assert!(start.iter().any(|c| c == "kata-set-param analysisWideRootNoise 0.3"), "{start:?}");
    assert_eq!(wb.view(false).engine.kata_params.pda, Some(1.2));
}

/// SetKataEngines.applyExistingSettings: only what the user changed since the window loaded it is
/// saved and sent; an unchanged row never overwrites newer engine or saved values.
#[test]
fn the_parameter_window_applies_only_the_rows_the_user_changed() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_engine_settings(vec![profile("KataGo", "/opt/katago")], EngineStartup::Default).unwrap();
    wb.start_engines();
    let id = world.launched()[0].0;
    let mut params = engine_params("0", "0");
    answer_params(&mut wb, &world, id, "KataGo", &mut params);
    let saved = KataEngineParamsEdit {
        pda: Some(row(true, Some(0.5), true)),
        wrn: row(true, Some(0.3), true),
        show_pda: true,
        show_wrn: true,
    };
    assert_eq!(wb.save_kata_engine_params(saved).unwrap(), None);
    answer_params(&mut wb, &world, id, "KataGo", &mut params);
    // 激进度 shows another value but was not touched; 广度 was changed, its 自动加载 was not.
    let unchanged = |enabled, value| KataParamEdit { enabled, value, auto_load: false, edited: false, auto_load_edited: false };
    let edit = KataEngineParamsEdit {
        pda: Some(unchanged(true, Some(1.0))),
        wrn: KataParamEdit { edited: true, ..unchanged(true, Some(0.4)) },
        show_pda: false,
        show_wrn: true,
    };
    assert_eq!(wb.save_kata_engine_params(edit).unwrap(), None);
    let sent = answer_params(&mut wb, &world, id, "KataGo", &mut params);
    let sets: Vec<&String> = sent.iter().filter(|c| c.starts_with("kata-set-param")).collect();
    assert_eq!(sets, ["kata-set-param analysisWideRootNoise 0.4"]);
    let e = &wb.config().engine;
    assert_eq!((e.kata_pda_enabled, e.kata_pda), (true, Some(0.5)));
    assert_eq!((e.kata_params.wrn_enabled, e.kata_params.wrn), (true, Some(0.4)));
    let s = &e.kata_params;
    assert_eq!((s.auto_load_pda, s.auto_load_pda_value, s.auto_load_wrn, s.auto_load_wrn_value), (true, Some(0.5), true, Some(0.3)));
    assert_eq!((s.show_pda, s.show_wrn), (false, true), "the toolbar choices are always saved");
}

/// Toolbar 广度: a check or a value goes to the engine, which searches again; every stored
/// result gives way to the next live one. Unchecking sends 0. Out-of-range values are sent.
#[test]
fn the_toolbar_breadth_reaches_the_engine_and_lets_stored_results_give_way() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_engine_settings(vec![profile("KataGo", "/opt/katago")], EngineStartup::Default).unwrap();
    wb.start_engines();
    let id = world.launched()[0].0;
    let mut params = engine_params("0", "0");
    answer_params(&mut wb, &world, id, "KataGo", &mut params);
    open(&mut wb, ANALYSED);
    answer_params(&mut wb, &world, id, "KataGo", &mut params);
    out(&mut wb, id, &info("D4", 50));
    assert_eq!(wb.view(false).analysis.unwrap().engine, "old", "the cache holds the stronger result");

    assert_eq!(wb.set_kata_param(KataParam::Wrn, true, 2.5).unwrap(), None);
    // The stopped search's late line is not shown.
    out(&mut wb, id, &info("D4", 60));
    assert_eq!(wb.view(false).analysis.unwrap().engine, "old");
    let sent = answer_params(&mut wb, &world, id, "KataGo", &mut params);
    assert_eq!(sent, ["kata-set-param analysisWideRootNoise 2.5", "kata-analyze B 10 rootInfo true"]);
    assert_eq!(wb.view(false).engine.kata_params.wrn, Some(2.5));
    out(&mut wb, id, &info("D4", 5));
    assert_eq!(wb.view(false).analysis.unwrap().visits, 5, "the weaker new result replaces the stored one");
    let s = &wb.config().engine.kata_params;
    assert_eq!((s.wrn_enabled, s.wrn), (true, Some(2.5)));

    wb.set_kata_param(KataParam::Wrn, false, 2.5).unwrap();
    let sent = answer_params(&mut wb, &world, id, "KataGo", &mut params);
    assert_eq!(sent[0], "kata-set-param analysisWideRootNoise 0");
    assert_eq!(wb.view(false).engine.kata_params.wrn, Some(0.0));
    let s = &wb.config().engine.kata_params;
    assert_eq!((s.wrn_enabled, s.wrn), (false, Some(2.5)), "the unchecked value is kept");
}

/// Toolbar 激进度: live results are made and saved with the engine's PDA (the sixth header
/// field), and a stored result made with another PDA gives way.
#[test]
fn live_results_carry_the_engine_pda_into_the_saved_record() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_engine_settings(vec![profile("KataGo", "/opt/katago")], EngineStartup::Default).unwrap();
    wb.start_engines();
    let id = world.launched()[0].0;
    let mut params = engine_params("0", "0");
    answer_params(&mut wb, &world, id, "KataGo", &mut params);
    wb.set_append_winrate_to_comment(false).unwrap();
    out(&mut wb, id, &info("Q16", 200));
    assert_eq!(wb.set_kata_param(KataParam::Pda, true, 0.5).unwrap(), None);
    let sent = answer_params(&mut wb, &world, id, "KataGo", &mut params);
    assert_eq!(sent[0], "kata-set-param playoutDoublingAdvantage 0.5");
    out(&mut wb, id, &info("Q16", 20));
    assert_eq!(wb.view(false).analysis.unwrap().visits, 20, "made with PDA 0, the stored result gives way");
    let text = wb.begin_save("pda.sgf", "interop:test").text;
    let header = text.split("LZOP[").nth(1).and_then(|t| t.lines().next()).unwrap_or_default();
    assert!(
        header.ends_with(" 0.5 rootVisits=20 wrn=0.0"),
        "the PDA is the sixth header field: {text}"
    );
    out(&mut wb, id, &info("Q16", 10));
    assert_eq!(wb.view(false).analysis.unwrap().visits, 20, "same PDA: the cache holds");
    let c = wb.config();
    assert_eq!((c.engine.kata_pda_enabled, c.engine.kata_pda), (true, Some(0.5)));
}

/// A KataGo line with a score uncertainty, as KataGo's summaries carry it.
fn kata_info(mv: &str, visits: u64) -> String {
    format!(
        "info move {mv} visits {visits} winrate 0.6 scoreLead 1.5 scoreStdev 7 prior 0.2 order 0 pv {mv} D4 rootInfo visits {visits}"
    )
}

/// The breadth of a stored result in a save of `wb`: the `wrn=` of the root's `LZOP` header.
fn saved_breadth(wb: &mut Workbench) -> (String, Option<f64>) {
    let text = wb.begin_save("wrn.sgf", "interop:test").text;
    let rec = sgf::read(&text).unwrap();
    let saved = lizzie_core::formats::saved_analysis::primary(&rec.node(lizzie_core::record::ROOT).properties);
    (text, saved.expect("the root's analysis is saved").wrn)
}

/// Java `BoardDataWrnMetadataTest`: a live result records the breadth its engine confirmed, also
/// a 0 (Java `BoardData.wrnKnown`/`wrn` of the source engine); a result of an engine that has
/// confirmed none is unknown, never filled in from the toolbar. The summary shows only a known
/// non-zero breadth.
#[test]
fn live_results_record_the_breadth_their_engine_confirmed() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_engine_settings(vec![profile("KataGo", "/opt/katago")], EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let id = world.launched()[0].0;
    let mut params = engine_params("0", "0.5");
    answer_params(&mut wb, &world, id, "KataGo", &mut params);
    open(&mut wb, "(;SZ[9]KM[7.5];B[ee])");
    answer_params(&mut wb, &world, id, "KataGo", &mut params);
    out(&mut wb, id, &kata_info("D4", 200));
    let (_, wrn) = saved_breadth(&mut wb);
    assert_eq!(wrn, Some(0.5));
    wb.navigate(Navigation::Forward(1));
    answer_params(&mut wb, &world, id, "KataGo", &mut params);
    out(&mut wb, id, &kata_info("D4", 200));
    let comment = wb.view(false).comment_with_analysis.unwrap_or_default();
    assert!(comment.ends_with("贴目: 7.5 广度:0.5"), "{comment}");
    wb.navigate(Navigation::Back(1));
    answer_params(&mut wb, &world, id, "KataGo", &mut params);

    // Unchecked, the engine searches with 0: a known 0, saved but not shown.
    wb.set_kata_param(KataParam::Wrn, false, 0.5).unwrap();
    answer_params(&mut wb, &world, id, "KataGo", &mut params);
    out(&mut wb, id, &kata_info("D4", 20));
    let (text, wrn) = saved_breadth(&mut wb);
    assert_eq!(wrn, Some(0.0), "{text}");
    assert!(text.contains(" rootVisits=20 wrn=0.0"), "{text}");
    wb.navigate(Navigation::Forward(1));
    answer_params(&mut wb, &world, id, "KataGo", &mut params);
    out(&mut wb, id, &kata_info("D4", 20));
    let comment = wb.view(false).comment_with_analysis.unwrap_or_default();
    assert!(comment.ends_with("贴目: 7.5"), "{comment}");

    // An engine that confirmed no breadth: unknown.
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    out(&mut wb, id, &kata_info("Q16", 200));
    let (text, wrn) = saved_breadth(&mut wb);
    assert_eq!(wrn, None, "{text}");
    assert!(!text.contains("wrn="), "{text}");
}

#[test]
fn kata_parameters_need_a_ready_katago_in_the_foreground() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    assert_eq!(wb.set_kata_param(KataParam::Wrn, true, 0.5).unwrap(), Some("LizzieFrame.setParamNoEngineHint"));
    wb.save_engine_settings(vec![profile("Leela", "/opt/leelaz")], EngineStartup::Default).unwrap();
    wb.start_engines();
    let id = world.launched()[0].0;
    let mut params = engine_params("0", "0");
    answer_params(&mut wb, &world, id, "Leela Zero", &mut params);
    let v = wb.view(false).engine.kata_params;
    assert_eq!((v.pda_unavailable.as_deref(), v.wrn_unavailable.as_deref()), (Some("SetKataRules.notKataGoHint"), Some("SetKataRules.notKataGoHint")));
    let edit = KataEngineParamsEdit { pda: None, wrn: row(true, Some(0.5), true), show_pda: true, show_wrn: true };
    assert_eq!(wb.save_kata_engine_params(edit).unwrap(), Some("SetKataRules.notKataGoHint"));
    assert!(world.take(id).is_empty());
    assert!(!wb.config().engine.kata_params.auto_load_wrn, "a refused window saves nothing");
}

const LEELA: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nlz-analyze\nlz-setoption\nclear_cache";

fn option(enabled: bool, auto_load: bool, value: &str) -> LeelaOptionSetting {
    LeelaOptionSetting { enabled, auto_load, value: value.into() }
}

/// SetLeelaEngines 确定 sends each row with 修改 checked at once (an empty value sends nothing)
/// and the engine searches again; Leelaz.setLeelaSaiEnginePara sends the rows with 修改 and
/// 自动加载 on every Leela Zero / Sai start. 自动加载 keeps its tick while 修改 is off.
#[test]
fn the_leela_window_sends_checked_options_now_and_auto_loaded_ones_on_every_start() {
    for name in ["Leela Zero", "sai 0.18"] {
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let (mut wb, id, start) = start_family(dir.path(), &world, name, LEELA);
        assert!(!start.iter().any(|c| c.starts_with("lz-setoption")), "nothing is auto-loaded yet: {start:?}");
        let options = LeelaOptionSettings {
            memory: option(true, true, "4096"),
            visits: option(true, false, "800"),
            lagbuffer: option(false, true, "5"),
            resign: option(true, true, " "),
        };
        assert_eq!(wb.save_leela_engine_options(options.clone()).unwrap(), None);
        let sent = answer_family(&mut wb, &world, id, name, LEELA);
        assert_eq!(
            sent,
            [
                "lz-setoption name Maximum Memory Use (MiB) value 4096",
                "lz-setoption name Visits value 800",
                "lz-analyze B 10",
            ],
            "{name}: the analysis searches again after the options"
        );
        assert_eq!(wb.config().engine.leela_options, options, "{name}: the rows are saved as they are");

        wb.restart_engine().unwrap();
        let id = world.launched().last().unwrap().0;
        let start = answer_family(&mut wb, &world, id, name, LEELA);
        let set: Vec<&String> = start.iter().filter(|c| c.starts_with("lz-setoption")).collect();
        assert_eq!(set, ["lz-setoption name Maximum Memory Use (MiB) value 4096"], "{name}: {start:?}");
        let analyse = start.iter().position(|c| c.starts_with("lz-analyze")).unwrap();
        assert!(start.iter().position(|c| c.starts_with("lz-setoption")).unwrap() < analyse);
    }
}

/// Only Leela Zero and Sai take `lz-setoption`: KataGo has its own window, and any other engine
/// (Sayuri; Pachi, which Java counts as Leela for analysis) gets Java's not-Leela/Sai notice
/// instead of options it would refuse; none gets the auto-loaded options at start-up.
#[test]
fn leela_options_reach_only_leela_zero_and_sai() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let options = LeelaOptionSettings { visits: option(true, true, "800"), ..LeelaOptionSettings::default() };
    assert_eq!(wb.save_leela_engine_options(options.clone()).unwrap(), Some("LizzieFrame.setParamNoEngineHint"));
    wb.save_engine_settings(
        vec![
            profile("Leela", "/opt/leelaz"),
            profile("KataGo", "/opt/katago"),
            profile("Sayuri", "/opt/sayuri"),
            profile("Pachi", "/opt/pachi"),
        ],
        EngineStartup::Default,
    )
    .unwrap();
    wb.start_engines();
    let leela = world.launched()[0].0;
    answer_family(&mut wb, &world, leela, "Leela Zero", LEELA);
    assert!(wb.view(false).engine.foreground.unwrap().leela_sai);
    assert_eq!(wb.save_leela_engine_options(options.clone()).unwrap(), None);
    answer_family(&mut wb, &world, leela, "Leela Zero", LEELA);

    for (index, name) in [(1, "KataGo"), (2, "Sayuri"), (3, "Pachi 12.84")] {
        assert!(wb.switch_engine(index).unwrap());
        let id = world.launched().last().unwrap().0;
        let start = answer_family(&mut wb, &world, id, name, &format!("{LEELA}\nkata-analyze"));
        assert!(!start.iter().any(|c| c.starts_with("lz-setoption")), "{name}: {start:?}");
        assert!(!wb.view(false).engine.foreground.unwrap().leela_sai, "{name}");
        let other = LeelaOptionSettings::default();
        assert_eq!(wb.save_leela_engine_options(other).unwrap(), Some("LizzieFrame.setParamsWarning"), "{name}");
        assert!(world.take(id).is_empty(), "{name}");
        assert_eq!(wb.config().engine.leela_options, options, "{name}: a refused window saves nothing");
    }
}

/// Answers the start-up of a `KataGoPda` engine: its name, the two private queries with their
/// labelled bodies, and every other command as the ordinary helper does.
fn answer_all_pda(wb: &mut Workbench, world: &World, id: InstanceId, list: &str) -> Vec<String> {
    let mut done = Vec::new();
    loop {
        let pending = world.take(id);
        if pending.is_empty() {
            return done;
        }
        for line in pending {
            out(wb, id, "");
            let n = number(&line);
            let cmd = line.split_once(' ').unwrap().1.to_string();
            match cmd.split(' ').next().unwrap() {
                "name" => answer(wb, id, n, "KataGoPda 1.16.4"),
                "version" => answer(wb, id, n, "1.16.4"),
                "list_commands" => answer(wb, id, n, list),
                "kata-get-rules" => answer(
                    wb,
                    id,
                    n,
                    r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#,
                ),
                "getpda" => answer(wb, id, n, "pda: 0.25"),
                "getdympdacap" => answer(wb, id, n, "PDACap: 0.5"),
                "kata-analyze" | "lz-analyze" => out(wb, id, &format!("={n}")),
                _ => answer(wb, id, n, ""),
            }
            done.push(cmd);
        }
    }
}

fn started_pda_with(
    dir: &std::path::Path,
    world: &World,
    settings: CustomPdaSettings,
) -> (Workbench, InstanceId, Vec<String>) {
    let mut wb = workbench(dir, world);
    if settings != CustomPdaSettings::default() {
        let _ = wb.save_custom_pda(settings).unwrap();
    }
    wb.save_engine_settings(vec![profile("KataGoPda", "/opt/katago")], EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let id = world.launched()[0].0;
    let sent = answer_all_pda(&mut wb, world, id, KATA);
    (wb, id, sent)
}

fn started_pda(dir: &std::path::Path, world: &World) -> (Workbench, InstanceId) {
    let (wb, id, _) = started_pda_with(dir, world, CustomPdaSettings::default());
    (wb, id)
}

fn commands(sent: &[String]) -> Vec<String> {
    sent.iter().map(|line| line.split_once(' ').unwrap().1.to_string()).collect()
}

fn dynamic(cap: f64, auto_apply: bool) -> CustomPdaSettings {
    CustomPdaSettings {
        mode: CustomPdaMode::Dynamic,
        dynamic_cap: Some(cap),
        fixed_value: None,
        auto_apply,
    }
}

fn pda_candidates() -> String {
    [
        "info move D4 visits 50 winrate 0.6 scoreLead 1.5 prior 0.2 order 0 pv D4",
        "info move Q16 visits 40 winrate 0.55 scoreLead 0.5 prior 0.2 order 1 pv Q16",
        "info move C3 visits 10 winrate 0.5 scoreLead -0.5 prior 0.2 order 2 pv C3 rootInfo visits 50",
    ]
    .join(" ")
}

#[test]
fn kata_go_pda_start_up_queries_its_private_values() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, _, sent) = started_pda_with(dir.path(), &world, CustomPdaSettings::default());
    assert!(sent.contains(&"getpda".to_string()), "{sent:?}");
    assert!(sent.contains(&"getdympdacap".to_string()), "{sent:?}");
    assert!(!sent.iter().any(|c| c == "pda 0"), "自动套用 is off: {sent:?}");
    let pda = wb.view(false).engine.custom_pda;
    assert_eq!(pda.pda, Some(0.25));
    assert_eq!(pda.cap, Some(0.5));
    assert_eq!(pda.mode, CustomPdaMode::Dynamic);
}

#[test]
fn a_standard_katago_never_receives_the_private_queries_or_settings() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    world.take(id);
    assert_eq!(wb.save_custom_pda(dynamic(0.5, false)).unwrap(), None);
    assert!(world.sent(id).is_empty(), "a standard KataGo gets no pda command");
    assert_eq!(wb.config().engine.custom_pda, dynamic(0.5, false));
    assert_eq!(wb.view(false).engine.custom_pda.mode, CustomPdaMode::Dynamic);
}

#[test]
fn the_dialog_sends_only_a_kata_go_pda_engine_its_settings() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started_pda(dir.path(), &world);
    world.take(id);

    let fixed = CustomPdaSettings {
        mode: CustomPdaMode::Fixed,
        dynamic_cap: None,
        fixed_value: Some(2.0),
        auto_apply: false,
    };
    assert_eq!(wb.save_custom_pda(fixed.clone()).unwrap(), None);
    assert_eq!(commands(&world.take(id)), ["pda 2.0"]);
    assert_eq!(wb.config().engine.custom_pda, fixed);
    assert_eq!(wb.view(false).engine.custom_pda.pda, Some(2.0));

    // An empty value is saved but sends nothing.
    let empty = CustomPdaSettings {
        mode: CustomPdaMode::Dynamic,
        dynamic_cap: None,
        fixed_value: None,
        auto_apply: false,
    };
    assert_eq!(wb.save_custom_pda(empty.clone()).unwrap(), None);
    assert!(world.sent(id).is_empty());
    assert_eq!(wb.config().engine.custom_pda, empty);

    // A zero 系数 is the dialog's wrongParameter and changes nothing.
    assert_eq!(
        wb.save_custom_pda(dynamic(0.0, false)).unwrap(),
        Some("SetKataPDA.wrongParameter")
    );
    assert_eq!(wb.config().engine.custom_pda, empty);
}

#[test]
fn auto_apply_follows_the_two_start_up_answers_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, _, sent) = started_pda_with(dir.path(), &world, dynamic(0.5, true));
    let at = |needle: &str| sent.iter().position(|c| c == needle).unwrap_or_else(|| panic!("{needle}: {sent:?}"));
    assert!(at("getpda") < at("getdympdacap"));
    // Java replays the saved `AutoPDA` (`dympdacap <cap>`) alone; the engine keeps its own PDA.
    assert!(at("getdympdacap") < at("dympdacap 0.5"));
    assert!(!sent.iter().any(|c| c == "pda 0"), "{sent:?}");
    let pda = wb.view(false).engine.custom_pda;
    assert_eq!((pda.pda, pda.cap), (Some(0.25), Some(0.5)));
}

#[test]
fn unsolicited_private_labels_update_the_current_values() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started_pda(dir.path(), &world);
    out(&mut wb, id, "pda: 0.75");
    out(&mut wb, id, "PDACap: 0.25");
    out(&mut wb, id, "");
    let pda = wb.view(false).engine.custom_pda;
    assert_eq!((pda.pda, pda.cap), (Some(0.75), Some(0.25)));
}

#[test]
fn priority_points_reach_a_custom_engine_and_are_kept_beyond_the_candidate_limit() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started_pda(dir.path(), &world);
    analysis_settings(&mut wb, |s| s.limit_max_suggestion = 2);
    let _ = answer_all_pda(&mut wb, &world, id, KATA);
    world.take(id);
    out(&mut wb, id, &pda_candidates());
    let c3 = Point { x: 2, y: 16 };
    assert!(wb.set_priority_point(c3).unwrap());
    assert_eq!(commands(&world.take(id)), ["setmaxpolicy C3 1.1"]);
    let view = wb.view(false);
    assert_eq!(view.engine.priority_points, [c3]);
    assert!(
        view.analysis.unwrap().candidates.iter().any(|c| c.point == Some(c3)),
        "the priority point stays in the stored candidates beyond 最大候选点数"
    );
}

#[test]
fn clear_priority_sends_clearpolicy_and_leaving_the_position_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started_pda(dir.path(), &world);
    world.take(id);
    let point = Point { x: 15, y: 3 };
    assert!(wb.set_priority_point(point).unwrap());
    world.take(id);
    assert!(wb.clear_priority_points().unwrap());
    assert_eq!(commands(&world.take(id)), ["clearpolicy"]);
    assert!(wb.view(false).engine.priority_points.is_empty());

    // A move clears the record without a command.
    let _ = wb.set_priority_point(point).unwrap();
    world.take(id);
    wb.edit(EditCommand::Play { point: Point { x: 3, y: 15 }, color: Some(Stone::Black) })
        .unwrap();
    assert!(wb.view(false).engine.priority_points.is_empty());
    assert!(!commands(&world.sent(id)).iter().any(|c| c == "clearpolicy"));
}

#[test]
fn priority_is_refused_for_a_standard_katago() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    world.take(id);
    assert!(!wb.set_priority_point(Point { x: 3, y: 15 }).unwrap());
    assert!(!wb.clear_priority_points().unwrap());
    assert!(world.sent(id).is_empty());
}

#[test]
fn setting_priority_resumes_a_paused_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started_pda(dir.path(), &world);
    assert!(wb.toggle_analysis().unwrap());
    let _ = answer_all_pda(&mut wb, &world, id, KATA);
    world.take(id);
    assert!(wb.set_priority_point(Point { x: 3, y: 15 }).unwrap());
    let sent = commands(&world.take(id));
    assert!(sent.iter().any(|c| c.starts_with("kata-analyze")), "{sent:?}");
    assert!(sent.iter().any(|c| c == "setmaxpolicy D4 1.1"), "{sent:?}");
}

#[test]
fn auto_apply_runs_on_a_handicap_games_first_white_move_only() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id, _) = started_pda_with(dir.path(), &world, dynamic(0.5, true));
    world.take(id);

    // A move on an ordinary empty board is not a handicap game's first White move.
    wb.edit(EditCommand::Play { point: Point { x: 3, y: 15 }, color: Some(Stone::Black) })
        .unwrap();
    assert!(!commands(&world.sent(id)).iter().any(|c| c == "dympdacap 0.5"));

    // Handicap stones as root setup (this product's form) and White to play.
    open(&mut wb, "(;SZ[19]HA[2]AB[dd][pp]PL[W])");
    world.take(id);
    wb.edit(EditCommand::Play { point: Point { x: 15, y: 3 }, color: Some(Stone::White) })
        .unwrap();
    let sent = commands(&world.take(id));
    assert!(sent.contains(&"dympdacap 0.5".to_string()), "{sent:?}");
    assert!(!sent.contains(&"pda 0".to_string()), "only the saved AutoPDA command: {sent:?}");
    assert_eq!(wb.view(false).engine.custom_pda.pda, Some(0.25), "the engine's own PDA stays shown");
}

#[test]
fn a_custom_toolbar_pda_edit_leaves_the_standard_preferences_alone() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started_pda(dir.path(), &world);
    world.take(id);
    let before = (wb.config().engine.kata_pda, wb.config().engine.kata_pda_enabled);
    assert_eq!(wb.set_kata_param(KataParam::Pda, true, 1.5).unwrap(), None);
    assert_eq!(commands(&world.take(id)), ["kata-set-param playoutDoublingAdvantage 1.5"]);
    assert_eq!((wb.config().engine.kata_pda, wb.config().engine.kata_pda_enabled), before);
}

#[test]
fn priority_resumes_automatically_stopped_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started_pda(dir.path(), &world);
    analysis_settings(&mut wb, |s| s.stop_at_empty_board = true);
    answer_all_pda(&mut wb, &world, id, KATA);
    out(&mut wb, id, &info("Q16", 5));
    assert!(wb.view(false).engine.stopped_by_limit);
    answer_all_pda(&mut wb, &world, id, KATA);
    wb.set_priority_point(Point { x: 3, y: 15 }).unwrap();
    let sent = answer_all_pda(&mut wb, &world, id, KATA);
    assert!(sent.iter().any(|c| c.starts_with("kata-analyze")), "{sent:?}");
    out(&mut wb, id, &info("D4", 50));
    assert!(!wb.view(false).engine.stopped_by_limit);
}

#[test]
fn handicap_first_white_pass_applies_pda_once() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id, _) = started_pda_with(dir.path(), &world, dynamic(0.5, true));
    open(&mut wb, "(;SZ[19]HA[2]AB[dd][pp]PL[W])");
    answer_all_pda(&mut wb, &world, id, KATA);
    wb.edit(EditCommand::Pass).unwrap();
    let sent = answer_all_pda(&mut wb, &world, id, KATA);
    assert!(sent.iter().any(|c| c == "dympdacap 0.5"), "{sent:?}");
    wb.edit(EditCommand::Pass).unwrap();
    wb.edit(EditCommand::Pass).unwrap();
    assert!(!commands(&world.take(id)).iter().any(|c| c == "dympdacap 0.5"));
}

/// Java `B11ModelNotice`: the title's B11 speed notice follows the model this engine process
/// loads. A local model is known from its header (a renamed B11 counts, a B11 file name with
/// another header or an unreadable model does not), read off the workbench; a replaced
/// process's late answer never reaches its successor. A Zhizi connection is known from the
/// model it selected; an indirect launcher's or SSH host's model paths are not this host's and
/// never show it.
#[test]
fn the_b11_speed_notice_follows_the_model_the_engine_loads() {
    use lizzie_core::engine::cloud::{CloudLaunch, CloudProvider};
    let tmp = tempfile::tempdir().unwrap();
    // A folder with a space, as a user's temporary folder can be: each path is one argument.
    let root = tmp.path().join("test root");
    std::fs::create_dir(&root).unwrap();
    let renamed = root.join("renamed.bin");
    std::fs::write(&renamed, "kata1-tf3-b11c768-s12002M-d6304M\n16\n19\n19\nrest").unwrap();
    let misnamed = root.join("kata1-tf3-b11c768-s1M-d1M.bin");
    std::fs::write(&misnamed, "kata1-b6c96-s175395328-d26788732\n16\n19\n19\nrest").unwrap();
    let local = |name: &str, command: String| EngineProfile { name: name.into(), command, ..EngineProfile::default() };
    let zhizi = |name: &str, args: &str| EngineProfile {
        name: name.into(),
        command: "remote-compute://zhizi".into(),
        cloud: Some(CloudLaunch { provider: CloudProvider::Zhizi, identifier: String::new(), args: args.into(), url: String::new() }),
        ..EngineProfile::default()
    };
    let world = World::default();
    let mut wb = workbench(tmp.path(), &world);
    wb.save_engine_settings(
        vec![
            local("Renamed", format!("/opt/katago gtp -model \"{}\"", renamed.display())),
            local("Misnamed", format!("/opt/katago gtp -model \"{}\"", misnamed.display())),
            local("Equals", format!("/opt/katago gtp --model=\"{}\"", renamed.display())),
            local("Script", format!("\"{}\" gtp -model \"{}\"", root.join("katago.sh").display(), renamed.display())),
            zhizi("Zhizi B11", "--gpu-type 1x --kata-weight 11B768T"),
            zhizi("Zhizi default", "--gpu-type 1x"),
            EngineProfile {
                ssh: Some(lizzie_core::engine::ssh::SshSettings::default()),
                ..local("SSH", format!("/opt/katago gtp -model \"{}\"", renamed.display()))
            },
            local("Missing", format!("/opt/katago gtp -model \"{}\"", root.join("missing.bin.gz").display())),
        ],
        EngineStartup::Default,
    )
    .unwrap();
    let b11 = |wb: &mut Workbench| wb.view(false).engine.foreground.unwrap().b11;
    wb.start_engines();
    answer_all(&mut wb, &world, world.launched()[0].0, KATA);
    assert!(!b11(&mut wb), "unknown until the header is read");
    let stale = wb.take_model_lookups();
    assert_eq!(stale.len(), 1);
    assert!(wb.take_model_lookups().is_empty(), "each lookup is handed out once");

    wb.restart_engine().unwrap();
    answer_all(&mut wb, &world, world.launched().last().unwrap().0, KATA);
    let current = wb.take_model_lookups();
    for found in stale {
        wb.finish_model_lookup(found.run());
    }
    assert!(!b11(&mut wb), "the replaced process's answer is not the new one's");
    for found in current {
        assert!(wb.finish_model_lookup(found.run()));
    }
    assert!(b11(&mut wb), "a renamed B11 is known by its header");

    for (index, expected) in [(1, false), (2, true), (3, false), (4, true), (5, false), (6, false), (7, false)] {
        assert!(wb.switch_engine(index).unwrap());
        answer_all(&mut wb, &world, world.launched().last().unwrap().0, KATA);
        for found in wb.take_model_lookups() {
            wb.finish_model_lookup(found.run());
        }
        assert_eq!(b11(&mut wb), expected, "engine {index}");
    }
    assert!(wb.switch_engine(0).unwrap());
    assert!(b11(&mut wb), "a reused process keeps its model");
}

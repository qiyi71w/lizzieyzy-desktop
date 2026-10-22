//! 闪电分析 through the public `Workbench` operations with controlled processes: the targets of
//! 整盘速览 / 局部精析 / 全部分支精析, an analysis process and the borrowed foreground engine, the
//! user's pause, cancellation and failures, the hand-over from a running automatic quick analysis
//! (`MANUAL_FLASH`), and a record switch while the engine is borrowed.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lizzie_core::api::{FlashOutcome, FlashTarget, OpenSource, UnrestoredReason};
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats::sgf;
use lizzie_core::record::RecordFormat;
use lizzie_core::settings::{AnalysisEngineSettings, Storage};
use lizzie_core::workbench::{OpenOrigin, Workbench, ANALYSIS_COMMAND_MISSING};

#[derive(Default)]
struct Proc {
    program: String,
    args: Vec<String>,
    sent: Vec<String>,
    terminated: bool,
    held: Vec<String>,
}

#[derive(Clone, Default)]
struct World(Arc<Mutex<Vec<(InstanceId, Proc)>>>);

impl World {
    fn take(&self, id: InstanceId) -> Vec<String> {
        let mut w = self.0.lock().unwrap();
        w.iter_mut()
            .find(|p| p.0 == id)
            .map(|p| std::mem::take(&mut p.1.sent))
            .unwrap_or_default()
    }
    fn workers(&self) -> Vec<InstanceId> {
        let w = self.0.lock().unwrap();
        w.iter()
            .filter(|p| p.1.args.first().is_some_and(|a| a == "analysis"))
            .map(|p| p.0)
            .collect()
    }
    fn engine(&self, program: &str) -> InstanceId {
        let w = self.0.lock().unwrap();
        w.iter().rev().find(|p| p.1.program == program).map(|p| p.0).unwrap()
    }
    fn terminated(&self, id: InstanceId) -> bool {
        self.0.lock().unwrap().iter().any(|p| p.0 == id && p.1.terminated)
    }
    fn hold(&self, id: InstanceId, line: String) {
        let mut w = self.0.lock().unwrap();
        w.iter_mut().find(|p| p.0 == id).unwrap().1.held.push(line);
    }
    fn release_held(&self, id: InstanceId) {
        let mut w = self.0.lock().unwrap();
        let p = &mut w.iter_mut().find(|p| p.0 == id).unwrap().1;
        let held = std::mem::take(&mut p.held);
        p.sent.extend(held);
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
            .1
            .sent
            .push(line.to_string());
        Ok(())
    }
    fn terminate(self: Box<Self>) {
        let mut w = self.world.0.lock().unwrap();
        w.iter_mut().find(|p| p.0 == self.id).unwrap().1.terminated = true;
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
                ..Proc::default()
            },
        ));
        Ok(Box::new(FakeIo {
            id,
            world: self.0.clone(),
        }))
    }
}

const KATA: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nset_position\nkata-analyze\nkata-set-rules\nkata-get-rules\nstop";
const RECORD: &str = "(;SZ[19];B[pd];W[dd];B[pp];W[dp])";
const ANALYSIS: &str = "/opt/katago analysis -config /a.cfg";
const CHINESE: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;

fn workbench(dir: &std::path::Path, world: &World) -> Workbench {
    Workbench::new(
        Storage::in_dirs(&dir.join("config"), &dir.join("data")),
        None,
        Box::new(FakeLauncher(world.clone())),
        Vec::new(),
    )
}

/// Flash analysis with an analysis process (`reuse` false) or the borrowed foreground engine;
/// automatic quick analysis on or off.
fn configure(wb: &mut Workbench, reuse: bool, quick: bool) {
    let settings = AnalysisEngineSettings {
        command: ANALYSIS.into(),
        reuse_current_engine: reuse,
        ..AnalysisEngineSettings::default()
    };
    wb.save_analysis_engine_settings(settings, "", true).unwrap().unwrap();
    wb.set_auto_quick_analyze(quick).unwrap();
}

fn out(wb: &mut Workbench, id: InstanceId, line: &str) {
    wb.engine_input(id, EngineInput::Stdout(line.to_string()));
}

/// Answers every outstanding GTP command of `id`; a `kata-analyze` gets its header and one line
/// of `visits` (0: silent); `name` is answered only when `name` is set. Returns the commands.
fn serve(wb: &mut Workbench, world: &World, id: InstanceId, visits: u64, name: bool) -> Vec<String> {
    let mut done = Vec::new();
    loop {
        let pending = world.take(id);
        if pending.is_empty() {
            return done;
        }
        for line in pending {
            out(wb, id, "");
            let n: u64 = line.split(' ').next().unwrap().parse().unwrap();
            let cmd = line.split_once(' ').unwrap().1.to_string();
            let reply = |wb: &mut Workbench, text: &str| {
                out(wb, id, &format!("={n} {text}"));
                out(wb, id, "");
            };
            match cmd.split(' ').next().unwrap() {
                "name" if !name => world.hold(id, line.clone()),
                "name" => reply(wb, "KataGo"),
                "version" => reply(wb, "1.18.2"),
                "list_commands" => reply(wb, KATA),
                "kata-get-rules" => reply(wb, CHINESE),
                "kata-analyze" => {
                    out(wb, id, &format!("={n}"));
                    if visits > 0 {
                        out(wb, id, &format!(
                            "info move K10 visits {visits} winrate 0.5 scoreLead 0.5 prior 0.2 order 0 pv K10 rootInfo visits {visits} winrate 0.4 scoreLead -0.5"
                        ));
                    }
                }
                _ => reply(wb, ""),
            }
            done.push(cmd);
        }
    }
}

/// The analysis queries sent to `worker` so far.
fn queries(world: &World, worker: InstanceId) -> Vec<serde_json::Value> {
    world
        .take(worker)
        .iter()
        .map(|q| serde_json::from_str(q).unwrap())
        .collect()
}

fn answer(wb: &mut Workbench, worker: InstanceId, q: &serde_json::Value) {
    let reply = serde_json::json!({
        "id": q["id"],
        "moveInfos": [{"move": "K10", "visits": 2, "winrate": 0.5, "scoreLead": 1.0, "order": 0, "pv": ["K10"]}],
        "rootInfo": {"visits": 2, "winrate": 0.5, "scoreLead": 1.0},
    });
    out(wb, worker, &reply.to_string());
}

/// The last move of each query, as GTP coordinates.
fn last_moves(qs: &[serde_json::Value]) -> Vec<String> {
    qs.iter()
        .map(|q| {
            q["moves"].as_array().unwrap().last().unwrap()[1]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect()
}

fn open(wb: &mut Workbench, text: &str) -> bool {
    let t = wb.begin_open(OpenOrigin::File {
        path: "/x.sgf".into(),
        source: OpenSource::Drop,
    });
    wb.complete_open(t, Ok((RecordFormat::Sgf, sgf::read(text).unwrap())))
}

fn with_engine(wb: &mut Workbench, world: &World) -> InstanceId {
    let profile = EngineProfile {
        name: "KataGo".into(),
        command: "/opt/katago gtp".into(),
        ..EngineProfile::default()
    };
    wb.save_engine_settings(vec![profile], EngineStartup::Default).unwrap();
    wb.start_engines();
    let fg = world.engine("/opt/katago");
    serve(wb, world, fg, 0, true);
    fg
}

/// Whether the foreground engine was told to analyse (its own `kata-analyze`, not a lease's
/// finite search).
fn foreground_analyses(sent: &[String]) -> bool {
    sent.iter()
        .any(|c| c.starts_with("kata-analyze") && !c.contains("maxmoves"))
}

fn start(wb: &mut Workbench, target: FlashTarget) {
    assert_eq!(wb.start_flash_analysis(target).unwrap(), Ok(true));
}

#[test]
fn a_whole_game_run_asks_every_main_trunk_move_and_completes_with_the_foreground_back() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, false, false);
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    serve(&mut wb, &world, fg, 0, true);
    start(&mut wb, FlashTarget::WholeGame);

    let worker = *world.workers().last().expect("an analysis process");
    let v = wb.view(false);
    let f = v.flash.expect("running");
    assert_eq!((f.done, f.total, f.loading, f.borrowed), (0, 4, false, false));
    let fg_sent = serve(&mut wb, &world, fg, 0, true);
    assert!(
        fg_sent.contains(&"stop".to_string()) && !foreground_analyses(&fg_sent),
        "{fg_sent:?}"
    );
    let qs = queries(&world, worker);
    assert_eq!(last_moves(&qs), ["Q16", "D16", "Q4", "D4"]);
    assert!(qs.iter().all(|q| q["maxVisits"] == 2), "单步计算量 + 1");

    answer(&mut wb, worker, &qs[0]);
    assert_eq!(wb.view(false).flash.unwrap().done, 1);
    for q in &qs[1..] {
        answer(&mut wb, worker, q);
    }
    let v = wb.view(false);
    assert!(v.flash.is_none());
    assert!(matches!(v.flash_outcome, Some(FlashOutcome::Completed { .. })));
    assert!(wb.view(false).flash_outcome.is_none(), "reported once");
    assert_eq!(
        v.position.move_number, 1,
        "browsing at the root steps to the first move"
    );
    assert!(world.terminated(worker), "结束分析后退出引擎");
    assert!(!v.document.unsaved);
    assert_eq!(v.analysis.unwrap().engine, "闪电分析");
    assert!(
        foreground_analyses(&serve(&mut wb, &world, fg, 0, true)),
        "the foreground analyses again"
    );
}

#[test]
fn move_ranges_end_before_their_end_move_and_searched_moves_are_skipped_unless_overridden() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, false, false);
    let mut settings = wb.config().analysis_engine.clone();
    settings.auto_quit = false;
    wb.save_analysis_engine_settings(settings.clone(), "", false)
        .unwrap()
        .unwrap();
    open(&mut wb, RECORD);

    start(
        &mut wb,
        FlashTarget::MoveRange {
            start_move: 2,
            end_move: 4,
        },
    );
    let worker = *world.workers().last().unwrap();
    let qs = queries(&world, worker);
    assert_eq!(last_moves(&qs), ["D16", "Q4"], "moves 2 and 3");
    qs.iter().for_each(|q| answer(&mut wb, worker, q));
    assert!(!world.terminated(worker), "kept without 结束分析后退出引擎");
    let range = workbench(dir.path(), &World::default())
        .view(false)
        .settings
        .flash_range;
    assert_eq!((range.start_move, range.end_move), (2, 4), "the range is remembered");

    // The kept process answers the next run; moves with enough visits are not asked again.
    start(&mut wb, FlashTarget::WholeGame);
    assert_eq!(world.workers(), [worker]);
    let qs = queries(&world, worker);
    assert_eq!(last_moves(&qs), ["Q16", "D4"]);
    qs.iter().for_each(|q| answer(&mut wb, worker, q));
    settings.always_override = true;
    wb.save_analysis_engine_settings(settings, "", false).unwrap().unwrap();
    start(&mut wb, FlashTarget::WholeGame);
    assert_eq!(queries(&world, worker).len(), 4, "总是覆盖 asks every move");
}

#[test]
fn all_branches_are_asked_depth_first_with_the_main_line_first() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, false, false);
    open(&mut wb, "(;SZ[19];B[pd](;W[dd];B[pp])(;W[dp]))");
    start(&mut wb, FlashTarget::AllBranches);
    let worker = *world.workers().last().unwrap();
    let qs = queries(&world, worker);
    assert_eq!(last_moves(&qs), ["Q16", "D16", "Q4", "D4"]);
    let lengths: Vec<usize> = qs.iter().map(|q| q["moves"].as_array().unwrap().len()).collect();
    assert_eq!(lengths, [1, 2, 3, 2]);
}

#[test]
fn cancelling_keeps_the_answers_so_far_and_leaves_the_foreground_stopped() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, false, false);
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    start(&mut wb, FlashTarget::WholeGame);
    let worker = *world.workers().last().unwrap();
    let qs = queries(&world, worker);
    answer(&mut wb, worker, &qs[0]);
    serve(&mut wb, &world, fg, 0, true);

    assert!(wb.cancel_flash_analysis());
    let v = wb.view(false);
    assert!(v.flash.is_none() && v.flash_outcome.is_none());
    assert!(world.terminated(worker));
    // `destroyAnalysisEngine` does not resume the foreground engine.
    assert!(v.engine.paused);
    assert!(!foreground_analyses(&serve(&mut wb, &world, fg, 0, true)));
    // A late answer reaches nothing; the earlier one stays.
    answer(&mut wb, worker, &qs[1]);
    wb.navigate(Navigation::Forward(1));
    assert!(wb.view(false).analysis.is_some());
    wb.navigate(Navigation::Forward(1));
    assert!(wb.view(false).analysis.is_none());
    wb.toggle_analysis().unwrap();
    assert!(foreground_analyses(&serve(&mut wb, &world, fg, 0, true)));
}

#[test]
fn resuming_during_a_process_run_lets_the_foreground_analyse_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, false, false);
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    serve(&mut wb, &world, fg, 0, true);
    start(&mut wb, FlashTarget::WholeGame);
    assert!(!foreground_analyses(&serve(&mut wb, &world, fg, 0, true)));
    wb.toggle_analysis().unwrap();
    let v = wb.view(false);
    assert!(!v.engine.paused && v.flash.is_some(), "the run continues");
    assert!(foreground_analyses(&serve(&mut wb, &world, fg, 0, true)));
}

#[test]
fn a_borrowed_run_searches_each_move_and_completes_only_once_the_engine_is_back() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    serve(&mut wb, &world, fg, 0, true);
    start(&mut wb, FlashTarget::WholeGame);
    assert!(world.workers().is_empty());
    assert!(wb.view(false).flash.unwrap().borrowed);

    let sent = serve(&mut wb, &world, fg, 20, false);
    let searches: Vec<&String> = sent.iter().filter(|c| c.contains("maxmoves 1")).collect();
    assert_eq!(searches.len(), 4, "{sent:?}");
    let v = wb.view(false);
    assert_eq!(
        v.flash.as_ref().map(|f| (f.done, f.total)),
        Some((4, 4)),
        "still giving back"
    );
    assert!(v.flash_outcome.is_none());

    world.release_held(fg);
    let after = serve(&mut wb, &world, fg, 0, true);
    let v = wb.view(false);
    assert!(v.flash.is_none());
    assert!(matches!(v.flash_outcome, Some(FlashOutcome::Completed { .. })));
    assert_eq!(v.engine.unrestored, None);
    assert!(
        foreground_analyses(&after),
        "given back: the foreground analyses again {after:?}"
    );
    assert_eq!(v.problems.analyzed, 3);
}

#[test]
fn a_pause_during_a_borrowed_run_keeps_the_run_and_holds_after_the_hand_back() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    start(&mut wb, FlashTarget::WholeGame);
    // The engine is being taken over when the user pauses.
    wb.toggle_analysis().unwrap();
    let v = wb.view(false);
    assert!(v.engine.paused && v.flash.is_some());
    let sent = serve(&mut wb, &world, fg, 20, true);
    let v = wb.view(false);
    assert!(matches!(v.flash_outcome, Some(FlashOutcome::Completed { .. })));
    assert!(v.engine.paused);
    let name = sent.iter().rposition(|c| c == "name").unwrap();
    assert!(!foreground_analyses(&sent[name..]), "{sent:?}");
}

#[test]
fn a_failed_hand_back_is_reported_and_the_engine_stays_isolated() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    start(&mut wb, FlashTarget::WholeGame);
    serve(&mut wb, &world, fg, 20, false);
    wb.engine_tick(Instant::now() + Duration::from_secs(31));
    let v = wb.view(false);
    assert!(v.flash.is_none());
    assert_eq!(
        v.flash_outcome,
        Some(FlashOutcome::Failed {
            run: 1,
            message_key: "AnalysisEngine.foregroundRestoreFailed".into()
        })
    );
    assert_eq!(v.engine.unrestored, Some(UnrestoredReason::Restore));
    assert!(!foreground_analyses(&serve(&mut wb, &world, fg, 0, true)));
    assert_eq!(
        wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(),
        Err("AnalysisSettings.reuseStatus.engine_state_unrestored".into())
    );
}

#[test]
fn a_running_quick_analysis_hands_its_process_over() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, false, true);
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    let worker = *world.workers().last().expect("quick analysis process");
    let quick = queries(&world, worker);
    serve(&mut wb, &world, fg, 0, true);

    start(&mut wb, FlashTarget::WholeGame);
    let v = wb.view(false);
    assert!(!v.quick.running && v.flash.is_some());
    assert_eq!(world.workers(), [worker], "the same process (`MANUAL_FLASH`)");
    assert!(!world.terminated(worker));
    assert!(
        !foreground_analyses(&serve(&mut wb, &world, fg, 0, true)),
        "no resume in between"
    );
    // A late quick answer is not part of the run.
    answer(&mut wb, worker, &quick[0]);
    assert_eq!(wb.view(false).flash.unwrap().done, 0);
    let qs = queries(&world, worker);
    assert_eq!(qs.len(), 4);
    qs.iter().for_each(|q| answer(&mut wb, worker, q));
    assert!(matches!(
        wb.view(false).flash_outcome,
        Some(FlashOutcome::Completed { .. })
    ));
}

#[test]
fn a_borrowing_quick_analysis_gives_the_engine_back_before_the_run_borrows_it() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, true);
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    serve(&mut wb, &world, fg, 0, false);
    assert!(wb.view(false).quick.borrowed);

    start(&mut wb, FlashTarget::WholeGame);
    let v = wb.view(false);
    assert!(v.flash.unwrap().loading, "waits for the hand-back");
    world.release_held(fg);
    let sent = serve(&mut wb, &world, fg, 20, true);
    // The quick lease's `name`, then the run's take-over: nothing in between analyses.
    let returned = sent.iter().position(|c| c == "name").unwrap();
    let taken = returned + sent[returned..].iter().position(|c| c == "kata-get-rules").unwrap();
    assert!(!foreground_analyses(&sent[returned..taken]), "{sent:?}");
    let v = wb.view(false);
    assert!(matches!(v.flash_outcome, Some(FlashOutcome::Completed { .. })));
    assert_eq!(v.problems.analyzed, 3);
    assert_eq!(v.engine.unrestored, None);
}

#[test]
fn opening_a_record_during_a_borrowed_run_waits_for_the_hand_back_and_ends_it_silently() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    start(&mut wb, FlashTarget::WholeGame);
    serve(&mut wb, &world, fg, 0, false);
    let before = wb.view(false).document.id;
    assert!(open(&mut wb, "(;SZ[19];B[qd])"));
    let v = wb.view(false);
    assert_eq!(v.document.id, before, "not adopted while the engine is borrowed");
    assert!(v.flash.is_none());
    world.release_held(fg);
    serve(&mut wb, &world, fg, 20, true);
    let v = wb.view(false);
    assert_ne!(v.document.id, before);
    assert!(v.flash_outcome.is_none());
    assert_eq!(v.engine.unrestored, None);
    // The new record's move has no result; only the foreground engine analyses the root.
    wb.navigate(Navigation::Forward(1));
    assert!(
        wb.view(false).analysis.is_none(),
        "nothing of the old run reaches the new record"
    );
}

#[test]
fn a_run_is_refused_without_a_command_or_an_engine_to_borrow() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    open(&mut wb, RECORD);
    assert_eq!(
        wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(),
        Err("AnalysisSettings.reuseStatus.no_foreground_engine".into())
    );
    let config = wb.storage().config_file.clone();
    drop(wb);
    let mut json: serde_json::Value = serde_json::from_slice(&std::fs::read(&config).unwrap()).unwrap();
    json["analysisEngine"]["command"] = "".into();
    json["analysisEngine"]["reuseCurrentEngine"] = false.into();
    std::fs::write(&config, json.to_string()).unwrap();
    let mut wb = workbench(dir.path(), &world);
    assert_eq!(
        wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(),
        Err(ANALYSIS_COMMAND_MISSING.into())
    );
    assert!(world.workers().is_empty() && wb.view(false).flash.is_none());
}

#[test]
fn a_failed_analysis_process_is_reported_and_the_run_can_be_redone() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, false, false);
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    serve(&mut wb, &world, fg, 0, true);
    start(
        &mut wb,
        FlashTarget::MoveRange {
            start_move: 3,
            end_move: -1,
        },
    );
    serve(&mut wb, &world, fg, 0, true);
    let worker = *world.workers().last().unwrap();
    wb.engine_input(worker, EngineInput::Exited(Some(1)));
    let v = wb.view(false);
    assert!(v.flash.is_none() && v.engine_failure.is_some());
    assert!(matches!(v.flash_outcome, Some(FlashOutcome::EngineFailed { .. })));
    // `finishAbortedAnalysis` does not resume the foreground engine.
    assert!(v.engine.paused);
    assert!(!foreground_analyses(&serve(&mut wb, &world, fg, 0, true)));
    assert_eq!(wb.redo_flash_analysis().unwrap(), Ok(true));
    let again = *world.workers().last().unwrap();
    assert_ne!(again, worker);
    assert_eq!(
        last_moves(&queries(&world, again)),
        ["Q4", "D4"],
        "the same range again"
    );
}

#[test]
fn a_rejected_query_fails_the_run_instead_of_completing_it() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, false, false);
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    serve(&mut wb, &world, fg, 0, true);
    start(
        &mut wb,
        FlashTarget::MoveRange {
            start_move: 1,
            end_move: 3,
        },
    );
    serve(&mut wb, &world, fg, 0, true);
    let worker = *world.workers().last().unwrap();
    let qs = queries(&world, worker);
    answer(&mut wb, worker, &qs[0]);
    let error = serde_json::json!({"id": qs[1]["id"], "error": "bad query"});
    out(&mut wb, worker, &error.to_string());
    let v = wb.view(false);
    assert!(v.flash.is_none());
    assert_eq!(
        v.flash_outcome,
        Some(FlashOutcome::Failed {
            run: 1,
            message_key: "AnalysisEngine.requestDispatchFailed".into()
        })
    );
    assert_eq!(v.position.move_number, 0, "no completion step to the first move");
    wb.navigate(Navigation::Forward(1));
    assert_eq!(
        wb.view(false).analysis.unwrap().engine,
        "闪电分析",
        "the earlier answer stays"
    );
    // `finishFailedRequestDispatch` resumes the foreground engine; the process stays.
    assert!(foreground_analyses(&serve(&mut wb, &world, fg, 0, true)));
    assert!(!world.terminated(worker));
}

#[test]
fn a_preload_while_a_run_holds_the_analysis_process_starts_no_second_one() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, false, false);
    let mut settings = wb.config().analysis_engine.clone();
    settings.preload = true;
    wb.save_analysis_engine_settings(settings, "", false).unwrap().unwrap();
    open(&mut wb, RECORD);
    start(&mut wb, FlashTarget::WholeGame);
    let worker = *world.workers().last().unwrap();
    // The foreground engine becomes ready during the run.
    with_engine(&mut wb, &world);
    assert_eq!(world.workers(), [worker]);
}

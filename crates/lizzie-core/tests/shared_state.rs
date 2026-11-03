//! Shared-state conflicts between modules that the per-mode suites cover only separately, through
//! the public `Workbench` operations with controlled processes: a save receipt against an edit
//! and automatic quick analysis results (R5, R6); focus tracking across an edit, browsing, the
//! user's pause and an engine switch (Focus rule, R7); the foreground analysis lease handed from
//! quick to flash analysis, cancelled or stopped borrowed runs and a record opened while a
//! borrowed engine returns, each under the user's pause (R7, R8); and background batches beside
//! the open record (R5, T17).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use lizzie_core::api::{
    BatchMode, EditCommand, FlashOutcome, FlashTarget, OpenSource, SaveOutcome, TrackingCommand, WholeGameState,
};
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, FocusCapability, InstanceId, Launcher};
use lizzie_core::formats::{self, sgf};
use lizzie_core::record::{Point, RecordFormat, ROOT};
use lizzie_core::settings::{AnalysisEngineSettings, AutoAnalysisSettings, Storage};
use lizzie_core::workbench::{OpenOrigin, Workbench};

// ---------------------------------------------------------------------------- controlled processes

#[derive(Default)]
struct Proc {
    program: String,
    args: Vec<String>,
    sent: Vec<String>,
    /// Commands left unanswered so far.
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
    /// Analysis processes started so far, oldest first.
    fn workers(&self) -> Vec<InstanceId> {
        let w = self.0.lock().unwrap();
        w.iter()
            .filter(|p| p.1.args.first().is_some_and(|a| a == "analysis"))
            .map(|p| p.0)
            .collect()
    }
    /// The latest process of `program`.
    fn engine(&self, program: &str) -> InstanceId {
        let w = self.0.lock().unwrap();
        w.iter().rev().find(|p| p.1.program == program).map(|p| p.0).unwrap()
    }
    fn hold(&self, id: InstanceId, line: String) {
        let mut w = self.0.lock().unwrap();
        w.iter_mut().find(|p| p.0 == id).unwrap().1.held.push(line);
    }
    /// The commands left unanswered are answered by the next `serve`.
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
    fn terminate(self: Box<Self>) {}
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
const CHINESE: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;
const RECORD: &str = "(;SZ[19];B[pd];W[dd];B[pp];W[dp])";
const OTHER: &str = "(;SZ[19];B[qd];W[dc])";
const ANALYSIS: &str = "/opt/katago analysis -config /a.cfg";
const PROBE_ARGS: &str = "10 rootInfo true focus pass 0";
/// D4 as a board point.
const D4: Point = Point { x: 3, y: 15 };

fn workbench(dir: &Path, world: &World) -> Workbench {
    Workbench::new(
        Storage::in_dirs(&dir.join("config"), &dir.join("data")),
        None,
        Box::new(FakeLauncher(world.clone())),
        Vec::new(),
    )
}

/// 精析设置 with an analysis process (`reuse` false) or the borrowed foreground engine; automatic
/// quick analysis on or off.
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

fn open(wb: &mut Workbench, path: &str, text: &str) -> bool {
    let t = wb.begin_open(OpenOrigin::File {
        path: path.into(),
        source: OpenSource::Drop,
    });
    wb.complete_open(t, Ok((RecordFormat::Sgf, sgf::read(text).unwrap())))
}

/// Answers every outstanding GTP command of `id`; a `kata-analyze` gets its header and one line of
/// `visits` (0: the stream stays silent); `name` is answered only when `name` is set (else it is
/// held for [`World::release_held`]). Returns the commands (without numbers).
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
                        let root = visits + 5;
                        out(wb, id, &format!(
                            "info move K10 visits {visits} winrate 0.5 scoreLead 0.5 prior 0.2 order 0 pv K10 rootInfo visits {root} winrate 0.4 scoreLead -0.5"
                        ));
                    }
                }
                _ => reply(wb, ""),
            }
            done.push(cmd);
        }
    }
}

/// A KataGo foreground engine, ready.
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

/// Whether any search at all (analysis or a generated move with analysis) was sent.
fn any_search(sent: &[String]) -> bool {
    sent.iter()
        .any(|c| c.starts_with("kata-analyze") || c.contains("genmove"))
}

/// Side-to-move win rate after `moves` moves of [`RECORD`].
fn winrate(moves: usize) -> f64 {
    [0.5, 0.5, 0.8, 0.2, 0.8][moves.min(4)]
}

/// Answers one query of the quick analysis process.
fn quick_answer(wb: &mut Workbench, worker: InstanceId, query: &str) {
    let v: serde_json::Value = serde_json::from_str(query).unwrap();
    let moves = v["moves"].as_array().unwrap().len();
    let w = winrate(moves);
    let reply = serde_json::json!({
        "id": v["id"], "turnNumber": moves,
        "moveInfos": [{"move": "K10", "visits": 1, "winrate": w, "scoreLead": 1.0, "prior": 0.1, "order": 0, "pv": ["K10"]}],
        "rootInfo": {"visits": 2, "winrate": w, "scoreLead": 1.0},
    });
    out(wb, worker, &reply.to_string());
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

// ---------------------------------------------------------------------------- focus harness

/// How an engine answers the start-up `focus` probe.
#[derive(Clone, Copy)]
enum Probe {
    Accept,
    /// Official KataGo before `focus`: its argument error for exactly the probe's arguments.
    Reject,
}

/// Answers every outstanding command of `id`; the probe as `probe` says, an ordinary
/// `kata-analyze` with its header only. Returns the commands.
fn serve_focus(wb: &mut Workbench, world: &World, id: InstanceId, probe: Probe) -> Vec<String> {
    let mut done = Vec::new();
    loop {
        let pending = world.take(id);
        if pending.is_empty() {
            return done;
        }
        for line in pending {
            out(wb, id, "");
            let (n, cmd) = line.split_once(' ').unwrap();
            let reply = |wb: &mut Workbench, text: &str| {
                out(wb, id, &format!("={n} {text}"));
                out(wb, id, "");
            };
            match cmd.split(' ').next().unwrap() {
                "name" => reply(wb, "KataGo"),
                "version" => reply(wb, "1.18.2"),
                "list_commands" => reply(wb, KATA),
                "kata-get-rules" => reply(wb, "{}"),
                "kata-analyze" if cmd.ends_with(PROBE_ARGS) => match probe {
                    Probe::Accept => {
                        out(wb, id, &format!("={n}"));
                        out(wb, id, "info move D4 visits 3 winrate 0.5 scoreLead 0 prior 0.1 order 0 pv D4 rootInfo visits 3 winrate 0.5 scoreLead 0");
                    }
                    Probe::Reject => {
                        out(
                            wb,
                            id,
                            &format!("?{n} Could not parse analyze arguments or arguments out of range: '{PROBE_ARGS}'"),
                        );
                        out(wb, id, "");
                    }
                },
                "kata-analyze" => out(wb, id, &format!("={n}")),
                _ => reply(wb, ""),
            }
            done.push(cmd.to_string());
        }
    }
}

/// The ordinary analysis requests among `sent` (the probe names no colour).
fn analyses(sent: &[String]) -> Vec<&String> {
    sent.iter()
        .filter(|c| c.starts_with("kata-analyze B") || c.starts_with("kata-analyze W"))
        .collect()
}

/// One analysis line with these candidates (point name, visits), best first.
fn info(candidates: &[(&str, u64)]) -> String {
    let mut line = String::new();
    for (order, (name, visits)) in candidates.iter().enumerate() {
        let winrate = 0.5 - order as f64 * 0.01;
        line.push_str(&format!(
            "info move {name} visits {visits} winrate {winrate} scoreLead 0.5 prior 0.1 order {order} pv {name} "
        ));
    }
    let total: u64 = candidates.iter().map(|c| c.1).sum();
    line.push_str(&format!("rootInfo visits {total} winrate 0.5 scoreLead 0.5"));
    line
}

// ---------------------------------------------------------------------------- batch harness

const GAME_A: &str = "(;SZ[19];B[pd];W[dd];B[pp];W[dp])";
const GAME_B: &str = "(;SZ[19];B[dd];W[pp])";

/// A workbench with a ready foreground engine (analysis process for 批量精析, no automatic quick
/// analysis) and an open record of its own (`/open.sgf`).
fn batch_setup(dir: &Path) -> (Workbench, World, InstanceId) {
    let world = World::default();
    let mut wb = workbench(dir, &world);
    configure(&mut wb, false, false);
    let fg = with_engine(&mut wb, &world);
    assert!(open(&mut wb, "/open.sgf", "(;SZ[19];B[pd];W[dd])"));
    serve(&mut wb, &world, fg, 0, true);
    (wb, world, fg)
}

fn write(dir: &Path, name: &str, text: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path.to_string_lossy().into_owned()
}

/// Runs the batch's file reads as the adapter does (off the lock, real files).
fn deliver_reads(wb: &mut Workbench) -> usize {
    let reads = wb.take_batch_reads();
    let n = reads.len();
    for (ticket, path, no_capture) in reads {
        assert!(wb.complete_batch_read(ticket, formats::read_file(&path, no_capture)));
    }
    n
}

/// Writes the captured background saves.
fn write_saves(wb: &mut Workbench) -> Vec<PathBuf> {
    let mut written = Vec::new();
    for job in wb.take_background_saves() {
        formats::write_record_file(&job.path, &job.text).unwrap();
        wb.complete_save(job.ticket, Ok(()));
        written.push(job.path);
    }
    written
}

fn ordinary() -> AutoAnalysisSettings {
    AutoAnalysisSettings {
        visits_enabled: true,
        visits: 10,
        time_enabled: false,
        auto_save: false,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------- 1. save × edit × quick

/// R6 + R5: the receipt of a save of revision N confirms only N, so a real move played while it is
/// written stays unsaved; quick analysis results stored meanwhile stay on their own nodes and never
/// move browsing off the played move.
#[test]
fn an_edit_during_a_save_stays_unsaved_while_quick_analysis_writes() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, false, true);
    assert!(open(&mut wb, "/x.sgf", RECORD));
    let worker = *world.workers().last().expect("a quick analysis process");
    let quick = world.take(worker);
    assert_eq!(quick.len(), 4, "one query per main-trunk move");
    assert!(wb.view(false).quick.running);
    assert_eq!(wb.cursor(), ROOT);

    let saved = wb.view(false).document.revision;
    let path = dir.path().join("game.sgf");
    let job = wb.begin_save(path.to_str().unwrap(), "LizzieYzy Tauri:test");
    // Part of the results arrives while the write is out ...
    for q in &quick[..2] {
        quick_answer(&mut wb, worker, q);
    }
    // ... then the user plays a real move (a new branch at the root) ...
    wb.edit(EditCommand::Play {
        point: Point { x: 2, y: 2 },
        color: None,
    })
    .unwrap();
    let played = wb.cursor();
    assert_ne!(played, ROOT);
    assert!(wb.view(false).document.revision > saved);
    // ... and the rest before the receipt.
    for q in &quick[2..] {
        quick_answer(&mut wb, worker, q);
    }
    assert!(!wb.view(false).quick.running);
    assert_eq!(wb.cursor(), played, "results never move browsing");

    wb.complete_save(job.ticket, Ok(()));
    let v = wb.view(false);
    assert!(
        matches!(v.save_outcome, Some(SaveOutcome::Saved { revision, .. }) if revision == saved),
        "{:?}",
        v.save_outcome
    );
    assert_eq!(v.document.saved_revision, Some(saved));
    assert!(v.document.revision > saved);
    assert!(v.document.unsaved, "the move played during the save is still unsaved");
    assert_eq!(v.position.node, played, "browsing stays on the played move");
    assert!(v.analysis.is_none(), "the played move has no quick result");
    assert_eq!(
        v.problems.analyzed, 3,
        "the results written during the save stay on the main trunk"
    );
    let again = wb.begin_save(path.to_str().unwrap(), "LizzieYzy Tauri:test");
    assert_eq!(
        again.text.matches("LZ[").count(),
        4,
        "every main-trunk move carries its result: {}",
        again.text
    );
}

// ---------------------------------------------------------------------------- 2. focus × engine switch

/// Focus rule + R7: tracking belongs to one position and engine instance. An edit and browsing end
/// it; after a paused switch to B, only B's own probe decides B's capability, the probe does not
/// resume the pause, A's tracked point does not survive onto B and A's late lines publish nothing.
#[test]
fn focus_after_edit_browse_pause_and_engine_switch_belongs_to_the_new_engine() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.set_auto_quick_analyze(false).unwrap();
    let profile = |name: &str| EngineProfile {
        name: name.into(),
        command: format!("/opt/{name} gtp"),
        ..EngineProfile::default()
    };
    wb.save_engine_settings(vec![profile("a"), profile("b")], EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let a = world.engine("/opt/a");
    serve_focus(&mut wb, &world, a, Probe::Accept);
    assert_eq!(wb.view(false).focus.capability, FocusCapability::Supported);
    assert!(open(&mut wb, "/x.sgf", RECORD));
    serve_focus(&mut wb, &world, a, Probe::Accept);

    // Tracked at the root, then a real move: the point belonged to the root.
    assert!(wb.tracking(TrackingCommand::Track { point: D4 }).unwrap());
    let sent = serve_focus(&mut wb, &world, a, Probe::Accept);
    assert_eq!(
        analyses(&sent).last().map(|c| c.as_str()),
        Some("kata-analyze B 10 rootInfo true focus D4 0.5"),
        "{sent:?}"
    );
    wb.edit(EditCommand::Play {
        point: Point { x: 2, y: 2 },
        color: None,
    })
    .unwrap();
    assert!(wb.view(false).focus.points.is_empty(), "the edit ends the root's tracking");
    let sent = serve_focus(&mut wb, &world, a, Probe::Accept);
    assert!(analyses(&sent).iter().all(|c| !c.contains("focus")), "{sent:?}");
    wb.navigate(Navigation::Back(1));
    assert_eq!(wb.cursor(), ROOT);
    assert!(
        wb.view(false).focus.points.is_empty(),
        "browsing back does not bring it back"
    );
    serve_focus(&mut wb, &world, a, Probe::Accept);

    // Tracked again, then paused: the point stays with A, inactive.
    assert!(wb.tracking(TrackingCommand::Track { point: D4 }).unwrap());
    serve_focus(&mut wb, &world, a, Probe::Accept);
    wb.toggle_analysis().unwrap();
    serve_focus(&mut wb, &world, a, Probe::Accept);
    let v = wb.view(false);
    assert!(v.engine.paused);
    assert_eq!(v.focus.points.len(), 1);

    // Switched to B while paused; A's focused stream still produces lines.
    assert!(wb.switch_engine(1).unwrap());
    let b = world.engine("/opt/b");
    assert_ne!(a, b);
    out(&mut wb, a, &info(&[("D4", 400), ("Q16", 50)]));
    out(&mut wb, a, "info move D4 visits 3 winrate 0.5 scoreLead 0 prior 0.1 order 0 pv D4 rootInfo visits 3 winrate 0.5 scoreLead 0");
    serve_focus(&mut wb, &world, a, Probe::Accept);
    let sent = serve_focus(&mut wb, &world, b, Probe::Reject);
    assert!(sent.iter().any(|c| c.ends_with(PROBE_ARGS)), "B is probed: {sent:?}");
    assert!(analyses(&sent).is_empty(), "the probe does not resume the pause: {sent:?}");
    let v = wb.view(false);
    assert!(v.engine.paused);
    assert_eq!(v.engine.foreground.as_ref().unwrap().name, "b");
    assert_eq!(
        v.focus.capability,
        FocusCapability::Unsupported,
        "B's own probe decides, not A's"
    );
    assert!(!v.focus.available);
    assert!(v.focus.points.is_empty(), "A's tracked point does not survive onto B");
    assert!(
        v.analysis.is_none() && v.history.is_empty(),
        "A's late lines publish nothing"
    );
    assert!(!wb.tracking(TrackingCommand::Track { point: D4 }).unwrap());

    // Still later lines of A; B stays paused until the user resumes.
    out(&mut wb, a, &info(&[("D4", 800), ("Q16", 90)]));
    assert!(wb.view(false).analysis.is_none());
    assert!(analyses(&serve_focus(&mut wb, &world, b, Probe::Reject)).is_empty());
    wb.toggle_analysis().unwrap();
    let sent = serve_focus(&mut wb, &world, b, Probe::Reject);
    assert_eq!(analyses(&sent), ["kata-analyze B 10 rootInfo true"], "{sent:?}");
}

// ---------------------------------------------------------------------------- 3. quick → flash hand-off

/// R7 + R8 + T14 (借用中的快析先归还; 暂停在租约中保持到归还后): a pause while the run waits for the
/// quick lease's hand-back keeps the run, which then borrows, completes after its own confirmed
/// hand-back, and neither hand-back resumes the foreground.
#[test]
fn a_pause_during_the_quick_to_flash_hand_off_resumes_neither() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, true);
    let fg = with_engine(&mut wb, &world);
    assert!(open(&mut wb, "/x.sgf", RECORD));
    serve(&mut wb, &world, fg, 0, false);
    assert!(wb.view(false).quick.borrowed);

    assert_eq!(wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(), Ok(true));
    assert!(
        wb.view(false).flash.as_ref().is_some_and(|f| f.loading),
        "waits for the quick lease's hand-back"
    );
    wb.toggle_analysis().unwrap();
    let v = wb.view(false);
    assert!(v.engine.paused);
    assert!(v.flash.as_ref().is_some_and(|f| f.loading), "the run still waits");

    // The quick lease is given back; its `name` is held.
    let mut sent = serve(&mut wb, &world, fg, 0, false);
    let v = wb.view(false);
    assert!(v.engine.paused);
    assert!(v.flash.as_ref().is_some_and(|f| f.loading), "{:?}", v.flash);
    // Confirmed: the run borrows, searches and gives the engine back.
    world.release_held(fg);
    sent.extend(serve(&mut wb, &world, fg, 20, true));
    assert!(sent.iter().any(|c| c.contains("maxmoves 1")), "the run searched: {sent:?}");
    assert!(
        !foreground_analyses(&sent),
        "neither hand-back resumes the foreground: {sent:?}"
    );
    let v = wb.view(false);
    assert!(
        matches!(v.flash_outcome, Some(FlashOutcome::Completed { .. })),
        "{:?}",
        v.flash_outcome
    );
    assert!(v.engine.paused);
    assert_eq!(v.engine.unrestored, None);
    assert!(!v.quick.running);
    assert_eq!(v.problems.analyzed, 3);
    assert!(!foreground_analyses(&serve(&mut wb, &world, fg, 0, true)));
    // Only the user's resume lets it analyse again.
    wb.toggle_analysis().unwrap();
    assert!(foreground_analyses(&serve(&mut wb, &world, fg, 0, true)));
}

// ---------------------------------------------------------------------------- 4. open × batch

/// R5 + T17 (后台批量文档): a record opened while 批量分析 analyses its background document is adopted
/// at once; the batch goes on with its own records and nothing of it reaches the new document.
#[test]
fn opening_a_record_during_a_background_batch_keeps_the_batch_and_its_results_apart() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, fg) = batch_setup(dir.path());
    let games = dir.path().join("games");
    std::fs::create_dir(&games).unwrap();
    let a = write(&games, "a.sgf", GAME_A);
    let b = write(&games, "b.sgf", GAME_B);
    assert_eq!(
        wb.open_batch(BatchMode::Ordinary, vec![a, b], OpenSource::Chooser),
        Ok(true)
    );
    assert_eq!(deliver_reads(&mut wb), 1);
    assert_eq!(wb.start_batch_ordinary(ordinary()).unwrap(), Ok(true));
    serve(&mut wb, &world, fg, 3, true); // below 每手总计算量: a is still at its first position
    assert!(wb.view(false).auto_analysis.active);
    let before = wb.document().id;

    assert!(open(&mut wb, "/other.sgf", OTHER));
    let v = wb.view(false);
    assert_ne!(v.document.id, before, "the open completes");
    assert!(v.pending_open.is_none());
    assert!(v.batch.running.is_some(), "the batch goes on");
    let opened = v.document.id;

    // The open sent the engine nothing: the batch's stream goes on and now reaches its target.
    assert!(serve(&mut wb, &world, fg, 0, true).is_empty(), "the open leaves the batch's engine alone");
    out(&mut wb, fg, "info move K10 visits 10 winrate 0.5 scoreLead 0.5 prior 0.2 order 0 pv K10 rootInfo visits 15 winrate 0.4 scoreLead -0.5");
    serve(&mut wb, &world, fg, 10, true);
    let v = wb.view(false);
    assert_eq!(v.batch.items[0].name, "b.sgf", "a finished on its own document");
    assert_eq!(v.position.node, ROOT, "the new record's browsing position never moves");
    assert!(
        v.analysis.is_none() && v.history.is_empty(),
        "batch results do not enter the new record"
    );
    let written = write_saves(&mut wb);
    assert_eq!(written.len(), 1);
    assert!(std::fs::read_to_string(&written[0]).unwrap().contains("LZ["));

    assert_eq!(deliver_reads(&mut wb), 1);
    serve(&mut wb, &world, fg, 10, true);
    write_saves(&mut wb);
    let v = wb.view(false);
    assert_eq!(
        v.batch.notice.as_ref().map(|n| n.message_key.as_str()),
        Some("Leelaz.batchAutoAnalyzeComplete")
    );
    assert_eq!(v.document.id, opened);
    assert_eq!(v.document.path.as_deref(), Some("/other.sgf"));
    assert_eq!((v.document.revision, v.document.unsaved), (0, false));
    assert!(v.analysis.is_none() && v.history.is_empty());
    for _ in 0..2 {
        wb.navigate(Navigation::Forward(1));
        assert!(wb.view(false).analysis.is_none(), "no batch result on the new record");
    }
}

// ---------------------------------------------------------------------------- 5. pause × batch

/// R8 + T15/T17 (批量分析＝逐谱自动分析; 空格按设置退出): with 手动暂停分析则退出自动分析 (the default)
/// the pause stops the foreground engine and ends the batch's automatic analysis without its
/// stop/save path; the end resumes nothing and the open record is untouched.
#[test]
fn a_foreground_pause_during_a_background_batch() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, fg) = batch_setup(dir.path());
    let a = write(dir.path(), "a.sgf", GAME_A);
    let b = write(dir.path(), "b.sgf", GAME_B);
    wb.open_batch(BatchMode::Ordinary, vec![a, b], OpenSource::Chooser)
        .unwrap();
    deliver_reads(&mut wb);
    wb.start_batch_ordinary(ordinary()).unwrap().unwrap();
    serve(&mut wb, &world, fg, 3, true);
    assert!(wb.view(false).auto_analysis.active);
    let doc = wb.document().id;

    wb.toggle_analysis().unwrap();
    let v = wb.view(false);
    assert!(v.engine.paused);
    assert!(!v.auto_analysis.active && v.auto_analysis.pause_tip);
    assert!(v.batch.running.is_none());
    let sent = serve(&mut wb, &world, fg, 10, true);
    assert!(!any_search(&sent), "the foreground engine stops: {sent:?}");
    assert!(wb.take_background_saves().is_empty(), "no stop/save path");
    assert_eq!(deliver_reads(&mut wb), 0, "no next record loads");
    let v = wb.view(false);
    assert!(v.engine.paused);
    assert_eq!(v.document.id, doc);
    assert_eq!(v.position.node, ROOT);
    assert!(v.analysis.is_none() && v.history.is_empty());
}

/// R7 + R8 + T17 (批量精析 with its own analysis process): the first Space only gives the open record
/// back to the foreground engine; the user's pause then stops it while the batch's process goes on
/// to the end, and the batch's end does not resume the paused foreground.
#[test]
fn a_foreground_pause_during_a_background_flash_batch_lets_its_process_finish() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, fg) = batch_setup(dir.path());
    let a = write(dir.path(), "a.sgf", GAME_A);
    let b = write(dir.path(), "b.sgf", GAME_B);
    wb.open_batch(BatchMode::Flash, vec![a, b], OpenSource::Chooser).unwrap();
    deliver_reads(&mut wb);
    assert_eq!(wb.start_batch_flash(-1, -1, 2, false).unwrap(), Ok(true));
    deliver_reads(&mut wb);
    let worker = *world.workers().last().expect("the batch's analysis process");
    let qs = queries(&world, worker);
    assert_eq!(qs.len(), 4);
    assert!(!foreground_analyses(&serve(&mut wb, &world, fg, 0, true)));

    // The first Space resumes the temporarily stopped engine; the second is the user's pause.
    wb.toggle_analysis().unwrap();
    assert!(!wb.view(false).engine.paused);
    assert!(foreground_analyses(&serve(&mut wb, &world, fg, 0, true)));
    wb.toggle_analysis().unwrap();
    assert!(wb.view(false).engine.paused);
    let mut sent = serve(&mut wb, &world, fg, 0, true);

    qs.iter().for_each(|q| answer(&mut wb, worker, q));
    assert_eq!(write_saves(&mut wb).len(), 1, "a is saved");
    assert_eq!(deliver_reads(&mut wb), 1);
    let qs = queries(&world, worker);
    assert_eq!(qs.len(), 2, "the same process continues with b");
    qs.iter().for_each(|q| answer(&mut wb, worker, q));
    assert_eq!(write_saves(&mut wb).len(), 1, "b is saved");
    sent.extend(serve(&mut wb, &world, fg, 0, true));

    let v = wb.view(false);
    assert_eq!(
        v.batch.notice.as_ref().map(|n| n.message_key.as_str()),
        Some("Leelaz.batchAutoAnalyzeComplete")
    );
    assert!(v.batch.items.is_empty() && v.batch.running.is_none());
    assert!(v.engine.paused, "the batch's end does not resume the foreground");
    assert!(!any_search(&sent), "{sent:?}");
    assert!(v.analysis.is_none() && v.history.is_empty(), "the open record gets nothing");
}

// ---------------------------------------------------------------------------- 6. cancel × pause

/// R7 + R8 + T14 (取消不恢复): a borrowed flash run cancelled while paused gives the engine back, and
/// after the confirmed hand-back nothing is searched and the pause holds.
#[test]
fn cancelling_a_run_while_paused_never_resumes_the_foreground() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let fg = with_engine(&mut wb, &world);
    assert!(open(&mut wb, "/x.sgf", RECORD));
    serve(&mut wb, &world, fg, 0, true);
    assert_eq!(wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(), Ok(true));
    // Taken over; the first search stays silent.
    let taken = serve(&mut wb, &world, fg, 0, true);
    assert!(taken.iter().any(|c| c.contains("maxmoves 1")), "{taken:?}");

    wb.toggle_analysis().unwrap();
    let v = wb.view(false);
    assert!(v.engine.paused && v.flash.is_some());
    assert!(wb.cancel_flash_analysis());
    let v = wb.view(false);
    assert!(v.flash.is_none() && v.flash_outcome.is_none());
    assert!(v.engine.paused);
    // The hand-back, its `name` held.
    let returning = serve(&mut wb, &world, fg, 0, false);
    assert!(!any_search(&returning), "{returning:?}");
    assert!(wb.view(false).engine.paused);
    world.release_held(fg);
    let returned = serve(&mut wb, &world, fg, 0, true);
    assert!(returned.iter().any(|c| c == "name"), "confirmed: {returned:?}");
    assert!(!any_search(&returned), "{returned:?}");
    let v = wb.view(false);
    assert!(v.engine.paused);
    assert_eq!(v.engine.unrestored, None);
    assert!(serve(&mut wb, &world, fg, 0, true).is_empty());
}

/// R7 + R8 + T15 (停止保留结果; only the user's pause prevents resumption): a borrowed whole-game run
/// stopped while paused gives the engine back, and after the confirmed hand-back nothing is
/// searched and the pause holds.
#[test]
fn stopping_a_borrowed_whole_game_run_while_paused_never_resumes_the_foreground() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let fg = with_engine(&mut wb, &world);
    assert!(open(&mut wb, "/x.sgf", RECORD));
    serve(&mut wb, &world, fg, 0, true);
    assert_eq!(wb.start_whole_game_analysis(1000).unwrap(), Ok(true));
    let taken = serve(&mut wb, &world, fg, 0, true);
    assert!(taken.iter().any(|c| c.contains("maxmoves")), "{taken:?}");

    wb.toggle_analysis().unwrap();
    assert!(wb.view(false).engine.paused);
    assert!(wb.stop_whole_game_analysis());
    let v = wb.view(false);
    assert_eq!(v.whole_game.state, WholeGameState::Cancelled);
    assert!(v.engine.paused);
    let returning = serve(&mut wb, &world, fg, 0, false);
    assert!(!any_search(&returning), "{returning:?}");
    assert!(wb.view(false).engine.paused);
    world.release_held(fg);
    let returned = serve(&mut wb, &world, fg, 0, true);
    assert!(returned.iter().any(|c| c == "name"), "confirmed: {returned:?}");
    assert!(!any_search(&returned), "{returned:?}");
    let v = wb.view(false);
    assert!(v.engine.paused);
    assert_eq!(v.engine.unrestored, None);
    assert!(serve(&mut wb, &world, fg, 0, true).is_empty());
}

// ---------------------------------------------------------------------------- 7. open × whole-game return

/// R7 + R8 + the open-waits-for-the-hand-back rule (quick_analysis.rs / flash_analysis.rs): a record
/// opened while a stopped whole-game borrow is being given back is adopted only after the
/// confirmed hand-back; until then the user's pause holds, and late whole-game answers never reach
/// the new document. The adopted record then starts a new analysis context (engine.rs
/// `a_new_record_ends_the_user_pause`).
#[test]
fn opening_a_record_while_a_whole_game_borrow_is_being_returned() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let fg = with_engine(&mut wb, &world);
    assert!(open(&mut wb, "/x.sgf", RECORD));
    serve(&mut wb, &world, fg, 0, true);
    assert_eq!(wb.start_whole_game_analysis(1000).unwrap(), Ok(true));
    let taken = serve(&mut wb, &world, fg, 0, true);
    assert!(taken.iter().any(|c| c.contains("maxmoves")), "{taken:?}");
    wb.toggle_analysis().unwrap();
    assert!(wb.stop_whole_game_analysis());
    let before = wb.view(false).document.id;
    // A late answer of the outstanding borrowed search.
    let late = "info move K10 visits 1000 winrate 0.5 scoreLead 0.5 prior 0.2 order 0 pv K10 rootInfo visits 1000 winrate 0.4 scoreLead -0.5";
    out(&mut wb, fg, late);

    assert!(open(&mut wb, "/other.sgf", OTHER));
    let v = wb.view(false);
    assert_eq!(v.document.id, before, "not adopted while the engine is being given back");
    assert!(v.pending_open.is_some());
    assert!(v.engine.paused);
    let returning = serve(&mut wb, &world, fg, 0, false);
    out(&mut wb, fg, late);
    assert!(!foreground_analyses(&returning), "the pause holds: {returning:?}");
    let v = wb.view(false);
    assert_eq!(v.document.id, before, "still not adopted before `name` is confirmed");
    assert!(v.engine.paused);

    world.release_held(fg);
    let after = serve(&mut wb, &world, fg, 0, true);
    let name = after
        .iter()
        .position(|c| c == "name")
        .unwrap_or_else(|| panic!("confirmed: {after:?}"));
    assert!(
        !foreground_analyses(&after[..name]),
        "nothing analyses before the confirmation: {after:?}"
    );
    let v = wb.view(false);
    assert_ne!(v.document.id, before, "adopted after the confirmed hand-back");
    assert!(v.pending_open.is_none());
    assert_eq!(v.document.path.as_deref(), Some("/other.sgf"));
    assert_eq!(v.engine.unrestored, None);
    assert!(!v.whole_game.show_results);
    assert!(
        v.analysis.is_none() && v.history.is_empty(),
        "late whole-game answers do not reach the new document"
    );
    for _ in 0..2 {
        wb.navigate(Navigation::Forward(1));
        assert!(wb.view(false).analysis.is_none());
    }
    // A new load context: the user pause ends with the adopted record, as for any open.
    assert!(!v.engine.paused);
    assert!(foreground_analyses(&after[name..]), "{after:?}");
}

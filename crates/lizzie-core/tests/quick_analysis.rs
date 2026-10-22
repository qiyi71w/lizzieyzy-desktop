//! 自动棋谱快析 through the public `Workbench` operations with controlled processes: a dedicated
//! KataGo analysis process and the borrowed foreground engine; the user's pause in each phase,
//! record switches with late answers, failures and retries, and the 问题手 list built from the
//! results.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lizzie_core::api::{OpenSource, QuickModelStatus, ResourceCommand, ResourceItem, UnrestoredReason};
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats::sgf;
use lizzie_core::record::{RecordFormat, Stone};
use lizzie_core::resources as r;
use lizzie_core::settings::{AnalysisEngineSettings, Storage};
use lizzie_core::workbench::{run_resource_job, OpenOrigin, Workbench};
use sha2::{Digest, Sha256};

#[derive(Default)]
struct Proc {
    program: String,
    args: Vec<String>,
    sent: Vec<String>,
    terminated: bool,
    /// What `kata-set-rules` last set; `kata-get-rules` reports it.
    rules: Option<String>,
    /// Commands left unanswered so far.
    held: Vec<String>,
}

/// How the controlled engines answer: the version they report and commands refused once.
#[derive(Default)]
struct Script {
    version: Option<String>,
    refused: Vec<String>,
}

#[derive(Clone, Default)]
struct World(Arc<Mutex<Vec<(InstanceId, Proc)>>>, Arc<Mutex<Script>>);

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
    fn args(&self, id: InstanceId) -> Vec<String> {
        let w = self.0.lock().unwrap();
        w.iter().find(|p| p.0 == id).map(|p| p.1.args.clone()).unwrap()
    }
    fn terminated(&self, id: InstanceId) -> bool {
        self.0.lock().unwrap().iter().any(|p| p.0 == id && p.1.terminated)
    }
    fn rules(&self, id: InstanceId) -> Option<String> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.0 == id)
            .and_then(|p| p.1.rules.clone())
    }
    fn set_rules(&self, id: InstanceId, rules: &str) {
        let mut w = self.0.lock().unwrap();
        w.iter_mut().find(|p| p.0 == id).unwrap().1.rules = Some(rules.to_string());
    }
    fn version(&self) -> String {
        self.1
            .lock()
            .unwrap()
            .version
            .clone()
            .unwrap_or_else(|| "1.18.2".into())
    }
    fn set_version(&self, version: &str) {
        self.1.lock().unwrap().version = Some(version.into());
    }
    /// The next `command` is refused.
    fn refuse(&self, command: &str) {
        self.1.lock().unwrap().refused.push(command.into());
    }
    fn refused(&self, command: &str) -> bool {
        let mut s = self.1.lock().unwrap();
        let at = s.refused.iter().position(|c| c == command);
        at.map(|i| s.refused.remove(i)).is_some()
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

fn workbench(dir: &std::path::Path, world: &World) -> Workbench {
    Workbench::new(
        Storage::in_dirs(&dir.join("config"), &dir.join("data")),
        None,
        Box::new(FakeLauncher(world.clone())),
        Vec::new(),
    )
}

fn analysis_settings(reuse: bool) -> AnalysisEngineSettings {
    AnalysisEngineSettings {
        command: ANALYSIS.into(),
        reuse_current_engine: reuse,
        ..AnalysisEngineSettings::default()
    }
}

fn out(wb: &mut Workbench, id: InstanceId, line: &str) {
    wb.engine_input(id, EngineInput::Stdout(line.to_string()));
}

fn number(line: &str) -> u64 {
    line.split(' ').next().unwrap().parse().unwrap()
}

const CHINESE: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;
const JAPANESE: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"TERRITORY","suicide":false,"tax":"SEKI","whiteHandicapBonus":"0"}"#;

/// Answers every outstanding GTP command of `id`; a `kata-analyze` gets its header and one
/// line of `visits` (0: the stream stays silent), and `name` is answered only when `name` is
/// set. The engine reports the rules last set (Chinese at first) and refuses what the world's
/// script says. Returns the commands (without numbers).
fn serve(wb: &mut Workbench, world: &World, id: InstanceId, visits: u64, name: bool) -> Vec<String> {
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
            let reply = |wb: &mut Workbench, text: &str| {
                out(wb, id, &format!("={n} {text}"));
                out(wb, id, "");
            };
            let (word, arg) = cmd.split_once(' ').unwrap_or((&cmd, ""));
            match word {
                _ if world.refused(&cmd) => {
                    out(wb, id, &format!("?{n} refused"));
                    out(wb, id, "");
                }
                "name" if !name => world.hold(id, line.clone()),
                "name" => reply(wb, "KataGo"),
                "version" => reply(wb, &world.version()),
                "list_commands" => reply(wb, KATA),
                "kata-set-rules" => {
                    world.set_rules(id, arg);
                    reply(wb, "");
                }
                "kata-get-rules" => reply(wb, &world.rules(id).unwrap_or_else(|| CHINESE.into())),
                "kata-analyze" => {
                    out(wb, id, &format!("={n}"));
                    if visits > 0 {
                        // The root's own evaluation differs from its best move's, as in KataGo.
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

/// Side-to-move win rate after `moves` moves of [`RECORD`]: White's second move loses 30 points.
fn winrate(moves: usize) -> f64 {
    [0.5, 0.5, 0.8, 0.2, 0.8][moves.min(4)]
}

/// Answers the analysis process's queries; returns their ids.
fn answer_queries(wb: &mut Workbench, world: &World, worker: InstanceId) -> Vec<String> {
    let mut ids = Vec::new();
    for q in world.take(worker) {
        let v: serde_json::Value = serde_json::from_str(&q).unwrap();
        let id = v["id"].as_str().unwrap().to_string();
        let moves = v["moves"].as_array().unwrap().len();
        let w = winrate(moves);
        let reply = serde_json::json!({
            "id": id, "turnNumber": moves,
            "moveInfos": [{"move": "K10", "visits": 1, "winrate": w, "scoreLead": 1.0, "prior": 0.1, "order": 0, "pv": ["K10"]}],
            "rootInfo": {"visits": 2, "winrate": w, "scoreLead": 1.0},
        });
        out(wb, worker, &reply.to_string());
        ids.push(id);
    }
    ids
}

fn open(wb: &mut Workbench, text: &str) -> bool {
    let t = wb.begin_open(OpenOrigin::File {
        path: "/x.sgf".into(),
        source: OpenSource::Drop,
    });
    wb.complete_open(t, Ok((RecordFormat::Sgf, sgf::read(text).unwrap())))
}

/// A KataGo foreground engine, ready and analysing.
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

fn analysed_moves(wb: &mut Workbench) -> u32 {
    wb.view(false).problems.analyzed
}

#[test]
fn a_loaded_record_is_analysed_by_an_analysis_process_while_the_foreground_waits() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_analysis_engine_settings(analysis_settings(false), "", true)
        .unwrap()
        .unwrap();
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);

    let worker = *world.workers().last().expect("an analysis process");
    let args = world.args(worker).join(" ");
    assert!(
        args.contains("-override-config analysisPVLen=15,numSearchThreads=,numSearchThreadsPerAnalysisThread=1"),
        "{args}"
    );
    let v = wb.view(false);
    assert!(v.quick.running && !v.quick.borrowed);
    // The foreground engine stops and stays idle meanwhile.
    let fg_sent = serve(&mut wb, &world, fg, 0, true);
    let stopped = fg_sent.iter().rposition(|c| c == "stop").expect("stopped");
    assert!(
        !fg_sent[stopped..].iter().any(|c| c.starts_with("kata-analyze")),
        "{fg_sent:?}"
    );

    let queries: Vec<serde_json::Value> = world
        .take(worker)
        .iter()
        .map(|q| serde_json::from_str(q).unwrap())
        .collect();
    assert_eq!(queries.len(), 4, "one query per main-trunk move");
    assert_eq!(queries[0]["maxVisits"], 2);
    assert_eq!(queries[3]["moves"].as_array().unwrap().len(), 4);
    for q in &queries {
        let line = serde_json::json!({
            "id": q["id"], "moveInfos": [{"move": "K10", "visits": 2, "winrate": 0.5, "order": 0, "pv": ["K10"]}],
            "rootInfo": {"visits": 2},
        });
        out(&mut wb, worker, &line.to_string());
    }
    let v = wb.view(false);
    assert!(!v.quick.running);
    assert!(world.terminated(worker), "a quick analysis does not keep its process");
    assert!(!v.document.unsaved, "results do not change the record");
    // Results are stored on their nodes under 闪电分析; the foreground resumes.
    wb.navigate(Navigation::Last);
    let shown = wb.view(false).analysis.expect("stored result");
    assert_eq!(shown.engine, "闪电分析");
    let fg_sent = serve(&mut wb, &world, fg, 0, true);
    assert!(fg_sent.iter().any(|c| c.starts_with("kata-analyze")), "{fg_sent:?}");
}

#[test]
fn pausing_cancels_the_record_for_good_and_resuming_analyses_in_the_foreground() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_analysis_engine_settings(analysis_settings(false), "", true)
        .unwrap()
        .unwrap();
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    let worker = *world.workers().last().unwrap();
    // One answer arrives before the pause; it stays.
    let first = world.take(worker).remove(0);
    let q: serde_json::Value = serde_json::from_str(&first).unwrap();
    out(&mut wb, worker, &serde_json::json!({"id": q["id"], "moveInfos": [{"move": "K10", "visits": 2, "winrate": 0.5, "order": 0}], "rootInfo": {"visits": 2}}).to_string());

    wb.toggle_analysis().unwrap();
    let v = wb.view(false);
    assert!(v.engine.paused && !v.quick.running);
    assert!(world.terminated(worker));
    assert_eq!(
        analysed_moves(&mut wb),
        0,
        "move 1 is never a problem move, but its result is kept"
    );
    wb.navigate(Navigation::Forward(1));
    assert!(wb.view(false).analysis.is_some(), "the completed result is kept");

    wb.toggle_analysis().unwrap();
    let fg_sent = serve(&mut wb, &world, fg, 0, true);
    assert!(
        fg_sent.iter().any(|c| c.starts_with("kata-analyze")),
        "the foreground resumes"
    );
    // Browsing after the timers does not revive the cancelled quick analysis.
    wb.navigate(Navigation::Forward(1));
    wb.engine_tick(Instant::now() + Duration::from_secs(5));
    assert_eq!(world.workers().len(), 1);
    assert!(!wb.view(false).quick.running);
    // A newly opened record starts afresh.
    open(&mut wb, RECORD);
    assert_eq!(world.workers().len(), 2);
}

#[test]
fn a_pause_while_waiting_for_the_engine_cancels_before_anything_starts() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_analysis_engine_settings(analysis_settings(false), "", true)
        .unwrap()
        .unwrap();
    let profile = EngineProfile {
        name: "KataGo".into(),
        command: "/opt/katago gtp".into(),
        ..EngineProfile::default()
    };
    wb.save_engine_settings(vec![profile], EngineStartup::Default).unwrap();
    wb.start_engines();
    open(&mut wb, RECORD);
    let v = wb.view(false);
    assert!(
        v.quick.running && v.quick.retrying,
        "waits while the foreground engine starts"
    );
    assert!(world.workers().is_empty());
    wb.toggle_analysis().unwrap();
    let fg = world.engine("/opt/katago");
    serve(&mut wb, &world, fg, 0, true);
    wb.engine_tick(Instant::now() + Duration::from_secs(5));
    assert!(world.workers().is_empty(), "the cancelled task never starts");
    assert!(!wb.view(false).quick.running);
}

#[test]
fn a_new_record_drops_the_old_process_and_its_late_answers() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_analysis_engine_settings(analysis_settings(false), "", true)
        .unwrap()
        .unwrap();
    open(&mut wb, RECORD);
    let old = *world.workers().last().unwrap();
    let old_queries = world.take(old);
    open(&mut wb, "(;SZ[19];B[qd];W[cc])");
    assert!(world.terminated(old));
    let new = *world.workers().last().unwrap();
    assert_ne!(old, new);
    // A late answer of the old process reaches nothing.
    let q: serde_json::Value = serde_json::from_str(&old_queries[1]).unwrap();
    out(&mut wb, old, &serde_json::json!({"id": q["id"], "moveInfos": [{"move": "K10", "visits": 2, "winrate": 0.5, "order": 0}], "rootInfo": {"visits": 2}}).to_string());
    wb.navigate(Navigation::Forward(2));
    assert!(wb.view(false).analysis.is_none());
    answer_queries(&mut wb, &world, new);
    assert!(wb.view(false).analysis.is_some());
    assert!(!wb.view(false).quick.running);
}

#[test]
fn a_failed_attempt_lets_the_foreground_analyse_and_retries_later() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_analysis_engine_settings(analysis_settings(false), "", true)
        .unwrap()
        .unwrap();
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    let worker = *world.workers().last().unwrap();
    wb.engine_input(worker, EngineInput::Exited(Some(1)));
    let v = wb.view(false);
    assert!(v.quick.running && v.quick.retrying);
    let fg_sent = serve(&mut wb, &world, fg, 0, true);
    assert!(
        fg_sent.iter().any(|c| c.starts_with("kata-analyze")),
        "the failure is the foreground's opportunity"
    );
    // 1800 ms << 1 later the task tries again.
    let now = Instant::now();
    wb.engine_tick(now + Duration::from_millis(2000));
    assert_eq!(world.workers().len(), 1, "not before the delay");
    wb.engine_tick(now + Duration::from_millis(3700));
    assert_eq!(world.workers().len(), 2);
    // The new attempt has the position again: the foreground engine stops.
    let fg_sent = serve(&mut wb, &world, fg, 0, true);
    let stopped = fg_sent
        .iter()
        .rposition(|c| c == "stop")
        .expect("stopped for the retry");
    assert!(
        !fg_sent[stopped..].iter().any(|c| c.starts_with("kata-analyze")),
        "{fg_sent:?}"
    );
    answer_queries(&mut wb, &world, *world.workers().last().unwrap());
    assert!(!wb.view(false).quick.running);
}

#[test]
fn problem_moves_come_from_the_results_and_a_jump_marks_the_move() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_analysis_engine_settings(analysis_settings(false), "", true)
        .unwrap()
        .unwrap();
    open(&mut wb, RECORD);
    let p = wb.view(false).problems;
    assert_eq!((p.analyzed, p.total, p.running), (0, 3, true));
    answer_queries(&mut wb, &world, *world.workers().last().unwrap());
    let p = wb.view(false).problems;
    assert_eq!((p.analyzed, p.total, p.running), (3, 3, false));
    assert!(p.black.is_empty());
    assert_eq!(p.white.len(), 1);
    let w = &p.white[0];
    assert_eq!(
        (w.move_number, w.mover, w.tier, w.playouts),
        (2, Stone::White, 5, 2),
        "playouts are the root's visits"
    );
    assert!((w.winrate_loss - 30.0).abs() < 1e-9);
    assert!(wb.jump_to_problem_move(w.node).unwrap());
    let v = wb.view(false);
    assert_eq!(v.position.move_number, 1, "the position before the problem move");
    assert_eq!(v.problems.mark, Some(w.point));
    assert!(v.problems.white[0].current);
}

/// The borrowed foreground: take-over, a finite search per move, and the hand-back.
#[test]
fn the_borrowed_foreground_analyses_each_move_and_is_given_back() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_analysis_engine_settings(analysis_settings(true), "", false)
        .unwrap()
        .unwrap();
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    assert!(world.workers().is_empty(), "no analysis process");
    let sent = serve(&mut wb, &world, fg, 20, true);
    let searches: Vec<&String> = sent.iter().filter(|c| c.contains("maxmoves 1")).collect();
    assert_eq!(
        searches,
        [
            "kata-analyze W 1 maxmoves 1 rootInfo true",
            "kata-analyze B 1 maxmoves 1 rootInfo true",
            "kata-analyze W 1 maxmoves 1 rootInfo true",
            "kata-analyze B 1 maxmoves 1 rootInfo true",
        ],
        "one finite search per move, for the side to move after it"
    );
    let rules = sent.iter().position(|c| c == "kata-get-rules").unwrap();
    assert_eq!(
        sent[rules - 1],
        "stop",
        "taken over with `stop`, then its rules are captured"
    );
    // Given back: `stop`, the browsing position again and `name`.
    let name = sent.iter().rposition(|c| c == "name").unwrap();
    assert!(
        sent[name..].iter().any(|c| c.starts_with("kata-analyze B 10")),
        "{sent:?}"
    );
    let v = wb.view(false);
    assert!(!v.quick.running, "{:?}", v.quick);
    assert_eq!(v.engine.unrestored, None);
    // Given back: the foreground engine analyses the browsing position again.
    assert!(sent.last().is_some_and(|c| c.starts_with("kata-analyze")), "{sent:?}");
    assert_eq!(v.problems.analyzed, 3);
    // A borrowed search stores the root's visits and its own evaluation.
    wb.navigate(Navigation::Forward(2));
    let a = wb.view(false).analysis.unwrap();
    assert_eq!(
        (a.engine.as_str(), a.visits, a.winrate, a.score_lead),
        ("闪电分析", 25, 40.0, Some(-0.5))
    );
}

#[test]
fn a_pause_during_the_lease_waits_for_the_hand_back_and_a_failed_one_keeps_the_pause() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_analysis_engine_settings(analysis_settings(true), "", false)
        .unwrap()
        .unwrap();
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    // Take-over and the first search stream, then the pause.
    serve(&mut wb, &world, fg, 0, false);
    wb.toggle_analysis().unwrap();
    // Resume while the engine is being given back: analysis waits for the hand-back.
    wb.toggle_analysis().unwrap();
    let v = wb.view(false);
    assert!(!v.engine.paused && v.quick.running && v.quick.borrowed);
    // The hand-back's `name` is never answered: after the restore timeout the engine stays
    // unrestored and the pause holds.
    serve(&mut wb, &world, fg, 0, false);
    wb.engine_tick(Instant::now() + Duration::from_secs(31));
    let v = wb.view(false);
    assert!(v.engine.paused && !v.quick.running);
    assert_eq!(v.engine.unrestored, Some(UnrestoredReason::Restore));
    let after = serve(&mut wb, &world, fg, 0, true);
    assert!(!after.iter().any(|c| c.starts_with("kata-analyze")), "{after:?}");
}

#[test]
fn opening_a_record_during_the_lease_waits_for_the_hand_back() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_analysis_engine_settings(analysis_settings(true), "", false)
        .unwrap()
        .unwrap();
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    serve(&mut wb, &world, fg, 0, false);
    let before = wb.view(false).document.id;
    assert!(open(&mut wb, "(;SZ[19];B[qd])"));
    assert_eq!(
        wb.view(false).document.id,
        before,
        "not adopted while the engine is borrowed"
    );
    assert!(wb.view(false).pending_open.is_some());
    // The hand-back completes: the record opens and a new task borrows again.
    serve(&mut wb, &world, fg, 0, true);
    let v = wb.view(false);
    assert_ne!(v.document.id, before);
    assert_eq!(v.engine.unrestored, None);
}

#[test]
fn the_analysis_settings_command_is_customized_only_when_changed_and_reopens() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let shown = wb.view(false).settings.analysis_engine.command;
    let empty = AnalysisEngineSettings {
        command: " ".into(),
        ..AnalysisEngineSettings::default()
    };
    assert_eq!(
        wb.save_analysis_engine_settings(empty, &shown, false).unwrap(),
        Err("AnalysisSettings.commandRequired")
    );
    // Unchanged command: not customized; a reused engine needs no command.
    let same = AnalysisEngineSettings {
        max_visits: 0,
        always_override: true,
        ..AnalysisEngineSettings::default()
    };
    assert_eq!(wb.save_analysis_engine_settings(same, &shown, false).unwrap(), Ok(true));
    let s = wb.view(false).settings.analysis_engine;
    assert!(!s.command_customized && s.always_override && s.max_visits == 1);
    let edited = AnalysisEngineSettings {
        command: ANALYSIS.into(),
        ..s.clone()
    };
    assert_eq!(
        wb.save_analysis_engine_settings(edited, &shown, false).unwrap(),
        Ok(true)
    );
    let reopened = workbench(dir.path(), &world).view(false).settings.analysis_engine;
    assert!(reopened.command_customized);
    assert_eq!(reopened.command, ANALYSIS);
}

#[test]
fn a_save_overlapping_the_quick_analysis_keeps_the_record_saved_and_later_saves_carry_the_results() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_analysis_engine_settings(analysis_settings(false), "", true)
        .unwrap()
        .unwrap();
    open(&mut wb, RECORD);
    assert!(!wb.view(false).document.unsaved);
    let path = dir.path().join("game.sgf");
    let job = wb.begin_save(path.to_str().unwrap(), "LizzieYzy Tauri:test");
    assert!(!job.text.contains("LZ["), "captured before any result");
    answer_queries(&mut wb, &world, *world.workers().last().unwrap());
    wb.complete_save(job.ticket, Ok(()));
    let v = wb.view(false);
    assert!(!v.document.unsaved, "results are no content change");
    let again = wb.begin_save(path.to_str().unwrap(), "LizzieYzy Tauri:test");
    assert_eq!(again.text.matches("LZ[").count(), 4, "{}", again.text);
}

struct Probe;
impl r::Probe for Probe {
    fn run(&self, _: &str, _: &[String], _: Duration) -> std::io::Result<r::ProbeOutput> {
        Ok(r::ProbeOutput::default())
    }
    fn read_file(&self, _: &std::path::Path) -> std::io::Result<Vec<u8>> {
        Err(std::io::ErrorKind::NotFound.into())
    }
    fn list_dir(&self, _: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
        Ok(Vec::new())
    }
    fn env_var(&self, _: &str) -> Option<String> {
        None
    }
}

struct Fetch(Vec<u8>);
impl r::Fetch for Fetch {
    fn get(&self, _: &str, start: Option<u64>) -> r::ResourceResult<r::FetchResponse> {
        let offset = start.unwrap_or(0) as usize;
        Ok(r::FetchResponse {
            status: if offset == 0 { 200 } else { 206 },
            total_length: Some(self.0.len() as u64),
            body: Box::new(std::io::Cursor::new(self.0[offset..].to_vec())),
        })
    }
}

/// 按需下载 of the lightweight model with the catalog's size; the job's expected digest is the
/// served bytes' own, as the real file's would be.
fn download_quick_model(wb: &mut Workbench) -> std::path::PathBuf {
    let bytes = vec![0u8; 38_245_488];
    let mut job = wb
        .begin_resource_task(ResourceCommand::Download {
            item: ResourceItem::QuickModel,
        })
        .unwrap();
    job.catalog.models.get_mut("b10-lightweight").unwrap().sha256 = format!("{:x}", Sha256::digest(&bytes));
    let result = run_resource_job(&job, &Fetch(bytes), &Probe, |_, _, _| {});
    assert!(wb.complete_resource_task(job.id, result));
    wb.storage()
        .resources_dir
        .join("quick-analysis-models/b10c384h6nbttflrs.bin.gz")
}

#[test]
fn the_downloaded_lightweight_model_is_used_while_it_verifies_on_katago_1_17() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_analysis_engine_settings(analysis_settings(false), "", true)
        .unwrap()
        .unwrap();
    assert_eq!(wb.view(false).settings.quick_model.status, QuickModelStatus::Missing);
    // A file of the catalog's name and size whose contents were never verified is not used.
    let model = wb
        .storage()
        .resources_dir
        .join("quick-analysis-models/b10c384h6nbttflrs.bin.gz");
    std::fs::create_dir_all(model.parent().unwrap()).unwrap();
    std::fs::write(&model, vec![1u8; 38_245_488]).unwrap();
    with_engine(&mut wb, &world);
    assert_eq!(wb.view(false).settings.quick_model.status, QuickModelStatus::Missing);
    std::fs::remove_file(&model).unwrap();

    // The download verifies it and turns 自动使用 on.
    assert_eq!(download_quick_model(&mut wb), model);
    let s = wb.view(false).settings.quick_model;
    assert_eq!((s.status, s.enabled), (QuickModelStatus::Enabled, true));
    open(&mut wb, RECORD);
    let args = world.args(*world.workers().last().unwrap());
    let at = args.iter().position(|a| a == "-model").unwrap();
    assert_eq!(std::path::Path::new(&args[at + 1]), model);
    let config = &args[args.iter().position(|a| a == "-config").unwrap() + 1];
    assert!(
        config.ends_with("configs/analysis.cfg") && std::path::Path::new(config).is_file(),
        "{config}"
    );

    // An older KataGo cannot run it: the configured command.
    world.set_version("1.16.4");
    wb.end_engines();
    let fg = with_engine(&mut wb, &world);
    serve(&mut wb, &world, fg, 0, true);
    assert_eq!(
        wb.view(false).settings.quick_model.status,
        QuickModelStatus::RequiresKataGo117
    );
    open(&mut wb, RECORD);
    assert!(world
        .args(*world.workers().last().unwrap())
        .contains(&"/a.cfg".to_string()));

    // Changed contents (same size, later time) are checked again and refused.
    world.set_version("1.18.2");
    wb.end_engines();
    let fg = with_engine(&mut wb, &world);
    serve(&mut wb, &world, fg, 0, true);
    assert_eq!(wb.view(false).settings.quick_model.status, QuickModelStatus::Enabled);
    let mut file = std::fs::OpenOptions::new().write(true).open(&model).unwrap();
    std::io::Write::write_all(&mut file, b"x").unwrap();
    file.set_modified(std::time::SystemTime::now() + Duration::from_secs(5))
        .unwrap();
    drop(file);
    assert_eq!(wb.view(false).settings.quick_model.status, QuickModelStatus::Missing);
    open(&mut wb, RECORD);
    assert!(world
        .args(*world.workers().last().unwrap())
        .contains(&"/a.cfg".to_string()));
}

/// A record whose main trunk has `moves` distinct moves.
fn long_record(moves: usize) -> String {
    let letters = b"abcdefghijklmnopqrs";
    let body: String = (0..moves)
        .map(|i| {
            let (x, y) = (letters[i % 19] as char, letters[(i / 19) * 2] as char);
            format!(";{}[{x}{y}]", if i % 2 == 0 { "B" } else { "W" })
        })
        .collect();
    format!("(;SZ[19]{body})")
}

/// The move number the engine holds at each finite search, from what it was sent.
fn searched_moves(sent: &[String]) -> Vec<usize> {
    let mut held = 0usize;
    let mut searched = Vec::new();
    for c in sent {
        match c.split(' ').next().unwrap() {
            "clear_board" => held = 0,
            "play" => held += 1,
            "undo" => held -= 1,
            "kata-analyze" if c.contains("maxmoves 1") => searched.push(held),
            _ => {}
        }
    }
    searched
}

#[test]
fn a_borrowed_engine_overviews_a_long_game_first_and_then_fills_it_in() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_analysis_engine_settings(analysis_settings(true), "", false)
        .unwrap()
        .unwrap();
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, &long_record(30));
    let searched = searched_moves(&serve(&mut wb, &world, fg, 20, true));
    assert_eq!(
        &searched[..5],
        &[1, 30, 9, 17, 25],
        "first, last and every 8th move first"
    );
    let mut all = searched.clone();
    all.sort_unstable();
    assert_eq!(all, (1..=30).collect::<Vec<_>>(), "then every other move once");
    assert!(!wb.view(false).quick.running);
}

#[test]
fn a_borrowed_engine_searches_with_the_chosen_rules_and_gets_its_own_back() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let settings = AnalysisEngineSettings {
        use_current_rules: false,
        specific_rules: JAPANESE.into(),
        ..analysis_settings(true)
    };
    wb.save_analysis_engine_settings(settings, "", false).unwrap().unwrap();
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    let sent = serve(&mut wb, &world, fg, 20, true);
    let first_search = sent.iter().position(|c| c.contains("maxmoves 1")).unwrap();
    assert!(
        sent[..first_search]
            .iter()
            .any(|c| c == &format!("kata-set-rules {JAPANESE}")),
        "{sent:?}"
    );
    let name = sent.iter().rposition(|c| c == "name").unwrap();
    let last_search = sent.iter().rposition(|c| c.contains("maxmoves 1")).unwrap();
    assert!(
        sent[last_search..name]
            .iter()
            .any(|c| c == &format!("kata-set-rules {CHINESE}")),
        "the captured rules are restored: {sent:?}"
    );
    assert_eq!(world.rules(fg).as_deref(), Some(CHINESE));
}

#[test]
fn changed_analysis_rules_drop_answers_asked_under_the_old_ones() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_analysis_engine_settings(analysis_settings(false), "", true)
        .unwrap()
        .unwrap();
    open(&mut wb, RECORD);
    let old = *world.workers().last().unwrap();
    let japanese = AnalysisEngineSettings {
        use_current_rules: false,
        specific_rules: JAPANESE.into(),
        ..wb.view(false).settings.analysis_engine
    };
    wb.save_analysis_engine_settings(japanese, ANALYSIS, false)
        .unwrap()
        .unwrap();
    answer_queries(&mut wb, &world, old);
    assert_eq!(analysed_moves(&mut wb), 0, "answers under the old rules are not kept");
    wb.engine_tick(Instant::now() + Duration::from_secs(3));
    let new = *world.workers().last().unwrap();
    assert_ne!(old, new, "asked again");
    let queries = world.take(new);
    assert!(queries.iter().all(|q| q.contains("TERRITORY")), "{queries:?}");
    for q in queries {
        let v: serde_json::Value = serde_json::from_str(&q).unwrap();
        let moves = v["moves"].as_array().unwrap().len();
        let w = winrate(moves);
        out(
            &mut wb,
            new,
            &serde_json::json!({
                "id": v["id"], "moveInfos": [{"move": "K10", "visits": 1, "winrate": w, "order": 0, "pv": ["K10"]}],
                "rootInfo": {"visits": 2},
            })
            .to_string(),
        );
    }
    assert_eq!(analysed_moves(&mut wb), 3);
}

#[test]
fn a_record_declaring_rules_waits_until_the_engine_confirms_them() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_analysis_engine_settings(analysis_settings(false), "", true)
        .unwrap()
        .unwrap();
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, "(;SZ[19]RU[Japanese];B[pd];W[dd];B[pp];W[dp])");
    assert!(
        world.workers().is_empty(),
        "not asked under the engine's previous rules"
    );
    assert!(wb.view(false).quick.retrying);
    serve(&mut wb, &world, fg, 0, true);
    wb.engine_tick(Instant::now() + Duration::from_secs(2));
    let worker = *world.workers().last().expect("asked once confirmed");
    let queries = world.take(worker);
    assert!(
        !queries.is_empty() && queries.iter().all(|q| q.contains("TERRITORY")),
        "{queries:?}"
    );
}

#[test]
fn a_borrowed_engine_whose_rules_cannot_be_read_is_given_back_without_searching() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_analysis_engine_settings(analysis_settings(true), "", false)
        .unwrap()
        .unwrap();
    let fg = with_engine(&mut wb, &world);
    world.refuse("kata-get-rules");
    open(&mut wb, RECORD);
    let sent = serve(&mut wb, &world, fg, 20, true);
    assert!(!sent.iter().any(|c| c.contains("maxmoves 1")), "{sent:?}");
    let v = wb.view(false);
    assert!(v.quick.running && v.quick.retrying, "a failed attempt");
    assert_eq!(v.engine.unrestored, None);
    assert!(
        sent.last().is_some_and(|c| c.starts_with("kata-analyze B 10")),
        "given back: {sent:?}"
    );
}

#[test]
fn a_refused_position_during_a_borrowed_search_is_rebuilt_when_given_back() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_analysis_engine_settings(analysis_settings(true), "", false)
        .unwrap()
        .unwrap();
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    wb.navigate(Navigation::Last);
    // The last move's position is refused while the borrowed engine searches it.
    world.refuse("play W D4");
    let sent = serve(&mut wb, &world, fg, 20, true);
    let refused = sent.iter().position(|c| c == "play W D4").unwrap();
    let name = sent.iter().rposition(|c| c == "name").unwrap();
    assert!(name > refused);
    assert!(
        sent[refused + 1..name].iter().any(|c| c == "clear_board")
            && sent[refused + 1..name].iter().any(|c| c == "play W D4"),
        "the browsing position is rebuilt before the engine is given back: {sent:?}"
    );
}

#[test]
fn a_pause_while_the_engine_is_taken_over_gives_it_back_unsearched() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_analysis_engine_settings(analysis_settings(true), "", false)
        .unwrap()
        .unwrap();
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    // The take-over `stop` is still unanswered.
    wb.toggle_analysis().unwrap();
    let sent = serve(&mut wb, &world, fg, 20, true);
    assert!(!sent.iter().any(|c| c.contains("maxmoves 1")), "{sent:?}");
    let name = sent
        .iter()
        .position(|c| c == "name")
        .unwrap_or_else(|| panic!("given back: {sent:?}"));
    let v = wb.view(false);
    assert!(v.engine.paused && !v.quick.running);
    assert_eq!(v.engine.unrestored, None);
    assert!(
        !sent[name..].iter().any(|c| c.starts_with("kata-analyze")),
        "the pause holds: {sent:?}"
    );
}

#[test]
fn a_pause_while_a_completed_lease_is_given_back_keeps_the_results_and_the_pause() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_analysis_engine_settings(analysis_settings(true), "", false)
        .unwrap()
        .unwrap();
    let fg = with_engine(&mut wb, &world);
    open(&mut wb, RECORD);
    // Every move searched; the hand-back's `name` is not answered yet.
    serve(&mut wb, &world, fg, 20, false);
    assert!(wb.view(false).quick.running);
    wb.toggle_analysis().unwrap();
    world.release_held(fg);
    let sent = serve(&mut wb, &world, fg, 0, true);
    let v = wb.view(false);
    assert!(v.engine.paused && !v.quick.running, "{:?}", v.quick);
    assert_eq!(v.engine.unrestored, None);
    assert_eq!(v.problems.analyzed, 3, "the results stay");
    assert!(
        !sent.iter().any(|c| c.starts_with("kata-analyze")),
        "the pause holds: {sent:?}"
    );
}

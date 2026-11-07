//! 自动检测引擎是否存活 through the public `Workbench` operations with controlled GTP processes.
//! Independent expectations: Java `EngineManager.checkEngineAlive` (3649-3675),
//! `Leelaz.AutomaticRestartAttempt` (17635-18130) with `canCheckAlive` (547, 6120, 7478, 7608,
//! 17991) and the engine-game participant recovery (EngineManager.java:5403-5506, 8845-9093);
//! the local branch is restarted too (the Java defect that left it dead is not kept).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lizzie_core::api::EngineScope;
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats::sgf;
use lizzie_core::record::RecordFormat;
use lizzie_core::settings::{AnalysisEngineSettings, Storage};
use lizzie_core::workbench::{OpenOrigin, Workbench};

/// Launched processes: instance, program, lines not yet answered.
type Processes = Vec<(InstanceId, String, Vec<String>)>;

/// The launched processes and whether launches fail.
#[derive(Clone, Default)]
struct World(Arc<Mutex<Processes>>, Arc<AtomicBool>);

impl World {
    fn take(&self, id: InstanceId) -> Vec<String> {
        let mut w = self.0.lock().unwrap();
        w.iter_mut().find(|p| p.0 == id).map(|p| std::mem::take(&mut p.2)).unwrap_or_default()
    }
    fn launched(&self) -> Vec<(InstanceId, String)> {
        self.0.lock().unwrap().iter().map(|p| (p.0, p.1.clone())).collect()
    }
    fn last(&self) -> InstanceId {
        self.0.lock().unwrap().last().unwrap().0
    }
    /// From now on no process can be started (the program disappeared).
    fn refuse_launches(&self) {
        self.1.store(true, Ordering::SeqCst);
    }
}

struct Io(World, InstanceId);
impl EngineIo for Io {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        let mut w = self.0 .0.lock().unwrap();
        w.iter_mut().find(|p| p.0 == self.1).unwrap().2.push(line.to_string());
        Ok(())
    }
    fn terminate(self: Box<Self>) {}
}

struct Launch(World);
impl Launcher for Launch {
    fn launch(&self, id: InstanceId, spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        if self.0 .1.load(Ordering::SeqCst) {
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "No such file or directory"));
        }
        self.0 .0.lock().unwrap().push((id, spec.program.clone(), Vec::new()));
        Ok(Box::new(Io(self.0.clone(), id)))
    }
}

const KATA: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nset_position\nkata-analyze\nkata-set-rules\nkata-get-rules\nstop";
const RULES: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;
const RECORD: &str = "(;SZ[19]KM[6.5];B[pd];W[dd];B[pp])";

fn out(wb: &mut Workbench, id: InstanceId, line: &str) {
    wb.engine_input(id, EngineInput::Stdout(line.to_string()));
}

/// Answers every command of `id` (analysis requests get their header only). Returns them.
fn serve(wb: &mut Workbench, world: &World, id: InstanceId) -> Vec<String> {
    let mut done = Vec::new();
    loop {
        let pending = world.take(id);
        if pending.is_empty() {
            return done;
        }
        for line in pending {
            if line.starts_with('{') {
                continue;
            }
            out(wb, id, "");
            let (n, cmd) = line.split_once(' ').unwrap();
            let reply = match cmd.split(' ').next().unwrap() {
                "name" => "KataGo",
                "version" => "1.16.4",
                "list_commands" => KATA,
                "kata-get-rules" => RULES,
                _ => "",
            };
            out(wb, id, &format!("={n} {reply}"));
            if !cmd.starts_with("kata-analyze") {
                out(wb, id, "");
            }
            done.push(cmd.to_string());
        }
    }
}

fn info(wb: &mut Workbench, id: InstanceId, visits: u64) {
    out(wb, id, &format!("info move Q16 visits {visits} winrate 0.6 scoreLead 1.5 prior 0.2 order 0 pv Q16 D4 rootInfo visits {visits}"));
}

fn open(wb: &mut Workbench, text: &str) {
    let t = wb.begin_open(OpenOrigin::File { path: "/x.sgf".into(), source: lizzie_core::api::OpenSource::Drop });
    assert!(wb.complete_open(t, Ok((RecordFormat::Sgf, sgf::read(text).unwrap()))));
}

fn profile(id: &str, program: &str) -> EngineProfile {
    EngineProfile { id: id.into(), name: id.into(), command: format!("{program} gtp"), ..EngineProfile::default() }
}

fn workbench(dir: &std::path::Path, world: &World, engines: Vec<EngineProfile>) -> Workbench {
    let storage = Storage::in_dirs(&dir.join("config"), &dir.join("data"));
    let mut wb = Workbench::new(storage, None, Box::new(Launch(world.clone())), Vec::new());
    wb.set_auto_quick_analyze(false).unwrap();
    wb.save_engine_settings(engines, EngineStartup::Default).unwrap();
    wb
}

/// A ready foreground KataGo analysing the last move of `RECORD`; it produced a line.
fn analysing(dir: &std::path::Path, world: &World) -> (Workbench, InstanceId) {
    let mut wb = workbench(dir, world, vec![profile("kata", "/opt/katago")]);
    wb.start_engines();
    let id = world.last();
    serve(&mut wb, world, id);
    open(&mut wb, RECORD);
    wb.navigate(Navigation::Last);
    serve(&mut wb, world, id);
    info(&mut wb, id, 50);
    (wb, id)
}

fn set_auto_check(wb: &mut Workbench, on: bool) {
    let mut s = wb.config().engine.analysis.clone();
    s.auto_check_engine_alive = on;
    wb.set_analysis_settings(s).unwrap();
}

#[test]
fn a_foreground_engine_that_dies_starts_again_and_resumes_the_current_position() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, old) = analysing(dir.path(), &world);
    wb.set_komi(0.5).unwrap();
    serve(&mut wb, &world, old);
    wb.engine_input(old, EngineInput::Exited(Some(137)));

    // Java's abnormal-end diagnostic is still reported, as an end after start-up.
    let v = wb.view(false);
    let f = v.engine_failure.expect("the end is reported");
    assert!(!f.during_startup);
    let launched = world.launched();
    assert_eq!(launched.len(), 2, "the same engine starts again");
    let new = world.last();
    assert_eq!(launched[1].1, "/opt/katago");
    // While it starts it is the foreground engine (引擎正在加载中), not a failed one.
    let fg = v.engine.foreground.expect("restarted in the foreground");
    assert!(!fg.ready && !v.engine.down);

    let sent = serve(&mut wb, &world, new);
    let v = wb.view(false);
    assert!(v.engine.foreground.unwrap().ready);
    assert!(v.engine.analysing, "it was analysing, so it analyses again");
    assert_eq!(v.komi, 0.5, "the game's komi, not the profile's");
    assert!(sent.iter().any(|c| c == "komi 0.5"));
    assert!(sent.iter().any(|c| c == "play B Q4"), "synced to the current node: {sent:?}");
    assert!(sent.iter().any(|c| c.starts_with("kata-analyze W")));
}

#[test]
fn a_paused_analysis_stays_paused_after_the_restart() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, old) = analysing(dir.path(), &world);
    wb.toggle_analysis().unwrap();
    serve(&mut wb, &world, old);
    wb.engine_input(old, EngineInput::Exited(Some(1)));
    let new = world.last();
    assert_ne!(new, old);
    let sent = serve(&mut wb, &world, new);
    assert!(!sent.iter().any(|c| c.starts_with("kata-analyze") && !c.contains("focus")), "{sent:?}");
    assert!(!wb.view(false).engine.analysing);
}

#[test]
fn the_position_reached_while_restarting_is_the_one_synced() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, old) = analysing(dir.path(), &world);
    wb.engine_input(old, EngineInput::Exited(Some(1)));
    let new = world.last();
    // The user goes back two moves before the new process answered anything.
    wb.navigate(Navigation::Back(2));
    let sent = serve(&mut wb, &world, new);
    assert!(sent.iter().any(|c| c == "play B Q16"), "{sent:?}");
    assert!(!sent.iter().any(|c| c == "play B Q4"), "the abandoned position is not restored: {sent:?}");
    assert!(sent.iter().any(|c| c.starts_with("kata-analyze W")));
    assert_eq!(wb.view(false).position.node, 1);
}

#[test]
fn turning_the_setting_off_leaves_a_dead_engine_down() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, old) = analysing(dir.path(), &world);
    set_auto_check(&mut wb, false);
    wb.engine_input(old, EngineInput::Exited(Some(1)));
    assert_eq!(world.launched().len(), 1);
    let v = wb.view(false);
    assert!(v.engine.foreground.is_none() && v.engine.down, "引擎未成功加载");
    assert!(v.engine_failure.is_some());
    // On again: the next end of a running engine restarts it.
    set_auto_check(&mut wb, true);
    wb.switch_engine(0).unwrap();
    let id = world.last();
    serve(&mut wb, &world, id);
    wb.engine_input(id, EngineInput::Exited(Some(1)));
    assert_eq!(world.launched().len(), 3);
}

#[test]
fn a_restarted_engine_that_dies_before_producing_anything_is_not_restarted_again() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, old) = analysing(dir.path(), &world);
    wb.engine_input(old, EngineInput::Exited(Some(1)));
    let second = world.last();
    serve(&mut wb, &world, second);
    // Ready and synced, but no accepted analysis line yet (`canCheckAlive` still false).
    wb.engine_input(second, EngineInput::Exited(Some(1)));
    assert_eq!(world.launched().len(), 2, "no restart loop");
    let v = wb.view(false);
    assert!(v.engine.down && v.engine_failure.is_some());

    // A manually started engine is checked once ready; a restarted one after its first line.
    wb.switch_engine(0).unwrap();
    let third = world.last();
    serve(&mut wb, &world, third);
    wb.engine_input(third, EngineInput::Exited(Some(1)));
    let fourth = world.last();
    assert_ne!(fourth, third);
    serve(&mut wb, &world, fourth);
    info(&mut wb, fourth, 20);
    wb.engine_input(fourth, EngineInput::Exited(Some(1)));
    assert_ne!(world.last(), fourth, "proven by its analysis output");
}

#[test]
fn an_engine_that_fails_while_starting_is_not_restarted() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world, vec![profile("kata", "/opt/katago")]);
    wb.start_engines();
    let id = world.last();
    wb.engine_input(id, EngineInput::Exited(Some(127)));
    assert_eq!(world.launched().len(), 1);
    let v = wb.view(false);
    assert!(v.engine_failure.unwrap().during_startup && v.engine.down);
}

#[test]
fn a_restart_that_cannot_launch_is_reported_like_any_start_and_not_retried() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, old) = analysing(dir.path(), &world);
    world.refuse_launches();
    wb.engine_input(old, EngineInput::Exited(Some(1)));
    let v = wb.view(false);
    let failure = v.engine_failure.expect("the launch failure is shown");
    assert!(failure.during_startup && failure.message.contains("No such file"));
    assert!(v.engine.foreground.is_none() && v.engine.down);
    assert_eq!(world.launched().len(), 1);
    wb.engine_tick(Instant::now() + Duration::from_secs(10));
    assert!(wb.view(false).engine_failure.is_none(), "no further attempt");
}

#[test]
fn user_shutdown_switch_and_exit_are_not_deaths() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world, vec![profile("a", "/opt/a"), profile("b", "/opt/b")]);
    wb.start_engines();
    let a = world.last();
    serve(&mut wb, &world, a);
    info(&mut wb, a, 10);
    // Switching away keeps A running in the background (fast engine change).
    wb.switch_engine(1).unwrap();
    let b = world.last();
    serve(&mut wb, &world, b);
    info(&mut wb, b, 10);
    // A background engine that dies is not the foreground engine: no restart.
    wb.engine_input(a, EngineInput::Exited(Some(1)));
    assert_eq!(world.launched().len(), 2);
    assert!(wb.view(false).engine.foreground.unwrap().ready);
    // 关闭当前引擎, then the process's own end arrives late.
    wb.shutdown_engines(EngineScope::Current);
    wb.engine_input(b, EngineInput::Exited(Some(0)));
    assert_eq!(world.launched().len(), 2);
    let v = wb.view(false);
    assert!(v.engine.foreground.is_none() && !v.engine.down);
    // Exit.
    wb.switch_engine(0).unwrap();
    let c = world.last();
    serve(&mut wb, &world, c);
    wb.end_engines();
    wb.engine_input(c, EngineInput::Exited(Some(0)));
    assert_eq!(world.launched().len(), 3);
}

#[test]
fn the_old_instances_late_output_and_end_neither_restart_nor_reach_the_new_engine() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, old) = analysing(dir.path(), &world);
    wb.engine_input(old, EngineInput::Exited(Some(1)));
    let new = world.last();
    serve(&mut wb, &world, new);
    // More visits than the result stored before the end, so the new line replaces it.
    info(&mut wb, new, 60);
    // Late lines and a second end from the dead process.
    info(&mut wb, old, 9999);
    wb.engine_input(old, EngineInput::Exited(Some(1)));
    wb.engine_input(old, EngineInput::Stdout("= ".into()));
    assert_eq!(world.launched().len(), 2);
    let v = wb.view(false);
    assert_eq!(v.analysis.unwrap().visits, 60);
    assert!(v.engine.foreground.unwrap().ready);
}

#[test]
fn an_engine_lent_to_quick_analysis_when_it_dies_is_not_restarted() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = analysing(dir.path(), &world);
    let reuse = AnalysisEngineSettings { reuse_current_engine: true, ..Default::default() };
    wb.save_analysis_engine_settings(reuse, "", false).unwrap().unwrap();
    wb.set_auto_quick_analyze(true).unwrap();
    open(&mut wb, RECORD);
    serve(&mut wb, &world, id);
    wb.engine_tick(Instant::now() + Duration::from_secs(2));
    serve(&mut wb, &world, id);
    assert!(wb.view(true).quick.borrowed, "the foreground engine is lent");
    wb.engine_input(id, EngineInput::Exited(Some(1)));
    assert_eq!(world.launched().len(), 1, "the lease owned its state (exclusive GTP session)");
    assert!(wb.view(false).engine.down);
}

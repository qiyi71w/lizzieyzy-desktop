//! 自动检测引擎是否存活 through the public `Workbench` operations with controlled GTP processes.
//! Independent expectations: Java `EngineManager.checkEngineAlive` (3649-3675),
//! `Leelaz.AutomaticRestartAttempt` (17635-18130) with `canCheckAlive` (547, 6120, 7478, 7608,
//! 17991) and the engine-game participant recovery (EngineManager.java:5403-5506, 8845-9093);
//! the local branch is restarted too (the Java defect that left it dead is not kept).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lizzie_core::api::{BatchMode, EngineScope, OpenSource, UnrestoredReason};
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats::{self, sgf};
use lizzie_core::record::RecordFormat;
use lizzie_core::settings::{AnalysisEngineSettings, AutoAnalysisSettings, Storage};
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
    serve_with(wb, world, id, "", &mut |_, _| {})
}

/// As [`serve`], refusing the command `refused` (`?illegal move`) and calling `before` with
/// each command before it is answered.
fn serve_with(
    wb: &mut Workbench,
    world: &World,
    id: InstanceId,
    refused: &str,
    before: &mut dyn FnMut(&mut Workbench, &str),
) -> Vec<String> {
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
            before(wb, cmd);
            let reply = match cmd.split(' ').next().unwrap() {
                "name" => "KataGo",
                "version" => "1.16.4",
                "list_commands" => KATA,
                "kata-get-rules" => RULES,
                _ => "",
            };
            if cmd == refused {
                out(wb, id, &format!("?{n} illegal move"));
                out(wb, id, "");
                done.push(cmd.to_string());
                continue;
            }
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
fn cloud_recovery_replays_the_latest_position_without_ordinary_alive_check_or_failure_dialog() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world, Vec::new());
    set_auto_check(&mut wb, false);
    open(&mut wb, RECORD);
    wb.navigate(Navigation::Last);
    wb.activate_cloud(lizzie_core::cloud::CloudSettings {
        provider: lizzie_core::cloud::Provider::Zhizi,
        identifier: "fixture-account".into(),
        ..Default::default()
    }, true).unwrap();
    let old = world.last();
    serve(&mut wb, &world, old);
    wb.engine_input(old, EngineInput::CloudRecovery { message: "connection lost".into() });
    let new = world.last();
    assert_ne!(new, old);
    wb.navigate(Navigation::Back(2));
    wb.set_komi(0.5).unwrap();
    let sent = serve(&mut wb, &world, new);
    let view = wb.view(false);
    assert!(view.engine.foreground.unwrap().ready);
    assert!(view.engine_failure.is_none(), "cloud reconstruction is quiet");
    assert!(sent.iter().any(|c| c == "komi 0.5"));
    assert!(sent.iter().any(|c| c == "play B Q16"));
    assert!(!sent.iter().any(|c| c == "play B Q4"));
    wb.engine_input(old, EngineInput::CloudRecovery { message: "late retired callback".into() });
    assert_eq!(world.last(), new);
}

#[test]
fn cloud_recovery_retires_the_borrow_and_rebuilds_the_foreground() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world, Vec::new());
    set_auto_check(&mut wb, false);
    wb.activate_cloud(lizzie_core::cloud::CloudSettings {
        provider: lizzie_core::cloud::Provider::Zhizi,
        identifier: "fixture-account".into(),
        ..Default::default()
    }, true).unwrap();
    let old = world.last();
    serve(&mut wb, &world, old);
    wb.set_auto_quick_analyze(true).unwrap();
    open(&mut wb, RECORD);
    serve(&mut wb, &world, old);
    wb.engine_tick(Instant::now() + Duration::from_secs(2));
    serve(&mut wb, &world, old);
    assert!(wb.view(true).quick.borrowed);
    wb.engine_input(old, EngineInput::CloudRecovery { message: "accepted progress stalled".into() });
    let new = world.last();
    assert_ne!(new, old, "cloud recovery follows lease retirement, unlike ordinary alive checks");
    serve(&mut wb, &world, new);
    assert!(wb.view(false).engine.foreground.unwrap().ready);
}

fn zhizi(args: &str) -> lizzie_core::cloud::CloudSettings {
    lizzie_core::cloud::CloudSettings {
        provider: lizzie_core::cloud::Provider::Zhizi,
        identifier: "fixture-account".into(),
        args: args.into(),
        ..Default::default()
    }
}

/// The cloud switch receipt (seated profile, `view_seq` read under the commit): a launch that
/// fails at once is already down in the first later view, and re-enabling after that failure
/// shows the new engine after its boundary while the old failure stays before it.
#[test]
fn cloud_switch_boundary_separates_an_old_failure_from_the_switch_outcome() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world, Vec::new());
    world.refuse_launches();
    let profile = wb.activate_cloud(zhizi(""), true).unwrap();
    let after = wb.view_seq();
    assert_eq!(wb.config().engine.engines.iter().find(|p| p.is_default).unwrap().id, profile);
    let failed = wb.view(false);
    assert!(failed.seq > after);
    assert!(failed.engine.down && failed.engine.foreground.is_none(), "the immediate launch failure is the first later view");

    world.1.store(false, Ordering::SeqCst);
    let old = wb.view(false);
    assert!(old.engine.down, "the old failure is still published before the next switch commits");
    let again = wb.activate_cloud(zhizi(""), true).unwrap();
    let after = wb.view_seq();
    assert_eq!(again, profile);
    assert!(old.seq <= after, "the old failure is not an outcome of the new switch");
    let starting = wb.view(false);
    assert!(starting.seq > after && !starting.engine.down);
    let fg = starting.engine.foreground.expect("the switched-in engine holds the seat");
    assert!(!fg.ready);
    assert_eq!(wb.config().engine.engines[fg.index as usize].id, profile);
    serve(&mut wb, &world, world.last());
    assert!(wb.view(false).engine.foreground.unwrap().ready);
}

/// Re-enabling the same provider with other launch arguments keeps its profile but seats a new
/// launch: the old ready engine's view lies before the receipt boundary, and the first later
/// view shows the new launch loading until it is ready.
#[test]
fn cloud_switch_with_changed_args_seats_a_new_launch_of_the_same_profile() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world, Vec::new());
    let profile = wb.activate_cloud(zhizi("--gpu-type vip-share --kata-weight 10b512t"), true).unwrap();
    let old = world.last();
    serve(&mut wb, &world, old);
    let before = wb.view(false);
    assert!(before.engine.foreground.unwrap().ready);

    let again = wb.activate_cloud(zhizi("--gpu-type vip-share --kata-weight 18b"), true).unwrap();
    let after = wb.view_seq();
    assert_eq!(again, profile, "the same provider keeps its profile");
    assert!(before.seq <= after, "the old ready engine is not an outcome of the new switch");
    let new = world.last();
    assert_ne!(new, old, "the changed launch starts a new engine");
    let v = wb.view(false);
    assert!(v.seq > after);
    let fg = v.engine.foreground.expect("the new launch holds the seat");
    assert!(!fg.ready, "not ready until the new launch is");
    let seated = &wb.config().engine.engines[fg.index as usize];
    assert_eq!(seated.id, profile);
    assert_eq!(seated.cloud.as_ref().unwrap().args, "--gpu-type vip-share --kata-weight 18b");
    serve(&mut wb, &world, new);
    assert!(wb.view(false).engine.foreground.unwrap().ready);
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

/// Ticket decision and Java's restore round (Leelaz.java:18146-18205): a restarted engine that
/// refuses the position it is brought to is left unrestored, without a dialog, and is not tried
/// again.
#[test]
fn a_restarted_engine_that_refuses_the_position_is_left_unrestored() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, old) = analysing(dir.path(), &world);
    wb.engine_input(old, EngineInput::Exited(Some(1)));
    assert!(wb.view(false).engine_failure.is_some(), "the end itself is reported");
    let new = world.last();
    serve_with(&mut wb, &world, new, "play B Q4", &mut |_, _| {});
    let v = wb.view(false);
    assert!(v.engine_failure.is_none(), "no dialog for the refused restore");
    assert_eq!(v.engine.unrestored, Some(UnrestoredReason::Restore), "引擎状态未恢复");
    assert!(v.analysis.is_none_or(|a| a.visits == 50), "nothing analysed for the refused position");
    // It follows no wish any more, and its end is not restarted either.
    wb.navigate(Navigation::Back(1));
    assert!(serve(&mut wb, &world, new).is_empty());
    wb.engine_input(new, EngineInput::Exited(Some(1)));
    assert_eq!(world.launched().len(), 2, "not tried again");
}

/// The restore round of a restarted engine includes the record's rules: declining the rules
/// override it asks for fails the restore, which leaves the engine unrestored without a dialog
/// and without another attempt (not merely paused).
#[test]
fn declining_the_rules_override_of_a_restarted_engine_leaves_it_unrestored() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world, vec![profile("kata", "/opt/katago")]);
    wb.start_engines();
    let old = world.last();
    serve(&mut wb, &world, old);
    // The engine keeps its own (Chinese) rules: the record's Japanese ones cannot be applied.
    open(&mut wb, "(;SZ[19]RU[Japanese];B[pd];W[dd];B[pp])");
    wb.navigate(Navigation::Last);
    serve(&mut wb, &world, old);
    let p = wb.view(false).rules_prompt.expect("asked about the record's rules");
    assert!(wb.answer_rules_prompt(p.prompt, true));
    serve(&mut wb, &world, old);
    info(&mut wb, old, 50);

    wb.engine_input(old, EngineInput::Exited(Some(1)));
    assert!(wb.view(false).engine_failure.is_some(), "the end itself is reported");
    let new = world.last();
    serve(&mut wb, &world, new);
    let p = wb.view(false).rules_prompt.expect("the restarted engine asks again");
    assert!(wb.answer_rules_prompt(p.prompt, false));
    serve(&mut wb, &world, new);
    let v = wb.view(false);
    assert!(v.engine_failure.is_none(), "no dialog for the failed restore");
    assert_eq!(v.engine.unrestored, Some(UnrestoredReason::Restore), "引擎状态未恢复");
    // It follows no wish: resuming analyses nothing, and nothing is tried again.
    wb.toggle_analysis().unwrap();
    assert!(!serve(&mut wb, &world, new).iter().any(|c| c.starts_with("kata-analyze")));
    assert_eq!(world.launched().len(), 2);
}

/// The position the user moved to while the restarted engine was being restored is the one it
/// must hold: a refusal of that catch-up leaves it unrestored; holding it ends the restore.
#[test]
fn the_position_reached_during_the_restore_settles_it() {
    for refused in ["undo", ""] {
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let (mut wb, old) = analysing(dir.path(), &world);
        wb.engine_input(old, EngineInput::Exited(Some(1)));
        wb.view(false);
        let new = world.last();
        let mut moved = false;
        let sent = serve_with(&mut wb, &world, new, refused, &mut |wb, cmd| {
            if cmd == "play B Q4" && !moved {
                moved = true;
                wb.navigate(Navigation::Back(1));
            }
        });
        assert!(moved && sent.iter().any(|c| c == "undo"), "caught up after the first restore: {sent:?}");
        let v = wb.view(false);
        assert!(v.engine_failure.is_none());
        if refused.is_empty() {
            assert_eq!(v.engine.unrestored, None);
            assert!(v.engine.analysing && v.position.node == 2);
        } else {
            assert_eq!(v.engine.unrestored, Some(UnrestoredReason::Restore), "the newest position was refused");
        }
    }
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

/// A restarted engine analysing 批量分析's record is proven only by a line that record accepts
/// (Leelaz.java:7478, 7608): a line without candidates leaves it unproven.
#[test]
fn only_a_batch_line_that_is_accepted_proves_a_restarted_engine() {
    for accepted in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let (mut wb, old) = analysing(dir.path(), &world);
        wb.engine_input(old, EngineInput::Exited(Some(1)));
        let new = world.last();
        serve(&mut wb, &world, new);
        let path = dir.path().join("a.sgf");
        std::fs::write(&path, "(;SZ[19];B[pd];W[dd];B[pp];W[dp])").unwrap();
        wb.open_batch(BatchMode::Ordinary, vec![path.to_string_lossy().into_owned()], OpenSource::Chooser).unwrap();
        let deliver = |wb: &mut Workbench| {
            for (ticket, path, no_capture) in wb.take_batch_reads() {
                assert!(wb.complete_batch_read(ticket, formats::read_file(&path, no_capture)));
            }
        };
        deliver(&mut wb);
        let settings = AutoAnalysisSettings { visits_enabled: true, visits: 1000, time_enabled: false, auto_save: false, ..Default::default() };
        wb.start_batch_ordinary(settings).unwrap().unwrap();
        deliver(&mut wb);
        serve(&mut wb, &world, new);
        assert!(wb.view(false).batch.running.is_some());
        if accepted {
            info(&mut wb, new, 5);
        } else {
            out(&mut wb, new, "info rootInfo visits 5 winrate 0.5 scoreLead 0.5");
        }
        let before = world.launched().len();
        wb.engine_input(new, EngineInput::Exited(Some(1)));
        assert_eq!(world.launched().len(), before + usize::from(accepted), "accepted line: {accepted}");
    }
}

#[test]
fn cloud_local_skips_profiles_with_missing_model_config_or_jar() {
    let dir = tempfile::tempdir().unwrap();
    let executable = dir.path().join("engine");
    std::fs::write(&executable, "fixture").unwrap();
    let missing = dir.path().join("missing");
    let world = World::default();
    let mut profiles = Vec::new();
    for (i, option) in ["-model", "--config=", "-jar"].iter().enumerate() {
        let mut p = profile(&format!("missing-{i}"), executable.to_str().unwrap());
        p.command = format!("\"{}\" {}{}\"{}\"", executable.display(), option,
            if option.ends_with('=') { "" } else { " " }, missing.display());
        profiles.push(p);
    }
    profiles.push(profile("usable", executable.to_str().unwrap()));
    let mut wb = workbench(dir.path(), &world, profiles);
    wb.cloud_local().unwrap();
    assert_eq!(wb.config().engine.engines.iter().find(|p| p.is_default).unwrap().id, "usable");
    assert_eq!(wb.config().cloud.provider, lizzie_core::cloud::Provider::Local);
}

/// Persist the same provider/default/last choice as an explicit cloud activation, then open a
/// fresh workbench with no session credentials. No real transport or network is involved.
fn cloud_startup_workbench(
    dir: &std::path::Path,
    world: &World,
    provider: lizzie_core::cloud::Provider,
    startup: EngineStartup,
    with_local: bool,
) -> Workbench {
    let executable = dir.join("local-engine");
    std::fs::write(&executable, "fixture").unwrap();
    let mut missing = profile("missing-model", executable.to_str().unwrap());
    missing.command = format!("\"{}\" -model \"{}\"", executable.display(), dir.join("missing-model").display());
    let mut profiles = vec![missing];
    if with_local {
        profiles.push(profile("first-usable-local", executable.to_str().unwrap()));
        let later = dir.join("later-local-engine");
        std::fs::write(&later, "fixture").unwrap();
        profiles.push(profile("later-usable-local", later.to_str().unwrap()));
    }
    let staging_world = World::default();
    let mut staging = workbench(dir, &staging_world, profiles);
    staging.activate_cloud(lizzie_core::cloud::CloudSettings {
        provider, identifier: "fixture-account".into(),
        custom_url: "ws://fixture.invalid/private-path".into(),
        ..Default::default()
    }, true).unwrap();
    let profiles = staging.config().engine.engines.clone();
    staging.save_engine_settings(profiles, startup).unwrap();
    staging.end_engines();
    drop(staging);
    Workbench::new(
        Storage::in_dirs(&dir.join("config"), &dir.join("data")),
        None, Box::new(Launch(world.clone())), Vec::new(),
    )
}

#[test]
fn unauthenticated_zhizi_default_and_last_fall_back_without_changing_saved_selection() {
    for startup in [EngineStartup::Default, EngineStartup::Last] {
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let mut wb = cloud_startup_workbench(dir.path(), &world,
            lizzie_core::cloud::Provider::Zhizi, startup, true);
        let path = dir.path().join("config/config.json");
        let bytes_before = std::fs::read(&path).unwrap();
        let config_before = serde_json::to_value(wb.config()).unwrap();
        let selected = wb.config().engine.engines.iter().find(|p| p.is_default).unwrap().id.clone();
        assert_eq!(wb.config().engine.last_engine.as_deref(), Some(selected.as_str()));
        wb.start_engines();
        assert_eq!(world.launched().len(), 1);
        assert_eq!(world.launched()[0].1, dir.path().join("local-engine").to_str().unwrap());
        assert_eq!(serde_json::to_value(wb.config()).unwrap(), config_before);
        assert_eq!(std::fs::read(&path).unwrap(), bytes_before);
        assert_eq!(wb.config().cloud.provider, lizzie_core::cloud::Provider::Zhizi);
        assert_eq!(wb.config().engine.startup, startup);
        assert_eq!(wb.config().engine.last_engine.as_deref(), Some(selected.as_str()));
        assert_eq!(wb.config().engine.engines.iter().find(|p| p.is_default).unwrap().id, selected);
    }
}

#[test]
fn authenticated_zhizi_startup_keeps_the_requested_cloud_engine() {
    for startup in [EngineStartup::Default, EngineStartup::Last] {
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let mut wb = cloud_startup_workbench(dir.path(), &world,
            lizzie_core::cloud::Provider::Zhizi, startup, true);
        let bytes_before = std::fs::read(dir.path().join("config/config.json")).unwrap();
        assert!(!wb.set_cloud_authenticated(true));
        assert!(wb.set_cloud_authenticated(false));
        assert!(!wb.set_cloud_authenticated(true));
        assert_eq!(std::fs::read(dir.path().join("config/config.json")).unwrap(), bytes_before);
        wb.start_engines();
        assert_eq!(world.launched().len(), 1);
        assert_eq!(world.launched()[0].1, "", "cloud launch has no local executable");
        assert_eq!(wb.config().cloud.provider, lizzie_core::cloud::Provider::Zhizi);
    }
}

#[test]
fn custom_cloud_startup_does_not_require_zhizi_authentication() {
    for startup in [EngineStartup::Default, EngineStartup::Last] {
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let mut wb = cloud_startup_workbench(dir.path(), &world,
            lizzie_core::cloud::Provider::Custom, startup, true);
        wb.start_engines();
        assert_eq!(world.launched().len(), 1);
        assert_eq!(world.launched()[0].1, "");
        assert_eq!(wb.config().cloud.provider, lizzie_core::cloud::Provider::Custom);
    }
}

#[test]
fn unauthenticated_zhizi_without_a_usable_local_keeps_the_requested_selection() {
    for startup in [EngineStartup::Default, EngineStartup::Last] {
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let mut wb = cloud_startup_workbench(dir.path(), &world,
            lizzie_core::cloud::Provider::Zhizi, startup, false);
        let config_before = serde_json::to_value(wb.config()).unwrap();
        wb.start_engines();
        assert_eq!(world.launched().len(), 1);
        assert_eq!(world.launched()[0].1, "");
        assert_eq!(serde_json::to_value(wb.config()).unwrap(), config_before);
    }
}

#[test]
fn cloud_login_fallback_does_not_start_an_engine_when_startup_is_empty_or_manual() {
    for startup in [EngineStartup::Empty, EngineStartup::Manual] {
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let mut wb = cloud_startup_workbench(dir.path(), &world,
            lizzie_core::cloud::Provider::Zhizi, startup, true);
        wb.start_engines();
        assert!(world.launched().is_empty());
    }
}

#[test]
fn session_local_fallback_preserves_independent_preloaded_profiles() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = cloud_startup_workbench(dir.path(), &world,
        lizzie_core::cloud::Provider::Zhizi, EngineStartup::Last, true);
    let mut profiles = wb.config().engine.engines.clone();
    profiles.iter_mut().find(|p| p.id == "later-usable-local").unwrap().preload = true;
    wb.save_engine_settings(profiles, EngineStartup::Last).unwrap();
    let bytes_before = std::fs::read(dir.path().join("config/config.json")).unwrap();
    wb.start_engines();
    let launched: Vec<String> = world.launched().into_iter().map(|(_, program)| program).collect();
    assert_eq!(launched, vec![
        dir.path().join("local-engine").to_str().unwrap().to_string(),
        dir.path().join("later-local-engine").to_str().unwrap().to_string(),
    ]);
    assert_eq!(std::fs::read(dir.path().join("config/config.json")).unwrap(), bytes_before);
}

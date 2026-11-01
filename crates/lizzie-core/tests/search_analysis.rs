//! 强制计算区域 and 选点重点研究 through the public `Workbench` operations with controlled GTP
//! engines: each engine's start-up `focus` probe (supported, unsupported, failed, a paused start,
//! a replaced engine answering late), the focus requests sent for tracked points and their end,
//! the binding of tracking to one position and engine, and the region's requests, lifetime and
//! effect on stored results.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lizzie_core::api::{OpenSource, RegionCommand, TrackingCommand};
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, FocusCapability, InstanceId, Launcher};
use lizzie_core::formats::sgf;
use lizzie_core::record::{Point, RecordFormat};
use lizzie_core::settings::{RegionSettings, Storage, TrackingSettings};
use lizzie_core::workbench::{OpenOrigin, Workbench};

#[derive(Default)]
struct Proc {
    program: String,
    sent: Vec<String>,
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
    fn engine(&self, program: &str) -> InstanceId {
        let w = self.0.lock().unwrap();
        w.iter().rev().find(|p| p.1.program == program).map(|p| p.0).unwrap()
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
const PROBE_ARGS: &str = "10 rootInfo true focus pass 0";

/// How an engine answers the start-up `focus` probe.
#[derive(Clone, Copy)]
enum Probe {
    Accept,
    /// Official KataGo before `focus`: its argument error for exactly the probe's arguments.
    Reject,
    Fail,
}

struct Engine {
    id: InstanceId,
    probe: Probe,
}

fn workbench(dir: &std::path::Path, world: &World) -> Workbench {
    Workbench::new(
        Storage::in_dirs(&dir.join("config"), &dir.join("data")),
        None,
        Box::new(FakeLauncher(world.clone())),
        Vec::new(),
    )
}

fn out(wb: &mut Workbench, id: InstanceId, line: &str) {
    wb.engine_input(id, EngineInput::Stdout(line.to_string()));
}

/// Answers every outstanding command; an ordinary `kata-analyze` gets its header and the
/// `info` line, if any. Returns the commands.
fn serve(wb: &mut Workbench, world: &World, e: &Engine, info: Option<&str>) -> Vec<String> {
    let mut done = Vec::new();
    loop {
        let pending = world.take(e.id);
        if pending.is_empty() {
            return done;
        }
        for line in pending {
            out(wb, e.id, "");
            let (n, cmd) = line.split_once(' ').unwrap();
            let reply = |wb: &mut Workbench, text: &str| {
                out(wb, e.id, &format!("={n} {text}"));
                out(wb, e.id, "");
            };
            match cmd.split(' ').next().unwrap() {
                "name" => reply(wb, "KataGo"),
                "version" => reply(wb, "1.18.2"),
                "list_commands" => reply(wb, KATA),
                "kata-get-rules" => reply(wb, "{}"),
                "kata-analyze" if cmd.ends_with(PROBE_ARGS) => match e.probe {
                    // The stream stays open until the `stop` that follows; its lines are no
                    // request's.
                    Probe::Accept => {
                        out(wb, e.id, &format!("={n}"));
                        out(wb, e.id, "info move D4 visits 3 winrate 0.5 scoreLead 0 prior 0.1 order 0 pv D4 rootInfo visits 3 winrate 0.5 scoreLead 0");
                    }
                    Probe::Reject => {
                        out(
                            wb,
                            e.id,
                            &format!(
                                "?{n} Could not parse analyze arguments or arguments out of range: '{PROBE_ARGS}'"
                            ),
                        );
                        out(wb, e.id, "");
                    }
                    Probe::Fail => {
                        out(wb, e.id, &format!("?{n} out of memory"));
                        out(wb, e.id, "");
                    }
                },
                "kata-analyze" => {
                    out(wb, e.id, &format!("={n}"));
                    if let Some(info) = info {
                        out(wb, e.id, info);
                    }
                }
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

fn profile(name: &str) -> EngineProfile {
    EngineProfile {
        name: name.into(),
        command: format!("/opt/{name} gtp"),
        ..EngineProfile::default()
    }
}

/// Engines `names` configured, the first started and served with `probe`; no automatic quick
/// analysis takes the engine when a record opens.
fn with_engines(wb: &mut Workbench, world: &World, names: &[&str], probe: Probe) -> Engine {
    wb.set_auto_quick_analyze(false).unwrap();
    wb.save_engine_settings(names.iter().map(|n| profile(n)).collect(), EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let e = Engine {
        id: world.engine(&format!("/opt/{}", names[0])),
        probe,
    };
    serve(wb, world, &e, None);
    e
}

fn open(wb: &mut Workbench, text: &str) {
    let t = wb.begin_open(OpenOrigin::File {
        path: "/x.sgf".into(),
        source: OpenSource::Drop,
    });
    assert!(wb.complete_open(t, Ok((RecordFormat::Sgf, sgf::read(text).unwrap()))));
}

fn p(x: u16, y: u16) -> Point {
    Point { x, y }
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

// ------------------------------------------------------------------------ focus capability

#[test]
fn every_engine_start_probes_focus_whatever_the_pause() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let a = with_engines(&mut wb, &world, &["a", "b", "c"], Probe::Accept);
    assert_eq!(wb.view(false).focus.capability, FocusCapability::Supported);

    // A paused start of another engine is probed too (Java skipped it after a switch), and the
    // probe neither resumes analysis nor is analysis.
    wb.toggle_analysis().unwrap();
    serve(&mut wb, &world, &a, None);
    wb.switch_engine(1).unwrap();
    let b = Engine {
        id: world.engine("/opt/b"),
        probe: Probe::Reject,
    };
    let sent = serve(&mut wb, &world, &b, None);
    assert!(sent.iter().any(|c| c.ends_with(PROBE_ARGS)), "{sent:?}");
    assert!(analyses(&sent).is_empty(), "{sent:?}");
    let v = wb.view(false);
    assert!(v.engine.paused);
    assert_eq!(v.focus.capability, FocusCapability::Unsupported);
    assert!(!v.focus.available);

    wb.switch_engine(2).unwrap();
    let c = Engine {
        id: world.engine("/opt/c"),
        probe: Probe::Fail,
    };
    serve(&mut wb, &world, &c, None);
    assert_eq!(wb.view(false).focus.capability, FocusCapability::Failed);

    // Each instance keeps its own answer.
    wb.switch_engine(0).unwrap();
    serve(&mut wb, &world, &a, None);
    assert_eq!(wb.view(false).focus.capability, FocusCapability::Supported);
}

#[test]
fn a_replaced_engine_answering_its_probe_late_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_engine_settings(vec![profile("a"), profile("b")], EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let a = Engine {
        id: world.engine("/opt/a"),
        probe: Probe::Accept,
    };
    // `a` has not answered anything when the user switches to `b`.
    wb.switch_engine(1).unwrap();
    let b = Engine {
        id: world.engine("/opt/b"),
        probe: Probe::Reject,
    };
    serve(&mut wb, &world, &b, None);
    serve(&mut wb, &world, &a, None);
    let v = wb.view(false);
    assert_eq!(v.engine.foreground.as_ref().unwrap().name, "b");
    assert_eq!(v.focus.capability, FocusCapability::Unsupported);
    assert!(!v.focus.available);
}

#[test]
fn the_probes_analysis_lines_are_not_results() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    with_engines(&mut wb, &world, &["a"], Probe::Accept);
    let v = wb.view(false);
    assert!(
        v.analysis.is_none() && v.history.is_empty(),
        "the probe's D4 line is no result"
    );
}

// ------------------------------------------------------------------------ tracked points

#[test]
fn tracked_points_are_added_removed_and_cleared_by_resending_the_same_request() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let e = with_engines(&mut wb, &world, &["a"], Probe::Accept);
    open(&mut wb, RECORD);
    serve(&mut wb, &world, &e, None);
    let plain = "kata-analyze B 10 rootInfo true";

    assert!(wb.tracking(TrackingCommand::Track { point: p(16, 15) }).unwrap()); // R4
    assert!(wb.tracking(TrackingCommand::Track { point: p(3, 15) }).unwrap()); // D4
    let pending = wb.view(false);
    assert_eq!(
        pending.focus.points.iter().map(|t| t.point).collect::<Vec<_>>(),
        [p(16, 15), p(3, 15)]
    );
    assert!(pending.focus.points.iter().all(|t| t.active));
    let sent = serve(&mut wb, &world, &e, None);
    assert_eq!(
        analyses(&sent).last().unwrap().as_str(),
        format!("{plain} focus D4,R4 0.5")
    );
    // The position stays as it is: no new board, moves or process; KataGo keeps its tree.
    assert!(sent.iter().all(|c| c.starts_with("kata-analyze")), "{sent:?}");

    assert!(wb.tracking(TrackingCommand::Untrack { point: p(3, 15) }).unwrap());
    assert!(
        wb.view(false).focus.cancellation_pending,
        "D4 is still searched until the new request"
    );
    let sent = serve(&mut wb, &world, &e, None);
    assert_eq!(sent, [format!("{plain} focus R4 0.5")]);
    assert!(!wb.view(false).focus.cancellation_pending);

    assert!(wb.tracking(TrackingCommand::Clear).unwrap());
    assert_eq!(serve(&mut wb, &world, &e, None), [plain]);
    assert!(wb.view(false).focus.points.is_empty());
}

#[test]
fn a_tracked_point_stops_at_its_target_or_after_eight_seconds_without_progress() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let e = with_engines(&mut wb, &world, &["a"], Probe::Accept);
    wb.set_tracking_settings(TrackingSettings {
        max_visits: 100,
        ..TrackingSettings::default()
    })
    .unwrap();
    open(&mut wb, RECORD);
    serve(&mut wb, &world, &e, None);
    wb.tracking(TrackingCommand::Track { point: p(3, 15) }).unwrap(); // D4
    wb.tracking(TrackingCommand::Track { point: p(16, 15) }).unwrap(); // R4
    let line = info(&[("D4", 120), ("R4", 10)]);
    let sent = serve(&mut wb, &world, &e, Some(&line));
    assert_eq!(
        analyses(&sent).last().unwrap().as_str(),
        "kata-analyze B 10 rootInfo true focus R4 0.5",
        "{sent:?}"
    );
    let v = wb.view(false);
    let d4 = v.focus.points.iter().find(|t| t.point == p(3, 15)).unwrap();
    assert!(!d4.active && d4.visits == 120, "D4 reached the target");
    assert!(d4.rank.is_some(), "its quality against the best candidate");

    // R4 makes no progress for 8 seconds.
    assert!(wb.engine_tick(Instant::now() + Duration::from_secs(9)));
    assert_eq!(serve(&mut wb, &world, &e, None), ["kata-analyze B 10 rootInfo true"]);
    let v = wb.view(false);
    assert_eq!(v.focus.points.len(), 2, "both stay tracked");
    assert!(v.focus.points.iter().all(|t| !t.active));
}

#[test]
fn tracking_counts_the_current_search_not_a_stronger_stored_result() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let e = with_engines(&mut wb, &world, &["a"], Probe::Accept);
    wb.set_tracking_settings(TrackingSettings {
        max_visits: 500,
        ..TrackingSettings::default()
    })
    .unwrap();
    open(&mut wb, RECORD);
    serve(&mut wb, &world, &e, Some(&info(&[("D4", 1000), ("R4", 900)])));
    // Back on the position later, the new search is weaker than the stored result.
    wb.navigate(Navigation::Forward(1));
    serve(&mut wb, &world, &e, None);
    wb.navigate(Navigation::Back(1));
    serve(&mut wb, &world, &e, Some(&info(&[("R4", 40), ("D4", 30)])));
    assert_eq!(
        wb.view(false).analysis.unwrap().visits,
        1900,
        "outside tracking the stronger result stays"
    );

    assert!(wb.tracking(TrackingCommand::Track { point: p(3, 15) }).unwrap()); // D4
    let d4 = wb.view(false).focus.points[0].clone();
    assert!(
        d4.active && d4.visits == 30,
        "the current search's D4, not the stored 1000: {d4:?}"
    );
    let sent = serve(&mut wb, &world, &e, Some(&info(&[("D4", 300), ("R4", 50)])));
    assert_eq!(analyses(&sent), ["kata-analyze B 10 rootInfo true focus D4 0.5"]);
    let v = wb.view(false);
    assert!(v.focus.points[0].active && v.focus.points[0].visits == 300);
    assert_eq!(
        v.analysis.unwrap().visits,
        350,
        "the tracked search's own result is shown"
    );
}

#[test]
fn changing_tracked_points_keeps_the_analysis_time_limit() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let e = with_engines(&mut wb, &world, &["a"], Probe::Accept);
    let mut settings = wb.config().engine.analysis.clone();
    settings.limit_time = true;
    settings.max_analyze_time_seconds = 1;
    wb.set_analysis_settings(settings).unwrap();
    open(&mut wb, RECORD);
    serve(&mut wb, &world, &e, Some(&info(&[("D4", 10)])));
    std::thread::sleep(Duration::from_millis(600));
    wb.tracking(TrackingCommand::Track { point: p(16, 15) }).unwrap(); // R4
    let sent = serve(&mut wb, &world, &e, None);
    assert_eq!(analyses(&sent), ["kata-analyze B 10 rootInfo true focus R4 0.5"]);
    std::thread::sleep(Duration::from_millis(600));
    out(&mut wb, e.id, &info(&[("D4", 20), ("R4", 10)]));
    assert!(
        wb.view(false).engine.stopped_by_limit,
        "the second since the position's analysis began is up; the focus request did not restart it"
    );
}

#[test]
fn pausing_ends_the_extra_search_and_tracking_never_resumes_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let e = with_engines(&mut wb, &world, &["a"], Probe::Accept);
    open(&mut wb, RECORD);
    serve(&mut wb, &world, &e, None);
    wb.tracking(TrackingCommand::Track { point: p(3, 15) }).unwrap();
    serve(&mut wb, &world, &e, None);

    wb.toggle_analysis().unwrap();
    assert_eq!(serve(&mut wb, &world, &e, None), ["stop"]);
    let v = wb.view(false);
    assert_eq!(v.focus.points.len(), 1);
    assert!(!v.focus.points[0].active && !v.focus.cancellation_pending);

    assert!(wb.tracking(TrackingCommand::Track { point: p(16, 15) }).unwrap());
    assert!(serve(&mut wb, &world, &e, None).is_empty(), "no request while paused");
    assert!(wb.view(false).engine.paused);

    wb.toggle_analysis().unwrap();
    assert_eq!(
        serve(&mut wb, &world, &e, None),
        ["kata-analyze B 10 rootInfo true"],
        "resuming adds no focus"
    );
}

#[test]
fn tracking_belongs_to_one_position_engine_and_target() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let e = with_engines(&mut wb, &world, &["a"], Probe::Accept);
    open(&mut wb, RECORD);
    serve(&mut wb, &world, &e, None);

    wb.tracking(TrackingCommand::Track { point: p(3, 15) }).unwrap();
    serve(&mut wb, &world, &e, None);
    wb.navigate(Navigation::Forward(1));
    let sent = serve(&mut wb, &world, &e, None);
    assert!(analyses(&sent).iter().all(|c| !c.contains("focus")), "{sent:?}");
    wb.navigate(Navigation::Back(1));
    assert!(
        wb.view(false).focus.points.is_empty(),
        "coming back does not bring it back"
    );
    serve(&mut wb, &world, &e, None);

    wb.tracking(TrackingCommand::Track { point: p(3, 15) }).unwrap();
    serve(&mut wb, &world, &e, None);
    wb.set_tracking_settings(TrackingSettings {
        max_visits: 900,
        ..TrackingSettings::default()
    })
    .unwrap();
    assert!(wb.view(false).focus.points.is_empty(), "another target ends it");
    serve(&mut wb, &world, &e, None);

    wb.tracking(TrackingCommand::Track { point: p(3, 15) }).unwrap();
    serve(&mut wb, &world, &e, None);
    wb.restart_engine().unwrap();
    let restarted = Engine {
        id: world.engine("/opt/a"),
        probe: Probe::Accept,
    };
    assert_ne!(restarted.id, e.id);
    let sent = serve(&mut wb, &world, &restarted, None);
    assert!(
        sent.iter().any(|c| c.ends_with(PROBE_ARGS)),
        "the new process is probed: {sent:?}"
    );
    assert!(analyses(&sent).iter().all(|c| !c.contains("focus")), "{sent:?}");
    assert!(wb.view(false).focus.points.is_empty());
}

#[test]
fn an_engine_without_focus_explains_and_keeps_analysing() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let e = with_engines(&mut wb, &world, &["a"], Probe::Reject);
    open(&mut wb, RECORD);
    let sent = serve(&mut wb, &world, &e, None);
    assert_eq!(analyses(&sent), ["kata-analyze B 10 rootInfo true"]);
    assert!(!wb.tracking(TrackingCommand::Track { point: p(3, 15) }).unwrap());
    let v = wb.view(false);
    assert!(!v.focus.available && v.focus.points.is_empty());
    assert_eq!(v.focus.capability, FocusCapability::Unsupported);
    assert!(
        serve(&mut wb, &world, &e, None).is_empty(),
        "the ordinary analysis goes on"
    );
}

// ------------------------------------------------------------------------ 强制计算区域

#[test]
fn a_selected_region_restricts_both_colours_and_lets_live_results_replace_stored_ones() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let e = with_engines(&mut wb, &world, &["a"], Probe::Accept);
    open(&mut wb, RECORD);
    wb.navigate(Navigation::Forward(1));
    serve(&mut wb, &world, &e, Some(&info(&[("K10", 1000)])));
    assert_eq!(wb.view(false).analysis.unwrap().visits, 1000);
    let revision = wb.view(false).document.revision;

    let region = RegionCommand::Select {
        allow: true,
        from: p(3, 3),
        to: p(4, 4),
    };
    assert!(wb.region(region).unwrap());
    // A line of the old, unrestricted stream still arrives before the engine reads the new
    // request; it searched without the region and does not count.
    out(&mut wb, e.id, &info(&[("K10", 2000)]));
    let sent = serve(&mut wb, &world, &e, Some(&info(&[("D16", 50)])));
    assert_eq!(
        analyses(&sent),
        ["kata-analyze W 10 allow b D16,D15,E16,E15 999 allow w D16,D15,E16,E15 999 rootInfo true"]
    );
    let v = wb.view(false);
    assert_eq!(
        v.analysis.unwrap().visits,
        50,
        "the region's result replaces the stored one"
    );
    assert_eq!(v.region.points, [p(3, 3), p(3, 4), p(4, 3), p(4, 4)]);
    assert_eq!(v.document.revision, revision, "a region never changes the record");

    // Kept across positions.
    wb.navigate(Navigation::Forward(1));
    let sent = serve(&mut wb, &world, &e, None);
    assert!(analyses(&sent)[0].contains(" allow b D16,D15,E16,E15 999 "), "{sent:?}");

    assert!(wb.region(RegionCommand::Clear).unwrap());
    let sent = serve(&mut wb, &world, &e, None);
    assert_eq!(analyses(&sent), ["kata-analyze B 10 rootInfo true"]);
    assert!(wb.view(false).region.points.is_empty());
}

#[test]
fn a_right_click_region_lasts_for_its_request_unless_kept() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let e = with_engines(&mut wb, &world, &["a"], Probe::Accept);
    open(&mut wb, RECORD);
    serve(&mut wb, &world, &e, None);

    wb.region(RegionCommand::AllowPoint { point: p(3, 15) }).unwrap();
    let sent = serve(&mut wb, &world, &e, None);
    assert_eq!(
        analyses(&sent),
        ["kata-analyze B 10 allow b D4 1 allow w D4 1 rootInfo true"]
    );
    wb.navigate(Navigation::Forward(1));
    let sent = serve(&mut wb, &world, &e, None);
    assert_eq!(analyses(&sent), ["kata-analyze W 10 rootInfo true"]);
    assert!(wb.view(false).region.points.is_empty());

    // 持续分析/不分析: the next requests keep it, with the configured depth.
    wb.region(RegionCommand::ToggleKeepForcing).unwrap();
    assert!(wb.view(false).region.keep_forcing);
    wb.region(RegionCommand::AllowPoint { point: p(3, 15) }).unwrap();
    wb.region(RegionCommand::AddAllowPoint { point: p(16, 15) }).unwrap();
    let sent = serve(&mut wb, &world, &e, None);
    assert!(
        analyses(&sent).last().unwrap().contains(" allow b D4,R4 1 "),
        "{sent:?}"
    );
    wb.navigate(Navigation::Back(1));
    let sent = serve(&mut wb, &world, &e, None);
    assert!(analyses(&sent)[0].contains(" allow b D4,R4 999 "), "{sent:?}");

    // Tracking respects a kept region.
    assert!(!wb.tracking(TrackingCommand::Track { point: p(10, 10) }).unwrap());
    assert!(wb.tracking(TrackingCommand::Track { point: p(3, 15) }).unwrap());
}

#[test]
fn choosing_a_region_resumes_a_paused_analysis_and_the_toolbar_clear_does_not() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let e = with_engines(&mut wb, &world, &["a"], Probe::Accept);
    open(&mut wb, RECORD);
    serve(&mut wb, &world, &e, None);
    wb.set_region_settings(RegionSettings {
        avoid_moves: 3,
        ..RegionSettings::default()
    })
    .unwrap();

    wb.toggle_analysis().unwrap();
    serve(&mut wb, &world, &e, None);
    wb.region(RegionCommand::AvoidPoint { point: p(3, 15) }).unwrap();
    let sent = serve(&mut wb, &world, &e, None);
    assert_eq!(
        analyses(&sent),
        ["kata-analyze B 10 avoid b D4 3 avoid w D4 3 rootInfo true"]
    );
    assert!(!wb.view(false).engine.paused);

    wb.toggle_analysis().unwrap();
    serve(&mut wb, &world, &e, None);
    wb.region(RegionCommand::Clear).unwrap();
    assert!(serve(&mut wb, &world, &e, None).is_empty());
    assert!(wb.view(false).engine.paused);

    wb.region(RegionCommand::ClearForcing).unwrap();
    assert_eq!(
        analyses(&serve(&mut wb, &world, &e, None)),
        ["kata-analyze B 10 rootInfo true"]
    );
}

#[test]
fn a_new_depth_asks_the_region_again() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let e = with_engines(&mut wb, &world, &["a"], Probe::Accept);
    open(&mut wb, RECORD);
    serve(&mut wb, &world, &e, None);
    wb.region(RegionCommand::Select {
        allow: true,
        from: p(3, 15),
        to: p(3, 15),
    })
    .unwrap();
    serve(&mut wb, &world, &e, None);
    wb.set_region_settings(RegionSettings {
        allow_moves: 1,
        ..RegionSettings::default()
    })
    .unwrap();
    assert_eq!(
        analyses(&serve(&mut wb, &world, &e, None)),
        ["kata-analyze B 10 allow b D4 1 allow w D4 1 rootInfo true"]
    );
}

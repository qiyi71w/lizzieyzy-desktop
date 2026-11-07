//! 启动Lizzie时预加载引擎 through the public `Workbench` operations with controlled processes:
//! when the warm-up starts the analysis process at once, waits for the foreground engine or a
//! record's engine sync, or gives up (Java `currentQuickAnalysisWarmupAction`), and when the
//! foreground engine's search or a placed stone ends the idle process (`onMainEnginePonder`).

use std::sync::{Arc, Mutex};

use lizzie_core::api::{EditCommand, OpenSource};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::engine::ssh::SshSettings;
use lizzie_core::formats::{sgf, ReadError};
use lizzie_core::record::{Point, RecordFormat};
use lizzie_core::resources::ManagedProfile;
use lizzie_core::settings::{AnalysisEngineSettings, AutoAnalysisSettings, Storage};
use lizzie_core::workbench::{OpenOrigin, Workbench};

struct Proc {
    id: InstanceId,
    spec: LaunchSpec,
    sent: Vec<String>,
    terminated: bool,
}

/// The processes started, and the programs that cannot start or GTP commands that are refused.
#[derive(Clone, Default)]
struct World(Arc<Mutex<Vec<Proc>>>, Arc<Mutex<Vec<String>>>);

impl World {
    fn refuse(&self, what: &str) {
        self.1.lock().unwrap().push(what.into());
    }
    fn refused(&self, what: &str) -> bool {
        self.1.lock().unwrap().iter().any(|r| r == what)
    }
    fn take(&self, id: InstanceId) -> Vec<String> {
        let mut w = self.0.lock().unwrap();
        w.iter_mut()
            .find(|p| p.id == id)
            .map(|p| std::mem::take(&mut p.sent))
            .unwrap_or_default()
    }
    /// Processes started other than the foreground engines (`program` is theirs).
    fn others(&self, program: &str) -> Vec<InstanceId> {
        let w = self.0.lock().unwrap();
        w.iter().filter(|p| p.spec.program != program).map(|p| p.id).collect()
    }
    fn latest(&self, program: &str) -> InstanceId {
        let w = self.0.lock().unwrap();
        w.iter().rev().find(|p| p.spec.program == program).unwrap().id
    }
    fn args(&self, id: InstanceId) -> Vec<String> {
        let w = self.0.lock().unwrap();
        w.iter().find(|p| p.id == id).unwrap().spec.args.clone()
    }
    fn terminated(&self, id: InstanceId) -> bool {
        self.0.lock().unwrap().iter().any(|p| p.id == id && p.terminated)
    }
}

struct FakeIo(World, InstanceId);

impl EngineIo for FakeIo {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        let mut w = self.0 .0.lock().unwrap();
        w.iter_mut().find(|p| p.id == self.1).unwrap().sent.push(line.to_string());
        Ok(())
    }
    fn terminate(self: Box<Self>) {
        let mut w = self.0 .0.lock().unwrap();
        w.iter_mut().find(|p| p.id == self.1).unwrap().terminated = true;
    }
}

struct FakeLauncher(World);

impl Launcher for FakeLauncher {
    fn launch(&self, id: InstanceId, spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        if self.0.refused(&spec.program) {
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no such program"));
        }
        self.0 .0.lock().unwrap().push(Proc {
            id,
            spec: spec.clone(),
            sent: Vec::new(),
            terminated: false,
        });
        Ok(Box::new(FakeIo(self.0.clone(), id)))
    }
}

const KATA: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nset_position\nkata-analyze\nkata-set-rules\nkata-get-rules\nstop";
/// An engine without an analysis command: ready, it never searches.
const PLAIN: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\ngenmove";
const RULES: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;
const ANALYSIS: &str = "/opt/katago analysis -config /a.cfg";
const ENGINE: &str = "/opt/engine";

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

/// 精析设置 with 启动Lizzie时预加载引擎 `preload`.
fn configure(wb: &mut Workbench, preload: bool, reuse: bool) {
    let settings = AnalysisEngineSettings {
        command: ANALYSIS.into(),
        preload,
        reuse_current_engine: reuse,
        ..AnalysisEngineSettings::default()
    };
    wb.save_analysis_engine_settings(settings, "", true).unwrap().unwrap();
}

/// A foreground engine profile; `asset` marks it as a managed package.
fn profile(name: &str, asset: Option<&str>) -> EngineProfile {
    let command = format!("{ENGINE} gtp -name {name}");
    EngineProfile {
        name: name.into(),
        managed: asset.map(|a| ManagedProfile {
            command: command.clone(),
            asset_id: a.into(),
            ..ManagedProfile::default()
        }),
        command,
        ..EngineProfile::default()
    }
}

/// Starts `profiles[0]` as the foreground engine, not answered yet.
fn start(wb: &mut Workbench, world: &World, profiles: Vec<EngineProfile>) -> InstanceId {
    wb.save_engine_settings(profiles, EngineStartup::Default).unwrap();
    wb.start_engines();
    world.latest(ENGINE)
}

fn out(wb: &mut Workbench, id: InstanceId, line: &str) {
    wb.engine_input(id, EngineInput::Stdout(line.to_string()));
}

/// Answers every outstanding command as a KataGo (`commands` = [`KATA`]) or a plain engine.
fn serve(wb: &mut Workbench, world: &World, id: InstanceId, commands: &str) -> Vec<String> {
    let name = if commands == KATA { "KataGo" } else { "GNU Go" };
    let mut done = Vec::new();
    loop {
        let pending = world.take(id);
        if pending.is_empty() {
            return done;
        }
        for line in pending {
            out(wb, id, "");
            let (n, cmd) = line.split_once(' ').unwrap();
            let word = cmd.split(' ').next().unwrap();
            if world.refused(word) {
                out(wb, id, &format!("?{n} refused"));
                out(wb, id, "");
                done.push(cmd.to_string());
                continue;
            }
            let reply = match word {
                "name" => name,
                "version" => "1.18.2",
                "list_commands" => commands,
                "kata-get-rules" => RULES,
                _ => "",
            };
            out(wb, id, &format!("={n} {reply}"));
            out(wb, id, "");
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

#[test]
fn without_a_foreground_engine_the_preload_starts_the_configured_analysis_process() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    assert!(world.others(ENGINE).is_empty(), "saving the setting starts nothing");
    wb.preload_analysis_engine();
    let [worker] = world.others(ENGINE)[..] else { panic!("one analysis process") };
    assert_eq!(world.args(worker)[..3], ["analysis", "-config", "/a.cfg"]);
    // A second request finds the idle process (`isAnalysisEngineReusable`).
    wb.preload_analysis_engine();
    assert_eq!(world.others(ENGINE), [worker]);
    wb.end_engines();
    assert!(world.terminated(worker), "closing the application ends it");
}

#[test]
fn with_the_setting_off_nothing_is_preloaded() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, false, false);
    wb.preload_analysis_engine();
    assert!(world.others(ENGINE).is_empty());
}

#[test]
fn an_ordinary_foreground_engine_is_not_waited_for() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    start(&mut wb, &world, vec![profile("Local", None)]);
    wb.preload_analysis_engine();
    let [worker] = world.others(ENGINE)[..] else { panic!("started while the engine loads") };
    wb.end_engines();
    assert!(world.terminated(worker));
}

#[test]
fn a_ready_foreground_engine_alone_preloads_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let fg = start(&mut wb, &world, vec![profile("Local", None)]);
    serve(&mut wb, &world, fg, PLAIN);
    assert!(world.others(ENGINE).is_empty());
}

#[test]
fn a_managed_gpu_foreground_is_waited_for_and_the_process_starts_once_it_is_ready() {
    for asset in ["windows-opencl", "windows-nvidia", "windows-tensorrt"] {
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let mut wb = workbench(dir.path(), &world);
        configure(&mut wb, true, false);
        let fg = start(&mut wb, &world, vec![profile("Managed", Some(asset))]);
        wb.preload_analysis_engine();
        assert!(world.others(ENGINE).is_empty(), "{asset}: waits while the engine loads");
        serve(&mut wb, &world, fg, PLAIN);
        assert_eq!(world.others(ENGINE).len(), 1, "{asset}: a dedicated process once ready");
    }
}

#[test]
fn a_waiting_preload_ends_when_the_foreground_starts_searching_first() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let fg = start(&mut wb, &world, vec![profile("Managed", Some("windows-nvidia"))]);
    wb.preload_analysis_engine();
    // Ready and searching at once: `onMainEnginePonder` stops the warm-up.
    assert!(serve(&mut wb, &world, fg, KATA).iter().any(|c| c.starts_with("kata-analyze")));
    serve(&mut wb, &world, fg, KATA);
    assert!(world.others(ENGINE).is_empty());
}

#[test]
fn reusing_the_foreground_engine_waits_for_it_and_starts_no_process() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, true);
    wb.preload_analysis_engine();
    let fg = start(&mut wb, &world, vec![profile("Local", None)]);
    serve(&mut wb, &world, fg, PLAIN);
    assert!(world.others(ENGINE).is_empty());
}

#[test]
fn a_failed_foreground_it_depends_on_stops_the_preload() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let fg = start(&mut wb, &world, vec![profile("Managed", Some("windows-opencl"))]);
    wb.preload_analysis_engine();
    wb.engine_input(fg, EngineInput::Exited(Some(1)));
    assert!(world.others(ENGINE).is_empty());
    // Asking again with the failed engine still in its seat stops at once.
    wb.preload_analysis_engine();
    assert!(world.others(ENGINE).is_empty());
}

#[test]
fn pause_unchecking_the_setting_or_an_engine_switch_stop_a_waiting_preload() {
    type Interrupt = fn(&mut Workbench);
    let interrupts: [(&str, Interrupt); 3] = [
        ("pause", |wb| assert!(wb.toggle_analysis().unwrap())),
        ("uncheck", |wb| configure(wb, false, false)),
        ("switch", |wb| assert!(wb.switch_engine(1).unwrap())),
    ];
    for (name, interrupt) in interrupts {
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let mut wb = workbench(dir.path(), &world);
        configure(&mut wb, true, false);
        let fg = start(
            &mut wb,
            &world,
            vec![profile("Managed", Some("windows-opencl")), profile("Other", Some("windows-opencl"))],
        );
        wb.preload_analysis_engine();
        interrupt(&mut wb);
        if name == "pause" {
            // Resuming before the engine is ready does not bring the warm-up back.
            assert!(wb.toggle_analysis().unwrap());
        }
        serve(&mut wb, &world, fg, PLAIN);
        let new = world.latest(ENGINE);
        serve(&mut wb, &world, new, PLAIN);
        assert!(world.others(ENGINE).is_empty(), "{name}");
    }
}

#[test]
fn a_paused_analysis_gives_up_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let fg = start(&mut wb, &world, vec![profile("Local", None)]);
    serve(&mut wb, &world, fg, PLAIN);
    assert!(wb.toggle_analysis().unwrap());
    wb.preload_analysis_engine();
    assert!(wb.toggle_analysis().unwrap());
    assert!(world.others(ENGINE).is_empty());
}

#[test]
fn a_record_engine_sync_is_waited_for_and_its_end_stops_the_preload() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let fg = start(&mut wb, &world, vec![profile("Local", None)]);
    serve(&mut wb, &world, fg, KATA);
    open(&mut wb, "(;SZ[19]RU[Japanese];B[pd];W[dd])");
    wb.preload_analysis_engine();
    assert!(world.others(ENGINE).is_empty(), "waits for the declared rules");
    serve(&mut wb, &world, fg, KATA);
    assert!(world.others(ENGINE).is_empty(), "`onSynchronized` stops the warm-up");
}

#[test]
fn a_record_without_a_sync_to_wait_for_starts_the_preload() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let fg = start(&mut wb, &world, vec![profile("Local", None)]);
    serve(&mut wb, &world, fg, KATA);
    open(&mut wb, "(;SZ[19];B[pd];W[dd])");
    wb.preload_analysis_engine();
    assert_eq!(world.others(ENGINE).len(), 1);
}

#[test]
fn the_foreground_search_ends_the_idle_local_process() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    wb.preload_analysis_engine();
    let [worker] = world.others(ENGINE)[..] else { panic!("one analysis process") };
    let fg = start(&mut wb, &world, vec![profile("Local", None)]);
    serve(&mut wb, &world, fg, PLAIN);
    assert!(!world.terminated(worker), "a ready engine that does not search keeps it");
    let fg = start(&mut wb, &world, vec![profile("Kata", None)]);
    serve(&mut wb, &world, fg, KATA);
    serve(&mut wb, &world, fg, KATA);
    assert!(world.terminated(worker), "RELEASE_IDLE_SECONDARY");
}

#[test]
fn a_placed_stone_ends_the_idle_local_process() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    wb.preload_analysis_engine();
    let [worker] = world.others(ENGINE)[..] else { panic!("one analysis process") };
    wb.edit(EditCommand::Play { point: Point { x: 3, y: 3 }, color: None }).unwrap();
    assert!(world.terminated(worker), "`Board.place` → `onMainEnginePonder`");
}

#[test]
fn an_idle_process_on_the_ssh_host_outlives_the_foreground_search() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let settings = AnalysisEngineSettings {
        command: "~/katago analysis -config ~/a.cfg".into(),
        preload: true,
        use_ssh: true,
        ssh: SshSettings {
            host: "gpu".into(),
            port: "22".into(),
            user: "go".into(),
            ..SshSettings::default()
        },
        ..AnalysisEngineSettings::default()
    };
    wb.save_analysis_engine_settings(settings, "", true).unwrap().unwrap();
    wb.preload_analysis_engine();
    let [worker] = world.others(ENGINE)[..] else { panic!("one remote process") };
    wb.edit(EditCommand::Play { point: Point { x: 3, y: 3 }, color: None }).unwrap();
    assert!(!world.terminated(worker), "not a local dedicated process");
    wb.end_engines();
    assert!(world.terminated(worker), "closing the application ends it");
}

#[test]
fn a_managed_foreground_that_cannot_even_launch_stops_the_preload() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    world.refuse(ENGINE);
    let profiles = vec![profile("Managed", Some("windows-nvidia"))];
    wb.save_engine_settings(profiles, EngineStartup::Default).unwrap();
    wb.start_engines();
    wb.preload_analysis_engine();
    assert!(world.others(ENGINE).is_empty(), "Java's primary `isDownWithError` stops it");
}

#[test]
fn unticking_the_setting_ends_a_waiting_preload_even_if_ticked_again() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let fg = start(&mut wb, &world, vec![profile("Managed", Some("windows-opencl"))]);
    wb.preload_analysis_engine();
    configure(&mut wb, false, false);
    configure(&mut wb, true, false);
    serve(&mut wb, &world, fg, PLAIN);
    assert!(world.others(ENGINE).is_empty(), "saving never asks for a preload");
    wb.preload_analysis_engine();
    assert_eq!(world.others(ENGINE).len(), 1, "a new request still works");
}

/// Java loads a start-up record before the warm-up decides; an SGF record then always gets an
/// engine sync (completing at once without an engine), whose end stops the warm-up. A GIB record
/// gets none (`loadFile`, `scheduleEngineSyncAndResumeAfterKifuLoad`, `onSynchronized`).
#[test]
fn a_record_still_being_read_is_waited_for_and_an_sgf_record_ends_the_preload() {
    let record = |format| Ok((format, sgf::read("(;SZ[19];B[pd];W[dd])").unwrap()));
    let origin = || OpenOrigin::File { path: "/x.sgf".into(), source: OpenSource::CommandLine };
    // With a foreground engine.
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let fg = start(&mut wb, &world, vec![profile("Local", None)]);
    serve(&mut wb, &world, fg, PLAIN);
    let t = wb.begin_open(origin());
    wb.preload_analysis_engine();
    assert!(world.others(ENGINE).is_empty(), "waits while the record is read");
    assert!(wb.complete_open(t, record(RecordFormat::Sgf)));
    serve(&mut wb, &world, fg, PLAIN);
    assert!(world.others(ENGINE).is_empty(), "the engine sync of the record ends the warm-up");

    // Without an engine too; a GIB record and a failed read decide at once.
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let t = wb.begin_open(origin());
    wb.preload_analysis_engine();
    assert!(world.others(ENGINE).is_empty());
    assert!(wb.complete_open(t, record(RecordFormat::Sgf)));
    assert!(world.others(ENGINE).is_empty(), "a sync without an engine completes and ends it");

    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let t = wb.begin_open(origin());
    wb.preload_analysis_engine();
    assert!(wb.complete_open(t, record(RecordFormat::Gib)));
    assert_eq!(world.others(ENGINE).len(), 1, "a GIB record has no engine sync");

    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let fg = start(&mut wb, &world, vec![profile("Local", None)]);
    serve(&mut wb, &world, fg, PLAIN);
    let t = wb.begin_open(origin());
    wb.preload_analysis_engine();
    assert!(wb.complete_open(t, Err(ReadError::Invalid("x".into()))));
    assert_eq!(world.others(ENGINE).len(), 1, "no record, no sync");
}

#[test]
fn a_failed_rules_sync_also_ends_a_preload_waiting_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let fg = start(&mut wb, &world, vec![profile("Local", None)]);
    serve(&mut wb, &world, fg, KATA);
    open(&mut wb, "(;SZ[19]RU[Japanese];B[pd];W[dd])");
    wb.preload_analysis_engine();
    assert!(world.others(ENGINE).is_empty(), "waits for the declared rules");
    world.refuse("kata-set-rules");
    serve(&mut wb, &world, fg, KATA);
    assert!(world.others(ENGINE).is_empty(), "a failed sync stops the warm-up as well");
}

#[test]
fn automatic_analysis_running_gives_up_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    configure(&mut wb, true, false);
    let fg = start(&mut wb, &world, vec![profile("Local", None)]);
    serve(&mut wb, &world, fg, KATA);
    open(&mut wb, "(;SZ[19];B[pd];W[dd])");
    serve(&mut wb, &world, fg, KATA);
    let settings = AutoAnalysisSettings { time_enabled: true, time_seconds: 2, auto_save: false, ..Default::default() };
    wb.start_auto_analysis(settings).unwrap().unwrap();
    wb.preload_analysis_engine();
    serve(&mut wb, &world, fg, KATA);
    wb.stop_auto_analysis();
    assert!(world.others(ENGINE).is_empty());
}

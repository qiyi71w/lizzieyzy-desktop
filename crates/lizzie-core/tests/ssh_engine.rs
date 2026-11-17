//! Remote (SSH) engines through the public `Workbench` operations with a controlled connection:
//! what is launched where, the SSH refresh interval, how each remote failure is reported, and
//! that a replaced connection's late events reach neither the new engine nor the position.

use std::sync::{Arc, Mutex};

use lizzie_core::api::{FlashOutcome, FlashTarget, OpenSource};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::engine::ssh::{RemoteFailure, SshSettings};
use lizzie_core::formats::sgf;
use lizzie_core::record::RecordFormat;
use lizzie_core::settings::{AnalysisEngineSettings, Storage};
use lizzie_core::workbench::{OpenOrigin, Workbench};

struct Proc {
    id: InstanceId,
    spec: LaunchSpec,
    sent: Vec<String>,
    terminated: bool,
}

#[derive(Clone, Default)]
struct World(Arc<Mutex<Vec<Proc>>>);

impl World {
    fn take(&self, id: InstanceId) -> Vec<String> {
        let mut w = self.0.lock().unwrap();
        w.iter_mut()
            .find(|p| p.id == id)
            .map(|p| std::mem::take(&mut p.sent))
            .unwrap_or_default()
    }
    fn spec(&self, id: InstanceId) -> LaunchSpec {
        self.0.lock().unwrap().iter().find(|p| p.id == id).unwrap().spec.clone()
    }
    fn last(&self) -> InstanceId {
        self.0.lock().unwrap().last().unwrap().id
    }
    fn count(&self) -> usize {
        self.0.lock().unwrap().len()
    }
    fn terminated(&self, id: InstanceId) -> bool {
        self.0.lock().unwrap().iter().any(|p| p.id == id && p.terminated)
    }
}

struct FakeIo(World, InstanceId);

impl EngineIo for FakeIo {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        let mut w = self.0 .0.lock().unwrap();
        w.iter_mut()
            .find(|p| p.id == self.1)
            .unwrap()
            .sent
            .push(line.to_string());
        Ok(())
    }
    fn terminate(self: Box<Self>) {
        let mut w = self.0 .0.lock().unwrap();
        w.iter_mut().find(|p| p.id == self.1).unwrap().terminated = true;
    }
}

/// Starts nothing: like the SSH connector, it accepts the launch and reports later.
struct FakeLauncher(World);

impl Launcher for FakeLauncher {
    fn launch(&self, id: InstanceId, spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
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
const RULES: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;

fn ssh(host: &str) -> SshSettings {
    SshSettings {
        host: host.into(),
        port: "2222".into(),
        user: "go".into(),
        ..SshSettings::default()
    }
}

fn remote(name: &str, host: &str) -> EngineProfile {
    EngineProfile {
        name: name.into(),
        command: "~/katago/katago gtp -model ~/m.bin.gz".into(),
        ssh: Some(ssh(host)),
        ..EngineProfile::default()
    }
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

fn out(wb: &mut Workbench, id: InstanceId, line: &str) {
    wb.engine_input(id, EngineInput::Stdout(line.to_string()));
}

/// Answers every outstanding command; analysis requests get only their header.
fn serve(wb: &mut Workbench, world: &World, id: InstanceId) -> Vec<String> {
    let mut done = Vec::new();
    loop {
        let pending = world.take(id);
        if pending.is_empty() {
            return done;
        }
        for line in pending {
            out(wb, id, "");
            let (n, cmd) = line.split_once(' ').unwrap();
            let reply = match cmd.split(' ').next().unwrap() {
                "name" => "KataGo",
                "version" => "1.16.4",
                "list_commands" => KATA,
                "kata-get-rules" => RULES,
                "kata-analyze" => {
                    out(wb, id, &format!("={n}"));
                    done.push(cmd.to_string());
                    continue;
                }
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
fn a_remote_engine_runs_its_command_on_the_host_as_written_and_refreshes_at_the_ssh_interval() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let mut analysis = wb.config().engine.analysis.clone();
    (analysis.interval_centisec, analysis.interval_centisec_ssh) = (10, 35);
    wb.set_analysis_settings(analysis).unwrap();
    wb.save_engine_settings(vec![remote("Remote", "gpu.example")], EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let id = world.last();
    let launch = world.spec(id).remote.expect("launched over SSH");
    assert_eq!(
        (launch.host.as_str(), launch.port, launch.user.as_str()),
        ("gpu.example", 2222, "go")
    );
    assert_eq!(
        launch.command, "~/katago/katago gtp -model ~/m.bin.gz",
        "no local path resolution"
    );
    assert_eq!(launch.key_file, None);
    let sent = serve(&mut wb, &world, id);
    assert!(
        sent.contains(&"kata-analyze B 35 rootInfo true".to_string()),
        "{sent:?}"
    );
    assert!(wb.view(false).engine.foreground.unwrap().ready);
}

#[test]
fn a_refused_login_during_start_up_is_reported_as_such_for_its_profile() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_engine_settings(vec![remote("Remote", "gpu.example")], EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let id = world.last();
    wb.engine_input(id, EngineInput::Remote(RemoteFailure::Authentication));
    let v = wb.view(false);
    let f = v.engine_failure.expect("failure shown");
    assert!(f.during_startup);
    assert_eq!(f.remote, Some(RemoteFailure::Authentication));
    assert_eq!(f.profile_id, wb.config().engine.engines[0].id);
    assert!(v.engine.foreground.is_none());
    assert!(world.terminated(id));
}

/// Both ends are reported apart, and either way the remote engine is connected and started again
/// (`auto-check-engine-alive`: `isLoaded && canCheckAlive && javaSSHClosed`).
#[test]
fn the_remote_process_ending_and_the_connection_closing_are_reported_apart_and_restarted() {
    for (end, expected) in [
        (
            RemoteFailure::Ended {
                status: Some(137),
                signal: None,
            },
            "remote command exited with code 137",
        ),
        (
            RemoteFailure::Lost {
                detail: "keepalive timed out".into(),
            },
            "remote connection lost: keepalive timed out",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let mut wb = workbench(dir.path(), &world);
        wb.save_engine_settings(vec![remote("Remote", "gpu.example")], EngineStartup::Default)
            .unwrap();
        wb.start_engines();
        let id = world.last();
        serve(&mut wb, &world, id);
        out(
            &mut wb,
            id,
            "info move Q16 visits 30 winrate 0.6 scoreLead 1.5 prior 0.2 order 0 pv Q16 rootInfo visits 30",
        );
        wb.engine_input(id, EngineInput::Remote(end.clone()));
        let v = wb.view(false);
        let f = v.engine_failure.expect("failure shown");
        assert!(!f.during_startup);
        assert_eq!((f.remote, f.message.as_str()), (Some(end), expected));
        let again = world.last();
        assert_ne!(again, id, "a new connection");
        assert_eq!(world.spec(again).remote.unwrap().host, "gpu.example");
        serve(&mut wb, &world, again);
        assert!(wb.view(false).engine.foreground.unwrap().ready);
    }
}

#[test]
fn a_replaced_connections_late_events_reach_neither_the_new_engine_nor_the_position() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_engine_settings(
        vec![remote("A", "a.example"), remote("B", "b.example")],
        EngineStartup::Default,
    )
    .unwrap();
    wb.start_engines();
    let a = world.last();
    serve(&mut wb, &world, a);
    open(&mut wb, "(;SZ[19];B[pd])");
    serve(&mut wb, &world, a);
    assert!(wb.switch_engine(1).unwrap());
    let b = world.last();
    assert_ne!(a, b);
    assert_eq!(world.spec(b).remote.unwrap().host, "b.example");
    serve(&mut wb, &world, b);
    wb.shutdown_engines(lizzie_core::api::EngineScope::Others);
    assert!(world.terminated(a));
    // A's connection still delivers what was in flight, then the end of its command.
    out(
        &mut wb,
        a,
        "info move T19 visits 999 winrate 0.99 scoreLead 50 prior 0.2 order 0 pv T19 rootInfo visits 999",
    );
    wb.engine_input(
        a,
        EngineInput::Remote(RemoteFailure::Ended {
            status: Some(0),
            signal: None,
        }),
    );
    wb.engine_input(
        a,
        EngineInput::Remote(RemoteFailure::Lost {
            detail: "closed".into(),
        }),
    );
    let v = wb.view(false);
    assert!(
        v.engine_failure.is_none(),
        "the old connection's end is not B's failure"
    );
    assert!(v.engine.foreground.unwrap().ready);
    assert!(v.analysis.is_none_or(|a| a.candidates.iter().all(|c| c.visits != 999)));
}

/// 精析设置's 远程引擎: the analysis process runs on its host; its failure ends the run.
#[test]
fn the_remote_analysis_engine_runs_flash_analysis_and_its_lost_connection_ends_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    let settings = AnalysisEngineSettings {
        command: "~/katago analysis -config ~/a.cfg -model ~/m.bin.gz".into(),
        reuse_current_engine: false,
        use_ssh: true,
        ssh: ssh("analysis.example"),
        ..AnalysisEngineSettings::default()
    };
    wb.save_analysis_engine_settings(settings, "", true).unwrap().unwrap();
    open(&mut wb, "(;SZ[19];B[pd];W[dd])");
    assert_eq!(wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(), Ok(true));
    let worker = world.last();
    let launch = world.spec(worker).remote.expect("analysis over SSH");
    assert_eq!(launch.host, "analysis.example");
    assert!(
        launch
            .command
            .starts_with("~/katago analysis -config ~/a.cfg -model ~/m.bin.gz"),
        "{}",
        launch.command
    );
    let queries = world.take(worker);
    assert!(
        !queries.is_empty(),
        "queries wait in the connection until the command runs"
    );

    wb.engine_input(
        worker,
        EngineInput::Remote(RemoteFailure::Lost { detail: "reset".into() }),
    );
    let v = wb.view(false);
    assert!(v.flash.is_none());
    assert!(matches!(v.flash_outcome, Some(FlashOutcome::EngineFailed { .. })));
    let f = v.engine_failure.expect("failure shown");
    assert_eq!(f.remote, Some(RemoteFailure::Lost { detail: "reset".into() }));
    assert!(
        f.profile_id.is_empty(),
        "the analysis engine is not a configured engine"
    );

    // Turning 远程引擎 off: the next run starts a local process instead of reusing anything.
    let mut settings = wb.config().analysis_engine.clone();
    settings.use_ssh = false;
    wb.save_analysis_engine_settings(settings, "", false).unwrap().unwrap();
    let before = world.count();
    assert_eq!(wb.redo_flash_analysis().unwrap(), Ok(true));
    assert_eq!(world.count(), before + 1);
    assert!(world.spec(world.last()).remote.is_none());
}

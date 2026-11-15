//! 引擎参数 window's 搜索线程数（本次运行） through the public `Workbench` operation with a controlled
//! KataGo: reading the process's `numSearchThreads`, applying a value, restoring the launch source
//! by restarting the engine, and the process identity the override belongs to.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lizzie_core::api::{
    CommandResult, KataEngineParamsEdit, KataParamEdit, SearchThreadsCommand, SearchThreadsStatus, ThreadRestoreSource,
};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher, ThreadOverride};
use lizzie_core::engine::tuning::{ThreadPolicy, ThreadSource};
use lizzie_core::settings::Storage;
use lizzie_core::workbench::Workbench;

#[derive(Default)]
struct Proc {
    sent: Vec<String>,
    terminated: bool,
}

#[derive(Clone, Default)]
struct World(Arc<Mutex<Vec<(InstanceId, Proc)>>>);

impl World {
    fn take(&self, id: InstanceId) -> Vec<String> {
        let mut w = self.0.lock().unwrap();
        w.iter_mut().find(|p| p.0 == id).map(|p| std::mem::take(&mut p.1.sent)).unwrap_or_default()
    }
    fn last(&self) -> InstanceId {
        self.0.lock().unwrap().last().unwrap().0
    }
    fn count(&self) -> usize {
        self.0.lock().unwrap().len()
    }
    fn terminated(&self, id: InstanceId) -> bool {
        self.0.lock().unwrap().iter().any(|p| p.0 == id && p.1.terminated)
    }
}

struct FakeIo {
    id: InstanceId,
    world: World,
}

impl EngineIo for FakeIo {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        let mut w = self.world.0.lock().unwrap();
        w.iter_mut().find(|p| p.0 == self.id).unwrap().1.sent.push(line.to_string());
        Ok(())
    }
    fn terminate(self: Box<Self>) {
        let mut w = self.world.0.lock().unwrap();
        w.iter_mut().find(|p| p.0 == self.id).unwrap().1.terminated = true;
    }
}

struct FakeLauncher(World);

impl Launcher for FakeLauncher {
    fn launch(&self, id: InstanceId, _spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        self.0 .0.lock().unwrap().push((id, Proc::default()));
        Ok(Box::new(FakeIo { id, world: self.0.clone() }))
    }
}

const LIST: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nset_position\nkata-analyze\nkata-set-rules\nkata-get-rules\nkata-set-param\nkata-get-param\nstop";
const RULES: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;

/// A KataGo process: its search threads (the configuration's value until a set changes it) and
/// whether it refuses a new value.
struct Katago {
    threads: u32,
    reject: bool,
}

impl Katago {
    fn new() -> Katago {
        Katago { threads: 2, reject: false }
    }
}

fn out(wb: &mut Workbench, id: InstanceId, line: &str) {
    wb.engine_input(id, EngineInput::Stdout(line.to_string()));
}

fn answer(wb: &mut Workbench, id: InstanceId, n: u64, text: &str) {
    out(wb, id, &format!("={n} {text}"));
    out(wb, id, "");
}

/// Answers everything `id` was sent, analysis requests with their header only. Returns the
/// commands (without numbers).
fn serve(wb: &mut Workbench, world: &World, id: InstanceId, engine: &mut Katago) -> Vec<String> {
    let mut done = Vec::new();
    loop {
        let pending = world.take(id);
        if pending.is_empty() {
            return done;
        }
        for line in pending {
            out(wb, id, "");
            let (n, cmd) = line.split_once(' ').unwrap();
            let n: u64 = n.parse().unwrap();
            let words: Vec<&str> = cmd.split(' ').collect();
            match (words[0], words.get(1).copied()) {
                ("name", _) => answer(wb, id, n, "KataGo"),
                ("version", _) => answer(wb, id, n, "1.16.4"),
                ("list_commands", _) => answer(wb, id, n, LIST),
                ("kata-analyze", _) => out(wb, id, &format!("={n}")),
                ("kata-get-param", Some("numSearchThreads")) => answer(wb, id, n, &engine.threads.to_string()),
                ("kata-get-param", _) => answer(wb, id, n, "0"),
                ("kata-get-rules", _) => answer(wb, id, n, RULES),
                ("kata-set-param", Some("numSearchThreads")) if engine.reject => {
                    out(wb, id, &format!("?{n} numSearchThreads cannot change now"));
                    out(wb, id, "");
                }
                ("kata-set-param", Some("numSearchThreads")) => {
                    engine.threads = words[2].parse().unwrap();
                    answer(wb, id, n, "");
                }
                _ => answer(wb, id, n, ""),
            }
            done.push(cmd.to_string());
        }
    }
}

fn katago(command: &str) -> EngineProfile {
    EngineProfile { name: "KataGo".into(), command: command.into(), ..EngineProfile::default() }
}

/// `profiles` saved, the first started as the foreground engine and answered through start-up.
fn started(dir: &std::path::Path, world: &World, profiles: Vec<EngineProfile>, engine: &mut Katago) -> (Workbench, InstanceId) {
    let storage = Storage::in_dirs(&dir.join("config"), &dir.join("data"));
    let mut wb = Workbench::new(storage, None, Box::new(FakeLauncher(world.clone())), Vec::new());
    wb.set_auto_quick_analyze(false).unwrap();
    wb.save_engine_settings(profiles, EngineStartup::Default).unwrap();
    wb.start_engines();
    let id = world.last();
    serve(&mut wb, world, id, engine);
    (wb, id)
}

fn params() -> KataEngineParamsEdit {
    KataEngineParamsEdit {
        pda: Some(KataParamEdit { enabled: false, value: Some(0.0), auto_load: false }),
        wrn: KataParamEdit { enabled: true, value: Some(0.3), auto_load: false },
        show_pda: false,
        show_wrn: true,
    }
}

fn step(wb: &mut Workbench, op: SearchThreadsCommand) -> u64 {
    match wb.search_threads(op).unwrap() {
        CommandResult::SearchThreads { ticket } => ticket,
        other => panic!("{other:?}"),
    }
}

fn status(wb: &mut Workbench, ticket: u64) -> SearchThreadsStatus {
    let op = wb.view(false).engine.search_threads_operation.expect("an operation");
    assert_eq!(op.ticket, ticket);
    op.status
}

fn row(wb: &mut Workbench) -> (u64, ThreadOverride, Option<u32>, bool) {
    let v = wb.view(false).engine.search_threads.expect("a ready KataGo");
    (v.process, v.state, v.confirmed, v.remote)
}

fn confirm(process: u64, enabled: bool, text: &str, edited: bool) -> SearchThreadsCommand {
    SearchThreadsCommand::Confirm { process, enabled, text: text.into(), edited, params: params() }
}

fn failed(key: &str) -> impl Fn(&SearchThreadsStatus) -> bool + '_ {
    move |s| matches!(s, SearchThreadsStatus::Failed { message_key, .. } if message_key == key)
}

/// Sets `threads` on the process from the window (the override is on afterwards).
fn apply(wb: &mut Workbench, world: &World, id: InstanceId, engine: &mut Katago, threads: u32) {
    let ticket = step(wb, confirm(id, true, &threads.to_string(), true));
    serve(wb, world, id, engine);
    assert_eq!(status(wb, ticket), SearchThreadsStatus::Done { threads: Some(threads) });
}

/// Opening reads the process's value; 确定 with a valid value sets it, then saves the other rows;
/// reopening shows the override checked with that value. Invalid input sends nothing; a value the
/// engine refuses fails with its reason and changes nothing.
#[test]
fn the_window_reads_and_applies_the_process_search_threads() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut engine = Katago::new();
    let (mut wb, id) = started(dir.path(), &world, vec![katago("/opt/katago gtp")], &mut engine);
    assert_eq!(row(&mut wb), (id, ThreadOverride::Off, None, false));

    let ticket = step(&mut wb, SearchThreadsCommand::Read { process: id });
    assert_eq!(status(&mut wb, ticket), SearchThreadsStatus::Running);
    let sent = serve(&mut wb, &world, id, &mut engine);
    assert!(sent.contains(&"kata-get-param numSearchThreads".to_string()), "{sent:?}");
    assert_eq!(status(&mut wb, ticket), SearchThreadsStatus::Done { threads: Some(2) });
    assert_eq!(row(&mut wb), (id, ThreadOverride::Off, Some(2), false));

    let ticket = step(&mut wb, confirm(id, true, " 6 ", true));
    let sent = serve(&mut wb, &world, id, &mut engine);
    let set = sent.iter().position(|c| c == "kata-set-param numSearchThreads 6").expect("set");
    let wrn = sent.iter().position(|c| c == "kata-set-param analysisWideRootNoise 0.3").expect("the other rows");
    assert!(set < wrn, "the other rows are saved once the engine confirmed: {sent:?}");
    assert_eq!(status(&mut wb, ticket), SearchThreadsStatus::Done { threads: Some(6) });
    assert_eq!(row(&mut wb), (id, ThreadOverride::On, Some(6), false));
    assert_eq!(wb.config().engine.kata_params.wrn, Some(0.3));

    // Reopened: checked with the confirmed value, refreshed from the engine.
    let ticket = step(&mut wb, SearchThreadsCommand::Read { process: id });
    serve(&mut wb, &world, id, &mut engine);
    assert_eq!(status(&mut wb, ticket), SearchThreadsStatus::Done { threads: Some(6) });
    assert_eq!(row(&mut wb).1, ThreadOverride::On);

    // The unedited confirmed value only saves the other rows.
    let ticket = step(&mut wb, confirm(id, true, "6", false));
    let sent = serve(&mut wb, &world, id, &mut engine);
    assert!(!sent.iter().any(|c| c.contains("numSearchThreads")), "{sent:?}");
    assert_eq!(status(&mut wb, ticket), SearchThreadsStatus::Done { threads: Some(6) });

    for text in ["0", "1025", "abc", "", "2.5"] {
        let ticket = step(&mut wb, confirm(id, true, text, true));
        assert!(failed("SetKataEngines.threadInvalid")(&status(&mut wb, ticket)), "{text:?}");
        assert!(world.take(id).is_empty(), "nothing is sent for {text:?}");
    }

    engine.reject = true;
    let ticket = step(&mut wb, confirm(id, true, "8", true));
    serve(&mut wb, &world, id, &mut engine);
    let s = status(&mut wb, ticket);
    assert!(failed("SetKataEngines.threadFailed")(&s), "{s:?}");
    let SearchThreadsStatus::Failed { detail: Some(detail), .. } = &s else { panic!("{s:?}") };
    assert!(detail.contains("KataGo rejected numSearchThreads") && detail.contains("cannot change now"), "{detail}");
    assert_eq!(row(&mut wb), (id, ThreadOverride::On, Some(6), false), "a refused value changes nothing");
}

/// A request the engine does not answer in time fails; its late answer changes nothing.
#[test]
fn an_unanswered_request_times_out_and_its_late_answer_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut engine = Katago::new();
    let (mut wb, id) = started(dir.path(), &world, vec![katago("/opt/katago gtp")], &mut engine);
    let ticket = step(&mut wb, confirm(id, true, "7", true));
    let pending = world.take(id);
    let set = pending.iter().find(|l| l.ends_with("kata-set-param numSearchThreads 7")).expect("set").clone();
    wb.engine_tick(Instant::now() + Duration::from_secs(9));
    let s = status(&mut wb, ticket);
    assert!(failed("SetKataEngines.threadFailed")(&s), "{s:?}");
    assert!(matches!(&s, SearchThreadsStatus::Failed { detail: Some(d), .. } if d.contains("Timed out")), "{s:?}");
    let n: u64 = set.split(' ').next().unwrap().parse().unwrap();
    answer(&mut wb, id, n, "");
    assert_eq!(row(&mut wb), (id, ThreadOverride::Off, None, false), "the late answer is not taken");
    assert!(failed("SetKataEngines.threadFailed")(&status(&mut wb, ticket)));
}

/// Unchecking an override offers restoring the launch source (configuration file, benchmark
/// recommendation or launch command). Declining changes nothing; confirming restarts the engine
/// at the same position and analysis state, the new process starts without the override, its
/// value is read and the other rows are saved.
#[test]
fn unchecking_an_override_restores_the_launch_source_by_restarting() {
    let benchmark = EngineProfile {
        thread_policy: Some(ThreadPolicy { source: ThreadSource::BENCHMARK, threads: 3, ..ThreadPolicy::default() }),
        ..katago("/opt/katago gtp")
    };
    for (profile, source, recommended) in [
        (katago("/opt/katago gtp"), ThreadRestoreSource::CFG, None),
        (benchmark, ThreadRestoreSource::BENCHMARK, Some(3)),
        (katago("/opt/katago gtp -override-config numSearchThreads=5"), ThreadRestoreSource::COMMAND, None),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let mut engine = Katago::new();
        let (mut wb, id) = started(dir.path(), &world, vec![profile], &mut engine);
        apply(&mut wb, &world, id, &mut engine, 6);
        let analysing = wb.view(false).engine.analysing;

        // Declined: the override stays and the process keeps running.
        let ticket = step(&mut wb, confirm(id, false, "6", false));
        assert_eq!(status(&mut wb, ticket), SearchThreadsStatus::ConfirmRestore { source, recommended });
        assert!(world.take(id).is_empty() && !world.terminated(id));
        assert_eq!(row(&mut wb), (id, ThreadOverride::On, Some(6), false));

        let ticket = step(&mut wb, confirm(id, false, "6", false));
        assert!(matches!(status(&mut wb, ticket), SearchThreadsStatus::ConfirmRestore { .. }));
        assert_eq!(step(&mut wb, SearchThreadsCommand::Restore { ticket }), ticket);
        assert!(world.terminated(id), "the overridden process ends");
        let successor = world.last();
        assert_ne!(successor, id);
        assert_eq!(status(&mut wb, ticket), SearchThreadsStatus::Running);
        let mut fresh = Katago::new();
        let sent = serve(&mut wb, &world, successor, &mut fresh);
        assert!(sent.iter().any(|c| c.starts_with("boardsize") || c.starts_with("clear_board")), "synced: {sent:?}");
        let read = sent.iter().position(|c| c == "kata-get-param numSearchThreads").expect("the new value is read");
        let wrn = sent.iter().rposition(|c| c == "kata-set-param analysisWideRootNoise 0.3").expect("the other rows");
        assert!(read < wrn, "{sent:?}");
        assert_eq!(status(&mut wb, ticket), SearchThreadsStatus::Done { threads: Some(fresh.threads) });
        assert_eq!(row(&mut wb), (successor, ThreadOverride::Off, Some(fresh.threads), false));
        assert_eq!(wb.view(false).engine.analysing, analysing, "analysis resumes as before");

        // The offer is used once.
        let again = step(&mut wb, SearchThreadsCommand::Restore { ticket });
        assert_ne!(again, ticket);
        assert!(failed("SetKataEngines.threadRestoreFailed")(&status(&mut wb, again)));
        assert_eq!(world.count(), 2, "no second restart");
    }
}

/// A remote-managed engine's state is unknown until the window sets it, and its source cannot be
/// restored from this client: unchecking is refused and the override stays.
#[test]
fn a_remote_engine_override_cannot_be_restored() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut engine = Katago::new();
    let (mut wb, id) = started(dir.path(), &world, vec![katago("ssh gpu-host katago gtp")], &mut engine);
    assert_eq!(row(&mut wb), (id, ThreadOverride::Unknown, None, true));
    apply(&mut wb, &world, id, &mut engine, 6);
    assert_eq!(row(&mut wb), (id, ThreadOverride::On, Some(6), true));
    let ticket = step(&mut wb, confirm(id, false, "6", false));
    assert!(failed("SetKataEngines.threadRemoteRestoreUnsupported")(&status(&mut wb, ticket)));
    assert!(!world.terminated(id) && world.count() == 1);
    assert_eq!(row(&mut wb), (id, ThreadOverride::On, Some(6), true));
}

/// The override and the confirmed value belong to one process: a restart or another engine
/// starts without them, a step for the replaced process is stale, and its late answer is not
/// taken. Coming back to a process that kept running finds what it held.
#[test]
fn the_override_does_not_outlive_its_process() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut engine = Katago::new();
    let profiles = vec![katago("/opt/katago gtp"), katago("/opt/other/katago gtp")];
    let (mut wb, id) = started(dir.path(), &world, profiles, &mut engine);
    apply(&mut wb, &world, id, &mut engine, 6);

    // The window stays open while another engine takes the seat: its read is stale, and the
    // answer of the switched-away (still running) process is not taken.
    let ticket = step(&mut wb, SearchThreadsCommand::Read { process: id });
    let pending = world.take(id);
    wb.switch_engine(1).unwrap();
    let other = world.last();
    let mut second = Katago::new();
    serve(&mut wb, &world, other, &mut second);
    assert_eq!(row(&mut wb), (other, ThreadOverride::Off, None, false));
    engine.threads = 9;
    for line in pending {
        let (n, cmd) = line.split_once(' ').unwrap();
        answer(&mut wb, id, n.parse().unwrap(), if cmd.starts_with("kata-get-param numSearchThreads") { "9" } else { "" });
    }
    assert!(failed("SetKataEngines.threadStale")(&status(&mut wb, ticket)));
    let ticket = step(&mut wb, confirm(id, true, "4", true));
    assert!(failed("SetKataEngines.threadStale")(&status(&mut wb, ticket)));

    // Back to the first process, which kept running: it still holds its own override.
    wb.switch_engine(0).unwrap();
    serve(&mut wb, &world, id, &mut engine);
    assert_eq!(row(&mut wb), (id, ThreadOverride::On, Some(6), false));

    // A restart is another process.
    wb.restart_engine().unwrap();
    let restarted = world.last();
    assert_ne!(restarted, id);
    let mut fresh = Katago::new();
    serve(&mut wb, &world, restarted, &mut fresh);
    assert_eq!(row(&mut wb), (restarted, ThreadOverride::Off, None, false));
    let ticket = step(&mut wb, SearchThreadsCommand::Read { process: id });
    assert!(failed("SetKataEngines.threadStale")(&status(&mut wb, ticket)));
}

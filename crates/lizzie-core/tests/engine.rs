//! Engines through the public `Workbench` operations with a controlled GTP process: start-up and
//! identification, command confirmation, position sync, analysis ownership (late results,
//! another document), pause, limits, switching, failure and exit.

use std::sync::{Arc, Mutex};

use lizzie_core::api::{EngineNotice, EngineScope};
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats::sgf;
use lizzie_core::record::RecordFormat;
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
    Workbench::new(storage(dir), None, Box::new(FakeLauncher(world.clone())), Vec::new())
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
    let mut wb = workbench(dir, world);
    wb.save_engine_settings(vec![profile("KataGo", "/opt/katago")], EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let id = world.launched()[0].0;
    answer_all(&mut wb, world, id, KATA);
    (wb, id)
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
    let f = v.engine_failure.expect("failure shown");
    assert!(f.during_startup && f.message.contains("127"));
    assert!(f.output.iter().any(|l| l.contains("libcudart")));
    assert!(v.engine.foreground.is_none());
    assert!(!world.terminated(b), "the other engine keeps running");
    // Output of the dead instance is ignored.
    out(&mut wb, a, "=1 KataGo");
    assert!(wb.view(false).engine.foreground.is_none());
}

#[test]
fn launch_failure_is_reported_without_an_instance() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = workbench(dir.path(), &world);
    wb.save_engine_settings(vec![profile("X", "/opt/missing")], EngineStartup::Default)
        .unwrap();
    wb.start_engines();
    let v = wb.view(false);
    assert!(v.engine_failure.unwrap().message.contains("No such file"));
    assert!(v.engine.foreground.is_none() && world.launched().is_empty());
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
            answer(
                &mut wb,
                id,
                n,
                r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"TERRITORY","suicide":false,"tax":"SEKI","whiteHandicapBonus":"0"}"#,
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
    assert!(v.engine_failure.is_some());
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
    for line in &sent {
        let n = number(line);
        out(&mut wb, a, "");
        if line.contains(" play ") || line.contains(" set_position ") {
            out(&mut wb, a, &format!("?{n} illegal move"));
            out(&mut wb, a, "");
        } else if line.contains("kata-analyze") {
            out(&mut wb, a, &format!("={n}"));
            out(&mut wb, a, &info("D4", 900));
        } else {
            answer(&mut wb, a, n, "");
        }
    }
    let v = wb.view(false);
    assert!(v.analysis.is_none(), "result of a failed sync shown: {sent:?}");
    assert!(
        v.history.iter().all(|h| h.visits == 0),
        "result of a failed sync stored: {:?}",
        v.history
    );
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
        "info move A18 visits 30 winrate 0.6 prior 0.2 order 0 pv A18 pass A19 Z99 C3 rootInfo visits 30",
    );
    let v = wb.view(false);
    let c = &v.analysis.unwrap().candidates[0];
    assert_eq!(c.pv, [Some(Point { x: 0, y: 1 }), None, Some(Point { x: 0, y: 0 })]);
    assert_eq!(c.pv_captures, [vec![Point { x: 0, y: 0 }], vec![], vec![]]);
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
        .engine_failure
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
    let (mut wb, id, sent) = start_family(dir.path(), &world, "Leela", commands);
    assert_eq!(sent.iter().filter(|s| *s == "time_left b 0 0").count(), 1);
    let node = wb.view(false).position.node;
    let now = Instant::now();
    wb.engine_tick(now);
    assert!(world.take(id).is_empty(), "no probe before the interval");
    wb.engine_tick(now + Duration::from_millis(1001));
    let probe = world.take(id);
    assert_eq!(probe.len(), 1);
    assert!(probe[0].ends_with(" name"));
    wb.engine_tick(now + Duration::from_secs(3));
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
    assert!(wb.begin_startup_benchmark().is_none());
    wb.set_startup_benchmark(true).unwrap();
    // Apple automatic optimization requires its trusted Metal resource package.
    if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        assert!(wb.begin_startup_benchmark().is_none());
        return;
    }
    let job = wb.begin_startup_benchmark().unwrap();
    let notice = wb.view(false).tuning.unwrap().startup_notice.unwrap();
    assert_eq!(notice.id, job.id);
    assert!(notice.automatic);
    assert!(!wb.dismiss_startup_notice(job.id + 1));
    assert!(wb.dismiss_startup_notice(job.id));
    wb.complete_benchmark(job.id, Err("cancelled".into()));
    assert!(wb.begin_startup_benchmark().is_none());
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

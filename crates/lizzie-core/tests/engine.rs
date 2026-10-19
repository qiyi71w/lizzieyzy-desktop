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
        self.0
             .0
            .lock()
            .unwrap()
            .push((id, spec.program.clone(), Proc::default()));
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
fn time_limit_stops_this_position_once_and_navigation_resumes() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    let mut settings = wb.config().engine.analysis.clone();
    settings.max_analyze_time_seconds = 0;
    wb.set_analysis_settings(settings).unwrap();
    open(&mut wb, "(;B[pd];W[dd])");
    answer_all(&mut wb, &world, id, KATA);
    out(&mut wb, id, &info("Q16", 5));
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

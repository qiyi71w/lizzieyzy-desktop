//! 双引擎模式 through the public `Workbench` operations with two controlled GTP processes: each
//! engine's results stay its own, the secondary engine is never the foreground engine, pause and
//! limits, late results during quick navigation, failures and leaving the mode, rules on both
//! engines, and the secondary analysis saved in records (`LZ2`/`LZOP2`).

use std::sync::{Arc, Mutex};

use lizzie_core::api::{EngineNotice, EngineScope, OpenOutcome};
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats::sgf;
use lizzie_core::record::RecordFormat;
use lizzie_core::settings::{ExtraMode, Storage};
use lizzie_core::workbench::{OpenOrigin, Workbench};

/// Per instance: its program and the commands it was sent and not yet answered.
type Sent = Vec<(InstanceId, String, Vec<String>, bool)>;

#[derive(Clone, Default)]
struct World(Arc<Mutex<Sent>>);

impl World {
    fn take(&self, id: InstanceId) -> Vec<String> {
        let mut w = self.0.lock().unwrap();
        w.iter_mut()
            .find(|p| p.0 == id)
            .map(|p| std::mem::take(&mut p.2))
            .unwrap_or_default()
    }
    fn instance(&self, program: &str) -> Option<InstanceId> {
        let w = self.0.lock().unwrap();
        w.iter().rev().find(|p| p.1 == program).map(|p| p.0)
    }
    fn launches(&self) -> usize {
        self.0.lock().unwrap().len()
    }
    fn terminated(&self, id: InstanceId) -> bool {
        self.0.lock().unwrap().iter().any(|p| p.0 == id && p.3)
    }
}

struct FakeIo(InstanceId, World);

impl EngineIo for FakeIo {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        let mut w = (self.1).0.lock().unwrap();
        w.iter_mut().find(|p| p.0 == self.0).unwrap().2.push(line.to_string());
        Ok(())
    }
    fn terminate(self: Box<Self>) {
        let mut w = (self.1).0.lock().unwrap();
        w.iter_mut().find(|p| p.0 == self.0).unwrap().3 = true;
    }
}

struct FakeLauncher(World);

impl Launcher for FakeLauncher {
    fn launch(&self, id: InstanceId, spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        self.0
             .0
            .lock()
            .unwrap()
            .push((id, spec.program.clone(), Vec::new(), false));
        Ok(Box::new(FakeIo(id, self.0.clone())))
    }
}

const KATA: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nset_position\nkata-analyze\nkata-raw-nn\nkata-set-rules\nkata-get-rules\nstop";
const CHINESE: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;
const JAPANESE: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"TERRITORY","suicide":false,"tax":"SEKI","whiteHandicapBonus":"0"}"#;

fn out(wb: &mut Workbench, id: InstanceId, line: &str) {
    wb.engine_input(id, EngineInput::Stdout(line.to_string()));
}

/// Answers every outstanding command of `id`, reporting `rules` to `kata-get-rules`; analysis
/// requests get only their header, so their stream stays open. Returns the commands answered.
fn answer_all_with(wb: &mut Workbench, world: &World, id: InstanceId, rules: &str) -> Vec<String> {
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
                "name" => Some("KataGo"),
                "version" => Some("1.16.4"),
                "list_commands" => Some(KATA),
                "kata-get-rules" => Some(rules),
                "kata-analyze" => None,
                _ => Some(""),
            };
            match reply {
                Some(text) => {
                    out(wb, id, &format!("={n} {text}"));
                    out(wb, id, "");
                }
                None => out(wb, id, &format!("={n}")),
            }
            done.push(cmd.to_string());
        }
    }
}

fn answer_all(wb: &mut Workbench, world: &World, id: InstanceId) -> Vec<String> {
    answer_all_with(wb, world, id, CHINESE)
}

fn info(mv: &str, visits: u64) -> String {
    format!(
        "info move {mv} visits {visits} winrate 0.6 scoreLead 1.5 prior 0.2 order 0 pv {mv} rootInfo visits {visits}"
    )
}

fn analyses(sent: &[String]) -> usize {
    sent.iter().filter(|c| c.starts_with("kata-analyze")).count()
}

fn stops(sent: &[String]) -> usize {
    sent.iter().filter(|c| *c == "stop").count()
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

fn open(wb: &mut Workbench, text: &str) -> Option<OpenOutcome> {
    let t = wb.begin_open(OpenOrigin::File {
        path: "/x.sgf".into(),
        source: lizzie_core::api::OpenSource::Drop,
    });
    assert!(wb.complete_open(t, Ok((RecordFormat::Sgf, sgf::read(text).unwrap()))));
    wb.view(false).open_outcome
}

/// Engines A (foreground), B and C configured; A started; 双引擎模式 on with B as the secondary
/// engine; both answered through start-up and analysing the current position.
fn dual(dir: &std::path::Path, world: &World) -> (Workbench, InstanceId, InstanceId) {
    let mut wb = Workbench::new(storage(dir), None, Box::new(FakeLauncher(world.clone())), Vec::new());
    let engines = vec![profile("A", "/opt/a"), profile("B", "/opt/b"), profile("C", "/opt/c")];
    wb.save_engine_settings(engines, EngineStartup::Default).unwrap();
    wb.start_engines();
    let a = world.instance("/opt/a").unwrap();
    answer_all(&mut wb, world, a);
    assert!(wb.set_extra_mode(ExtraMode::DoubleEngine).unwrap());
    assert!(wb.switch_secondary_engine(1).unwrap());
    let b = world.instance("/opt/b").unwrap();
    answer_all(&mut wb, world, b);
    answer_all(&mut wb, world, a);
    (wb, a, b)
}

#[test]
fn both_engines_analyse_the_position_and_keep_their_own_results() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, a, b) = dual(dir.path(), &world);
    out(&mut wb, a, &info("Q16", 120));
    out(&mut wb, b, &info("D4", 80));
    let v = wb.view(false);
    let fg = v.engine.foreground.expect("foreground");
    let sec = v.engine.secondary.expect("secondary");
    assert_eq!(
        (fg.index, fg.name.as_str(), sec.index, sec.name.as_str()),
        (0, "A", 1, "B")
    );
    assert!(v.engine.analysing && sec.analysing && sec.ready);
    let primary = v.analysis.expect("primary result");
    let secondary = v.secondary.expect("right board").analysis.expect("secondary result");
    assert_eq!((primary.engine.as_str(), primary.visits), ("A", 120));
    assert_eq!((secondary.engine.as_str(), secondary.visits), ("B", 80));
    assert_eq!(
        primary.candidates[0].point,
        Some(lizzie_core::record::Point { x: 15, y: 3 })
    );
    assert_eq!(
        secondary.candidates[0].point,
        Some(lizzie_core::record::Point { x: 3, y: 15 })
    );

    // The foreground engine cannot be chosen as the secondary engine, nor the other way round.
    assert!(wb.switch_secondary_engine(0).unwrap());
    assert_eq!(wb.view(false).engine_notice, Some(EngineNotice::SameEngine));
    assert!(wb.switch_engine(1).unwrap());
    let v = wb.view(false);
    assert_eq!(v.engine_notice, Some(EngineNotice::SameEngine));
    assert_eq!(
        (v.engine.foreground.unwrap().index, v.engine.secondary.unwrap().index),
        (0, 1)
    );
}

#[test]
fn streaming_results_send_neither_engine_another_command() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, a, b) = dual(dir.path(), &world);
    for n in 1..30 {
        out(&mut wb, b, &info("D4", n * 10));
        out(&mut wb, a, &info("Q16", n));
    }
    assert_eq!((world.take(a), world.take(b)), (Vec::new(), Vec::new()));
    let v = wb.view(false);
    assert!(v.engine.analysing && v.engine.secondary.unwrap().analysing);
}

#[test]
fn no_secondary_engine_is_chosen_during_setup_mode() {
    use lizzie_core::api::EditCommand;
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    // Setup mode needs no foreground engine; the secondary engine menu still shows in 双引擎模式.
    let mut wb = Workbench::new(
        storage(dir.path()),
        None,
        Box::new(FakeLauncher(world.clone())),
        Vec::new(),
    );
    let engines = vec![profile("A", "/opt/a"), profile("B", "/opt/b")];
    wb.save_engine_settings(engines, EngineStartup::Empty).unwrap();
    assert!(wb.set_extra_mode(ExtraMode::DoubleEngine).unwrap());
    wb.edit(EditCommand::SetSetupMode {
        enabled: true,
        confirmed: None,
    })
    .unwrap();
    assert!(wb.setup_mode());
    wb.switch_secondary_engine(1).unwrap();
    let v = wb.view(false);
    assert_eq!(v.engine_notice, Some(EngineNotice::SetupModeActive));
    assert_eq!(world.launches(), 0);
    assert!(v.engine.secondary.is_none());
    wb.edit(EditCommand::SetSetupMode {
        enabled: false,
        confirmed: None,
    })
    .unwrap();
    wb.switch_secondary_engine(1).unwrap();
    assert_eq!(world.launches(), 1);
}

#[test]
fn quick_navigation_keeps_late_lines_on_their_own_node_for_each_engine() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, a, b) = dual(dir.path(), &world);
    open(&mut wb, "(;SZ[19];B[pd];W[dp];B[pp])");
    answer_all(&mut wb, &world, a);
    answer_all(&mut wb, &world, b);
    // Two quick steps: each engine is still on the root when the user is on move 2.
    wb.navigate(Navigation::Forward(1));
    wb.navigate(Navigation::Forward(1));
    out(&mut wb, b, &info("D4", 40));
    out(&mut wb, a, &info("Q4", 60));
    let v = wb.view(false);
    assert_eq!(v.position.move_number, 2);
    assert!(
        v.analysis.is_none() && v.secondary.unwrap().analysis.is_none(),
        "late lines are not move 2's"
    );
    // Each engine finishes the batch in flight (move 1), then holds move 2: its last request.
    let (sent_a, sent_b) = (answer_all(&mut wb, &world, a), answer_all(&mut wb, &world, b));
    for sent in [&sent_a, &sent_b] {
        let last = sent.iter().rev().find(|c| c.starts_with("kata-analyze"));
        assert!(last.is_some_and(|c| c.starts_with("kata-analyze B")), "{sent:?}");
    }
    out(&mut wb, b, &info("R16", 30));
    let v = wb.view(false);
    assert_eq!(v.secondary.unwrap().analysis.unwrap().visits, 30);
    assert!(
        v.analysis.is_none(),
        "the secondary engine's line is not the foreground engine's"
    );
    // The late root lines were kept as each engine's history of the root.
    wb.navigate(Navigation::First);
    let v = wb.view(false);
    assert_eq!(v.analysis.unwrap().engine, "A");
    assert_eq!(v.secondary.unwrap().analysis.unwrap().engine, "B");
}

#[test]
fn the_user_pause_stops_both_and_no_secondary_ending_resumes_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, a, b) = dual(dir.path(), &world);
    wb.toggle_analysis().unwrap();
    let (sent_a, sent_b) = (answer_all(&mut wb, &world, a), answer_all(&mut wb, &world, b));
    assert_eq!((stops(&sent_a), stops(&sent_b)), (1, 1), "{sent_a:?} {sent_b:?}");
    assert!(wb.view(false).engine.paused);

    // The secondary engine's stream ends, then the process exits: the pause stays and nothing
    // asks either engine to analyse again.
    out(&mut wb, b, "");
    wb.engine_input(b, EngineInput::Exited(Some(1)));
    let v = wb.view(false);
    assert!(v.engine.paused && v.engine_failure.is_some());
    assert!(v.engine.secondary.is_none(), "the failed engine leaves its seat");
    assert_eq!(
        v.engine.foreground.as_ref().map(|f| f.index),
        Some(0),
        "never made the foreground engine"
    );
    assert_eq!(analyses(&answer_all(&mut wb, &world, a)), 0);

    // Resuming restarts the foreground engine only.
    wb.toggle_analysis().unwrap();
    assert_eq!(analyses(&answer_all(&mut wb, &world, a)), 1);
}

#[test]
fn limits_stop_the_secondary_alone_but_the_foreground_limit_stops_both() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, a, b) = dual(dir.path(), &world);
    let mut settings = wb.view(false).settings.engine.analysis;
    settings.limit_playout = true;
    settings.limit_playouts = 100;
    wb.set_analysis_settings(settings).unwrap();
    open(&mut wb, "(;SZ[19];B[pd];W[dp])");
    answer_all(&mut wb, &world, a);
    answer_all(&mut wb, &world, b);

    // The secondary engine passes the limit: only it stops (Java leelaz2 stops itself).
    out(&mut wb, b, &info("D4", 101));
    let v = wb.view(false);
    assert!(v.engine.secondary.unwrap().stopped_by_limit && !v.engine.stopped_by_limit);
    assert_eq!(v.engine_notice, Some(EngineNotice::StoppedByLimit));
    let (sent_a, sent_b) = (answer_all(&mut wb, &world, a), answer_all(&mut wb, &world, b));
    assert_eq!((stops(&sent_a), stops(&sent_b)), (0, 1), "{sent_a:?} {sent_b:?}");

    // Another position: both analyse again; the foreground limit then stops both.
    wb.navigate(Navigation::Forward(1));
    let (sent_a, sent_b) = (answer_all(&mut wb, &world, a), answer_all(&mut wb, &world, b));
    assert_eq!((analyses(&sent_a), analyses(&sent_b)), (1, 1));
    out(&mut wb, a, &info("Q4", 101));
    let v = wb.view(false);
    assert!(v.engine.stopped_by_limit && !v.engine.paused);
    let (sent_a, sent_b) = (answer_all(&mut wb, &world, a), answer_all(&mut wb, &world, b));
    assert_eq!((stops(&sent_a), stops(&sent_b)), (1, 1), "{sent_a:?} {sent_b:?}");
    // Space resumes both.
    wb.toggle_analysis().unwrap();
    let (sent_a, sent_b) = (answer_all(&mut wb, &world, a), answer_all(&mut wb, &world, b));
    assert_eq!((analyses(&sent_a), analyses(&sent_b)), (1, 1));
}

#[test]
fn leaving_the_mode_keeps_the_secondary_engine_running_without_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, a, b) = dual(dir.path(), &world);
    assert!(wb.set_extra_mode(ExtraMode::Normal).unwrap());
    assert_eq!(stops(&answer_all(&mut wb, &world, b)), 1);
    let v = wb.view(false);
    assert!(v.secondary.is_none() && v.settings.extra_mode == ExtraMode::Normal);
    assert!(!v.engine.secondary.as_ref().unwrap().analysing && !world.terminated(b));
    // Browsing moves the foreground engine only.
    open(&mut wb, "(;SZ[19];B[pd])");
    wb.navigate(Navigation::Forward(1));
    assert!(analyses(&answer_all(&mut wb, &world, a)) >= 1);
    let sent_b = answer_all(&mut wb, &world, b);
    assert_eq!(analyses(&sent_b), 0, "{sent_b:?}");
    // Back in 双引擎模式 the same process catches up with the position and analyses it.
    let launches = world.launches();
    assert!(wb.set_extra_mode(ExtraMode::DoubleEngine).unwrap());
    let sent_b = answer_all(&mut wb, &world, b);
    assert!(
        sent_b.iter().any(|c| c.starts_with("play B")) && analyses(&sent_b) == 1,
        "{sent_b:?}"
    );
    assert_eq!(world.launches(), launches, "no new process");
    // The mode is remembered; the secondary engine is not.
    let reopened = Workbench::new(
        storage(dir.path()),
        None,
        Box::new(FakeLauncher(World::default())),
        Vec::new(),
    );
    assert_eq!(reopened.config().ui.extra_mode, ExtraMode::DoubleEngine);
}

#[test]
fn a_secondary_that_fails_while_starting_returns_the_seat_to_the_kept_one() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, a, b) = dual(dir.path(), &world);
    let mut settings = wb.view(false).settings.engine.analysis;
    settings.fast_engine_change = true;
    wb.set_analysis_settings(settings).unwrap();
    assert!(wb.switch_secondary_engine(2).unwrap());
    let c = world.instance("/opt/c").unwrap();
    assert_eq!(
        stops(&answer_all(&mut wb, &world, b)),
        1,
        "the kept engine stops analysing"
    );
    wb.engine_input(c, EngineInput::Exited(Some(127)));
    let v = wb.view(false);
    assert!(v.engine_failure.unwrap().during_startup);
    assert_eq!(
        v.engine.secondary.map(|s| s.index),
        Some(1),
        "back to the previous secondary engine"
    );
    assert_eq!(analyses(&answer_all(&mut wb, &world, b)), 1);
    assert_eq!(v.engine.foreground.map(|f| f.index), Some(0));
    assert!(answer_all(&mut wb, &world, a).is_empty());

    // Shutting the foreground engine down leaves the secondary engine where it is.
    assert!(wb.shutdown_engines(EngineScope::Current));
    let v = wb.view(false);
    assert!(v.engine.foreground.is_none() && v.engine.secondary.map(|s| s.index) == Some(1));
    // "关闭当前引擎" of the secondary menu ends it; its results stay on the board.
    out(&mut wb, b, &info("D4", 10));
    assert!(wb.shutdown_secondary_engine());
    let v = wb.view(false);
    assert!(v.engine.secondary.is_none() && world.terminated(b));
    assert_eq!(v.secondary.unwrap().analysis.unwrap().visits, 10);
}

#[test]
fn declared_rules_hold_both_engines_until_both_confirm_or_the_user_continues() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, a, b) = dual(dir.path(), &world);
    open(&mut wb, "(;SZ[19]RU[Japanese];B[pd])");
    // The foreground engine takes the rules, the secondary engine keeps Chinese.
    let sent_a = answer_all_with(&mut wb, &world, a, JAPANESE);
    let sent_b = answer_all_with(&mut wb, &world, b, CHINESE);
    assert!(sent_b.iter().any(|c| c.starts_with("kata-set-rules")), "{sent_b:?}");
    assert_eq!(analyses(&sent_a) + analyses(&sent_b), 0, "{sent_a:?} {sent_b:?}");
    let prompt = wb.view(false).rules_prompt.expect("asked about the secondary engine");
    assert_eq!(prompt.failure, "TARGET_MISMATCH");
    assert!(prompt.observed.unwrap().starts_with("ko=SIMPLE, scoring=AREA"));
    assert!(wb.answer_rules_prompt(prompt.prompt, true));
    let (sent_a, sent_b) = (
        answer_all_with(&mut wb, &world, a, JAPANESE),
        answer_all(&mut wb, &world, b),
    );
    assert_eq!((analyses(&sent_a), analyses(&sent_b)), (1, 1), "{sent_a:?} {sent_b:?}");
    assert!(wb.view(false).rules_prompt.is_none(), "asked once");
}

#[test]
fn console_commands_and_the_pure_network_follow_both_engines() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, a, b) = dual(dir.path(), &world);
    assert_eq!(wb.send_gtp("kata-set-param maxVisits 50").unwrap(), None);
    let has = |sent: &[String], c: &str| sent.iter().any(|s| s == c);
    let (sent_a, sent_b) = (answer_all(&mut wb, &world, a), answer_all(&mut wb, &world, b));
    assert!(has(&sent_a, "kata-set-param maxVisits 50") && has(&sent_b, "kata-set-param maxVisits 50"));
    assert!(wb.toggle_raw_net());
    let (sent_a, sent_b) = (world.take(a), world.take(b));
    let raw = |sent: &[String]| sent.iter().any(|s| s.contains("kata-raw-nn"));
    assert!(raw(&sent_a) && raw(&sent_b), "{sent_a:?} {sent_b:?}");
    let v = wb.view(false);
    assert!(v.raw_net.is_some() && v.secondary.unwrap().raw_net.is_some());
}

#[test]
fn secondary_analysis_in_records_is_read_offered_and_saved_as_its_own() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = Workbench::new(
        storage(dir.path()),
        None,
        Box::new(FakeLauncher(World::default())),
        Vec::new(),
    );
    wb.set_append_winrate_to_comment(true).unwrap();
    let text = "(;SZ[9]KM[7.5]LZOP2[b6 50.0 80 0.5 1.0\nmove E5 visits 80 winrate 5000 scoreMean 0.5 pv E5]\
                ;B[ee]LZ2[b6 40.0 100 1.0 2.0\nmove D4 visits 100 winrate 6000 scoreMean 1.0 pv D4]\
                ;W[dd]LZ[kata 70.0 300 3.0 1.0\nmove C3 visits 300 winrate 3000 scoreMean -3.0 pv C3]\
                LZ2[b6 45.0 200 -2.0 1.5\nmove C3 visits 200 winrate 5500 scoreMean -2.0 pv C3])";
    match open(&mut wb, text) {
        Some(OpenOutcome::Adopted { double_engine_hint, .. }) => assert!(double_engine_hint),
        other => panic!("{other:?}"),
    }
    assert!(
        wb.view(false).secondary.is_none(),
        "the right board belongs to 双引擎模式"
    );
    wb.set_extra_mode(ExtraMode::DoubleEngine).unwrap();
    wb.navigate(Navigation::Forward(1));
    let v = wb.view(false);
    assert!(v.analysis.is_none());
    let s = v.secondary.as_ref().unwrap().analysis.clone().unwrap();
    assert_eq!((s.engine.as_str(), s.visits, s.winrate), ("b6", 100, 60.0));
    // The secondary engine's own history (`winrate2`/`scoreMean2` per node), root included: the
    // 副引擎 list takes the next move's row and its score columns from it, not from the primary.
    let history = v.secondary.as_ref().unwrap().history.clone();
    assert_eq!(
        history.iter().map(|h| (h.visits, h.score_lead)).collect::<Vec<_>>(),
        vec![(80, Some(0.5)), (100, Some(1.0)), (200, Some(-2.0))]
    );
    assert_eq!(v.history.len(), 1, "the primary history keeps its own single node");

    // Saving writes each engine's payload under its own tag; 双引擎模式 comments carry the
    // secondary engine's summary measured against its own previous result.
    let saved = wb.record_text("test");
    assert_eq!(saved.matches("LZ2[b6 ").count(), 2, "{saved}");
    assert_eq!(saved.matches("LZOP2[b6 ").count(), 1, "{saved}");
    assert_eq!(saved.matches("LZ[kata ").count(), 1, "{saved}");
    let second = saved.split(";W[dd]").nth(1).unwrap();
    assert!(
        second.contains("(b6 / 200") && second.contains("(-15.0%)") && second.contains("(+1.0)"),
        "{second}"
    );
    wb.set_extra_mode(ExtraMode::Normal).unwrap();
    let saved = wb.record_text("test");
    assert!(saved.split(";W[dd]").nth(1).unwrap().contains("(kata / 300"), "{saved}");
    assert_eq!(
        saved.matches("LZ2[b6 ").count(),
        2,
        "secondary data is saved whatever the mode"
    );

    // Opening again in 双引擎模式 does not ask.
    wb.set_extra_mode(ExtraMode::DoubleEngine).unwrap();
    match open(&mut wb, text) {
        Some(OpenOutcome::Adopted { double_engine_hint, .. }) => assert!(!double_engine_hint),
        other => panic!("{other:?}"),
    }
}

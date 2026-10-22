//! Pure-network H through the same Workbench operations as desktop commands. Fake processes
//! preserve GTP command ids and the separate stdout/stderr streams used by real engines.
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use lizzie_core::api::OpenSource;
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::raw_net::RawNetMode;
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats::sgf;
use lizzie_core::record::RecordFormat;
use lizzie_core::settings::Storage;
use lizzie_core::workbench::{OpenOrigin, Workbench};

#[derive(Clone, Default)]
struct World(Arc<Mutex<HashMap<InstanceId, Vec<String>>>>);
struct FakeIo(World, InstanceId);
impl EngineIo for FakeIo {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        self.0.0.lock().unwrap().get_mut(&self.1).unwrap().push(line.to_string());
        Ok(())
    }
    fn terminate(self: Box<Self>) {}
}
impl Launcher for World {
    fn launch(&self, id: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        self.0.lock().unwrap().insert(id, Vec::new());
        Ok(Box::new(FakeIo(self.clone(), id)))
    }
}
impl World {
    fn take(&self, id: InstanceId) -> Vec<String> {
        std::mem::take(self.0.lock().unwrap().get_mut(&id).unwrap())
    }
}
fn out(wb: &mut Workbench, id: InstanceId, line: &str) {
    wb.engine_input(id, EngineInput::Stdout(line.to_string()));
}
fn answer(wb: &mut Workbench, id: InstanceId, n: u64, text: &str) {
    let mut lines = text.lines();
    out(wb, id, &format!("={n} {}", lines.next().unwrap_or("")));
    for line in lines { out(wb, id, line); }
    out(wb, id, "");
}
fn number(cmd: &str) -> u64 { cmd.split_once(' ').unwrap().0.parse().unwrap() }
fn command(cmd: &str) -> &str { cmd.split_once(' ').unwrap().1 }

/// Respond through start-up / sync, leave analysis streaming and raw network unanswered.
fn settle(wb: &mut Workbench, world: &World, id: InstanceId, kata: bool) -> Vec<String> {
    let mut commands = Vec::new();
    loop {
        let batch = world.take(id);
        if batch.is_empty() { return commands; }
        for line in batch {
            out(wb, id, ""); // a new command closes the old analysis stream
            let n = number(&line);
            let cmd = command(&line);
            match cmd.split_whitespace().next().unwrap() {
                "name" => answer(wb, id, n, if kata { "KataGo" } else { "Leela Zero" }),
                "version" => answer(wb, id, n, "1.18.2"),
                "list_commands" => answer(wb, id, n, if kata {
                    "name\nversion\nlist_commands\nboardsize\nclear_board\nkomi\nplay\nundo\nset_position\nkata-analyze\nkata-raw-nn\nstop"
                } else {
                    "name\nversion\nlist_commands\nboardsize\nclear_board\nkomi\nplay\nundo\nlz-analyze\nheatmap"
                }),
                "kata-analyze" | "lz-analyze" => out(wb, id, &format!("={n}")),
                "kata-raw-nn" | "heatmap" => {},
                _ => answer(wb, id, n, ""),
            }
            commands.push(line);
        }
    }
}
fn started(dir: &std::path::Path, kata: bool) -> (Workbench, World, InstanceId) {
    let world = World::default();
    let mut wb = Workbench::new(Storage::in_dirs(&dir.join("config"), &dir.join("data")), None, Box::new(world.clone()), Vec::new());
    // About the foreground engine; automatic quick analysis has its own tests.
    wb.set_auto_quick_analyze(false).unwrap();
    wb.save_engine_settings(vec![EngineProfile {
        name: "Test".into(), command: "/fake/engine gtp".into(), width: 3, height: 3,
        ..EngineProfile::default()
    }], EngineStartup::Default).unwrap();
    wb.start_engines();
    let id = *world.0.lock().unwrap().keys().next().unwrap();
    settle(&mut wb, &world, id, kata);
    (wb, world, id)
}
fn raw_request(commands: &[String]) -> u64 {
    number(commands.iter().find(|s| command(s).starts_with("kata-raw-nn") || command(s) == "heatmap").expect("raw network command"))
}

// KataGo GTP_Extensions.md raw-nn keys and real wire layout. NAN is a legal policy entry;
// scores and ownership are always White's, not the player to move's. Only the fixture's
// numerical values / tiny board are chosen for deterministic assertions.
const KATA_RAW: &str = "symmetry 5\nwhiteWin 0.625\nwhiteLoss 0.375\nnoResult 0\nwhiteLead 2.5\nwhiteScoreSelfplay 2.8\nwhiteScoreSelfplaySq 9.2\nvarTimeLeft 12\nshorttermWinlossError 0.1\nshorttermScoreError 1.0\npolicy\n0.10 0.20 NAN\n0.05 0.15 0.10\n0.10 0.10 0.10\npolicyPass 0\nwhiteOwnership\n-0.9 -0.6 0.2\n-0.3 0 0.7\n0.4 0.8 1";

#[test]
fn katago_cycles_policy_ownership_off_and_uses_raw_instead_of_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, id) = started(dir.path(), true);
    assert!(wb.view(false).raw_net.is_none());
    for mode in [RawNetMode::Policy, RawNetMode::Ownership] {
        assert!(wb.toggle_raw_net());
        let pending = wb.view(false).raw_net.unwrap();
        assert_eq!(pending.mode, mode);
        assert!(pending.policy.is_none());
        let sent = settle(&mut wb, &world, id, true);
        assert!(!sent.iter().any(|s| command(s).starts_with("kata-analyze")));
        let n = raw_request(&sent);
        let symmetry: u8 = command(sent.iter().find(|s| number(s) == n).unwrap()).split_whitespace().nth(1).unwrap().parse().unwrap();
        assert!(symmetry < 8);
        answer(&mut wb, id, n, KATA_RAW);
        let v = wb.view(false).raw_net.unwrap();
        assert_eq!(v.mode, mode);
        assert_eq!(v.policy.unwrap(), [0.1, 0.2, 0.0, 0.05, 0.15, 0.1, 0.1, 0.1, 0.1]);
        assert_eq!(v.white_winrate, Some(0.625));
        assert_eq!(v.white_lead, Some(2.5));
        assert_eq!(v.symmetry, Some(5));
        if mode == RawNetMode::Ownership {
            assert_eq!(v.ownership.unwrap(), [0.9, 0.6, -0.2, 0.3, -0.0, -0.7, -0.4, -0.8, -1.0]);
        } else { assert!(v.ownership.is_none()); }
        assert!(world.take(id).is_empty(), "one evaluation, not an endless raw-nn loop");
    }
    assert!(wb.toggle_raw_net());
    assert!(wb.view(false).raw_net.is_none());
    let sent = settle(&mut wb, &world, id, true);
    assert!(sent.iter().any(|s| command(s).starts_with("kata-analyze")));
}

#[test]
fn stop_resumes_analysis_even_when_h_was_enabled_from_pause() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, id) = started(dir.path(), true);
    wb.toggle_analysis().unwrap();
    settle(&mut wb, &world, id, true);
    wb.toggle_raw_net();
    let n = raw_request(&settle(&mut wb, &world, id, true));
    answer(&mut wb, id, n, KATA_RAW);
    assert!(wb.stop_raw_net());
    assert!(!wb.stop_raw_net());
    assert!(wb.view(false).raw_net.is_none());
    assert!(settle(&mut wb, &world, id, true).iter().any(|s| command(s).starts_with("kata-analyze")));
}

fn open(wb: &mut Workbench, text: &str) {
    let token = wb.begin_open(OpenOrigin::File { path: "/test.sgf".into(), source: OpenSource::Drop });
    assert!(wb.complete_open(token, Ok((RecordFormat::Sgf, sgf::read(text).unwrap()))));
}
#[test]
fn navigation_coalesces_and_discards_superseded_answers_and_keeps_white_ownership_sign() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, id) = started(dir.path(), true);
    open(&mut wb, "(;SZ[3];B[aa];W[bb];B[cc])");
    settle(&mut wb, &world, id, true);
    wb.toggle_raw_net();
    wb.toggle_raw_net(); // ownership, while Policy request is still outstanding
    let old = raw_request(&settle(&mut wb, &world, id, true));
    wb.navigate(Navigation::Forward(1));
    wb.navigate(Navigation::Forward(1));
    wb.navigate(Navigation::Forward(1));
    assert!(world.take(id).is_empty(), "only one batch in flight");
    answer(&mut wb, id, old, KATA_RAW);
    let pending = wb.view(false).raw_net.unwrap();
    assert!(pending.policy.is_none(), "old node is not the new node's payload");
    assert_eq!(pending.node, wb.view(false).position.node);
    let latest = settle(&mut wb, &world, id, true);
    assert_eq!(latest.iter().filter(|s| command(s).starts_with("kata-raw-nn")).count(), 1);
    answer(&mut wb, id, raw_request(&latest), KATA_RAW);
    let view = wb.view(false).raw_net.unwrap();
    assert_eq!(view.mode, RawNetMode::Ownership);
    // After three moves White plays; raw whiteOwnership is still negated, just as on Black.
    assert_eq!(view.ownership.unwrap()[0], 0.9);
}

#[test]
fn return_to_same_node_does_not_revive_an_old_request() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, id) = started(dir.path(), true);
    open(&mut wb, "(;SZ[3];B[aa])");
    settle(&mut wb, &world, id, true);
    wb.toggle_raw_net();
    let n = raw_request(&settle(&mut wb, &world, id, true));
    wb.navigate(Navigation::Forward(1));
    wb.navigate(Navigation::Back(1));
    answer(&mut wb, id, n, KATA_RAW);
    assert!(wb.view(false).raw_net.unwrap().policy.is_none());
    let new = raw_request(&settle(&mut wb, &world, id, true));
    assert_ne!(n, new);
    answer(&mut wb, id, new, KATA_RAW);
    assert!(wb.view(false).raw_net.unwrap().policy.is_some());
}

#[test]
fn leela_heatmap_stderr_and_stdout_ack_can_arrive_in_either_order() {
    for ack_first in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (mut wb, world, id) = started(dir.path(), false);
        wb.toggle_raw_net();
        let sent = settle(&mut wb, &world, id, false);
        assert!(sent.iter().any(|s| command(s) == "heatmap"));
        assert!(!sent.iter().any(|s| command(s).starts_with("lz-analyze")));
        let n = raw_request(&sent);
        if ack_first { answer(&mut wb, id, n, ""); }
        for row in ["100 200 0", "50 150 100", "100 100 100", "pass: 0", "winrate: 0.75"] {
            wb.engine_input(id, EngineInput::Stderr(row.into()));
        }
        if !ack_first {
            assert!(wb.view(false).raw_net.unwrap().policy.is_none());
            answer(&mut wb, id, n, "");
        }
        let view = wb.view(false).raw_net.unwrap();
        assert_eq!(view.mode, RawNetMode::Policy);
        assert_eq!(view.policy.unwrap()[1], 0.2);
        assert_eq!(view.white_winrate, Some(0.25));
        assert!(view.ownership.is_none());
        wb.toggle_raw_net();
        assert!(wb.view(false).raw_net.is_none(), "non-KataGo has no Ownership state");
        assert!(settle(&mut wb, &world, id, false).iter().any(|s| command(s).starts_with("lz-analyze")));
    }
}

#[test]
fn restart_and_process_death_clear_mode_and_old_instance_payloads() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, old_id) = started(dir.path(), true);
    wb.toggle_raw_net();
    let n = raw_request(&settle(&mut wb, &world, old_id, true));
    answer(&mut wb, old_id, n, KATA_RAW);
    assert!(wb.view(false).raw_net.is_some());
    wb.restart_engine().unwrap();
    assert!(wb.view(false).raw_net.is_none());
    answer(&mut wb, old_id, n, KATA_RAW);
    assert!(wb.view(false).raw_net.is_none());
    let id = *world.0.lock().unwrap().keys().max().unwrap();
    settle(&mut wb, &world, id, true);
    wb.toggle_raw_net();
    let n = raw_request(&settle(&mut wb, &world, id, true));
    wb.engine_input(id, EngineInput::Exited(Some(1)));
    assert!(wb.view(false).raw_net.is_none());
    answer(&mut wb, id, n, KATA_RAW);
    assert!(wb.view(false).raw_net.is_none());
    assert!(!wb.toggle_raw_net());
    assert!(!wb.stop_raw_net());
}

#[test]
fn raw_parser_accepts_permuted_whitespace_and_rejects_incomplete_payload() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, id) = started(dir.path(), true);
    wb.toggle_raw_net();
    let n = raw_request(&settle(&mut wb, &world, id, true));
    answer(&mut wb, id, n, "symmetry 4 whiteLead -1 unknownFutureKey 12 3 whiteWin 0.4 policy 0.1 0.2");
    assert!(wb.view(false).raw_net.unwrap().policy.is_none());
    wb.toggle_raw_net();
    let n = raw_request(&settle(&mut wb, &world, id, true));
    answer(&mut wb, id, n, "whiteOwnership -1 -0.8 -0.6 -0.4 -0.2 0 0.2 0.4 1 whiteLead -1 policy 0.1 0.1 0.1 0.1 0.1 0.1 0.1 0.1 0.1 symmetry 4 whiteWin 0.4");
    let v = wb.view(false).raw_net.unwrap();
    assert_eq!(v.symmetry, Some(4));
    assert_eq!(v.ownership.unwrap()[0], 1.0);
}

#[test]
fn switching_away_and_back_does_not_restore_instance_heatmap_state() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, old) = started(dir.path(), true);
    let mut settings = wb.view(false).settings.engine.engines;
    settings.push(EngineProfile {
        name: "Other".into(), command: "/fake/second gtp".into(), width: 3, height: 3,
        ..EngineProfile::default()
    });
    wb.save_engine_settings(settings, EngineStartup::Default).unwrap();
    wb.toggle_raw_net();
    let n = raw_request(&settle(&mut wb, &world, old, true));
    wb.switch_engine(1).unwrap();
    assert!(wb.view(false).raw_net.is_none());
    answer(&mut wb, old, n, KATA_RAW);
    assert!(wb.view(false).raw_net.is_none());
    let new = *world.0.lock().unwrap().keys().max().unwrap();
    settle(&mut wb, &world, new, true);
    wb.switch_engine(0).unwrap();
    assert!(wb.view(false).raw_net.is_none());
}

#[test]
fn off_discards_an_inflight_answer_then_resumes_the_existing_analysis_path() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, id) = started(dir.path(), true);
    wb.toggle_raw_net();
    let n = raw_request(&settle(&mut wb, &world, id, true));
    assert!(wb.stop_raw_net());
    assert!(wb.view(false).raw_net.is_none());
    assert!(world.take(id).is_empty(), "Off still respects the current command fence");
    answer(&mut wb, id, n, KATA_RAW);
    assert!(wb.view(false).raw_net.is_none());
    assert!(settle(&mut wb, &world, id, true).iter().any(|s| command(s).starts_with("kata-analyze")));
}

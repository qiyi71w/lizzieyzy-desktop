//! Single-record Java automatic analysis through the same public Workbench interface as Tauri.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use lizzie_core::api::{FlashTarget, OpenSource};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats::sgf;
use lizzie_core::record::RecordFormat;
use lizzie_core::settings::{AnalysisEngineSettings, AutoAnalysisSettings, Storage};
use lizzie_core::workbench::{OpenOrigin, Workbench};

#[derive(Clone, Default)]
struct World(Arc<Mutex<Vec<(InstanceId, Vec<String>)>>>);
struct Io(World, InstanceId);
impl EngineIo for Io {
    fn send(&mut self, text: &str) -> std::io::Result<()> {
        self.0.0.lock().unwrap().iter_mut().find(|p| p.0 == self.1).unwrap().1.push(text.into()); Ok(())
    }
    fn terminate(self: Box<Self>) {}
}
impl Launcher for World {
    fn launch(&self, id: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        self.0.lock().unwrap().push((id, vec![])); Ok(Box::new(Io(self.clone(), id)))
    }
}
fn out(wb: &mut Workbench, id: InstanceId, text: &str) { wb.engine_input(id, EngineInput::Stdout(text.into())); }
const COMMANDS: &str = "name\nversion\nlist_commands\nboardsize\nclear_board\nkomi\nplay\nundo\nset_position\nkata-analyze\nkata-set-rules\nkata-get-rules\nstop";
fn serve(wb: &mut Workbench, world: &World, id: InstanceId) {
    loop {
        let pending = {
            let mut p = world.0.lock().unwrap();
            std::mem::take(&mut p.iter_mut().find(|p| p.0 == id).unwrap().1)
        };
        if pending.is_empty() { return; }
        for text in pending {
            if text.starts_with('{') { continue; }
            out(wb, id, "");
            let (n, command) = text.split_once(' ').unwrap();
            let value = match command.split(' ').next().unwrap() {
                "name" => "KataGo", "version" => "1.18.2", "list_commands" => COMMANDS,
                "kata-get-rules" => r#"{"ko":"SIMPLE","scoring":"AREA","tax":"NONE","suicide":false,"whiteHandicapBonus":"N","friendlyPassOk":true,"hasButton":false}"#,
                _ => "",
            };
            out(wb, id, &format!("={n} {value}"));
            if !command.starts_with("kata-analyze") { out(wb, id, ""); }
        }
    }
}
fn info(wb: &mut Workbench, id: InstanceId, first: u64, second: u64, win: f64) {
    out(wb, id, &format!("info move Q16 visits {first} winrate {win} scoreLead 0 prior 0.5 order 0 pv Q16 info move D4 visits {second} winrate {win} scoreLead 0 prior 0.5 order 1 pv D4"));
}
fn open(wb: &mut Workbench, text: &str) {
    let t = wb.begin_open(OpenOrigin::File { path: "/games/test.sgf".into(), source: OpenSource::Drop });
    wb.complete_open(t, Ok((RecordFormat::Sgf, sgf::read(text).unwrap())));
}
fn setup(dir: &std::path::Path, text: &str) -> (Workbench, World, InstanceId) {
    let world = World::default();
    let mut wb = Workbench::new(Storage::in_dirs(&dir.join("config"), &dir.join("data")), Some("zh-CN"), Box::new(world.clone()), vec![]);
    wb.set_auto_quick_analyze(false).unwrap();
    wb.save_engine_settings(vec![EngineProfile { name: "test".into(), command: "katago gtp".into(), ..Default::default() }], EngineStartup::Empty).unwrap();
    open(&mut wb, text);
    wb.switch_engine(0).unwrap();
    let id = world.0.lock().unwrap()[0].0;
    serve(&mut wb, &world, id);
    (wb, world, id)
}
fn settings() -> AutoAnalysisSettings {
    AutoAnalysisSettings { visits_enabled: true, visits: 10, time_enabled: false, auto_save: false, ..Default::default() }
}
const LINE: &str = "(;SZ[19];B[pd];W[dd];B[pp];W[dp])";

#[test]
fn advances_by_total_and_first_candidate_visits_not_stored_cache() {
    for first_only in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let (mut wb, world, id) = setup(dir.path(), LINE);
        info(&mut wb, id, 500, 0, 0.5); // stronger pre-existing cache must not satisfy a new run
        let mut s = settings(); s.visits_enabled = !first_only; s.first_visits_enabled = first_only; s.first_visits = 7;
        wb.start_auto_analysis(s).unwrap().unwrap(); serve(&mut wb, &world, id);
        info(&mut wb, id, 6, 3, 0.5);
        assert_eq!(wb.view(true).position.node, 0);
        if first_only { info(&mut wb, id, 7, 0, 0.5); } else { info(&mut wb, id, 6, 4, 0.5); }
        assert_eq!(wb.view(true).position.node, 1);
        wb.goto_node(0).unwrap();
        assert_eq!(wb.view(true).analysis.unwrap().visits, if first_only { 7 } else { 10 });
        assert!(!wb.document().unsaved());
    }
}

#[test]
fn advances_on_elapsed_time_only_after_foreground_packet() {
    let dir = tempfile::tempdir().unwrap(); let (mut wb, world, id) = setup(dir.path(), LINE);
    let s = AutoAnalysisSettings { time_enabled: true, time_seconds: 1, auto_save: false, ..Default::default() };
    wb.start_auto_analysis(s).unwrap().unwrap(); serve(&mut wb, &world, id);
    info(&mut wb, id, 1, 0, 0.5); assert_eq!(wb.view(true).position.node, 0);
    std::thread::sleep(Duration::from_millis(1050));
    info(&mut wb, id, 2, 0, 0.5); assert_eq!(wb.view(true).position.node, 1);
}

#[test]
fn auto_analysis_excludes_whole_game_and_hawkeye_list_navigation() {
    use lizzie_core::api::HawkeyeCommand;
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, id) = setup(dir.path(), LINE);
    wb.hawkeye(HawkeyeCommand::Toggle).unwrap();
    wb.start_auto_analysis(settings()).unwrap().unwrap();
    serve(&mut wb, &world, id);
    assert_eq!(wb.start_whole_game_analysis(1000).unwrap(), Err("WholeGameAnalysis.conflict".into()));
    // A list row is ignored (`isAutoAna`), a chart position is not (`goToMoveNumberBeyondBranch`).
    let document = wb.document().id;
    assert_eq!(wb.hawkeye(HawkeyeCommand::GotoRow { document, node: 3 }), Ok(false));
    assert_eq!(wb.view(true).position.node, 0);
    assert_eq!(wb.hawkeye(HawkeyeCommand::GotoMoveNumber { document, number: 2 }), Ok(true));
    assert_eq!(wb.view(true).position.node, 2);
    wb.stop_auto_analysis();
    assert!(!wb.view(true).auto_analysis.active);
    wb.goto_node(0).unwrap();
    assert_eq!(wb.hawkeye(HawkeyeCommand::GotoRow { document, node: 3 }), Ok(true));
    assert_eq!(wb.view(true).position.node, 2);
}

#[test]
fn start_end_side_skips_and_line_end_keep_cursor_and_results() {
    let dir = tempfile::tempdir().unwrap(); let (mut wb, world, id) = setup(dir.path(), LINE);
    let mut s = settings(); s.start_move = 2; s.end_move = 2; s.white = false;
    wb.start_auto_analysis(s).unwrap().unwrap(); serve(&mut wb, &world, id);
    assert_eq!(wb.view(true).position.node, 1); // start-1, White to play, skipped on first packet
    info(&mut wb, id, 1, 0, 0.5); serve(&mut wb, &world, id);
    assert_eq!(wb.view(true).position.node, 2);
    info(&mut wb, id, 10, 0, 0.5); serve(&mut wb, &world, id);
    assert_eq!(wb.view(true).position.node, 3); // Java checks end < current on next update
    info(&mut wb, id, 1, 0, 0.5);
    let view = wb.view(true); assert!(!view.auto_analysis.active); assert_eq!(view.position.node, 3);
    assert_eq!(view.auto_analysis.notice.unwrap().message_key, "BottomToolbar.stopAutoAnaHint");
    let mut s = settings(); s.start_move = 5;
    wb.start_auto_analysis(s).unwrap().unwrap(); serve(&mut wb, &world, id);
    info(&mut wb, id, 10, 0, 0.5);
    let view = wb.view(true); assert!(!view.auto_analysis.active); assert_eq!(view.position.node, 4); assert!(view.engine.paused);
}

#[test]
fn all_branches_are_reverse_child_depth_first() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, id) = setup(dir.path(), "(;SZ[19];B[pd](;W[dd];B[pp])(;W[dp];B[dc]))");
    let mut s = settings(); s.all_branches = true;
    wb.start_auto_analysis(s).unwrap().unwrap();
    let mut order = vec![];
    while wb.view(true).auto_analysis.active {
        serve(&mut wb, &world, id); order.push(wb.view(true).position.node); info(&mut wb, id, 10, 0, 0.5);
    }
    assert_eq!(order, vec![0, 1, 4, 5, 2, 3]);
}

#[test]
fn nodes_deleted_after_planning_are_skipped_in_branches_and_strengthening() {
    use lizzie_core::record::DocumentEdit;
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, id) = setup(dir.path(), "(;SZ[19];B[pd](;W[dd];B[pp])(;W[dp];B[dc]))");
    let mut s = settings(); s.all_branches = true;
    wb.start_auto_analysis(s).unwrap().unwrap();
    serve(&mut wb, &world, id);
    assert!(wb.edit_document(DocumentEdit::Remove { node: 4 }).unwrap());
    let mut order = vec![];
    for _ in 0..6 {
        if !wb.view(true).auto_analysis.active { break; }
        serve(&mut wb, &world, id); order.push(wb.view(true).position.node); info(&mut wb, id, 10, 0, 0.5);
    }
    assert_eq!(order, vec![0, 1, 2, 3]);
    assert!(!wb.view(true).auto_analysis.active);

    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, id) = setup(dir.path(), "(;SZ[19];B[pd];W[dd])");
    let mut s = settings(); s.diff_enable = true; s.diff_time = -1; s.diff_visits = 20;
    wb.start_auto_analysis(s).unwrap().unwrap(); serve(&mut wb, &world, id);
    info(&mut wb, id, 10, 0, 0.8); serve(&mut wb, &world, id);
    info(&mut wb, id, 10, 0, 0.8); serve(&mut wb, &world, id);
    wb.stop_auto_analysis();
    assert_eq!(wb.view(true).auto_analysis.diff_nodes, 2); // root and move 1; move 2 had no result yet
    // The confirmation leaves the root queued first; the rest of the plan is deleted meanwhile.
    wb.goto_node(0).unwrap();
    assert!(wb.edit_document(DocumentEdit::Remove { node: 1 }).unwrap());
    wb.answer_auto_analysis_diff(true);
    serve(&mut wb, &world, id);
    assert_eq!(wb.view(true).position.node, 0);
    info(&mut wb, id, 20, 0, 0.5);
    let view = wb.view(true);
    assert!(!view.auto_analysis.active);
    assert_eq!(view.position.node, 0);
}

#[test]
fn forced_stop_keeps_position_and_pause_exit_respects_setting() {
    for exit in [false, true] {
        let dir = tempfile::tempdir().unwrap(); let (mut wb, world, id) = setup(dir.path(), LINE);
        let mut s = settings(); s.exit_by_pause = exit;
        wb.start_auto_analysis(s).unwrap().unwrap(); serve(&mut wb, &world, id);
        info(&mut wb, id, 3, 0, 0.5);
        wb.toggle_analysis().unwrap();
        let view = wb.view(true); assert_eq!(view.auto_analysis.active, !exit); assert_eq!(view.auto_analysis.pause_tip, exit); assert_eq!(view.position.node, 0);
        if exit { wb.dismiss_auto_analysis_tip(true).unwrap(); assert!(!wb.config().auto_analysis.exit_tip); }
        else { wb.toggle_analysis().unwrap(); serve(&mut wb, &world, id); info(&mut wb, id, 10, 0, 0.5); }
        let at = wb.view(true).position.node;
        if !exit { assert!(wb.stop_auto_analysis()); assert_eq!(wb.view(true).auto_analysis.notice.unwrap().message_key, "BottomToolbar.stopAutoAnaStoppedHint"); }
        assert_eq!(wb.view(true).position.node, at);
        wb.goto_node(0).unwrap(); assert!(wb.view(true).analysis.is_some());
    }
}

#[test]
fn diff_collects_pairs_deduplicates_and_asks_only_on_forced_stop() {
    for forced in [false, true] {
        let dir = tempfile::tempdir().unwrap(); let (mut wb, world, id) = setup(dir.path(), "(;SZ[19];B[pd];W[dd])");
        let mut s = settings(); s.diff_enable = true; s.diff_time = -1; s.diff_visits = 20;
        wb.start_auto_analysis(s).unwrap().unwrap(); serve(&mut wb, &world, id);
        info(&mut wb, id, 10, 0, 0.8); serve(&mut wb, &world, id);
        info(&mut wb, id, 10, 0, 0.8); serve(&mut wb, &world, id);
        if forced { wb.stop_auto_analysis(); } else { info(&mut wb, id, 10, 0, 0.8); }
        let view = wb.view(true);
        assert_eq!(view.auto_analysis.confirmation, forced);
        assert!(view.auto_analysis.diff_nodes >= 2);
        if forced { assert!(!view.auto_analysis.active); wb.answer_auto_analysis_diff(true); }
        assert!(wb.view(true).auto_analysis.diff);
        let count = wb.view(true).auto_analysis.diff_nodes;
        for _ in 0..count {
            serve(&mut wb, &world, id); info(&mut wb, id, 19, 0, 0.5); assert!(wb.view(true).auto_analysis.active);
            info(&mut wb, id, 20, 0, 0.5);
        }
        assert!(!wb.view(true).auto_analysis.active);
    }
}

#[test]
fn declining_diff_confirmation_finishes_without_reanalysis() {
    let dir = tempfile::tempdir().unwrap(); let (mut wb, world, id) = setup(dir.path(), LINE);
    let mut s = settings(); s.diff_enable = true;
    wb.start_auto_analysis(s).unwrap().unwrap(); serve(&mut wb, &world, id);
    info(&mut wb, id, 10, 0, 0.8); serve(&mut wb, &world, id); info(&mut wb, id, 10, 0, 0.8);
    wb.stop_auto_analysis(); assert!(wb.view(true).auto_analysis.confirmation);
    let at = wb.view(true).position.node; wb.answer_auto_analysis_diff(false);
    assert_eq!(wb.view(true).position.node, at); assert!(!wb.view(true).auto_analysis.diff);
}

#[test]
fn flash_conflict_and_quick_process_handover() {
    let dir = tempfile::tempdir().unwrap(); let (mut wb, world, id) = setup(dir.path(), LINE);
    wb.save_analysis_engine_settings(AnalysisEngineSettings { command: "katago analysis".into(), ..Default::default() }, "", true).unwrap().unwrap();
    wb.start_flash_analysis(FlashTarget::WholeGame).unwrap().unwrap();
    assert_eq!(wb.start_auto_analysis(settings()).unwrap(), Err("AutoAnalyze.conflict"));
    wb.cancel_flash_analysis();
    wb.set_auto_quick_analyze(true).unwrap(); open(&mut wb, LINE); serve(&mut wb, &world, id);
    wb.engine_tick(Instant::now() + Duration::from_secs(2));
    wb.start_auto_analysis(settings()).unwrap().unwrap();
    let view = wb.view(true); assert!(view.auto_analysis.active); assert!(!view.quick.running);
}

#[test]
fn document_switch_and_engine_switch_drop_old_run() {
    let dir = tempfile::tempdir().unwrap(); let (mut wb, world, id) = setup(dir.path(), LINE);
    wb.start_auto_analysis(settings()).unwrap().unwrap(); serve(&mut wb, &world, id);
    open(&mut wb, "(;SZ[9];B[aa])"); info(&mut wb, id, 100, 0, 0.5);
    let view = wb.view(true); assert!(!view.auto_analysis.active); assert!(view.analysis.is_none()); assert_eq!(view.position.node, 0);
    serve(&mut wb, &world, id);
    wb.start_auto_analysis(settings()).unwrap().unwrap();
    wb.restart_engine().unwrap(); assert!(!wb.view(true).auto_analysis.active);
}

#[test]
fn autosave_is_a_captured_copy_and_settings_survive_restart() {
    let dir = tempfile::tempdir().unwrap(); let (mut wb, world, id) = setup(dir.path(), LINE);
    assert_eq!(wb.auto_analysis_output("20261022123456", "已分析").to_string_lossy(), "/games/test_已分析_20261022123456.sgf");
    let mut s = settings(); s.auto_save = true; s.exit_tip = false;
    wb.start_auto_analysis(s.clone()).unwrap().unwrap(); serve(&mut wb, &world, id);
    info(&mut wb, id, 10, 0, 0.5); wb.stop_auto_analysis();
    let jobs = wb.take_auto_analysis_saves(); assert_eq!(jobs.len(), 1);
    assert!(jobs[0].create_parent); assert!(jobs[0].path.to_string_lossy().contains("test_已分析_"));
    // Only the root was searched: Java saves root/setup analysis as LZOP, not move-node LZ.
    let captured = sgf::read(&jobs[0].text).unwrap();
    let saved = lizzie_core::formats::saved_analysis::primary(&captured.node(0).properties).expect("captured root analysis");
    assert_eq!(saved.visits, 10);
    assert!(wb.complete_save(jobs[0].ticket, Ok(())));
    assert_eq!(wb.document().path.as_deref(), Some("/games/test.sgf"));
    assert_eq!(wb.view(true).auto_analysis.notice.unwrap().message_key, "Leelaz.autoAnalyzeStoppedSaved");
    drop(wb);
    let wb = Workbench::new(Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data")), None, Box::new(world), vec![]);
    assert_eq!(wb.config().auto_analysis, s);
    assert_eq!(wb.auto_analysis_output("20261022123456", "已分析"), dir.path().join("data/AnalyzedGames/20261022123456.sgf"));
}

#[test]
fn validation_and_stop_before_first_result_do_not_save() {
    let dir = tempfile::tempdir().unwrap(); let (mut wb, world, id) = setup(dir.path(), LINE);
    let mut s = settings(); s.black = false; s.white = false;
    assert_eq!(wb.start_auto_analysis(s).unwrap(), Err("SetDiffAnalyze.noBlackWhite"));
    assert_eq!(wb.start_auto_analysis(AutoAnalysisSettings { time_enabled: false, ..Default::default() }).unwrap(), Err("SetDiffAnalyze.noCondition"));
    let mut s = settings(); s.auto_save = true;
    wb.start_auto_analysis(s).unwrap().unwrap(); serve(&mut wb, &world, id); wb.stop_auto_analysis();
    assert!(wb.take_auto_analysis_saves().is_empty());
}

#[test]
fn borrowed_quick_analysis_returns_before_automatic_analysis_starts() {
    let dir = tempfile::tempdir().unwrap(); let (mut wb, world, id) = setup(dir.path(), LINE);
    wb.save_analysis_engine_settings(AnalysisEngineSettings { reuse_current_engine: true, ..Default::default() }, "", false).unwrap().unwrap();
    wb.set_auto_quick_analyze(true).unwrap(); open(&mut wb, LINE); serve(&mut wb, &world, id);
    wb.engine_tick(Instant::now() + Duration::from_secs(2)); serve(&mut wb, &world, id);
    assert!(wb.view(true).quick.borrowed);
    let mut s = settings(); s.exit_by_pause = false;
    wb.start_auto_analysis(s).unwrap().unwrap();
    assert!(wb.view(true).auto_analysis.starting);
    wb.toggle_analysis().unwrap();
    serve(&mut wb, &world, id);
    assert!(wb.view(true).auto_analysis.active); assert!(!wb.view(true).auto_analysis.starting);
    assert!(wb.view(true).engine.paused, "returning a quick engine cannot undo the user's pause");
    wb.toggle_analysis().unwrap(); serve(&mut wb, &world, id);
    assert_eq!(world.0.lock().unwrap().len(), 1, "automatic analysis never launches another engine");
    info(&mut wb, id, 10, 0, 0.5); assert_eq!(wb.view(true).position.node, 1);
}

#[test]
fn score_diff_obeys_mover_filter_and_stop_during_strengthening_is_final() {
    let dir = tempfile::tempdir().unwrap(); let (mut wb, world, id) = setup(dir.path(), "(;SZ[19];B[pd];W[dd])");
    let mut s = settings(); s.diff_enable = true; s.diff_use_win = false; s.diff_use_score = true; s.diff_score_threshold = 3.0; s.diff_black = false;
    wb.start_auto_analysis(s).unwrap().unwrap();
    for _ in 0..3 {
        serve(&mut wb, &world, id);
        out(&mut wb, id, "info move Q16 visits 10 winrate 0.5 scoreLead 5 prior 0.5 order 0 pv Q16");
    }
    let view = wb.view(true); assert!(view.auto_analysis.diff); assert_eq!(view.auto_analysis.diff_moves, 1); assert_eq!(view.auto_analysis.diff_nodes, 2);
    wb.stop_auto_analysis(); let view = wb.view(true); assert!(!view.auto_analysis.active && !view.auto_analysis.confirmation);
}

#[test]
fn autosave_summary_includes_the_final_moves_and_never_runs_without_autosave() {
    use lizzie_core::record::DocumentEdit;
    // (auto-save, 保存时附加胜率到评论, the user edited the record before)
    for (save, append, edited) in [(false, true, false), (true, true, false), (true, false, false), (true, true, true)] {
        let mut record = String::from("(;SZ[19]");
        for i in 0..20 { record.push_str(&format!(";{}[{}{}]", if i % 2 == 0 { "B" } else { "W" }, (b'a' + (i % 19) as u8) as char, (b'a' + (i / 19) as u8) as char)); }
        record.push(')');
        let dir = tempfile::tempdir().unwrap(); let (mut wb, world, id) = setup(dir.path(), &record);
        wb.set_append_winrate_to_comment(append).unwrap();
        if edited { wb.edit_document(DocumentEdit::SetComment { node: 3, comment: "mine".into() }).unwrap(); }
        assert_eq!(wb.document().unsaved(), edited);
        let mut s = settings(); s.auto_save = save;
        wb.start_auto_analysis(s).unwrap().unwrap();
        for _ in 0..21 { serve(&mut wb, &world, id); info(&mut wb, id, 10, 0, 0.5); }
        assert!(!wb.view(true).auto_analysis.active);
        // Like Java, the summary is in the open record too, which keeps its saved state.
        assert_eq!(wb.document().record.node(0).comment.contains("黑吻合度:"), save);
        assert_eq!(wb.document().unsaved(), edited, "analysis never marks the record unsaved");
        let jobs = wb.take_auto_analysis_saves();
        assert_eq!(jobs.len(), usize::from(save));
        if let Some(job) = jobs.first() {
            assert!(job.text.contains("黑吻合度:") && job.text.contains("白吻合度:"));
            assert!(wb.complete_save(job.ticket, Ok(())));
            assert_eq!(wb.document().unsaved(), edited, "the analysed copy confirms nothing");
        }
    }
}

#[test]
fn engine_failure_ends_auto_without_saving_or_adopting_late_results() {
    let dir = tempfile::tempdir().unwrap(); let (mut wb, world, id) = setup(dir.path(), LINE);
    let mut s = settings(); s.auto_save = true;
    wb.start_auto_analysis(s).unwrap().unwrap(); serve(&mut wb, &world, id);
    info(&mut wb, id, 10, 0, 0.5); serve(&mut wb, &world, id);
    wb.engine_input(id, EngineInput::Exited(Some(1)));
    assert!(!wb.view(true).auto_analysis.active); assert!(wb.take_auto_analysis_saves().is_empty());
    info(&mut wb, id, 1000, 0, 0.5); assert_eq!(wb.view(true).position.node, 1);
}

#[test]
fn failed_quick_handback_refuses_auto_and_keeps_the_foreground_quarantined() {
    let dir = tempfile::tempdir().unwrap(); let (mut wb, world, id) = setup(dir.path(), LINE);
    wb.save_analysis_engine_settings(AnalysisEngineSettings { reuse_current_engine: true, ..Default::default() }, "", false).unwrap().unwrap();
    wb.set_auto_quick_analyze(true).unwrap(); open(&mut wb, LINE); serve(&mut wb, &world, id);
    wb.engine_tick(Instant::now() + Duration::from_secs(2)); serve(&mut wb, &world, id);
    assert!(wb.view(true).quick.borrowed);
    wb.start_auto_analysis(settings()).unwrap().unwrap();
    let pending = {
        let mut p = world.0.lock().unwrap();
        std::mem::take(&mut p.iter_mut().find(|p| p.0 == id).unwrap().1)
    };
    let stop = pending.iter().find(|line| line.split_once(' ').is_some_and(|(_, command)| command == "stop")).expect("active-search stop");
    let n = stop.split_once(' ').unwrap().0;
    // Ending a running search first stops that search. Its failure does not establish a
    // failed handback: the runtime still sends the separate, confirmed final release stop.
    out(&mut wb, id, ""); out(&mut wb, id, &format!("?{n} refused search stop")); out(&mut wb, id, "");
    assert!(wb.view(true).auto_analysis.starting, "wait for the final release boundary");
    let pending = {
        let mut p = world.0.lock().unwrap();
        std::mem::take(&mut p.iter_mut().find(|p| p.0 == id).unwrap().1)
    };
    let final_stop = pending.iter().find(|line| line.split_once(' ').is_some_and(|(_, command)| command == "stop")).expect("lease final release stop");
    let final_n = final_stop.split_once(' ').unwrap().0;
    assert_ne!(n, final_n);
    out(&mut wb, id, &format!("?{final_n} refused final release stop")); out(&mut wb, id, "");
    let view = wb.view(true);
    assert!(!view.auto_analysis.active); assert!(view.engine.unrestored.is_some());
    assert_eq!(view.auto_analysis.notice.unwrap().message_key, "AutoAnalyze.releaseFailed");
    assert!(wb.take_auto_analysis_saves().is_empty());
}

#[test]
fn unavailable_engine_and_failed_autosave_have_java_messages() {
    let dir = tempfile::tempdir().unwrap(); let (mut wb, world, id) = setup(dir.path(), LINE);
    let mut s = settings(); s.auto_save = true;
    wb.start_auto_analysis(s).unwrap().unwrap(); serve(&mut wb, &world, id);
    info(&mut wb, id, 10, 0, 0.5); wb.stop_auto_analysis();
    let job = wb.take_auto_analysis_saves().remove(0);
    wb.complete_save(job.ticket, Err("read-only directory".into()));
    assert_eq!(wb.view(true).auto_analysis.notice.unwrap().message_key, "LizzieFrame.saveFileFailed");
    wb.shutdown_engines(lizzie_core::api::EngineScope::Current);
    assert_eq!(wb.start_auto_analysis(settings()).unwrap(), Err("AutoAnalyze.engineUnavailable"));
}

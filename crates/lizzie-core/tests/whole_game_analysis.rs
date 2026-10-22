//! WholeGameAnalysisSession's public behavior with controlled analysis/GTP processes.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use lizzie_core::api::{EditCommand, FlashTarget, OpenSource, WholeGameState as State};
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats::sgf;
use lizzie_core::record::{DocumentEdit, RecordFormat};
use lizzie_core::settings::{AnalysisEngineSettings, Storage, WholeGameSettings};
use lizzie_core::workbench::{OpenOrigin, Workbench};
use serde_json::{json, Value};

#[derive(Default)]
struct Process { id: InstanceId, args: Vec<String>, sent: Vec<String>, ended: bool }
#[derive(Clone, Default)]
struct World(Arc<Mutex<Vec<Process>>>);
impl World {
    fn take(&self, id: InstanceId) -> Vec<String> {
        std::mem::take(&mut self.0.lock().unwrap().iter_mut().find(|p| p.id == id).unwrap().sent)
    }
    fn worker(&self) -> InstanceId {
        self.0.lock().unwrap().iter().rev().find(|p| p.args.first().is_some_and(|a| a == "analysis")).unwrap().id
    }
    fn ended(&self, id: InstanceId) -> bool { self.0.lock().unwrap().iter().find(|p| p.id == id).unwrap().ended }
    fn queries(&self, id: InstanceId) -> Vec<Value> { self.take(id).into_iter().map(|s| serde_json::from_str(&s).unwrap()).collect() }
}
struct Io(World, InstanceId);
impl EngineIo for Io {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        self.0.0.lock().unwrap().iter_mut().find(|p| p.id == self.1).unwrap().sent.push(line.into()); Ok(())
    }
    fn terminate(self: Box<Self>) { self.0.0.lock().unwrap().iter_mut().find(|p| p.id == self.1).unwrap().ended = true; }
}
struct Launch(World);
impl Launcher for Launch {
    fn launch(&self, id: InstanceId, spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        if spec.program == "missing-katago" { return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "missing")); }
        self.0.0.lock().unwrap().push(Process { id, args: spec.args.clone(), ..Process::default() });
        Ok(Box::new(Io(self.0.clone(), id)))
    }
}
const RECORD: &str = "(;SZ[19];B[pd];W[dd];B[pp];W[dp])";
const KATA: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nset_position\nkata-analyze\nkata-set-rules\nkata-get-rules\nstop";
const RULES: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;
fn make(path: &std::path::Path, world: &World) -> Workbench {
    let mut wb = Workbench::new(Storage::in_dirs(&path.join("config"), &path.join("data")), None, Box::new(Launch(world.clone())), vec![]);
    wb.set_auto_quick_analyze(false).unwrap();
    wb.save_analysis_engine_settings(AnalysisEngineSettings { command: "/opt/katago analysis -config /a.cfg".into(), command_customized: true, ..AnalysisEngineSettings::default() }, "", true).unwrap().unwrap();
    wb
}
fn open(wb: &mut Workbench, text: &str) {
    let ticket = wb.begin_open(OpenOrigin::File { path: "/x.sgf".into(), source: OpenSource::Drop });
    assert!(wb.complete_open(ticket, Ok((RecordFormat::Sgf, sgf::read(text).unwrap()))));
}
fn output(wb: &mut Workbench, id: InstanceId, text: &str) { wb.engine_input(id, EngineInput::Stdout(text.into())); }
fn answer(wb: &mut Workbench, worker: InstanceId, q: &Value) {
    let visits = q["maxVisits"].as_u64().unwrap();
    let mut value = json!({"id":q["id"], "moveInfos":[{"move":"K10", "visits":visits, "winrate":0.5, "scoreLead":1.0, "order":0,"pv":["K10"]}], "rootInfo":{"visits":visits,"winrate":0.5,"scoreLead":1.0}});
    if q["includeOwnership"] == true { value["ownership"] = json!(vec![0.2;361]); }
    output(wb, worker, &value.to_string());
}
fn finish(wb: &mut Workbench, world: &World, worker: InstanceId) {
    loop {
        let qs = world.queries(worker); if qs.is_empty() { return; }
        for q in qs { answer(wb, worker, &q); }
    }
}
fn start(wb: &mut Workbench) { assert_eq!(wb.start_whole_game_analysis(1000).unwrap(), Ok(true)); }
fn serve(wb: &mut Workbench, world: &World, engine: InstanceId, visits: u64) -> Vec<String> {
    let mut result = vec![];
    loop {
        let pending = world.take(engine); if pending.is_empty() { return result; }
        for line in pending {
            output(wb, engine, "");
            let (id, cmd) = line.split_once(' ').unwrap();
            let text = match cmd.split(' ').next().unwrap() {
                "name" => "KataGo", "version" => "1.18.2", "list_commands" => KATA, "kata-get-rules" => RULES, _ => "",
            };
            if cmd.starts_with("kata-analyze") {
                // KataGo's response stays open until stop. A blank here would end/refuse the
                // stream, and any subsequent info would be unattributable to this request.
                output(wb, engine, &format!("={id}"));
                if visits > 0 {
                    output(wb, engine, &format!("info move K10 visits {visits} winrate 0.5 scoreLead 0.5 prior 0.2 order 0 pv K10 rootInfo visits {visits} winrate 0.4 scoreLead -0.5"));
                }
            } else {
                output(wb, engine, &format!("={id} {text}")); output(wb, engine, "");
            }
            result.push(cmd.to_string());
        }
    }
}
fn foreground(wb: &mut Workbench, world: &World) -> InstanceId {
    wb.save_engine_settings(vec![EngineProfile { name:"KataGo".into(), command:"/opt/katago gtp".into(), ..EngineProfile::default() }], EngineStartup::Default).unwrap();
    wb.start_engines(); let id = world.0.lock().unwrap().last().unwrap().id; serve(wb, world, id, 0); id
}
fn borrow(wb: &mut Workbench) {
    let mut s = wb.config().analysis_engine.clone(); s.reuse_current_engine = true;
    wb.save_analysis_engine_settings(s, "", true).unwrap().unwrap();
}
fn foreground_searches(commands: &[String]) -> bool { commands.iter().any(|s| s.starts_with("kata-analyze") && !s.contains("maxmoves")) }

#[test]
fn baseline_then_deep_use_exact_visits_progress_and_clean_result_storage() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    open(&mut wb, RECORD); let revision = wb.document().revision; start(&mut wb);
    let worker = world.worker(); let qs = world.queries(worker); assert_eq!(qs.len(), 5, "the root and four moves");
    assert!(qs[0]["moves"].as_array().unwrap().is_empty(), "the root position is planned first");
    assert!(qs.iter().all(|q| q["maxVisits"] == 32));
    answer(&mut wb, worker, &qs[0]); assert_eq!(wb.view(false).whole_game.percent, 4);
    for q in &qs[1..] { answer(&mut wb, worker, q); }
    let deep = world.queries(worker); assert_eq!(deep.len(), 5); assert!(deep.iter().all(|q| q["maxVisits"] == 1000));
    let v = wb.view(false).whole_game; assert_eq!((v.state,v.percent,v.completed),(State::Deep,20,0));
    answer(&mut wb, worker, &deep[0]); assert_eq!(wb.view(false).whole_game.percent,36);
    for q in &deep[1..] { answer(&mut wb, worker, q); }
    let v = wb.view(false); assert_eq!((v.whole_game.state,v.whole_game.percent),(State::Complete,100));
    assert_eq!(wb.document().revision,revision); assert!(!wb.document().unsaved()); assert!(world.ended(worker));
    wb.navigate(Navigation::Last); let v = wb.view(false); assert!(v.whole_game.show_results);
    assert_eq!(v.analysis.unwrap().engine,"整盘精析");
}
#[test]
fn only_real_main_trunk_actions_are_planned_and_browsing_is_free() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    open(&mut wb,"(;SZ[19];B[pd];W[];DD[true](;B[pp])(;B[dp]))"); start(&mut wb);
    // DD attaches a dummy first child to W; the B continuations are later siblings. The main
    // trunk ends at that placeholder, so only the root, B[pd] and the real W pass are requested.
    let worker = world.worker(); let qs = world.queries(worker); assert_eq!(qs.len(),3);
    assert_eq!(qs[2]["moves"].as_array().unwrap().last().unwrap()[1], "pass");
    wb.navigate(Navigation::Last); assert_ne!(wb.cursor(),0); assert_eq!(wb.view(false).whole_game.state,State::Baseline);
    for q in qs { answer(&mut wb,worker,&q); } finish(&mut wb,&world,worker);
    assert_eq!(wb.view(false).whole_game.state,State::Complete);
    open(&mut wb,"(;SZ[19];B[pd];W[](;B[pp])(;DD[true])(;B[dp]))"); start(&mut wb);
    let worker = world.worker(); let qs = world.queries(worker); assert_eq!(qs.len(),4);
    assert!(qs.iter().all(|q| q["moves"].as_array().unwrap().last().is_none_or(|m| m[1] != "D4")));
    for q in qs { answer(&mut wb,worker,&q); } finish(&mut wb,&world,worker);
    assert_eq!(wb.view(false).whole_game.state,State::Complete);
}
#[test]
fn deeper_results_are_reused_even_when_always_override_is_on() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    open(&mut wb,RECORD); assert_eq!(wb.start_whole_game_analysis(3000).unwrap(),Ok(true));
    let worker = world.worker(); finish(&mut wb,&world,worker);
    let mut settings = wb.config().analysis_engine.clone(); settings.always_override = true;
    wb.save_analysis_engine_settings(settings,"",true).unwrap().unwrap(); start(&mut wb);
    assert_eq!(wb.view(false).whole_game.state,State::Complete);
    wb.navigate(Navigation::Last); assert_eq!(wb.view(false).analysis.unwrap().visits,3000);
}
#[test]
fn pause_returns_process_resume_replans_only_missing_and_stop_keeps_answers() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    open(&mut wb,RECORD); start(&mut wb); let first = world.worker(); let qs = world.queries(first);
    answer(&mut wb,first,&qs[0]); assert!(wb.pause_whole_game_analysis()); assert!(world.ended(first));
    assert_eq!(wb.view(false).whole_game.state,State::Paused); assert!(wb.resume_whole_game_analysis());
    let worker = world.worker(); assert_ne!(worker,first); let qs = world.queries(worker); assert_eq!(qs.len(),4);
    assert!(qs.iter().all(|q| !q["moves"].as_array().unwrap().is_empty()), "the answered root is not asked again");
    assert!(wb.stop_whole_game_analysis()); assert!(world.ended(worker));
    let v = wb.view(false);
    assert_eq!(v.whole_game.state,State::Cancelled); assert_eq!(v.analysis.unwrap().visits,32);
    // Late callbacks from the closed process are not written.
    answer(&mut wb,worker,&qs[0]); wb.navigate(Navigation::Forward(1)); assert!(wb.view(false).analysis.is_none());
}
#[test]
fn a_main_line_edit_fails_but_comments_and_branches_do_not() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    open(&mut wb,RECORD); start(&mut wb);
    wb.edit_document(DocumentEdit::SetComment {node:1,comment:"still same positions".into()}).unwrap();
    wb.engine_tick(Instant::now()+Duration::from_secs(1)); assert_eq!(wb.view(false).whole_game.state,State::Baseline);
    wb.navigate(Navigation::Last); wb.edit(EditCommand::Pass).unwrap();
    wb.engine_tick(Instant::now()+Duration::from_secs(2));
    let v = wb.view(false).whole_game; assert_eq!(v.state,State::Failed); assert_eq!(v.detail_key,"WholeGameAnalysis.error.gameChanged");
}
#[test]
fn komi_edits_are_refused_and_external_komi_changes_trip_the_guard() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    open(&mut wb,RECORD); start(&mut wb);
    assert!(wb.set_komi(6.5).unwrap_err().message.contains("WholeGameAnalysis.conflict.analysis"));
    assert!(matches!(wb.edit(EditCommand::SetGameInfo {black:"a".into(),white:"b".into(),komi:6.5}).unwrap(),lizzie_core::api::CommandResult::Refused {..}));
    wb.edit_document(DocumentEdit::SetKomi {komi:6.5}).unwrap();
    wb.engine_tick(Instant::now()+Duration::from_secs(1));
    assert_eq!(wb.view(false).whole_game.detail_key,"WholeGameAnalysis.error.gameChanged");
}
#[test]
fn replacement_fails_the_session_and_deactivates_result_display() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    open(&mut wb,RECORD); start(&mut wb); let worker = world.worker(); let qs = world.queries(worker); answer(&mut wb,worker,&qs[0]);
    open(&mut wb,"(;SZ[9];B[aa])"); let v = wb.view(false).whole_game;
    assert_eq!(v.state,State::Failed); assert_eq!(v.detail_key,"WholeGameAnalysis.error.gameChanged"); assert!(!v.show_results); assert!(world.ended(worker));
}
#[test]
fn pause_does_not_hide_game_changes_on_resume() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    open(&mut wb,RECORD); start(&mut wb); wb.pause_whole_game_analysis();
    wb.navigate(Navigation::Last); wb.edit(EditCommand::Pass).unwrap(); assert!(wb.resume_whole_game_analysis());
    assert_eq!(wb.view(false).whole_game.detail_key,"WholeGameAnalysis.error.gameChanged");
}
#[test]
fn request_errors_retry_only_missing_nodes_three_times_then_fail() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    open(&mut wb,RECORD); start(&mut wb); let worker = world.worker();
    let first = world.queries(worker); answer(&mut wb,worker,&first[0]);
    for q in &first[1..] { output(&mut wb,worker,&json!({"id":q["id"],"error":"rejected"}).to_string()); }
    for _ in 0..2 { let qs = world.queries(worker); assert_eq!(qs.len(),4); for q in qs { output(&mut wb,worker,&json!({"id":q["id"],"error":"rejected"}).to_string()); } }
    assert_eq!(wb.view(false).whole_game.detail_key,"WholeGameAnalysis.error.request"); assert!(world.ended(worker));
}
#[test]
fn startup_failure_is_reported_without_losing_the_saved_visit_choice() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    let mut settings = wb.config().analysis_engine.clone(); settings.command = "missing-katago analysis".into();
    wb.save_analysis_engine_settings(settings,"",true).unwrap().unwrap(); open(&mut wb,RECORD); start(&mut wb);
    assert_eq!(wb.view(false).whole_game.detail_key,"WholeGameAnalysis.error.engine"); assert_eq!(wb.config().whole_game.deep_visits,1000);
}
#[test]
fn flash_conflicts_and_empty_game_refusals_use_java_keys() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    assert_eq!(wb.start_whole_game_analysis(1000).unwrap(),Err("WholeGameAnalysis.noGame".into())); open(&mut wb,RECORD);
    wb.start_flash_analysis(FlashTarget::WholeGame).unwrap().unwrap();
    assert_eq!(wb.start_whole_game_analysis(1000).unwrap(),Err("WholeGameAnalysis.conflict.analysis".into()));
}
#[test]
fn borrowed_engine_searches_sequentially_and_is_returned_on_completion() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    let fg = foreground(&mut wb,&world); borrow(&mut wb); open(&mut wb,RECORD); serve(&mut wb,&world,fg,0); start(&mut wb);
    assert!(world.0.lock().unwrap().iter().all(|p| p.args.first().is_none_or(|a| a != "analysis")));
    let commands = serve(&mut wb,&world,fg,1000);
    assert!(commands.iter().any(|c| c.starts_with("kata-analyze") && c.contains("maxmoves")));
    assert_eq!(wb.view(false).whole_game.state,State::Complete); assert!(!world.ended(fg));
    assert!(foreground_searches(&commands),"foreground analysis resumes after handback");
}
#[test]
fn borrowed_engine_pause_returns_before_resume_and_stop_returns_it_again() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    let fg = foreground(&mut wb,&world); borrow(&mut wb); open(&mut wb,RECORD); serve(&mut wb,&world,fg,0); start(&mut wb);
    serve(&mut wb,&world,fg,0); wb.pause_whole_game_analysis(); serve(&mut wb,&world,fg,0);
    assert_eq!(wb.view(false).whole_game.state,State::Paused); assert!(wb.resume_whole_game_analysis());
    serve(&mut wb,&world,fg,0); assert!(wb.stop_whole_game_analysis()); serve(&mut wb,&world,fg,0);
    assert_eq!(wb.view(false).whole_game.state,State::Cancelled); assert!(!world.ended(fg));
}
#[test]
fn foreground_is_suspended_and_only_user_pause_prevents_resumption() {
    for paused in [false,true] {
        let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
        let fg = foreground(&mut wb,&world); open(&mut wb,RECORD); serve(&mut wb,&world,fg,0); start(&mut wb);
        let commands = serve(&mut wb,&world,fg,0); assert!(!foreground_searches(&commands));
        if paused {
            wb.toggle_analysis().unwrap(); // the first Space resumes the temporarily stopped engine
            serve(&mut wb,&world,fg,0);
            wb.toggle_analysis().unwrap(); // the second is the user's actual pause
        }
        wb.stop_whole_game_analysis(); let commands = serve(&mut wb,&world,fg,0);
        assert_eq!(foreground_searches(&commands),!paused); assert_eq!(wb.view(false).engine.paused,paused);
    }
}
#[test]
fn visit_settings_persist_and_invalid_stored_values_fall_back() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    open(&mut wb,RECORD); wb.start_whole_game_analysis(3000).unwrap().unwrap(); wb.stop_whole_game_analysis();
    let reopened = Workbench::new(wb.storage().clone(),None,Box::new(Launch(world)),vec![]);
    assert_eq!(reopened.config().whole_game.deep_visits,3000);
    for n in [-1,0,499,1_000_001] { assert_eq!(WholeGameSettings::from_stored(n).deep_visits,1000); }
    assert_eq!(serde_json::from_value::<WholeGameSettings>(json!({"deepVisits":-7})).unwrap().deep_visits,1000);
}
#[test]
fn quick_analysis_is_taken_over_without_a_conflict() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    let fg = foreground(&mut wb,&world); wb.set_auto_quick_analyze(true).unwrap(); open(&mut wb,RECORD);
    serve(&mut wb,&world,fg,0); wb.engine_tick(Instant::now()+Duration::from_secs(5));
    start(&mut wb); assert_eq!(wb.view(false).whole_game.state,State::Baseline);
    let worker = world.worker(); finish(&mut wb,&world,worker); assert_eq!(wb.view(false).whole_game.state,State::Complete);
}

#[test]
fn ownership_is_requested_only_in_deep_and_refills_without_replacing_deeper_results() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    open(&mut wb, RECORD); wb.start_whole_game_analysis(3000).unwrap().unwrap();
    let worker = world.worker(); finish(&mut wb, &world, worker);
    wb.set_kata_estimate(lizzie_core::api::KataEstimateMode::Main).unwrap(); start(&mut wb);
    let worker = world.worker(); let qs = world.queries(worker);
    assert_eq!(qs.len(),5); assert!(qs.iter().all(|q| q["maxVisits"] == 1000 && q["includeOwnership"] == true));
    for q in qs { answer(&mut wb,worker,&q); }
    wb.navigate(Navigation::Last); let v = wb.view(false);
    assert_eq!(v.whole_game.state,State::Complete); assert_eq!(v.analysis.unwrap().visits,3000);
    assert!(v.kata_estimate.ownership.is_some());
}

#[test]
fn worker_exit_fails_request_and_does_not_discard_completed_nodes() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    open(&mut wb,RECORD); start(&mut wb); let worker = world.worker(); let qs = world.queries(worker);
    answer(&mut wb,worker,&qs[0]); wb.engine_input(worker,EngineInput::Exited(Some(1)));
    assert_eq!(wb.view(false).whole_game.detail_key,"WholeGameAnalysis.error.request");
    assert_eq!(wb.view(false).analysis.unwrap().visits,32, "the answered root keeps its result");
}

#[test]
fn stored_benchmark_estimates_only_missing_work_and_result_overrides_do_not_mutate_settings() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    open(&mut wb,RECORD); assert_eq!(wb.estimate_whole_game_analysis(1000),None);
    wb.save_engine_settings(vec![EngineProfile { name:"KataGo".into(), command:"/opt/katago gtp".into(),
        thread_policy: Some(lizzie_core::engine::tuning::ThreadPolicy { threads:4, visits_per_second:1000.0,
            ..Default::default() }), ..Default::default() }],EngineStartup::Default).unwrap();
    assert_eq!(wb.estimate_whole_game_analysis(1000),Some(5160));
    let mut d = wb.config().display.clone(); d.move_numbers = -1; d.move_rank = -1;
    d.suggestion_winrate = false; d.suggestion_playouts = false; d.suggestion_variations = false;
    wb.set_display_settings(d.clone()).unwrap(); start(&mut wb);
    let worker = world.worker(); finish(&mut wb,&world,worker); wb.navigate(Navigation::Last);
    let v = wb.view(false); assert!(v.whole_game.show_results); assert!(v.board.ranks_after_numbers);
    assert!(!v.board.move_numbers.is_empty()); assert!(!v.board.ranks.is_empty());
    assert_eq!(wb.config().display,d); assert_eq!(wb.estimate_whole_game_analysis(1000),Some(1));
}

#[test]
fn percent_caps_at_99_until_all_callbacks_finish_even_if_foreground_fills_last_position() {
    let dir = tempfile::tempdir().unwrap(); let world = World::default(); let mut wb = make(dir.path(), &world);
    let fg = foreground(&mut wb,&world); open(&mut wb,RECORD); wb.navigate(Navigation::Last); serve(&mut wb,&world,fg,0);
    start(&mut wb); let worker = world.worker();
    for q in world.queries(worker) { answer(&mut wb,worker,&q); }
    let deep = world.queries(worker); assert_eq!(deep.len(),5);
    wb.toggle_analysis().unwrap();
    assert!(foreground_searches(&serve(&mut wb,&world,fg,1000)));
    for q in deep.iter().filter(|q| q["moves"].as_array().unwrap().len() != 4) { answer(&mut wb,worker,q); }
    let v = wb.view(false).whole_game; assert_eq!((v.state,v.percent),(State::Deep,99));
    let last = deep.iter().find(|q| q["moves"].as_array().unwrap().len() == 4).unwrap(); answer(&mut wb,worker,last);
    assert_eq!(wb.view(false).whole_game.percent,100);
}

#[test]
fn whole_game_command_uses_java_throughput_profile_and_preserves_explicit_overrides() {
    use lizzie_core::engine::{analysis_command, gtp, tuning};
    let command = analysis_command::whole_game("katago analysis -model m.bin.gz -config a.cfg",15,16);
    let spec = gtp::prepare_launch(&command,&[]).unwrap(); let overrides = tuning::overrides(&spec.args);
    assert_eq!(overrides["numanalysisthreads"],"8"); assert_eq!(overrides["numsearchthreadsperanalysisthread"],"2");
    assert_eq!(overrides["numsearchthreads"],""); assert_eq!(overrides["analysispvlen"],"15");
    let command = analysis_command::whole_game("katago analysis -override-config numAnalysisThreads=3,numSearchThreadsPerAnalysisThread=4,analysisPVLen=9",15,16);
    let spec = gtp::prepare_launch(&command,&[]).unwrap(); let overrides = tuning::overrides(&spec.args);
    assert_eq!(overrides["numanalysisthreads"],"3"); assert_eq!(overrides["numsearchthreadsperanalysisthread"],"4"); assert_eq!(overrides["analysispvlen"],"9");
}

//! 批量分析 / 批量精析 through the public `Workbench` operations, with controlled engines and real
//! record files: each queued record is analysed as a background document and saved beside its
//! source, the open record is never written, a forced stop / cancel / unreadable file / failed
//! save keeps what finished and reports what did not.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use lizzie_core::api::{BatchMode, BatchMove, FlashTarget, OpenSource};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats::{self, sgf};
use lizzie_core::record::{DocumentEdit, RecordFormat, ROOT};
use lizzie_core::settings::{AnalysisEngineSettings, AutoAnalysisSettings, Storage};
use lizzie_core::workbench::{OpenOrigin, Workbench};

#[derive(Default)]
struct Proc {
    program: String,
    args: Vec<String>,
    sent: Vec<String>,
    terminated: bool,
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
    fn workers(&self) -> Vec<InstanceId> {
        let w = self.0.lock().unwrap();
        w.iter()
            .filter(|p| p.1.args.first().is_some_and(|a| a == "analysis"))
            .map(|p| p.0)
            .collect()
    }
    fn engine(&self, program: &str) -> InstanceId {
        let w = self.0.lock().unwrap();
        w.iter().rev().find(|p| p.1.program == program).map(|p| p.0).unwrap()
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
        w.iter_mut()
            .find(|p| p.0 == self.id)
            .unwrap()
            .1
            .sent
            .push(line.to_string());
        Ok(())
    }
    fn terminate(self: Box<Self>) {
        let mut w = self.world.0.lock().unwrap();
        w.iter_mut().find(|p| p.0 == self.id).unwrap().1.terminated = true;
    }
}

struct FakeLauncher(World);

impl Launcher for FakeLauncher {
    fn launch(&self, id: InstanceId, spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        self.0 .0.lock().unwrap().push((
            id,
            Proc {
                program: spec.program.clone(),
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

const KATA: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nset_position\nkata-analyze\nkata-set-rules\nkata-get-rules\nstop";
const CHINESE: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;
const OPEN: &str = "(;SZ[19];B[pd];W[dd])";
const GAME_A: &str = "(;SZ[19];B[pd];W[dd];B[pp];W[dp])";
const GAME_B: &str = "(;SZ[19];B[dd];W[pp])";
const ANALYSIS: &str = "/opt/katago analysis -config /a.cfg";

fn out(wb: &mut Workbench, id: InstanceId, line: &str) {
    wb.engine_input(id, EngineInput::Stdout(line.to_string()));
}

/// Answers every outstanding GTP command of `id`; each `kata-analyze` gets one line of `visits`.
fn serve(wb: &mut Workbench, world: &World, id: InstanceId, visits: u64) -> Vec<String> {
    let mut done = Vec::new();
    loop {
        let pending = world.take(id);
        if pending.is_empty() {
            return done;
        }
        for line in pending {
            out(wb, id, "");
            let n: u64 = line.split(' ').next().unwrap().parse().unwrap();
            let cmd = line.split_once(' ').unwrap().1.to_string();
            let reply = |wb: &mut Workbench, text: &str| {
                out(wb, id, &format!("={n} {text}"));
                out(wb, id, "");
            };
            match cmd.split(' ').next().unwrap() {
                "name" => reply(wb, "KataGo"),
                "version" => reply(wb, "1.18.2"),
                "list_commands" => reply(wb, KATA),
                "kata-get-rules" => reply(wb, CHINESE),
                "kata-analyze" => {
                    out(wb, id, &format!("={n}"));
                    if visits > 0 {
                        out(wb, id, &format!(
                            "info move K10 visits {visits} winrate 0.5 scoreLead 0.5 prior 0.2 order 0 pv K10 rootInfo visits {visits} winrate 0.4 scoreLead -0.5"
                        ));
                    }
                }
                _ => reply(wb, ""),
            }
            done.push(cmd);
        }
    }
}

fn queries(world: &World, worker: InstanceId) -> Vec<serde_json::Value> {
    world
        .take(worker)
        .iter()
        .map(|q| serde_json::from_str(q).unwrap())
        .collect()
}

fn answer(wb: &mut Workbench, worker: InstanceId, q: &serde_json::Value) {
    let reply = serde_json::json!({
        "id": q["id"],
        "moveInfos": [{"move": "K10", "visits": 2, "winrate": 0.5, "scoreLead": 1.0, "order": 0, "pv": ["K10"]}],
        "rootInfo": {"visits": 2, "winrate": 0.5, "scoreLead": 1.0},
    });
    out(wb, worker, &reply.to_string());
}

/// A workbench with a ready foreground engine and an open record of its own (`/open.sgf`).
fn setup(dir: &Path, reuse: bool) -> (Workbench, World, InstanceId) {
    let world = World::default();
    let mut wb = Workbench::new(
        Storage::in_dirs(&dir.join("config"), &dir.join("data")),
        Some("zh-CN"),
        Box::new(FakeLauncher(world.clone())),
        Vec::new(),
    );
    let settings = AnalysisEngineSettings {
        command: ANALYSIS.into(),
        reuse_current_engine: reuse,
        ..AnalysisEngineSettings::default()
    };
    wb.save_analysis_engine_settings(settings, "", true).unwrap().unwrap();
    wb.set_auto_quick_analyze(false).unwrap();
    let profile = EngineProfile {
        name: "KataGo".into(),
        command: "/opt/katago gtp".into(),
        ..EngineProfile::default()
    };
    wb.save_engine_settings(vec![profile], EngineStartup::Default).unwrap();
    wb.start_engines();
    let fg = world.engine("/opt/katago");
    serve(&mut wb, &world, fg, 0);
    let t = wb.begin_open(OpenOrigin::File {
        path: "/open.sgf".into(),
        source: OpenSource::Drop,
    });
    wb.complete_open(t, Ok((RecordFormat::Sgf, sgf::read(OPEN).unwrap())));
    serve(&mut wb, &world, fg, 0);
    (wb, world, fg)
}

fn write(dir: &Path, name: &str, text: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path.to_string_lossy().into_owned()
}

/// Runs the batch's file reads as the adapter does (off the lock, real files).
fn deliver_reads(wb: &mut Workbench) -> usize {
    let reads = wb.take_batch_reads();
    let n = reads.len();
    for (ticket, path, no_capture) in reads {
        assert!(wb.complete_batch_read(ticket, formats::read_file(&path, no_capture)));
    }
    n
}

/// Writes the captured background saves; `fail` names a file whose write fails.
fn write_saves(wb: &mut Workbench, fail: Option<&str>) -> Vec<PathBuf> {
    let mut written = Vec::new();
    for job in wb.take_background_saves() {
        let name = job.path.file_name().unwrap().to_string_lossy().into_owned();
        if fail.is_some_and(|f| name.starts_with(f)) {
            wb.complete_save(job.ticket, Err("read-only folder".into()));
        } else {
            formats::write_record_file(&job.path, &job.text).unwrap();
            wb.complete_save(job.ticket, Ok(()));
            written.push(job.path);
        }
    }
    written
}

fn ordinary() -> AutoAnalysisSettings {
    AutoAnalysisSettings {
        visits_enabled: true,
        visits: 10,
        time_enabled: false,
        auto_save: false,
        ..Default::default()
    }
}

fn analysed_copies(dir: &Path, stem: &str) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&format!("{stem}_已分析_"))
        })
        .collect()
}

#[test]
fn ordinary_batch_analyses_each_record_in_the_background_while_the_open_record_is_edited() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, fg) = setup(dir.path(), false);
    let games = dir.path().join("games");
    std::fs::create_dir(&games).unwrap();
    let a = write(&games, "a.sgf", GAME_A);
    let b = write(&games, "b.sgf", GAME_B);
    let open_doc = wb.document().id;

    assert_eq!(
        wb.open_batch(BatchMode::Ordinary, vec![a.clone(), b.clone()], OpenSource::Chooser),
        Ok(true)
    );
    let v = wb.view(false);
    assert_eq!(v.batch.setup, Some(BatchMode::Ordinary));
    assert_eq!(
        v.batch.items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
        ["a.sgf", "b.sgf"]
    );
    let table = v.batch.open_table;
    assert_eq!(wb.start_batch_ordinary(ordinary()).unwrap(), Ok(true));
    assert_eq!(
        wb.view(false).batch.open_table,
        table + 1,
        "进度表 opens for several records"
    );
    assert_eq!(deliver_reads(&mut wb), 1);

    // The user edits the open record while the background record is analysed.
    wb.edit_document(DocumentEdit::SetComment {
        node: ROOT,
        comment: "mine".into(),
    })
    .unwrap();
    let edited = wb.document().revision;
    serve(&mut wb, &world, fg, 10);
    let v = wb.view(false);
    assert_eq!(v.batch.items[0].name, "b.sgf", "a finished and left the table");
    assert_eq!(v.position.node, ROOT, "the open record's browsing position never moves");
    assert!(v.analysis.is_none(), "batch results do not enter the open record");

    let written = write_saves(&mut wb, None);
    assert_eq!(written.len(), 1);
    let copy = std::fs::read_to_string(&written[0]).unwrap();
    assert!(copy.contains("LZ["), "the copy carries the analysis: {copy}");
    assert_eq!(analysed_copies(&games, "a").len(), 1);
    assert_eq!(
        std::fs::read_to_string(&a).unwrap(),
        GAME_A,
        "the source is not replaced"
    );

    assert_eq!(deliver_reads(&mut wb), 1);
    serve(&mut wb, &world, fg, 10);
    write_saves(&mut wb, None);
    let v = wb.view(false);
    assert!(v.batch.running.is_none() && v.batch.items.is_empty());
    let notice = v.batch.notice.expect("the end is reported");
    assert_eq!(notice.message_key, "Leelaz.batchAutoAnalyzeComplete");
    assert!(notice.failures.is_empty());
    assert_eq!(analysed_copies(&games, "b").len(), 1);

    let doc = wb.document();
    assert_eq!(
        (doc.id, doc.revision),
        (open_doc, edited),
        "the open record keeps its identity and edit"
    );
    assert!(doc.unsaved(), "batch saves never mark the open record saved");
    assert_eq!(doc.path.as_deref(), Some("/open.sgf"));
    assert_eq!(doc.record.node(ROOT).comment, "mine");
    assert!(!v.auto_analysis.active);
}

#[test]
fn a_forced_stop_saves_the_current_record_and_forgets_the_queue() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_A);
    let b = write(dir.path(), "b.sgf", GAME_B);
    wb.open_batch(BatchMode::Ordinary, vec![a, b], OpenSource::Chooser)
        .unwrap();
    wb.start_batch_ordinary(ordinary()).unwrap().unwrap();
    deliver_reads(&mut wb);
    serve(&mut wb, &world, fg, 3); // below 每手总计算量: still at the first position
    assert!(wb.view(false).auto_analysis.active);
    assert!(wb.stop_auto_analysis());
    assert_eq!(write_saves(&mut wb, None).len(), 1, "the current record is saved");
    assert_eq!(deliver_reads(&mut wb), 0, "no further record is loaded");
    let v = wb.view(false);
    assert_eq!(v.batch.notice.unwrap().message_key, "Leelaz.batchAutoAnalyzeStopped");
    assert!(v.batch.items.is_empty() && v.batch.running.is_none());
    assert_eq!(analysed_copies(dir.path(), "a").len(), 1);
    assert!(analysed_copies(dir.path(), "b").is_empty());
}

#[test]
fn an_unreadable_record_ends_the_ordinary_batch_naming_it_after_the_finished_record_is_saved() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_A);
    let bad = write(dir.path(), "bad.sgf", "not a game record");
    let c = write(dir.path(), "c.sgf", GAME_B);
    wb.open_batch(BatchMode::Ordinary, vec![a, bad.clone(), c], OpenSource::Chooser)
        .unwrap();
    wb.start_batch_ordinary(ordinary()).unwrap().unwrap();
    deliver_reads(&mut wb);
    serve(&mut wb, &world, fg, 10);
    deliver_reads(&mut wb); // bad.sgf: the batch ends while a's save is still being written
    assert!(
        wb.view(false).batch.notice.is_none(),
        "the end waits for the batch's saves"
    );
    write_saves(&mut wb, None);
    assert_eq!(deliver_reads(&mut wb), 0, "no further record is loaded");
    let v = wb.view(false);
    let notice = v.batch.notice.expect("reported");
    assert_eq!(notice.message_key, "LizzieFrame.prompt.failedToOpenFile");
    assert_eq!(notice.failures.len(), 1);
    assert_eq!(notice.failures[0].path, bad);
    assert!(
        v.batch.running.is_none() && v.batch.items.is_empty(),
        "批量分析 forgets its queue"
    );
    assert_eq!(
        analysed_copies(dir.path(), "a").len(),
        1,
        "the finished record stays saved"
    );
    assert!(analysed_copies(dir.path(), "c").is_empty());
}

#[test]
fn flash_batch_skips_an_unreadable_record_and_writes_nothing_under_its_name() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, _fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_B);
    let bad = write(dir.path(), "bad.sgf", "not a game record");
    let c = write(dir.path(), "c.sgf", GAME_B);
    wb.open_batch(BatchMode::Flash, vec![a, bad.clone(), c], OpenSource::Chooser)
        .unwrap();
    wb.start_batch_flash(-1, -1, 2, false).unwrap().unwrap();
    deliver_reads(&mut wb);
    let worker = *world.workers().last().expect("an analysis process");
    for q in &queries(&world, worker) {
        answer(&mut wb, worker, q);
    }
    write_saves(&mut wb, None);
    deliver_reads(&mut wb); // bad.sgf fails: c.sgf is asked for next
    let v = wb.view(false);
    assert!(v.batch.notice.is_none(), "the batch goes on");
    assert_eq!(
        v.batch.items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
        ["c.sgf"]
    );
    assert_eq!(deliver_reads(&mut wb), 1);
    for q in &queries(&world, worker) {
        answer(&mut wb, worker, q);
    }
    write_saves(&mut wb, None);
    let v = wb.view(false);
    let notice = v.batch.notice.expect("reported");
    assert_eq!(notice.message_key, "Leelaz.batchAutoAnalyzeComplete");
    assert_eq!(notice.failures.len(), 1);
    assert_eq!(notice.failures[0].path, bad);
    assert_eq!(notice.failures[0].message_key, "LizzieFrame.prompt.failedToOpenFile");
    assert_eq!(analysed_copies(dir.path(), "a").len(), 1);
    assert_eq!(analysed_copies(dir.path(), "c").len(), 1);
    assert!(analysed_copies(dir.path(), "bad").is_empty());
}

#[test]
fn a_failed_save_is_reported_instead_of_a_plain_success() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_A);
    wb.open_batch(BatchMode::Ordinary, vec![a], OpenSource::Chooser)
        .unwrap();
    wb.start_batch_ordinary(ordinary()).unwrap().unwrap();
    assert_eq!(wb.view(false).batch.open_table, 0, "one record does not open 进度表");
    deliver_reads(&mut wb);
    serve(&mut wb, &world, fg, 10);
    write_saves(&mut wb, Some("a_"));
    let v = wb.view(false);
    let notice = v.batch.notice.unwrap();
    assert_eq!(notice.message_key, "Leelaz.batchAutoAnalyzeComplete");
    assert_eq!(notice.failures.len(), 1);
    assert_eq!(notice.failures[0].message_key, "LizzieFrame.saveFileFailed");
    assert!(notice.failures[0].path.contains("a_已分析_"));
    assert!(v.save_outcome.is_none(), "batch saves are not the open record's saves");
}

#[test]
fn flash_batch_searches_batch_visits_and_a_cancel_keeps_the_current_record_to_continue() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_A);
    let b = write(dir.path(), "b.sgf", GAME_B);
    let c = write(dir.path(), "c.sgf", GAME_B);
    wb.open_batch(BatchMode::Flash, vec![a, b, c], OpenSource::Chooser)
        .unwrap();
    assert_eq!(wb.start_batch_flash(-1, -1, 1, false).unwrap(), Ok(true));
    assert_eq!(
        wb.config().analysis_engine.batch_visits,
        1,
        "batch-analysis-playouts is saved"
    );
    deliver_reads(&mut wb);
    let worker = *world.workers().last().expect("an analysis process");
    let qs = queries(&world, worker);
    assert_eq!(qs.len(), 4);
    assert!(
        qs.iter().all(|q| q["maxVisits"] == 2),
        "max(2, batch-analysis-playouts)"
    );
    let run = wb.view(false).batch.running.unwrap();
    assert_eq!((run.done, run.total, run.loading), (0, 4, false));
    for q in &qs {
        answer(&mut wb, worker, q);
    }
    assert_eq!(write_saves(&mut wb, None).len(), 1);
    assert!(
        wb.view(false).analysis.is_none(),
        "the open record is not analysed by the batch"
    );

    deliver_reads(&mut wb);
    let qs = queries(&world, worker);
    assert_eq!(qs.len(), 2, "the same process continues with b");
    answer(&mut wb, worker, &qs[0]);
    assert!(wb.cancel_batch_flash());
    assert!(world.terminated(worker));
    assert!(wb.take_background_saves().is_empty(), "a cancelled record is not saved");
    let v = wb.view(false);
    assert!(v.batch.running.is_none());
    assert_eq!(
        v.batch.items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
        ["b.sgf", "c.sgf"]
    );
    assert!(
        !serve(&mut wb, &world, fg, 0)
            .iter()
            .any(|c| c.starts_with("kata-analyze")),
        "the foreground engine stays stopped"
    );

    // 开始/结束(闪电模式) continues from the record that was cancelled.
    assert_eq!(wb.setup_batch(BatchMode::Flash), Ok(true));
    wb.start_batch_flash(-1, -1, 1, false).unwrap().unwrap();
    deliver_reads(&mut wb);
    let worker = *world.workers().last().unwrap();
    let qs = queries(&world, worker);
    assert_eq!(qs.len(), 2);
    for q in &qs {
        answer(&mut wb, worker, q);
    }
    write_saves(&mut wb, None);
    deliver_reads(&mut wb);
    for q in &queries(&world, worker) {
        answer(&mut wb, worker, q);
    }
    write_saves(&mut wb, None);
    let v = wb.view(false);
    assert_eq!(v.batch.notice.unwrap().message_key, "Leelaz.batchAutoAnalyzeComplete");
    assert!(v.batch.items.is_empty());
    assert!(world.terminated(worker), "结束分析后退出引擎 after the last record");
    for stem in ["a", "b", "c"] {
        assert_eq!(analysed_copies(dir.path(), stem).len(), 1, "{stem}");
    }
}

#[test]
fn a_running_batch_excludes_other_analyses_and_its_rows_move_like_java() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, fg) = setup(dir.path(), false);
    let paths: Vec<String> = ["a", "b", "c", "d"]
        .iter()
        .map(|n| write(dir.path(), &format!("{n}.sgf"), GAME_B))
        .collect();
    wb.open_batch(BatchMode::Ordinary, paths.clone(), OpenSource::Chooser)
        .unwrap();
    wb.start_batch_ordinary(ordinary()).unwrap().unwrap();
    deliver_reads(&mut wb);
    serve(&mut wb, &world, fg, 3);
    assert_eq!(wb.start_auto_analysis(ordinary()).unwrap(), Err("AutoAnalyze.conflict"));
    assert_eq!(
        wb.start_whole_game_analysis(1000).unwrap(),
        Err("WholeGameAnalysis.conflict".into())
    );
    assert_eq!(
        wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(),
        Err("AutoAnalyze.conflict".into())
    );
    assert_eq!(
        wb.open_batch(BatchMode::Flash, paths.clone(), OpenSource::Chooser),
        Err("AutoAnalyze.conflict")
    );

    let ids: Vec<u64> = wb.view(false).batch.items.iter().map(|i| i.id).collect();
    let names = |wb: &mut Workbench| {
        wb.view(false)
            .batch
            .items
            .iter()
            .map(|i| i.name.clone())
            .collect::<Vec<_>>()
    };
    assert!(
        !wb.move_batch_item(ids[0], BatchMove::Down),
        "the current record never moves"
    );
    assert!(!wb.move_batch_item(ids[1], BatchMove::Up), "nothing moves above it");
    assert!(!wb.remove_batch_item(ids[0]));
    assert!(wb.move_batch_item(ids[3], BatchMove::Prior));
    assert_eq!(names(&mut wb), ["a.sgf", "d.sgf", "b.sgf", "c.sgf"]);
    assert!(
        !wb.move_batch_item(ids[2], BatchMove::Down),
        "the last row cannot move down"
    );
    assert!(wb.remove_batch_item(ids[1]));
    assert_eq!(names(&mut wb), ["a.sgf", "d.sgf", "c.sgf"]);

    // 清空 stops the batch (its current record is saved) and empties the queue.
    assert!(wb.clear_batch());
    assert_eq!(write_saves(&mut wb, None).len(), 1);
    let v = wb.view(false);
    assert!(v.batch.items.is_empty() && v.batch.running.is_none() && !v.auto_analysis.active);
    assert_eq!(
        wb.start_auto_analysis(ordinary()).unwrap(),
        Ok(true),
        "the foreground engine is free again"
    );
}

#[test]
fn dropped_records_skip_the_recent_list_only_for_the_first_and_cancelled_dialogs_follow_java() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_B);
    let b = write(dir.path(), "b.sgf", GAME_B);
    wb.open_batch(BatchMode::Ordinary, vec![a.clone(), b.clone()], OpenSource::Drop)
        .unwrap();
    assert!(wb.cancel_batch_setup());
    assert!(
        wb.view(false).batch.items.is_empty(),
        "批量分析's cancelled dialog ends the batch"
    );

    wb.open_batch(BatchMode::Flash, vec![a.clone()], OpenSource::Chooser)
        .unwrap();
    assert!(wb.cancel_batch_setup());
    assert_eq!(
        wb.view(false).batch.items.len(),
        1,
        "批量精析 keeps its records in 进度表"
    );

    wb.open_batch(BatchMode::Ordinary, vec![a.clone(), b.clone()], OpenSource::Drop)
        .unwrap();
    wb.start_batch_ordinary(ordinary()).unwrap().unwrap();
    deliver_reads(&mut wb);
    serve(&mut wb, &world, fg, 10);
    write_saves(&mut wb, None);
    deliver_reads(&mut wb);
    let recent = wb.view(false).recent_files;
    assert!(!recent.contains(&a), "the dropped record opened from temp");
    assert!(
        recent.contains(&b),
        "later records load like Java's loadFile(file, false)"
    );
}

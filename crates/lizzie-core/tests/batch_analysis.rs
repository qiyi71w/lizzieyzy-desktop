//! 批量分析 / 批量精析 through the public `Workbench` operations, with controlled engines and real
//! record files: each queued record is analysed as a background document and saved beside its
//! source, the open record is never written, a forced stop / cancel / unreadable file / failed
//! save keeps what finished and reports what did not.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use lizzie_core::api::{BatchMode, BatchMove, FlashTarget, OpenSource, RegionCommand};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats::{self, sgf};
use lizzie_core::record::{DocumentEdit, Point, RecordFormat, ROOT};
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
    let info = (visits > 0).then(|| format!(
        "info move K10 visits {visits} winrate 0.5 scoreLead 0.5 prior 0.2 order 0 pv K10 rootInfo visits {visits} winrate 0.4 scoreLead -0.5"
    ));
    serve_with(wb, world, id, info.as_deref())
}

/// [`serve`] with `info` as each `kata-analyze`'s line.
fn serve_with(wb: &mut Workbench, world: &World, id: InstanceId, info: Option<&str>) -> Vec<String> {
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
                    if let Some(info) = info {
                        out(wb, id, info);
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
    assert_eq!(wb.view(false).batch.setup, None, "the first record is read first");
    assert_eq!(deliver_reads(&mut wb), 1);
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
    assert_eq!(deliver_reads(&mut wb), 0, "a was read before its settings");

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
    deliver_reads(&mut wb);
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
    deliver_reads(&mut wb);
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
    deliver_reads(&mut wb);
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
    deliver_reads(&mut wb);
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
    deliver_reads(&mut wb);
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
    assert_eq!(qs.len(), 1, "b keeps its answered position");
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
    deliver_reads(&mut wb);
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
    deliver_reads(&mut wb);
    assert!(wb.cancel_batch_setup());
    assert!(
        wb.view(false).batch.items.is_empty(),
        "批量分析's cancelled dialog ends the batch"
    );

    let c = write(dir.path(), "c.sgf", GAME_B);
    wb.open_batch(BatchMode::Flash, vec![c], OpenSource::Chooser).unwrap();
    deliver_reads(&mut wb);
    assert!(wb.cancel_batch_setup());
    assert_eq!(
        wb.view(false).batch.items.len(),
        1,
        "批量精析 keeps its records in 进度表"
    );

    wb.open_batch(BatchMode::Ordinary, vec![a.clone(), b.clone()], OpenSource::Drop)
        .unwrap();
    deliver_reads(&mut wb);
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

#[test]
fn flash_batch_keeps_its_engine_route_when_reuse_changes_mid_queue() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, _fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_B);
    let b = write(dir.path(), "b.sgf", GAME_B);
    wb.open_batch(BatchMode::Flash, vec![a, b], OpenSource::Chooser)
        .unwrap();
    deliver_reads(&mut wb);
    wb.start_batch_flash(-1, -1, 2, false).unwrap().unwrap();
    deliver_reads(&mut wb);
    let worker = *world.workers().last().unwrap();
    for q in &queries(&world, worker) {
        answer(&mut wb, worker, q);
    }
    write_saves(&mut wb, None);
    // 精析设置 switches to 复用当前前台引擎 while the queue runs.
    let reuse = AnalysisEngineSettings {
        reuse_current_engine: true,
        ..wb.config().analysis_engine.clone()
    };
    wb.save_analysis_engine_settings(reuse, ANALYSIS, false)
        .unwrap()
        .unwrap();
    deliver_reads(&mut wb);
    let qs = queries(&world, worker);
    assert_eq!(qs.len(), 2, "b goes on with the batch's own process");
    for q in &qs {
        answer(&mut wb, worker, q);
    }
    write_saves(&mut wb, None);
    assert_eq!(
        wb.view(false).batch.notice.unwrap().message_key,
        "Leelaz.batchAutoAnalyzeComplete"
    );
    assert!(world.terminated(worker), "the process ends with the batch");
    assert_eq!(world.workers().len(), 1);
}

#[test]
fn a_new_batch_waits_for_the_previous_batch_to_finish_writing() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_A);
    let b = write(dir.path(), "b.sgf", GAME_B);
    wb.open_batch(BatchMode::Ordinary, vec![a], OpenSource::Chooser)
        .unwrap();
    deliver_reads(&mut wb);
    wb.start_batch_ordinary(ordinary()).unwrap().unwrap();
    deliver_reads(&mut wb);
    serve(&mut wb, &world, fg, 10);
    let jobs = wb.take_background_saves();
    assert_eq!(jobs.len(), 1, "a's copy is being written");
    assert_eq!(
        wb.open_batch(BatchMode::Ordinary, vec![b], OpenSource::Chooser),
        Err("AutoAnalyze.conflict")
    );
    for job in jobs {
        wb.complete_save(job.ticket, Err("read-only folder".into()));
    }
    let notice = wb.view(false).batch.notice.expect("the first batch's end");
    assert_eq!(notice.message_key, "Leelaz.batchAutoAnalyzeComplete");
    assert_eq!(notice.failures.len(), 1, "its failed save is reported");
}

#[test]
fn a_read_finishing_after_exit_starts_no_engine() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, _fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_B);
    let b = write(dir.path(), "b.sgf", GAME_B);
    wb.open_batch(BatchMode::Flash, vec![a, b], OpenSource::Chooser)
        .unwrap();
    deliver_reads(&mut wb);
    wb.start_batch_flash(-1, -1, 2, false).unwrap().unwrap();
    let worker = *world.workers().last().unwrap();
    for q in &queries(&world, worker) {
        answer(&mut wb, worker, q);
    }
    write_saves(&mut wb, None);
    let reads = wb.take_batch_reads();
    assert_eq!(reads.len(), 1, "b is being read");
    wb.end_engines();
    for (ticket, path, no_capture) in reads {
        assert!(!wb.complete_batch_read(ticket, formats::read_file(&path, no_capture)));
    }
    assert_eq!(world.workers().len(), 1, "no analysis process after exit");
    assert!(queries(&world, worker).is_empty());
    assert!(wb.take_background_saves().is_empty());
}

/// Delivers the reads in flight, expecting each to be one the batch still wants or not.
fn deliver_reads_expecting(wb: &mut Workbench, wanted: bool) -> usize {
    let reads = wb.take_batch_reads();
    for (ticket, path, no_capture) in &reads {
        assert_eq!(
            wb.complete_batch_read(*ticket, formats::read_file(path, *no_capture)),
            wanted
        );
    }
    reads.len()
}

#[test]
fn stopping_an_ordinary_batch_while_its_record_loads_starts_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_A);
    let b = write(dir.path(), "b.sgf", GAME_B);
    wb.open_batch(BatchMode::Ordinary, vec![a, b], OpenSource::Chooser)
        .unwrap();
    deliver_reads(&mut wb);
    wb.start_batch_ordinary(ordinary()).unwrap().unwrap();
    serve(&mut wb, &world, fg, 10); // a done: its copy is written and b is being read
    write_saves(&mut wb, None);
    assert!(wb.view(false).batch.running.is_some());
    assert!(wb.stop_auto_analysis(), "停止自动(批量)分析 stops the loading batch");
    deliver_reads_expecting(&mut wb, false);
    assert!(
        !serve(&mut wb, &world, fg, 10)
            .iter()
            .any(|c| c.starts_with("kata-analyze")),
        "b is not analysed"
    );
    let v = wb.view(false);
    assert!(v.batch.running.is_none() && v.batch.items.is_empty());
    assert_eq!(v.batch.notice.unwrap().message_key, "Leelaz.batchAutoAnalyzeStopped");
    assert!(analysed_copies(dir.path(), "b").is_empty());
}

#[test]
fn single_record_auto_analysis_is_refused_while_a_batch_record_loads() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_A);
    let b = write(dir.path(), "b.sgf", GAME_B);
    wb.open_batch(BatchMode::Ordinary, vec![a, b], OpenSource::Chooser)
        .unwrap();
    deliver_reads(&mut wb);
    wb.start_batch_ordinary(ordinary()).unwrap().unwrap();
    deliver_reads(&mut wb);
    serve(&mut wb, &world, fg, 10); // a done: b is being read
    write_saves(&mut wb, None);
    assert!(wb.view(false).batch.running.is_some());
    assert!(!wb.take_batch_reads().is_empty(), "b's read is in flight");
    assert_eq!(wb.start_auto_analysis(ordinary()).unwrap(), Err("AutoAnalyze.conflict"));
}

#[test]
fn continuing_a_cancelled_flash_batch_asks_only_for_the_positions_still_missing() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, _fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_A);
    wb.open_batch(BatchMode::Flash, vec![a], OpenSource::Chooser).unwrap();
    deliver_reads(&mut wb);
    wb.start_batch_flash(-1, -1, 2, false).unwrap().unwrap();
    deliver_reads(&mut wb);
    let worker = *world.workers().last().unwrap();
    let qs = queries(&world, worker);
    assert_eq!(qs.len(), 4);
    answer(&mut wb, worker, &qs[0]);
    assert!(wb.cancel_batch_flash());
    assert_eq!(wb.setup_batch(BatchMode::Flash), Ok(true));
    wb.start_batch_flash(-1, -1, 2, false).unwrap().unwrap();
    deliver_reads(&mut wb);
    let worker = *world.workers().last().unwrap();
    let qs = queries(&world, worker);
    assert_eq!(qs.len(), 3, "the answered position keeps its result");
    for q in &qs {
        answer(&mut wb, worker, q);
    }
    let saved = write_saves(&mut wb, None);
    assert_eq!(saved.len(), 1);
    let text = std::fs::read_to_string(&saved[0]).unwrap();
    assert_eq!(text.matches("LZ[").count(), 4, "all four moves are in the copy");
}

#[test]
fn a_late_reply_of_a_failed_flash_attempt_is_not_taken_for_the_next_attempt() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, _fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_A);
    wb.open_batch(BatchMode::Flash, vec![a], OpenSource::Chooser).unwrap();
    deliver_reads(&mut wb);
    wb.start_batch_flash(-1, -1, 2, false).unwrap().unwrap();
    deliver_reads(&mut wb);
    let worker = *world.workers().last().unwrap();
    let old = queries(&world, worker);
    let error = serde_json::json!({"id": old[0]["id"], "error": "illegal query"});
    out(&mut wb, worker, &error.to_string());
    assert_eq!(
        wb.view(false).batch.notice.unwrap().message_key,
        "AnalysisEngine.requestDispatchFailed"
    );
    assert_eq!(wb.setup_batch(BatchMode::Flash), Ok(true));
    wb.start_batch_flash(-1, -1, 2, false).unwrap().unwrap();
    deliver_reads(&mut wb);
    assert_eq!(*world.workers().last().unwrap(), worker, "the kept process is reused");
    queries(&world, worker);
    answer(&mut wb, worker, &old[1]);
    assert_eq!(wb.view(false).batch.running.unwrap().done, 0);
}

#[test]
fn flash_batch_queries_use_each_records_own_rules() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, _fg) = setup(dir.path(), false);
    // Japanese first, then Chinese (the foreground's own rules are Chinese).
    let a = write(dir.path(), "a.sgf", "(;SZ[19]RU[Japanese];B[dd];W[pp])");
    let b = write(dir.path(), "b.sgf", "(;SZ[19]RU[Chinese];B[pd];W[dp])");
    wb.open_batch(BatchMode::Flash, vec![a, b], OpenSource::Chooser).unwrap();
    deliver_reads(&mut wb);
    wb.start_batch_flash(-1, -1, 2, false).unwrap().unwrap();
    deliver_reads(&mut wb);
    let worker = *world.workers().last().unwrap();
    let scoring = |qs: &[serde_json::Value]| qs.iter().map(|q| q["rules"]["scoring"].clone()).collect::<Vec<_>>();
    let first = queries(&world, worker);
    assert_eq!(scoring(&first), vec![serde_json::json!("TERRITORY"); 2], "a's own rules");
    for q in &first {
        answer(&mut wb, worker, q);
    }
    write_saves(&mut wb, None);
    deliver_reads(&mut wb);
    let second = queries(&world, worker);
    assert_eq!(scoring(&second), vec![serde_json::json!("AREA"); 2], "b's own rules, not a's");
}

#[test]
fn an_unreadable_first_record_ends_batch_analysis_before_its_settings() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, _world, _fg) = setup(dir.path(), false);
    let bad = write(dir.path(), "bad.sgf", "not a game record");
    let b = write(dir.path(), "b.sgf", GAME_B);
    wb.open_batch(BatchMode::Ordinary, vec![bad.clone(), b], OpenSource::Chooser)
        .unwrap();
    deliver_reads(&mut wb);
    let v = wb.view(false);
    assert!(v.batch.setup.is_none(), "no settings dialog");
    assert!(v.batch.items.is_empty());
    let notice = v.batch.notice.unwrap();
    assert_eq!(notice.message_key, "LizzieFrame.prompt.failedToOpenFile");
    assert_eq!(notice.failures[0].path, bad);
}

#[test]
fn the_first_chosen_record_is_remembered_even_if_the_settings_are_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, _world, _fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_A);
    let b = write(dir.path(), "b.sgf", GAME_B);
    wb.open_batch(BatchMode::Ordinary, vec![a.clone(), b], OpenSource::Chooser)
        .unwrap();
    deliver_reads(&mut wb);
    assert_eq!(wb.view(false).batch.setup, Some(BatchMode::Ordinary));
    assert!(wb.cancel_batch_setup());
    assert!(wb.view(false).recent_files.contains(&a));
}

#[test]
fn batch_analysis_leaves_single_record_auto_save_on_like_java() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, _world, _fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_A);
    wb.open_batch(BatchMode::Ordinary, vec![a], OpenSource::Chooser)
        .unwrap();
    deliver_reads(&mut wb);
    // 自动保存棋谱 was unchecked for single-record analysis.
    assert_eq!(wb.save_auto_analysis_settings(ordinary()).unwrap(), None);
    assert!(!wb.config().auto_analysis.auto_save);
    wb.start_batch_ordinary(ordinary()).unwrap().unwrap();
    assert!(wb.config().auto_analysis.auto_save);
}

#[test]
fn the_batch_dialogs_check_single_record_auto_save_even_when_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, _world, _fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_A);
    for mode in [BatchMode::Ordinary, BatchMode::Flash] {
        assert_eq!(wb.save_auto_analysis_settings(ordinary()).unwrap(), None);
        assert!(!wb.config().auto_analysis.auto_save);
        wb.open_batch(mode, vec![a.clone()], OpenSource::Chooser).unwrap();
        deliver_reads(&mut wb);
        assert!(wb.cancel_batch_setup());
        assert!(wb.config().auto_analysis.auto_save, "{mode:?}");
        wb.clear_batch();
    }
}

#[test]
fn stopping_batch_analysis_while_its_first_record_loads_ends_it_quietly() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, _world, _fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_A);
    let b = write(dir.path(), "b.sgf", GAME_B);
    wb.open_batch(BatchMode::Ordinary, vec![a, b], OpenSource::Chooser)
        .unwrap();
    assert_eq!(wb.view(false).batch.loading, Some(BatchMode::Ordinary));
    assert!(wb.stop_auto_analysis(), "停止自动(批量)分析 cancels the pending start");
    deliver_reads_expecting(&mut wb, false);
    let v = wb.view(false);
    assert!(v.batch.setup.is_none(), "no settings dialog");
    assert!(v.batch.items.is_empty() && v.batch.running.is_none());
    assert!(v.batch.notice.is_none(), "a cancelled start shows nothing");
}

#[test]
fn single_record_auto_analysis_is_refused_while_the_first_batch_record_loads() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, _world, _fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_A);
    wb.open_batch(BatchMode::Ordinary, vec![a], OpenSource::Chooser)
        .unwrap();
    assert_eq!(wb.start_auto_analysis(ordinary()).unwrap(), Err("AutoAnalyze.conflict"));
}

#[test]
fn the_tables_start_opens_batch_analysis_of_the_remaining_records_like_the_menu() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, _world, _fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_A);
    let b = write(dir.path(), "b.sgf", GAME_B);
    // 批量精析's cancelled dialog leaves the records in 进度表.
    wb.open_batch(BatchMode::Flash, vec![a, b], OpenSource::Drop).unwrap();
    deliver_reads(&mut wb);
    assert!(wb.cancel_batch_setup());
    assert_eq!(wb.save_auto_analysis_settings(ordinary()).unwrap(), None);
    assert!(!wb.config().auto_analysis.auto_save);

    // 进度表's 开始/结束: `openOrdinaryBatchAnalysis(remaining, false)`.
    assert_eq!(wb.setup_batch(BatchMode::Ordinary), Ok(true));
    assert_eq!(wb.view(false).batch.setup, None, "the first record is read first");
    assert_eq!(deliver_reads(&mut wb), 1);
    let v = wb.view(false);
    assert_eq!(v.batch.setup, Some(BatchMode::Ordinary));
    assert!(wb.config().auto_analysis.auto_save, "自动保存棋谱 is checked first");
    assert!(
        v.recent_files.iter().any(|f| f.ends_with("a.sgf")),
        "loaded with fromTemp = false"
    );
    assert!(wb.cancel_batch_setup());
    assert!(wb.view(false).batch.items.is_empty(), "cancelling ends the batch");
    assert!(wb.config().auto_analysis.auto_save);
}

/// A record carrying secondary-engine analysis (`LZOP2` at the root, `LZ2` on both moves), each
/// with two distinct candidates whose variations are several moves long.
const WITH_SECONDARY: &str = "(;SZ[19]KM[7.5]LZOP2[b6 50.0 80 0.5 1.0\nmove Q4 visits 50 winrate 5000 scoreMean 0.5 order 0 pv Q4 D16 \
info move R16 visits 30 winrate 4800 scoreMean -0.5 order 1 pv R16 C4]\
;B[pd]LZ2[b6 40.0 100 1.0 2.0\nmove D4 visits 70 winrate 6000 scoreMean 1.0 order 0 pv D4 Q16 C17 \
info move E16 visits 30 winrate 5800 scoreMean 0.4 order 1 pv E16 C3]\
;W[dd]LZ2[b6 45.0 200 -2.0 1.5\nmove C3 visits 120 winrate 5500 scoreMean -2.0 order 0 pv C3 Q3 \
info move R3 visits 80 winrate 5300 scoreMean -2.5 order 1 pv R3 C16])";

/// The candidates of each payload under `tags` along the main line, as (move, visits, win rate).
fn saved_candidates(text: &str, tags: [&str; 2]) -> Vec<Vec<(String, String, String)>> {
    let rec = sgf::read(text).unwrap();
    let mut found = Vec::new();
    let mut node = Some(ROOT);
    while let Some(id) = node {
        for p in rec.node(id).properties.iter().filter(|p| tags.contains(&p.id.as_str())) {
            let detail = p.values[0].split_once('\n').map_or("", |(_, d)| d);
            found.push(
                detail
                    .split(" info ")
                    .map(|c| {
                        let t: Vec<&str> = c.split_whitespace().collect();
                        let value = |key: &str| t.iter().position(|k| *k == key).map_or(String::new(), |i| t[i + 1].to_string());
                        (value("move"), value("visits"), value("winrate"))
                    })
                    .collect(),
            );
        }
        node = rec.main_child(id);
    }
    found
}

fn source_secondary() -> Vec<Vec<(String, String, String)>> {
    let c = |m: &str, v: &str, w: &str| (m.to_string(), v.to_string(), w.to_string());
    vec![
        vec![c("Q4", "50", "5000"), c("R16", "30", "4800")],
        vec![c("D4", "70", "6000"), c("E16", "30", "5800")],
        vec![c("C3", "120", "5500"), c("R3", "80", "5300")],
    ]
}

/// 最大候选点数 1: the secondary engine's results the copy carries over keep both of their
/// candidates, in order with their values (Java reads secondary-engine tags with 361 as the limit,
/// `SGFParser.parseAnalysisInfoForDetachedNode`).
fn limit_candidates_to_one(wb: &mut Workbench) {
    let mut analysis = wb.config().engine.analysis.clone();
    analysis.limit_max_suggestion = 1;
    wb.set_analysis_settings(analysis).unwrap();
}

#[test]
fn an_ordinary_batch_copy_keeps_the_sources_secondary_engine_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, fg) = setup(dir.path(), false);
    limit_candidates_to_one(&mut wb);
    let src = write(dir.path(), "s.sgf", WITH_SECONDARY);
    wb.open_batch(BatchMode::Ordinary, vec![src.clone()], OpenSource::Chooser).unwrap();
    deliver_reads(&mut wb);
    wb.start_batch_ordinary(ordinary()).unwrap().unwrap();
    deliver_reads(&mut wb);
    serve_with(&mut wb, &world, fg, Some(
        "info move K10 visits 6 winrate 0.5 scoreLead 0.5 prior 0.2 order 0 pv K10 info move C17 visits 4 winrate 0.4 \
         scoreLead 0.1 prior 0.1 order 1 pv C17 rootInfo visits 10 winrate 0.4 scoreLead -0.5",
    ));
    let written = write_saves(&mut wb, None);
    assert_eq!(written.len(), 1);
    let copy = std::fs::read_to_string(&written[0]).unwrap();
    let own = saved_candidates(&copy, ["LZ", "LZOP"]);
    assert!(!own.is_empty(), "the batch's own results: {copy}");
    assert_eq!(saved_candidates(&copy, ["LZ2", "LZOP2"]), source_secondary(), "{copy}");
    assert_eq!(std::fs::read_to_string(&src).unwrap(), WITH_SECONDARY);
}

#[test]
fn a_flash_batch_copy_keeps_the_sources_secondary_engine_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, _fg) = setup(dir.path(), false);
    limit_candidates_to_one(&mut wb);
    let src = write(dir.path(), "s.sgf", WITH_SECONDARY);
    wb.open_batch(BatchMode::Flash, vec![src], OpenSource::Chooser).unwrap();
    deliver_reads(&mut wb);
    wb.start_batch_flash(-1, -1, 2, false).unwrap().unwrap();
    deliver_reads(&mut wb);
    let worker = *world.workers().last().unwrap();
    for q in queries(&world, worker) {
        let reply = serde_json::json!({
            "id": q["id"],
            "moveInfos": [
                {"move": "K10", "visits": 2, "winrate": 0.5, "scoreLead": 1.0, "order": 0, "pv": ["K10"]},
                {"move": "C17", "visits": 1, "winrate": 0.4, "scoreLead": 0.2, "order": 1, "pv": ["C17"]},
            ],
            "rootInfo": {"visits": 3, "winrate": 0.5, "scoreLead": 1.0},
        });
        out(&mut wb, worker, &reply.to_string());
    }
    let written = write_saves(&mut wb, None);
    assert_eq!(written.len(), 1);
    let copy = std::fs::read_to_string(&written[0]).unwrap();
    let own = saved_candidates(&copy, ["LZ", "LZOP"]);
    assert!(!own.is_empty(), "the batch's own results: {copy}");
    assert_eq!(saved_candidates(&copy, ["LZ2", "LZOP2"]), source_secondary(), "{copy}");
}

/// The `kata-set-rules` argument in effect at each finite search of the position whose first
/// move is `first`, read from the commands the engine was sent (`None`: never set).
fn rules_at_searches(sent: &[String], first: &str) -> Vec<Option<String>> {
    let (mut rules, mut moves, mut found) = (None, Vec::new(), Vec::new());
    for c in sent {
        let (word, arg) = c.split_once(' ').unwrap_or((c, ""));
        match word {
            "kata-set-rules" => rules = Some(arg.to_string()),
            "boardsize" | "clear_board" => moves.clear(),
            "play" => moves.push(arg.to_string()),
            "undo" => {
                moves.pop();
            }
            "kata-analyze" if c.contains("maxmoves") && moves.first().is_some_and(|m| m == first) => {
                found.push(rules.clone())
            }
            _ => {}
        }
    }
    found
}

/// A borrowed 批量精析 with 使用当前规则: a record without `RU` is searched with the rules captured
/// from the engine, not with those the record before it set.
#[test]
fn a_borrowed_flash_batch_searches_a_record_without_rules_with_the_engines_own() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, fg) = setup(dir.path(), true);
    assert!(wb.config().analysis_engine.use_current_rules);
    let a = write(dir.path(), "a.sgf", "(;SZ[19]RU[Japanese];B[pd];W[dd])");
    let b = write(dir.path(), "b.sgf", GAME_B);
    wb.open_batch(BatchMode::Flash, vec![a, b], OpenSource::Chooser).unwrap();
    deliver_reads(&mut wb);
    wb.start_batch_flash(-1, -1, 2, false).unwrap().unwrap();
    let mut sent = Vec::new();
    for _ in 0..20 {
        deliver_reads(&mut wb);
        sent.extend(serve(&mut wb, &world, fg, 5));
        write_saves(&mut wb, None);
        if wb.view(false).batch.running.is_none() {
            break;
        }
    }
    assert!(wb.view(false).batch.running.is_none(), "the batch completed");
    let at_b = rules_at_searches(&sent, "B D16");
    assert!(!at_b.is_empty());
    assert!(at_b.iter().all(|r| r.as_deref() == Some(CHINESE)), "{at_b:?}");
}

/// With 手动暂停分析则退出自动分析 off, the paused batch keeps its pause when another record opens:
/// the pause belongs to the batch record, not to the record that was open.
#[test]
fn opening_another_record_keeps_a_paused_ordinary_batch_paused() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, fg) = setup(dir.path(), false);
    let a = write(dir.path(), "a.sgf", GAME_A);
    let b = write(dir.path(), "b.sgf", GAME_B);
    wb.open_batch(BatchMode::Ordinary, vec![a, b], OpenSource::Chooser).unwrap();
    deliver_reads(&mut wb);
    wb.start_batch_ordinary(AutoAnalysisSettings { exit_by_pause: false, ..ordinary() }).unwrap().unwrap();
    deliver_reads(&mut wb);
    serve(&mut wb, &world, fg, 3);
    wb.toggle_analysis().unwrap();
    serve(&mut wb, &world, fg, 0);
    assert!(wb.view(false).engine.paused && wb.view(false).auto_analysis.active);

    let t = wb.begin_open(OpenOrigin::File { path: "/c.sgf".into(), source: OpenSource::Drop });
    assert!(wb.complete_open(t, Ok((RecordFormat::Sgf, sgf::read("(;SZ[19];B[qq])").unwrap()))));
    let after = serve(&mut wb, &world, fg, 0);
    let v = wb.view(false);
    assert!(v.engine.paused && v.auto_analysis.active, "{after:?}");
    assert!(!after.iter().any(|c| c.starts_with("kata-analyze")), "{after:?}");
    // Resuming continues the batch.
    wb.toggle_analysis().unwrap();
    assert!(serve(&mut wb, &world, fg, 0).iter().any(|c| c.starts_with("kata-analyze")));
}

/// A one-shot 只分析此点 belongs to the open record's next search only: batch searches do not
/// carry it.
#[test]
fn a_one_shot_region_of_the_open_record_does_not_reach_the_batch_searches() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, world, fg) = setup(dir.path(), false);
    assert!(wb.region(RegionCommand::AllowPoint { point: Point { x: 9, y: 9 } }).unwrap());
    let first = serve(&mut wb, &world, fg, 0);
    assert!(first.iter().any(|c| c.starts_with("kata-analyze") && c.contains("allow")), "{first:?}");

    let b = write(dir.path(), "b.sgf", GAME_B);
    wb.open_batch(BatchMode::Ordinary, vec![b], OpenSource::Chooser).unwrap();
    deliver_reads(&mut wb);
    wb.start_batch_ordinary(ordinary()).unwrap().unwrap();
    deliver_reads(&mut wb);
    let batch = serve(&mut wb, &world, fg, 10);
    let searches: Vec<&String> = batch.iter().filter(|c| c.starts_with("kata-analyze")).collect();
    assert!(!searches.is_empty());
    assert!(searches.iter().all(|c| !c.contains("allow")), "{searches:?}");
}

//! 更多保存 exports, 存档与读档 slots, the automatic saves and the Java slot import through the
//! public `Workbench` API, on a real file system (fixed Java `SgfSaveCoordinator.Mode`,
//! `LizzieFrame.saveTempGame` / `saveAutoGame`, `Lizzie.java` start-up rotation and resume).

use std::path::{Path, PathBuf};

use base64::Engine as _;
use lizzie_core::api::{
    CommentLabels, ExportKind, ImportReason, ImportStatus, OpenOutcome, OpenSource, RecoveryCommand, RecoveryImport,
    SaveOutcome, SlotRef,
};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::formats::{self, sgf};
use lizzie_core::java_import::{self, JavaSource};
use lizzie_core::record::{GameRecord, NodeId, NodeKind, Point, RecordFormat, Stone, ROOT};
use lizzie_core::settings::{RecoverySettings, Storage};
use lizzie_core::workbench::{OpenOrigin, RecoveryStep, Workbench};
use serde_json::json;

const AP: &str = "LizzieYzy Tauri:test";

/// Root comment and analysis, an application marker, a move comment with analysis and a mark,
/// and two branches after the first move.
const RECORD: &str = "(;GM[1]SZ[9]AP[Other:1]DZ[G]C[root note]LZOP[root analysis]\
    ;B[ee]C[first]LZ[move analysis]TR[ee]\
    (;W[cc]C[main]MN[2];B[gg])\
    (;W[gc]C[side];B[cg]))";

struct NoEngines;

impl Launcher for NoEngines {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        Err(std::io::Error::other("no engines here"))
    }
}

fn storage(dir: &Path) -> Storage {
    Storage::in_dirs(&dir.join("config"), &dir.join("data"))
}

fn workbench(dir: &Path) -> Workbench {
    Workbench::new(storage(dir), None, Box::new(NoEngines), Vec::new())
}

fn labels() -> CommentLabels {
    CommentLabels {
        black: "黑棋".into(),
        white: "白棋".into(),
        winrate: "胜率:".into(),
        lead: "领先:".into(),
        stdev: "不确定度:".into(),
        playouts: "计算量".into(),
        komi: "贴目:".into(),
    }
}

fn open_text(wb: &mut Workbench, text: &str) {
    let t = wb.begin_open(OpenOrigin::File {
        path: "/x.sgf".into(),
        source: OpenSource::Drop,
    });
    assert!(wb.complete_open(
        t,
        formats::parse(RecordFormat::Sgf, text, false).map(|r| (RecordFormat::Sgf, r))
    ));
}

/// The node reached by following moves from the root.
fn node_at(rec: &GameRecord, moves: &[(Stone, u16, u16)]) -> NodeId {
    let mut id = ROOT;
    for &(color, x, y) in moves {
        id = *rec
            .node(id)
            .children
            .iter()
            .find(|&&c| {
                rec.node(c).kind
                    == NodeKind::Move {
                        color,
                        point: Point { x, y },
                    }
            })
            .expect("move in record");
    }
    id
}

fn moves_to(rec: &GameRecord, node: NodeId) -> Vec<NodeKind> {
    rec.path_to(node).iter().skip(1).map(|&n| rec.node(n).kind).collect()
}

fn png() -> String {
    let mut out = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(4, 4, image::Rgba([10, 20, 30, 255]))
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    base64::engine::general_purpose::STANDARD.encode(out.into_inner())
}

fn run(wb: &mut Workbench, op: RecoveryCommand) -> RecoveryStep {
    wb.recovery_command(op, AP).unwrap()
}

/// Loads a slot the way the platform adapter does.
fn load(wb: &mut Workbench, slot: SlotRef) {
    let RecoveryStep::Load { ticket, path } = run(wb, RecoveryCommand::LoadSlot { slot }) else {
        panic!("a slot load reads a file");
    };
    wb.complete_open(ticket, formats::read_file(&path, false));
}

fn names(wb: &mut Workbench) -> Vec<String> {
    wb.view(false).recovery.slots.iter().map(|s| s.name.clone()).collect()
}

// ------------------------------------------------------------------ exports

#[test]
fn exports_leave_out_analysis_and_follow_each_java_mode() {
    let tmp = tempfile::tempdir().unwrap();
    let mut wb = workbench(tmp.path());
    open_text(&mut wb, RECORD);
    let side = node_at(&wb.document().record, &[(Stone::Black, 4, 4), (Stone::White, 6, 2)]);
    wb.goto_node(side).unwrap();

    let raw = wb.export_text(ExportKind::Raw, &labels());
    for gone in ["LZ[", "LizzieYzy Tauri", "C[first]", "C[side]"] {
        assert!(!raw.contains(gone), "raw keeps {gone}: {raw}");
    }
    // The whole tree, marks, unmodelled properties, the root's own properties as read (Java writes
    // `rootData.propertiesString()`), the root comment and root analysis stay.
    // The whole tree, marks, unmodelled properties, the root comment and root analysis stay.
    for kept in [
        "AP[Other:1]",
        "DZ[G]",
        "C[root note]",
        "LZOP[root analysis]",
        "TR[ee]",
        "MN[2]",
        "(;W[cc]",
        "(;W[gc]",
    ] {
        assert!(raw.contains(kept), "raw drops {kept}: {raw}");
    }

    let commented = wb.export_text(ExportKind::RawWithComments, &labels());
    for kept in ["C[root note]", "C[first]", "C[main]", "C[side]"] {
        assert!(commented.contains(kept), "{kept}: {commented}");
    }
    assert!(!commented.contains("LZ[") && commented.contains("AP[Other:1]"));

    // Root to the current node, then first children: one line, no comments or root analysis.
    let branch = wb.export_text(ExportKind::Branch, &labels());
    let line = sgf::read(&branch).unwrap();
    let end = line.line_end(ROOT);
    assert_eq!(
        moves_to(&line, end),
        [
            NodeKind::Move {
                color: Stone::Black,
                point: Point { x: 4, y: 4 }
            },
            NodeKind::Move {
                color: Stone::White,
                point: Point { x: 6, y: 2 }
            },
            NodeKind::Move {
                color: Stone::Black,
                point: Point { x: 2, y: 6 }
            },
        ]
    );
    assert_eq!(line.node_count(), 4, "no other branch: {branch}");
    for gone in ["C[", "LZOP[", "LZ["] {
        assert!(!branch.contains(gone), "branch keeps {gone}: {branch}");
    }
    assert!(branch.contains("AP[Other:1]") && branch.contains("DZ[G]"), "{branch}");
    for text in [&raw, &commented] {
        assert!(sgf::read(text).is_ok());
    }
}

#[test]
fn only_the_raw_export_makes_the_file_the_records_own_and_it_stays_unsaved() {
    let tmp = tempfile::tempdir().unwrap();
    let own = tmp.path().join("own.sgf");
    std::fs::write(&own, RECORD).unwrap();
    let mut wb = workbench(tmp.path());
    let t = wb.begin_open(OpenOrigin::File {
        path: own.to_string_lossy().into_owned(),
        source: OpenSource::Drop,
    });
    wb.complete_open(t, formats::read_file(&own, false));
    assert!(!wb.view(false).document.unsaved);

    let export = |wb: &mut Workbench, kind: ExportKind, path: &Path| {
        let job = wb.begin_export(kind, &labels(), &path.to_string_lossy());
        let result = formats::write_record_file(&job.path, &job.text).map_err(|e| e.to_string());
        wb.complete_save(job.ticket, result);
        wb.view(false)
    };
    let out = tmp.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let branch = out.join("branch.sgf");
    let v = export(&mut wb, ExportKind::Branch, &branch);
    assert!(matches!(
        v.save_outcome,
        Some(SaveOutcome::Saved {
            export: Some(ExportKind::Branch),
            ..
        })
    ));
    assert_eq!(v.document.path.as_deref(), Some(own.to_str().unwrap()));
    assert!(!v.document.unsaved, "an export confirms nothing and changes nothing");
    assert_eq!(wb.persisted().last_folder.as_deref(), Some(out.to_str().unwrap()));

    let raw = out.join("raw.sgf");
    let v = export(&mut wb, ExportKind::Raw, &raw);
    assert_eq!(v.document.path.as_deref(), Some(raw.to_str().unwrap()));
    assert!(v.document.unsaved, "the raw file lacks comments and analysis");
    assert!(!std::fs::read_to_string(&raw).unwrap().contains("C[first]"));
    assert_eq!(
        std::fs::read_to_string(&own).unwrap(),
        RECORD,
        "the source file is untouched"
    );
}

// ------------------------------------------------------------------ slots

#[test]
fn a_slot_reopens_its_record_at_the_saved_position_without_owning_a_file() {
    let tmp = tempfile::tempdir().unwrap();
    let mut wb = workbench(tmp.path());
    open_text(&mut wb, RECORD);
    let side = node_at(&wb.document().record, &[(Stone::Black, 4, 4), (Stone::White, 6, 2)]);
    wb.goto_node(side).unwrap();
    let thumbnail = png();
    run(
        &mut wb,
        RecoveryCommand::NewSlot {
            name: "one".into(),
            thumbnail: thumbnail.clone(),
        },
    );
    let slot = wb.view(false).recovery.slots[0].clone();
    assert_eq!(
        (slot.name.as_str(), slot.move_number, slot.has_thumbnail),
        ("one", 2, true)
    );
    assert_eq!(
        wb.slot_thumbnail(SlotRef::Manual { index: 1 }),
        Some(base64::engine::general_purpose::STANDARD.decode(thumbnail).unwrap())
    );

    open_text(&mut wb, "(;SZ[19];B[pd])");
    let before = wb.document().id;
    load(&mut wb, SlotRef::Manual { index: 1 });
    let v = wb.view(false);
    assert_ne!(v.document.id, before);
    let rec = &wb.document().record;
    assert_eq!(
        moves_to(rec, wb.cursor()),
        [
            NodeKind::Move {
                color: Stone::Black,
                point: Point { x: 4, y: 4 }
            },
            NodeKind::Move {
                color: Stone::White,
                point: Point { x: 6, y: 2 }
            },
        ]
    );
    assert_eq!(rec.node(node_at(rec, &[(Stone::Black, 4, 4)])).comment, "first");
    assert_eq!(
        (v.document.path, v.document.origin_name.as_deref()),
        (None, Some("game1.sgf"))
    );
    assert!(v.document.unsaved);
    assert!(wb.config().recent_files.is_empty(), "a slot is not a recent record");
}

#[test]
fn deleting_a_slot_moves_the_later_ones_up_and_delete_all_keeps_the_automatic_slot() {
    let tmp = tempfile::tempdir().unwrap();
    let mut wb = workbench(tmp.path());
    for (name, record) in [
        ("a", "(;SZ[9];B[aa])"),
        ("b", "(;SZ[9];B[bb])"),
        ("c", "(;SZ[9];B[cc])"),
    ] {
        open_text(&mut wb, record);
        run(
            &mut wb,
            RecoveryCommand::NewSlot {
                name: name.into(),
                thumbnail: png(),
            },
        );
    }
    run(
        &mut wb,
        RecoveryCommand::DeleteSlot {
            slot: SlotRef::Manual { index: 2 },
        },
    );
    assert_eq!(names(&mut wb), ["a", "c"]);
    load(&mut wb, SlotRef::Manual { index: 2 });
    assert_eq!(wb.document().record.node_count(), 2);
    assert!(matches!(
        wb.document().record.node(wb.document().record.line_end(ROOT)).kind,
        NodeKind::Move {
            point: Point { x: 2, y: 2 },
            ..
        }
    ));
    assert!(!tmp.path().join("data/save/game3.sgf").exists());

    run(
        &mut wb,
        RecoveryCommand::RenameSlot {
            index: 1,
            name: "renamed".into(),
        },
    );
    // 保存 replaces the slot and gives it the record's save name.
    open_text(&mut wb, "(;SZ[9];B[dd])");
    run(
        &mut wb,
        RecoveryCommand::OverwriteSlot {
            index: 2,
            name: "Untitled".into(),
            thumbnail: png(),
        },
    );
    assert_eq!(names(&mut wb), ["renamed", "Untitled"]);
    load(&mut wb, SlotRef::Manual { index: 2 });
    assert!(matches!(
        wb.document().record.node(wb.document().record.line_end(ROOT)).kind,
        NodeKind::Move {
            point: Point { x: 3, y: 3 },
            ..
        }
    ));

    run(&mut wb, RecoveryCommand::ExitAutoSave { thumbnail: None });
    run(&mut wb, RecoveryCommand::DeleteAllSlots);
    let v = wb.view(false).recovery;
    assert!(v.slots.is_empty());
    assert!(v.auto.is_some());
}

#[test]
fn a_crash_promotes_the_periodic_save_and_the_next_start_resumes_it() {
    let tmp = tempfile::tempdir().unwrap();
    let mut wb = workbench(tmp.path());
    run(
        &mut wb,
        RecoveryCommand::SetSettings {
            settings: RecoverySettings {
                resume_previous_game: true,
                ..RecoverySettings::default()
            },
        },
    );
    open_text(&mut wb, "(;SZ[9];B[aa])");
    run(&mut wb, RecoveryCommand::ExitAutoSave { thumbnail: None });
    open_text(&mut wb, RECORD);
    run(&mut wb, RecoveryCommand::AutoSave { thumbnail: png() });
    // The process dies here: no exit save.
    drop(wb);

    let mut wb = workbench(tmp.path());
    let auto = wb.view(false).recovery.auto.expect("automatic slot");
    assert!(auto.has_thumbnail, "the periodic save's picture comes along");
    let (ticket, path) = wb.begin_resume().expect("resume");
    assert!(path.ends_with("autoGame1.sgf"));
    wb.complete_open(ticket, formats::read_file(&path, false));
    let rec = &wb.document().record;
    assert_eq!(rec.node_count(), 6, "the newer periodic save, not the older exit save");
    assert_eq!(wb.cursor(), rec.line_end(ROOT), "resume shows the end of the main line");

    // A clean exit afterwards: its own save is what the next start offers.
    open_text(&mut wb, "(;SZ[9];B[aa];W[bb])");
    run(&mut wb, RecoveryCommand::ExitAutoSave { thumbnail: None });
    drop(wb);
    let mut wb = workbench(tmp.path());
    let (ticket, path) = wb.begin_resume().unwrap();
    wb.complete_open(ticket, formats::read_file(&path, false));
    assert_eq!(wb.document().record.node_count(), 3);
}

#[test]
fn nothing_is_saved_or_resumed_automatically_while_switched_off() {
    let tmp = tempfile::tempdir().unwrap();
    let mut wb = workbench(tmp.path());
    open_text(&mut wb, RECORD);
    run(&mut wb, RecoveryCommand::ExitAutoSave { thumbnail: None });
    assert!(wb.begin_resume().is_none(), "启动时自动恢复 is off by default");
    run(
        &mut wb,
        RecoveryCommand::SetSettings {
            settings: RecoverySettings {
                auto_save_on_exit: false,
                resume_previous_game: true,
                zoom_thumbnails: true,
            },
        },
    );
    let before = std::fs::read(tmp.path().join("data/save/autoGame1.sgf")).unwrap();
    open_text(&mut wb, "(;SZ[9];B[aa])");
    run(&mut wb, RecoveryCommand::AutoSave { thumbnail: png() });
    run(&mut wb, RecoveryCommand::ExitAutoSave { thumbnail: None });
    assert!(!tmp.path().join("data/save/autoGame2.sgf").exists());
    assert_eq!(
        std::fs::read(tmp.path().join("data/save/autoGame1.sgf")).unwrap(),
        before
    );
    drop(wb);
    let wb = workbench(tmp.path());
    assert!(
        wb.config().recovery.resume_previous_game,
        "the check boxes are remembered"
    );
}

#[test]
fn a_slot_that_cannot_be_read_keeps_the_current_record() {
    let tmp = tempfile::tempdir().unwrap();
    let mut wb = workbench(tmp.path());
    open_text(&mut wb, "(;SZ[9];B[aa])");
    run(
        &mut wb,
        RecoveryCommand::NewSlot {
            name: "x".into(),
            thumbnail: png(),
        },
    );
    std::fs::write(tmp.path().join("data/save/game1.sgf"), [0xff, 0xfe, 0x00]).unwrap();
    open_text(&mut wb, RECORD);
    let current = wb.document().id;
    load(&mut wb, SlotRef::Manual { index: 1 });
    let v = wb.view(false);
    assert!(
        matches!(v.open_outcome, Some(OpenOutcome::Failed { .. })),
        "{:?}",
        v.open_outcome
    );
    assert_eq!(v.document.id, current);
}

// ------------------------------------------------------------------ Java slots

/// A Java work directory with Java's own slot list: manual slot 1 (picture included), manual
/// slot 2 written relative to Java's program folder, slot 3 whose record is gone, and an
/// automatic slot still marked by an unclean session (taken from `autoGame2`).
fn java_dirs(root: &Path) -> (PathBuf, PathBuf) {
    let java = root.join("java work");
    let program = root.join("java program");
    std::fs::create_dir_all(java.join("save")).unwrap();
    std::fs::create_dir_all(program.join("save")).unwrap();
    std::fs::write(
        java.join("config.txt"),
        json!({ "ui": { "auto-save-exit": false, "resume-previous-game": true, "load-save-zoom": false } }).to_string(),
    )
    .unwrap();
    let save = json!({ "save": {
        "save-game-index": [1, 2, 3],
        "save-game-name": ["开局", "slot two", "gone"],
        "save-game-time": ["2026-10-21 14:04:28", "2026-10-21 14:04:58", "2026-10-21 14:05:00"],
        "save-game-move-number": [0, 2, 0],
        // Newest first; the leading start stone (a setup stone of the record) is skipped.
        "save-game-move-list": ["", "6,2,w_4,4,b_0,0,b", ""],
        "save-auto-game-index1": 1,
        "save-auto-game-time1": "old",
        "save-auto-game-move-number1": 0,
        "save-auto-game-index2": -5,
        "save-auto-game-time2": "2026-10-21 14:06:00",
        "save-auto-game-move-number2": 1,
        "save-auto-game-move-list2": "4,4,b",
        "save-config": true,
    }});
    std::fs::write(java.join("save/save"), save.to_string()).unwrap();
    std::fs::write(java.join("save/game1.sgf"), "(;SZ[9];B[aa];W[bb])").unwrap();
    let mut bmp = std::io::Cursor::new(Vec::new());
    image::RgbImage::from_pixel(3, 3, image::Rgb([1, 2, 3]))
        .write_to(&mut bmp, image::ImageFormat::Bmp)
        .unwrap();
    std::fs::write(java.join("save/game1.bmp"), bmp.into_inner()).unwrap();
    std::fs::write(program.join("save/game2.sgf"), format!("(;AB[aa]{})", &RECORD[2..])).unwrap();
    std::fs::write(java.join("save/autoGame1.sgf"), "(;SZ[9];B[cc])").unwrap();
    std::fs::write(java.join("save/autoGame2.sgf"), RECORD).unwrap();
    (java, program)
}

fn files(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    for e in walk(dir) {
        out.push((e.clone(), std::fs::read(&e).unwrap()));
    }
    out.sort();
    out
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

fn java_preview(wb: &mut Workbench, java: &Path, program: &Path) -> lizzie_core::api::ImportPreviewView {
    let candidate = java_import::read(&JavaSource::new(java, Some(program))).unwrap();
    wb.preview_java_import(candidate)
}

#[test]
fn java_slots_are_imported_only_when_chosen_after_this_products_own() {
    let tmp = tempfile::tempdir().unwrap();
    let (java, program) = java_dirs(tmp.path());
    let java_before = (files(&java), files(&program));
    let product = tmp.path().join("product");
    let mut wb = workbench(&product);
    open_text(&mut wb, "(;SZ[9];B[ee])");
    run(
        &mut wb,
        RecoveryCommand::NewSlot {
            name: "mine".into(),
            thumbnail: png(),
        },
    );
    run(&mut wb, RecoveryCommand::ExitAutoSave { thumbnail: None });
    let own_auto = std::fs::read(product.join("data/save/autoGame1.sgf")).unwrap();
    let current = wb.document().id;

    let p = java_preview(&mut wb, &java, &program);
    let shown: Vec<_> = p
        .recovery
        .iter()
        .map(|s| (s.key.as_str(), s.status, s.reason))
        .collect();
    assert_eq!(
        shown,
        [
            ("save:autoGame2", ImportStatus::Imported, None),
            ("save:game1", ImportStatus::Imported, None),
            ("save:game2", ImportStatus::Imported, None),
            ("save:game3", ImportStatus::Incompatible, Some(ImportReason::Unreadable)),
        ]
    );
    assert_eq!(p.recovery[0].time, "2026-10-21 14:06:00");

    // Not chosen: reported, nothing added.
    let report = wb.apply_java_import(p.id, &[], false, None).unwrap().unwrap();
    assert_eq!(
        (report.recovery[1].status, report.recovery[1].reason),
        (ImportStatus::NotMigrated, Some(ImportReason::NotChosen))
    );
    assert_eq!(names(&mut wb), ["mine"]);
    assert!(
        !wb.config().recovery.auto_save_on_exit,
        "the Java check boxes are settings"
    );

    let p = java_preview(&mut wb, &java, &program);
    let choice = RecoveryImport {
        auto_slot_name: "自动存档".into(),
    };
    let report = wb.apply_java_import(p.id, &[], true, Some(&choice)).unwrap().unwrap();
    assert_eq!(report.recovery[0].status, ImportStatus::Imported);
    assert_eq!(names(&mut wb), ["mine", "自动存档", "开局", "slot two"]);
    let v = wb.view(false).recovery;
    assert!(v.slots[2].has_thumbnail, "Java's BMP picture becomes the slot picture");
    assert_eq!(
        std::fs::read(product.join("data/save/autoGame1.sgf")).unwrap(),
        own_auto
    );
    assert_eq!(wb.document().id, current, "the open record is not touched");
    assert_eq!(
        (files(&java), files(&program)),
        java_before,
        "Java's folders are never written"
    );

    // Java's move list becomes the position (the start stone on `aa` is skipped).
    load(&mut wb, SlotRef::Manual { index: 4 });
    let rec = &wb.document().record;
    assert_eq!(
        moves_to(rec, wb.cursor()),
        [
            NodeKind::Move {
                color: Stone::Black,
                point: Point { x: 4, y: 4 }
            },
            NodeKind::Move {
                color: Stone::White,
                point: Point { x: 6, y: 2 }
            },
        ]
    );
    load(&mut wb, SlotRef::Manual { index: 2 });
    assert_eq!(moves_to(&wb.document().record, wb.cursor()).len(), 1);
}

#[test]
fn imported_java_slots_are_taken_back_when_the_settings_cannot_be_written() {
    let tmp = tempfile::tempdir().unwrap();
    let (java, program) = java_dirs(tmp.path());
    let product = tmp.path().join("product");
    let mut wb = workbench(&product);
    open_text(&mut wb, "(;SZ[9];B[ee])");
    run(
        &mut wb,
        RecoveryCommand::NewSlot {
            name: "mine".into(),
            thumbnail: png(),
        },
    );
    let before = files(&product.join("data/save"));
    // The settings file cannot be replaced.
    std::fs::create_dir_all(storage(&product).config_file.join("blocker")).unwrap();
    let p = java_preview(&mut wb, &java, &program);
    let choice = RecoveryImport {
        auto_slot_name: "auto".into(),
    };
    assert!(wb.apply_java_import(p.id, &[], true, Some(&choice)).is_err());
    assert_eq!(names(&mut wb), ["mine"]);
    assert_eq!(files(&product.join("data/save")), before);
}

//! Saving through the public `Workbench` and format APIs. A saved record must reopen with the same
//! meaning as the record that was saved (fixed Java `SGFParser` save/parse contract and
//! `SNAPSHOT_NODE_KIND.md`), compared by structure rather than bytes; save receipts confirm only
//! the captured document revision.

use lizzie_core::api::{OpenSource, SaveOutcome};
use lizzie_core::formats::{self, sgf, sgf_write};
use lizzie_core::record::{DocumentEdit, GameRecord, Mark, NodeId, RecordFormat, ROOT};
use lizzie_core::settings::Storage;
use lizzie_core::workbench::{OpenOrigin, SavePlan, Workbench, SAVE_FILE_FAILED};

const AP: &str = "LizzieYzy Tauri:test";

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

fn storage(dir: &std::path::Path) -> Storage {
    Storage::in_dirs(&dir.join("config"), &dir.join("data"))
}

/// Properties the record keeps as read (not represented by its own fields).
fn retained(rec: &GameRecord, id: NodeId) -> Vec<(String, Vec<String>)> {
    const MODELLED: &[&str] = &[
        "B", "W", "AB", "AW", "AE", "PL", "C", "DD", "LB", "TR", "SQ", "CR", "MA", "SZ", "KM", "KO", "HA", "PB", "PW",
        "BR", "WR", "RE", "DT", "RU", "CA", "AP",
    ];
    let mut v: Vec<_> = rec
        .node(id)
        .properties
        .iter()
        .filter(|p| !MODELLED.contains(&p.id.as_str()))
        .map(|p| (p.id.clone(), p.values.clone()))
        .collect();
    v.sort();
    v
}

fn marks(rec: &GameRecord, id: NodeId) -> Vec<String> {
    let mut v: Vec<String> = rec.node(id).marks.iter().map(|m: &Mark| format!("{m:?}")).collect();
    v.sort();
    v
}

/// Same meaning: game information, and per node (children in order) the kind, side to play, real
/// move number, resulting position and captures, comment, marks and retained properties.
fn assert_same_meaning(expected: &GameRecord, actual: &GameRecord) {
    assert_eq!(expected.size, actual.size);
    assert_eq!(expected.info, actual.info);
    fn node(e: &GameRecord, a: &GameRecord, ei: NodeId, ai: NodeId, path: &str) {
        let (en, an) = (e.node(ei), a.node(ai));
        assert_eq!(en.kind, an.kind, "kind at {path}");
        assert_eq!(en.to_play, an.to_play, "side to play at {path}");
        assert_eq!(en.move_number, an.move_number, "move number at {path}");
        assert_eq!(e.position(ei), a.position(ai), "position at {path}");
        assert_eq!(en.comment, an.comment, "comment at {path}");
        assert_eq!(marks(e, ei), marks(a, ai), "marks at {path}");
        assert_eq!(retained(e, ei), retained(a, ai), "properties at {path}");
        assert_eq!(en.children.len(), an.children.len(), "children at {path}");
        for (k, (&ec, &ac)) in en.children.iter().zip(&an.children).enumerate() {
            node(e, a, ec, ac, &format!("{path}/{k}"));
        }
    }
    node(expected, actual, ROOT, ROOT, "root");
}

fn reopen(rec: &GameRecord) -> GameRecord {
    sgf::read(&sgf_write::write(rec, AP)).expect("saved SGF reads back")
}

/// Setup in several places, an inferred initial side, mid-game replacement and removal, real and
/// placeholder passes, marks, escaped text and the analysis/display properties Java keeps.
const RICH: &str = "(;GM[1]FF[4]SZ[9]KM[6]PB[Black \\] \\\\ name]PW[W]RE[B+R]DT[2026-10-19]RU[Japanese]\
GN[g]LZOP[root analysis\nmove ee]\
(;AB[cc][gg];W[ee]C[first]MN[7]LZ[KataGo 55.0 100\nmove dd visits 10]LZ2[Leela 40.0 50]\
;B[dd]TR[ee]SQ[dd]LB[cc:A][gg:x\\]y:z];W[];AE[cc]AW[gg]AB[aa]PL[W];W[ff]C[after setup]\
(;B[gf])(;DD[true]C[placeholder])(;B[]))\
(;B[ef]))";

#[test]
fn saved_records_reopen_with_the_same_meaning() {
    let rich = sgf::read(RICH).unwrap();
    let (_, review19) = formats::read_file(&fixture("review-19x19-zh.sgf")).unwrap();
    let (_, review9) = formats::read_file(&fixture("review-9x9.sgf")).unwrap();
    let (_, gib) = formats::read_file(&fixture("handicap-2.gib")).unwrap();
    let rect = sgf::read("(;SZ[5:7]AB[aa]AW[ea]PL[W];W[eg];B[])").unwrap();
    for (name, rec) in [
        ("rich", rich),
        ("19", review19),
        ("9", review9),
        ("gib", gib),
        ("5x7", rect),
    ] {
        let once = reopen(&rec);
        assert_same_meaning(&rec, &once);
        // Saving the reopened record again changes nothing (no setup nodes grow, Java contract).
        let twice_text = sgf_write::write(&once, AP);
        assert_eq!(
            sgf_write::write(&sgf::read(&twice_text).unwrap(), AP),
            twice_text,
            "{name}"
        );
    }
}

#[test]
fn initial_side_to_play_is_written_explicitly() {
    // Inferred from the first move (White) and from a completed handicap; Java writes the PL it
    // determined at import so the reopened record cannot be re-inferred differently.
    for (text, white) in [
        ("(;SZ[9];W[ee];B[dd])", true),
        ("(;SZ[9]HA[2]AB[cc][gg])", true),
        ("(;SZ[9])", false),
    ] {
        let saved = sgf_write::write(&sgf::read(text).unwrap(), AP);
        assert!(
            saved.contains(if white { "PL[W]" } else { "PL[B]" }),
            "{text} -> {saved}"
        );
    }
}

#[test]
fn secondary_engine_analysis_keeps_its_own_property() {
    let rec = sgf::read("(;SZ[9];B[ee]LZ[a]LZ2[b];W[dd]LZOP2[c])").unwrap();
    let first = rec.main_child(ROOT).unwrap();
    let ids: Vec<&str> = rec.node(first).properties.iter().map(|p| p.id.as_str()).collect();
    assert!(ids.contains(&"LZ") && ids.contains(&"LZ2"), "{ids:?}");
    let saved = sgf_write::write(&rec, AP);
    assert!(saved.contains("LZ[a]LZ2[b]") && saved.contains("LZOP2[c]"), "{saved}");
}

#[test]
fn passes_are_empty_moves_and_text_is_escaped() {
    // PB is `a]b\`, the comment `x]y`.
    let rec = sgf::read(r"(;SZ[19]PB[a\]b\\];B[];W[tt]C[x\]y])").unwrap();
    assert_eq!(rec.info.black, r"a]b\");
    let saved = sgf_write::write(&rec, AP);
    assert!(saved.contains(";B[]") && saved.contains(";W[]"), "{saved}");
    assert!(saved.contains(r"PB[a\]b\\]") && saved.contains(r"C[x\]y]"), "{saved}");
    let reopened = sgf::read(&saved).unwrap();
    assert_eq!(reopened.info.black, rec.info.black);
    assert_eq!(reopened.node(reopened.line_end(ROOT)).comment, "x]y");
    assert!(saved.starts_with("(;CA[UTF-8]"), "{saved}");
    assert!(saved.contains(&format!("AP[{AP}]")), "{saved}");
}

#[test]
fn a_replaced_setup_stone_survives_java_style_save_and_reopen() {
    use lizzie_core::record::{Point, Stone};
    let gg = Point { x: 6, y: 6 };
    // Java writes a replaced stone as AE then AW in one node (appendSetupStoneDelta) and ignores
    // AB/AW on an occupied point (BoardHistoryList.addExtraStone).
    let java = sgf::read("(;SZ[9]AB[gg];B[aa];PL[W]AE[gg]AW[gg])").unwrap();
    let end = java.line_end(ROOT);
    assert_eq!(java.position(end).at(gg), Some(Stone::White));
    let ignored = sgf::read("(;SZ[9]AB[gg];B[aa];AW[gg])").unwrap();
    assert_eq!(ignored.position(ignored.line_end(ROOT)).at(gg), Some(Stone::Black));
    let saved = sgf_write::write(&java, AP);
    assert!(saved.contains("AE[gg]AW[gg]"), "{saved}");
    assert_same_meaning(&java, &sgf::read(&saved).unwrap());
}

#[test]
fn the_save_target_gains_sgf_unless_present() {
    assert_eq!(formats::save_target("/d/game"), "/d/game.sgf");
    assert_eq!(formats::save_target("/d/game.SGF"), "/d/game.SGF");
    assert_eq!(formats::save_target("/d/game.gib"), "/d/game.gib.sgf");
    assert_eq!(formats::save_target("/d.sgf/game"), "/d.sgf/game.sgf");
}

#[test]
fn a_failed_write_leaves_the_existing_file_and_no_staged_file() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("keep.sgf");
    std::fs::write(&target, "(;C[old])").unwrap();
    // A directory in the way of the rename.
    let blocked = dir.path().join("blocked.sgf");
    std::fs::create_dir(&blocked).unwrap();
    assert!(formats::write_record_file(&blocked, "(;C[new])").is_err());
    // A folder that does not exist.
    assert!(formats::write_record_file(&dir.path().join("missing/x.sgf"), "(;)").is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let ro = dir.path().join("ro");
        std::fs::create_dir(&ro).unwrap();
        std::fs::write(ro.join("keep.sgf"), "(;C[old])").unwrap();
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = formats::write_record_file(&ro.join("keep.sgf"), "(;C[new])");
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(ro.join("keep.sgf")).unwrap(), "(;C[old])");
    }
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "(;C[old])");
    let names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(names.iter().all(|n| !n.ends_with(".tmp")), "{names:?}");
    // A successful write replaces the content as UTF-8 and keeps the file's permissions.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o640)).unwrap();
    }
    formats::write_record_file(&target, "(;C[新])").unwrap();
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "(;C[新])");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o640);
    }
}

fn open_file(wb: &mut Workbench, path: &str) {
    let t = wb.begin_open(OpenOrigin::File {
        path: path.to_string(),
        source: OpenSource::Drop,
    });
    assert!(wb.complete_open(t, formats::read_file(std::path::Path::new(path))));
}

/// Runs a save the way the platform adapter does.
fn save(wb: &mut Workbench, path: &str) -> u64 {
    let job = wb.begin_save(path, AP);
    let result = formats::write_record_file(&job.path, &job.text).map_err(|e| e.to_string());
    wb.complete_save(job.ticket, result);
    job.ticket
}

#[test]
fn save_overwrites_the_own_sgf_after_confirmation_and_chooses_otherwise() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = Workbench::new(storage(dir.path()), None);
    assert_eq!(wb.save_plan(None), SavePlan::Choose, "a new record has no file");
    let gib = dir.path().join("h.GIB");
    std::fs::copy(fixture("handicap-2.gib"), &gib).unwrap();
    open_file(&mut wb, gib.to_str().unwrap());
    assert_eq!(wb.save_plan(None), SavePlan::Choose, "GIB is never written");
    let sgf_path = dir.path().join("a.sgf");
    std::fs::copy(fixture("review-9x9.sgf"), &sgf_path).unwrap();
    open_file(&mut wb, sgf_path.to_str().unwrap());
    let doc = wb.document().id;
    assert_eq!(
        wb.save_plan(None),
        SavePlan::Confirm {
            document: doc,
            file_name: "a.sgf".into()
        }
    );
    assert_eq!(
        wb.save_plan(Some(doc)),
        SavePlan::Overwrite {
            path: sgf_path.to_string_lossy().into_owned()
        }
    );
    // A confirmation given for another document is asked again.
    open_file(&mut wb, sgf_path.to_str().unwrap());
    assert!(matches!(wb.save_plan(Some(doc)), SavePlan::Confirm { .. }));
    // "不再提示" persists and is restored by "重置界面位置".
    wb.set_replace_file_hint(false).unwrap();
    assert!(matches!(wb.save_plan(None), SavePlan::Overwrite { .. }));
    assert!(
        !Workbench::new(storage(dir.path()), None)
            .config()
            .record
            .show_replace_file_hint
    );
    wb.reset_ui_position().unwrap();
    assert!(matches!(wb.save_plan(None), SavePlan::Confirm { .. }));
}

#[test]
fn save_as_of_a_gib_makes_the_written_sgf_the_document_file() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = Workbench::new(storage(dir.path()), None);
    let gib = dir.path().join("in").join("h.gib");
    std::fs::create_dir_all(gib.parent().unwrap()).unwrap();
    std::fs::copy(fixture("handicap-2.gib"), &gib).unwrap();
    open_file(&mut wb, gib.to_str().unwrap());
    let before = wb.document().record.clone();
    let out = dir.path().join("out");
    std::fs::create_dir(&out).unwrap();
    let ticket = save(&mut wb, out.join("copy").to_str().unwrap());
    let written = out.join("copy.sgf");
    let v = wb.view(false);
    assert_eq!(
        v.save_outcome,
        Some(SaveOutcome::Saved {
            ticket,
            document: v.document.id,
            revision: 0,
            path: written.to_string_lossy().into_owned()
        })
    );
    assert_eq!(v.document.path.as_deref(), Some(written.to_str().unwrap()));
    assert_eq!(v.document.format, Some(RecordFormat::Sgf));
    assert!(!v.document.unsaved);
    assert_eq!(wb.persisted().last_folder.as_deref(), Some(out.to_str().unwrap()));
    assert!(
        wb.config().recent_files.is_empty(),
        "saving does not touch the recent list"
    );
    // Later "保存" overwrites the new file; the GIB is left alone.
    assert!(matches!(wb.save_plan(None), SavePlan::Confirm { ref file_name, .. } if file_name == "copy.sgf"));
    assert_eq!(
        std::fs::read(&gib).unwrap(),
        std::fs::read(fixture("handicap-2.gib")).unwrap()
    );
    let (format, reopened) = formats::read_file(&written).unwrap();
    assert_eq!(format, RecordFormat::Sgf);
    assert_same_meaning(&before, &reopened);
}

#[test]
fn a_receipt_confirms_only_the_captured_revision_of_its_document() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = Workbench::new(storage(dir.path()), None);
    let path = dir.path().join("a.sgf");
    std::fs::copy(fixture("review-9x9.sgf"), &path).unwrap();
    open_file(&mut wb, path.to_str().unwrap());
    let v = wb.view(false);
    assert_eq!(
        (v.document.revision, v.document.saved_revision, v.document.unsaved),
        (0, Some(0), false)
    );

    // Edit, start saving, edit again before the write finishes.
    wb.edit_document(DocumentEdit::SetComment {
        node: ROOT,
        comment: "one".into(),
    })
    .unwrap();
    let job = wb.begin_save(path.to_str().unwrap(), AP);
    wb.edit_document(DocumentEdit::SetComment {
        node: ROOT,
        comment: "two".into(),
    })
    .unwrap();
    assert!(wb.has_pending_saves());
    formats::write_record_file(&job.path, &job.text).unwrap();
    wb.complete_save(job.ticket, Ok(()));
    assert!(!wb.has_pending_saves());
    let v = wb.view(false);
    assert_eq!(v.document.revision, 2);
    assert_eq!(v.document.saved_revision, Some(1));
    assert!(v.document.unsaved, "the later edit is still unsaved");
    assert!(matches!(v.save_outcome, Some(SaveOutcome::Saved { revision: 1, .. })));
    assert_eq!(formats::read_file(&path).unwrap().1.node(ROOT).comment, "one");
    // An edit that changes nothing does not create a new revision.
    assert!(!wb
        .edit_document(DocumentEdit::SetComment {
            node: ROOT,
            comment: "two".into()
        })
        .unwrap());
    assert!(wb
        .edit_document(DocumentEdit::SetComment {
            node: 9999,
            comment: "x".into()
        })
        .is_err());
}

#[test]
fn switching_documents_while_saving_leaves_the_new_document_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = Workbench::new(storage(dir.path()), None);
    let a = dir.path().join("a.sgf");
    std::fs::copy(fixture("review-9x9.sgf"), &a).unwrap();
    open_file(&mut wb, a.to_str().unwrap());
    let first = wb.document().id;
    let job = wb.begin_save(dir.path().join("elsewhere").to_str().unwrap(), AP);
    // Another record is opened before the write finishes, then edited.
    let t = wb.begin_open(OpenOrigin::Clipboard);
    wb.complete_open(t, Workbench::read_clipboard_record("(;SZ[9];B[ee])"));
    wb.edit_document(DocumentEdit::SetComment {
        node: ROOT,
        comment: "c".into(),
    })
    .unwrap();
    formats::write_record_file(&job.path, &job.text).unwrap();
    wb.complete_save(job.ticket, Ok(()));
    let v = wb.view(false);
    assert!(matches!(v.save_outcome, Some(SaveOutcome::Saved { document, .. }) if document == first));
    assert_ne!(v.document.id, first);
    assert_eq!(v.document.path, None, "the pasted record did not become the saved file");
    assert_eq!(v.document.saved_revision, None);
    assert!(v.document.unsaved);
    assert_eq!(
        wb.persisted().last_folder,
        None,
        "no effects for a document that is gone"
    );
}

#[test]
fn a_failed_save_reports_and_keeps_document_state() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = Workbench::new(storage(dir.path()), None);
    let a = dir.path().join("a.sgf");
    std::fs::copy(fixture("review-9x9.sgf"), &a).unwrap();
    open_file(&mut wb, a.to_str().unwrap());
    wb.edit_document(DocumentEdit::SetComment {
        node: ROOT,
        comment: "edited".into(),
    })
    .unwrap();
    let ticket = save(&mut wb, dir.path().join("missing/x.sgf").to_str().unwrap());
    let v = wb.view(false);
    match v.save_outcome {
        Some(SaveOutcome::Failed {
            ticket: t,
            ref message_key,
            ..
        }) => {
            assert_eq!(t, ticket);
            assert_eq!(message_key, SAVE_FILE_FAILED);
        }
        ref other => panic!("{other:?}"),
    }
    assert_eq!(v.document.path.as_deref(), Some(a.to_str().unwrap()));
    assert!(v.document.unsaved);
    assert_eq!(
        std::fs::read(&a).unwrap(),
        std::fs::read(fixture("review-9x9.sgf")).unwrap()
    );
    assert!(
        !wb.complete_save(ticket, Ok(())),
        "a finished ticket is not completed twice"
    );
}

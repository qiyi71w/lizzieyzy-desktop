//! Browsing commands and the open/settings flow through the public `Workbench` API, the same
//! operations the UI, shortcuts and acceptance automation send.

use lizzie_core::api::{OpenOutcome, OpenSource, StoneSound};
use lizzie_core::browse::{self, Navigation};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::formats::{sgf, ReadError};
use lizzie_core::record::{BoardSize, GameRecord, NodeId, Point, Stone, ROOT};
use lizzie_core::settings::{FramePlacement, Language, Storage, WindowPlacement};
use lizzie_core::workbench::{OpenOrigin, PasteDecision, Workbench, FAILED_TO_OPEN_FILE, KIFU_LOAD_FAILED};

#[cfg(unix)]
#[path = "support/permissions.rs"]
mod permissions;

/// Browsing runs without engines; nothing is ever launched here.
struct NoEngines;

impl Launcher for NoEngines {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        Err(std::io::Error::other("no engines in browsing tests"))
    }
}

// Main line: root - B ee(1) - W gc(2) - B cg(3) - W gg(4) - B fg(5) - W gf(6)
//                                             \ W cc(4) - B dc(5)
const TREE: &str = "(;SZ[9]C[root];B[ee];W[gc];B[cg](;W[gg]C[c4];B[fg];W[gf])(;W[cc]C[side];B[dc]))";

fn rec(text: &str) -> GameRecord {
    sgf::read(text).unwrap()
}

fn at(rec: &GameRecord, moves: &[usize]) -> NodeId {
    // Follow child indexes from the root.
    let mut id = ROOT;
    for &i in moves {
        id = rec.node(id).children[i];
    }
    id
}

fn nav(rec: &GameRecord, from: NodeId, n: Navigation) -> NodeId {
    browse::navigate(rec, from, n)
}

#[test]
fn forward_back_first_last() {
    let r = rec(TREE);
    let side4 = at(&r, &[0, 0, 0, 1]);
    assert_eq!(nav(&r, ROOT, Navigation::Forward(1)), at(&r, &[0]));
    assert_eq!(nav(&r, ROOT, Navigation::Forward(10)), at(&r, &[0, 0, 0, 0, 0, 0]));
    assert_eq!(nav(&r, side4, Navigation::Back(10)), ROOT);
    assert_eq!(nav(&r, side4, Navigation::First), ROOT);
    // End inside a branch goes to the end of that branch, not back to the main line.
    assert_eq!(nav(&r, side4, Navigation::Last), at(&r, &[0, 0, 0, 1, 0]));
}

#[test]
fn left_right_switch_between_nodes_at_the_same_depth() {
    let r = rec(TREE);
    let main4 = at(&r, &[0, 0, 0, 0]);
    let side4 = at(&r, &[0, 0, 0, 1]);
    assert_eq!(nav(&r, main4, Navigation::NextBranch), side4);
    assert_eq!(nav(&r, side4, Navigation::PreviousBranch), main4);
    assert_eq!(nav(&r, side4, Navigation::NextBranch), side4);
    // Deeper than the main line: the main trunk end comes first.
    let r2 = rec("(;SZ[9];B[ee](;W[aa])(;W[bb];B[cc];W[dd]))");
    let deep = at(&r2, &[0, 1, 0, 0]);
    let main_end = at(&r2, &[0, 0]);
    assert_eq!(nav(&r2, deep, Navigation::PreviousBranch), main_end);
}

#[test]
fn branch_point_and_branch_start() {
    let r = rec(TREE);
    let side5 = at(&r, &[0, 0, 0, 1, 0]);
    let side4 = at(&r, &[0, 0, 0, 1]);
    let junction = at(&r, &[0, 0, 0]);
    // Shift+Up: to the junction child, then (already there) one back.
    assert_eq!(nav(&r, side5, Navigation::BranchStart), side4);
    assert_eq!(nav(&r, side4, Navigation::BranchStart), junction);
    // Ctrl+Left: to the junction; at the junction child it does not move.
    assert_eq!(nav(&r, side5, Navigation::BranchPoint), junction);
    assert_eq!(nav(&r, side4, Navigation::BranchPoint), side4);
}

#[test]
fn comment_navigation_and_main_trunk() {
    let r = rec(TREE);
    let main4 = at(&r, &[0, 0, 0, 0]);
    assert_eq!(nav(&r, ROOT, Navigation::NextComment), main4);
    // No later comment: to the end of the line.
    assert_eq!(nav(&r, main4, Navigation::NextComment), at(&r, &[0, 0, 0, 0, 0, 0]));
    assert_eq!(nav(&r, main4, Navigation::PreviousComment), ROOT);
    let side5 = at(&r, &[0, 0, 0, 1, 0]);
    assert_eq!(nav(&r, side5, Navigation::MainTrunk), at(&r, &[0, 0, 0]));
}

#[test]
fn move_number_jump_returns_to_main_line_when_branch_is_too_short() {
    let r = rec(TREE);
    let side5 = at(&r, &[0, 0, 0, 1, 0]);
    assert_eq!(browse::goto_move_number(&r, side5, 6), at(&r, &[0, 0, 0, 0, 0, 0]));
    assert_eq!(browse::goto_move_number(&r, side5, 2), at(&r, &[0, 0]));
}

#[test]
fn move_number_jump_counts_moves_not_setup_snapshots() {
    // root - B aa(1) - AB snapshot(1) - AW snapshot(1) - W pass(2) - B cc(3)
    let r = rec("(;SZ[9];B[aa];AB[bb];AW[dd];W[];B[cc])");
    let number = |id: NodeId| r.node(id).move_number;
    let snapshot = at(&r, &[0, 0, 0]);
    for (from, want) in [(ROOT, 2), (ROOT, 3), (snapshot, 2), (snapshot, 3), (at(&r, &[0, 0, 0, 0, 0]), 1)] {
        let got = browse::goto_move_number(&r, from, want);
        assert_eq!(number(got), want, "jump to move {want}");
    }
    assert_eq!(browse::goto_move_number(&r, ROOT, 2), at(&r, &[0, 0, 0, 0]));
    // Ordinary single steps still stop on the snapshots.
    assert_eq!(nav(&r, at(&r, &[0]), Navigation::Forward(1)), at(&r, &[0, 0]));

    // Snapshots do not lengthen a branch: move 3 is only on the main trunk.
    // Main: B ee(1) - W gc(2) - B cg(3); side: B ee(1) - AB - AW - W gg(2).
    let r = rec("(;SZ[9];B[ee](;W[gc];B[cg])(;AB[aa];AW[bb];W[gg]))");
    let side_snapshot = at(&r, &[0, 1]);
    assert_eq!(browse::goto_move_number(&r, side_snapshot, 3), at(&r, &[0, 0, 0]));
}

#[test]
fn dummy_placeholder_is_never_landed_on() {
    let r = rec("(;SZ[9];B[ee](;DD[true])(;W[aa]))");
    let first = at(&r, &[0]);
    // The main child is a dummy: forward stops (Java `next(false)`).
    assert_eq!(nav(&r, first, Navigation::Forward(1)), first);
    assert!(!browse::is_browsable(&r, at(&r, &[0, 0])));
}

fn storage(dir: &std::path::Path) -> Storage {
    Storage::in_dirs(&dir.join("config"), &dir.join("data"))
}

fn file_origin(path: &str, source: OpenSource) -> OpenOrigin {
    OpenOrigin::File {
        path: path.into(),
        source,
    }
}

fn read_ok(text: &str) -> Result<(lizzie_core::record::RecordFormat, GameRecord), ReadError> {
    Ok((lizzie_core::record::RecordFormat::Sgf, rec(text)))
}

#[test]
fn starts_empty_19x19_black_to_play_komi_7_5() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = Workbench::new(storage(dir.path()), Some("en-US"), Box::new(NoEngines), Vec::new());
    let v = wb.view(true);
    assert_eq!(v.document.size, BoardSize::STANDARD);
    assert_eq!(v.position.to_play, Stone::Black);
    assert_eq!(v.komi, 7.5);
    assert!(v.position.stones.iter().all(Option::is_none));
    assert!(v.tree.is_some());
}

#[test]
fn a_superseded_open_never_replaces_the_later_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    let slow = wb.begin_open(file_origin("/a.sgf", OpenSource::Chooser));
    let fast = wb.begin_open(file_origin("/b.sgf", OpenSource::Chooser));
    assert!(wb.complete_open(fast, read_ok("(;SZ[9];B[ee])")));
    let doc = wb.document().id;
    assert!(!wb.complete_open(slow, read_ok("(;SZ[13];B[aa])")));
    assert_eq!(wb.document().id, doc);
    assert_eq!(wb.document().record.size.width, 9);
    assert_eq!(wb.config().recent_files, vec!["/b.sgf".to_string()]);
}

#[test]
fn a_failed_open_keeps_the_current_document_and_position() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    let t = wb.begin_open(file_origin("/a.sgf", OpenSource::Chooser));
    wb.complete_open(t, read_ok(TREE));
    wb.navigate(Navigation::Forward(3));
    let (doc, cursor) = (wb.document().id, wb.cursor());
    let t = wb.begin_open(file_origin("/bad.sgf", OpenSource::Chooser));
    assert!(wb.complete_open(t, Err(ReadError::Invalid("x".into()))));
    assert_eq!((wb.document().id, wb.cursor()), (doc, cursor));
    let v = wb.view(false);
    assert!(
        matches!(v.open_outcome, Some(OpenOutcome::Failed { ref message_key, .. }) if message_key == FAILED_TO_OPEN_FILE)
    );
    assert_eq!(wb.config().recent_files, vec!["/a.sgf".to_string()]);
    // The outcome is reported once.
    assert!(wb.view(false).open_outcome.is_none());
}

#[test]
fn switching_board_size_leaves_nothing_of_the_previous_record() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    let t = wb.begin_open(file_origin("/19.sgf", OpenSource::Chooser));
    wb.complete_open(t, read_ok("(;SZ[19];B[pd];W[dd])"));
    wb.navigate(Navigation::Last);
    let t = wb.begin_open(file_origin("/9.sgf", OpenSource::Chooser));
    wb.complete_open(t, read_ok("(;SZ[9];B[ee])"));
    let v = wb.view(false);
    assert_eq!(v.document.size.width, 9);
    assert_eq!(v.position.stones.len(), 81);
    assert_eq!(v.position.node, ROOT, "opened records start at the root");
    assert_eq!(v.tree.as_ref().unwrap().nodes.len(), 2);
    let t = wb.begin_open(file_origin("/19.sgf", OpenSource::Chooser));
    wb.complete_open(t, read_ok("(;SZ[19];B[pd])"));
    assert_eq!(wb.view(false).position.stones.len(), 361);
}

#[test]
fn only_the_chooser_updates_recent_files_and_last_folder() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    for (path, source) in [
        ("/r/a.sgf", OpenSource::Recent),
        ("/d/b.sgf", OpenSource::Drop),
        ("/c/c.sgf", OpenSource::CommandLine),
    ] {
        let t = wb.begin_open(file_origin(path, source));
        wb.complete_open(t, read_ok("(;B[aa])"));
    }
    assert!(wb.config().recent_files.is_empty());
    assert!(wb.persisted().last_folder.is_none());
    for i in 0..7 {
        let t = wb.begin_open(file_origin(&format!("/k/{i}.sgf"), OpenSource::Chooser));
        wb.complete_open(t, read_ok("(;B[aa])"));
    }
    let t = wb.begin_open(file_origin("/k/3.sgf", OpenSource::Chooser));
    wb.complete_open(t, read_ok("(;B[aa])"));
    let v = wb.view(false);
    assert_eq!(
        v.recent_files,
        vec!["/k/3.sgf", "/k/6.sgf", "/k/5.sgf", "/k/4.sgf", "/k/2.sgf"]
    );
    assert_eq!(wb.persisted().last_folder.as_deref(), Some("/k"));
    // Saved and read back by a new session.
    let wb2 = Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    assert_eq!(wb2.config().recent_files.len(), 5);
    assert_eq!(wb2.persisted().last_folder.as_deref(), Some("/k"));
}

#[test]
fn record_komi_applies_only_with_read_komi_or_setup_games() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    let open = |wb: &mut Workbench, text: &str| {
        let t = wb.begin_open(file_origin("/x.sgf", OpenSource::Drop));
        wb.complete_open(t, read_ok(text));
        wb.view(false).komi
    };
    assert_eq!(open(&mut wb, "(;KM[6.5];B[aa])"), 6.5);
    // No KM: the session komi stays.
    assert_eq!(open(&mut wb, "(;B[aa])"), 6.5);
    wb.set_read_komi(false).unwrap();
    assert_eq!(open(&mut wb, "(;KM[0.5];B[aa])"), 6.5);
    // Setup / handicap games still use the record komi.
    assert_eq!(open(&mut wb, "(;HA[2]KM[0.5]AB[dd][pp])"), 0.5);
    let wb2 = Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    assert!(!wb2.config().record.read_komi);
}

#[test]
fn paste_decision_ignores_non_sgf_and_asks_before_replacing() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    assert_eq!(wb.paste_decision("", false), PasteDecision::Ignore);
    assert_eq!(wb.paste_decision("hello", false), PasteDecision::Ignore);
    assert_eq!(wb.paste_decision("(;B[aa])", false), PasteDecision::Proceed);
    let t = wb.begin_open(file_origin("/x.sgf", OpenSource::Drop));
    wb.complete_open(t, read_ok("(;B[aa])"));
    assert_eq!(wb.paste_decision("(;B[bb])", false), PasteDecision::Confirm);
    assert_eq!(wb.paste_decision("(;B[bb])", true), PasteDecision::Proceed);
    let t = wb.begin_open(OpenOrigin::Clipboard);
    wb.complete_open(t, Workbench::read_clipboard_record("(;B[bb]", false));
    let v = wb.view(false);
    assert!(v.document.path.is_none());
    assert_eq!(v.position.stones.iter().flatten().count(), 0);
    let t = wb.begin_open(OpenOrigin::Clipboard);
    wb.complete_open(t, Workbench::read_clipboard_record("(;C[x", false));
    assert!(
        matches!(wb.view(false).open_outcome, Some(OpenOutcome::Failed { ref message_key, .. }) if message_key == KIFU_LOAD_FAILED)
    );
}

#[test]
fn clear_personal_data_removes_only_the_recent_list() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    let t = wb.begin_open(file_origin("/k/a.sgf", OpenSource::Chooser));
    wb.complete_open(t, read_ok("(;B[aa])"));
    wb.set_language(Language::EnUs).unwrap();
    assert!(wb.clear_personal_data().unwrap());
    let wb2 = Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    assert!(wb2.config().recent_files.is_empty());
    assert_eq!(wb2.config().ui.language, Language::EnUs);
    assert_eq!(wb2.persisted().last_folder.as_deref(), Some("/k"));
    // The open document is untouched.
    assert_eq!(wb.document().record.node_count(), 2);
}

#[test]
fn reset_ui_position_deletes_window_state_until_next_start() {
    let dir = tempfile::tempdir().unwrap();
    let st = storage(dir.path());
    let mut wb = Workbench::new(st.clone(), None, Box::new(NoEngines), Vec::new());
    let placement = WindowPlacement {
        x: 10,
        y: 20,
        width: 800,
        height: 600,
        maximized: false,
    };
    wb.persist_ui(Some(placement), None, None).unwrap();
    assert_eq!(
        Workbench::new(st.clone(), None, Box::new(NoEngines), Vec::new())
            .persisted()
            .window,
        Some(placement)
    );
    let mut wb = Workbench::new(st.clone(), None, Box::new(NoEngines), Vec::new());
    wb.reset_ui_position().unwrap();
    assert!(!st.persist_file.exists());
    wb.persist_ui(Some(placement), None, None).unwrap();
    assert!(!st.persist_file.exists(), "exit after a reset does not write it back");
    assert_eq!(
        Workbench::new(st, None, Box::new(NoEngines), Vec::new())
            .persisted()
            .window,
        None
    );
}

#[test]
fn exit_persists_the_suggestions_frame_per_layout_and_the_list_columns() {
    let dir = tempfile::tempdir().unwrap();
    let st = storage(dir.path());
    let frame = |columns: Vec<u32>| FramePlacement {
        x: -9,
        y: 480,
        width: 650,
        height: 320,
        columns,
    };
    let mut wb = Workbench::new(st.clone(), None, Box::new(NoEngines), Vec::new());
    wb.persist_ui(
        None,
        Some(frame(vec![35, 20, 40, 70, 35, 50, 35])),
        Some(vec![10, 30, 30, 45, 30]),
    )
    .unwrap();
    let mut wb = Workbench::new(st.clone(), None, Box::new(NoEngines), Vec::new());
    assert_eq!(wb.persisted().main_list_columns, Some(vec![10, 30, 30, 45, 30]));
    // The 9-column layout is written beside the 7-column one; a hidden score column clears the
    // list widths like Java's empty `main-window-list`.
    wb.persist_ui(None, Some(frame(vec![35, 20, 40, 70, 35, 50, 35, 70, 35])), None)
        .unwrap();
    let persisted = Workbench::new(st, None, Box::new(NoEngines), Vec::new())
        .persisted()
        .clone();
    assert_eq!(
        persisted.suggestions_frame_7,
        Some(frame(vec![35, 20, 40, 70, 35, 50, 35]))
    );
    assert_eq!(
        persisted.suggestions_frame_9,
        Some(frame(vec![35, 20, 40, 70, 35, 50, 35, 70, 35]))
    );
    assert_eq!(persisted.main_list_columns, None);
}

#[test]
fn language_change_needs_restart_and_persists() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = Workbench::new(storage(dir.path()), Some("zh-CN"), Box::new(NoEngines), Vec::new());
    assert_eq!(wb.view(false).settings.active_language, Language::ZhCn);
    wb.set_language(Language::EnUs).unwrap();
    let v = wb.view(false);
    assert!(v.settings.restart_required);
    assert_eq!(v.settings.active_language, Language::ZhCn);
    let mut wb2 = Workbench::new(storage(dir.path()), Some("zh-CN"), Box::new(NoEngines), Vec::new());
    let v = wb2.view(false);
    assert_eq!(v.settings.active_language, Language::EnUs);
    assert!(!v.settings.restart_required);
    assert_eq!(Language::from_system_locale(Some("zh-Hant-TW")), Language::ZhTw);
    assert_eq!(Language::from_system_locale(Some("de-DE")), Language::ZhCn);
}

#[test]
fn position_view_reports_marks_comment_and_last_action() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    let t = wb.begin_open(file_origin("/x.sgf", OpenSource::Drop));
    wb.complete_open(t, read_ok("(;SZ[9];B[ee]C[hi]TR[aa];W[])"));
    assert!(wb.view(false).tree.is_some(), "a new document sends its tree");
    wb.navigate(Navigation::Forward(1));
    let v = wb.view(false);
    assert_eq!(v.position.comment, "hi");
    assert_eq!(v.position.marks[0].point, Point { x: 0, y: 0 });
    assert_eq!(
        v.position.last_action.as_ref().unwrap().point,
        Some(Point { x: 4, y: 4 })
    );
    assert!(v.tree.is_none(), "tree is not resent for navigation");
    wb.navigate(Navigation::Forward(1));
    let v = wb.view(false);
    assert_eq!(v.position.last_action.as_ref().unwrap().point, None);
    assert_eq!(v.position.move_number, 2);
    assert!(wb.goto_node(99).is_err());
}

#[cfg(unix)]
#[test]
fn a_settings_change_that_cannot_be_saved_is_not_applied() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let st = storage(dir.path());
    let config_dir = st.config_file.parent().unwrap().to_path_buf();
    let mut wb = Workbench::new(st, None, Box::new(NoEngines), Vec::new());
    let t = wb.begin_open(file_origin("/k/a.sgf", OpenSource::Chooser));
    wb.complete_open(t, read_ok("(;B[aa])"));
    assert_eq!(wb.config().recent_files.len(), 1);
    let set = |mode| std::fs::set_permissions(&config_dir, std::fs::Permissions::from_mode(mode)).unwrap();
    set(0o555);
    let cleared = wb.clear_personal_data();
    let komi = wb.set_read_komi(false);
    let lang = wb.set_language(Language::EnUs);
    set(0o755);
    assert!(cleared.is_err() && komi.is_err() && lang.is_err());
    // Nothing was applied: the projection still matches the file on disk.
    let v = wb.view(false);
    assert_eq!(v.recent_files, vec!["/k/a.sgf".to_string()]);
    assert!(v.settings.read_komi);
    assert_eq!(v.settings.language, Language::System);
    // A retry commits for real.
    assert!(wb.clear_personal_data().unwrap());
    let reopened = Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    assert!(reopened.config().recent_files.is_empty());
}

#[test]
fn settings_that_cannot_be_understood_are_kept_beside_the_file_that_replaces_them() {
    let dir = tempfile::tempdir().unwrap();
    let config_file = storage(dir.path()).config_file;
    let start = || Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    let mut wb = start();
    wb.set_read_komi(false).unwrap();
    drop(wb);
    // The user's settings as a broken hand edit or a damaged disk leaves them: half the file.
    let mut damaged = std::fs::read(&config_file).unwrap();
    damaged.truncate(damaged.len() / 2);
    std::fs::write(&config_file, &damaged).unwrap();

    // Nothing of it can be used, so the defaults are; the next change is saved as usual.
    let mut wb = start();
    assert!(wb.view(false).settings.read_komi);
    wb.set_language(Language::EnUs).unwrap();
    drop(wb);
    assert_eq!(start().view(false).settings.language, Language::EnUs);
    // What the user had is still there to repair (Java: `config.txt.unreadable-backup`).
    let kept = |name: &str| std::fs::read(config_file.with_file_name(name)).ok();
    assert!(kept("config.json.unreadable-backup").as_deref() == Some(&damaged[..]), "the damaged file is kept");

    // A later damaged file is kept under another name, not over the first one.
    std::fs::write(&config_file, b"{ \"ui\": ").unwrap();
    let mut wb = start();
    wb.set_read_komi(false).unwrap();
    drop(wb);
    assert!(kept("config.json.unreadable-backup").as_deref() == Some(&damaged[..]), "the first kept file stays");
    assert_eq!(kept("config.json.unreadable-backup.1").as_deref(), Some(&b"{ \"ui\": "[..]));
}

#[cfg(unix)]
#[test]
fn settings_that_cannot_be_read_at_start_are_not_replaced() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let config_file = storage(dir.path()).config_file;
    let start = || Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    let mut wb = start();
    wb.set_read_komi(false).unwrap();
    drop(wb);
    let saved = std::fs::read(&config_file).unwrap();
    let set = |mode| std::fs::set_permissions(&config_file, std::fs::Permissions::from_mode(mode)).unwrap();

    // The file is there but cannot be read while the program starts; its folder stays writable.
    set(0o000);
    let mut wb = permissions::obeying_permissions(&start);
    set(0o600);
    assert!(wb.view(false).settings.read_komi, "the defaults are in use");
    assert!(!wb.view(false).java_import.offer, "the settings exist: this is no first run");
    // A change is refused rather than written over the settings the user has.
    assert!(wb.set_language(Language::EnUs).is_err());
    assert_eq!(wb.view(false).settings.language, Language::System);
    drop(wb);
    assert_eq!(std::fs::read(&config_file).unwrap(), saved);
    // The next start has them again.
    assert!(!start().view(false).settings.read_komi);
}

#[cfg(unix)]
#[test]
fn settings_linked_from_a_folder_that_is_away_at_start_are_not_a_first_run() {
    let dir = tempfile::tempdir().unwrap();
    let config_file = storage(dir.path()).config_file;
    let start = || Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    let mut wb = start();
    wb.set_read_komi(false).unwrap();
    drop(wb);
    // The user keeps the settings in another folder and links them here.
    let elsewhere = dir.path().join("synced.json");
    std::fs::rename(&config_file, &elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &config_file).unwrap();

    // That folder is away while the program starts (a drive not mounted yet) and back afterwards.
    let away = dir.path().join("away.json");
    std::fs::rename(&elsewhere, &away).unwrap();
    let mut wb = start();
    std::fs::rename(&away, &elsewhere).unwrap();
    assert!(!wb.view(false).java_import.offer, "the settings exist: this is no first run");
    assert!(wb.set_language(Language::EnUs).is_err());
    drop(wb);
    assert!(config_file.symlink_metadata().unwrap().file_type().is_symlink(), "the link is kept");
    // The next start follows it to the user's settings.
    assert!(!start().view(false).settings.read_komi);
}

#[cfg(unix)]
#[test]
fn damaged_settings_that_cannot_be_kept_are_not_replaced() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let config_file = storage(dir.path()).config_file;
    let config_dir = config_file.parent().unwrap().to_path_buf();
    let start = || Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    let damaged = b"{ \"ui\": ";
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(&config_file, damaged).unwrap();
    let set = |mode| std::fs::set_permissions(&config_dir, std::fs::Permissions::from_mode(mode)).unwrap();
    let kept = || std::fs::read(config_file.with_file_name("config.json.unreadable-backup")).ok();

    // The folder takes no new file while the program starts, so there is nowhere to keep them.
    set(0o555);
    let mut wb = permissions::obeying_permissions(&start);
    set(0o755);
    assert!(!wb.view(false).java_import.offer, "the settings exist: this is no first run");
    assert!(wb.set_language(Language::EnUs).is_err());
    drop(wb);
    assert_eq!(std::fs::read(&config_file).unwrap(), damaged);
    assert_eq!(kept(), None);
    // A start that can keep them does, and saves again.
    let mut wb = start();
    wb.set_language(Language::EnUs).unwrap();
    assert_eq!(kept().as_deref(), Some(&damaged[..]));
}

#[cfg(unix)]
fn mode(path: &std::path::Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[cfg(unix)]
#[test]
fn settings_are_saved_for_their_owner_only_and_keep_permissions_the_user_set() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let config_file = storage(dir.path()).config_file;
    let mut wb = Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    wb.set_read_komi(false).unwrap();
    // A new settings file may hold a link or command with a credential: no one else reads it
    // (Java writes `config.txt` through `Files.createTempFile`, which is owner-only too).
    assert_eq!(mode(&config_file), 0o600);
    // What the user chose for the file afterwards survives the next save.
    std::fs::set_permissions(&config_file, std::fs::Permissions::from_mode(0o640)).unwrap();
    wb.set_language(Language::EnUs).unwrap();
    assert_eq!(mode(&config_file), 0o640);
}

#[cfg(unix)]
#[test]
fn damaged_settings_are_kept_for_their_owner_only() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let config_file = storage(dir.path()).config_file;
    std::fs::create_dir_all(config_file.parent().unwrap()).unwrap();
    std::fs::write(&config_file, b"{ \"ui\": ").unwrap();
    std::fs::set_permissions(&config_file, std::fs::Permissions::from_mode(0o600)).unwrap();

    drop(Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new()));
    // Neither the kept copy nor the rebuilt file is opened to anyone the damaged file was not.
    assert_eq!(mode(&config_file.with_file_name("config.json.unreadable-backup")), 0o600);
    assert_eq!(mode(&config_file), 0o600);
}

#[test]
fn inspecting_the_view_does_not_consume_what_the_ui_must_receive() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    wb.view(false);
    let t = wb.begin_open(file_origin("/bad.sgf", OpenSource::Chooser));
    wb.complete_open(t, Err(ReadError::Invalid("x".into())));
    wb.begin_open(file_origin("/a.sgf", OpenSource::Chooser));
    let inspected = wb.inspect();
    assert!(inspected.tree.is_some());
    let published = wb.view(false);
    assert!(matches!(published.open_outcome, Some(OpenOutcome::Failed { .. })));
    let t2 = wb.begin_open(file_origin("/b.sgf", OpenSource::Chooser));
    wb.complete_open(t2, read_ok("(;SZ[9];B[ee])"));
    wb.inspect();
    assert!(
        wb.view(false).tree.is_some(),
        "the new document's tree still reaches the UI"
    );
}

#[test]
fn forward_steps_sound_like_java_and_respect_the_setting() {
    // Utils.playVoiceFile per node entered forward: capture of 3+ -> deadStoneMore, 1-2 ->
    // deadStone, else Stone; backward steps are silent.
    let dir = tempfile::tempdir().unwrap();
    let mut wb = Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new());
    // Move 4, W[ab], captures B[aa].
    let t = wb.begin_open(file_origin("/s.sgf", OpenSource::Chooser));
    wb.complete_open(t, read_ok("(;SZ[9];B[aa];W[ba];B[ee];W[ab];B[ff])"));
    assert!(wb.view(false).sounds.is_empty(), "opening is silent");
    wb.navigate(Navigation::Forward(1));
    assert_eq!(wb.view(false).sounds, vec![StoneSound::Stone]);
    wb.navigate(Navigation::Back(1));
    assert!(wb.view(false).sounds.is_empty(), "backward steps are silent");
    wb.navigate(Navigation::Forward(4));
    assert_eq!(
        wb.view(false).sounds,
        vec![
            StoneSound::Stone,
            StoneSound::Stone,
            StoneSound::Stone,
            StoneSound::DeadStone
        ],
        "one sound per node entered"
    );
    assert!(wb.view(false).sounds.is_empty(), "sounds are handed over once");
    assert!(wb.set_play_sound(false).unwrap());
    wb.navigate(Navigation::First);
    wb.navigate(Navigation::Forward(1));
    assert!(wb.view(false).sounds.is_empty());
    assert!(
        !Workbench::new(storage(dir.path()), None, Box::new(NoEngines), Vec::new())
            .view(false)
            .settings
            .play_sound
    );
}

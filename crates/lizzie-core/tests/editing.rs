//! Document editing through the public `Workbench::edit` operation, as menus, keys, board input
//! and automation send it: playing and following moves, rule refusals, deletes with confirmation
//! and their undo, insert / stone edits replaying what follows, branch order, starting-position
//! setup, transforms, marks, 试下, and the document revision seen by saving.

use lizzie_core::api::{CommandResult, EditCommand, EditTarget};
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::formats::{self, sgf, sgf_write};
use lizzie_core::record::{
    GameRecord, MarkKind, MarkTool, NodeId, NodeKind, Point, RecordFormat, Stone, StoneChange, Transform, ROOT,
};
use lizzie_core::settings::Storage;
use lizzie_core::workbench::{OpenOrigin, Workbench};

struct NoEngines;

impl Launcher for NoEngines {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        Err(std::io::Error::other("no engines in editing tests"))
    }
}

fn opened(dir: &std::path::Path, text: &str) -> Workbench {
    let storage = Storage::in_dirs(&dir.join("config"), &dir.join("data"));
    let mut wb = Workbench::new(storage, None, Box::new(NoEngines), Vec::new());
    open(&mut wb, text);
    wb
}

/// Opens `text` the way the platform adapter reads a file: with the `no-capture` setting.
fn open(wb: &mut Workbench, text: &str) {
    let read = formats::parse(RecordFormat::Sgf, text, wb.config().edit.no_capture);
    let t = wb.begin_open(OpenOrigin::File {
        path: "/x.sgf".into(),
        source: lizzie_core::api::OpenSource::Drop,
    });
    assert!(wb.complete_open(t, read.map(|r| (RecordFormat::Sgf, r))));
}

/// SGF coordinates (`"dd"`).
fn p(s: &str) -> Point {
    let b = s.as_bytes();
    Point {
        x: u16::from(b[0] - b'a'),
        y: u16::from(b[1] - b'a'),
    }
}

fn edit(wb: &mut Workbench, op: EditCommand) -> CommandResult {
    wb.edit(op).expect("edit accepted")
}

/// Sends an edit that has to ask first; returns what the question is about.
fn asked(wb: &mut Workbench, op: EditCommand) -> EditTarget {
    match edit(wb, op) {
        CommandResult::NeedsEditConfirmation { target } => target,
        other => panic!("expected a question, got {other:?}"),
    }
}

fn play(wb: &mut Workbench, at: &str) -> CommandResult {
    edit(
        wb,
        EditCommand::Play {
            point: p(at),
            color: None,
        },
    )
}

fn rec(wb: &Workbench) -> &GameRecord {
    &wb.document().record
}

fn revision(wb: &Workbench) -> u64 {
    wb.document().revision
}

fn sgf_point(point: Point) -> String {
    [point.x, point.y].iter().map(|&c| char::from(b'a' + c as u8)).collect()
}

/// The main line from `from` as SGF-like moves (`"B[dd]"`, `"W[]"`, `"DD"`).
fn line(rec: &GameRecord, from: NodeId) -> Vec<String> {
    let mut out = Vec::new();
    let mut n = from;
    while let Some(c) = rec.main_child(n) {
        let side = |color| if color == Stone::Black { "B" } else { "W" };
        out.push(match rec.node(c).kind {
            NodeKind::Move { color, point } => format!("{}[{}]", side(color), sgf_point(point)),
            NodeKind::Pass { color } => format!("{}[]", side(color)),
            NodeKind::DummyPass => "DD".into(),
            NodeKind::Snapshot => "SNAP".into(),
        });
        n = c;
    }
    out
}

fn reopen(rec: &GameRecord) -> GameRecord {
    sgf::read(&sgf_write::write(rec, "LizzieYzy Tauri:test")).expect("saved SGF reads back")
}

#[test]
fn a_click_follows_the_same_move_and_otherwise_adds_the_next_variation() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), "(;SZ[9])");
    assert_eq!(play(&mut wb, "ee"), CommandResult::Done);
    assert_eq!(play(&mut wb, "dc"), CommandResult::Done);
    assert_eq!(revision(&wb), 2);
    wb.navigate(Navigation::First);
    // The same point again follows the existing move; nothing is added.
    play(&mut wb, "ee");
    assert_eq!(revision(&wb), 2);
    assert_eq!(rec(&wb).node(ROOT).children.len(), 1);
    wb.navigate(Navigation::First);
    play(&mut wb, "cc");
    let root = rec(&wb).node(ROOT);
    assert_eq!(root.children.len(), 2, "a new last variation");
    assert_eq!(wb.cursor(), root.children[1]);
    assert_eq!(line(rec(&wb), ROOT), ["B[ee]", "W[dc]"], "the main line is untouched");
    // An explicit colour (添加黑子) plays that colour whatever the side to play.
    edit(
        &mut wb,
        EditCommand::Play {
            point: p("gg"),
            color: Some(Stone::Black),
        },
    );
    assert_eq!(
        rec(&wb).node(wb.cursor()).kind,
        NodeKind::Move {
            color: Stone::Black,
            point: p("gg")
        }
    );
}

#[test]
fn illegal_moves_leave_the_record_and_position_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    // White bb has just captured the black stone at cb; Black retaking at once is ko.
    let mut wb = opened(
        dir.path(),
        "(;SZ[9]AW[ha][ib];B[ba];W[ca];B[ab];W[db];B[bc];W[cc];B[cb];W[bb])",
    );
    wb.navigate(Navigation::Last);
    let (before, at) = (revision(&wb), wb.cursor());
    for point in ["cb", "bb", "ha"] {
        assert_eq!(
            play(&mut wb, point),
            CommandResult::Unchanged,
            "ko / occupied / setup stone at {point}"
        );
    }
    // A single black stone in the corner between the white setup stones has no liberty.
    assert_eq!(
        edit(
            &mut wb,
            EditCommand::Play {
                point: p("ia"),
                color: Some(Stone::Black)
            }
        ),
        CommandResult::Unchanged,
        "suicide"
    );
    assert_eq!((wb.cursor(), revision(&wb)), (at, before));
    // After a move elsewhere on both sides the recapture is legal.
    play(&mut wb, "gg");
    play(&mut wb, "hh");
    assert_eq!(play(&mut wb, "cb"), CommandResult::Done);
}

#[test]
fn delete_asks_while_moves_follow_and_undo_brings_them_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), "(;SZ[9];B[ee]C[one];W[dd]C[two](;B[cc])(;B[gg]))");
    wb.navigate(Navigation::Forward(1));
    let first = wb.cursor();
    let before = revision(&wb);
    let target = asked(&mut wb, EditCommand::DeleteMove { confirmed: None });
    assert_eq!(
        (revision(&wb), rec(&wb).nodes().count()),
        (before, 5),
        "asking changes nothing"
    );
    assert_eq!(
        edit(
            &mut wb,
            EditCommand::DeleteMove {
                confirmed: Some(target)
            }
        ),
        CommandResult::Done
    );
    assert_eq!(wb.cursor(), ROOT);
    assert!(!rec(&wb).contains(first));
    assert!(wb.view(false).editing.can_undo);

    assert_eq!(edit(&mut wb, EditCommand::UndoEdit), CommandResult::Done);
    assert_eq!(line(rec(&wb), ROOT), ["B[ee]", "W[dd]", "B[cc]"]);
    assert_eq!(rec(&wb).node(wb.cursor()).comment, "one", "back at the deleted move");
    let v = wb.view(false);
    assert!(v.editing.can_redo && !v.editing.can_undo);
    assert!(
        revision(&wb) > before + 1,
        "restoring is a new revision, not the saved one"
    );

    assert_eq!(edit(&mut wb, EditCommand::RedoEdit), CommandResult::Done);
    assert!(rec(&wb).node(ROOT).children.is_empty());
    // A leaf goes without asking.
    play(&mut wb, "ee");
    assert_eq!(
        edit(&mut wb, EditCommand::DeleteMove { confirmed: None }),
        CommandResult::Done
    );
    assert_eq!(rec(&wb).nodes().count(), 1);
}

#[test]
fn deleting_a_branch_removes_only_that_branch() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), "(;SZ[9];B[ee];W[dd](;B[cc];W[cd])(;B[gg];W[gh]))");
    let dd = rec(&wb).main_child(rec(&wb).main_child(ROOT).unwrap()).unwrap();
    let gg = rec(&wb).node(dd).children[1];
    let gh = rec(&wb).main_child(gg).unwrap();
    wb.goto_node(gh).unwrap();
    let target = asked(&mut wb, EditCommand::DeleteBranch { confirmed: None });
    edit(
        &mut wb,
        EditCommand::DeleteBranch {
            confirmed: Some(target),
        },
    );
    assert_eq!(rec(&wb).node(dd).children.len(), 1);
    assert_eq!(line(rec(&wb), ROOT), ["B[ee]", "W[dd]", "B[cc]", "W[cd]"]);
    assert_eq!(wb.cursor(), dd);
}

#[test]
fn branch_order_moves_and_set_as_main_promotes_the_line() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), "(;SZ[9](;B[aa])(;B[bb];W[cc](;B[dd])(;B[ee]))(;B[ff]))");
    let root_children = rec(&wb).node(ROOT).children.clone();
    let bb = root_children[1];
    let cc = rec(&wb).main_child(bb).unwrap();
    let ee = rec(&wb).node(cc).children[1];
    wb.goto_node(bb).unwrap();
    edit(&mut wb, EditCommand::MoveBranch { up: false });
    assert_eq!(rec(&wb).node(ROOT).children, [root_children[0], root_children[2], bb]);
    edit(&mut wb, EditCommand::MoveBranch { up: true });
    assert_eq!(rec(&wb).node(ROOT).children, root_children);

    wb.goto_node(ee).unwrap();
    assert_eq!(edit(&mut wb, EditCommand::SetAsMain), CommandResult::Done);
    assert_eq!(line(rec(&wb), ROOT), ["B[bb]", "W[cc]", "B[ee]"]);
    assert_eq!(wb.cursor(), ee, "browsing stays on the promoted move");
    assert_eq!(edit(&mut wb, EditCommand::SetAsMain), CommandResult::Unchanged);
}

#[test]
fn insert_and_stone_edits_replay_what_follows_and_keep_node_content() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(
        dir.path(),
        "(;SZ[9];B[ee]C[first];W[dd]C[second]TR[ee]LZ[KataGo 55.0 100\nmove cc visits 10];B[cc]C[third])",
    );
    wb.navigate(Navigation::Forward(1));
    edit(
        &mut wb,
        EditCommand::Insert {
            point: p("gg"),
            color: Stone::White,
        },
    );
    assert_eq!(line(rec(&wb), ROOT), ["B[ee]", "W[gg]", "W[dd]", "B[cc]"]);
    let gg = rec(&wb).main_child(rec(&wb).main_child(ROOT).unwrap()).unwrap();
    let node = rec(&wb).node(rec(&wb).main_child(gg).unwrap());
    assert_eq!(node.comment, "second");
    assert_eq!(node.marks.len(), 1, "marks travel with the replayed move");
    assert!(
        node.properties.iter().all(|p| !p.id.starts_with("LZ")),
        "analysis of the old position is not kept"
    );
    assert_eq!(node.move_number, 3);
    // Two white moves in a row: the inserted move's side to play is the colour that follows it
    // (alignParentTurnStateWithFirstActionChild), in the record and after saving and reopening.
    assert_eq!(rec(&wb).node(gg).to_play, Stone::White);
    let saved = reopen(rec(&wb));
    let saved_gg = saved.main_child(saved.main_child(ROOT).unwrap()).unwrap();
    assert_eq!(saved.node(saved_gg).to_play, Stone::White);
    edit(&mut wb, EditCommand::UndoEdit);
    assert_eq!(line(rec(&wb), ROOT), ["B[ee]", "W[dd]", "B[cc]"]);

    // 反色 of the first stone, seen from the last move.
    wb.navigate(Navigation::Last);
    edit(
        &mut wb,
        EditCommand::EditStone {
            point: p("ee"),
            change: StoneChange::Switch,
        },
    );
    assert_eq!(line(rec(&wb), ROOT), ["W[ee]", "W[dd]", "B[cc]"]);
    assert_eq!(rec(&wb).node(rec(&wb).main_child(ROOT).unwrap()).comment, "first");
    // Moving a stone to an empty point, then deleting it.
    wb.navigate(Navigation::Last);
    edit(
        &mut wb,
        EditCommand::EditStone {
            point: p("dd"),
            change: StoneChange::MoveTo { point: p("fd") },
        },
    );
    assert_eq!(line(rec(&wb), ROOT), ["W[ee]", "W[fd]", "B[cc]"]);
    wb.navigate(Navigation::Last);
    edit(
        &mut wb,
        EditCommand::EditStone {
            point: p("fd"),
            change: StoneChange::Delete,
        },
    );
    assert_eq!(line(rec(&wb), ROOT), ["W[ee]", "B[cc]"]);
    let second = rec(&wb).main_child(rec(&wb).main_child(ROOT).unwrap()).unwrap();
    assert_eq!(rec(&wb).node(second).comment, "third");
}

#[test]
fn pass_and_change_turn_without_engine() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), "(;SZ[9];B[ee])");
    wb.navigate(Navigation::Last);
    assert_eq!(edit(&mut wb, EditCommand::Pass), CommandResult::Done);
    assert_eq!(line(rec(&wb), ROOT), ["B[ee]", "W[]"]);
    // Without an engine that takes either colour, 交换落子顺序 is a pass (changeNextTurn).
    edit(&mut wb, EditCommand::ChangeTurn);
    assert_eq!(line(rec(&wb), ROOT), ["B[ee]", "W[]", "B[]"]);
    assert_eq!(line(&reopen(rec(&wb)), ROOT), ["B[ee]", "W[]", "B[]"]);
}

#[test]
fn setup_mode_places_static_stones_and_the_side_to_play_on_the_root() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), "(;SZ[9])");
    assert_eq!(
        edit(
            &mut wb,
            EditCommand::SetSetupMode {
                enabled: true,
                confirmed: None
            }
        ),
        CommandResult::Done
    );
    assert!(wb.view(false).editing.setup_mode);
    edit(
        &mut wb,
        EditCommand::SetupStone {
            point: p("cc"),
            stone: Some(Stone::Black),
        },
    );
    edit(
        &mut wb,
        EditCommand::SetupStone {
            point: p("gg"),
            stone: Some(Stone::White),
        },
    );
    edit(
        &mut wb,
        EditCommand::SetupStone {
            point: p("ee"),
            stone: Some(Stone::Black),
        },
    );
    edit(
        &mut wb,
        EditCommand::SetupStone {
            point: p("ee"),
            stone: None,
        },
    );
    edit(&mut wb, EditCommand::SetupSideToPlay { color: Stone::White });
    assert_eq!(play(&mut wb, "dd"), CommandResult::Unchanged, "no moves in setup mode");
    assert_eq!(edit(&mut wb, EditCommand::Pass), CommandResult::Unchanged);
    let root = rec(&wb).node(ROOT);
    assert!(root.children.is_empty());
    assert_eq!((root.move_number, root.to_play), (0, Stone::White));
    assert_eq!(root.setup.add_black, [p("cc")]);
    assert_eq!(root.setup.add_white, [p("gg")]);

    edit(
        &mut wb,
        EditCommand::SetSetupMode {
            enabled: false,
            confirmed: None,
        },
    );
    play(&mut wb, "dd");
    let first = rec(&wb).node(wb.cursor());
    assert_eq!(
        first.kind,
        NodeKind::Move {
            color: Stone::White,
            point: p("dd")
        }
    );
    assert_eq!(first.move_number, 1, "setup stones take no move number");

    let saved = reopen(rec(&wb));
    assert_eq!(saved.node(ROOT).setup.add_black, [p("cc")]);
    assert_eq!(saved.node(ROOT).to_play, Stone::White);
    assert_eq!(line(&saved, ROOT), ["W[dd]"]);

    // With moves, setup edits are refused and entering setup mode asks to convert.
    assert!(matches!(
        edit(&mut wb, EditCommand::SetupClearAll),
        CommandResult::Refused { message_key } if message_key == "LizzieFrame.setupEditRequiresRootOnly"
    ));
    let target = asked(
        &mut wb,
        EditCommand::SetSetupMode {
            enabled: true,
            confirmed: None,
        },
    );
    assert!(!wb.view(false).editing.setup_mode);
    edit(
        &mut wb,
        EditCommand::SetSetupMode {
            enabled: true,
            confirmed: Some(target),
        },
    );
    assert!(wb.view(false).editing.setup_mode);
    let root = rec(&wb).node(ROOT);
    assert!(root.children.is_empty(), "the moves became the starting position");
    assert_eq!(root.to_play, Stone::Black, "the side to play after White's move");
    let mut white = root.setup.add_white.clone();
    white.sort_by_key(|q| (q.y, q.x));
    assert_eq!(white, [p("dd"), p("gg")]);
}

#[test]
fn converting_the_current_position_drops_the_moves_after_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), "(;SZ[9];B[ee];W[dd];B[cc])");
    wb.navigate(Navigation::Forward(2));
    let target = asked(&mut wb, EditCommand::ConvertToStartingPosition { confirmed: None });
    assert_eq!(rec(&wb).nodes().count(), 4);
    edit(
        &mut wb,
        EditCommand::ConvertToStartingPosition {
            confirmed: Some(target),
        },
    );
    let root = rec(&wb).node(ROOT);
    assert!(root.children.is_empty());
    assert_eq!(
        (root.setup.add_black.as_slice(), root.setup.add_white.as_slice()),
        (&[p("ee")][..], &[p("dd")][..])
    );
    assert_eq!(root.to_play, Stone::Black);
    assert_eq!(wb.cursor(), ROOT);
}

#[test]
fn transforms_map_every_stone_and_rotation_needs_a_square_board() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), "(;SZ[9]AB[ab];B[ca];W[ee])");
    edit(
        &mut wb,
        EditCommand::Transform {
            transform: Transform::RotateRight,
        },
    );
    // (x, y) → (w-1-y, x)
    assert_eq!(line(rec(&wb), ROOT), ["B[ic]", "W[ee]"]);
    assert_eq!(rec(&wb).node(ROOT).setup.add_black, [p("ha")]);
    edit(
        &mut wb,
        EditCommand::Transform {
            transform: Transform::ExchangeColors,
        },
    );
    assert_eq!(line(rec(&wb), ROOT), ["W[ic]", "B[ee]"]);
    assert_eq!(rec(&wb).node(ROOT).setup.add_white, [p("ha")]);

    let mut wide = opened(dir.path(), "(;SZ[9:5];B[aa])");
    assert!(matches!(
        edit(&mut wide, EditCommand::Transform { transform: Transform::RotateLeft }),
        CommandResult::Refused { message_key } if message_key == "SpinAndMirror.noneSquareError"
    ));
    edit(
        &mut wide,
        EditCommand::Transform {
            transform: Transform::MirrorVertical,
        },
    );
    assert_eq!(line(rec(&wide), ROOT), ["B[ia]"]);
}

#[test]
fn markup_tools_toggle_marks_and_labels_count_on() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), "(;SZ[9];B[ee])");
    wb.navigate(Navigation::Last);
    let mark = |wb: &mut Workbench, at: &str, tool| edit(wb, EditCommand::ToggleMark { point: p(at), tool });
    mark(&mut wb, "aa", MarkTool::Letter);
    mark(&mut wb, "bb", MarkTool::Letter);
    mark(&mut wb, "cc", MarkTool::Number);
    mark(&mut wb, "dd", MarkTool::Number);
    mark(&mut wb, "ee", MarkTool::Triangle);
    mark(&mut wb, "ff", MarkTool::Circle);
    mark(&mut wb, "ff", MarkTool::Circle);
    let node = wb.cursor();
    let at = |wb: &Workbench, q: &str| {
        rec(wb)
            .node(node)
            .marks
            .iter()
            .find(|m| m.point == p(q))
            .map(|m| m.mark.clone())
    };
    assert_eq!(at(&wb, "aa"), Some(MarkKind::Label("A".into())));
    assert_eq!(at(&wb, "bb"), Some(MarkKind::Label("B".into())));
    assert_eq!(at(&wb, "cc"), Some(MarkKind::Label("1".into())));
    assert_eq!(at(&wb, "dd"), Some(MarkKind::Label("2".into())));
    assert_eq!(at(&wb, "ee"), Some(MarkKind::Triangle));
    assert_eq!(at(&wb, "ff"), None, "the same tool again removes the mark");
    mark(&mut wb, "aa", MarkTool::Eraser);
    assert_eq!(at(&wb, "aa"), None);
    let saved = reopen(rec(&wb));
    assert_eq!(saved.node(saved.main_child(ROOT).unwrap()).marks.len(), 4);
    assert_eq!(edit(&mut wb, EditCommand::ClearMarks), CommandResult::Done);
    assert!(rec(&wb).node(node).marks.is_empty());
}

#[test]
fn comments_are_content_of_the_node_they_were_opened_on() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), "(;SZ[9];B[ee];W[dd])");
    let ee = rec(&wb).main_child(ROOT).unwrap();
    wb.navigate(Navigation::Last);
    edit(
        &mut wb,
        EditCommand::SetComment {
            node: ee,
            comment: "好手".into(),
        },
    );
    assert_eq!(rec(&wb).node(ee).comment, "好手");
    assert_ne!(wb.cursor(), ee, "committing does not move browsing");
    assert!(wb.view(false).document.unsaved);
    assert_eq!(reopen(rec(&wb)).node(ee).comment, "好手");
    assert!(wb
        .edit(EditCommand::SetComment {
            node: 999,
            comment: "x".into()
        })
        .is_err());
}

#[test]
fn edits_during_a_save_stay_unsaved() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), "(;SZ[9])");
    let path = dir.path().join("a.sgf");
    play(&mut wb, "ee");
    let job = wb.begin_save(path.to_str().unwrap(), "LizzieYzy Tauri:test");
    play(&mut wb, "dd");
    edit(
        &mut wb,
        EditCommand::ToggleMark {
            point: p("ee"),
            tool: MarkTool::Square,
        },
    );
    formats::write_record_file(&job.path, &job.text).unwrap();
    wb.complete_save(job.ticket, Ok(()));
    let v = wb.view(false);
    assert!(v.document.unsaved, "the later edits are not in the file");
    assert_eq!(v.document.saved_revision, Some(1));
    assert_eq!(line(&formats::read_file(&path, false).unwrap().1, ROOT), ["B[ee]"]);
}

#[test]
fn try_play_keeps_the_record_and_brings_it_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), "(;SZ[9];B[ee];W[dd];B[cc])");
    wb.navigate(Navigation::Forward(1));
    let at = wb.cursor();
    edit(&mut wb, EditCommand::TryPlay);
    assert!(wb.view(false).editing.trying);
    assert_eq!(line(rec(&wb), ROOT), ["B[ee]"], "what followed is set aside");
    play(&mut wb, "gg");
    play(&mut wb, "hh");
    edit(&mut wb, EditCommand::TryPlay);
    assert!(!wb.view(false).editing.trying);
    assert_eq!(line(rec(&wb), ROOT), ["B[ee]", "W[dd]", "B[cc]"]);
    assert_eq!(wb.cursor(), at);
    // A new board ends 试下 without restoring.
    edit(&mut wb, EditCommand::TryPlay);
    edit(&mut wb, EditCommand::ClearBoard);
    assert!(!wb.view(false).editing.trying);
    assert_eq!(rec(&wb).nodes().count(), 1);
}

#[test]
fn a_variation_added_as_branch_stays_beside_the_main_line() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), "(;SZ[9];B[ee])");
    wb.navigate(Navigation::Last);
    let at = wb.cursor();
    edit(
        &mut wb,
        EditCommand::AddVariationAsBranch {
            moves: vec![p("dd"), p("cc")],
        },
    );
    assert_eq!(wb.cursor(), at, "browsing stays");
    let children = rec(&wb).node(at).children.clone();
    assert_eq!(children.len(), 2);
    assert_eq!(
        rec(&wb).node(children[0]).kind,
        NodeKind::DummyPass,
        "a placeholder keeps the main line"
    );
    assert_eq!(line(rec(&wb), children[1]), ["B[cc]"]);
    // Playing on replaces the placeholder.
    play(&mut wb, "gg");
    assert_eq!(line(rec(&wb), ROOT), ["B[ee]", "W[gg]"]);
    assert_eq!(rec(&wb).node(at).children.len(), 2);
}

#[test]
fn a_new_board_asks_only_while_the_hint_is_on() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), "(;SZ[9];B[ee])");
    asked(&mut wb, EditCommand::NewBoard { confirmed: None });
    let mut settings = wb.config().edit.clone();
    settings.show_new_board_hint = false;
    assert!(wb.set_edit_settings(settings).unwrap());
    assert_eq!(
        edit(&mut wb, EditCommand::NewBoard { confirmed: None }),
        CommandResult::Done
    );
    assert_eq!(rec(&wb).nodes().count(), 1);

    // A non-standard size is remembered as the 其他 size.
    edit(&mut wb, EditCommand::SetBoardSize { width: 7, height: 11 });
    assert_eq!(rec(&wb).size.width, 7);
    assert_eq!(
        (wb.config().edit.other_size_width, wb.config().edit.other_size_height),
        (7, 11)
    );
}

#[test]
fn a_confirmation_applies_only_to_the_document_revision_and_node_it_was_asked_for() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), "(;SZ[9];B[ee];W[dd])");
    wb.navigate(Navigation::Forward(1));
    let target = asked(&mut wb, EditCommand::DeleteMove { confirmed: None });
    // Another record was opened (a dropped file) while the dialog was open.
    open(&mut wb, "(;SZ[9];B[aa];W[bb];B[cc])");
    wb.navigate(Navigation::Forward(1));
    let stale = EditCommand::DeleteMove {
        confirmed: Some(target),
    };
    assert_eq!(edit(&mut wb, stale), CommandResult::Unchanged);
    assert_eq!(rec(&wb).nodes().count(), 4, "the other record keeps its moves");
    // The same record, edited meanwhile, or another current node: also stale.
    let target = asked(&mut wb, EditCommand::DeleteMove { confirmed: None });
    edit(
        &mut wb,
        EditCommand::ToggleMark {
            point: p("ee"),
            tool: MarkTool::Circle,
        },
    );
    let after_edit = EditCommand::DeleteMove {
        confirmed: Some(target),
    };
    assert_eq!(edit(&mut wb, after_edit), CommandResult::Unchanged);
    let target = asked(&mut wb, EditCommand::NewBoard { confirmed: None });
    wb.navigate(Navigation::Forward(1));
    assert_eq!(
        edit(
            &mut wb,
            EditCommand::NewBoard {
                confirmed: Some(target)
            }
        ),
        CommandResult::Unchanged
    );
    assert_eq!(rec(&wb).nodes().count(), 4);
    // Setup mode asked on a record with moves; an empty record replaced it meanwhile.
    let target = asked(
        &mut wb,
        EditCommand::SetSetupMode {
            enabled: true,
            confirmed: None,
        },
    );
    open(&mut wb, "(;SZ[9])");
    let stale = EditCommand::SetSetupMode {
        enabled: true,
        confirmed: Some(target),
    };
    assert_eq!(edit(&mut wb, stale), CommandResult::Unchanged);
    assert!(!wb.view(false).editing.setup_mode);
}

#[test]
fn undo_and_try_play_bring_the_kept_komi_back_everywhere() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), "(;SZ[9]KM[6.5];B[ee];W[dd])");
    wb.navigate(Navigation::Forward(1));
    edit(&mut wb, EditCommand::DeleteMoveNoHint);
    wb.set_komi(7.5).unwrap();
    edit(&mut wb, EditCommand::UndoEdit);
    let v = wb.view(false);
    assert_eq!((v.document.info.komi, v.komi), (6.5, 6.5));

    edit(&mut wb, EditCommand::TryPlay);
    wb.set_komi(0.5).unwrap();
    edit(&mut wb, EditCommand::TryPlay);
    let v = wb.view(false);
    assert_eq!((v.document.info.komi, v.komi), (6.5, 6.5));
}

#[test]
fn try_play_without_changes_leaves_the_document_saved() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), "(;SZ[9];B[ee])");
    wb.navigate(Navigation::Last);
    let at = wb.cursor();
    edit(&mut wb, EditCommand::TryPlay);
    edit(&mut wb, EditCommand::TryPlay);
    let v = wb.view(false);
    assert!(!v.document.unsaved);
    assert_eq!((v.document.revision, wb.cursor()), (0, at));
    // A changed turn during 试下 is not saved content but still goes away with it.
    edit(&mut wb, EditCommand::TryPlay);
    edit(&mut wb, EditCommand::ChangeTurn);
    edit(&mut wb, EditCommand::TryPlay);
    assert_eq!(line(rec(&wb), ROOT), ["B[ee]"]);
}

#[test]
fn changing_a_position_in_place_drops_its_saved_analysis_only() {
    let lz = "LZ[KataGo 55.0 100\nmove ee visits 10]";
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), &format!("(;SZ[9]{lz})"));
    edit(
        &mut wb,
        EditCommand::SetSetupMode {
            enabled: true,
            confirmed: None,
        },
    );
    // 黑先 where Black already plays changes nothing (`setupSetSideToPlay`).
    let same = EditCommand::SetupSideToPlay { color: Stone::Black };
    assert_eq!(edit(&mut wb, same), CommandResult::Unchanged);
    assert!(sgf_write::write(rec(&wb), "t").contains("LZ["));
    edit(
        &mut wb,
        EditCommand::SetupStone {
            point: p("cc"),
            stone: Some(Stone::Black),
        },
    );
    let text = sgf_write::write(rec(&wb), "t");
    assert!(!text.contains("LZ["), "{text}");

    // A rotation keeps the analysis of positions it does not change (the empty root, a stone on
    // the centre) and drops it elsewhere.
    let mut wb = opened(dir.path(), &format!("(;SZ[9]{lz};B[ee]{lz};W[cc]{lz})"));
    edit(
        &mut wb,
        EditCommand::Transform {
            transform: Transform::RotateRight,
        },
    );
    let r = rec(&wb);
    let ee = r.main_child(ROOT).unwrap();
    let cc = r.main_child(ee).unwrap();
    let has_lz = |id: NodeId| r.node(id).properties.iter().any(|p| p.id == "LZ");
    assert_eq!((has_lz(ROOT), has_lz(ee), has_lz(cc)), (true, true, false));
}

#[test]
fn letters_continue_after_the_first_character_of_any_label() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path(), "(;SZ[9]LB[aa:Alpha][bb:12x])");
    for (at, tool) in [("ee", MarkTool::Letter), ("ff", MarkTool::Number)] {
        edit(&mut wb, EditCommand::ToggleMark { point: p(at), tool });
    }
    let marks = &rec(&wb).node(ROOT).marks;
    let label = |q: &str| marks.iter().find(|m| m.point == p(q)).map(|m| m.mark.clone());
    assert_eq!(label("ee"), Some(MarkKind::Label("B".into())));
    assert_eq!(label("ff"), Some(MarkKind::Label("1".into())), "12x is not a number");
}

#[test]
fn no_capture_places_stones_without_removing_any_and_keeps_earlier_positions() {
    let dir = tempfile::tempdir().unwrap();
    // Black surrounds a white stone on the edge; the last black move would capture it.
    let text = "(;SZ[9];B[ba];W[aa];B[ab])";
    let mut wb = opened(dir.path(), text);
    wb.navigate(Navigation::Last);
    assert_eq!(wb.view(false).position.stones[0], None, "normal rules capture");

    let mut settings = wb.config().edit.clone();
    settings.no_capture = true;
    wb.set_edit_settings(settings).unwrap();
    assert_eq!(
        wb.view(false).position.stones[0],
        None,
        "turning it on leaves positions as they were placed"
    );
    // White into the corner it was taken from: a suicide under normal rules, a stone that stays
    // here, and nothing more is captured.
    let corner = EditCommand::Play {
        point: p("aa"),
        color: Some(Stone::White),
    };
    assert_eq!(edit(&mut wb, corner), CommandResult::Done);
    let v = wb.view(false);
    assert_eq!(v.position.stones[0], Some(Stone::White));
    assert_eq!((v.position.black_captures, v.position.white_captures), (1, 0));
    // The setting is saved and a record read while it is on removes nothing either.
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&wb.storage().config_file).unwrap()).unwrap();
    assert_eq!(saved["edit"]["noCapture"], true);
    open(&mut wb, text);
    wb.navigate(Navigation::Last);
    assert_eq!(wb.view(false).position.stones[0], Some(Stone::White));
    // An occupied point is still refused.
    assert_eq!(play(&mut wb, "ba"), CommandResult::Unchanged);
    // Reading follows the setting move by move (`BoardHistoryList.place`): a white stone without
    // liberties is read, and a later move on its point is skipped instead of overwriting it.
    open(&mut wb, "(;SZ[9]AB[ba][ab];W[aa])");
    wb.navigate(Navigation::Last);
    let v = wb.view(false);
    assert_eq!((v.position.move_number, v.position.stones[0]), (1, Some(Stone::White)));
    open(&mut wb, "(;SZ[9];B[ba];W[aa];B[ab];B[aa])");
    wb.navigate(Navigation::Last);
    let v = wb.view(false);
    assert_eq!((v.position.move_number, v.position.stones[0]), (3, Some(Stone::White)));
}

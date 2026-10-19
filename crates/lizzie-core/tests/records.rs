//! Record reading semantics through the public reader API. Expectations come from the fixed
//! Java build (`SGFParser`, `GIBParser`, `docs/SNAPSHOT_NODE_KIND.md`) and SGF FF[4], not from
//! this implementation's output.

use lizzie_core::formats::{self, sgf};
use lizzie_core::record::{BoardSize, GameRecord, MarkKind, NodeId, NodeKind, Point, RecordFormat, Stone, ROOT};

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(name)
}

fn p(s: &str) -> Point {
    let b = s.as_bytes();
    Point {
        x: (b[0] - b'a') as u16,
        y: (b[1] - b'a') as u16,
    }
}

fn main_line(rec: &GameRecord) -> Vec<NodeId> {
    let mut line = vec![ROOT];
    while let Some(c) = rec.main_child(*line.last().unwrap()) {
        line.push(c);
    }
    line
}

fn read(text: &str) -> GameRecord {
    sgf::read(text).expect("valid sgf")
}

#[test]
fn root_setup_stones_are_static_and_do_not_count_as_moves() {
    let (_, rec) = formats::read_file(&fixture("review-19x19-zh.sgf")).unwrap();
    let root = rec.node(ROOT);
    assert_eq!(root.kind, NodeKind::Snapshot);
    assert_eq!(root.move_number, 0);
    let pos = rec.position(ROOT);
    assert_eq!(pos.at(p("pd")), Some(Stone::Black));
    assert_eq!(pos.at(p("dp")), Some(Stone::Black));
    assert_eq!(pos.at(p("dd")), Some(Stone::White));
    assert_eq!(pos.at(p("pp")), Some(Stone::White));
    assert_eq!(root.to_play, Stone::Black, "explicit PL[B]");
    let first = rec.main_child(ROOT).unwrap();
    assert_eq!(
        rec.node(first).kind,
        NodeKind::Move {
            color: Stone::Black,
            point: p("qq")
        }
    );
    assert_eq!(rec.node(first).move_number, 1);
    assert_eq!(rec.node(first).comment, "黑先占角");
}

#[test]
fn comments_keep_escaped_characters_and_line_breaks() {
    let (_, rec) = formats::read_file(&fixture("review-19x19-zh.sgf")).unwrap();
    assert_eq!(
        rec.node(ROOT).comment,
        "根注释：四角摆子开局。\n第二行含转义 ] 与反斜杠 \\"
    );
}

#[test]
fn branches_and_genuine_pass_follow_the_tree() {
    let (_, rec) = formats::read_file(&fixture("review-19x19-zh.sgf")).unwrap();
    let line = main_line(&rec);
    let end = *line.last().unwrap();
    assert_eq!(rec.node(end).comment, "主线终点");
    // Move 30 (W[rb]) has two continuations: the main line B[nf] and the side line B[jj].
    let branch_point = line[30];
    assert_eq!(
        rec.node(branch_point).kind,
        NodeKind::Move {
            color: Stone::White,
            point: p("rb")
        }
    );
    let kids = &rec.node(branch_point).children;
    assert_eq!(kids.len(), 2);
    assert_eq!(
        rec.node(kids[1]).kind,
        NodeKind::Move {
            color: Stone::Black,
            point: p("jj")
        }
    );
    assert_eq!(rec.node(kids[1]).marks[0].mark, MarkKind::Label("甲".into()));
    // The W[] variation is a real PASS that consumes a move number; play continues after it.
    let pass = rec
        .nodes()
        .find(|(_, n)| matches!(n.kind, NodeKind::Pass { .. }))
        .map(|(id, _)| id)
        .expect("pass node");
    let parent = rec.parent(pass).unwrap();
    assert_eq!(rec.node(pass).kind, NodeKind::Pass { color: Stone::White });
    assert_eq!(rec.node(pass).move_number, rec.node(parent).move_number + 1);
    assert_eq!(rec.node(pass).comment, "白方真实停着变化");
    assert_eq!(rec.node(pass).to_play, Stone::Black);
    let after = rec.main_child(pass).unwrap();
    assert_eq!(
        rec.node(after).kind,
        NodeKind::Move {
            color: Stone::Black,
            point: p("gf")
        }
    );
    assert_eq!(rec.position(after).stones, {
        let mut expect = rec.position(parent);
        expect.play(Stone::Black, p("gf"));
        expect.stones
    });
}

#[test]
fn marks_are_read_per_node() {
    let (_, rec) = formats::read_file(&fixture("review-19x19-zh.sgf")).unwrap();
    let marked = rec.nodes().find(|(_, n)| n.comment == "标记示例").unwrap().1;
    assert!(marked
        .marks
        .iter()
        .any(|m| m.point == p("dg") && m.mark == MarkKind::Square));
    assert!(marked
        .marks
        .iter()
        .any(|m| m.point == p("df") && m.mark == MarkKind::Label("A".into())));
    assert!(marked
        .marks
        .iter()
        .any(|m| m.point == p("rr") && m.mark == MarkKind::Label("B".into())));
    let tri = rec.nodes().find(|(_, n)| n.comment == "主线第 20 手").unwrap().1;
    assert_eq!(tri.move_number, 20);
    assert_eq!(tri.marks[0].mark, MarkKind::Triangle);
}

#[test]
fn game_info_and_komi_come_from_the_root() {
    let (format, rec) = formats::read_file(&fixture("review-19x19-zh.sgf")).unwrap();
    assert_eq!(format, RecordFormat::Sgf);
    assert_eq!(rec.info.black, "黑方测试");
    assert_eq!(rec.info.white, "白方测试");
    assert_eq!(rec.info.komi, 7.5);
    assert!(rec.komi_declared);
    assert_eq!(rec.info.rules.as_deref(), Some("Chinese"));
    assert!(rec.has_setup_or_handicap);
}

#[test]
fn nine_by_nine_record_uses_its_own_size() {
    let (_, rec) = formats::read_file(&fixture("review-9x9.sgf")).unwrap();
    assert_eq!(rec.size, BoardSize { width: 9, height: 9 });
    assert_eq!(rec.position(ROOT).stones.len(), 81);
    assert_eq!(rec.info.result, "W+R");
    let end = *main_line(&rec).last().unwrap();
    assert_eq!(rec.node(end).comment, "main end");
    assert_eq!(rec.node(end).move_number, 6);
}

#[test]
fn missing_size_is_19_and_rectangular_size_is_kept() {
    assert_eq!(read("(;B[aa])").size, BoardSize::STANDARD);
    assert_eq!(read("(;SZ[13:7];B[aa])").size, BoardSize { width: 13, height: 7 });
    assert_eq!(read("(;SZ[1];B[aa])").size, BoardSize::STANDARD);
}

#[test]
fn tt_is_a_pass_up_to_19_and_a_point_on_larger_boards() {
    let rec = read("(;SZ[19];B[pd];W[tt];B[dd])");
    let line = main_line(&rec);
    assert_eq!(rec.node(line[2]).kind, NodeKind::Pass { color: Stone::White });
    assert_eq!(rec.node(line[3]).move_number, 3);
    let big = read("(;SZ[21];B[tt])");
    assert_eq!(
        rec_kind(&big, 1),
        NodeKind::Move {
            color: Stone::Black,
            point: Point { x: 19, y: 19 }
        }
    );
}

fn rec_kind(rec: &GameRecord, depth: usize) -> NodeKind {
    rec.node(main_line(rec)[depth]).kind
}

#[test]
fn mid_game_setup_is_a_separate_snapshot_without_a_move_number() {
    let rec = read("(;SZ[9];B[ee];AW[aa][bb]C[setup];W[cc])");
    let line = main_line(&rec);
    assert_eq!(line.len(), 4);
    assert_eq!(rec.node(line[2]).kind, NodeKind::Snapshot);
    assert_eq!(rec.node(line[2]).move_number, 1);
    assert_eq!(rec.node(line[2]).comment, "setup");
    assert_eq!(rec.node(line[2]).to_play, Stone::White);
    assert_eq!(rec.node(line[3]).move_number, 2);
    assert_eq!(rec.position(line[3]).at(p("aa")), Some(Stone::White));
}

#[test]
fn setup_then_move_in_one_node_gives_snapshot_then_move() {
    let rec = read("(;SZ[9];B[ee];AB[aa]W[cc])");
    let line = main_line(&rec);
    assert_eq!(rec.node(line[2]).kind, NodeKind::Snapshot);
    assert_eq!(
        rec.node(line[3]).kind,
        NodeKind::Move {
            color: Stone::White,
            point: p("cc")
        }
    );
}

#[test]
fn comment_only_nodes_attach_to_the_previous_node() {
    let rec = read("(;SZ[9];B[ee]C[move];C[note]TR[aa];W[cc])");
    let line = main_line(&rec);
    assert_eq!(line.len(), 3, "no node for the comment-only SGF node");
    assert_eq!(rec.node(line[1]).comment, "move\nnote");
    assert_eq!(rec.node(line[1]).marks[0].point, p("aa"));
    // Before the first move the comment goes to the root; in a variation, to its parent.
    let rec = read("(;SZ[9];C[intro];B[ee](;C[v1];W[aa])(;W[bb]))");
    assert_eq!(rec.node(ROOT).comment, "intro");
    let first = rec.main_child(ROOT).unwrap();
    assert_eq!(rec.node(first).comment, "v1");
}

#[test]
fn illegal_moves_are_skipped_without_failing() {
    // Occupied point, then off-board, then a legal move.
    let rec = read("(;SZ[9];B[ee];W[ee];W[zz];W[aa])");
    let line = main_line(&rec);
    assert_eq!(line.len(), 3);
    assert_eq!(
        rec.node(line[2]).kind,
        NodeKind::Move {
            color: Stone::White,
            point: p("aa")
        }
    );
    // Suicide is rejected.
    let rec = read("(;SZ[9]AW[ba][ab];B[aa])");
    assert_eq!(main_line(&rec).len(), 1);
}

#[test]
fn captures_are_removed_and_counted() {
    let rec = read("(;SZ[9];B[ba];W[aa];B[ab])");
    let end = *main_line(&rec).last().unwrap();
    let pos = rec.position(end);
    assert_eq!(pos.at(p("aa")), None);
    assert_eq!(pos.black_captures, 1);
}

#[test]
fn initial_side_to_play_follows_the_import_rule() {
    // First main-line action decides when PL is missing.
    assert_eq!(read("(;SZ[9]AB[aa];W[ee])").node(ROOT).to_play, Stone::White);
    // Explicit PL wins over the first action.
    assert_eq!(read("(;SZ[9]AB[aa]PL[B];W[ee])").node(ROOT).to_play, Stone::Black);
    // Completed handicap without moves: White.
    assert_eq!(read("(;SZ[19]HA[2]AB[dd][pp])").node(ROOT).to_play, Stone::White);
    // Handicap count not matching the stones: Black.
    assert_eq!(read("(;SZ[19]HA[3]AB[dd][pp])").node(ROOT).to_play, Stone::Black);
    // A side line does not vote.
    assert_eq!(read("(;SZ[9]AB[aa](;B[ee])(;W[cc]))").node(ROOT).to_play, Stone::Black);
}

#[test]
fn compressed_point_lists_are_expanded() {
    let rec = read("(;SZ[9]AB[aa:bb])");
    let pos = rec.position(ROOT);
    for s in ["aa", "ba", "ab", "bb"] {
        assert_eq!(pos.at(p(s)), Some(Stone::Black), "{s}");
    }
}

#[test]
fn komi_is_normalised_like_java() {
    assert_eq!(read("(;KM[650])").info.komi, 6.5);
    assert_eq!(read("(;KM[375])").info.komi, 7.5);
    assert_eq!(read("(;KM[3.75])").info.komi, 7.5);
    assert_eq!(read("(;AP[foxwq]KM[650])").info.komi, 6.5);
    assert_eq!(read("(;AP[foxwq]RU[Chinese]KM[375])").info.komi, 7.5);
    assert!(!read("(;B[aa])").komi_declared);
}

#[test]
fn node_name_fills_an_empty_comment() {
    let rec = read("(;SZ[9];B[ee]N[Key move])");
    assert_eq!(rec.node(rec.main_child(ROOT).unwrap()).comment, "Key move");
}

#[test]
fn dummy_placeholder_is_not_a_move() {
    let rec = read("(;SZ[9];B[ee](;W[aa])(;DD[true]))");
    let dummy = rec
        .nodes()
        .find(|(_, n)| n.kind == NodeKind::DummyPass)
        .map(|(id, _)| id);
    assert!(dummy.is_some());
    assert_eq!(rec.node(rec.main_child(ROOT).unwrap()).children.len(), 2);
}

#[test]
fn malformed_text_fails_to_read() {
    assert!(sgf::read("no game here").is_err());
    assert!(sgf::read("(;C[unterminated").is_err());
    assert!(sgf::read("").is_err());
    assert!(formats::parse(RecordFormat::Sgf, "").is_err());
}

#[test]
fn literal_structural_newlines_are_accepted() {
    let rec = read("(;SZ[9]\\n;B[ee]\\r\\n;W[aa])");
    assert_eq!(main_line(&rec).len(), 3);
    assert!(sgf::read("(;SZ[9]\\x;B[ee])").is_err());
}

#[test]
fn gbk_bytes_are_decoded() {
    let (bytes, _, _) = encoding_rs::GBK.encode("(;PB[黑方棋手]PW[白方棋手]C[这是一个中文评论，用来检测编码])");
    let text = formats::decode(&bytes);
    let rec = read(&text);
    assert_eq!(rec.info.black, "黑方棋手");
}

#[test]
fn gib_handicap_is_setup_and_first_move_is_move_one() {
    let (format, rec) = formats::read_file(&fixture("handicap-2.gib")).unwrap();
    assert_eq!(format, RecordFormat::Gib);
    assert_eq!(rec.size, BoardSize::STANDARD);
    assert_eq!(rec.info.black, "黑方 (3D)");
    assert_eq!(rec.info.white, "白方 (5D)");
    assert_eq!(rec.info.handicap, 2);
    assert_eq!(rec.info.komi, 0.0);
    let root = rec.node(ROOT);
    assert_eq!(root.setup.add_black, vec![Point { x: 3, y: 15 }, Point { x: 15, y: 3 }]);
    assert_eq!(root.to_play, Stone::White);
    let line = main_line(&rec);
    assert_eq!(
        rec.node(line[1]).kind,
        NodeKind::Move {
            color: Stone::White,
            point: Point { x: 15, y: 15 }
        }
    );
    assert_eq!(rec.node(line[1]).move_number, 1);
    assert_eq!(rec.node(line[4]).kind, NodeKind::Pass { color: Stone::Black });
    assert_eq!(rec.node(line[5]).move_number, 5);
}

#[test]
fn gib_dispatch_is_by_extension_and_rejects_non_gib_text() {
    assert_eq!(formats::format_for_path("/x/game.GIB"), RecordFormat::Gib);
    assert_eq!(formats::format_for_path("/x/game.txt"), RecordFormat::Sgf);
    assert!(formats::parse(RecordFormat::Gib, "hello").is_err());
    assert!(formats::parse(RecordFormat::Gib, "STO 0 1 1 x 3").is_err());
}

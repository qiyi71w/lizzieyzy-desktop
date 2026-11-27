//! Java `teacher/MoveShapes.java`: names of a move's shape on the frozen board, the facts section
//! that lists them, the prompt clauses that ask the model to use them, and one controlled reply
//! through the grounded loop. Expectations are Java's own test examples, its rules and prompt
//! wording, and positions replayed by hand.
use lizzie_core::api::{CandidateView, NodeAnalysisView};
use lizzie_core::commentary::{
    self,
    evidence::{self, Line, Position},
    grounded, prompt,
    settings::Preferences,
    verify, ChatTransport, CommentaryFailure, CommentaryJob, JobConnection, Message,
};
use lizzie_core::formats::sgf;
use lizzie_core::record::{BoardSize, Point, Position as BoardPosition, Stone, ROOT};
use lizzie_core::settings::Language;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

fn point(gtp: &str) -> Point {
    evidence::parse_coordinate(gtp, BoardSize::STANDARD).expect("test coordinate")
}

fn board(stones: &[(&str, bool)]) -> BoardPosition {
    let mut board = BoardPosition::new(BoardSize::STANDARD);
    for (gtp, black) in stones {
        board.put(if *black { Stone::Black } else { Stone::White }, point(gtp));
    }
    board
}

fn shape(r#move: &str, black: bool, stones: &[(&str, bool)]) -> Option<String> {
    commentary::shapes::describe(&board(stones), r#move, black)
}

#[test]
fn tactical_moves_are_named_first() {
    assert_eq!(
        shape("A18", true, &[("A19", false), ("B19", true)]).as_deref(),
        Some("capture (提子) of W A19")
    );
    assert_eq!(
        shape("L10", true, &[("K10", false), ("K11", true), ("J10", true)]).as_deref(),
        Some("atari (打吃) on W K10")
    );
    assert_eq!(
        shape("E4", true, &[("D4", true), ("F4", true)]).as_deref(),
        Some("connection (粘) joining B D4 F4")
    );
    assert_eq!(
        shape("E4", true, &[("D4", false), ("F4", false)]).as_deref(),
        Some("wedge (挖) between W D4 and F4")
    );
    assert_eq!(
        shape("E4", true, &[("D4", false), ("E5", false), ("D5", true)]).as_deref(),
        Some("cut (断) between W D4 and E5")
    );
    // Between two stones with neither a gap nor our stone across: tactical, but Java names it
    // nothing and looks no further.
    assert_eq!(
        shape("E4", true, &[("D4", false), ("E5", false)]),
        None,
        "between two stones without a cut"
    );
}

#[test]
fn capture_takes_priority_over_connection() {
    // Java names a capture before checking whether the move also joins own groups.
    assert_eq!(
        shape(
            "E4",
            true,
            &[
                ("D4", true),
                ("F4", true),
                ("E5", false),
                ("D5", true),
                ("F5", true),
                ("E6", true),
            ]
        )
        .as_deref(),
        Some("capture (提子) of W E5")
    );
}

#[test]
fn contact_moves_are_named_by_the_stones_they_touch() {
    assert_eq!(
        shape("R17", true, &[("Q16", true), ("R16", false)]).as_deref(),
        Some("hane (扳) against W R16")
    );
    assert_eq!(
        shape("Q15", true, &[("Q16", true), ("R16", false)]).as_deref(),
        Some("extension (长) from B Q16")
    );
    assert_eq!(
        shape("R16", true, &[("Q16", false)]).as_deref(),
        Some("attachment (靠) to W Q16")
    );
    assert_eq!(
        shape("P16", true, &[("Q16", true), ("R16", false)]),
        None,
        "pulling back is not an extension"
    );
    assert_eq!(
        shape("L10", true, &[("K10", true), ("L11", false)]),
        None,
        "touching both colours is not named"
    );
}

#[test]
fn corner_moves_follow_the_stones_in_the_corner() {
    assert_eq!(
        shape("Q16", true, &[]).as_deref(),
        Some("move in an empty corner (占角)")
    );
    assert_eq!(
        shape("D4", true, &[]).as_deref(),
        Some("move in an empty corner (占角)")
    );
    assert_eq!(
        shape("R17", true, &[("Q16", false)]).as_deref(),
        Some("3-3 invasion (点三三) under W Q16")
    );
    assert_eq!(
        shape("R14", true, &[("Q16", false)]).as_deref(),
        Some("corner approach (挂角) to W Q16")
    );
    assert_eq!(
        shape("R14", true, &[("Q16", true)]).as_deref(),
        Some("corner enclosure (守角) as a knight's move (小飞) from B Q16")
    );
    assert_eq!(
        shape("R14", true, &[("Q16", false), ("O17", false)]),
        None,
        "an enclosed corner is not approached"
    );
    assert_eq!(
        shape("S18", true, &[("Q16", true)]),
        None,
        "the 2-2 point does not enclose a corner"
    );
}

/// Java `sideExtension`: on the third line, our own stone along the side with one to four empty
/// points between is 拆一 to 拆四; the gap is counted from the move.
#[test]
fn side_extensions_are_named_by_the_points_between() {
    for (stone, name) in [("R12", "拆一"), ("R13", "拆二"), ("R14", "拆三"), ("R15", "拆四")] {
        assert_eq!(
            shape("R10", true, &[(stone, true)]).as_deref(),
            Some(format!("extension along the side ({name}) from B {stone}").as_str()),
            "{stone}"
        );
    }
}

#[test]
fn moves_away_from_contact_are_named_by_their_own_stones() {
    assert_eq!(
        shape("L11", true, &[("K10", true)]).as_deref(),
        Some("diagonal move (尖) from B K10")
    );
    assert_eq!(
        shape("K12", true, &[("K10", true)]).as_deref(),
        Some("one-space jump (跳) from B K10")
    );
    assert_eq!(
        shape("L12", true, &[("K10", true)]).as_deref(),
        Some("knight's move (小飞) from B K10")
    );
    assert_eq!(
        shape("K13", true, &[("K10", true)]).as_deref(),
        Some("two-space jump (大跳) from B K10")
    );
    assert_eq!(
        shape("L13", true, &[("K10", true)]).as_deref(),
        Some("large knight's move (大飞) from B K10")
    );
    assert_eq!(
        shape("K13", true, &[("K10", true), ("K11", false)]),
        None,
        "a blocked jump is not a jump"
    );
    assert_eq!(
        shape(
            "K10",
            true,
            &[("H10", false), ("M10", false), ("K13", false), ("K7", false)]
        )
        .as_deref(),
        Some("invasion (打入)")
    );
}

/// Java `relation` tries 尖 before 跳: a move that is both is a diagonal move, and only without
/// the diagonal stone a jump.
#[test]
fn a_diagonal_move_is_named_before_a_jump() {
    assert_eq!(
        shape("K10", true, &[("L11", true), ("K12", true)]).as_deref(),
        Some("diagonal move (尖) from B L11")
    );
    assert_eq!(
        shape("K10", true, &[("K12", true)]).as_deref(),
        Some("one-space jump (跳) from B K12")
    );
}

#[test]
fn white_moves_and_unnamed_moves_work() {
    assert_eq!(
        shape("C3", false, &[("D4", true)]).as_deref(),
        Some("3-3 invasion (点三三) under B D4")
    );
    assert_eq!(shape("K10", true, &[]), None, "the centre has no name");
    assert_eq!(shape("pass", true, &[("K10", true)]), None);
    assert_eq!(shape("K10", true, &[("K10", true)]), None, "an occupied point");
}

/// `game`'s first position with `candidate` as the engine's one candidate, its lines replayed the
/// way commentary replays them.
fn replayed(game: &str, candidate: &str) -> Position {
    let rec = sgf::read(game).unwrap();
    let candidate = point(candidate);
    let analysis = NodeAnalysisView {
        node: ROOT,
        engine: "test".into(),
        to_play: Stone::Black,
        visits: 900,
        root_visits: None,
        winrate: 60.0,
        score_lead: Some(3.0),
        score_stdev: None,
        komi: 7.5,
        candidates: vec![CandidateView {
            point: Some(candidate),
            visits: 900,
            allocation_visits: 900,
            winrate: 60.0,
            score_lead: Some(3.0),
            score_stdev: None,
            prior: 0.5,
            lcb: None,
            order: 0,
            pv: vec![Some(candidate)],
            pv_visits: Vec::new(),
            pv_captures: vec![Vec::new()],
            shown: Default::default(),
            side_to_move: Default::default(),
            ownership: None,
            kept: false,
        }],
        black: Default::default(),
    };
    evidence::current(&rec, ROOT, Some(&analysis)).unwrap()
}

/// White A19 and Black B19, Black to play; the game went on with Black A18, which takes A19, and
/// the engine's candidate is Q16.
fn position() -> Position {
    replayed("(;GM[1]FF[4]SZ[19]AW[aa]AB[ba];B[ab])", "Q16")
}

fn last_frame(line: &Line) -> &evidence::Frame {
    line.frames.last().unwrap()
}

/// The replayed lines end where the moves leave the board: A18 takes the white stone, Q16 takes
/// nothing.
#[test]
fn each_line_ends_on_the_board_its_move_leaves() {
    let position = position();
    let context = position.board.as_ref().unwrap();
    assert_eq!(context.stones.stones, board(&[("A19", false), ("B19", true)]).stones);
    let [recorded, candidate] = context.lines.as_slice() else {
        panic!("{:?}", context.lines)
    };
    assert_eq!(
        (recorded.label.as_str(), recorded.moves.as_slice()),
        ("recorded", ["A18".to_string()].as_slice())
    );
    let after = last_frame(recorded);
    assert_eq!(
        (after.r#move.as_str(), after.color.as_str(), after.captures),
        ("A18", "B", 1)
    );
    assert_eq!(after.stones.stones, board(&[("A18", true), ("B19", true)]).stones);
    assert_eq!(
        (candidate.label.as_str(), candidate.moves.as_slice()),
        ("Q16", ["Q16".to_string()].as_slice())
    );
    let after = last_frame(candidate);
    assert_eq!(
        (after.r#move.as_str(), after.color.as_str(), after.captures),
        ("Q16", "B", 0)
    );
    assert_eq!(
        after.stones.stones,
        board(&[("A19", false), ("B19", true), ("Q16", true)]).stones
    );
}

#[test]
fn facts_list_each_line_first_move_shape_in_java_format() {
    let mut board_context = position().board.unwrap();
    board_context.knowledge = "sample knowledge\n".into();
    let facts = board_context.facts();
    let shapes = "Move shapes (from the stones alone; other moves have no listed name):\n\
                  Recorded A18: capture (提子) of W A19\n\
                  Candidate Q16: move in an empty corner (占角)\n";
    assert!(facts.contains(shapes), "{facts}");
    assert!(
        facts.find("Move shapes").unwrap() < facts.find("Position-specific knowledge").unwrap(),
        "{facts}"
    );
}

#[test]
fn facts_omit_the_section_when_no_line_has_a_named_shape() {
    // The game and the engine both open in the centre, which has no name.
    let position = replayed("(;GM[1]FF[4]SZ[19];B[jj])", "K10");
    let board_context = position.board.unwrap();
    assert_eq!(board_context.lines.len(), 2);
    assert!(
        !board_context.facts().contains("Move shapes"),
        "{}",
        board_context.facts()
    );
}

fn prompt_position() -> Position {
    Position {
        move_number: 1,
        to_play: "B".into(),
        playouts: 100,
        actual_move: String::new(),
        actual_winrate_loss: None,
        candidates: Vec::new(),
        played_continuation: Vec::new(),
        board: None,
    }
}

#[test]
fn prompts_ask_for_listed_shape_names_in_chinese_and_english() {
    let position = prompt_position();
    let chinese = prompt::for_position(&position, Language::ZhCn, &Preferences::default())[0]
        .content
        .clone();
    let english = prompt::for_position(&position, Language::EnUs, &Preferences::default())[0]
        .content
        .clone();
    assert!(chinese.contains("用招法名称讲棋"), "{chinese}");
    assert!(
        chinese.contains("没有列出、知识库也没有的招法不要自己命名"),
        "{chinese}"
    );
    for system in [chinese, english] {
        assert!(
            system.contains("when Move shapes lists a move, call it by that name"),
            "{system}"
        );
        assert!(system.contains("Never name a shape that is not listed"), "{system}");
        assert!(system.contains("give its coordinate once at most"), "{system}");
    }
}

struct Reply(String);
impl ChatTransport for Reply {
    fn complete(&mut self, _: &[Message], _: &AtomicBool) -> Result<String, CommentaryFailure> {
        Ok(self.0.clone())
    }
}

#[test]
fn controlled_reply_using_the_listed_shape_passes_the_full_lesson_flow() {
    let position = position();
    let messages = prompt::for_position(&position, Language::ZhCn, &Preferences::default());
    assert!(
        messages[1].content.contains("Recorded A18: capture (提子) of W A19"),
        "{}",
        messages[1].content
    );
    let job = CommentaryJob {
        id: 1,
        messages,
        positions: vec![position],
        connection: JobConnection::ApiKey {
            base_url: "https://example.com".into(),
            model: "test".into(),
        },
        cancel: Arc::new(AtomicBool::new(false)),
    };
    let answer = "黑 A18 提子，提掉白 A19。";
    let mut transport = Reply(answer.into());
    assert_eq!(grounded::run(&mut transport, &job).unwrap(), answer);
    assert!(verify::verify(answer, &job.positions).ok());
    assert!(!job.cancel.load(Ordering::Acquire));
}

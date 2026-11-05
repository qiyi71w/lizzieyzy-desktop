//! Board display projection (棋盘显示) as the UI receives it: move numbers in every Java mode
//! through `Workbench::view`, 落子评价 and 下一手 详细 from stored analysis, and the display
//! settings' validation and persistence.

use lizzie_core::api::{CandidateView, EditCommand, NodeAnalysisView};
use lizzie_core::browse::Navigation;
use lizzie_core::display::{self, MoveRank, NextMoveVerdict};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::formats::{self, sgf};
use lizzie_core::record::{GameRecord, NodeId, Point, RecordFormat, Stone, ROOT};
use lizzie_core::settings::{DisplaySettings, MoveRankMode, Storage};
use lizzie_core::workbench::{OpenOrigin, Workbench};

struct NoEngines;

impl Launcher for NoEngines {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        Err(std::io::Error::other("no engines in display tests"))
    }
}

fn storage(dir: &std::path::Path) -> Storage {
    Storage::in_dirs(&dir.join("config"), &dir.join("data"))
}

fn workbench(dir: &std::path::Path) -> Workbench {
    Workbench::new(storage(dir), None, Box::new(NoEngines), Vec::new())
}

fn open(wb: &mut Workbench, text: &str) {
    let read = formats::parse(RecordFormat::Sgf, text, wb.config().edit.no_capture);
    let t = wb.begin_open(OpenOrigin::File {
        path: "/x.sgf".into(),
        source: lizzie_core::api::OpenSource::Drop,
    });
    assert!(wb.complete_open(t, read.map(|r| (RecordFormat::Sgf, r))));
}

fn display(wb: &mut Workbench, change: impl FnOnce(&mut DisplaySettings)) {
    let mut s = wb.config().display.clone();
    change(&mut s);
    wb.set_display_settings(s).unwrap();
}

/// SGF coordinates (`"dd"`).
fn p(s: &str) -> Point {
    let b = s.as_bytes();
    Point {
        x: u16::from(b[0] - b'a'),
        y: u16::from(b[1] - b'a'),
    }
}

fn sgf_point(point: Point) -> String {
    [point.x, point.y].iter().map(|&c| char::from(b'a' + c as u8)).collect()
}

/// The drawn move numbers, `"ee1"`, with `*` after the last move's.
fn numbers(wb: &mut Workbench) -> Vec<String> {
    let mut out: Vec<_> = wb
        .view(false)
        .board
        .move_numbers
        .iter()
        .map(|l| format!("{}{}{}", sgf_point(l.point), l.number, if l.last { "*" } else { "" }))
        .collect();
    out.sort();
    out
}

fn child(rec: &GameRecord, moves: &[usize]) -> NodeId {
    let mut id = ROOT;
    for &i in moves {
        id = rec.node(id).children[i];
    }
    id
}

fn goto(wb: &mut Workbench, moves: &[usize]) {
    let id = child(&wb.document().record, moves);
    wb.goto_node(id).unwrap();
}

fn play(wb: &mut Workbench, at: &str) {
    wb.edit(EditCommand::Play { point: p(at), color: None }).unwrap();
}

#[test]
fn last_moves_count_back_from_the_newest_and_can_restart_at_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    open(&mut wb, "(;SZ[9];B[aa];W[ba];B[ca];W[da];B[ea])");
    wb.navigate(Navigation::Last);
    assert!(numbers(&mut wb).is_empty(), "a new profile shows no move numbers");
    display(&mut wb, |s| s.move_numbers = -1);
    assert_eq!(numbers(&mut wb), ["aa1", "ba2", "ca3", "da4", "ea5*"]);
    display(&mut wb, |s| s.move_numbers = 2);
    assert_eq!(numbers(&mut wb), ["da4", "ea5*"]);
    display(&mut wb, |s| s.move_numbers_from_one = true);
    assert_eq!(numbers(&mut wb), ["da1", "ea2*"]);
    // 全部 ignores 总是从1开始.
    display(&mut wb, |s| s.move_numbers = -1);
    assert_eq!(numbers(&mut wb), ["aa1", "ba2", "ca3", "da4", "ea5*"]);
}

const TREE: &str = "(;SZ[9];B[ee];W[gc];B[cg](;W[gg];B[fg];W[gf])(;W[cc];B[dc]))";

#[test]
fn a_branch_read_with_branch_numbering_starts_at_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    display(&mut wb, |s| s.move_numbers = -1);
    open(&mut wb, TREE);
    goto(&mut wb, &[0, 0, 0, 1, 0]);
    assert_eq!(numbers(&mut wb), ["cc1", "dc2*"]);
    goto(&mut wb, &[0, 0, 0, 0, 0, 0]);
    assert_eq!(numbers(&mut wb), ["cg3", "ee1", "fg5", "gc2", "gf6*", "gg4"]);

    // The setting applies to records read after it changed.
    display(&mut wb, |s| s.branch_numbers_from_one = false);
    goto(&mut wb, &[0, 0, 0, 1, 0]);
    assert_eq!(numbers(&mut wb), ["cc1", "dc2*"], "the open record keeps its numbering");
    open(&mut wb, TREE);
    goto(&mut wb, &[0, 0, 0, 1, 0]);
    assert_eq!(numbers(&mut wb), ["cc4", "cg3", "dc5*", "ee1", "gc2"]);
}

#[test]
fn branch_numbering_is_fixed_when_the_branch_is_played() {
    // Java 分支手数从1开始: "只影响新落子的分支".
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    display(&mut wb, |s| s.move_numbers = -1);
    open(&mut wb, "(;SZ[9];B[ee];W[gc])");
    goto(&mut wb, &[0]);
    play(&mut wb, "cc");
    assert_eq!(numbers(&mut wb), ["cc1*"]);
    display(&mut wb, |s| s.branch_numbers_from_one = false);
    assert_eq!(numbers(&mut wb), ["cc1*"], "an existing branch keeps its numbering");
    goto(&mut wb, &[0]);
    play(&mut wb, "dd");
    assert_eq!(numbers(&mut wb), ["dd2*", "ee1"]);
}

#[test]
fn a_pass_keeps_counting_inside_a_branch() {
    // Java left a pass uncounted, so the branch fell back to the total move number after it.
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    display(&mut wb, |s| s.move_numbers = -1);
    open(&mut wb, "(;SZ[9];B[ee](;W[gc])(;W[];B[cc];W[dd]))");
    goto(&mut wb, &[0, 1, 0, 0]);
    assert_eq!(numbers(&mut wb), ["cc2", "dd3*"]);
}

#[test]
fn setup_stones_take_no_number_and_do_not_start_the_count() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    display(&mut wb, |s| s.move_numbers = -1);
    open(&mut wb, "(;SZ[9];AB[aa]AW[bb];B[cc];W[dd])");
    wb.navigate(Navigation::Last);
    assert_eq!(numbers(&mut wb), ["cc1", "dd2*"]);
}

#[test]
fn an_sgf_move_number_renumbers_from_that_move() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    display(&mut wb, |s| s.move_numbers = -1);
    open(&mut wb, "(;SZ[9];B[aa];W[ba]MN[10];B[ca])");
    wb.navigate(Navigation::Last);
    assert_eq!(numbers(&mut wb), ["aa1", "ba10", "ca11*"]);
}

#[test]
fn captured_stones_lose_their_numbers() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    display(&mut wb, |s| s.move_numbers = -1);
    open(&mut wb, "(;SZ[9];B[ba];W[aa];B[ab])");
    wb.navigate(Navigation::Last);
    assert_eq!(numbers(&mut wb), ["ab3*", "ba1"]);
}

#[test]
fn all_numbers_in_a_branch_show_with_numbers_off_unless_move_ranks_do() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    // 落子评价 shows the last move by default; 不显示 it so the numbers can show.
    display(&mut wb, |s| {
        s.all_numbers_in_branch = true;
        s.move_rank = -1;
    });
    open(&mut wb, TREE);
    goto(&mut wb, &[0, 0, 0, 1, 0]);
    assert_eq!(numbers(&mut wb), ["cc1", "dc2*"]);
    goto(&mut wb, &[0, 0, 0, 0]);
    assert!(numbers(&mut wb).is_empty(), "the main trunk follows 不显示");
    // moveOverlayOrder: 落子评价 replaces the numbers while 手数 is off.
    display(&mut wb, |s| s.move_rank = 1);
    goto(&mut wb, &[0, 0, 0, 1, 0]);
    assert!(numbers(&mut wb).is_empty());
}

#[test]
fn invalid_display_settings_are_refused_and_valid_ones_reopen() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    let before = wb.config().display.clone();
    for bad in [
        DisplaySettings { move_rank: -2, ..before.clone() },
        DisplaySettings { move_rank_custom: 0, ..before.clone() },
        DisplaySettings { suggestion_info_order: [1, 2, 4], ..before.clone() },
        DisplaySettings { replay_interval_seconds: f64::NAN, ..before.clone() },
        DisplaySettings { win_loss_thresholds: [-1.0, f64::INFINITY, -6.0, -12.0, -24.0], ..before.clone() },
    ] {
        assert!(wb.set_display_settings(bad).is_err());
        assert_eq!(wb.config().display, before);
    }
    let chosen = DisplaySettings {
        move_numbers: 7,
        move_rank: 0,
        move_rank_mode: MoveRankMode::Combined,
        suggestion_info_order: [3, 1, 2],
        sub_board: lizzie_core::settings::SubBoardMode::Visits,
        coordinates: false,
        ..before.clone()
    };
    assert!(wb.set_display_settings(chosen.clone()).unwrap());
    assert!(!wb.set_display_settings(chosen.clone()).unwrap(), "an unchanged value is no change");
    assert_eq!(workbench(dir.path()).config().display, chosen);
}

// ------------------------------------------------------------------ analysis-based marks

fn candidate(at: &str, order: u32, visits: u64, winrate: f64, score: Option<f64>) -> CandidateView {
    CandidateView {
        point: Some(p(at)),
        visits,
        allocation_visits: visits,
        winrate,
        score_lead: score,
        score_stdev: None,
        prior: 0.1,
        lcb: None,
        order,
        pv: vec![Some(p(at))],
        pv_visits: Vec::new(),
        pv_captures: vec![Vec::new()],
        shown: Default::default(),
        side_to_move: Default::default(),
        ownership: None,
        kept: false,
    }
}

/// A position's analysis: side to move, best win rate and score, candidates (first is best).
fn analysis(node: NodeId, to_play: Stone, candidates: Vec<CandidateView>) -> NodeAnalysisView {
    NodeAnalysisView {
        node,
        engine: "test".into(),
        to_play,
        visits: candidates.iter().map(|c| c.visits).sum(),
        root_visits: None,
        winrate: candidates[0].winrate,
        score_lead: candidates[0].score_lead,
        score_stdev: None,
        komi: 7.5,
        candidates,
        black: Default::default(),
    }
}

fn ranks(rec: &GameRecord, at: NodeId, s: &DisplaySettings, stored: &[NodeAnalysisView]) -> Vec<(String, MoveRank, bool)> {
    let path = rec.path_to(at);
    let numbering = display::numbering(rec, &path);
    display::rank_marks(rec, &path, &numbering, s, |n| stored.iter().find(|a| a.node == n))
        .into_iter()
        .map(|m| (sgf_point(m.point), m.rank, m.last))
        .collect()
}

#[test]
fn move_ranks_follow_the_loss_of_each_move_back_to_the_limit() {
    let rec = sgf::read("(;SZ[9];B[ee];W[gc];B[cg])").unwrap();
    let [ee, gc, cg] = [child(&rec, &[0]), child(&rec, &[0, 0]), child(&rec, &[0, 0, 0])];
    let stored = vec![
        analysis(ROOT, Stone::Black, vec![candidate("ee", 0, 100, 50.0, Some(0.0))]),
        analysis(ee, Stone::White, vec![candidate("dd", 0, 100, 50.0, Some(0.0))]),
        // After gc Black leads by 3 points with 53%: White lost 3% and 3 points.
        analysis(gc, Stone::Black, vec![candidate("cg", 0, 100, 53.0, Some(3.0))]),
        analysis(cg, Stone::White, vec![candidate("dd", 0, 100, 47.0, Some(-3.0))]),
    ];
    let s = DisplaySettings { move_rank: 0, ..DisplaySettings::default() };
    assert_eq!(
        ranks(&rec, cg, &s, &stored),
        [
            ("cg".into(), MoveRank::Best, true),
            ("gc".into(), MoveRank::Inaccuracy, false),
            ("ee".into(), MoveRank::Best, false),
        ]
    );
    let by_winrate = DisplaySettings { move_rank_mode: MoveRankMode::Winrate, ..s.clone() };
    assert_eq!(ranks(&rec, cg, &by_winrate, &stored)[1].1, MoveRank::Normal);
    let last_only = DisplaySettings { move_rank: 1, ..s.clone() };
    assert_eq!(ranks(&rec, cg, &last_only, &stored), [("cg".into(), MoveRank::Best, true)]);
    // A move is ranked only with the analysis of both positions.
    let without_gc: Vec<_> = stored.iter().filter(|a| a.node != gc).cloned().collect();
    assert_eq!(ranks(&rec, cg, &s, &without_gc), [("ee".into(), MoveRank::Best, false)]);
    // Move numbers on: no marks.
    let numbered = DisplaySettings { move_numbers: -1, ..s };
    assert!(ranks(&rec, cg, &numbered, &stored).is_empty());
}

#[test]
fn move_rank_modes_choose_which_loss_counts() {
    let s = DisplaySettings::default();
    let with = |mode| DisplaySettings { move_rank_mode: mode, ..s.clone() };
    // Auto with a score: the score decides, the win rate only from 恶手 on.
    assert_eq!(display::classify(&s, -5.0, Some(-0.2)), MoveRank::Best);
    assert_eq!(display::classify(&s, -30.0, Some(-0.2)), MoveRank::Blunder);
    // Auto without a score (Leela): the win rate.
    assert_eq!(display::classify(&s, -5.0, None), MoveRank::Normal);
    assert_eq!(display::classify(&with(MoveRankMode::Score), -30.0, Some(-0.2)), MoveRank::Best);
    assert_eq!(display::classify(&with(MoveRankMode::Winrate), -0.5, Some(-7.0)), MoveRank::Best);
    assert_eq!(display::classify(&with(MoveRankMode::Combined), -0.5, Some(-7.0)), MoveRank::Mistake);
}

#[test]
fn next_move_information_compares_the_record_move_with_the_best_candidate() {
    let rec = sgf::read("(;SZ[9];B[ee])").unwrap();
    let ee = child(&rec, &[0]);
    let s = DisplaySettings::default();
    let info = |ee_visits: Option<u64>, reply_winrate: f64, reply_score: f64| {
        let mut list = vec![candidate("dd", 0, 200, 55.0, Some(2.0))];
        if let Some(v) = ee_visits {
            list.push(candidate("ee", 1, v, 50.0, Some(1.0)));
        }
        let stored = [
            analysis(ROOT, Stone::Black, list),
            analysis(ee, Stone::White, vec![candidate("cc", 0, 100, reply_winrate, Some(reply_score))]),
        ];
        display::next_move_info(&rec, ROOT, &s, false, 0, |n| stored.iter().find(|a| a.node == n))
    };
    let good = info(None, 46.0, -2.0).unwrap();
    assert_eq!((good.point, good.verdict, good.hides_candidate), (p("ee"), NextMoveVerdict::Good, false));
    assert_eq!(good.winrate, 54.0, "the mover's result");
    assert_eq!(info(None, 50.0, -1.5).unwrap().verdict, NextMoveVerdict::Questionable);
    assert_eq!(info(None, 60.0, -2.0).unwrap().verdict, NextMoveVerdict::Bad);
    // A candidate searched less than the move afterwards is replaced by the information.
    assert!(info(Some(10), 46.0, -2.0).unwrap().hides_candidate);
    // A candidate searched enough is drawn instead.
    assert!(info(Some(40), 46.0, -2.0).is_none());
    let off = DisplaySettings { next_move_info: false, ..s.clone() };
    let stored = [
        analysis(ROOT, Stone::Black, vec![candidate("dd", 0, 200, 55.0, Some(2.0))]),
        analysis(ee, Stone::White, vec![candidate("cc", 0, 100, 46.0, Some(-2.0))]),
    ];
    assert!(display::next_move_info(&rec, ROOT, &off, false, 0, |n| stored.iter().find(|a| a.node == n)).is_none());
}

#[test]
fn tree_numbers_follow_branch_restarts_mn_and_passes() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    open(&mut wb, "(;SZ[9];B[aa];W[bb](;B[cc])(;B[dd]MN[8];W[];B[ee]))");
    let rec = &wb.document().record;
    let ids = [ROOT, child(rec, &[0]), child(rec, &[0, 0]), child(rec, &[0, 0, 0]),
        child(rec, &[0, 0, 1]), child(rec, &[0, 0, 1, 0]), child(rec, &[0, 0, 1, 0, 0])];
    let tree = wb.view(true).tree.unwrap();
    for (id, number) in ids.into_iter().zip([0, 1, 2, 3, 8, 9, 10]) {
        assert_eq!(tree.nodes.iter().find(|n| n.id == id).unwrap().number, number);
    }
    open(&mut wb, "(;SZ[9];B[aa];W[bb](;B[cc])(;B[dd];W[];B[ee]))");
    let rec = &wb.document().record;
    let ids = [child(rec, &[0, 0, 1]), child(rec, &[0, 0, 1, 0]), child(rec, &[0, 0, 1, 0, 0])];
    let tree = wb.view(true).tree.unwrap();
    for (id, number) in ids.into_iter().zip([1, 2, 3]) {
        assert_eq!(tree.nodes.iter().find(|n| n.id == id).unwrap().number, number);
    }
    display(&mut wb, |s| s.branch_numbers_from_one = false);
    open(&mut wb, "(;SZ[9];B[aa];W[bb](;B[cc])(;B[dd];W[];B[ee]))");
    let rec = &wb.document().record;
    let id = child(rec, &[0, 0, 1, 0, 0]);
    assert_eq!(wb.view(true).tree.unwrap().nodes.iter().find(|n| n.id == id).unwrap().number, 5);
    // The main trunk shows the real move numbers, an SGF `MN` notwithstanding (lane 0).
    open(&mut wb, "(;SZ[9];B[aa];W[bb]MN[10];B[cc])");
    let rec = &wb.document().record;
    let ids = [child(rec, &[0]), child(rec, &[0, 0]), child(rec, &[0, 0, 0])];
    let tree = wb.view(true).tree.unwrap();
    for (id, number) in ids.into_iter().zip([1, 2, 3]) {
        assert_eq!(tree.nodes.iter().find(|n| n.id == id).unwrap().number, number);
    }
}

#[test]
fn next_move_hover_includes_the_played_move_then_the_reply_pv_and_captures() {
    let rec = sgf::read("(;SZ[9]PL[B]AB[ba]AW[aa];B[ab])").unwrap();
    let next = child(&rec, &[0]);
    let mut reply = candidate("cc", 0, 100, 45.0, Some(-2.0));
    reply.pv = vec![Some(p("cc")), None, Some(p("dd"))];
    let stored = [
        analysis(ROOT, Stone::Black, vec![candidate("ee", 0, 100, 55.0, Some(2.0))]),
        analysis(next, Stone::White, vec![reply]),
    ];
    let s = DisplaySettings::default();
    let info = |s: &DisplaySettings, no_capture, limit| {
        display::next_move_info(&rec, ROOT, s, no_capture, limit, |n| stored.iter().find(|a| a.node == n)).unwrap()
    };
    let preview = info(&s, false, 0);
    assert_eq!(preview.pv, [Some(p("ab")), Some(p("cc")), None, Some(p("dd"))]);
    assert_eq!(preview.pv_captures, [vec![p("aa")], vec![], vec![], vec![]]);
    assert_eq!(info(&s, false, 2).pv_captures, [vec![p("aa")], vec![]]);
    assert!(info(&s, true, 0).pv_captures.is_empty());
}

#[test]
fn negative_next_move_thresholds_are_signed_and_persisted() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    display(&mut wb, |s| s.next_move_min_playouts = i32::MIN);
    assert_eq!(workbench(dir.path()).config().display.next_move_min_playouts, i32::MIN);
    let rec = sgf::read("(;SZ[9];B[ee])").unwrap();
    let next = child(&rec, &[0]);
    let stored = [
        analysis(ROOT, Stone::Black, vec![candidate("dd", 0, 100, 55.0, None), candidate("ee", 1, 0, 50.0, None)]),
        analysis(next, Stone::White, vec![candidate("cc", 0, u64::MAX - 100, 45.0, None)]),
    ];
    assert!(display::next_move_info(&rec, ROOT, &wb.config().display, false, 0,
        |n| stored.iter().find(|a| a.node == n)).is_none(), "zero visits already exceeds a negative threshold");
}

#[test]
fn new_display_settings_validate_and_old_configs_receive_java_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    for settings in [
        DisplaySettings { suggestion_color_ratio: 0, ..Default::default() },
        DisplaySettings { suggestion_color_ratio: 4, ..Default::default() },
        DisplaySettings { entire_variation_first_seconds: f64::NAN, ..Default::default() },
        DisplaySettings { autoplay_main_seconds: f64::INFINITY, ..Default::default() },
        DisplaySettings { autoplay_main_seconds: 0.0, ..Default::default() },
        DisplaySettings { autoplay_sub_millis: 0, ..Default::default() },
    ] {
        assert!(wb.set_display_settings(settings).is_err());
    }
    let settings = DisplaySettings { autoplay_main_seconds: 1.25, autoplay_sub_millis: 300,
        entire_variation_first_seconds: -1.0, pv_visits_limit: -5, ..Default::default() };
    wb.set_display_settings(settings.clone()).unwrap();
    assert_eq!(workbench(dir.path()).config().display, settings);
    std::fs::write(storage(dir.path()).config_file, r#"{"display":{"coordinates":false}}"#).unwrap();
    assert_eq!(workbench(dir.path()).config().display, DisplaySettings { coordinates: false, ..Default::default() });
}

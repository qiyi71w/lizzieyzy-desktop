//! 超级鹰眼 (fixed Java `MoveListFrame`) through the public `Workbench` operations on records with
//! saved analysis: which moves it lists and counts (`NodeInfo`: a move is evaluated on the
//! position before it, a first-candidate move loses nothing), the 统计 range and 分支, row and
//! chart navigation with the marked move, the secondary engine's window and what is remembered.

use lizzie_core::api::{HawkeyeCommand, HawkeyePane, HawkeyeView, MoveEvaluation};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::record::{NodeId, ROOT};
use lizzie_core::settings::{ExtraMode, HawkeyeBranch, HawkeyePeriod, HawkeyeSettings, Storage};
use lizzie_core::workbench::{OpenOrigin, Workbench};

/// Nothing is analysed live here; the records carry their analysis.
struct NoEngines;

impl Launcher for NoEngines {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        Err(std::io::Error::other("no engines in hawkeye tests"))
    }
}

fn workbench(dir: &std::path::Path) -> Workbench {
    Workbench::new(
        Storage::in_dirs(&dir.join("config"), &dir.join("data")),
        Some("zh-CN"),
        Box::new(NoEngines),
        Vec::new(),
    )
}

fn open(wb: &mut Workbench, text: &str) {
    let ticket = wb.begin_open(OpenOrigin::Clipboard);
    wb.complete_open(ticket, Workbench::read_clipboard_record(text, false));
}

fn lz(tag: &str, winrate: f64, visits: u32, score: f64, best: &str) -> String {
    format!(
        "{tag}[b11 {winrate:.1} {visits} {score} 0.5 0 rootVisits={visits}\nmove {best} visits {visits} winrate {} scoreMean {score} pv {best}]",
        (winrate * 100.0).round()
    )
}

/// Moves 1-5 on the main line: E5 (the best), G3, C7 (D4 was best), D6 (whose position is not analysed) and
/// F6 (played from that unanalysed position), with a side line from move 2.
fn record() -> String {
    format!(
        "(;SZ[9]KM[7.5]{}\
         ;B[ee]{}\
         (;W[gg]{}\
          ;B[cc]{}\
          ;W[dd]\
          ;B[ff]{})\
         (;W[cg]{}))",
        lz("LZOP", 55.0, 100, 1.5, "E5"),
        lz("LZ", 45.0, 200, -1.0, "C3"),
        lz("LZ", 60.0, 300, 3.0, "D4"),
        lz("LZ", 35.0, 400, -4.0, "D4"),
        lz("LZ", 50.0, 500, 0.0, "G2"),
        lz("LZ", 52.0, 300, 0.5, "C7"),
    )
}

fn view(wb: &mut Workbench) -> HawkeyeView {
    wb.view(false).hawkeye.expect("the window is open")
}

fn main_line(wb: &Workbench) -> Vec<NodeId> {
    let rec = &wb.document().record;
    let mut line = vec![ROOT];
    while let Some(next) = rec.main_child(*line.last().unwrap()) {
        line.push(next);
    }
    line
}

fn evaluation(wb: &mut Workbench, node: NodeId) -> MoveEvaluation {
    view(wb).primary.moves.into_iter().find(|e| e.node == node).expect("listed")
}

fn opened(dir: &std::path::Path) -> Workbench {
    let mut wb = workbench(dir);
    open(&mut wb, &record());
    assert_eq!(wb.view(false).hawkeye, None, "closed by default (`show-badmoves-frame` false)");
    assert_eq!(wb.hawkeye(HawkeyeCommand::Toggle), Ok(true));
    wb
}

/// Java keeps move N's numbers on node N - 1: a move needs its position before analysed to be
/// counted at all (`analyzedMatchValue`), and both positions for its losses (`analyzed`).
#[test]
fn moves_are_listed_and_counted_like_java_node_info() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path());
    let line = main_line(&wb);
    let pane = view(&mut wb).primary;
    assert!(pane.katago, "the saved analysis has score leads");
    assert_eq!(pane.total_moves, 5);
    assert_eq!(pane.off_trunk_from, None);
    let listed: Vec<NodeId> = pane.moves.iter().map(|e| e.node).collect();
    // Move 5 is played from the unanalysed position after move 4.
    assert_eq!(listed, line[1..5].to_vec());

    let [b1, w2, b3, w4] = [1, 2, 3, 4].map(|i| evaluation(&mut wb, line[i]));
    assert!(b1.best && b1.analyzed && !w2.best && w2.analyzed && b3.analyzed);
    assert!(!w4.analyzed, "move 4's own position is not analysed");
    assert_eq!(w4.winrate_change, None, "an unknown change is not 0");

    // 简易列表: both positions analysed, thresholds 0; move 4 is not listed.
    assert_eq!(pane.rows, vec![0, 1, 2]);
    // 详细列表: the 黑 / 白 boxes start checked.
    assert_eq!(pane.detail_rows, pane.rows);

    // keyPanel: match numbers over moves with the position before analysed, losses over moves
    // with both; the first-candidate move 1 loses nothing whatever its raw change.
    let [black, white] = pane.sides;
    assert_eq!((black.match_analyzed, black.analyzed, black.best), (2, 2, 1));
    assert_eq!((white.match_analyzed, white.analyzed, white.best), (2, 1, 0));
    let loss = |e: &MoveEvaluation| (-e.winrate_change.unwrap()).max(0.0);
    let score_loss = |e: &MoveEvaluation| (-e.score_change.unwrap()).max(0.0);
    assert!((black.winrate_loss_sum - loss(&b3)).abs() < 1e-9, "{black:?} {b3:?}");
    assert!((white.winrate_loss_sum - loss(&w2)).abs() < 1e-9);
    assert!((black.score_loss_sum - score_loss(&b3)).abs() < 1e-9);
    assert!((black.share_sum - b1.share - b3.share).abs() < 1e-9);
    assert_eq!(black.matched, u32::from(b1.match_ai) + u32::from(b3.match_ai));

    // 损失分布 buckets and 分布 spreads count every analysed move of the range.
    assert_eq!(pane.winrate_buckets[0].iter().sum::<u32>(), 2);
    assert_eq!(pane.winrate_buckets[1].iter().sum::<u32>(), 1);
    assert_eq!(pane.winrate_buckets[0][0], 1, "the best move is in the first bucket");
    assert_eq!((pane.winrate_spread[0].analyzed, pane.score_spread[1].analyzed), (2, 1));
    // 分段吻合度: all in 布局 (moves up to 60).
    assert_eq!((pane.stages[0][0].match_analyzed, pane.stages[0][0].analyzed), (2, 2));
    assert_eq!(pane.stages[1][1], Default::default());
    // 吻合度走势: running sums before each listed move and after the last.
    assert_eq!(pane.running.len(), pane.moves.len() + 1);
    assert_eq!(pane.running[2][0].count, 1);
    // 翻车现场: analysed moves after move 1, worst first per side.
    let picked: Vec<u32> = pane.winrate_mistakes.iter().map(|m| m.index).collect();
    assert!(picked.contains(&1) && picked.contains(&2) && !picked.contains(&0), "{picked:?}");
}

/// A move without a score lead changes it by 0 in Java's statistics (`NodeInfo.scoreMeanDiff`),
/// and counts in the denominators like every analysed move.
#[test]
fn a_move_without_a_score_lead_counts_as_no_score_change() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    open(
        &mut wb,
        "(;SZ[9]KM[7.5]LZOP[LeelaZero 55.0 100\nmove E5 visits 100 winrate 5500 pv E5]\
         ;B[cc]LZ[LeelaZero 40.0 200\nmove C3 visits 200 winrate 6000 pv C3])",
    );
    wb.hawkeye(HawkeyeCommand::Toggle).unwrap();
    let pane = view(&mut wb).primary;
    assert!(!pane.katago);
    let black = pane.sides[0];
    assert_eq!((black.analyzed, black.score_loss_sum), (1, 0.0));
    assert!(black.winrate_loss_sum > 0.0);
    assert_eq!(pane.score_buckets[0][0], 1);
}

/// The 统计 periods choose moves by the number of the position before them; 当前 follows the
/// browsing position; thresholds filter the lists only.
#[test]
fn periods_and_thresholds_filter_like_java() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path());
    let custom = |start, end| HawkeyeSettings {
        period: HawkeyePeriod::Custom,
        custom_start: start,
        custom_end: end,
        ..wb_settings()
    };
    // Moves 2-3: positions 1 and 2 (`moveNumber + 1 <= last && moveNumber >= first`).
    assert_eq!(wb.set_hawkeye_settings(custom(2, 3)), Ok(true));
    let v = view(&mut wb);
    assert_eq!((v.first_move, v.last_move), (1, 3));
    let [black, white] = v.primary.sides;
    assert_eq!((black.match_analyzed, white.match_analyzed), (1, 1));

    // 当前 ends at the browsing position (the root: nothing).
    wb.set_hawkeye_settings(HawkeyeSettings { period: HawkeyePeriod::Current, ..wb_settings() })
        .unwrap();
    assert_eq!(view(&mut wb).primary.sides[0].match_analyzed, 0);
    assert_eq!(view(&mut wb).primary.rows.len(), 3, "the lists ignore the period");

    // 列表筛选: a win-rate threshold above every change empties the lists, not the statistics.
    wb.set_hawkeye_settings(HawkeyeSettings { winrate_threshold: 99, ..wb_settings() }).unwrap();
    let pane = view(&mut wb).primary;
    assert!(pane.rows.is_empty());
    assert_eq!(pane.sides[0].match_analyzed, 2);

    // 黑 / 白 boxes narrow the 详细列表 only.
    wb.set_hawkeye_settings(HawkeyeSettings { winrate_threshold: 0, ..wb_settings() }).unwrap();
    assert_eq!(wb.hawkeye(HawkeyeCommand::SetSides { black: false, white: true }), Ok(true));
    let pane = view(&mut wb).primary;
    assert_eq!((pane.rows.len(), pane.detail_rows.clone()), (3, vec![1]));

    // Out-of-range settings are refused.
    assert!(wb.set_hawkeye_settings(HawkeyeSettings { chart: 9, ..wb_settings() }).is_err());

    fn wb_settings() -> HawkeyeSettings {
        HawkeyeSettings { show_frame: true, ..HawkeyeSettings::default() }
    }
}

/// A row goes to the position before its move and marks the move; 主干 lists the main trunk
/// even while a side line is browsed, and only its moves can be chosen. Chart clicks go to a
/// move number, 0 (left of the chart) to the root.
#[test]
fn rows_and_charts_navigate_and_mark_like_java() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path());
    let line = main_line(&wb);
    assert_eq!(wb.hawkeye(HawkeyeCommand::GotoMove { node: line[3] }), Ok(true));
    let v = view(&mut wb);
    assert_eq!(v.current_move, 2);
    assert!(v.mark.is_some());
    assert_eq!(wb.hawkeye(HawkeyeCommand::GotoMove { node: line[3] }), Ok(false), "already there");

    // Browse the side line: 主干 still reads the trunk; its move cannot be chosen.
    let side = *wb.document().record.node(line[1]).children.last().unwrap();
    assert_eq!(wb.hawkeye(HawkeyeCommand::GotoMove { node: side }).map_err(|_| ()), Err(()));
    wb.hawkeye(HawkeyeCommand::GotoMoveNumber { number: 1 }).unwrap();
    assert_eq!(view(&mut wb).current_move, 1);
    wb.set_hawkeye_settings(HawkeyeSettings { show_frame: true, branch: HawkeyeBranch::Current, ..Default::default() })
        .unwrap();
    assert_eq!(wb.hawkeye(HawkeyeCommand::GotoMove { node: side }), Ok(true));
    let v = view(&mut wb);
    assert_eq!(v.primary.total_moves, 5, "the current line is the trunk at move 1");

    // Chart: move 4, then 0 goes back to the root.
    wb.hawkeye(HawkeyeCommand::GotoMoveNumber { number: 4 }).unwrap();
    assert_eq!(view(&mut wb).current_move, 4);
    assert_eq!(wb.hawkeye(HawkeyeCommand::GotoMoveNumber { number: 0 }), Ok(true));
    assert_eq!(view(&mut wb).current_move, 0);

    // Closing clears the mark (`toggleBadMoves`).
    wb.hawkeye(HawkeyeCommand::GotoMove { node: line[2] }).unwrap();
    wb.hawkeye(HawkeyeCommand::Toggle).unwrap();
    wb.hawkeye(HawkeyeCommand::Toggle).unwrap();
    assert_eq!(view(&mut wb).mark, None);
}

/// 当前 on a side line reads that line; its moves off the trunk dash the score line.
#[test]
fn the_current_branch_reads_the_browsed_line() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = opened(dir.path());
    let line = main_line(&wb);
    wb.set_hawkeye_settings(HawkeyeSettings { show_frame: true, branch: HawkeyeBranch::Current, ..Default::default() })
        .unwrap();
    let side = *wb.document().record.node(line[1]).children.last().unwrap();
    wb.hawkeye(HawkeyeCommand::GotoMove { node: side }).unwrap();
    // The side move's own position: browse it.
    wb.goto_node(side).unwrap();
    let pane: HawkeyePane = view(&mut wb).primary;
    assert_eq!(pane.total_moves, 2);
    assert_eq!(pane.off_trunk_from, Some(2));
    assert_eq!(pane.moves.iter().map(|e| e.node).collect::<Vec<_>>(), vec![line[1], side]);
}

/// 双引擎模式 opens a second window on the secondary engine's analysis.
#[test]
fn the_secondary_engine_has_its_own_window() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    open(
        &mut wb,
        &format!(
            "(;SZ[9]KM[7.5]{};B[ee]{}{};W[gg]{})",
            lz("LZOP", 55.0, 100, 1.5, "E5"),
            lz("LZ", 45.0, 200, -1.0, "C3"),
            lz("LZ2", 50.0, 200, 0.5, "C3"),
            lz("LZ", 60.0, 300, 3.0, "C7"),
        ),
    );
    wb.hawkeye(HawkeyeCommand::Toggle).unwrap();
    assert_eq!(view(&mut wb).secondary, None, "outside 双引擎模式");
    wb.set_extra_mode(ExtraMode::DoubleEngine).unwrap();
    let v = view(&mut wb);
    let secondary = v.secondary.expect("双引擎模式");
    assert_eq!(v.primary.moves.len(), 2);
    // The secondary engine analysed only the position after move 1: move 2 is counted on it.
    assert_eq!(secondary.moves.len(), 1);
    assert!(!secondary.moves[0].analyzed);
}

/// The open state and choices are remembered across restarts; the side boxes are not.
#[test]
fn settings_are_remembered() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut wb = opened(dir.path());
        wb.set_hawkeye_settings(HawkeyeSettings {
            show_frame: false,
            always_on_top: true,
            top_page: 2,
            chart: 7,
            winrate_threshold: 5,
            period: HawkeyePeriod::Middle,
            ..Default::default()
        })
        .unwrap();
        wb.hawkeye(HawkeyeCommand::SetSides { black: false, white: true }).unwrap();
    }
    let mut wb = workbench(dir.path());
    let s = wb.view(false).settings.hawkeye;
    assert!(s.show_frame, "SetHawkeyeSettings keeps the open state");
    assert!(s.always_on_top);
    assert_eq!((s.top_page, s.chart, s.winrate_threshold, s.period), (2, 7, 5, HawkeyePeriod::Middle));
    open(&mut wb, &record());
    let v = view(&mut wb);
    assert!(v.black && v.white, "the boxes start checked");
    wb.hawkeye(HawkeyeCommand::Toggle).unwrap();
    assert!(!workbench(dir.path()).view(false).settings.hawkeye.show_frame);
}

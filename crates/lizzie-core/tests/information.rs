//! Analysis information beside the candidates through the public `Workbench` operations with a
//! controlled KataGo: Kata形势 (ownership requests, projection, menu state and what is
//! remembered), each move's evaluation and AI总评分, and 终局点目.

use std::sync::{Arc, Mutex};

use lizzie_core::api::{KataEstimateMode, ScoreCommand};
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats::sgf;
use lizzie_core::record::{Point, RecordFormat, Stone};
use lizzie_core::settings::{DisplaySettings, KataEstimateState, Storage};
use lizzie_core::workbench::{OpenOrigin, Workbench};

/// Commands each instance was sent and not yet answered.
type Sent = Vec<(InstanceId, Vec<String>)>;

#[derive(Clone, Default)]
struct World(Arc<Mutex<Sent>>);

impl World {
    fn take(&self, id: InstanceId) -> Vec<String> {
        let mut w = self.0.lock().unwrap();
        w.iter_mut()
            .find(|p| p.0 == id)
            .map(|p| std::mem::take(&mut p.1))
            .unwrap_or_default()
    }
}

struct FakeIo(InstanceId, World);

impl EngineIo for FakeIo {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        let mut w = (self.1).0.lock().unwrap();
        w.iter_mut().find(|p| p.0 == self.0).unwrap().1.push(line.to_string());
        Ok(())
    }
    fn terminate(self: Box<Self>) {}
}

struct FakeLauncher(World);

impl Launcher for FakeLauncher {
    fn launch(&self, id: InstanceId, _spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        self.0 .0.lock().unwrap().push((id, Vec::new()));
        Ok(Box::new(FakeIo(id, self.0.clone())))
    }
}

const KATA: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nset_position\nkata-analyze\nkata-set-rules\nkata-get-rules\nstop";

fn out(wb: &mut Workbench, id: InstanceId, line: &str) {
    wb.engine_input(id, EngineInput::Stdout(line.to_string()));
}

/// Answers every outstanding command; analysis requests get only their header. Returns them.
fn answer_all(wb: &mut Workbench, world: &World, id: InstanceId) -> Vec<String> {
    let mut done = Vec::new();
    loop {
        let pending = world.take(id);
        if pending.is_empty() {
            return done;
        }
        for line in pending {
            out(wb, id, "");
            let (n, cmd) = line.split_once(' ').unwrap();
            let reply = match cmd.split(' ').next().unwrap() {
                "name" => Some("KataGo"),
                "version" => Some("1.16.4"),
                "list_commands" => Some(KATA),
                "kata-get-rules" => Some(
                    r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#,
                ),
                "kata-analyze" => None,
                _ => Some(""),
            };
            match reply {
                Some(text) => {
                    out(wb, id, &format!("={n} {text}"));
                    out(wb, id, "");
                }
                None => out(wb, id, &format!("={n}")),
            }
            done.push(cmd.to_string());
        }
    }
}

fn storage(dir: &std::path::Path) -> Storage {
    Storage::in_dirs(&dir.join("config"), &dir.join("data"))
}

fn started(dir: &std::path::Path, world: &World) -> (Workbench, InstanceId) {
    let mut wb = Workbench::new(storage(dir), None, Box::new(FakeLauncher(world.clone())), Vec::new());
    let profile = EngineProfile {
        name: "KataGo".into(),
        command: "/opt/katago gtp".into(),
        ..EngineProfile::default()
    };
    wb.save_engine_settings(vec![profile], EngineStartup::Default).unwrap();
    wb.start_engines();
    let id = world.0.lock().unwrap()[0].0;
    answer_all(&mut wb, world, id);
    (wb, id)
}

fn open(wb: &mut Workbench, text: &str) {
    let t = wb.begin_open(OpenOrigin::File {
        path: "/x.sgf".into(),
        source: lizzie_core::api::OpenSource::Drop,
    });
    assert!(wb.complete_open(t, Ok((RecordFormat::Sgf, sgf::read(text).unwrap()))));
}

fn numbers(values: &[f64]) -> String {
    values.iter().map(|v| v.to_string()).collect::<Vec<_>>().join(" ")
}

fn analyse(wb: &mut Workbench, world: &World, id: InstanceId, line: &str) -> Vec<String> {
    let sent = answer_all(wb, world, id);
    out(wb, id, line);
    sent
}

/// Kata形势 asks KataGo for the root and per-move ownership; the view gives Black-positive
/// values for the current node, else its parent's, and a candidate's own only while
/// 按选点显示评估 is on. Turned off, nothing is requested or shown.
#[test]
fn kata_estimate_requests_and_shows_black_positive_ownership() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    open(&mut wb, "(;SZ[5]KM[7.5];B[cc];W[dd])");
    wb.navigate(Navigation::Forward(1));
    let sent = answer_all(&mut wb, &world, id);
    assert!(
        sent.iter()
            .any(|c| c.starts_with("kata-analyze") && !c.contains("ownership")),
        "{sent:?}"
    );

    assert!(wb.toggle_kata_estimate());
    let state = wb.view(false).kata_estimate.state;
    assert_eq!(
        state,
        KataEstimateState {
            shown: true,
            on_main: true,
            on_sub: true
        }
    );
    let sent = answer_all(&mut wb, &world, id);
    let analyze = sent
        .iter()
        .find(|c| c.starts_with("kata-analyze"))
        .expect("analysis requested again");
    assert!(
        analyze.contains(" ownership true") && analyze.contains(" movesOwnership true"),
        "{analyze}"
    );

    // White to move: KataGo reports ownership for White.
    let own: Vec<f64> = (0..25).map(|k| if k < 10 { 0.8 } else { -0.25 }).collect();
    let moves: Vec<f64> = (0..25).map(|k| if k == 0 { 1.0 } else { 0.0 }).collect();
    out(
        &mut wb,
        id,
        &format!(
            "info move B2 visits 30 winrate 0.6 scoreLead 2 prior 0.2 order 0 pv B2 movesOwnership {} ownership {}",
            numbers(&moves),
            numbers(&own)
        ),
    );
    let v = wb.view(false);
    let shown = v.kata_estimate.ownership.expect("current node's ownership");
    assert_eq!(shown[0], -0.8);
    assert_eq!(shown[24], 0.25);
    assert_eq!(v.analysis.unwrap().candidates[0].ownership.as_ref().unwrap()[0], -1.0);

    // A node without its own estimate shows its parent's.
    wb.navigate(Navigation::Forward(1));
    assert_eq!(wb.view(false).kata_estimate.ownership.unwrap()[0], -0.8);

    // 按选点显示评估 off: no candidate ownership, the stream asks for root ownership only.
    wb.navigate(Navigation::Back(1));
    answer_all(&mut wb, &world, id);
    let display = DisplaySettings {
        use_moves_ownership: false,
        ..wb.config().display.clone()
    };
    wb.set_display_settings(display).unwrap();
    assert!(wb.view(false).analysis.unwrap().candidates[0].ownership.is_none());
    let sent = answer_all(&mut wb, &world, id);
    let analyze = sent
        .iter()
        .find(|c| c.starts_with("kata-analyze"))
        .expect("restreamed with the new options");
    assert!(
        analyze.contains(" ownership true") && !analyze.contains("movesOwnership"),
        "{analyze}"
    );

    wb.set_kata_estimate(KataEstimateMode::Close).unwrap();
    assert!(wb.view(false).kata_estimate.ownership.is_none());
    let sent = answer_all(&mut wb, &world, id);
    assert!(
        sent.iter()
            .any(|c| c.starts_with("kata-analyze") && !c.contains("ownership")),
        "{sent:?}"
    );
}

/// The menu choices act at once; only 关闭 is always remembered, the boards only with
/// 记忆评估显示状态 (whose change writes the state, or clears it). A start with the estimate
/// on and no board begins hidden, and `.` then toggles the boards instead of the estimate.
#[test]
fn kata_estimate_menu_state_is_remembered_like_java() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, _) = started(dir.path(), &world);
    let remembered = |wb: &Workbench| wb.config().display.kata_estimate;

    wb.set_kata_estimate(KataEstimateMode::Main).unwrap();
    assert_eq!(
        wb.view(false).kata_estimate.state,
        KataEstimateState {
            shown: true,
            on_main: true,
            on_sub: false
        }
    );
    assert_eq!(
        remembered(&wb),
        KataEstimateState::default(),
        "not remembered without 记忆评估显示状态"
    );

    let save = DisplaySettings {
        kata_estimate_save_status: true,
        ..wb.config().display.clone()
    };
    wb.set_display_settings(save).unwrap();
    assert_eq!(
        remembered(&wb),
        KataEstimateState {
            shown: true,
            on_main: true,
            on_sub: false
        }
    );

    wb.set_kata_estimate(KataEstimateMode::Hide).unwrap();
    let v = wb.view(false).kata_estimate;
    assert!(v.hidden);
    assert_eq!(
        remembered(&wb),
        KataEstimateState {
            shown: true,
            on_main: false,
            on_sub: false
        }
    );
    drop(wb);

    let (mut wb, _) = started(dir.path(), &World::default());
    let v = wb.view(false).kata_estimate;
    assert!(
        v.hidden && v.state.shown,
        "a remembered estimate on no board starts hidden"
    );
    assert!(wb.toggle_kata_estimate());
    let v = wb.view(false).kata_estimate;
    assert_eq!(
        (v.state, v.hidden),
        (
            KataEstimateState {
                shown: true,
                on_main: true,
                on_sub: true
            },
            true
        )
    );
    assert!(wb.toggle_kata_estimate());
    assert_eq!(
        wb.view(false).kata_estimate.state,
        KataEstimateState {
            shown: true,
            on_main: false,
            on_sub: false
        }
    );

    // Turning 记忆评估显示状态 off clears what is remembered; 关闭 remembers off.
    let off = DisplaySettings {
        kata_estimate_save_status: false,
        ..wb.config().display.clone()
    };
    wb.set_display_settings(off).unwrap();
    assert_eq!(
        remembered(&wb),
        KataEstimateState {
            shown: false,
            on_main: false,
            on_sub: false
        }
    );
    wb.set_kata_estimate(KataEstimateMode::Both).unwrap();
    wb.set_kata_estimate(KataEstimateMode::Close).unwrap();
    assert!(!remembered(&wb).shown);
    assert!(!wb.view(false).kata_estimate.state.shown);
}

/// `.` and the bottom-bar toggle need a KataGo foreground engine.
#[test]
fn kata_estimate_toggle_needs_katago() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = Workbench::new(storage(dir.path()), None, Box::new(FakeLauncher(world)), Vec::new());
    assert!(!wb.toggle_kata_estimate());
    assert!(!wb.view(false).kata_estimate.state.shown);
}

/// 记录选点信息到评论中: while an engine analyses, the comment pane shows the move's comment
/// with its analysis summary, before its own result the previous move's; turned off, or for
/// the root, the comment alone. The document is not changed by showing it.
#[test]
fn the_comment_pane_shows_the_analysis_summary_like_java() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    open(&mut wb, "(;SZ[19]KM[7.5]C[root];B[pd]C[first];W[dp])");
    analyse(
        &mut wb,
        &world,
        id,
        "info move D4 visits 30 winrate 0.55 scoreLead 2 prior 0.2 order 0 pv D4",
    );
    assert_eq!(wb.view(false).comment_with_analysis, None, "not for the root");
    wb.navigate(Navigation::Forward(1));
    analyse(
        &mut wb,
        &world,
        id,
        "info move D4 visits 40 winrate 0.52 scoreLead 0.5 prior 0.2 order 0 pv D4",
    );
    let shown = wb.view(false).comment_with_analysis.expect("summary while analysing");
    assert!(shown.starts_with("first\n") && shown.contains("KataGo / 40"), "{shown}");
    // Move 2 has no comment and no result yet: the previous move's is shown meanwhile.
    wb.navigate(Navigation::Forward(1));
    answer_all(&mut wb, &world, id);
    assert_eq!(wb.view(false).comment_with_analysis.as_deref(), Some(shown.as_str()));
    let revision = wb.document().revision;
    let node = wb.view(false).position.node;
    assert_eq!(wb.document().record.node(node).comment, "");
    wb.set_append_winrate_to_comment(false).unwrap();
    assert_eq!(wb.view(false).comment_with_analysis, None);
    assert_eq!(wb.document().revision, revision);
}

/// Each move is evaluated against the analysis of the position it was played in (match rank,
/// share, AI match, the mover's win-rate and score change), and AI总评分 averages the shares of
/// each side's evaluated moves up to here and the move played from here.
#[test]
fn moves_are_evaluated_against_the_analysis_before_them() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    open(&mut wb, "(;SZ[19]KM[7.5];B[pd];W[dp];B[pp])");
    // Root: Black to move; Q16 (pd) is the second candidate with half the most visits.
    analyse(&mut wb, &world, id, "info move D4 visits 100 winrate 0.55 scoreLead 2 prior 0.2 order 0 pv D4 info move Q16 visits 50 winrate 0.5 scoreLead 1 prior 0.1 order 1 pv Q16");
    wb.navigate(Navigation::Forward(1));
    // After B Q16, White to move: 52 % and +0.5 for White.
    analyse(
        &mut wb,
        &world,
        id,
        "info move D4 visits 80 winrate 0.52 scoreLead 0.5 prior 0.2 order 0 pv D4",
    );
    wb.navigate(Navigation::Forward(1));
    // After W D4 (White's best), Black to move; D4 is now occupied, so not analysed further.

    let v = wb.view(false);
    let stats = v.statistics;
    let first = stats.moves.iter().find(|e| e.move_number == 1).unwrap();
    assert_eq!(
        (first.mover, first.matched, first.share, first.match_ai, first.best),
        (Stone::Black, Some(2), 0.5, true, false)
    );
    // Black had 55 %; White now has 52 %, so Black kept 48 %: -7 points. Score: +2 → -0.5.
    assert!((first.winrate_change.unwrap() - -7.0).abs() < 1e-9);
    assert!((first.score_change.unwrap() - -2.5).abs() < 1e-9);
    let second = stats.moves.iter().find(|e| e.move_number == 2).unwrap();
    assert_eq!(
        (second.mover, second.matched, second.share, second.best),
        (Stone::White, Some(1), 1.0, true)
    );
    assert_eq!(second.winrate_change, None, "the position after it has no analysis");
    assert!(
        stats.moves.iter().all(|e| e.move_number != 3),
        "move 3's position has no analysis"
    );
    assert_eq!((stats.ai_score_black, stats.ai_score_white), (Some(50.0), Some(100.0)));

    // Back at the root only the move played from here counts.
    wb.navigate(Navigation::First);
    let stats = wb.view(false).statistics;
    assert_eq!((stats.ai_score_black, stats.ai_score_white), (Some(50.0), None));

    // Fewer than 20 % of the most visits is no AI match, whatever the rank.
    let strict = DisplaySettings {
        match_ai_percents_playouts: 60.0,
        ..wb.config().display.clone()
    };
    wb.set_display_settings(strict).unwrap();
    let first = wb
        .view(false)
        .statistics
        .moves
        .into_iter()
        .find(|e| e.move_number == 1)
        .unwrap();
    assert!(!first.match_ai);

    // 统计 上一手: against the parent's analysis, else two moves back.
    wb.navigate(Navigation::Forward(1));
    let last = wb.view(false).statistics.last_move.unwrap();
    assert!((last.winrate - -7.0).abs() < 1e-9 && (last.score.unwrap() - -2.5).abs() < 1e-9);
    wb.navigate(Navigation::Forward(1));
    assert_eq!(
        wb.view(false).statistics.last_move,
        None,
        "the current position is not analysed"
    );
    wb.navigate(Navigation::Forward(1));
    // After B Q4 (move 3), White to move: 45 % and -1 for White; move 2's position has none.
    analyse(
        &mut wb,
        &world,
        id,
        "info move C3 visits 40 winrate 0.45 scoreLead -1 prior 0.2 order 0 pv C3",
    );
    let last = wb.view(false).statistics.last_move.unwrap();
    assert!((last.winrate - 7.0).abs() < 1e-9 && (last.score.unwrap() - 1.5).abs() < 1e-9);
}

/// 终局点目 pauses analysis while scoring, counts areas with the user's dead-stone and 不计入
/// marks (a stone toggles its colour's stones reachable through empty points), follows the
/// rules choice and komi, writes the result into RE, and resumes analysis when it ends.
#[test]
fn scoring_counts_marked_positions_and_returns_analysis() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    // 5x5: Black owns columns a-b, White columns d-e, c is the border; a White stone sits in
    // Black's area at aa and a Black one in White's at ee.
    open(
        &mut wb,
        "(;SZ[5]KM[0.5]AB[ba][bb][bc][bd][be][ee]AW[da][db][dc][dd][de][aa])",
    );
    answer_all(&mut wb, &world, id);
    assert!(!wb.view(false).engine.paused);

    wb.score(ScoreCommand::Toggle).unwrap();
    let v = wb.view(false);
    assert!(v.engine.paused, "scoring pauses analysis");
    let s = v.score.expect("scoring");
    // Nothing marked: the c column touches both colours, as do a1..a5 and e1..e5 next to the
    // intruders, so only the regions bordered by one colour count.
    let at = |s: &lizzie_core::api::ScoreView, x: u16, y: u16| s.points[y as usize * 5 + x as usize];
    assert_eq!(at(&s, 2, 2).owner, None);
    assert_eq!((s.black_alive, s.white_alive), (6, 6));

    // White aa is dead, Black ee is dead.
    wb.score(ScoreCommand::Point {
        point: Point { x: 0, y: 0 },
    })
    .unwrap();
    wb.score(ScoreCommand::Point {
        point: Point { x: 4, y: 4 },
    })
    .unwrap();
    let s = wb.view(false).score.unwrap();
    assert!(at(&s, 0, 0).dead && at(&s, 4, 4).dead);
    assert_eq!(at(&s, 0, 2).owner, Some(Stone::Black));
    assert_eq!(at(&s, 4, 2).owner, Some(Stone::White));
    // Area: Black 5 stones + 5 points (a column incl. the dead White stone), White likewise
    // plus komi 0.5.
    assert_eq!(
        (s.black_alive, s.black_points, s.white_alive, s.white_points),
        (5, 5, 5, 5)
    );
    assert_eq!((s.black_captures, s.white_captures), (1, 1));
    assert_eq!((s.black_total, s.white_total, s.result.as_str()), (10.0, 10.5, "W+0.5"));

    // 不计入 on a1 (an empty Black point) leaves it out.
    wb.score(ScoreCommand::Point {
        point: Point { x: 0, y: 4 },
    })
    .unwrap();
    let s = wb.view(false).score.unwrap();
    assert!(at(&s, 0, 4).excluded);
    assert_eq!(s.black_points, 4);

    // Territory: points less the prisoners the other side took.
    let territory = DisplaySettings {
        territory_scoring: true,
        ..wb.config().display.clone()
    };
    wb.set_display_settings(territory).unwrap();
    let s = wb.view(false).score.unwrap();
    assert_eq!((s.black_total, s.white_total), (4.0 - 1.0, 5.0 - 1.0 + 0.5));

    wb.score(ScoreCommand::Confirm).unwrap();
    let v = wb.view(false);
    assert_eq!(wb.document().record.info.result, "W+1.5");
    assert!(v.document.unsaved, "the result is a content change");

    wb.score(ScoreCommand::Recalculate).unwrap();
    let s = wb.view(false).score.unwrap();
    assert!(!at(&s, 0, 0).dead, "recalculating starts from the stones again");

    wb.score(ScoreCommand::Toggle).unwrap();
    let v = wb.view(false);
    assert!(
        v.score.is_none() && !v.engine.paused,
        "leaving scoring resumes analysis"
    );
}

//! 边下边提醒's rating of one move from fixed reviews: the score leads, the HumanSL score spread and
//! the human policy as the companion reports them. The expected ratings follow Java
//! `MoveVerdict.of`, `MoveFeedback.judge` and `RankAwareWinrate.of`; the win chances are standard
//! normal quantiles (Python `statistics.NormalDist().inv_cdf`), not this implementation's output.
use lizzie_core::humansl::feedback::{
    counts, judge, key_mistakes, needs_position_after, win_chances, worst_mistakes, MoveFeedback, PositionReview,
    ReviewCandidate, Verdict, WORST_SHOWN,
};

/// z with Φ(z) = 0.451, 0.456, 0.45, 0.40, 0.30, 0.44 and 0.46.
const Z_451: f64 = -0.1231352477348365;
const Z_456: f64 = -0.11051620356204167;
const Z_45: f64 = -0.125661346855074;
const Z_40: f64 = -0.2533471031357998;
const Z_30: f64 = -0.5244005127080407;
const Z_44: f64 = -0.15096921549677725;
const Z_46: f64 = -0.10043372051146975;

fn candidate(vertex: &str, visits: u32, lead: f64) -> ReviewCandidate {
    ReviewCandidate { vertex: vertex.into(), visits, lead }
}

/// KataGo's choice Q16 with `best` points, the player's D4 searched as closely with `played`.
fn review(stdev: Option<f64>, best: f64, played: f64) -> PositionReview {
    PositionReview {
        root_lead: best,
        score_stdev: stdev,
        candidates: vec![candidate("Q16", 400, best), candidate("D4", 400, played)],
        human_shares: vec![("Q16".into(), 0.5), ("D4".into(), 0.5)],
    }
}

fn rate(stdev: Option<f64>, best: f64, played: f64) -> Verdict {
    judge(31, "D4", &review(stdev, best, played), None).unwrap().verdict
}

#[test]
fn the_win_chance_drop_rates_the_move_by_its_whole_percentage() {
    // The drop is rated as the panel shows it, rounded to a whole percent: 4.9% is "−5%".
    let cases = [
        (Z_456, Verdict::Good, "4.4%"),
        (Z_451, Verdict::Slow, "4.9%"),
        (Z_45, Verdict::Slow, "5%"),
        (Z_40, Verdict::Problem, "10%"),
        (Z_30, Verdict::Blunder, "20%"),
    ];
    for (z, verdict, drop) in cases {
        let f = judge(31, "d4", &review(Some(1.0), 0.0, z), None).unwrap();
        assert_eq!(f.verdict, verdict, "a drop of {drop}");
        assert!((f.win_before - 0.5).abs() < 1e-6, "{drop}: {}", f.win_before);
        assert!((f.points_lost + z).abs() < 1e-12);
        assert_eq!((f.move_number, f.vertex.as_str(), f.best_move.as_str(), f.played_best), (31, "d4", "Q16", false));
    }
}

#[test]
fn the_spread_is_twelve_points_without_one_and_at_least_one_point() {
    // Without a spread, 12 points: 12·z of Φ = 0.40 loses 10%; the same z points would not.
    assert_eq!(rate(None, 0.0, 12.0 * Z_40), Verdict::Problem);
    assert_eq!(rate(None, 0.0, Z_40), Verdict::Good);
    // A spread of zero is none.
    assert_eq!(rate(Some(0.0), 0.0, 12.0 * Z_40), Verdict::Problem);
    // A spread below a point counts as one: z points lose 10%, not the 30% half a point would give.
    assert_eq!(rate(Some(0.5), 0.0, Z_40), Verdict::Problem);
    assert_eq!(rate(Some(1.0), 0.0, Z_40), Verdict::Problem);
}

#[test]
fn a_decided_game_rates_by_points_from_ten_and_twenty() {
    // Behind by two spreads (2.3%) and ahead by four (99.997%) the game is decided; the win chance
    // hardly moves, and the points lost rate the move.
    for best in [-24.0, 48.0] {
        for (lost, verdict) in [(9.0, Verdict::Good), (10.0, Verdict::Problem), (19.0, Verdict::Problem), (20.0, Verdict::Blunder)] {
            let f = judge(40, "D4", &review(Some(12.0), best, best - lost), None).unwrap();
            assert!(f.win_before < 0.05 || f.win_before > 0.95);
            assert!((f.win_before - f.win_after) < 0.025, "{best} {lost}: {f:?}");
            assert_eq!(f.verdict, verdict, "ahead by {best}, {lost} points lost");
        }
    }
    // Undecided, 25 points over a wide spread lose 2.5%: still a good move.
    assert_eq!(rate(Some(400.0), 0.0, -25.0), Verdict::Good);
}

/// Q16 is KataGo's choice; `share` of players at the level play it, and their usual move D4 (60%)
/// loses down to the win chance Φ(`common`).
fn rare(share: f64, common: f64) -> PositionReview {
    PositionReview {
        root_lead: 0.0,
        score_stdev: Some(1.0),
        candidates: vec![candidate("Q16", 400, 0.0), candidate("D4", 400, common), candidate("C3", 400, 0.0)],
        human_shares: vec![("Q16".into(), share), ("D4".into(), 0.6), ("C3".into(), share)],
    }
}

#[test]
fn a_brilliant_move_needs_all_three_conditions() {
    let f = judge(12, "Q16", &rare(0.05, Z_44), None).unwrap();
    assert_eq!((f.verdict, f.played_best, f.human_share, f.best_share), (Verdict::Brilliant, true, Some(0.05), Some(0.05)));
    // Not KataGo's choice, though as good and as rare.
    assert_eq!(judge(12, "C3", &rare(0.05, Z_44), None).unwrap().verdict, Verdict::Good);
    // Played by a tenth of the players.
    assert_eq!(judge(12, "Q16", &rare(0.10, Z_44), None).unwrap().verdict, Verdict::Good);
    // The usual move loses only 4%.
    assert_eq!(judge(12, "Q16", &rare(0.05, Z_46), None).unwrap().verdict, Verdict::Good);
    // The usual move's loss is unknown: KataGo hardly looked at it.
    let mut unseen = rare(0.05, Z_44);
    unseen.candidates[1].visits = 15;
    assert_eq!(judge(12, "Q16", &unseen, None).unwrap().verdict, Verdict::Good);
    // Without a policy nobody knows how rare it is.
    let mut blind = rare(0.05, Z_44);
    blind.human_shares.clear();
    let f = judge(12, "Q16", &blind, None).unwrap();
    assert_eq!((f.verdict, f.human_share), (Verdict::Good, None));
}

#[test]
fn a_move_the_search_hardly_saw_is_judged_from_the_position_after_it() {
    // D4 had 19 visits against the best move's 400: fewer than 20 are too few.
    let mut before = review(None, 0.0, 0.0);
    before.candidates[1].visits = 19;
    assert!(needs_position_after(&before, "D4"));
    assert!(judge(31, "D4", &before, None).is_none(), "nothing to judge it by");
    // The opponent leads by −z of Φ = 0.40 there; the spread of the position after counts when
    // the first review has none.
    let after = PositionReview { root_lead: -Z_40, score_stdev: Some(1.0), ..PositionReview::default() };
    assert_eq!(judge(31, "D4", &before, Some(&after)).unwrap().verdict, Verdict::Problem);
    before.candidates[1].visits = 20;
    assert!(!needs_position_after(&before, "D4"));
    // A pass KataGo did not search, and an empty review.
    assert!(needs_position_after(&before, "pass"));
    let f = judge(32, "PASS", &before, Some(&after)).unwrap();
    assert_eq!(f.vertex, "pass");
    assert!(judge(31, "D4", &PositionReview::default(), Some(&after)).is_none());
}

/// Move `n` rated `verdict`, the win chance going from `before` to `after`.
fn rated(n: u32, verdict: Verdict, before: f64, after: f64) -> MoveFeedback {
    MoveFeedback {
        move_number: n,
        vertex: "D4".into(),
        verdict,
        win_before: before,
        win_after: after,
        points_lost: 0.0,
        human_share: None,
        played_best: false,
        best_move: "Q16".into(),
        best_share: None,
    }
}

fn numbers(moves: &[MoveFeedback]) -> Vec<u32> {
    moves.iter().map(|m| m.move_number).collect()
}

#[test]
fn of_five_mistakes_the_three_worst_are_listed_in_the_order_played() {
    let game = [
        rated(9, Verdict::Good, 0.50, 0.50),
        rated(11, Verdict::Blunder, 0.60, 0.35),
        rated(15, Verdict::Problem, 0.50, 0.38),
        rated(17, Verdict::Brilliant, 0.40, 0.45),
        rated(19, Verdict::Blunder, 0.70, 0.30),
        rated(23, Verdict::Slow, 0.40, 0.34),
        rated(27, Verdict::Problem, 0.50, 0.32),
    ];
    // Both 恶手 first, the larger drop (#19, 40%) before #11 (25%); then the 问题手 that lost more,
    // #27 (18%) before #15 (12%); the 缓手 does not make it.
    assert_eq!(WORST_SHOWN, 3);
    assert_eq!(numbers(&worst_mistakes(&game, WORST_SHOWN)), vec![11, 19, 27]);
    assert_eq!(numbers(&worst_mistakes(&game, 1)), vec![19]);
    assert_eq!(key_mistakes(&game), 2, "the 恶手 only");
}

#[test]
fn fewer_mistakes_list_what_there_is_and_none_list_nothing() {
    // Java MoveReviewerTest: a 恶手 at 11, a good move at 13 and a 问题手 at 15.
    let game = [
        rated(11, Verdict::Blunder, 0.55, 0.20),
        rated(13, Verdict::Good, 0.20, 0.21),
        rated(15, Verdict::Problem, 0.21, 0.10),
    ];
    assert_eq!(numbers(&worst_mistakes(&game, 3)), vec![11, 15]);
    assert_eq!(numbers(&worst_mistakes(&game, 1)), vec![11]);
    assert_eq!(key_mistakes(&game), 1, "the 恶手 only, not the 问题手");
    let clean = [rated(1, Verdict::Good, 0.5, 0.5), rated(3, Verdict::Brilliant, 0.5, 0.6)];
    assert!(worst_mistakes(&clean, 3).is_empty());
    assert_eq!(key_mistakes(&clean), 0);
    assert!(worst_mistakes(&[], 3).is_empty());
}

#[test]
fn equal_mistakes_keep_the_order_played() {
    let game = [
        rated(5, Verdict::Slow, 0.50, 0.44),
        rated(7, Verdict::Slow, 0.50, 0.44),
        rated(9, Verdict::Slow, 0.50, 0.44),
        rated(11, Verdict::Slow, 0.50, 0.44),
    ];
    assert_eq!(numbers(&worst_mistakes(&game, 3)), vec![5, 7, 9]);
}

#[test]
fn the_ratings_are_counted_best_first() {
    let game = [
        rated(1, Verdict::Good, 0.5, 0.5),
        rated(3, Verdict::Blunder, 0.5, 0.2),
        rated(5, Verdict::Good, 0.2, 0.2),
        rated(7, Verdict::Problem, 0.2, 0.1),
        rated(9, Verdict::Brilliant, 0.1, 0.2),
        rated(11, Verdict::Blunder, 0.6, 0.3),
    ];
    // 妙手, 好棋, 缓手, 问题手, 恶手.
    assert_eq!(counts(&game), [1, 2, 0, 1, 2]);
    assert_eq!(counts(&[]), [0; 5]);
}

#[test]
fn the_win_chances_run_through_each_rated_move_then_the_position_to_move_in() {
    let game = [rated(1, Verdict::Good, 0.52, 0.50), rated(3, Verdict::Blunder, 0.55, 0.30)];
    assert!(win_chances(&[], None).is_empty());
    let close = |got: Vec<f64>, want: &[f64]| {
        assert_eq!(got.len(), want.len(), "{got:?}");
        assert!(got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-4), "{got:?} ≠ {want:?}");
    };
    close(win_chances(&game, None), &[52.0, 50.0, 55.0, 30.0]);
    // The position to move in: KataGo's choice leads by 12·z of Φ = 0.40 with a 12-point spread.
    let turn = PositionReview { root_lead: 0.0, score_stdev: Some(12.0), candidates: vec![candidate("Q16", 400, 12.0 * Z_40)], ..PositionReview::default() };
    close(win_chances(&game, Some(&turn)), &[52.0, 50.0, 55.0, 30.0, 40.0]);
    // A review without a choice adds nothing.
    close(win_chances(&game, Some(&PositionReview::default())), &[52.0, 50.0, 55.0, 30.0]);
}
